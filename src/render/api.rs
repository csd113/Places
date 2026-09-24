//! The level-geometry entry points.
//!
//! These are the functions the game and the audits call: each one builds the
//! static mesh (and, where asked, the prop batches) from a level, a catalog and
//! a material table, with the lighting baked exactly once per level load.

use std::sync::Arc;

use super::geometry::build_level_geometry_mesh_with_lightmaps;
use super::props::{PropMeshBatch, resolve_prop_instances};
use super::{
    LevelDef, LevelLighting, LevelMesh, LevelSurfaces, MaterialTable, PropDef,
    build_level_geometry_mesh,
};
use crate::lighting::lightmap::{
    LevelLightmaps, LightmapAtlas, LightmapCache, LightmapConfig, LightmapFailure, LightmapMode,
    LightmapPlan, LightmapStats, content_key_with_extra, fill_chart, write_page_png,
};
use crate::lighting::BakeConfig;

/// Builds the level mesh with real prop geometry where possible, plus one
/// batched draw per distinct prop model.
///
/// Props whose model is missing, malformed or simply absent from the catalogue
/// still emit their catalogue-sized placeholder box into
/// `LevelMesh::batches.prop_batch`, so a broken asset degrades visibly instead
/// of vanishing, and never crashes or loops (failures are cached by
/// [`crate::props::PropAssets`]).
pub fn build_level_geometry_with_assets(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>) {
    let materials = logical_materials(level);
    let (mesh, batches, _lighting) = build_level_geometry_with_assets_and_lighting_and_materials(
        level, catalog, assets, &materials,
    );
    (mesh, batches)
}

/// [`build_level_geometry_with_assets`], also returning the baked lighting that
/// was folded into the vertex colours.
///
/// The lighting is baked exactly once here, at level load, and passed to both
/// the world geometry and the prop instancing so the whole level shares one
/// consistent set of room baselines, fixture pools and opening blends.
pub fn build_level_geometry_with_assets_and_lighting(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let materials = logical_materials(level);
    build_level_geometry_with_assets_and_lighting_and_materials(level, catalog, assets, &materials)
}

/// [`build_level_geometry_with_assets_and_lighting`] with an explicitly
/// resolved material table.
///
/// The renderer uses this with the level's loaded table (including pack
/// materials and decoded images); tests and the lighting audit use the
/// catalog-only wrapper above.
pub fn build_level_geometry_with_assets_and_lighting_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let (mesh, batches, lighting, _) =
        build_level_geometry_timed(level, catalog, assets, materials);
    (mesh, batches, lighting)
}

/// Stage-by-stage timings for one level build, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BuildTimings {
    pub lighting_millis: f64,
    pub props_millis: f64,
    pub surfaces_millis: f64,
}

/// What a level build should do about lightmaps, and against which budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapBuildOptions {
    /// Bake and stamp lightmaps, or take the historical vertex-lit path.
    pub mode: LightmapMode,
    /// Density and page budget the lightmapped path packs against.
    pub config: LightmapConfig,
    /// Profile the config came from; only used for the content key.
    pub profile: crate::quality::QualityProfile,
}

impl LightmapBuildOptions {
    /// The options one quality profile implies for a mode.
    #[must_use]
    pub const fn for_profile(profile: crate::quality::QualityProfile, mode: LightmapMode) -> Self {
        Self {
            mode,
            config: LightmapConfig::for_profile(profile),
            profile,
        }
    }
}

/// Everything one level build produced.
///
/// `lightmaps` is `Some` only when the level was built *and* baked with
/// [`LightmapMode::On`]; `lightmap_failure` is set when an `On` build had to
/// fall back, which is the named reason the caller can log. A build that was
/// asked for `Off` has both `None` and is the historical vertex-lit level,
/// byte for byte.
pub struct LevelBuild {
    pub mesh: LevelMesh,
    pub batches: Vec<PropMeshBatch>,
    pub lighting: LevelLighting,
    pub timings: BuildTimings,
    pub lightmaps: Option<Arc<LevelLightmaps>>,
    /// Why an `On` build fell back to vertex colours, if it did.
    pub lightmap_failure: Option<LightmapFailure>,
    /// Wall-clock cost of filling and packing the atlas, in milliseconds.
    pub lightmap_millis: f64,
}

/// [`build_level_geometry_with_assets_and_lighting`], also reporting how the
/// build time splits between the lighting bake, prop instancing and static
/// surface emission.
///
/// Kept separate from the untimed entry point so the timing does not change what
/// the normal load path does.
pub fn build_level_geometry_timed(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting, BuildTimings) {
    let started = std::time::Instant::now();
    let lighting = LevelLighting::bake(level);
    let lighting_millis = started.elapsed().as_secs_f64() * 1000.0;

    let surfaces = LevelSurfaces::new(level);
    let started = std::time::Instant::now();
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets, &lighting, &surfaces);
    let props_millis = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let mesh = build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials);
    let surfaces_millis = started.elapsed().as_secs_f64() * 1000.0;

    (
        mesh,
        batches,
        lighting,
        BuildTimings {
            lighting_millis,
            props_millis,
            surfaces_millis,
        },
    )
}

/// [`build_level_geometry_timed`] with the lightmap path.
///
/// This is the renderer's entry point. With [`LightmapMode::On`] the mesh is
/// emitted with a [`LightmapPlan`], the plan's charts are filled from the same
/// [`LevelLighting`] the historical build bakes into vertex colours, and the
/// result is returned alongside the mesh. With [`LightmapMode::Off`] the build
/// is the historical one and no plan is created at all.
///
/// A failed plan (page overflow, a degenerate quad) or a failed fill is rebuilt
/// once with the historical mesh path and the same baked lighting, so the
/// returned level is always drawable: `lightmaps` is then `None` and
/// `lightmap_failure` names the reason. There is deliberately no third state —
/// never a half-baked atlas, never black surfaces.
///
/// `cache` is best-effort: a hit skips the fill pass entirely, and `None`
/// always bakes fresh (which is what the tests use to prove determinism).
pub fn build_level_geometry_timed_with_lightmaps(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
    options: LightmapBuildOptions,
    mut cache: Option<&mut LightmapCache>,
) -> LevelBuild {
    let started = std::time::Instant::now();
    // The vertex-lit mode is the *historical* path and must stay byte-identical
    // to it, so it bakes with [`BakeConfig::HARD`] whatever profile is active: a
    // soft-shadow, fine-occluder bake would change vertex colours that the
    // fallback contract says are the historical ones.
    let bake = match options.mode {
        LightmapMode::On => options.profile.bake_config(),
        LightmapMode::Off => BakeConfig::HARD,
    };
    let lighting = LevelLighting::bake_with(level, bake);
    let lighting_millis = elapsed_millis(started);

    let surfaces = LevelSurfaces::new(level);
    let started = std::time::Instant::now();
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets, &lighting, &surfaces);
    let props_millis = elapsed_millis(started);

    let mut plan = (options.mode == LightmapMode::On).then(|| LightmapPlan::new(options.config));
    let started = std::time::Instant::now();
    let mesh = plan.as_mut().map_or_else(
        || build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials),
        |plan| {
            build_level_geometry_mesh_with_lightmaps(
                level,
                catalog,
                &fallbacks,
                &lighting,
                materials,
                Some(plan),
            )
        },
    );
    let mut surfaces_millis = elapsed_millis(started);

    let mut lightmaps: Option<Arc<LevelLightmaps>> = None;
    let mut lightmap_failure: Option<LightmapFailure> = None;
    let mut lightmap_millis = 0.0;
    if let Some(plan) = plan.as_ref() {
        // Report invisible slivers the plan skipped: the level kept its whole
        // lightmap, but the author should still know the geometry is there.
        let slivers = plan.slivers_skipped();
        if slivers > 0 {
            crate::logging::warn_once(
                format!("lightmap-slivers:{}", level.id),
                format!(
                    "[lightmaps] '{}' left {} sub-texel sliver quad(s) vertex-lit",
                    level.id, slivers
                ),
            );
        }
        if let Some(plan_failure) = plan.failure() {
            lightmap_failure = Some(plan_failure);
        } else {
            // The key covers the level definition, the lightmap config, the
            // quality profile, the *bake settings* (visibility taps and the
            // prop-occlusion cell) and the occluder set the bake actually uses,
            // so a prop model, a light or a shadow-quality constant change
            // invalidates the cached atlas while a texture-only edit does not.
            // See [`LevelLighting::occlusion_fingerprint`].
            let mut extra: Vec<u8> = Vec::with_capacity(13);
            extra.extend_from_slice(&lighting.occlusion_fingerprint().to_le_bytes());
            extra.push(bake.sampling.taps_per_axis);
            extra.extend_from_slice(&bake.prop_occlusion_cell_m.to_bits().to_le_bytes());
            let key = content_key_with_extra(level, &options.config, options.profile, &extra);
            if let Some(cached) = cache.as_deref_mut().and_then(|cache| cache.get(&key)) {
                lightmaps = Some(cached);
            }
            if lightmaps.is_none() {
                match bake_lightmaps(&lighting, plan, &options, &key) {
                    Ok(baked) => {
                        lightmap_millis = baked.stats.bake_millis;
                        dump_lightmaps_if_requested(level, &baked);
                        if let Some(cache) = cache {
                            cache.insert(&key, Arc::clone(&baked));
                        }
                        lightmaps = Some(baked);
                    }
                    Err(fill_failure) => lightmap_failure = Some(fill_failure),
                }
            }
        }
    }

    let mut timings = BuildTimings {
        lighting_millis,
        props_millis,
        surfaces_millis,
    };

    // A failed lightmap build must never be drawn: rebuild the static mesh with
    // the historical vertex path against the lighting already baked above. Only
    // the surface emission repeats, and the fallback is exactly the mesh an
    // `Off` build produces.
    if lightmap_failure.is_some() && options.mode == LightmapMode::On {
        let started = std::time::Instant::now();
        let mesh = build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials);
        surfaces_millis += elapsed_millis(started);
        timings.surfaces_millis = surfaces_millis;
        return LevelBuild {
            mesh,
            batches,
            lighting,
            timings,
            lightmaps: None,
            lightmap_failure,
            lightmap_millis,
        };
    }

    LevelBuild {
        mesh,
        batches,
        lighting,
        timings,
        lightmaps,
        lightmap_failure,
        lightmap_millis,
    }
}

/// Fills every chart of a finished plan into atlas pages.
fn bake_lightmaps(
    lighting: &LevelLighting,
    plan: &LightmapPlan,
    options: &LightmapBuildOptions,
    key: &str,
) -> Result<Arc<LevelLightmaps>, LightmapFailure> {
    let started = std::time::Instant::now();
    let atlas = LightmapAtlas::bake(
        &options.config,
        plan.page_count(),
        plan.charts(),
        |patch, chart| fill_chart(lighting, patch, chart),
    )?;
    let mut texels = 0usize;
    for (_, chart) in plan.charts() {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        texels = texels.saturating_add(width.saturating_mul(height));
    }
    let edge = usize::try_from(options.config.page_edge).unwrap_or(0);
    let stats = LightmapStats {
        charts: plan.chart_count(),
        pages: atlas.page_count(),
        texels,
        page_texels: atlas.page_count().saturating_mul(edge).saturating_mul(edge),
        bake_millis: elapsed_millis(started),
        cache_hit: false,
    };
    Ok(Arc::new(LevelLightmaps {
        pages: atlas.into_pages(),
        charts: plan.charts().to_vec(),
        stats,
        cache_key: key.to_string(),
    }))
}

/// Milliseconds since `started`.
fn elapsed_millis(started: std::time::Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

/// Writes every atlas page of a fresh bake under `target/agent-work/atlases/`
/// when `LIMINAL_DUMP_LIGHTMAPS=1` is set in the environment.
///
/// A developer dump, not a shipping path; the write failure is a one-line
/// diagnostic because there is no logger here to route it through.
#[allow(clippy::print_stderr)]
fn dump_lightmaps_if_requested(level: &LevelDef, lightmaps: &LevelLightmaps) {
    if std::env::var("LIMINAL_DUMP_LIGHTMAPS").as_deref() != Ok("1") {
        return;
    }
    let id: String = level
        .id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let id = if id.is_empty() {
        "level".to_string()
    } else {
        id
    };
    let dir = crate::assets::state_path("target/agent-work/atlases");
    for (index, page) in lightmaps.pages.iter().enumerate() {
        let path = dir.join(format!("{id}_page{index}.png"));
        if let Err(error) = write_page_png(page, &path) {
            crate::logging::warn_once(
                format!("lightmap-page:{}", path.display()),
                format!("[lightmaps] cannot write {}: {error}", path.display()),
            );
        }
    }
}

/// The shipped catalog, loaded once per process for geometry-only callers.
pub(super) fn shipped_asset_catalog() -> &'static crate::assets::AssetCatalog {
    static CATALOG: std::sync::OnceLock<crate::assets::AssetCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(crate::assets::AssetCatalog::load_default)
}

/// The logical material table for a level, resolved through the shipped
/// catalog with no image decoding.
///
/// Geometry only needs each material's index, tiling and tint, so the tests and
/// the lighting audit can build meshes without touching the filesystem.
#[must_use]
pub fn logical_materials(level: &LevelDef) -> MaterialTable {
    MaterialTable::logical(level, shipped_asset_catalog(), None)
}

/// Builds level geometry using only built-in prop fallbacks.
///
/// Callers that can resolve the prop catalog should prefer
/// [`build_level_geometry_with_catalog`].
#[must_use]
pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    build_level_geometry_with_catalog(level, &crate::loader::PropCatalog::builtin())
}

/// Builds level geometry using the shipped catalog's logical materials.
#[must_use]
pub fn build_level_geometry_with_materials(
    level: &LevelDef,
    materials: &MaterialTable,
) -> LevelMesh {
    build_level_geometry_with_catalog_and_materials(
        level,
        &crate::loader::PropCatalog::builtin(),
        materials,
    )
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
#[must_use]
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
) -> LevelMesh {
    let materials = logical_materials(level);
    build_level_geometry_with_catalog_and_materials(level, catalog, &materials)
}

/// [`build_level_geometry_with_catalog`] with an explicitly resolved material
/// table.
#[must_use]
pub fn build_level_geometry_with_catalog_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    materials: &MaterialTable,
) -> LevelMesh {
    let lighting = LevelLighting::bake(level);
    let fallbacks: Vec<&PropDef> = level.props.iter().collect();
    build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials)
}

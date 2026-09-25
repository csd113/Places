//! The level-geometry entry points.
//!
//! These are the functions the game and the audits call: each one builds the
//! static mesh (and, where asked, the prop batches) from a level, a catalog and
//! a material table, with the lighting baked exactly once per level load.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::geometry::build_level_geometry_mesh_with_lightmaps;
use super::props::{PropMeshBatch, resolve_prop_instances};
use super::{
    LevelDef, LevelLighting, LevelMesh, LevelSurfaces, MaterialTable, PropDef,
    build_level_geometry_mesh,
};
use crate::lighting::BakeConfig;
use crate::lighting::lightmap::{
    Chart, LevelLightmaps, LightmapAtlas, LightmapCache, LightmapConfig, LightmapFailure,
    LightmapMode, LightmapPatch, LightmapPlan, LightmapStats, content_key_with_extra, fill_chart,
    write_page_png,
};

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
    /// The shadow bake to run: taps and prop-occlusion cell.
    pub bake: BakeConfig,
}

impl LightmapBuildOptions {
    /// The options one quality level implies for a mode.
    ///
    /// The lightmap configuration and the bake come from the level; the profile
    /// is only the protected content-key boundary (`Medium` and `High` both map
    /// to `Full`, and the key itself keeps their configs and bake settings
    /// apart).
    #[must_use]
    pub const fn for_level(level: crate::quality::QualityLevel, mode: LightmapMode) -> Self {
        Self {
            mode,
            config: level.lightmap_config(),
            profile: level.profile(),
            bake: level.bake_config(),
        }
    }

    /// The options one validated profile implies for a mode.
    ///
    /// Kept for the callers that deliberately work at the profile boundary
    /// (the lightmap audits and tests); the renderer always uses
    /// [`Self::for_lightmaps`].
    #[must_use]
    pub const fn for_profile(profile: crate::quality::QualityProfile, mode: LightmapMode) -> Self {
        Self {
            mode,
            config: LightmapConfig::for_profile(profile),
            profile,
            bake: profile.bake_config(),
        }
    }

    /// The options the Lightmaps quality implies.
    ///
    /// This is the renderer's entry point now that Lightmaps is its own
    /// setting: [`crate::quality::LightmapQuality::Off`] builds the historical
    /// vertex-lit level, and Medium/Full bake against their own density, page
    /// budget and shadow bake. The overall [`crate::quality::QualityLevel`] no
    /// longer decides any of it, so `Low + Lightmaps Full` bakes a Full atlas
    /// and `High + Lightmaps Off` stays vertex-lit.
    #[must_use]
    pub const fn for_lightmaps(lightmaps: crate::quality::LightmapQuality) -> Self {
        Self::for_lightmap_quality(
            lightmaps,
            match lightmaps.lightmap_config() {
                Some(_) => LightmapMode::On,
                None => LightmapMode::Off,
            },
        )
    }

    /// [`Self::for_lightmaps`] with an explicit mode, for the callers that
    /// deliberately need the historical vertex-lit build whatever the setting
    /// says (the renderer's defensive atlas-upload fallback).
    #[must_use]
    pub const fn for_lightmap_quality(
        lightmaps: crate::quality::LightmapQuality,
        mode: LightmapMode,
    ) -> Self {
        let config = match lightmaps.lightmap_config() {
            Some(config) => config,
            None => match crate::quality::LightmapQuality::Full.lightmap_config() {
                Some(config) => config,
                None => LightmapConfig::for_profile(crate::quality::QualityProfile::Full),
            },
        };
        let bake = match lightmaps.bake_config() {
            Some(bake) => bake,
            None => BakeConfig::HARD,
        };
        Self {
            mode,
            config,
            profile: lightmaps.profile(),
            bake,
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

/// A finished CPU level build that may still owe its lightmap fill.
///
/// This is the renderer's split point for a non-blocking lightmap change: the
/// lighting bake, prop instancing and the plan-stamped mesh run on the main
/// thread, and only the per-texel fill can be deferred. `fill` is `None`
/// exactly when the build is complete — a cache hit, [`LightmapMode::Off`], or
/// a plan failure that already fell back to the historical vertex-lit mesh.
pub struct PreparedLightmapBuild {
    /// The mesh (stamped with the lightmap plan when one was asked for), the
    /// prop batches, the baked lighting and the stage timings.
    pub build: LevelBuild,
    /// The fill a cache miss still owes, or `None` when the build is complete.
    pub fill: Option<LightmapFillRequest>,
}

/// A pure, `Send`-safe description of one lightmap fill.
///
/// It carries everything [`fill_chart`] reads and nothing else: the baked
/// lighting (shared, immutable), the atlas configuration, the plan's charts
/// and page count, and the deterministic content key the result must be cached
/// under. The renderer hands one of these to a worker thread and keeps drawing
/// the previous atlas until the result arrives, so an uncached Full bake no
/// longer freezes a frame.
#[derive(Clone, Debug)]
pub struct LightmapFillRequest {
    /// The baked lighting every texel samples.
    pub lighting: Arc<LevelLighting>,
    /// Density, page budget and padding the plan packed against.
    pub config: LightmapConfig,
    /// Every chart of the finished plan, paired with its patch, in plan order.
    pub charts: Vec<(LightmapPatch, Chart)>,
    /// Pages the plan's packer opened.
    pub page_count: usize,
    /// Deterministic content key of the inputs this fill describes.
    pub content_key: String,
}

/// What one lightmap fill produced.
#[derive(Debug)]
pub enum LightmapFillOutcome {
    /// The atlas was filled; its pages are byte-identical to an inline bake of
    /// the same request.
    Filled(LevelLightmaps),
    /// The fill failed (`FillSize`, `FillNonFinite`, `Layout`, ...); the caller
    /// must keep the historical vertex-lit fallback.
    Failed(LightmapFailure),
    /// A newer request superseded this one; the result must never activate.
    Cancelled,
}

/// One live lightmap fill on a background thread.
///
/// The renderer owns at most one: starting a new fill supersedes the previous
/// worker, and dropping this value cancels and joins it, so no fill can outlive
/// the renderer and shutdown is clean. The thread is a plain `std::thread`
/// (there is no async runtime), and its only work is the per-chart fill; the
/// cancellation check runs between charts, which bounds a supersede's join to
/// one chart's fill.
pub struct LightmapFillWorker {
    cancel: Arc<AtomicBool>,
    receiver: std::sync::mpsc::Receiver<LightmapFillOutcome>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl LightmapFillWorker {
    /// Starts one worker filling `request`.
    ///
    /// Returns `None` when the operating system refuses a thread; the caller
    /// then fills inline, which costs one blocking frame but still applies the
    /// setting instead of leaving it half applied.
    #[must_use]
    pub fn spawn(request: LightmapFillRequest) -> Option<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("places-lightmap-fill".to_string())
            .spawn(move || {
                let outcome = fill_lightmaps_cancellable(&request, &flag);
                // A closed receiver means the renderer already superseded this
                // request; there is nothing to report it to.
                let _ = sender.send(outcome);
            })
            .ok()?;
        Some(Self {
            cancel,
            receiver,
            handle: Some(handle),
        })
    }

    /// Non-blockingly takes the finished outcome, if the fill has completed.
    ///
    /// A disconnected channel (the worker dropped its sender without sending,
    /// which cannot happen in the current body) reads as
    /// [`LightmapFillOutcome::Cancelled`], so a broken worker can never
    /// activate a result.
    #[must_use]
    pub fn try_take(&mut self) -> Option<LightmapFillOutcome> {
        match self.receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(LightmapFillOutcome::Cancelled)
            }
        }
    }

    /// Asks the worker to stop at the next chart boundary.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for LightmapFillWorker {
    /// Cancels the fill and joins the thread: an atlas fill can never outlive
    /// the renderer that asked for it.
    fn drop(&mut self) {
        self.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Fills one request to completion on the calling thread.
///
/// This is the inline path (and the tests' reference): the same body the worker
/// runs, with a cancellation flag that is never set, which is what keeps the
/// synchronous and asynchronous pages byte-identical.
///
/// # Errors
///
/// Returns the named [`LightmapFailure`] when a chart's texels are not exactly
/// what the plan described; the caller must rebuild the vertex-lit level.
pub fn fill_lightmaps(request: &LightmapFillRequest) -> Result<LevelLightmaps, LightmapFailure> {
    bake_request(request, None)
}

/// The worker's body: [`fill_lightmaps`] with a cancellation check between
/// charts, reported as [`LightmapFillOutcome::Cancelled`] when the flag was set.
fn fill_lightmaps_cancellable(
    request: &LightmapFillRequest,
    cancel: &AtomicBool,
) -> LightmapFillOutcome {
    match bake_request(request, Some(cancel)) {
        Ok(lightmaps) if !cancel.load(Ordering::Relaxed) => LightmapFillOutcome::Filled(lightmaps),
        Ok(_) => LightmapFillOutcome::Cancelled,
        Err(_) if cancel.load(Ordering::Relaxed) => LightmapFillOutcome::Cancelled,
        Err(failure) => LightmapFillOutcome::Failed(failure),
    }
}

/// The shared fill body, optionally cancellation-aware.
///
/// A cancelled chart returns an empty texel run, which the atlas builder
/// reports as [`LightmapFailure::FillSize`]; the worker checks the flag
/// afterwards and reports cancellation instead, so the failure kind is never
/// mistaken for a real bake failure.
fn bake_request(
    request: &LightmapFillRequest,
    cancel: Option<&AtomicBool>,
) -> Result<LevelLightmaps, LightmapFailure> {
    let started = std::time::Instant::now();
    let atlas = LightmapAtlas::bake(
        &request.config,
        request.page_count,
        &request.charts,
        |patch, chart| {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Vec::new();
            }
            fill_chart(&request.lighting, patch, chart)
        },
    )?;
    let mut texels = 0usize;
    for (_, chart) in &request.charts {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        texels = texels.saturating_add(width.saturating_mul(height));
    }
    let edge = usize::try_from(request.config.page_edge).unwrap_or(0);
    let stats = LightmapStats {
        charts: request.charts.len(),
        pages: atlas.page_count(),
        texels,
        page_texels: atlas.page_count().saturating_mul(edge).saturating_mul(edge),
        bake_millis: elapsed_millis(started),
        cache_hit: false,
    };
    Ok(LevelLightmaps {
        pages: atlas.into_pages(),
        charts: request.charts.clone(),
        stats,
        cache_key: request.content_key.clone(),
    })
}

/// [`build_level_geometry_timed_with_lightmaps`] up to, but not including, the
/// lightmap fill.
///
/// This is the renderer's entry point for a runtime graphics change: it bakes
/// the lighting, instances the props and emits the plan-stamped mesh in one
/// pass, then reports whether an atlas fill is still owed. A cache hit is
/// already activated in the returned build, so the caller never starts a
/// worker for an atlas it already has.
#[must_use]
pub fn prepare_level_geometry_with_lightmaps(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
    options: LightmapBuildOptions,
    cache: Option<&mut LightmapCache>,
) -> PreparedLightmapBuild {
    let started = std::time::Instant::now();
    // The vertex-lit mode is the *historical* path and must stay byte-identical
    // to it, so it bakes with [`BakeConfig::HARD`] whatever level is active: a
    // soft-shadow, fine-occluder bake would change vertex colours that the
    // fallback contract says are the historical ones.
    let bake = match options.mode {
        LightmapMode::On => options.bake,
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
    let mesh = build_mesh_for_options(
        level,
        catalog,
        &fallbacks,
        &lighting,
        materials,
        plan.as_mut(),
    );
    let surfaces_millis = elapsed_millis(started);

    let mut build = LevelBuild {
        mesh,
        batches,
        lighting,
        timings: BuildTimings {
            lighting_millis,
            props_millis,
            surfaces_millis,
        },
        lightmaps: None,
        lightmap_failure: None,
        lightmap_millis: 0.0,
    };
    let mut fill: Option<LightmapFillRequest> = None;
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
            build.lightmap_failure = Some(plan_failure);
        } else {
            // The key covers the level definition, the lightmap config, the
            // quality level, the *bake settings* (visibility taps and the
            // prop-occlusion cell) and the occluder set the bake actually uses,
            // so a prop model, a light or a shadow-quality constant change
            // invalidates the cached atlas while a texture-only edit does not.
            // See [`LevelLighting::occlusion_fingerprint`].
            let mut extra: Vec<u8> = Vec::with_capacity(13);
            extra.extend_from_slice(&build.lighting.occlusion_fingerprint().to_le_bytes());
            extra.push(bake.sampling.taps_per_axis);
            extra.extend_from_slice(&bake.prop_occlusion_cell_m.to_bits().to_le_bytes());
            let key = content_key_with_extra(level, &options.config, options.profile, &extra);
            if let Some(cached) = cache.and_then(|cache| cache.get(&key)) {
                build.lightmaps = Some(cached);
            } else {
                fill = Some(LightmapFillRequest {
                    lighting: Arc::new(build.lighting.clone()),
                    config: options.config,
                    charts: plan.charts().to_vec(),
                    page_count: plan.page_count(),
                    content_key: key,
                });
            }
        }
    }

    // A failed plan must never be drawn: rebuild the static mesh with the
    // historical vertex path against the lighting already baked above. This is
    // the exact fallback the inline entry point always ran, and it does not
    // re-bake the lighting (which would change every vertex colour).
    if build.lightmap_failure.is_some() && options.mode == LightmapMode::On {
        let started = std::time::Instant::now();
        let mesh =
            build_level_geometry_mesh(level, catalog, &fallbacks, &build.lighting, materials);
        build.timings.surfaces_millis += elapsed_millis(started);
        build.mesh = mesh;
    }

    PreparedLightmapBuild { build, fill }
}

/// Rebuilds the historical vertex-lit mesh against an already-baked lighting
/// field.
///
/// The fallback both the inline and the asynchronous fills use when a plan or
/// fill fails: the same lighting keeps every baked vertex colour, and the
/// result is exactly the mesh a [`LightmapMode::Off`] build produces. Prop
/// instancing is re-resolved here, so the original build's batches are not
/// needed; that repeats a little work, but only on the failure path.
#[must_use]
pub fn rebuild_vertex_lit_level(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
    lighting: &LevelLighting,
) -> LevelMesh {
    let surfaces = LevelSurfaces::new(level);
    let (_, fallbacks) = resolve_prop_instances(level, catalog, assets, lighting, &surfaces);
    build_level_geometry_mesh(level, catalog, &fallbacks, lighting, materials)
}

/// [`prepare_level_geometry_with_lightmaps`] finished inline.
///
/// This is the historical synchronous entry point: it prepares the build and
/// fills any owed lightmaps on the calling thread, so a plan or fill failure
/// degrades to the exact vertex-lit level. The renderer's runtime changes use
/// the two halves separately and fill on a worker; both share
/// [`fill_lightmaps`], so their pages cannot drift.
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
    let PreparedLightmapBuild { mut build, fill } = prepare_level_geometry_with_lightmaps(
        level,
        catalog,
        assets,
        materials,
        options,
        cache.as_deref_mut(),
    );
    let Some(request) = fill else {
        return build;
    };
    match fill_lightmaps(&request) {
        Ok(lightmaps) => {
            let lightmaps = Arc::new(lightmaps);
            build.lightmap_millis = lightmaps.stats.bake_millis;
            dump_lightmaps_for_level(level, &lightmaps);
            if let Some(cache) = cache {
                cache.insert(&request.content_key, Arc::clone(&lightmaps));
            }
            build.lightmaps = Some(lightmaps);
        }
        Err(fill_failure) => {
            build.lightmap_failure = Some(fill_failure);
            let started = std::time::Instant::now();
            let mesh = rebuild_vertex_lit_level(level, catalog, assets, materials, &build.lighting);
            build.timings.surfaces_millis += elapsed_millis(started);
            build.mesh = mesh;
        }
    }
    build
}

/// Builds the static mesh for one build: stamped with the lightmap plan when one
/// exists, or the historical vertex-lit mesh otherwise.
fn build_mesh_for_options(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallbacks: &[&PropDef],
    lighting: &LevelLighting,
    materials: &MaterialTable,
    plan: Option<&mut LightmapPlan>,
) -> LevelMesh {
    build_level_geometry_mesh_with_lightmaps(level, catalog, fallbacks, lighting, materials, plan)
}

/// Milliseconds since `started`.
fn elapsed_millis(started: std::time::Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

/// Writes every atlas page of a fresh bake under `target/agent-work/atlases/`
/// when `PLACES_DUMP_LIGHTMAPS=1` is set in the environment.
///
/// A developer dump, not a shipping path; the write failure is a one-line
/// diagnostic because there is no logger here to route it through.
#[allow(clippy::print_stderr)]
fn dump_lightmaps_if_requested(level: &LevelDef, lightmaps: &LevelLightmaps) {
    if std::env::var("PLACES_DUMP_LIGHTMAPS").as_deref() != Ok("1") {
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

/// Writes every atlas page of `lightmaps` under `target/agent-work/atlases/`
/// when `PLACES_DUMP_LIGHTMAPS=1` is set.
///
/// Public so the renderer can dump an asynchronously filled atlas exactly as
/// the inline path dumps its own; the identifier sanitising and the target
/// directory are the developer dump's, unchanged.
pub fn dump_lightmaps_for_level(level: &LevelDef, lightmaps: &LevelLightmaps) {
    dump_lightmaps_if_requested(level, lightmaps);
}

/// Whether a finished fill stamped `finished` may activate.
///
/// The renderer gives every lightmap request a monotonically increasing
/// generation and only installs the newest one, so a superseded worker whose
/// result raced its own cancellation can never replace the world. The policy
/// is a free function so it is unit-testable without a GPU device.
#[must_use]
pub const fn fill_may_activate(finished: u64, newest: u64) -> bool {
    finished == newest
}

/// What the renderer is doing about a graphics change, for a cheap status hint.
///
/// `Preparing` names the stage in progress — today only `"lightmaps"`, the one
/// asynchronous stage — so the game loop can show a subtle "Applying..."
/// message without any expensive query. The previous configuration keeps
/// rendering throughout a `Preparing` transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsTransition {
    /// Nothing is in flight; the applied configuration is fully resident.
    Idle,
    /// Work is in progress; the previous configuration keeps rendering.
    Preparing(&'static str),
}

impl GraphicsTransition {
    /// True when no transition is in flight.
    #[must_use]
    pub const fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// The shipped catalog, loaded once per process for geometry-only callers.
pub fn shipped_asset_catalog() -> &'static crate::assets::AssetCatalog {
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

#[cfg(test)]
mod tests {
    // Test code: index-free fixture access, unwraps and exact float compares
    // are idiomatic here (the crate's production lints stay enforced above).
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::panic
    )]

    use super::*;
    use crate::quality::{LightmapQuality, QualityProfile};

    /// One 8x8 m lit room: enough patches to fill an atlas, small enough that a
    /// test bake costs milliseconds.
    fn tiny_level() -> LevelDef {
        LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "async_fill_test",
                "name": "Async Fill Test",
                "spawn": { "x": 0.0, "z": 0.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 },
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("the tiny test level parses")
    }

    fn build_materials(level: &LevelDef) -> MaterialTable {
        logical_materials(level)
    }

    /// The worker's fill must produce exactly the inline build's pages.
    ///
    /// Both paths share [`fill_lightmaps`], and this pins it: the pages, the
    /// chart set and the content key are compared byte for byte (the pages are
    /// `Vec<u8>` and the charts are plain data, so equality is the same
    /// contract the atlas cache stores).
    #[test]
    fn a_worker_fill_is_byte_identical_to_the_inline_fill() {
        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let options = LightmapBuildOptions::for_lightmaps(LightmapQuality::Full);

        let mut assets = crate::props::PropAssets::default();
        let mut cache = LightmapCache::memory_only();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            options,
            Some(&mut cache),
        );
        assert!(prepared.build.lightmaps.is_none(), "uncached build");
        let fill = prepared.fill.expect("an uncached build owes a fill");

        let mut worker = LightmapFillWorker::spawn(fill.clone()).expect("a worker thread");
        let outcome = loop {
            if let Some(outcome) = worker.try_take() {
                break outcome;
            }
            std::thread::yield_now();
        };
        let LightmapFillOutcome::Filled(worker_atlas) = outcome else {
            panic!("the worker fill produced {outcome:?}");
        };

        let mut inline_assets = crate::props::PropAssets::default();
        let inline = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut inline_assets,
            &materials,
            options,
            None,
        );
        let inline_atlas = inline.lightmaps.expect("the inline fill produced an atlas");
        assert_eq!(worker_atlas.pages, inline_atlas.pages);
        assert_eq!(worker_atlas.charts, inline_atlas.charts);
        assert_eq!(worker_atlas.stats.charts, inline_atlas.stats.charts);
        assert_eq!(worker_atlas.stats.texels, inline_atlas.stats.texels);
        assert_eq!(worker_atlas.cache_key, fill.content_key);
        assert_eq!(worker_atlas.cache_key, inline_atlas.cache_key);
    }

    /// A cache hit must not start a fill; the prepared build already carries
    /// the atlas.
    #[test]
    fn a_cached_atlas_activates_without_a_fill() {
        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let options = LightmapBuildOptions::for_lightmaps(LightmapQuality::Full);
        let mut cache = LightmapCache::memory_only();

        let mut assets = crate::props::PropAssets::default();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            options,
            Some(&mut cache),
        );
        let fill = prepared.fill.expect("first build owes a fill");
        let filled = fill_lightmaps(&fill).expect("the tiny level fills");
        cache.insert(&fill.content_key, Arc::new(filled));

        let mut assets = crate::props::PropAssets::default();
        let cached = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            options,
            Some(&mut cache),
        );
        assert!(cached.fill.is_none(), "a cache hit never owes a fill");
        let atlas = cached.build.lightmaps.expect("the hit carries its atlas");
        assert_eq!(atlas.cache_key, fill.content_key);
    }

    /// A pre-set cancel flag stops the fill at the first chart boundary and is
    /// reported as cancellation, never as a bake failure.
    #[test]
    fn a_cancelled_fill_reports_cancelled_not_a_failure() {
        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let mut assets = crate::props::PropAssets::default();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            LightmapBuildOptions::for_lightmaps(LightmapQuality::Full),
            None,
        );
        let fill = prepared.fill.expect("uncached build owes a fill");
        let cancel = AtomicBool::new(true);
        let outcome = fill_lightmaps_cancellable(&fill, &cancel);
        assert!(matches!(outcome, LightmapFillOutcome::Cancelled));
    }

    /// Dropping a worker cancels it and joins the thread; the test itself
    /// proves the shutdown returns (a leaked thread would hang the suite).
    #[test]
    fn dropping_a_worker_cancels_and_joins() {
        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let mut assets = crate::props::PropAssets::default();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            LightmapBuildOptions::for_lightmaps(LightmapQuality::Full),
            None,
        );
        let fill = prepared.fill.expect("uncached build owes a fill");
        let worker = LightmapFillWorker::spawn(fill).expect("a worker thread");
        worker.cancel();
        drop(worker);
    }

    /// Only the newest generation may activate; a superseded result is
    /// discarded even if it finished.
    #[test]
    fn only_the_newest_generation_may_activate() {
        assert!(fill_may_activate(7, 7));
        assert!(!fill_may_activate(6, 7), "an older fill must not activate");
        assert!(!fill_may_activate(8, 7), "a future id is not the newest");
    }

    /// The worker's request is `Send` (and the lighting `Send + Sync`), which
    /// is what lets the fill move off the main thread with no unsafe code.
    #[test]
    fn the_fill_request_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LevelLighting>();
        assert_send_sync::<LightmapFillRequest>();
        assert_send_sync::<LightmapFillOutcome>();
    }

    /// The Lightmaps setting owns the whole build configuration, so the
    /// overall quality level can no longer change a baked texel.
    #[test]
    fn the_lightmap_setting_owns_the_build_configuration() {
        let off = LightmapBuildOptions::for_lightmaps(LightmapQuality::Off);
        assert_eq!(off.mode, LightmapMode::Off);
        let medium = LightmapBuildOptions::for_lightmaps(LightmapQuality::Medium);
        let full = LightmapBuildOptions::for_lightmaps(LightmapQuality::Full);
        assert_eq!(medium.mode, LightmapMode::On);
        assert_eq!(full.mode, LightmapMode::On);
        assert!(
            medium.config.texels_per_metre < full.config.texels_per_metre,
            "Medium bakes fewer texels per metre than Full"
        );
        assert_eq!(medium.config.page_edge, full.config.page_edge);
        assert_eq!(medium.profile, QualityProfile::Full);
        assert_eq!(full.profile, QualityProfile::Full);
        assert_eq!(medium.bake, LightmapQuality::Medium.bake_config().unwrap());
        assert_eq!(full.bake, LightmapQuality::Full.bake_config().unwrap());
        // The vertex-lit build ignores the setting's bake entirely and keeps
        // the historical hard-shadow bake.
        assert_eq!(off.bake, BakeConfig::HARD);
    }

    /// The status is `Idle` unless a stage is running.
    #[test]
    fn the_transition_status_is_idle_by_default() {
        assert!(GraphicsTransition::Idle.is_idle());
        assert!(!GraphicsTransition::Preparing("lightmaps").is_idle());
    }
}

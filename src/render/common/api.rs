//! The level-geometry entry points.
//!
//! These are the functions the game and the audits call: each one builds the
//! static mesh (and, where asked, the prop batches) from a level, a catalog and
//! a material table, with the lighting baked exactly once per level load.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::geometry::build_level_geometry_mesh_with_lightmaps;
use super::props::{PropMeshBatch, resolve_prop_instances, resolve_prop_instances_lightmapped};
use super::{
    LevelDef, LevelLighting, LevelMesh, LevelSurfaces, MaterialTable, PropDef,
    build_level_geometry_mesh,
};
use crate::lighting::BakeConfig;
use crate::lighting::lightmap::{
    Chart, LevelLightmaps, LightmapAtlas, LightmapCache, LightmapConfig, LightmapFailure,
    LightmapMode, LightmapPatch, LightmapPlan, LightmapStats, LightmapTexel, SwitchableLightmaps,
    content_key_with_extra, write_page_png,
};
use crate::lighting::probes::ProbeField;
use crate::lighting::transport::{MAX_TRANSPORT_WORKERS, SolveOptions};

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
    /// Transport budget: emitter taps, diffuse bounces and gather samples.
    pub solve: SolveOptions,
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
            solve: match mode {
                LightmapMode::On => solve_options_for_profile(level.profile()),
                LightmapMode::Off => SolveOptions {
                    taps_per_axis: 1,
                    bounces: 0,
                    gather_samples: 1,
                    workers: 1,
                },
            },
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
            solve: solve_options_for_profile(profile),
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
            solve: solve_options_for_lightmaps(lightmaps),
        }
    }
}

/// The transport budget one lightmap quality selects.
///
/// `Off` never solves; `Medium` runs one diffuse bounce with a coarse emitter
/// tap pattern; `Full` runs two with the full pattern. The two prepared
/// qualities therefore ship genuinely different transport data, not the same
/// solve at a different resolution.
#[must_use]
pub const fn solve_options_for_lightmaps(quality: crate::quality::LightmapQuality) -> SolveOptions {
    match quality {
        crate::quality::LightmapQuality::Off => SolveOptions {
            taps_per_axis: 1,
            bounces: 0,
            gather_samples: 1,
            workers: 1,
        },
        crate::quality::LightmapQuality::Medium => SolveOptions {
            taps_per_axis: 2,
            bounces: 1,
            gather_samples: 32,
            workers: 1,
        },
        crate::quality::LightmapQuality::Full => SolveOptions {
            taps_per_axis: 3,
            bounces: 2,
            gather_samples: 64,
            workers: 1,
        },
    }
}

/// The transport budget one quality profile selects (audits and tests).
#[must_use]
pub const fn solve_options_for_profile(profile: crate::quality::QualityProfile) -> SolveOptions {
    match profile {
        crate::quality::QualityProfile::Low => SolveOptions {
            taps_per_axis: 1,
            bounces: 0,
            gather_samples: 1,
            workers: 1,
        },
        crate::quality::QualityProfile::Full => SolveOptions {
            taps_per_axis: 2,
            bounces: 1,
            gather_samples: 32,
            workers: 1,
        },
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
    /// The prepared irradiance field moving objects sample, when this build
    /// produced one.
    pub probes: Option<Arc<crate::lighting::probes::ProbeField>>,
    /// Why an `On` build fell back to vertex colours, if it did.
    pub lightmap_failure: Option<LightmapFailure>,
    /// Wall-clock cost of filling and packing the atlas, in milliseconds.
    pub lightmap_millis: f64,
}

impl LevelBuild {
    /// Conservative retained data size. Shared images are counted once within
    /// this build; shared atlases across cache entries may be counted twice.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        let mut bytes = std::mem::size_of::<Self>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
            .saturating_add(allocation_bytes(&self.mesh.ranges))
            .saturating_add(allocation_bytes(&self.batches))
            .saturating_add(self.lighting.retained_heap_bytes());
        for range in &self.mesh.ranges {
            bytes = bytes
                .saturating_add(allocation_bytes(&range.vertices))
                .saturating_add(allocation_bytes(&range.indices));
        }
        let mut images = std::collections::HashSet::new();
        for batch in &self.batches {
            bytes = bytes
                .saturating_add(batch.model.capacity())
                .saturating_add(allocation_bytes(&batch.vertices))
                .saturating_add(allocation_bytes(&batch.indices))
                .saturating_add(allocation_bytes(&batch.submeshes))
                .saturating_add(allocation_bytes(&batch.textures));
            for image in &batch.textures {
                if images.insert(Arc::as_ptr(image)) {
                    bytes = bytes
                        .saturating_add(std::mem::size_of_val(image.as_ref()))
                        .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
                        .saturating_add(image.rgba.capacity());
                }
            }
        }
        if let Some(atlas) = &self.lightmaps {
            bytes = bytes
                .saturating_add(std::mem::size_of_val(atlas.as_ref()))
                .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
                .saturating_add(allocation_bytes(&atlas.pages))
                .saturating_add(allocation_bytes(&atlas.charts))
                .saturating_add(allocation_bytes(&atlas.switchable))
                .saturating_add(atlas.cache_key.capacity());
            for page in &atlas.pages {
                bytes = bytes.saturating_add(allocation_bytes(&page.texels));
            }
            for contribution in &atlas.switchable {
                bytes = bytes.saturating_add(allocation_bytes(&contribution.pages));
                for page in &contribution.pages {
                    bytes = bytes.saturating_add(allocation_bytes(&page.texels));
                }
            }
        }
        bytes
    }
}

/// Vec capacity is required here: a slice omits allocated unused elements.
const fn allocation_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(std::mem::size_of::<T>())
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
/// The loader prepares lighting, props, and the plan-stamped mesh on its
/// worker before completing the cancellable per-texel fill. `fill` is `None`
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
/// It carries everything the transport solve reads and nothing else: the
/// prebuilt static scene and its acceleration structure, the atlas
/// configuration, the plan's charts and page count, the switchable lights whose
/// contributions need their own layer sets, the solve budget, and the
/// deterministic content key the result must be cached under. The loader
/// executes the request on its preparation worker while the renderer keeps
/// drawing the resident world.
#[derive(Clone, Debug)]
pub struct LightmapFillRequest {
    /// Density, page budget and padding the plan packed against.
    pub config: LightmapConfig,
    /// Every chart of the finished plan, paired with its patch, in plan order.
    pub charts: Vec<(LightmapPatch, Chart)>,
    /// Pages the plan's packer opened.
    pub page_count: usize,
    /// Deterministic content key of the inputs this fill describes.
    pub content_key: String,
    /// The static scene and light sources the solve evaluates.
    pub transport: Arc<crate::lighting::transport::TransportScene>,
    /// Tap counts, bounce count and sample budget.
    pub options: crate::lighting::transport::SolveOptions,
    /// Bake the irradiance field moving objects sample at runtime.
    pub probe_bake: bool,
}

/// One finished fill: the atlas and the prepared moving-object field.
#[derive(Debug)]
pub struct LightmapFillProduct {
    /// The assembled HDR atlas.
    pub lightmaps: LevelLightmaps,
    /// The prepared irradiance field, when the request asked for one. Probe
    /// rooms are unassigned until the compiler labels them.
    pub probes: Option<ProbeField>,
}

impl LightmapFillProduct {
    /// The atlas alone, for callers that do not need the field. Test-only:
    /// production callers keep the probes for the blended-decal relight.
    #[cfg(test)]
    #[must_use]
    pub fn into_lightmaps(self) -> LevelLightmaps {
        self.lightmaps
    }
}

/// What one lightmap fill produced.
#[derive(Debug)]
pub enum LightmapFillOutcome {
    /// The atlas was filled; its pages are identical to an inline bake of the
    /// same request, and the prepared probe field is included when asked for.
    Filled(Box<LightmapFillProduct>),
    /// The fill failed (`FillSize`, `FillNonFinite`, `Layout`, ...); the caller
    /// must keep the historical vertex-lit fallback.
    Failed(LightmapFailure),
    /// A newer request superseded this one; the result must never activate.
    Cancelled,
}

/// Fills one request to completion on the calling thread.
///
/// This is the tests' reference: the same body the worker runs, with a
/// cancellation flag that is never set, which is what keeps the synchronous and
/// asynchronous pages byte-identical. Production callers use
/// [`fill_lightmaps_full`] or [`fill_lightmaps_cancellable`] so the prepared
/// probe field survives for the blended-decal relight.
///
/// # Errors
///
/// Returns the named [`LightmapFailure`] when a chart's texels are not exactly
/// what the plan described; the caller must rebuild the vertex-lit level.
#[cfg(test)]
pub fn fill_lightmaps(request: &LightmapFillRequest) -> Result<LevelLightmaps, LightmapFailure> {
    Ok(fill_lightmaps_full(request)?.into_lightmaps())
}

/// [`fill_lightmaps`] returning the prepared probe field as well.
///
/// # Errors
///
/// Returns the named [`LightmapFailure`] when a chart's texels are not exactly
/// what the plan described; the caller must rebuild the vertex-lit level.
pub fn fill_lightmaps_full(
    request: &LightmapFillRequest,
) -> Result<LightmapFillProduct, LightmapFailure> {
    bake_request(request, None)
}

/// The worker's body: [`fill_lightmaps`] with cancellation polled by the
/// solver, reported as [`LightmapFillOutcome::Cancelled`] when the flag was set.
pub fn fill_lightmaps_cancellable(
    request: &LightmapFillRequest,
    cancel: &AtomicBool,
) -> LightmapFillOutcome {
    match bake_request_with_workers(request, Some(cancel), runtime_fill_workers()) {
        Ok(lightmaps) if !cancel.load(Ordering::Relaxed) => {
            LightmapFillOutcome::Filled(Box::new(lightmaps))
        }
        Ok(_) => LightmapFillOutcome::Cancelled,
        Err(_) if cancel.load(Ordering::Relaxed) => LightmapFillOutcome::Cancelled,
        Err(failure) => LightmapFillOutcome::Failed(failure),
    }
}

/// The shared fill body, optionally cancellation-aware.
///
/// Cancellation is polled inside the transport solve; a cancelled solve stops
/// early and reports a fill failure, which the worker maps to `Cancelled` by
/// re-checking the flag after the fact, so a cancellation is never mistaken for
/// a real bake failure.
fn bake_request(
    request: &LightmapFillRequest,
    cancel: Option<&AtomicBool>,
) -> Result<LightmapFillProduct, LightmapFailure> {
    bake_request_with_workers(request, cancel, 1)
}

/// Process-wide atlas-fill worker override, set once by the offline compiler
/// from its `--workers` allocation.
///
/// `0` means "derive from available parallelism". The value is clamped to the
/// transport solve's own 1..=[`MAX_TRANSPORT_WORKERS`] bound, and `1` selects
/// the serial reference path.
static FILL_WORKERS: AtomicUsize = AtomicUsize::new(0);

/// Overrides the atlas-fill worker count for this process.
///
/// The offline compiler calls this with its `--workers` value so a tool run
/// honours the shared CPU allocation; `1` forces the serial reference path.
/// Values above the transport solve's bound are clamped. The same value caps
/// nested preparation work (for example level texture decode) through
/// [`crate::perf::set_prepare_workers`], so one command never exceeds its one
/// shared budget.
pub fn set_fill_workers(workers: usize) {
    FILL_WORKERS.store(workers.clamp(1, MAX_TRANSPORT_WORKERS), Ordering::Relaxed);
    crate::perf::set_prepare_workers(workers);
}

/// Ordinary tests and inline bakes remain serial. The runtime loader owns the
/// only outer preparation worker; leave one logical CPU for UI/event handling.
fn runtime_fill_workers() -> usize {
    if cfg!(test) {
        return 1;
    }
    let override_workers = FILL_WORKERS.load(Ordering::Relaxed);
    if override_workers > 0 {
        return override_workers;
    }
    if let Ok(value) = std::env::var("PLACES_TOOL_WORKERS")
        && let Ok(requested @ 1..=MAX_TRANSPORT_WORKERS) = value.parse::<usize>()
    {
        return requested;
    }
    let available = std::thread::available_parallelism().map_or(1, |count| {
        count
            .get()
            .saturating_sub(1)
            .clamp(1, MAX_TRANSPORT_WORKERS)
    });
    if std::env::var("PLACES_BENCH").as_deref() == Ok("1")
        && let Ok(value) = std::env::var("PLACES_LIGHTMAP_WORKERS")
    {
        if let Ok(requested @ 1..=MAX_TRANSPORT_WORKERS) = value.parse::<usize>() {
            return requested.min(available);
        }
        crate::logging::warn(
            "PLACES_LIGHTMAP_WORKERS must be a worker count from 1 to 12; using available workers",
        );
    }
    available
}

fn bake_request_with_workers(
    request: &LightmapFillRequest,
    cancel: Option<&AtomicBool>,
    workers: usize,
) -> Result<LightmapFillProduct, LightmapFailure> {
    let started = std::time::Instant::now();
    let options = crate::lighting::transport::SolveOptions {
        workers,
        ..request.options
    };
    crate::logging::info(format_args!(
        "[lightmaps] transport workers={} charts={} bounces={} taps={}",
        options.workers.clamp(1, MAX_TRANSPORT_WORKERS),
        request.charts.len(),
        options.bounces,
        options.taps_per_axis
    ));
    // Every chart is validated before the solver walks it: an out-of-page or
    // absurdly large rectangle is a named failure, never a huge allocation.
    LightmapAtlas::validate_layout(&request.config, request.page_count, &request.charts)?;
    let solve = request.transport.solve_with_probes(
        &request.charts,
        options,
        cancel,
        request.probe_bake,
    )?;
    let solution = solve.solution;
    let base: Vec<Vec<LightmapTexel>> = solution
        .charts
        .iter()
        .map(|chart| chart.texels.clone())
        .collect();
    let atlas =
        LightmapAtlas::assemble(&request.config, request.page_count, &request.charts, &base)?;
    let mut switchable = Vec::with_capacity(solution.switchable.len());
    for (light_index, charts) in &solution.switchable {
        let texels: Vec<Vec<LightmapTexel>> =
            charts.iter().map(|chart| chart.texels.clone()).collect();
        let pages = LightmapAtlas::assemble(
            &request.config,
            request.page_count,
            &request.charts,
            &texels,
        )?
        .into_pages();
        switchable.push(SwitchableLightmaps {
            light_index: *light_index,
            pages,
        });
    }
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
    Ok(LightmapFillProduct {
        lightmaps: LevelLightmaps {
            pages: atlas.into_pages(),
            charts: request.charts.clone(),
            stats,
            cache_key: request.content_key.clone(),
            padding: request.config.padding,
            switchable,
        },
        probes: solve.probes,
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
#[allow(clippy::too_many_lines)] // one cohesive prepare-and-decide pass
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
    let (mut batches, fallbacks) =
        resolve_prop_instances(level, catalog, assets, &lighting, &surfaces);
    let mut props_millis = elapsed_millis(started);

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
    if let Some(plan) = plan.as_mut() {
        let started = std::time::Instant::now();
        let (receivers, _) =
            resolve_prop_instances_lightmapped(level, catalog, assets, &lighting, &surfaces, plan);
        if !plan.failed() {
            batches = receivers;
        }
        props_millis += elapsed_millis(started);
    }

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
        probes: None,
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
            // prop-occlusion cell), the occluder set the bake actually uses and
            // the light-model constants the equation evaluates with, so a prop
            // model, a light, a shadow-quality constant or a lighting-model
            // recalibration invalidates the cached atlas while a texture-only
            // edit does not. The transport solver's own revision also enters the
            // key through its fingerprint, so a solver change invalidates every
            // cached atlas without invalidating an unchanged source.
            let mut extra: Vec<u8> = Vec::with_capacity(32);
            extra.extend_from_slice(&build.lighting.occlusion_fingerprint().to_le_bytes());
            extra.push(bake.sampling.taps_per_axis);
            extra.extend_from_slice(&bake.prop_occlusion_cell_m.to_bits().to_le_bytes());
            extra.extend_from_slice(&crate::lighting::model_fingerprint().to_le_bytes());
            extra
                .extend_from_slice(&crate::lighting::transport::solver_fingerprint().to_le_bytes());
            extra.push(options.solve.taps_per_axis);
            extra.push(options.solve.bounces);
            extra.extend_from_slice(
                &u32::try_from(options.solve.gather_samples)
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            let key = content_key_with_extra(level, &options.config, options.profile, &extra);
            if let Some(cached) = cache.and_then(|cache| cache.get(&key)) {
                build.lightmaps = Some(cached);
            } else {
                match super::light_transport::build_transport_scene(
                    level,
                    &build.mesh,
                    &build.batches,
                    materials,
                    &build.lighting,
                    plan.charts(),
                ) {
                    Some((transport, scene_stats)) => {
                        crate::logging::info(format_args!(
                            "[lightmaps] transport scene triangles={} ({} skipped) emitters={} ({} switchable)",
                            scene_stats.triangles,
                            scene_stats.skipped_triangles,
                            scene_stats.emitters,
                            scene_stats.switchable_emitters
                        ));
                        fill = Some(LightmapFillRequest {
                            config: options.config,
                            charts: plan.charts().to_vec(),
                            page_count: plan.page_count(),
                            content_key: key,
                            transport: Arc::new(transport),
                            options: options.solve,
                            probe_bake: true,
                        });
                    }
                    None => {
                        build.lightmap_failure = Some(LightmapFailure::TransportScene);
                    }
                }
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
        (build.batches, _) =
            resolve_prop_instances(level, catalog, assets, &build.lighting, &surfaces);
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
    batches: &mut Vec<PropMeshBatch>,
) -> LevelMesh {
    let surfaces = LevelSurfaces::new(level);
    let (legacy, fallbacks) = resolve_prop_instances(level, catalog, assets, lighting, &surfaces);
    *batches = legacy;
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
    match fill_lightmaps_full(&request) {
        Ok(product) => {
            let crate::render::common::api::LightmapFillProduct { lightmaps, probes } = product;
            let lightmaps = Arc::new(lightmaps);
            build.lightmap_millis = lightmaps.stats.bake_millis;
            dump_lightmaps_for_level(level, &lightmaps);
            if let Some(cache) = cache {
                cache.insert(&request.content_key, Arc::clone(&lightmaps));
            }
            // The prepared solve lights the surfaces; blended feather decals
            // must read with it instead of the vertex-lit approximation (see
            // `relight_blend_decals`). Cut-out decals are untouched.
            if let Some(field) = probes.as_ref() {
                crate::render::relight_blend_decals(
                    &mut build.mesh,
                    level,
                    catalog.assets(),
                    &build.lighting,
                    field,
                );
            }
            build.lightmaps = Some(lightmaps);
        }
        Err(fill_failure) => {
            build.lightmap_failure = Some(fill_failure);
            let started = std::time::Instant::now();
            let mesh = rebuild_vertex_lit_level(
                level,
                catalog,
                assets,
                materials,
                &build.lighting,
                &mut build.batches,
            );
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

/// Writes every atlas page of a fresh bake under `target/diagnostics/atlases/`
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
    let dir = crate::assets::state_path("target/diagnostics/atlases");
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

/// Writes every atlas page of `lightmaps` under `target/diagnostics/atlases/`
/// when `PLACES_DUMP_LIGHTMAPS=1` is set.
///
/// Public so the renderer can dump an asynchronously filled atlas exactly as
/// the inline path dumps its own; the identifier sanitising and the target
/// directory are the developer dump's, unchanged.
pub fn dump_lightmaps_for_level(level: &LevelDef, lightmaps: &LevelLightmaps) {
    dump_lightmaps_if_requested(level, lightmaps);
}

/// What the renderer is doing about a graphics change, for a cheap status hint.
///
/// `Preparing` names the GPU preparation stage so the game loop can show a status
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
    // The asset-less path draws every prop as its placeholder box, but a
    // floating prop has no static box: its geometry lives in the dynamic
    // scene's float lane, so drawing a box on the basin floor would disagree
    // with the drawn float and leak into the vertex-lit parity checks.
    let fallbacks: Vec<&PropDef> = level
        .props
        .iter()
        .filter(|prop| prop.float.is_none())
        .collect();
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
                "format_version": 3,
                "id": "async_fill_test",
                "name": "Async Fill Test",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
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

        let outcome = std::thread::scope(|scope| {
            scope
                .spawn(|| fill_lightmaps_cancellable(&fill, &AtomicBool::new(false)))
                .join()
                .expect("the cancellable fill thread completed")
        });
        let LightmapFillOutcome::Filled(worker_product) = outcome else {
            panic!("the worker fill produced {outcome:?}");
        };
        let worker_atlas = worker_product.lightmaps;
        assert!(
            worker_product.probes.is_some(),
            "the worker fill must prepare the moving-object field"
        );

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
        let filled = Arc::new(fill_lightmaps(&fill).expect("the tiny level fills"));
        let cold_pages = filled.pages.clone();
        let cold_charts = filled.charts.clone();
        cache.insert(&fill.content_key, Arc::clone(&filled));

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
        assert_eq!(
            atlas.pages, cold_pages,
            "a warm cache hit must serve the cold fill's pages byte for byte"
        );
        assert_eq!(atlas.charts, cold_charts, "charts are part of the atlas");
    }

    /// Changing only the bake settings (taps and prop-occlusion cell) must be a
    /// new cache entry: these values change baked texels, so a warm atlas at one
    /// setting can never answer a request at the other.
    #[test]
    fn a_bake_settings_change_is_a_new_cache_entry() {
        use crate::lighting::ShadowSampling;

        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let cache = LightmapCache::memory_only();
        let fine = LightmapBuildOptions::for_lightmaps(LightmapQuality::Full);
        let mut coarse = fine;
        coarse.bake = BakeConfig {
            sampling: ShadowSampling { taps_per_axis: 1 },
            prop_occlusion_cell_m: 0.15,
        };
        assert_eq!(fine.config, coarse.config, "only the bake settings differ");
        assert_eq!(fine.profile, coarse.profile);

        let mut assets = crate::props::PropAssets::default();
        let mut cache = cache;
        let fine_build = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            fine,
            Some(&mut cache),
        );
        let fine_fill = fine_build.fill.expect("the fine bake owes a fill");
        let fine_atlas = fill_lightmaps(&fine_fill).expect("the fine bake fills");
        cache.insert(&fine_fill.content_key, Arc::new(fine_atlas));

        // The same configuration is a hit...
        let mut assets = crate::props::PropAssets::default();
        let hit = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            fine,
            Some(&mut cache),
        );
        assert!(hit.fill.is_none(), "the fine configuration is warm");

        // ...but the coarse bake is a miss, even though the atlas config and
        // profile are identical: the bake settings are part of the key.
        let mut assets = crate::props::PropAssets::default();
        let coarse_build = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            coarse,
            Some(&mut cache),
        );
        let coarse_fill = coarse_build
            .fill
            .expect("the coarse bake owes its own fill");
        assert_ne!(
            coarse_fill.content_key, fine_fill.content_key,
            "bake taps and prop-occlusion cell must be part of the key"
        );
        let coarse_atlas = fill_lightmaps(&coarse_fill).expect("the coarse bake fills");
        // The tiny fixture is unoccluded, so both bakes can resolve the same
        // visibility fractions and their texels may agree; the key difference
        // above is the contract. Only assert the coarse fill really produced
        // addressable pages.
        assert!(!coarse_atlas.pages.is_empty());
    }

    /// The renderer's vertex-lit entry point is the historical, quality-blind
    /// path: `Lightmaps Off` from the Lightmaps setting and an Off build at any
    /// overall level must produce the exact same mesh and lighting.
    #[test]
    fn the_renderer_vertex_lit_entry_point_is_quality_independent() {
        use crate::quality::QualityLevel;

        let level = tiny_level();
        let materials = build_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let reference = LightmapBuildOptions::for_lightmaps(LightmapQuality::Off);
        assert_eq!(reference.mode, LightmapMode::Off);
        assert_eq!(reference.bake, BakeConfig::HARD);
        let mut assets = crate::props::PropAssets::default();
        let reference_build = prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            reference,
            None,
        );
        assert!(reference_build.fill.is_none(), "Off never owes a fill");
        assert!(reference_build.build.lightmaps.is_none());

        for quality in QualityLevel::ALL {
            let options = LightmapBuildOptions::for_level(quality, LightmapMode::Off);
            let mut assets = crate::props::PropAssets::default();
            let build = prepare_level_geometry_with_lightmaps(
                &level,
                &catalog,
                &mut assets,
                &materials,
                options,
                None,
            );
            assert!(build.fill.is_none(), "{quality:?}");
            assert!(build.build.lightmaps.is_none(), "{quality:?}");
            assert_eq!(
                build.build.mesh.ranges, reference_build.build.mesh.ranges,
                "{quality:?} + Off must not change a single vertex"
            );
            assert_eq!(
                build.build.lighting.summary(),
                reference_build.build.lighting.summary(),
                "{quality:?} + Off must not change the bake"
            );
        }
    }

    #[test]
    fn transport_workers_match_serial_and_handle_cancellation() {
        let mut level = tiny_level();
        level.walls =
            serde_json::from_str(r#"[{"x":1.0,"z":1.0,"width":4.0,"depth":0.2,"height":2.0}]"#)
                .expect("vertical faces exercise the transport scene");
        let materials = build_materials(&level);
        let mut assets = crate::props::PropAssets::default();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &crate::loader::PropCatalog::builtin(),
            &mut assets,
            &materials,
            LightmapBuildOptions::for_lightmaps(LightmapQuality::Full),
            None,
        );
        let request = prepared.fill.expect("uncached tiny plan");
        assert!(request.charts.len() > 3);
        assert!(
            request.transport.triangle_count() > 0,
            "the transport scene must see the level's triangles"
        );
        let serial = fill_lightmaps_full(&request).expect("serial fill");
        let serial_atlas = &serial.lightmaps;
        assert!(
            serial.probes.is_some(),
            "the solve prepares the moving-object field"
        );
        for workers in [2_usize, 3, 4, 8] {
            let parallel =
                super::bake_request_with_workers(&request, Some(&AtomicBool::new(false)), workers)
                    .expect("parallel fill");
            assert_eq!(
                parallel.lightmaps.pages, serial_atlas.pages,
                "workers={workers}"
            );
            assert_eq!(
                parallel.lightmaps.charts, serial_atlas.charts,
                "workers={workers}"
            );
            assert_eq!(
                parallel.lightmaps.switchable, serial_atlas.switchable,
                "workers={workers}"
            );
            assert_eq!(parallel.lightmaps.cache_key, serial_atlas.cache_key);
            assert_eq!(
                parallel.probes, serial.probes,
                "the probe field must not depend on the worker count"
            );
        }
        let cancel = AtomicBool::new(true);
        assert_eq!(
            super::bake_request_with_workers(&request, Some(&cancel), 4).err(),
            Some(LightmapFailure::FillSize)
        );
    }

    /// Invalid chart layouts and configurations are rejected before the solver
    /// walks them, so a malformed plan can never request an unsafe allocation.
    #[test]
    fn invalid_chart_layouts_are_rejected_by_the_fill() {
        let level = tiny_level();
        let materials = build_materials(&level);
        let mut assets = crate::props::PropAssets::default();
        let prepared = prepare_level_geometry_with_lightmaps(
            &level,
            &crate::loader::PropCatalog::builtin(),
            &mut assets,
            &materials,
            LightmapBuildOptions::for_lightmaps(LightmapQuality::Full),
            None,
        );
        let mut invalid = prepared.fill.expect("uncached tiny plan");
        let original = invalid.charts.first().expect("chart").1;
        for (width, height, page) in [
            (u32::MAX, original.height, original.page),
            (original.width, u32::MAX, original.page),
            (0, original.height, original.page),
            (original.width, original.height, u16::MAX),
        ] {
            let chart = &mut invalid.charts.first_mut().expect("chart").1;
            chart.width = width;
            chart.height = height;
            chart.page = page;
            assert_eq!(
                LightmapAtlas::validate_layout(
                    &invalid.config,
                    invalid.page_count,
                    &invalid.charts
                ),
                Err(LightmapFailure::Layout)
            );
            assert_eq!(
                fill_lightmaps(&invalid).err(),
                Some(LightmapFailure::Layout)
            );
        }
        invalid.charts.first_mut().expect("chart").1 = original;
        invalid.config.page_edge = 0;
        assert_eq!(
            fill_lightmaps(&invalid).err(),
            Some(LightmapFailure::InvalidConfig)
        );
        invalid.config.page_edge = u32::MAX;
        assert_eq!(
            fill_lightmaps(&invalid).err(),
            Some(LightmapFailure::InvalidConfig)
        );
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

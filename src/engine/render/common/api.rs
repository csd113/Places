//! The level-geometry entry points.
//!
//! These are the functions the game and the audits call: each one builds the
//! static mesh (and, where asked, the prop batches) from a level, a catalog and
//! a material table, with the lighting baked exactly once per level load.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::geometry::build_level_geometry_mesh_with_fixture_models;
use super::props::{
    PropMeshBatch, resolve_prop_fallbacks, resolve_prop_instances,
    resolve_prop_instances_lightmapped,
};
use super::{
    LevelDef, LevelLighting, LevelMesh, LevelSurfaces, MaterialTable, PropDef,
    build_level_geometry_mesh,
};
use crate::engine::lighting::BakeConfig;
use crate::engine::lighting::lightmap::{
    Chart, LevelLightmaps, LightmapAtlas, LightmapCache, LightmapConfig, LightmapFailure,
    LightmapMode, LightmapPatch, LightmapPlan, LightmapStats, LightmapTexel, SwitchableLightmaps,
    content_key_with_extra, write_page_png,
};
use crate::engine::lighting::probes::ProbeField;
use crate::engine::lighting::transport::{MAX_TRANSPORT_WORKERS, SolveOptions};

/// Builds the level mesh with real prop geometry where possible, plus one
/// batched draw per distinct prop model.
///
/// Props whose model is missing, malformed or simply absent from the catalogue
/// still emit their catalogue-sized placeholder box into
/// `LevelMesh::batches.prop_batch`, so a broken asset degrades visibly instead
/// of vanishing, and never crashes or loops (failures are cached by
/// [`crate::engine::props::PropAssets`]).
pub fn build_level_geometry_with_assets(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
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
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
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
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
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
    pub profile: crate::engine::quality::QualityProfile,
    /// The shadow bake to run: taps and prop-occlusion cell.
    pub bake: BakeConfig,
    /// Transport budget: emitter taps, diffuse bounces and gather samples.
    pub solve: SolveOptions,
}

impl LightmapBuildOptions {
    /// Keep the quality's density and gutters while bounding its page count
    /// by the actual base and switchable groups the compiler will serialize.
    fn with_prepared_page_budget(mut self, level: &LevelDef, lighting: &LevelLighting) -> Self {
        if self.mode == LightmapMode::On {
            let switchable = super::light_transport::switchable_groups(level, lighting).len();
            self.config.max_pages = self.config.max_pages.min(
                crate::engine::package::lightmaps::preparation_page_limit(
                    self.config.page_edge,
                    switchable,
                ),
            );
        }
        self
    }

    /// The options one quality level implies for a mode.
    ///
    /// The lightmap configuration and the bake come from the level; the profile
    /// is only the protected content-key boundary (`Medium` and `High` both map
    /// to `Full`, and the key itself keeps their configs and bake settings
    /// apart).
    #[must_use]
    pub const fn for_level(
        level: crate::engine::quality::QualityLevel,
        mode: LightmapMode,
    ) -> Self {
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
    pub const fn for_profile(
        profile: crate::engine::quality::QualityProfile,
        mode: LightmapMode,
    ) -> Self {
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
    /// setting: [`crate::engine::quality::LightmapQuality::Off`] builds the historical
    /// vertex-lit level, and Medium/Full bake against their own density, page
    /// budget and shadow bake. The overall [`crate::engine::quality::QualityLevel`] no
    /// longer decides any of it, so `Low + Lightmaps Full` bakes a Full atlas
    /// and `High + Lightmaps Off` stays vertex-lit.
    #[must_use]
    pub const fn for_lightmaps(lightmaps: crate::engine::quality::LightmapQuality) -> Self {
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
        lightmaps: crate::engine::quality::LightmapQuality,
        mode: LightmapMode,
    ) -> Self {
        let config = match lightmaps.lightmap_config() {
            Some(config) => config,
            None => match crate::engine::quality::LightmapQuality::Full.lightmap_config() {
                Some(config) => config,
                None => LightmapConfig::for_profile(crate::engine::quality::QualityProfile::Full),
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
pub const fn solve_options_for_lightmaps(
    quality: crate::engine::quality::LightmapQuality,
) -> SolveOptions {
    match quality {
        crate::engine::quality::LightmapQuality::Off => SolveOptions {
            taps_per_axis: 1,
            bounces: 0,
            gather_samples: 1,
            workers: 1,
        },
        crate::engine::quality::LightmapQuality::Medium => SolveOptions {
            taps_per_axis: 2,
            bounces: 1,
            gather_samples: 32,
            workers: 1,
        },
        crate::engine::quality::LightmapQuality::Full => SolveOptions {
            taps_per_axis: 3,
            bounces: 2,
            gather_samples: 64,
            workers: 1,
        },
    }
}

/// The transport budget one quality profile selects (audits and tests).
#[must_use]
pub const fn solve_options_for_profile(
    profile: crate::engine::quality::QualityProfile,
) -> SolveOptions {
    match profile {
        crate::engine::quality::QualityProfile::Low => SolveOptions {
            taps_per_axis: 1,
            bounces: 0,
            gather_samples: 1,
            workers: 1,
        },
        crate::engine::quality::QualityProfile::Full => SolveOptions {
            taps_per_axis: 2,
            bounces: 1,
            gather_samples: 32,
            workers: 1,
        },
    }
}

/// One exact optional vertex-lit off state, prepared by the compiler.
pub struct StaticLightState {
    pub fixtures: Vec<usize>,
    pub mesh: LevelMesh,
    pub batches: Vec<PropMeshBatch>,
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
    pub static_light_off: Option<Arc<StaticLightState>>,
    pub lighting: LevelLighting,
    pub timings: BuildTimings,
    pub lightmaps: Option<Arc<LevelLightmaps>>,
    /// The prepared irradiance field moving objects sample, when this build
    /// produced one.
    pub probes: Option<Arc<crate::engine::lighting::probes::ProbeField>>,
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
        if let Some(state) = &self.static_light_off {
            bytes = bytes
                .saturating_add(std::mem::size_of_val(state.as_ref()))
                .saturating_add(allocation_bytes(&state.fixtures))
                .saturating_add(allocation_bytes(&state.mesh.ranges))
                .saturating_add(allocation_bytes(&state.batches));
            for range in &state.mesh.ranges {
                bytes = bytes
                    .saturating_add(allocation_bytes(&range.vertices))
                    .saturating_add(allocation_bytes(&range.indices));
            }
            for batch in &state.batches {
                bytes = bytes
                    .saturating_add(batch.model.capacity())
                    .saturating_add(allocation_bytes(&batch.vertices))
                    .saturating_add(allocation_bytes(&batch.indices))
                    .saturating_add(allocation_bytes(&batch.submeshes))
                    .saturating_add(allocation_bytes(&batch.source_primitives))
                    .saturating_add(allocation_bytes(&batch.textures));
            }
        }
        for batch in &self.batches {
            bytes = bytes
                .saturating_add(batch.model.capacity())
                .saturating_add(allocation_bytes(&batch.vertices))
                .saturating_add(allocation_bytes(&batch.indices))
                .saturating_add(allocation_bytes(&batch.submeshes))
                .saturating_add(allocation_bytes(&batch.source_primitives))
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
                bytes = bytes.saturating_add(page.retained_pixel_bytes());
            }
            for contribution in &atlas.switchable {
                bytes = bytes.saturating_add(allocation_bytes(&contribution.pages));
                for page in &contribution.pages {
                    bytes = bytes.saturating_add(page.retained_pixel_bytes());
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
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting, BuildTimings) {
    let started = std::time::Instant::now();
    let lighting = LevelLighting::bake(level);
    let lighting_millis = started.elapsed().as_secs_f64() * 1_000.0_f64;

    let surfaces = LevelSurfaces::new(level);
    let props_started = std::time::Instant::now();
    let (batches, fallbacks) =
        resolve_prop_instances(level, catalog, assets, &lighting, &surfaces, materials);
    let props_millis = props_started.elapsed().as_secs_f64() * 1_000.0_f64;

    let surfaces_started = std::time::Instant::now();
    let mesh = build_level_geometry_mesh_with_fixture_models(
        level, catalog, &fallbacks, &lighting, materials, None, &batches,
    );
    let surfaces_millis = surfaces_started.elapsed().as_secs_f64() * 1_000.0_f64;

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

/// Prepares an inspection view through the ordinary geometry and asset path.
///
/// Authoring needs readable surfaces before lights have been placed. This
/// applies a neutral studio fill to the shared vertex albedo after the cheap
/// vertex preparation; it never solves lightmaps or captures reflection probes.
/// The returned world uses the ordinary staged renderer upload unchanged.
pub fn build_authoring_preview(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
) -> LevelBuild {
    authoring_preview(level, catalog, assets, materials, true)
}

/// Prepares the editor's cutaway with authored vertex lighting and atmosphere.
/// This is the bounded interactive preview; Play uses the compiled transport.
pub fn build_authoring_environment_preview(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
) -> LevelBuild {
    authoring_preview(level, catalog, assets, materials, false)
}

fn authoring_preview(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
    neutral: bool,
) -> LevelBuild {
    let (mut mesh, mut batches, lighting, timings) =
        build_level_geometry_timed(level, catalog, assets, materials);
    // A cutaway inspection view retains roof ownership for walls and fixtures,
    // while omitting only the ceiling draw ranges from the editor's GPU copy.
    mesh.ranges
        .retain(|range| range.key.kind != super::SurfaceKind::Ceiling);
    mesh.vertex_count = mesh.ranges.iter().map(|range| range.vertices.len()).sum();
    mesh.index_count = mesh.ranges.iter().map(|range| range.indices.len()).sum();
    mesh.batches.ceiling_batch = super::BatchRange::default();
    if neutral {
        for range in &mut mesh.ranges {
            illuminate_preview_vertices(&mut range.vertices);
        }
        for batch in &mut batches {
            illuminate_preview_vertices(&mut batch.vertices);
        }
    }
    LevelBuild {
        mesh,
        batches,
        static_light_off: None,
        lighting,
        timings,
        lightmaps: None,
        probes: None,
        lightmap_failure: None,
        lightmap_millis: 0.0,
    }
}

fn illuminate_preview_vertices(vertices: &mut [super::Vertex]) {
    let direction = glam::Vec3::new(-0.35, 0.8, 0.45).normalize();
    for vertex in vertices {
        let shade = 0.4_f32.mul_add(
            glam::Vec3::from_array(vertex.normal)
                .dot(direction)
                .max(0.0),
            0.55,
        );
        for (channel, albedo) in vertex.color.iter_mut().zip(vertex.caustics_albedo) {
            *channel = albedo * shade;
        }
    }
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
    pub transport: Arc<crate::engine::lighting::transport::TransportScene>,
    /// Tap counts, bounce count and sample budget.
    pub options: crate::engine::lighting::transport::SolveOptions,
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

/// Fills one request synchronously, retaining its prepared probe field.
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

/// Fills one request on the worker, with cancellation polled by the
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
/// [`crate::engine::perf::set_prepare_workers`], so one command never exceeds its one
/// shared budget.
pub fn set_fill_workers(workers: usize) {
    FILL_WORKERS.store(workers.clamp(1, MAX_TRANSPORT_WORKERS), Ordering::Relaxed);
    crate::engine::perf::set_prepare_workers(workers);
}

/// Inline bakes remain serial. The runtime loader owns the
/// only outer preparation worker; leave one logical CPU for UI/event handling.
fn runtime_fill_workers() -> usize {
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
        crate::engine::logging::warn(
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
    let options = crate::engine::lighting::transport::SolveOptions {
        workers,
        ..request.options
    };
    crate::engine::logging::info(format_args!(
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
    crate::engine::logging::info(format_args!(
        "[lightmap-work] triangles={} texels={} direct_rays={} bounce_rays={} cache_cells={}",
        request.transport.triangle_count(),
        solution
            .charts
            .iter()
            .map(|chart| chart.texels.len())
            .fold(0usize, usize::saturating_add),
        solution.direct_rays,
        solution.bounce_rays,
        solution.cache_cells
    ));
    let atlas_started = std::time::Instant::now();
    crate::engine::progress::stage("assembling lightmap atlases");
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
    crate::engine::logging::info(format_args!(
        "[lightmap-timing] atlas_assembly_ms={:.3}",
        elapsed_millis(atlas_started)
    ));
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
#[expect(
    clippy::too_many_lines,
    reason = "one cohesive prepare-and-decide pass"
)] // one cohesive prepare-and-decide pass
pub fn prepare_level_geometry_with_lightmaps(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
    mut options: LightmapBuildOptions,
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
    options = options.with_prepared_page_budget(level, &lighting);

    let surfaces = LevelSurfaces::new(level);
    let props_started = std::time::Instant::now();
    let (mut batches, fallbacks) = if options.mode == LightmapMode::On {
        resolve_prop_fallbacks(level, catalog, assets, &lighting, &surfaces, materials)
    } else {
        resolve_prop_instances(level, catalog, assets, &lighting, &surfaces, materials)
    };
    let mut props_millis = elapsed_millis(props_started);

    let mut plan = (options.mode == LightmapMode::On).then(|| LightmapPlan::new(options.config));
    let surfaces_started = std::time::Instant::now();
    let mesh = build_mesh_for_options(
        level,
        catalog,
        &fallbacks,
        &lighting,
        materials,
        plan.as_mut(),
        &batches,
    );
    let surfaces_millis = elapsed_millis(surfaces_started);
    if let Some(active_plan) = plan.as_mut() {
        let receivers_started = std::time::Instant::now();
        let (receivers, _) = resolve_prop_instances_lightmapped(
            level,
            catalog,
            assets,
            &lighting,
            &surfaces,
            materials,
            active_plan,
        );
        if !active_plan.failed() {
            batches = receivers;
        }
        props_millis += elapsed_millis(receivers_started);
    }

    let mut build = LevelBuild {
        static_light_off: None,
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
    if let Some(active_plan) = plan.as_ref() {
        // Report invisible slivers the plan skipped: the level kept its whole
        // lightmap, but the author should still know the geometry is there.
        let slivers = active_plan.slivers_skipped();
        if slivers > 0 {
            crate::engine::logging::warn_once(
                format!("lightmap-slivers:{}", level.id),
                format!(
                    "[lightmaps] '{}' left {} sub-texel sliver quad(s) vertex-lit",
                    level.id, slivers
                ),
            );
        }
        if let Some(plan_failure) = active_plan.failure() {
            build.lightmap_failure = Some(plan_failure);
        } else {
            // This transient key covers source/settings/solver inputs, not
            // resolved catalog, PNG or GLB bytes. A caller retaining this cache
            // must keep those inputs immutable or clear it after an asset edit.
            // The compiler starts a fresh cache per build; its package/stage
            // fingerprints cover resolved resources. The player consumes those
            // prepared packages and verifies installed dependency hashes.
            let key = lightmap_content_key(level, &build.lighting, options);
            if let Some(cached) = cache.and_then(|lightmap_cache| lightmap_cache.get(&key)) {
                build.lightmaps = Some(cached);
            } else {
                let scene_started = std::time::Instant::now();
                match super::light_transport::build_transport_scene(
                    level,
                    &build.mesh,
                    &build.batches,
                    materials,
                    &build.lighting,
                    active_plan.charts(),
                )
                .and_then(|(scene, stats)| {
                    scene
                        .with_chart_sample_density(active_plan.sample_densities().to_vec())
                        .map(|sampled_scene| (sampled_scene, stats))
                }) {
                    Some((transport, scene_stats)) => {
                        crate::engine::logging::info(format_args!(
                            "[lightmaps] transport scene triangles={} ({} skipped) emitters={} ({} switchable)",
                            scene_stats.triangles,
                            scene_stats.skipped_triangles,
                            scene_stats.emitters,
                            scene_stats.switchable_emitters
                        ));
                        fill = Some(LightmapFillRequest {
                            config: options.config,
                            charts: active_plan.charts().to_vec(),
                            page_count: active_plan.page_count(),
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
                crate::engine::logging::info(format_args!(
                    "[lightmap-timing] transport_scene_and_targets_ms={:.3}",
                    elapsed_millis(scene_started)
                ));
            }
        }
    }

    // A failed plan must never be drawn: rebuild the static mesh with the
    // historical vertex path against the lighting already baked above. This is
    // the exact fallback the inline entry point always ran, and it does not
    // re-bake the lighting (which would change every vertex colour).
    if build.lightmap_failure.is_some() && options.mode == LightmapMode::On {
        let fallback_started = std::time::Instant::now();
        (build.batches, _) = resolve_prop_instances(
            level,
            catalog,
            assets,
            &build.lighting,
            &surfaces,
            materials,
        );
        build.mesh = build_level_geometry_mesh_with_fixture_models(
            level,
            catalog,
            &fallbacks,
            &build.lighting,
            materials,
            None,
            &build.batches,
        );
        build.timings.surfaces_millis += elapsed_millis(fallback_started);
    }

    PreparedLightmapBuild { build, fill }
}

/// Source/settings identity for a transient atlas cache and compiler metadata.
///
/// Resolved catalog, image and model bytes belong to the compiler's package
/// identity, not this helper. Manual persistent-cache callers must retain
/// immutable resolved inputs or clear the cache when those resources change.
/// A decoded prepared lighting record lets the compiler refresh this metadata
/// after a display/navigation edit without repeating transport.
#[must_use]
pub fn lightmap_content_key(
    level: &LevelDef,
    lighting: &LevelLighting,
    mut options: LightmapBuildOptions,
) -> String {
    options = options.with_prepared_page_budget(level, lighting);
    let mut extra = Vec::with_capacity(32);
    extra.extend_from_slice(&lighting.occlusion_fingerprint().to_le_bytes());
    extra.push(options.bake.sampling.taps_per_axis);
    extra.extend_from_slice(&options.bake.prop_occlusion_cell_m.to_bits().to_le_bytes());
    extra.extend_from_slice(&crate::engine::lighting::model_fingerprint().to_le_bytes());
    extra
        .extend_from_slice(&crate::engine::lighting::transport::solver_fingerprint().to_le_bytes());
    extra.push(options.solve.taps_per_axis);
    extra.push(options.solve.bounces);
    extra.extend_from_slice(
        &u32::try_from(options.solve.gather_samples)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    content_key_with_extra(level, &options.config, options.profile, &extra)
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
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
    lighting: &LevelLighting,
    batches: &mut Vec<PropMeshBatch>,
) -> LevelMesh {
    let surfaces = LevelSurfaces::new(level);
    let (legacy, fallbacks) =
        resolve_prop_instances(level, catalog, assets, lighting, &surfaces, materials);
    *batches = legacy;
    build_level_geometry_mesh_with_fixture_models(
        level, catalog, &fallbacks, lighting, materials, None, batches,
    )
}

/// [`prepare_level_geometry_with_lightmaps`] finished inline.
///
/// This is the historical synchronous entry point: it prepares the build and
/// fills any owed lightmaps on the calling thread, so a plan or fill failure
/// degrades to the exact vertex-lit level. The renderer's runtime changes use
/// the two halves separately and fill on a worker; both share
/// the same request bake, so their pages cannot drift.
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
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
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
        if options.mode == LightmapMode::On && build.lightmap_failure.is_none() {
            super::props::relight_probe_props(&mut build.batches, &build.lighting, None);
        }
        return build;
    };
    match fill_lightmaps_full(&request) {
        Ok(product) => {
            let crate::engine::render::common::api::LightmapFillProduct { lightmaps, probes } =
                product;
            super::props::relight_probe_props(&mut build.batches, &build.lighting, probes.as_ref());
            let shared_lightmaps = Arc::new(lightmaps);
            build.lightmap_millis = shared_lightmaps.stats.bake_millis;
            dump_lightmaps_for_level(level, &shared_lightmaps);
            if let Some(lightmap_cache) = cache {
                lightmap_cache.insert(&request.content_key, Arc::clone(&shared_lightmaps));
            }
            // The prepared solve lights the surfaces; blended feather decals
            // must read with it instead of the vertex-lit approximation (see
            // `relight_blend_decals`). Cut-out decals are untouched.
            if let Some(field) = probes.as_ref() {
                crate::engine::render::relight_blend_decals(
                    &mut build.mesh,
                    level,
                    catalog.assets(),
                    &build.lighting,
                    field,
                );
            }
            build.lightmaps = Some(shared_lightmaps);
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
    catalog: &crate::engine::loader::PropCatalog,
    fallbacks: &[&PropDef],
    lighting: &LevelLighting,
    materials: &MaterialTable,
    plan: Option<&mut LightmapPlan>,
    batches: &[PropMeshBatch],
) -> LevelMesh {
    build_level_geometry_mesh_with_fixture_models(
        level, catalog, fallbacks, lighting, materials, plan, batches,
    )
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
    let level_id = if id.is_empty() {
        "level".to_string()
    } else {
        id
    };
    let Ok(paths) = crate::engine::paths::user() else {
        return;
    };
    let dir = paths.cache.join("diagnostics/atlases");
    for (index, page) in lightmaps.pages.iter().enumerate() {
        let path = dir.join(format!("{level_id}_page{index}.png"));
        if let Err(error) = write_page_png(page, &path) {
            crate::engine::logging::warn_once(
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
pub fn shipped_asset_catalog() -> &'static crate::engine::assets::AssetCatalog {
    static CATALOG: std::sync::OnceLock<crate::engine::assets::AssetCatalog> =
        std::sync::OnceLock::new();
    CATALOG.get_or_init(crate::engine::assets::AssetCatalog::load_default)
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
    build_level_geometry_with_catalog(level, &crate::engine::loader::PropCatalog::builtin())
}

/// Builds level geometry using the shipped catalog's logical materials.
#[must_use]
pub fn build_level_geometry_with_materials(
    level: &LevelDef,
    materials: &MaterialTable,
) -> LevelMesh {
    build_level_geometry_with_catalog_and_materials(
        level,
        &crate::engine::loader::PropCatalog::builtin(),
        materials,
    )
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
#[must_use]
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
) -> LevelMesh {
    let materials = logical_materials(level);
    build_level_geometry_with_catalog_and_materials(level, catalog, &materials)
}

/// [`build_level_geometry_with_catalog`] with an explicitly resolved material
/// table.
#[must_use]
pub fn build_level_geometry_with_catalog_and_materials(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
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

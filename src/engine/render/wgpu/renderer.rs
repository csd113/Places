//! The wgpu renderer: device/surface lifecycle and the complete frame.
//!
//! The renderer owns the GPU side of one level: the device, queue and surface
//! lifecycle, the static world vertex/index buffers and render pipelines, the
//! texture and material caches, the lightmap atlas, the reflection targets,
//! the props and dynamic meshes, the decal set, the post-processing chain and
//! the HUD. It never bakes lighting or builds geometry; it uploads and draws
//! what the renderer-neutral `render::common` layer produced.
//!
//! The lifecycle handles four things together:
//!
//! 1. SDL hosts a wgpu surface on the intended native desktop backend;
//! 2. adapter/device creation and surface configuration are verified against
//!    that backend;
//! 3. the surface lifecycle (resize, minimize, restore, surface loss, device
//!    loss) recovers without leaking GPU objects;
//! 4. the engine sees only the `render::Renderer` facade and never learns any
//!    wgpu vocabulary.
//!
//! The world path uploads level geometry into persistent GPU buffers and draws
//! it from the shared Places camera with depth testing and indexed draws; the
//! material path resolves each draw's state, binds normal maps and runs the
//! opaque, cut-out and translucent passes with their alpha states and depth
//! ordering, with WGSL assembling linear HDR material colour and
//! preparing the material normal. The surface/device path is independent of
//! that content.
//!
//! Everything wgpu-specific lives under `render::wgpu`; the engine reaches it
//! only through `render::Renderer`. The backend draws the complete frame
//! (lightmaps, reflections, props, dynamics, fixtures, emission, decals, fog,
//! post-processing and the HUD).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sdl3::video::Window;

use super::character::{CharacterUploadContext, WgpuCharacters};
use super::decals::{DecalPipeline, WgpuDecals};
use super::dynamic::{DynamicUploadContext, WgpuDynamic};
use super::effects::{EffectsPipeline, WgpuEffects};
use super::environment::{
    EnvironmentBindings, fallback_planar, fallback_probe, static_environment,
};
use super::gpu_timing::{self, GpuTimer};
use super::lightmap::{LightmapAtlas, needs_upload_fallback};
use super::material::{WorldMaterialInputs, WorldMaterials, material_bind_group_layout};
use super::postprocess::{EMISSIVE_FORMAT, PRESENTED_FORMAT, PostProcess, SCENE_FORMAT};
use super::props::{PropUpload, WgpuProps};
use super::readback::{read_back_cube_face, read_back_rgba};
use super::reflections::{
    CaptureFrame, ProbeCube, ReflectionTargets, planar_size_for, planar_view_projection,
    probe_bake_position, probe_face_size, probe_face_view_projection, probe_mip_edge,
};
use super::sky::{SkyPipeline, WgpuSky};
use super::surface::{
    self, CLEAR_COLOR, CLEAR_COLOR_SRGB, DEPTH_FORMAT, SurfaceRecovery, SurfaceStatus,
    present_mode_label,
};
use super::texture::{TextureCache, TextureFiltering};
use super::ui::{UiRenderer, UiTiming};
use super::world::{
    EnvironmentUniform, WgpuWorldGeometry, WorldDrawTotals, WorldPipeline, WorldTextures,
    environment_bind_group_layout, prepare_world_frame,
};
use crate::engine::lighting::lightmap::{LightmapFailure, LightmapMode};
use crate::engine::loader::LoadedLevel;
use crate::engine::logging;
use crate::engine::quality::{
    GraphicsSpec, LightmapQuality, QualityLevel, ReflectionQuality, TextureClass,
};
use crate::engine::render::RenderCamera;
use crate::engine::render::common::SurfaceKind;
use crate::engine::render::common::Vertex;
use crate::engine::render::common::animation::{AnimationEffect, EmissionAnimation};
use crate::engine::render::common::api::{
    GraphicsTransition, LevelBuild, LightmapBuildOptions, build_level_geometry_timed_with_lightmaps,
};
use crate::engine::render::common::atmosphere::LevelFog;
use crate::engine::render::common::character::{CharacterScene, EntityFrame};
use crate::engine::render::common::dynamic::{
    DynamicId, DynamicObject, DynamicScene, DynamicUpdate, SpawnOrientation,
};
use crate::engine::render::common::effects::EffectScene;
use crate::engine::render::common::materials::MaterialRenderState;
use crate::engine::render::common::postprocess::PostSettings;
use crate::engine::render::common::reflections::{
    Reflections, mirror_point, nearest_visible_reflection_plane, routing_from_mesh,
};
use crate::engine::render::common::stats::{GpuTiming, LevelBuildStats, RenderStats};
use crate::engine::render::common::view::DrawableSize;
use crate::engine::scene::LocomotionSnapshot;

/// Three warm steady frames only; the explicit diagnostic is excluded from timing.
fn cpu_trace_start(seconds: f32) -> Option<std::time::Instant> {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = ENABLED.get_or_init(|| {
        std::env::var_os("PLACES_BENCH").is_some()
            && std::env::var_os("PLACES_BENCH_CPU_TRACE").is_some()
    });
    (*enabled && (3.0..3.05).contains(&seconds)).then(std::time::Instant::now)
}

fn cpu_trace_finish(stage: &str, started: Option<std::time::Instant>) {
    if let Some(begin) = started {
        logging::info(format!(
            "[cpu-stage] {stage} {:.6} ms",
            begin.elapsed().as_secs_f64() * 1_000.0_f64
        ));
    }
}

/// Fixed scalar storage for explicit native resource-lifetime diagnostics.
/// Peaks describe sampled observations, including staged old/new worlds; they
/// do not claim to measure an unsampled driver allocation high-water mark.
#[cfg(feature = "visual-diagnostics")]
struct GpuResourceTrace {
    frame: u64,
    peak: [isize; 16],
}

#[cfg(feature = "visual-diagnostics")]
impl GpuResourceTrace {
    fn requested() -> Option<Self> {
        std::env::var("PLACES_GPU_RESOURCE_TRACE")
            .is_ok_and(|value| value.trim() == "1")
            .then_some(Self {
                frame: 0,
                peak: [0; 16],
            })
    }
}

/// Where one installed door stands: its hinge and closed-leaf yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DoorPlacement {
    /// World position of the leaf's bottom hinge edge.
    hinge: [f32; 3],
    /// Yaw of the closed leaf, in degrees.
    yaw_degrees: f32,
}

/// One switchable fixture's runtime bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SwitchableLight {
    /// The fixture's position in the level's `ceiling_lights` array.
    fixture_index: usize,
    /// The light it baked into.
    light_index: usize,
    /// The material slot of its luminous face, for the on/off emission write.
    material_slot: Option<usize>,
    /// Static cannot select prepared illumination; opt-in fixtures retain
    /// the state their vertex-lit surfaces were baked with.
    hold_in_static: bool,
    baked_enabled: bool,
}

impl SwitchableLight {
    const fn enabled_for_mode(self, requested: bool, prepared_illumination: bool) -> bool {
        if self.hold_in_static && !prepared_illumination {
            self.baked_enabled
        } else {
            requested
        }
    }
}

/// One fixed prop's sources share a prepared layer and luminous material state.
#[derive(Clone, Debug)]
struct PropLightBinding {
    controller: usize,
    sources: Vec<usize>,
    baked_enabled: bool,
}

/// The dynamic object handles one door draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DoorDraw {
    /// The static frame's object, spawned once per install.
    frame: Option<DynamicId>,
    /// The moving leaf's object, transformed every frame.
    leaf: Option<DynamicId>,
}

/// The size-dependent depth attachment for the current drawable.
///
/// The texture is kept alongside its view: the view borrows GPU state, and
/// keeping the pair together makes the "recreated with the surface" ownership
/// obvious.
struct DepthTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The colour format a `PLACES_CAPTURE` read-back renders into.
///
/// The capture target must be the format the world pipelines were built for,
/// which is the configured surface format. RGBA byte order is restored on the
/// CPU after mapping, so a `Bgra8UnormSrgb` surface still produces the
/// capture contract's RGBA byte order.
const fn capture_format(surface_format: wgpu::TextureFormat) -> wgpu::TextureFormat {
    surface_format
}

/// The device limits the world pipelines need.
///
/// The world pipeline layout binds groups 0..=4 — group 4 is the frame's
/// attached-light uniform — which is one more than WebGPU's default
/// `maxBindGroups` of 4; the renderer asks for exactly what it binds. Every
/// device the world pipeline is created on goes through this request.
pub(super) fn world_device_limits() -> wgpu::Limits {
    wgpu::Limits {
        max_bind_groups: 5,
        ..wgpu::Limits::default()
    }
}

/// Physical pixels must fit every size-dependent GPU attachment. Empty windows
/// remain a normal minimized state; failure stops before any allocation.
fn validate_drawable(size: DrawableSize, maximum: u32) -> Result<(), String> {
    if size.is_empty() || (size.width <= maximum && size.height <= maximum) {
        return Ok(());
    }
    Err(format!(
        "The window drawable is {}x{} pixels; this GPU device supports at most {maximum} pixels per edge. Choose a smaller window resolution and restart Places.",
        size.width, size.height
    ))
}

/// Resources for a new world stay separate until every upload is ready.
struct PreparedInstall {
    loaded: LoadedLevel,
    build: Arc<LevelBuild>,
    assets: crate::engine::props::PropAssets,
    characters: CharacterScene,
    preserve_playback: bool,
    textures: TextureCache,
    graphics: GraphicsConfig,
    materials: MaterialRenderState,
    blacklights: crate::engine::render::common::blacklight::BlacklightSet,
    uv_transport: Option<Arc<crate::engine::lighting::transport::TransportScene>>,
    atlas: Option<LightmapAtlas>,
    fixture_sheets: Vec<Arc<super::texture::GpuTexture>>,
    world: Option<WgpuWorldGeometry>,
    world_textures: Option<WorldTextures>,
    world_materials: Option<WorldMaterials>,
    props: Option<PropUpload>,
    prop_cursor: usize,
    phase: UploadPhase,
    /// Packaged reflection captures, when the world was prepared offline.
    probes: crate::engine::package::world::ProbeCaptures,
    /// True when `probes` must be uploaded instead of baking captures.
    precompiled_probes: bool,
}

/// Texture/filtering/reflection changes cannot alter immutable atlas data.
/// New or absent CPU resources, including reload replacements, require upload.
fn resident_atlas_matches(
    resident: bool,
    current: Option<&Arc<crate::engine::lighting::lightmap::LevelLightmaps>>,
    next: Option<&Arc<crate::engine::lighting::lightmap::LevelLightmaps>>,
) -> bool {
    resident
        && current
            .zip(next)
            .is_some_and(|(installed, prepared)| Arc::ptr_eq(installed, prepared))
}

#[derive(Clone, Copy)]
enum UploadPhase {
    Atlas,
    World,
    Textures,
    Materials,
    Props,
}

struct UploadedLevel {
    atlas: LightmapAtlas,
    fixture_sheets: Vec<Arc<super::texture::GpuTexture>>,
    world: WgpuWorldGeometry,
    world_textures: WorldTextures,
    world_materials: WorldMaterials,
    props: WgpuProps,
}

/// The inactive exact Static state; toggling swaps these already-uploaded banks.
struct StaticLightBank {
    fixtures: Vec<usize>,
    off_active: bool,
    world: Option<WgpuWorldGeometry>,
    props: Option<WgpuProps>,
    textures: Option<WorldTextures>,
    materials: Option<WorldMaterials>,
    decals: Option<WgpuDecals>,
}

/// What one acquisition attempt produced.
enum Acquired {
    /// A surface texture is ready for the frame's render pass.
    Frame(wgpu::SurfaceTexture),
    /// No texture this frame; the surface is healthy and the frame is skipped.
    Skip,
}

/// The player-facing graphics configuration a settings action asks for.
///
/// Setters record the requested configuration. Frame-only changes apply
/// immediately; resource changes become applied when preparation commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GraphicsConfig {
    /// Overall quality: texture budgets and scene target.
    quality: QualityLevel,
    /// Lighting response, independent of texture budgets.
    lighting_quality: QualityLevel,
    /// Ordinary world texture filtering (a sampler-handle swap at bind time).
    filtering: TextureFiltering,
    /// Whether the emissive/bloom chain runs.
    bloom: bool,
    /// Whether a baked lightmap atlas exists and how dense it is.
    lightmaps: LightmapQuality,
    /// Whether probe cubemaps and the planar mirror exist.
    reflections: ReflectionQuality,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            quality: QualityLevel::default(),
            lighting_quality: QualityLevel::default(),
            filtering: TextureFiltering::DEFAULT,
            bloom: true,
            lightmaps: LightmapQuality::default(),
            reflections: ReflectionQuality::default(),
        }
    }
}

impl GraphicsConfig {
    #[cfg(feature = "visual-diagnostics")]
    fn diagnostic_state(self) -> serde_json::Value {
        serde_json::json!({
            "quality": self.quality.name(), "lighting": self.lighting_quality.name(),
            "filtering": self.filtering.name(), "bloom": self.bloom,
            "lightmaps": self.lightmaps.name(), "reflections": self.reflections.name(),
        })
    }

    /// What changed between the applied configuration and `next`.
    fn delta_from(self, next: Self) -> GraphicsDelta {
        GraphicsDelta {
            quality: self.quality != next.quality,
            lighting: self.lighting_quality != next.lighting_quality,
            filtering: self.filtering != next.filtering,
            bloom: self.bloom != next.bloom,
            lightmaps: self.lightmaps != next.lightmaps,
            reflections: self.reflections != next.reflections,
        }
    }

    /// The engine-neutral projection the game loop compares.
    ///
    /// The filtering preset crosses the seam as its canonical name so the
    /// settings layer and the renderer never share an internal type; equality
    /// is still exact because both sides canonicalise through the same three
    /// names.
    const fn spec(self) -> GraphicsSpec {
        GraphicsSpec {
            quality: self.quality,
            lighting: self.lighting_quality,
            filtering: self.filtering.name(),
            bloom: self.bloom,
            lightmaps: self.lightmaps,
            reflections: self.reflections,
        }
    }
}

/// Which graphics settings changed between the applied state and a request.
///
/// The flags mirror the work each setting owes, which is not the same for all
/// of them: filtering and bloom are frame gates and owe no resource work at
/// all, a quality change re-fits the retained CPU build's textures, and only a
/// lightmap change needs the expensive CPU build. The whole file reads the
/// delta instead of comparing settings again, so a change is classified once.
///
/// One bool per player setting is deliberate: they are independent flags, not
/// a state machine with one active variant (the same reasoning as
/// [`WgpuRenderer`]'s own boolean group).
#[expect(
    clippy::struct_excessive_bools,
    reason = "These flags independently track graphics preferences and surface lifecycle conditions; several can be active together."
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GraphicsDelta {
    /// Texture budgets and scene target.
    quality: bool,
    /// Optional material response.
    lighting: bool,
    /// The world sampler preset (recorded only).
    filtering: bool,
    /// The bloom gate (recorded only).
    bloom: bool,
    /// The atlas exists/density: the one flag that needs a CPU build.
    lightmaps: bool,
    /// Probe cubemaps and the planar gate.
    reflections: bool,
}

impl GraphicsDelta {
    /// True when any GPU resource work is owed.
    ///
    /// Filtering is a sampler handle the bind groups swap at bind time, and
    /// bloom is a per-frame post gate, so neither ever rebuilds a resource:
    /// this is the predicate that proves a filtering-only or bloom-only change
    /// is zero-work.
    const fn needs_gpu_work(self) -> bool {
        self.quality || self.lighting || self.lightmaps || self.reflections
    }
}

/// Resolved level emission curves. Fixture sheets and ordinary materials use
/// separate namespaces even when their numeric indices happen to coincide.
#[derive(Default)]
struct LevelEmissionAnimations {
    materials: Vec<Option<EmissionAnimation>>,
    fixtures: Vec<Option<EmissionAnimation>>,
}

/// The wgpu implementation of the renderer facade.
///
/// It owns the device/surface lifecycle and the world material passes.
/// Field order is drop order: the surface (and its swapchain resources) drops
/// before the device and the instance. A retained SDL window clone drops last,
/// keeping the raw handles valid through surface destruction and recovery.
///
/// The flags below are independent pieces of renderer state — whether the
/// surface needs reconfiguring, the player's `VSync` preference, whether the
/// surface was reported lost, and whether the CPU frustum test runs — not a
/// state machine with one active variant, so an enum per flag would be worse.
#[expect(
    clippy::struct_excessive_bools,
    reason = "These flags independently track graphics preferences and surface lifecycle conditions; several can be active together."
)]
pub struct WgpuRenderer {
    /// The frame acquired between `render_scene` and `present`.
    ///
    /// Declared before the surface on purpose: an unpresented frame is a
    /// `SurfaceTexture` that must be discarded *before* the surface it came
    /// from is destroyed (its drop reaches back into the surface's swapchain).
    /// Rust drops fields in declaration order, so this releases the frame first
    /// on an unclean shutdown (`PLACES_BENCH_NOSWAP`, a mid-frame fatal error).
    pending_frame: Option<wgpu::SurfaceTexture>,
    surface: Option<wgpu::Surface<'static>>,
    capabilities: Option<wgpu::SurfaceCapabilities>,
    config: wgpu::SurfaceConfiguration,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    instance: wgpu::Instance,
    depth: Option<DepthTarget>,
    depth_size: DrawableSize,
    drawable_size: DrawableSize,
    /// The world pipelines and camera binding.
    ///
    /// Built for the configured surface format and kept; the world upload keeps
    /// the buffers. Both drop after the device, like the depth target: wgpu
    /// resources keep the device alive internally, and the surface (which must
    /// precede the device) already has the ordering constraint that matters.
    world_pipeline: Option<WorldPipeline>,
    flashlight: Option<crate::engine::render::common::Flashlight>,
    flashlight_shadow: Option<super::flashlight::FlashlightShadow>,
    flashlight_characters: std::cell::RefCell<Vec<super::flashlight::CharacterVisibility>>,
    /// The static world mesh uploaded for the loaded level, if any.
    world: Option<WgpuWorldGeometry>,
    /// The loaded level's uploaded prop batches, if any.
    world_props: Option<WgpuProps>,
    static_light_bank: Option<StaticLightBank>,
    /// The loaded level's per-draw GPU textures, resolved at upload.
    world_textures: Option<WorldTextures>,
    /// The loaded level's per-draw GPU materials, resolved at upload.
    world_materials: Option<WorldMaterials>,
    /// The group-2 bind group layout: material parameters, normal map and its
    /// sampler. Created once with the device; shared by every world pipeline
    /// rebuild and every cached material bind group.
    material_layout: wgpu::BindGroupLayout,
    /// The group-3 environment bind group layout: the baked-light switch and
    /// scale, fog, the lightmap pages, the probe cubemap and the planar mirror.
    /// Created once with the device.
    environment_layout: wgpu::BindGroupLayout,
    caustics: super::caustics::Caustics,
    /// The renderer-owned texture cache: layout, shared samplers, fallback and
    /// every uploaded texture. Renderer lifetime; pack entries are dropped per
    /// level.
    textures: TextureCache,
    /// The player's filtering setting, selecting the world's shared sampler at
    /// bind time (the lightmap sampler never follows it).
    filtering: TextureFiltering,
    /// Prop model cache used only to build the neutral level geometry.
    prop_catalog: crate::engine::loader::PropCatalog,
    prop_assets: crate::engine::props::PropAssets,
    /// The graphics configuration the setters have recorded (requested).
    ///
    /// Resource preparation snapshots this configuration for its commit.
    graphics_requested: GraphicsConfig,
    /// The graphics configuration whose resources are resident.
    graphics_applied: GraphicsConfig,
    /// The quality level the resident GPU textures were fitted at.
    ///
    /// Compared against the requested level at install time to decide whether
    /// the texture caches must be released; a lightmap-only rebuild at the same
    /// quality reuses every cached texture.
    installed_quality: QualityLevel,
    /// Keeps the active CPU build available to the loader through its weak identity.
    retained_build: Option<Arc<LevelBuild>>,
    prepared_install: Option<PreparedInstall>,
    /// The Lightmaps setting of the resident world.
    installed_lightmaps: LightmapQuality,
    /// Whether the loaded level's atlas uploaded and the shader should sample
    /// it.
    lightmaps_resident: bool,
    /// The loaded level's lightmap atlas pages; an empty atlas (with its white
    /// fallback) before the first level upload and whenever the vertex-lit
    /// fallback is active.
    lightmaps: LightmapAtlas,
    /// The level's decal batches, or `None` before the first upload.
    decals: Option<WgpuDecals>,
    /// The decal pass pipeline for the surface format.
    decal_pipeline: Option<DecalPipeline>,
    /// The decal pass pipeline for the reflection capture format.
    decal_reflection_pipeline: Option<DecalPipeline>,
    /// The renderer-owned UI pass (the 480x272 HUD).
    ui: Option<UiRenderer>,
    /// The UI vertex list of the last `render_ui`, replayed by the one-shot
    /// capture so it includes the HUD the window showed. Capacity is reused
    /// across frames.
    last_ui: Vec<Vertex>,
    /// The offscreen scene target, emissive pass, bloom blur and resolve.
    post: Option<PostProcess>,
    /// The world pipeline set for the offscreen scene format (culling as the
    /// main pass).
    scene_pipeline: Option<WorldPipeline>,
    /// The world pipeline set for the raw emissive format.
    emissive_pipeline: Option<WorldPipeline>,
    /// The decal pipeline for the offscreen scene format.
    decal_scene_pipeline: Option<DecalPipeline>,
    /// The level's uploaded sky, or `None` for a level that declares none.
    sky: Option<WgpuSky>,
    /// The sky pass's pipeline pair, one per colour target convention.
    sky_pipeline: Option<SkyPipeline>,
    /// The frame/level environment bindings for the loaded level.
    environment: Option<EnvironmentBindings>,
    /// The 1x1 black cube bound while no probe is resident (renderer lifetime).
    _probe_fallback: wgpu::Texture,
    probe_fallback_view: wgpu::TextureView,
    /// The 1x1 white planar image bound while no mirror is active.
    _planar_fallback: wgpu::Texture,
    planar_fallback_view: wgpu::TextureView,
    /// One GPU sheet per fixture family for the loaded level, indexed by
    /// [`crate::engine::lighting::FixtureKind::index`]; the white fallback for a family
    /// the level does not use.
    fixture_sheets: Vec<std::sync::Arc<super::texture::GpuTexture>>,
    /// The loaded level's separate material and fixture-face animations.
    material_animations: LevelEmissionAnimations,
    /// Seconds of emission-animation time accumulated since the level load.
    animation_seconds: f32,
    /// Invalidates centre samples when immutable lighting resources or switches change.
    entity_lighting_revision: u64,
    /// The level's atmosphere (global fog plus any regional volumes), applied
    /// by every world draw from the shared group-3 uniform.
    fog: LevelFog,
    /// Whether the player asked for bloom. The emissive/bloom pass gates on it;
    /// recorded here so `set_bloom_enabled` can be applied without a rebuild.
    bloom_requested: bool,
    #[cfg(feature = "visual-diagnostics")]
    visual_diagnostic: super::diagnostics::VisualDiagnosticMode,
    /// Whether the player asked for reflections. The probe and planar
    /// resources live in `render::wgpu::reflections`; the flag is recorded
    /// here and gates the environment's reflection sampling.
    reflections_enabled: bool,
    /// The level's reflection routing and gates (neutral data).
    reflections: Reflections,
    /// Packaged reflection captures for the installed world, when it came from
    /// a compiled package.
    packaged_probes: Option<crate::engine::package::world::ProbeCaptures>,
    /// The level's probe cubemaps and planar target.
    reflection_targets: ReflectionTargets,
    /// The world pipeline set both reflection captures run through: no
    /// culling, reversed front face (the probe projection's Y flip and the
    /// planar mirror both reverse winding), the reflection colour
    /// format.
    capture_pipeline: Option<WorldPipeline>,
    /// The mirror plane selected for the last rendered frame, if any.
    active_plane: Option<usize>,
    /// The nearest probe cubemap selected for the last rendered frame, as an
    /// index into the level's probe list.
    active_probe: usize,
    /// The top mip level of the resident probe chain, from the packaged
    /// payload (0 while live captures carry level 0 only). Packed into the
    /// environment uniform so the shader's roughness selection never samples a
    /// level that does not exist.
    probe_max_mip: u32,
    /// Capture passes submitted since the level loaded, for the neutral stats.
    reflection_passes: usize,
    /// Whether the CPU frustum test is applied to the world draw ranges.
    culling: bool,
    /// The active texture and scene-target quality. Lighting response has its
    /// own selection so Low lighting retains the chosen texture budgets.
    quality: QualityLevel,
    /// Lighting path selected independently of texture and scene budgets.
    lighting_quality: QualityLevel,
    /// The last level upload's counters.
    level_stats: LevelBuildStats,
    /// The last submitted frame's counters.
    render_stats: RenderStats,
    /// Persistent asynchronous render-pass timestamp queries/readback slots.
    gpu_timer: GpuTimer,
    #[cfg(feature = "visual-diagnostics")]
    gpu_resource_trace: Option<GpuResourceTrace>,
    /// The surface must be (re)configured before the next acquisition.
    needs_configure: bool,
    /// The player's `VSync` preference, which selects the presentation mode.
    vsync: bool,
    /// A frame hop reported the surface lost; recreate it on the next present
    /// (which owns the SDL window reference).
    surface_lost: bool,
    /// Set once the renderer must stop issuing GPU work.
    fatal: Option<String>,
    /// Filled by wgpu's device-lost callback on whichever thread reports it.
    device_lost: Arc<Mutex<Option<String>>>,
    /// The neutral dynamic scene. The engine spawns its objects through
    /// `set_dynamic_demo` and the level's doors through `spawn_doors`; the GPU
    /// side lives in `world_dynamic`.
    dynamic: DynamicScene,
    /// The engine's runtime spawn keys and the live dynamic object each one
    /// holds, so a key can replace, move, re-material and despawn its object
    /// without the engine ever naming a [`DynamicId`]. Cleared wherever the
    /// neutral scene is wiped (a level change or a demo respawn), because the
    /// objects it maps are gone with it.
    runtime_objects: HashMap<u64, DynamicId>,
    /// The installed level's doors: their placement and code-built models.
    /// Built at level install, consumed by [`Self::spawn_doors`] whenever the
    /// neutral scene is cleared (a level change or a demo respawn).
    door_sources: Vec<(
        DoorPlacement,
        crate::engine::render::common::doors::DoorModels,
    )>,
    /// The live dynamic objects one door draws: its static frame and its leaf.
    door_objects: Vec<DoorDraw>,
    /// Every switchable fixture's runtime bindings: the baked light it drives
    /// and the material slot of its luminous face.
    switchable_lights: Vec<SwitchableLight>,
    prop_light_bindings: Vec<PropLightBinding>,
    /// Share immutable prepared pages with the retained build; light switches
    /// read only the page count and source identities, never mutate texels.
    lightmap_cpu: Option<Arc<crate::engine::lighting::lightmap::LevelLightmaps>>,
    /// The id of the level currently resident. Installing a different
    /// level clears the neutral dynamic scene; a quality rebuild of the same
    /// level keeps it, so its objects survive a quality change.
    level_id: Option<String>,
    /// The installed level definition, retained for the runtime spawn seams.
    /// A runtime character is spawned by instance id alone (see
    /// [`Self::spawn_runtime_character`]), so the renderer keeps the level the
    /// placement's walkable floor is resolved from; the caller never has to
    /// mirror the level into the call.
    installed_level: Option<crate::engine::level::LevelDef>,
    /// The GPU side of the dynamic scene: one mesh per model, one environment
    /// per object. Built when the demo spawns, dropped with the level.
    world_dynamic: Option<WgpuDynamic>,
    /// The installed level's ambient effect emitters. Built at level install
    /// from the level and its resolved material table; dropped when the level
    /// changes.
    effects: EffectScene,
    /// The GPU side of the effect scene: buffers sized to the level budget and
    /// one uploaded sheet per distinct effect material. Dropped with the
    /// level.
    world_effects: Option<WgpuEffects>,
    /// The effect billboard pipeline for the configured surface format.
    effects_pipeline: Option<EffectsPipeline>,
    /// The effect billboard pipeline for the offscreen scene format.
    effects_scene_pipeline: Option<EffectsPipeline>,
    /// The neutral character scene: every placed skinned prop, its animator
    /// and its baked per-vertex albedo, plus the gameplay's runtime-spawned
    /// actors. Rebuilt on level install and graphics changes; advanced by
    /// `update_characters`.
    characters: CharacterScene,
    /// The GPU side of the character scene: shared index buffers, one mutable
    /// skinned vertex buffer per character and one environment per character.
    world_characters: Option<WgpuCharacters>,
    /// The `CharacterScene::runtime_generation` the GPU side was last built
    /// from. A runtime spawn or despawn bumps the scene's generation;
    /// `update_characters` rebuilds `world_characters` when this differs, so
    /// the change is visible on the frame it happens.
    character_runtime_generation: u64,
    /// The level's CPU bake, kept for the dynamic objects' light probes. The
    /// static path needs it only while building; a moving object samples it as
    /// it moves.
    dynamic_lighting: Option<crate::engine::lighting::LevelLighting>,
    /// The prepared irradiance field moving objects and characters sample.
    dynamic_field: Option<std::sync::Arc<crate::engine::lighting::probes::ProbeField>>,
    /// Exact prepared triangles and alpha for moving-source visibility; no solve.
    dynamic_transport: Option<std::sync::Arc<crate::engine::lighting::transport::TransportScene>>,
    visibility_materials: Option<crate::engine::materials::MaterialTable>,
    blacklights: crate::engine::render::common::blacklight::BlacklightSet,
    dynamic_visibility: crate::engine::render::common::dynamic_visibility::DynamicVisibility,
    dynamic_visibility_error: Option<String>,
    #[cfg(feature = "visual-diagnostics")]
    probe_visualization: Option<WgpuWorldGeometry>,
    /// The frame state of the last submitted `render_scene`, so the one-shot
    /// capture can re-encode the same view. `None` until a frame has been
    /// rendered, and after a level or size change that invalidated the state.
    last_frame: Option<super::world::WorldFrame>,
    adapter_info: wgpu::AdapterInfo,
    /// Keeps the raw SDL handles alive until after the surface and GPU fields.
    /// Headless compilers retain no window. SDL's clone owns the same context.
    window: Option<Window>,
}

impl WgpuRenderer {
    /// Creates the wgpu backend for an existing SDL window.
    ///
    /// The window is a plain SDL3 window: the raw-window-handle implementation
    /// reports its content view and wgpu attaches its own Metal layer, so no
    /// backend-specific window flags are requested.
    ///
    /// # Errors
    ///
    /// Returns an actionable message when this build has no backend for the
    /// target, when the surface cannot be created for the window, when no
    /// compatible native adapter exists, when the device request fails, or when
    /// the adapter is not the intended native backend.
    // one cohesive device/surface bootstrap
    pub fn new(window: &Window) -> Result<Self, String> {
        if !surface::native_backend_is_compiled() {
            return Err(format!(
                "the wgpu renderer was requested, but this build has no {} backend compiled \
                 (enabled backends: {:?})",
                surface::native_backend_label(),
                wgpu::Instance::enabled_backend_features()
            ));
        }

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: surface::native_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let retained_window = window.clone();
        // SAFETY: the clone owns the same SDL handles. It is retained by the
        // renderer after its surface field, including during surface recovery.
        let wgpu_surface = unsafe { surface::create(&instance, &retained_window) }?;
        let adapter = Self::request_adapter(&instance, &wgpu_surface)?;
        let adapter_info = adapter.get_info();
        Self::check_native_backend(&adapter_info)?;

        let capabilities = wgpu_surface.get_capabilities(&adapter);
        let format = surface::select_surface_format(&capabilities).ok_or_else(|| {
            format!(
                "the {} surface is not compatible with adapter '{}'",
                surface::native_backend_label(),
                adapter_info.name
            )
        })?;
        let present_mode = surface::select_present_mode(&capabilities, true);
        let alpha_mode = surface::select_alpha_mode(&capabilities);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("places-wgpu"),
            required_limits: world_device_limits(),
            required_features: gpu_timing::required_features(adapter.features()),
            ..Default::default()
        }))
        .map_err(|error| {
            format!(
                "wgpu could not create a device on '{}' ({}): {error}",
                adapter_info.name,
                surface::native_backend_label()
            )
        })?;
        // Explicit native shader acceptance probe. It uses this window's Metal
        // device and never allocates or waits in a normal game/benchmark run.
        #[cfg(feature = "visual-diagnostics")]
        if std::env::var("PLACES_BENCH").is_ok_and(|value| value == "1")
            && std::env::var("PLACES_BENCH_PRESENT_PROBE").is_ok_and(|value| value == "1")
        {
            super::postprocess::run_present_probe(&device, &queue)?;
        }
        let device_lost = Self::watch_device_loss(&device);
        let (width, height) = window.size_in_pixels();
        validate_drawable(
            DrawableSize::new(width, height),
            device.limits().max_texture_dimension_2d,
        )?;
        let mut renderer = Self::assemble(
            instance,
            Some(wgpu_surface),
            Some(capabilities),
            adapter,
            adapter_info,
            device,
            queue,
            format,
            present_mode,
            alpha_mode,
            DrawableSize::new(width, height),
            device_lost,
        );
        renderer.window = Some(retained_window);
        Ok(renderer)
    }

    /// Builds a window-free renderer for the offline probe capture.
    ///
    /// The engine never calls this: the player always owns a surface. The
    /// compiler uses it to install a prepared world and render its reflection
    /// probes with the exact pipelines the player will use, so the packaged
    /// cubemaps are the proven capture result rather than a second
    /// implementation. There is no swapchain, so `present`, `render_scene` and
    /// recovery are never called; installation, probe capture and read-back
    /// only touch the device and queue.
    ///
    /// # Errors
    ///
    /// Returns an error when no native adapter or device is available.
    pub fn new_headless(drawable_size: DrawableSize) -> Result<Self, String> {
        if !surface::native_backend_is_compiled() {
            return Err(format!(
                "the wgpu renderer was requested, but this build has no {} backend compiled \
                 (enabled backends: {:?})",
                surface::native_backend_label(),
                wgpu::Instance::enabled_backend_features()
            ));
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: surface::native_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|error| {
            format!(
                "no {adapter_label} adapter for the offline probe capture: {error}",
                adapter_label = surface::native_backend_label()
            )
        })?;
        let adapter_info = adapter.get_info();
        Self::check_native_backend(&adapter_info)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("places-wgpu-compiler"),
            required_limits: world_device_limits(),
            required_features: gpu_timing::required_features(adapter.features()),
            ..Default::default()
        }))
        .map_err(|error| {
            format!(
                "wgpu could not create a device on '{}' ({}): {error}",
                adapter_info.name,
                surface::native_backend_label()
            )
        })?;
        let device_lost = Self::watch_device_loss(&device);
        validate_drawable(drawable_size, device.limits().max_texture_dimension_2d)?;
        // A synthetic presentation configuration: pipelines that name the
        // surface format must exist, but no surface is ever configured or
        // presented. Probe captures render to the fixed reflection format.
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        Ok(Self::assemble(
            instance,
            None,
            None,
            adapter,
            adapter_info,
            device,
            queue,
            format,
            wgpu::PresentMode::Fifo,
            wgpu::CompositeAlphaMode::Opaque,
            drawable_size,
            device_lost,
        ))
    }

    /// Completes renderer construction from an initialized device.
    #[expect(clippy::too_many_arguments, reason = "one cohesive construction seam")] // one cohesive construction seam
    #[expect(clippy::too_many_lines, reason = "one cohesive construction seam")] // one cohesive construction seam
    fn assemble(
        instance: wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        capabilities: Option<wgpu::SurfaceCapabilities>,
        adapter: wgpu::Adapter,
        adapter_info: wgpu::AdapterInfo,
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        present_mode: wgpu::PresentMode,
        alpha_mode: wgpu::CompositeAlphaMode,
        drawable_size: DrawableSize,
        device_lost: Arc<Mutex<Option<String>>>,
    ) -> Self {
        // The texture layout, samplers and fallback sheet the world draws
        // bind. One fallback upload at construction; levels add their own.
        // Anisotropic filtering is a downlevel capability, not a device
        // feature: when it is absent the world presets clamp their request to
        // 1x instead of failing.
        let anisotropy_supported = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING);
        let textures = TextureCache::new(&device, &queue, anisotropy_supported);
        // The material bind group layout, created once and shared by every
        // pipeline rebuild and material binding.
        let material_layout = material_bind_group_layout(&device);
        // The environment layout (group 3), the reflection fallbacks and the
        // fixture-sheet slots.
        let environment_layout = environment_bind_group_layout(&device);
        let caustics = super::caustics::Caustics::empty(&device, &textures);
        let (probe_fallback, probe_fallback_view) = fallback_probe(&device);
        let (planar_fallback, planar_fallback_view) = fallback_planar(&device, &queue);
        let lightmaps = LightmapAtlas::upload(&device, &queue, None);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: drawable_size.width.max(1),
            height: drawable_size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };

        let gpu_timer = GpuTimer::new(&adapter, &device, &queue);
        let renderer = Self {
            window: None,
            surface,
            capabilities,
            config,
            adapter,
            device,
            queue,
            instance,
            depth: None,
            depth_size: DrawableSize::new(0, 0),
            drawable_size,
            world_pipeline: None,
            flashlight: None,
            flashlight_shadow: None,
            flashlight_characters: std::cell::RefCell::new(Vec::new()),
            world: None,
            world_props: None,
            static_light_bank: None,
            world_textures: None,
            world_materials: None,
            material_layout,
            environment_layout,
            caustics,
            textures,
            filtering: TextureFiltering::DEFAULT,
            prop_catalog: crate::engine::loader::PropCatalog::load_default(),
            prop_assets: crate::engine::props::PropAssets::load_default(),
            graphics_requested: GraphicsConfig::default(),
            graphics_applied: GraphicsConfig::default(),
            installed_quality: QualityLevel::default(),
            retained_build: None,
            prepared_install: None,
            installed_lightmaps: LightmapQuality::default(),
            lightmaps_resident: false,
            lightmaps,
            decals: None,
            decal_pipeline: None,
            decal_reflection_pipeline: None,
            sky: None,
            sky_pipeline: None,
            ui: None,
            last_ui: Vec::new(),
            post: None,
            scene_pipeline: None,
            emissive_pipeline: None,
            decal_scene_pipeline: None,
            environment: None,
            _probe_fallback: probe_fallback,
            probe_fallback_view,
            _planar_fallback: planar_fallback,
            planar_fallback_view,
            fixture_sheets: Vec::new(),
            material_animations: LevelEmissionAnimations::default(),
            animation_seconds: 0.0,
            entity_lighting_revision: 0,
            fog: LevelFog::global_only(),
            bloom_requested: true,
            #[cfg(feature = "visual-diagnostics")]
            visual_diagnostic: super::diagnostics::VisualDiagnosticMode::Final,
            reflections_enabled: true,
            reflections: Reflections::default(),
            reflection_targets: ReflectionTargets::default(),
            packaged_probes: None,
            capture_pipeline: None,
            active_plane: None,
            active_probe: 0,
            probe_max_mip: 0,
            reflection_passes: 0,
            culling: true,
            quality: QualityLevel::default(),
            lighting_quality: QualityLevel::default(),
            level_stats: LevelBuildStats::default(),
            render_stats: RenderStats::default(),
            gpu_timer,
            #[cfg(feature = "visual-diagnostics")]
            gpu_resource_trace: GpuResourceTrace::requested(),
            needs_configure: !drawable_size.is_empty(),
            vsync: true,
            pending_frame: None,
            surface_lost: false,
            fatal: None,
            device_lost,
            dynamic: DynamicScene::new(),
            runtime_objects: HashMap::new(),
            effects: EffectScene::new(),
            world_effects: None,
            effects_pipeline: None,
            effects_scene_pipeline: None,
            door_sources: Vec::new(),
            door_objects: Vec::new(),
            switchable_lights: Vec::new(),
            prop_light_bindings: Vec::new(),
            lightmap_cpu: None,
            level_id: None,
            installed_level: None,
            world_dynamic: None,
            characters: CharacterScene::new(),
            world_characters: None,
            character_runtime_generation: 0,
            dynamic_lighting: None,
            dynamic_field: None,
            dynamic_transport: None,
            visibility_materials: None,
            blacklights: crate::engine::render::common::blacklight::BlacklightSet::default(),
            dynamic_visibility:
                crate::engine::render::common::dynamic_visibility::DynamicVisibility::new(),
            dynamic_visibility_error: None,
            #[cfg(feature = "visual-diagnostics")]
            probe_visualization: None,
            last_frame: None,
            adapter_info,
        };
        renderer.log_startup();
        renderer
    }

    /// Requests the adapter, preferring real hardware and never a fallback.
    fn request_adapter(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<wgpu::Adapter, String> {
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(surface),
            apply_limit_buckets: false,
        };
        pollster::block_on(instance.request_adapter(&options)).map_err(|error| {
            let discovered: Vec<String> =
                pollster::block_on(instance.enumerate_adapters(surface::native_backends()))
                    .into_iter()
                    .map(|adapter| {
                        let info = adapter.get_info();
                        format!("'{}' ({:?}, {:?})", info.name, info.backend, info.device_type)
                    })
                    .collect();
            format!(
                "no compatible {} adapter for the Places window: {error}; adapters discovered: {discovered:?}",
                surface::native_backend_label()
            )
        })
    }

    /// Refuses an adapter from any backend but the intended native one.
    ///
    /// The build only compiles one backend, so this cannot normally trigger; it
    /// exists so a driver or wgpu change can never silently downgrade the new
    /// desktop path to GL.
    fn check_native_backend(info: &wgpu::AdapterInfo) -> Result<(), String> {
        let expected = surface::NATIVE_BACKEND;
        if info.backend == expected {
            return Ok(());
        }
        Err(format!(
            "wgpu selected the {:?} backend ('{}') but this build targets {expected:?}; \
             the wgpu path does not fall back to another backend",
            info.backend, info.name
        ))
    }

    /// Installs the device-lost callback.
    ///
    /// The callback only records the reason; the main thread reports it once
    /// and stops issuing GPU work.
    fn watch_device_loss(device: &wgpu::Device) -> Arc<Mutex<Option<String>>> {
        let slot = Arc::new(Mutex::new(None));
        let writer = Arc::clone(&slot);
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut loss_message) = writer.lock()
                && loss_message.is_none()
            {
                *loss_message = Some(format!("{reason:?}: {message}"));
            }
        });
        slot
    }

    /// Emits the one startup diagnostic for the wgpu path.
    ///
    /// Developer telemetry: printed only when `PLACES_VERBOSE` is set. The
    /// adapter line is what proves which native backend actually initialized.
    fn log_startup(&self) {
        logging::event(format!(
            "[renderer] wgpu | adapter: {} | backend: {:?} | device type: {:?} | surface format: {:?} | present mode: {:?} | alpha mode: {:?} | depth format: {:?} | drawable: {}x{}",
            self.adapter_info.name,
            self.adapter_info.backend,
            self.adapter_info.device_type,
            self.config.format,
            self.config.present_mode,
            self.config.alpha_mode,
            DEPTH_FORMAT,
            self.drawable_size.width,
            self.drawable_size.height
        ));
        logging::info(format!(
            "[wgpu] GPU timing: {} | pass timestamps supported={} requested={} | encoder timestamps supported={} | period={} ns/tick | scope=union of GPU pass stage spans (scene, planar, bloom, resolve, UI, surface copy); overlaps count once, disjoint gaps excluded; includes GPU scheduling stalls, not active utilization",
            self.gpu_timer.status().status(),
            self.adapter
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY),
            self.device
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY),
            self.adapter
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
            self.gpu_timer.period_ns(),
        ));
        if !surface::surface_format_is_srgb(self.config.format) {
            logging::warn(format!(
                "[wgpu] surface format {:?} uses encoded display copying; direct raw 8-bit drawing uses software sRGB encoding",
                self.config.format
            ));
        }
        // What the three Texture Filtering presets request, and whether this
        // adapter can honour it: every preset is trilinear plus anisotropy at
        // 4x/8x/16x, and an adapter without the downlevel capability clamps to
        // 1x while keeping the same linear levels. The active preset itself is
        // reported by `set_texture_filtering`, which the engine applies after
        // construction (and again on a live change).
        let world_presets = TextureFiltering::ALL
            .into_iter()
            .map(|level| format!("{} {}x", level.name(), level.anisotropy()))
            .collect::<Vec<_>>()
            .join(" / ");
        logging::info(format!(
            "[wgpu] texture filtering: world presets {world_presets} requested | anisotropy {}",
            if self.textures.anisotropy_supported() {
                "supported"
            } else {
                "unsupported (world presets clamp to 1x)"
            },
        ));
    }

    /// Records the player's `VSync` preference and rebuilds the presentation mode.
    ///
    /// Returns the interval the presentation mode corresponds to (`1` for the
    /// synchronized FIFO path, `0` for immediate).
    pub fn set_swap_interval(&mut self, want_vsync: bool) -> i32 {
        if self.check_device_lost() {
            return i32::from(self.config.present_mode != wgpu::PresentMode::Immediate);
        }
        self.vsync = want_vsync;
        let Some(capabilities) = self.capabilities.as_ref() else {
            // A headless (compiler) renderer has no presentation mode.
            return 1;
        };
        let mode = surface::select_present_mode(capabilities, want_vsync);
        if mode != self.config.present_mode {
            self.config.present_mode = mode;
            self.needs_configure = true;
            self.ensure_ready();
            logging::info(format!(
                "[wgpu] present mode {} ({})",
                present_mode_label(mode),
                if want_vsync { "vsync" } else { "immediate" }
            ));
        }
        // Report the interval the selected mode really corresponds to: a
        // surface that cannot present immediately keeps the synchronized path.
        i32::from(mode != wgpu::PresentMode::Immediate)
    }

    /// Records the drawable size and schedules a surface/depth rebuild.
    ///
    /// A zero width or height (a minimized or hidden window) is not an error:
    /// the surface is left configured but no frame is acquired until the
    /// drawable is valid again. Returns `true` when the size changed.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        if self.drawable_size == size {
            return false;
        }
        self.drawable_size = size;
        self.needs_configure = true;
        self.invalidate_frame();
        if let Err(error) = validate_drawable(size, self.device.limits().max_texture_dimension_2d) {
            self.mark_fatal(error);
        }
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("drawable-changed");
        true
    }

    /// A frame and its HUD can only be replayed against the resources and size
    /// they were rendered with. Retiring them also releases a held swapchain
    /// image before a resize, install or fatal-device shutdown.
    fn invalidate_frame(&mut self) {
        self.pending_frame = None;
        self.last_frame = None;
        self.last_ui.clear();
    }

    /// The current dynamic scene (empty until `set_dynamic_demo` spawns
    /// objects; the static world is uploaded separately).
    #[must_use]
    pub const fn dynamic_scene(&self) -> &DynamicScene {
        &self.dynamic
    }

    /// Advances the dynamic scene and the emission-animation clock.
    ///
    /// The neutral scene updates transforms and refreshes environmental
    /// lighting each frame; the GPU side then writes each object's new model
    /// matrix and light scale into its environment uniform. A still scene
    /// writes nothing.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        if self.check_device_lost() {
            return DynamicUpdate::default();
        }
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.animation_seconds += delta_seconds;
        }
        // Floats ride an absolute clock before the spin/probe pass, so a
        // moved float's probe is sampled at this frame's surface pose.
        self.caustics.update(&self.queue, self.animation_seconds);
        let _update_stats = self.dynamic.update_floats(self.animation_seconds);
        // Ambient effects ride the same absolute clock: a particle's pose is a
        // pure function of it, so this only records the time and reports which
        // emitters advanced. Their billboards are synced in `render_scene`,
        // where the frame's camera exists.
        let _update_stats_2 = self.effects.update(self.animation_seconds);
        let lighting_started = cpu_trace_start(self.animation_seconds);
        let update = self.refresh_dynamic_lighting(delta_seconds);
        cpu_trace_finish("dynamic_lighting", lighting_started);
        let upload_started = cpu_trace_start(self.animation_seconds);
        let environment = self.level_environment();
        if let Some(dynamic) = self.world_dynamic.as_mut() {
            dynamic.sync_with_blacklight(
                &self.queue,
                &self.dynamic,
                &environment,
                self.dynamic_transport.as_deref().map(|scene| {
                    crate::engine::render::common::blacklight::BlacklightVisibility {
                        sources: &self.blacklights,
                        scene,
                        dynamic: self
                            .dynamic_visibility_error
                            .is_none()
                            .then_some(&self.dynamic_visibility),
                    }
                }),
            );
        }
        #[cfg(feature = "visual-diagnostics")]
        if self.visual_diagnostic == super::diagnostics::VisualDiagnosticMode::ProbeNeighborhood {
            self.refresh_probe_visualization();
        }
        cpu_trace_finish("dynamic_upload", upload_started);
        update
    }

    fn refresh_dynamic_lighting(&mut self, delta_seconds: f32) -> DynamicUpdate {
        let visibility = self
            .dynamic_transport
            .as_ref()
            .map(|_| &mut self.dynamic_visibility);
        match self.dynamic.update_with_visibility_and_lighting_revision(
            delta_seconds,
            self.dynamic_lighting.as_ref(),
            self.dynamic_field.as_deref(),
            self.dynamic_transport.as_deref(),
            visibility,
            self.entity_lighting_revision,
        ) {
            Ok(update) => {
                self.dynamic_visibility_error = None;
                update
            }
            Err(error) => {
                logging::warn_once(
                    "movable-visibility",
                    format!("[entity-lighting] movable visibility unavailable: {error}"),
                );
                self.dynamic_visibility_error = Some(error);
                self.dynamic.update_with_field_and_scene(
                    0.0,
                    self.dynamic_lighting.as_ref(),
                    self.dynamic_field.as_deref(),
                    self.dynamic_transport.as_deref(),
                )
            }
        }
    }

    /// Invalid values are refused without replacing a valid current beam.
    pub fn set_flashlight(
        &mut self,
        light: Option<crate::engine::render::common::Flashlight>,
    ) -> bool {
        if light.is_some_and(|candidate| !candidate.is_well_formed()) {
            return false;
        }
        self.flashlight = light;
        true
    }

    /// Static lighting and texture settings do not gate collision visibility.
    /// Build the existing transport resource on demand where the prepared
    /// practical-light path did not already install one; there is no solve.
    fn ensure_flashlight_transport(&mut self) {
        if self.dynamic_transport.is_some() {
            return;
        }
        let (Some(level), Some(build), Some(materials)) = (
            self.installed_level.as_ref(),
            self.retained_build.as_ref(),
            self.visibility_materials.as_ref(),
        ) else {
            return;
        };
        self.dynamic_transport =
            crate::engine::render::common::light_transport::build_transport_scene(
                level,
                &build.mesh,
                &build.batches,
                materials,
                &build.lighting,
                &[],
            )
            .map(|(scene, _)| Arc::new(scene));
    }

    #[must_use]
    pub fn light_transmittance(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        excluded: Option<DynamicId>,
    ) -> f32 {
        if !from.iter().chain(&to).all(|value| value.is_finite()) {
            return 0.0;
        }
        self.ensure_flashlight_transport();
        let Some(transport) = self.dynamic_transport.as_deref() else {
            return 0.0;
        };
        match self.dynamic_visibility.sync(&self.dynamic) {
            Ok(_) => self.dynamic_visibility_error = None,
            Err(error) => {
                self.dynamic_visibility_error = Some(error);
                return 0.0;
            }
        }
        transport.transmittance(from, to)
            * self.dynamic_visibility.transmittance(from, to, excluded)
    }

    /// Incident beam light using the same falloff/cone as material shading and
    /// the shared static/movable triangle visibility. Animated blockers use
    /// their current posed geometry and exclude only the receiver instance.
    #[must_use]
    pub fn flashlight_illumination(
        &mut self,
        point: [f32; 3],
        excluded_instance: Option<&str>,
    ) -> f32 {
        let Some(light) = self.flashlight else {
            return 0.0;
        };
        let incident = light.illumination(point);
        if incident <= 0.0 {
            return 0.0;
        }
        let from = light.position.to_array();
        let throughput = self.light_transmittance(from, point, None);
        if throughput <= 0.0 {
            return 0.0;
        }
        incident
            * throughput
            * super::flashlight::character_transmittance(
                &self.characters,
                &self.flashlight_characters,
                from,
                point,
                excluded_instance,
            )
    }

    fn prepare_flashlight_shadow(&mut self) {
        let Some(light) = self.flashlight else {
            return;
        };
        if self.flashlight_shadow.is_none() {
            self.flashlight_shadow = Some(super::flashlight::FlashlightShadow::new(
                &self.device,
                self.textures.layout(),
                &self.material_layout,
                &self.environment_layout,
                light,
            ));
        }
        let Some(shadow) = self.flashlight_shadow.as_mut() else {
            return;
        };
        shadow.frame = super::flashlight::flashlight_frame(light);
        shadow
            .pipeline
            .upload_camera(&self.queue, shadow.frame.view_projection, shadow.frame.eye);
        for pipeline in [
            &mut self.world_pipeline,
            &mut self.scene_pipeline,
            &mut self.emissive_pipeline,
            &mut self.capture_pipeline,
        ]
        .into_iter()
        .flatten()
        {
            pipeline.bind_flashlight_shadow(&self.device, &shadow.view);
        }
        for pipeline in [
            &mut self.decal_pipeline,
            &mut self.decal_scene_pipeline,
            &mut self.decal_reflection_pipeline,
        ]
        .into_iter()
        .flatten()
        {
            pipeline.bind_flashlight_shadow(&self.device, &shadow.view);
        }
    }

    fn encode_flashlight_shadow(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.flashlight.is_none() {
            return;
        }
        let (Some(shadow), Some(world), Some(textures), Some(materials), Some(environment)) = (
            self.flashlight_shadow.as_ref(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.world_materials.as_ref(),
            self.environment.as_ref(),
        ) else {
            return;
        };
        let checkpoint = self.gpu_timer.pass_checkpoint();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("places-flashlight-visibility"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: self.gpu_timer.pass_writes("flashlight_visibility"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let totals = shadow.pipeline.encode(
            &mut pass,
            super::world::WorldEncodeInputs {
                geometry: world,
                textures,
                materials,
                environment: environment.bind_group_for_probe(self.active_probe),
                props: self.world_props.as_ref(),
                dynamic: self.world_dynamic.as_ref(),
                characters: self.world_characters.as_ref(),
                capture_plane: None,
                filtering: self.filtering,
                frame: &shadow.frame,
                cull: true,
            },
        );
        drop(pass);
        self.gpu_timer
            .record_pass_draws(checkpoint, totals.draw_calls);
    }

    /// The installed catalog's reusable physics profile for a rigid prop.
    ///
    /// This cold metadata lookup clones the bounded profile without disk reads
    /// or model decoding; frame updates retain their existing item state.
    #[must_use]
    pub fn rigid_runtime_model_physics(
        &self,
        id: &str,
    ) -> Option<crate::engine::physics::config::PhysicsDef> {
        self.prop_catalog.assets().get(id)?.physics.clone()
    }

    /// Resolve a bounded rigid catalog prop through the existing asset cache.
    /// No file-path fallback, animated bind pose or morph model is accepted here.
    pub fn rigid_runtime_model_bounds(&mut self, id: &str) -> Result<([f32; 3], [f32; 3]), String> {
        let entry = self
            .prop_catalog
            .assets()
            .get(id)
            .ok_or_else(|| format!("Unknown catalog ID `{id}`."))?;
        if !entry.is_placeable() {
            return Err(format!("`{id}` is not a placeable prop."));
        }
        let path = entry
            .model
            .as_deref()
            .ok_or_else(|| format!("`{id}` has no runtime model."))?;
        let asset = self.prop_assets.resolve(path)?;
        if asset.model.is_animatable() || !asset.model.morph_targets.is_empty() {
            return Err("Rigid items and /give previews need a model without skin, animation clips or morph targets; animated models require an entity template.".to_string());
        }
        let bounds = asset
            .model
            .bounds()
            .ok_or_else(|| "Model has no usable bounds.".to_string())?;
        let [x0, y0, z0] = bounds.0;
        let [x1, y1, z1] = bounds.1;
        let extent = glam::Vec3::new(x1 - x0, y1 - y0, z1 - z0);
        if !extent.is_finite() || extent.min_element() <= 0.0 || extent.max_element() > 100.0 {
            return Err(
                "Rigid props must have positive finite bounds no larger than 100 m per axis."
                    .to_string(),
            );
        }
        Ok(bounds)
    }

    /// Shared decoded bounds and pose capability for catalog-based visuals.
    pub fn runtime_model_info(
        &mut self,
        id: &str,
    ) -> Result<crate::engine::render::RuntimeModelInfo, String> {
        let entry = self
            .prop_catalog
            .assets()
            .get(id)
            .ok_or_else(|| format!("Unknown catalog ID `{id}`."))?;
        if !entry.is_placeable() {
            return Err(format!("`{id}` is not a placeable model."));
        }
        let path = entry
            .model
            .as_deref()
            .ok_or_else(|| format!("`{id}` has no runtime model."))?;
        let asset = self.prop_assets.resolve(path)?;
        let bounds = asset
            .model
            .bounds()
            .ok_or_else(|| "Model has no usable bounds.".to_string())?;
        let [x0, y0, z0] = bounds.0;
        let [x1, y1, z1] = bounds.1;
        let extent = glam::Vec3::new(x1 - x0, y1 - y0, z1 - z0);
        if !glam::Vec3::from(bounds.0).is_finite()
            || !glam::Vec3::from(bounds.1).is_finite()
            || !extent.is_finite()
            || extent.min_element() <= 0.0
            || extent.max_element() > 100.0
        {
            return Err(
                "Models need positive finite bounds no larger than 100 m per axis.".to_string(),
            );
        }
        let animated = asset.model.is_animatable();
        if animated && crate::engine::render::CharacterAnimator::new(&asset.model).is_none() {
            return Err(format!("`{id}` has no poseable rig."));
        }
        Ok(crate::engine::render::RuntimeModelInfo { bounds, animated })
    }

    /// Spawns one runtime entity's model as a dynamic object.
    ///
    /// `key` is the engine's stable runtime token: spawning with a live key
    /// despawns the object that key held first, so a respawn replaces rather
    /// than accumulates, and the key's new object can then be moved with
    /// [`Self::set_runtime_transform`] and re-materialed with
    /// [`Self::set_runtime_emission`]. `model` is a catalogue registry id
    /// (such as `core:crate`) or a direct model path; it resolves through the
    /// same catalogue and asset cache the placed-prop and floating paths use.
    ///
    /// Returns `false` when the model cannot be resolved or the scene's object
    /// or mesh budget is exhausted. An unresolvable model is reported once per
    /// model path, exactly like the static prop fallback; this never panics.
    pub fn spawn_runtime_model(
        &mut self,
        key: u64,
        model: &str,
        position: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
    ) -> bool {
        if self.check_device_lost() {
            return false;
        }
        // A live key is replaced, not stacked: its previous object is dropped
        // before the new one resolves, so a failed resolution cannot leave two
        // objects claiming the same key.
        let replaced = self
            .runtime_objects
            .remove(&key)
            .is_some_and(|previous| self.dynamic.despawn(previous));
        // The catalogue maps a registry id to its model path; a string the
        // catalogue does not know is tried as a model path directly, so the
        // call serves both the engine's registry ids and raw paths.
        let path = self
            .prop_catalog
            .get(model)
            .model
            .filter(|path| !path.is_empty());
        let resolved_path = path.as_deref().unwrap_or(model);
        let asset = match self.prop_assets.resolve(resolved_path) {
            Ok(asset) => asset,
            Err(error) => {
                self.prop_assets.report_failure(resolved_path, &error);
                if replaced {
                    self.upload_dynamic();
                }
                return false;
            }
        };
        let Some(id) = self
            .dynamic
            .spawn(&asset, position, yaw_degrees, scale, 0.0)
        else {
            if replaced {
                self.upload_dynamic();
            }
            return false;
        };
        let _previous_value = self.runtime_objects.insert(key, id);
        if let Some(tint) = self.prop_catalog.tint(model) {
            let _tint_applied = self.dynamic.set_albedo_tint(id, tint);
        }
        self.upload_dynamic();
        true
    }

    /// Spawns an authored prop through the shared runtime model cache.
    ///
    /// `position` is the prop's world-space model origin, including its resolved
    /// floor height and authored vertical offset. The level's catalog resolves
    /// `prop.model`; authored yaw and uniform scale are retained. Runtime keys
    /// and poses survive a graphics rebuild of the same installed level.
    pub fn spawn_runtime_prop(
        &mut self,
        key: u64,
        prop: &crate::engine::level::PropDef,
        position: [f32; 3],
    ) -> bool {
        self.spawn_runtime_model(
            key,
            &prop.model,
            position,
            prop.rotation_degrees,
            prop.scale,
        )
    }

    /// Despawns the runtime object a key spawned, if it is still live.
    ///
    /// Returns whether the key held a live object. The GPU side is re-uploaded
    /// without it, so the object stops drawing on the next frame. Doors and
    /// demonstration objects are not addressed by runtime keys and are never
    /// touched.
    pub fn despawn_runtime_model(&mut self, key: u64) -> bool {
        if self.check_device_lost() {
            return false;
        }
        let Some(id) = self.runtime_objects.remove(&key) else {
            return false;
        };
        let removed = self.dynamic.despawn(id);
        if removed {
            self.upload_dynamic();
        }
        removed
    }

    /// Moves a live runtime object to `position`, yawed by `yaw_degrees` about
    /// its own model axis.
    ///
    /// The object keeps the mesh, scale and material it spawned with, and the
    /// write costs no geometry work: it lands in the neutral transform and the
    /// frame's [`Self::update_dynamic`] sync publishes it through the object's
    /// environment uniform. Returns whether the key held a live object and the
    /// transform was accepted (a non-finite position or yaw is refused, like
    /// every other dynamic transform).
    pub fn set_runtime_transform(
        &mut self,
        key: u64,
        position: [f32; 3],
        yaw_degrees: f32,
    ) -> bool {
        let Some(id) = self.runtime_objects.get(&key).copied() else {
            return false;
        };
        let Some(scale) = self.dynamic.get(id).map(DynamicObject::scale) else {
            return false;
        };
        self.dynamic.set_transform(id, position, yaw_degrees, scale)
    }

    /// Sets a live runtime model's rigid-body pose while preserving its scale.
    ///
    /// `orientation` uses quaternion `[x, y, z, w]` order and is normalized.
    /// Invalid poses or missing keys return `false` without changing the object.
    /// The next dynamic update publishes model, bounds and lighting uniforms;
    /// this call neither reloads assets nor uploads geometry.
    pub fn set_runtime_model_pose(
        &mut self,
        key: u64,
        position: [f32; 3],
        orientation: [f32; 4],
    ) -> bool {
        let Some(id) = self.runtime_objects.get(&key).copied() else {
            return false;
        };
        self.dynamic.set_pose(id, position, orientation)
    }

    /// Hides or restores a retained runtime model without reloading resources.
    ///
    /// Hidden models keep their key and scene budget, but do not draw, cast
    /// movable shadows or receive lighting updates. The next dynamic update
    /// refreshes the shared visibility resource and restored lighting.
    pub fn set_runtime_model_visible(&mut self, key: u64, visible: bool) -> bool {
        let Some(id) = self.runtime_objects.get(&key).copied() else {
            return false;
        };
        if !self.dynamic.set_visible(id, visible) {
            return false;
        }
        if let Some(slot) = self
            .dynamic
            .objects()
            .iter()
            .position(|object| object.id() == id)
            && let Some(gpu) = self.world_dynamic.as_mut()
        {
            let _visibility_set = gpu.set_object_visible(slot, visible);
        }
        true
    }

    /// Read-only pose, sampled HDR and published uniform data for one runtime key.
    /// Allocates only on an explicit diagnostic call and performs no GPU readback.
    #[must_use]
    pub fn runtime_model_lighting_snapshot(&self, key: u64) -> Option<serde_json::Value> {
        let id = self.runtime_objects.get(&key)?;
        let (slot, object) = self
            .dynamic
            .objects()
            .iter()
            .enumerate()
            .find(|(_, object)| object.id() == *id)?;
        let assets = self.prop_assets.stats();
        let mut snapshot = object.diagnostic_lighting_snapshot();
        let fields = snapshot.as_object_mut()?;
        let _previous_key = fields.insert("key".to_string(), serde_json::json!(key));
        let _previous_published = fields.insert(
            "published".to_string(),
            self.world_dynamic
                .as_ref()
                .and_then(|gpu| gpu.diagnostic_lighting_snapshot(slot))
                .unwrap_or(serde_json::Value::Null),
        );
        let _previous_resources = fields.insert(
            "resources".to_string(),
            serde_json::json!({
                "geometry_revision": self.dynamic.revision(),
                "gpu": self.world_dynamic.as_ref().map(WgpuDynamic::stats),
                "visibility": self.dynamic_visibility.stats(),
                "visibility_revision": self.dynamic_visibility.revision(),
                "visibility_error": self.dynamic_visibility_error,
            "switchable_indirect_sources": self.dynamic_field.as_ref().map(|field| {
                field.switchable_indirect.iter().map(|plane| plane.light_index).collect::<Vec<_>>()
            }),
                "models_loaded": assets.models_loaded,
                "models_failed": assets.models_failed,
                "texture_bytes": assets.texture_bytes,
            }),
        );
        Some(snapshot)
    }

    /// Selects the emission scale of a live runtime object.
    ///
    /// This is the runtime material-variant effect: the scale multiplies
    /// whichever emission each of the object's primitives resolves (its own,
    /// or the object-wide override set through the neutral scene), so `0.0`
    /// switches the object's emission off and `1.0` restores it. The GPU
    /// materials receive only a changed emission-scale uniform. Returns
    /// whether the key held a live object and the scale was
    /// accepted; a non-finite or negative scale is refused.
    pub fn set_runtime_emission(&mut self, key: u64, scale: f32) -> bool {
        if self.check_device_lost() {
            return false;
        }
        let Some(id) = self.runtime_objects.get(&key).copied() else {
            return false;
        };
        // Selecting the variant that is already live is not a material change:
        // report success without rebuilding every dynamic material. The
        // total-order comparison keeps a NaN from looking equal.
        let unchanged = self
            .dynamic
            .get(id)
            .is_some_and(|object| object.emission_scale().total_cmp(&scale).is_eq());
        if unchanged {
            return true;
        }
        if !self.dynamic.set_emission_scale(id, scale) {
            return false;
        }
        if let Some(dynamic) = self.world_dynamic.as_mut() {
            let _updated = dynamic.set_object_emission_scale(&self.queue, id, scale);
        }
        true
    }

    /// Number of live runtime-spawned objects: the spawn budget's used slots.
    ///
    /// Doors and the washer-drum demonstration are not runtime-spawned and are
    /// not counted.
    #[must_use]
    pub fn runtime_spawn_count(&self) -> usize {
        self.runtime_objects.len()
    }

    /// Spawns the level's dynamic demonstration objects and uploads them.
    ///
    /// Creates one spinning washer drum in front of
    /// every placed machine. Returns how many objects spawned.
    pub fn set_dynamic_demo(&mut self, level: &crate::engine::level::LevelDef) -> usize {
        if self.check_device_lost() {
            return 0;
        }
        self.dynamic.clear_all();
        // The wipe removed every runtime-spawned object with the scene; their
        // keys must not resolve to a new object later.
        self.runtime_objects.clear();
        self.door_objects.clear();
        self.world_dynamic = None;
        // The wipe removed every door object too; respawn them from the
        // models the install already built, so a demonstration level keeps its
        // doors.
        self.spawn_doors();
        let spawned =
            self.dynamic
                .spawn_washer_drum_demo(level, &self.prop_catalog, &mut self.prop_assets);
        self.upload_dynamic();
        spawned
    }

    /// Builds and records the installed level's door models.
    ///
    /// Runs once per level install, where the resolved material table (and so
    /// every door texture) is available. Placement is resolved here too: the
    /// hinge's base height is the walkable floor under it, the same surface the
    /// physical collider uses.
    fn prepare_door_sources(
        &mut self,
        level: &crate::engine::level::LevelDef,
        materials: &crate::engine::materials::MaterialTable,
    ) {
        self.door_sources.clear();
        self.door_objects.clear();
        if level.doors.is_empty() {
            return;
        }
        for def in &level.doors {
            let frame = level.door_frame(def);
            let Some(models) =
                crate::engine::render::common::doors::build_door_models(def, materials, frame)
            else {
                logging::warn(format!(
                    "[doors] `{}` has an unresolved door material; the leaf will not draw",
                    def.id
                ));
                continue;
            };
            let hinge = [def.x, def.base_y(level), def.z];
            if !hinge.iter().all(|value| value.is_finite()) {
                continue;
            }
            self.door_sources.push((
                DoorPlacement {
                    hinge,
                    yaw_degrees: def.rotation_degrees,
                },
                models,
            ));
        }
    }

    /// Spawns one frame object and one leaf object per installed door.
    ///
    /// Idempotent: any previous door objects are despawned first, so this can
    /// run after a scene wipe without duplicating anything.
    fn spawn_doors(&mut self) {
        if self.door_sources.is_empty() {
            return;
        }
        if self.door_objects.len() == self.door_sources.len() {
            // Already spawned against the current scene.
            return;
        }
        for draw in self.door_objects.drain(..) {
            if let Some(id) = draw.frame {
                let _despawn_status = self.dynamic.despawn(id);
            }
            if let Some(id) = draw.leaf {
                let _despawn_status_2 = self.dynamic.despawn(id);
            }
        }
        for (index, (placement, models)) in self.door_sources.iter().enumerate() {
            let orientation = SpawnOrientation {
                base_rotation: glam::Quat::from_rotation_y(placement.yaw_degrees.to_radians()),
                spin_axis: [0.0, 1.0, 0.0],
            };
            let frame_key = format!("door:frame:{index}");
            let leaf_key = format!("door:leaf:{index}");
            let frame = self
                .dynamic
                .register_model(
                    &frame_key,
                    &models.frame,
                    models.textures.clone(),
                    &models.frame_alphas,
                )
                .and_then(|mesh| {
                    self.dynamic
                        .spawn_registered(mesh, placement.hinge, orientation, 0.0, 1.0)
                });
            let leaf = self
                .dynamic
                .register_model(
                    &leaf_key,
                    &models.leaf,
                    models.textures.clone(),
                    &models.leaf_alphas,
                )
                .and_then(|mesh| {
                    self.dynamic
                        .spawn_registered(mesh, placement.hinge, orientation, 0.0, 1.0)
                });
            self.door_objects.push(DoorDraw { frame, leaf });
        }
    }

    /// Republishes every door leaf's angle from the gameplay state.
    ///
    /// Called once per frame after movement and actions; the drawn slab and the
    /// physical collider both read [`crate::engine::door::DoorRuntime::angle`], so the
    /// two can never disagree. A still door writes the same transform it
    /// already has, which the environment uniform comparison skips.
    pub fn sync_doors(&mut self, doors: &crate::engine::door::Doors) {
        if self.door_objects.is_empty() || doors.is_empty() {
            return;
        }
        for (slot, door) in doors.iter().enumerate() {
            let Some(draw) = self.door_objects.get(slot) else {
                continue;
            };
            let Some(leaf) = draw.leaf else {
                continue;
            };
            let hinge = [door.def.x, door.base_y(), door.def.z];
            let _transform_changed = self.dynamic.set_transform(leaf, hinge, door.angle(), 1.0);
        }
    }

    /// How many doors the installed level draws, and how many spawned.
    #[must_use]
    pub const fn door_render_counts(&self) -> (usize, usize) {
        (self.door_sources.len(), self.door_objects.len())
    }

    /// Records every switchable fixture's runtime bindings for this install.
    ///
    /// The face slot comes from the uploaded draw set and materials: a
    /// switchable fixture owns its own `Light` draw (its private material
    /// index), so the material entry that draw uses is exactly the uniform the
    /// on/off write targets.
    fn prepare_switchable_lights(&mut self, level: &crate::engine::level::LevelDef) {
        self.switchable_lights.clear();
        self.prop_light_bindings.clear();
        let Some(lighting) = self.dynamic_lighting.as_ref() else {
            return;
        };
        let animations =
            Self::resolve_animations(level, &crate::engine::materials::MaterialTable::default());
        for (fixture_index, fixture) in level.ceiling_lights.iter().enumerate() {
            let kind = crate::engine::lighting::fixture_profile(&fixture.fixture).kind;
            let face_material =
                crate::engine::level::fixture_face_material_index(fixture_index, true, kind);
            let animation = usize::try_from(face_material)
                .ok()
                .and_then(|slot| animations.fixtures.get(slot))
                .copied()
                .flatten();
            if let Some(props) = self.world_props.as_mut() {
                props.configure_light_control(
                    &self.queue,
                    u32::try_from(fixture_index).unwrap_or(u32::MAX),
                    if fixture.switchable
                        || kind == crate::engine::lighting::FixtureKind::BlacklightTube
                    {
                        fixture.enabled
                    } else {
                        true
                    },
                    1.0,
                    animation,
                );
            }
            if !fixture.switchable && kind != crate::engine::lighting::FixtureKind::BlacklightTube {
                continue;
            }
            let Some(light_index) = lighting.fixture_light_index(fixture_index) else {
                continue;
            };
            let material_slot = self
                .world
                .as_ref()
                .zip(self.world_materials.as_ref())
                .and_then(|(world, materials)| {
                    world
                        .draws()
                        .iter()
                        .position(|draw| {
                            draw.kind == SurfaceKind::Light && draw.material == face_material
                        })
                        .and_then(|draw| materials.slot_for_draw(draw))
                });
            if let Some(slot) = material_slot
                && let Some(materials) = self.world_materials.as_mut()
            {
                let _changed = materials.set_emission_scale(
                    &self.queue,
                    slot,
                    if fixture.enabled { 1.0 } else { 0.0 },
                );
            }
            self.switchable_lights.push(SwitchableLight {
                fixture_index,
                light_index,
                material_slot,
                hold_in_static: fixture.hold_in_static,
                baked_enabled: fixture.enabled,
            });
        }
        for (index, prop) in level.props.iter().enumerate() {
            let Some(control) = prop.light_control() else {
                continue;
            };
            let controller = level.ceiling_lights.len().saturating_add(index);
            if let Some(props) = self.world_props.as_mut() {
                props.configure_light_control(
                    &self.queue,
                    u32::try_from(controller).unwrap_or(u32::MAX),
                    control.enabled,
                    control.emission_scale,
                    None,
                );
            }
            if control.switchable {
                self.prop_light_bindings.push(PropLightBinding {
                    controller,
                    sources: lighting
                        .prop_light_indices(index)
                        .zip(&prop.lights)
                        .filter_map(|(source, light)| light.enabled.then_some(source))
                        .collect(),
                    baked_enabled: control.enabled,
                });
            }
        }
    }

    /// Applies the frame's fixture switches: illumination and face emission.
    ///
    /// A switchable fixture's illumination is a prepared layer group, so
    /// toggling it only rewrites the environment uniform's enabled mask: the
    /// shader adds exactly the pairs of the groups that are on. Its luminous
    /// face is scaled by the same state, so a light that is off does not keep
    /// glowing. Both writes are bounded by one material uniform and one
    /// environment uniform, and an empty list costs nothing.
    pub fn apply_light_toggles(&mut self, toggles: &[(usize, bool)]) {
        if toggles.is_empty() || self.check_device_lost() {
            return;
        }
        self.select_static_light_state(toggles);
        let mut changed = false;
        for (controller, requested) in toggles {
            if let Some(binding) = self
                .switchable_lights
                .iter()
                .find(|binding| binding.fixture_index == *controller)
                .copied()
            {
                let enabled = binding.enabled_for_mode(*requested, self.lightmaps.is_resident());
                if let Some(slot) = binding.material_slot
                    && let Some(materials) = self.world_materials.as_mut()
                {
                    let _changed = materials.set_emission_scale(
                        &self.queue,
                        slot,
                        if enabled { 1.0 } else { 0.0 },
                    );
                }
                if let Some(props) = self.world_props.as_mut() {
                    props.set_light_enabled(
                        &self.queue,
                        u32::try_from(*controller).unwrap_or(u32::MAX),
                        enabled,
                    );
                }
                if let Some(lighting) = self.dynamic_lighting.as_mut() {
                    for source in lighting.fixture_light_indices(binding.fixture_index) {
                        changed |= lighting.set_light_enabled(source, enabled);
                    }
                }
            } else if let Some(binding) = self
                .prop_light_bindings
                .iter()
                .find(|binding| binding.controller == *controller)
            {
                // Static stores one immutable vertex-lit state. Hold both the
                // face and live source to that same state; Full selects layers.
                let enabled = if self.lightmaps.is_resident() {
                    *requested
                } else {
                    binding.baked_enabled
                };
                if let Some(props) = self.world_props.as_mut() {
                    props.set_light_enabled(
                        &self.queue,
                        u32::try_from(*controller).unwrap_or(u32::MAX),
                        enabled,
                    );
                }
                if let Some(lighting) = self.dynamic_lighting.as_mut() {
                    for source in &binding.sources {
                        changed |= lighting.set_light_enabled(*source, enabled);
                    }
                }
            }
        }
        if changed {
            if let Some(lighting) = self.dynamic_lighting.as_ref() {
                self.caustics.refresh_sources(&self.queue, lighting);
            }
            self.entity_lighting_revision = self.entity_lighting_revision.wrapping_add(1);
            let uniform = self.level_environment();
            if let Some(environment) = self.environment.as_mut() {
                let _update_stats = environment.update(&self.queue, &uniform);
            }
        }
    }

    /// Spawns every placed prop that authors `float` on its water surface.
    ///
    /// Runs for every level (a float is a level-authored prop, not a
    /// demonstration), clears any previous float set first so it is idempotent
    /// across level reloads, and uploads when anything changed. Returns how
    /// many floats spawned.
    pub fn set_floating_props(&mut self, level: &crate::engine::level::LevelDef) -> usize {
        if self.check_device_lost() {
            return 0;
        }
        let before = self.dynamic.float_count();
        self.dynamic.clear_floats();
        let spawned =
            self.dynamic
                .spawn_floating_props(level, &self.prop_catalog, &mut self.prop_assets);
        if spawned > 0 || before > 0 {
            self.upload_dynamic();
        }
        spawned
    }

    /// Installs (or re-installs) the level's ambient effect emitters.
    ///
    /// The level install already builds the neutral scene from the resolved
    /// material table — the only place that table exists — and uploads it, so
    /// this engine-facing hand-off is **idempotent**: for the installed level
    /// it uploads the GPU side only when it is missing, and any other level is
    /// ignored because the install path owns it. Returns the number of live
    /// emitters.
    /// Enables or disables one authored effect emitter at runtime.
    ///
    /// The billboards are rebuilt every frame from the neutral scene, but the
    /// GPU-side draw groups are an upload-time snapshot of the scene's
    /// material grouping, so a toggle re-uploads the effect buffers. A toggle
    /// is a rare gameplay event, not a per-frame path.
    pub fn set_effect_enabled(&mut self, authored_index: usize, enabled: bool) -> bool {
        if !self.effects.set_enabled(authored_index, enabled) {
            return false;
        }
        if self.world_effects.is_some() {
            self.upload_effects();
        }
        true
    }

    pub fn set_level_effects(&mut self, level: &crate::engine::level::LevelDef) -> usize {
        if self.check_device_lost() {
            return 0;
        }
        if self.level_id.as_deref() != Some(level.id.as_str()) {
            return 0;
        }
        if self.world_effects.is_none() && !self.effects.is_empty() {
            self.upload_effects();
        }
        self.effects.len()
    }

    /// Changes particle motion, outdoor extinction and sky blend together.
    /// Both weather states already share the uploaded effect resources.
    pub fn set_weather_alternate(&mut self, alternate: bool) -> bool {
        self.effects.set_weather_alternate(alternate)
    }

    pub fn set_weather_strength(&mut self, strength: f32, transition_seconds: f32) -> bool {
        self.effects
            .set_weather_strength(strength, transition_seconds)
    }

    pub fn set_weather_cycle(&mut self, enabled: bool) -> bool {
        self.effects.set_weather_cycle(enabled)
    }

    pub fn update_weather(&mut self, delta_seconds: f32) {
        self.effects.update_weather(delta_seconds);
    }

    #[must_use]
    pub const fn weather_state(&self) -> (f32, f32, bool) {
        self.effects.weather_state()
    }

    /// Uploads the effect scene's GPU buffers and sheets.
    ///
    /// Called once per level install, and again on a quality rebuild of the
    /// same level because a quality change refits the texture cache. The
    /// neutral scene is level content and survives a quality rebuild; the GPU
    /// side is always rebuilt against the resources just installed.
    fn upload_effects(&mut self) {
        if self.check_device_lost() {
            self.world_effects = None;
            return;
        }
        if self.effects.is_empty() {
            self.world_effects = None;
            return;
        }
        let effects = WgpuEffects::upload(
            &self.device,
            &self.queue,
            &mut self.textures,
            &self.effects,
            self.quality,
        );
        let stats = effects.stats();
        logging::info(format!(
            "[wgpu] effects: {} emitter(s), {} particle(s), {} draw(s), {} sheet(s) \
             ({} uploaded, {} cached)",
            stats.emitters,
            stats.particles,
            stats.draws.max(1),
            self.effects.textures().len(),
            stats.texture_uploads,
            stats.texture_cache_hits,
        ));
        self.world_effects = Some(effects);
    }

    /// Uploads the dynamic scene's GPU meshes, materials and environments.
    ///
    /// Called after the static level exists, once per dynamic spawn.
    fn upload_dynamic(&mut self) {
        if self.check_device_lost() {
            self.world_dynamic = None;
            return;
        }
        if self.dynamic.is_empty() {
            self.world_dynamic = None;
            return;
        }
        let probe_views: Vec<&wgpu::TextureView> = self
            .reflection_targets
            .probes()
            .iter()
            .map(ProbeCube::view)
            .collect();
        // The dynamic path keeps the fallback planar image. A dynamic object is
        // drawn inside the planar capture as well as the ordinary frame, and an
        // environment bind group that sampled the live planar target would make
        // that capture pass sample the texture it is rendering into — a wgpu
        // validation error and a texture feedback loop. Every level load binds
        // the fallback because the planar
        // target is created after it; this keeps that behaviour stable across a
        // later re-upload instead of depending on the target's creation timing.
        let planar = &self.planar_fallback_view;
        let environment = self.level_environment();
        let mut ctx = DynamicUploadContext {
            device: &self.device,
            queue: &self.queue,
            cache: &mut self.textures,
            material_layout: &self.material_layout,
            environment_layout: &self.environment_layout,
            caustics: &self.caustics,
            lightmaps: &self.lightmaps,
            environment,
            probes: &probe_views,
            planar,
            probe_fallback: &self.probe_fallback_view,
            planar_fallback: &self.planar_fallback_view,
            level: self.quality,
            lighting_level: self.lighting_quality,
        };
        self.world_dynamic = Some(WgpuDynamic::upload(&mut ctx, &self.dynamic));
    }

    /// Advances every character's pose and uploads the characters that moved.
    ///
    /// The neutral scene evaluates the blend weights, gait phase and clip
    /// crossfade; the GPU side re-skins only the characters whose pose
    /// revision changed and writes their vertex buffers and live transforms.
    /// A still character writes nothing. A runtime spawn or despawn since the
    /// last pass bumps the scene's runtime generation, and the GPU side is
    /// rebuilt before it syncs, so the change is visible on the same frame.
    /// Returns how many characters moved this pass.
    pub fn update_characters(
        &mut self,
        delta_seconds: f32,
        locomotion: LocomotionSnapshot,
        frames: &[EntityFrame],
    ) -> crate::engine::render::CharacterUpdate {
        if self.check_device_lost() {
            return crate::engine::render::CharacterUpdate::default();
        }
        let animation_started = cpu_trace_start(self.animation_seconds);
        let update = self.characters.update(delta_seconds, locomotion, frames);
        cpu_trace_finish("character_animation", animation_started);
        let lighting_started = cpu_trace_start(self.animation_seconds);
        if let Some(lighting) = self.dynamic_lighting.as_ref() {
            self.characters
                .refresh_lighting_with_visibility_and_revision(
                    lighting,
                    self.dynamic_field.as_deref(),
                    self.dynamic_transport.as_deref(),
                    self.dynamic_transport
                        .as_ref()
                        .filter(|_| self.dynamic_visibility_error.is_none())
                        .map(|_| &self.dynamic_visibility),
                    self.entity_lighting_revision,
                );
        }
        let generation = self.characters.runtime_generation();
        cpu_trace_finish("character_lighting", lighting_started);
        if generation != self.character_runtime_generation {
            self.character_runtime_generation = generation;
            self.upload_characters_gpu();
        }
        let environment = self.level_environment();
        let upload_started = cpu_trace_start(self.animation_seconds);
        if let Some(characters) = self.world_characters.as_mut() {
            let _sync_status = characters.sync_with_blacklight(
                &self.queue,
                &self.characters,
                &environment,
                self.dynamic_transport.as_deref().map(|scene| {
                    crate::engine::render::common::blacklight::BlacklightVisibility {
                        sources: &self.blacklights,
                        scene,
                        dynamic: self
                            .dynamic_visibility_error
                            .is_none()
                            .then_some(&self.dynamic_visibility),
                    }
                }),
            );
        }
        cpu_trace_finish("character_skin_upload", upload_started);
        update
    }

    /// Spawns an animatable runtime actor through the character path.
    ///
    /// The actor resolves its model exactly like [`Self::spawn_runtime_model`]
    /// (a catalogue registry id or a direct model path), is lit by the
    /// installed level's baked lighting and irradiance field, and animates
    /// through the ordinary character/clip path: an [`EntityFrame`] addressed
    /// to `instance_id` drives its transform and pose cue. Spawning a live
    /// instance id replaces that actor. The GPU side is rebuilt by the next
    /// [`Self::update_characters`], which is where a frame loop already syncs
    /// characters.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the model cannot be animated (the caller falls back
    /// to the dynamic path) or the scene refused the placement for another
    /// reason: no installed level or lighting, a non-finite position or a
    /// non-positive scale, or a full character budget.
    pub fn spawn_runtime_character(
        &mut self,
        instance_id: &str,
        model: &str,
        position: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
    ) -> Result<(), String> {
        if self.check_device_lost() {
            return Err("the GPU device is lost; no runtime character was spawned".to_string());
        }
        let Some(level) = self.installed_level.as_ref() else {
            return Err("no level is installed; a runtime character has no placement".to_string());
        };
        let Some(lighting) = self.dynamic_lighting.as_ref() else {
            return Err(
                "the installed level has no baked lighting; the runtime character cannot be lit"
                    .to_string(),
            );
        };
        // The catalogue maps a registry id to its model path; a string the
        // catalogue does not know is tried as a model path directly, exactly
        // like `spawn_runtime_model`.
        let entry = self.prop_catalog.get(model);
        let path = entry.model.filter(|path| !path.is_empty());
        let resolved_path = path.as_deref().unwrap_or(model);
        self.characters.spawn_runtime_character(
            level,
            &self.prop_catalog,
            &mut self.prop_assets,
            lighting,
            self.dynamic_field.as_deref(),
            instance_id,
            resolved_path,
            glam::Vec3::from(position),
            yaw_degrees,
            scale,
        )
    }

    /// Removes a runtime actor; true when one existed.
    ///
    /// The GPU side is rebuilt by the next [`Self::update_characters`].
    pub fn despawn_runtime_character(&mut self, instance_id: &str) -> bool {
        if self.check_device_lost() {
            return false;
        }
        self.characters.despawn_runtime_character(instance_id)
    }

    /// Moves a live runtime actor; true when one existed.
    ///
    /// The actor keeps its model, scale and playback; the write lands in the
    /// neutral scene and reaches the GPU through the next
    /// [`Self::update_characters`] sync.
    pub fn set_runtime_character_transform(
        &mut self,
        instance_id: &str,
        position: [f32; 3],
        yaw_degrees: f32,
    ) -> bool {
        self.characters.set_runtime_character_transform(
            instance_id,
            glam::Vec3::from(position),
            yaw_degrees,
        )
    }

    /// Applies a physics/world orientation independently of the animated rig.
    pub fn set_runtime_character_pose(
        &mut self,
        instance_id: &str,
        position: [f32; 3],
        orientation: [f32; 4],
    ) -> bool {
        self.characters.set_runtime_character_pose(
            instance_id,
            glam::Vec3::from(position),
            glam::Quat::from_array(orientation),
        )
    }

    /// Retains the simulation snapshot used by one runtime character's rig.
    pub fn set_runtime_character_locomotion(
        &mut self,
        instance_id: &str,
        locomotion: LocomotionSnapshot,
    ) -> bool {
        self.characters
            .set_runtime_character_locomotion(instance_id, locomotion)
    }

    /// Supplies one actor's next pose delta independently of scaled world time.
    pub fn set_runtime_character_frame_delta(
        &mut self,
        instance_id: &str,
        delta_seconds: f32,
    ) -> bool {
        self.characters
            .set_runtime_character_frame_delta(instance_id, delta_seconds)
    }

    /// Adds or removes a runtime actor's retained socket/offset illumination.
    pub fn set_runtime_character_glow(
        &mut self,
        instance_id: &str,
        glow: Option<crate::engine::entity::GlowCue>,
    ) -> bool {
        self.characters
            .set_runtime_character_glow(instance_id, glow)
    }

    /// Retains a suspended runtime actor while hiding its complete visual.
    pub fn set_runtime_character_visible(&mut self, instance_id: &str, visible: bool) -> bool {
        self.characters
            .set_runtime_character_visible(instance_id, visible)
    }

    /// The number of live characters in the current level.
    #[must_use]
    pub const fn character_count(&self) -> usize {
        self.characters.len()
    }

    /// Uploads the current neutral character scene's GPU resources.
    ///
    /// Called from `install_level` after the probe bake (and after the neutral
    /// scene was rebuilt), and again after a reflection-only change: a
    /// character's group-3 environment samples the probe cubemaps, so it must
    /// not be bound while those are being captured.
    fn upload_characters_gpu(&mut self) {
        if self.check_device_lost() {
            self.world_characters = None;
            return;
        }
        if self.characters.is_empty() {
            self.world_characters = None;
            return;
        }
        let probe_views: Vec<&wgpu::TextureView> = self
            .reflection_targets
            .probes()
            .iter()
            .map(ProbeCube::view)
            .collect();
        // The dynamic upload's fallback-planar rule applies unchanged: a
        // character is drawn inside the planar capture, and an environment
        // bound to the live planar target would sample the texture it renders
        // into.
        let planar = &self.planar_fallback_view;
        let environment = self.level_environment();
        let mut ctx = CharacterUploadContext {
            device: &self.device,
            queue: &self.queue,
            cache: &mut self.textures,
            material_layout: &self.material_layout,
            environment_layout: &self.environment_layout,
            caustics: &self.caustics,
            lightmaps: &self.lightmaps,
            environment,
            probes: &probe_views,
            planar,
            probe_fallback: &self.probe_fallback_view,
            planar_fallback: &self.planar_fallback_view,
            level: self.quality,
            lighting_level: self.lighting_quality,
        };
        self.world_characters = Some(WgpuCharacters::upload(&mut ctx, &self.characters));
        if let Some(characters) = self.world_characters.as_ref() {
            let stats = characters.stats();
            logging::info(format!(
                "[wgpu] characters: {} character(s), {} mesh(es), {} draw(s), {} vertices",
                stats.characters, stats.meshes, stats.draws, stats.vertices
            ));
        }
    }

    /// Whether GPU preparation is in progress for the next world.
    #[must_use]
    pub const fn graphics_transition_status(&self) -> GraphicsTransition {
        if self.prepared_install.is_some() {
            GraphicsTransition::Preparing("level resources")
        } else {
            GraphicsTransition::Idle
        }
    }

    /// The graphics configuration the resident world was installed with.
    ///
    /// This is the renderer's ground truth: the engine compares the player's
    /// request against this value to decide whether the last selection has
    /// really been applied.
    #[must_use]
    pub const fn graphics_applied(&self) -> GraphicsSpec {
        self.graphics_applied.spec()
    }

    /// The graphics configuration the latest settings action recorded.
    ///
    /// Setters record it before any work is scheduled; a staged install
    /// snapshots the same value when it starts, so a mismatch with
    /// [`Self::staged_graphics`] means a newer selection superseded the
    /// install.
    #[must_use]
    pub const fn graphics_requested(&self) -> GraphicsSpec {
        self.graphics_requested.spec()
    }

    /// The configuration an in-flight staged install will commit.
    ///
    /// `None` while no GPU upload is staged: the CPU preparation has not taken
    /// its snapshot yet, so it will install whatever is requested when staging
    /// starts.
    #[must_use]
    pub fn staged_graphics(&self) -> Option<GraphicsSpec> {
        self.prepared_install
            .as_ref()
            .map(|pending| pending.graphics.spec())
    }

    /// Abandons an outstanding graphics request and follows the resident
    /// configuration again.
    ///
    /// Used when a transition fails or is superseded by a newer selection: the
    /// renderer must keep rendering exactly what it holds, so the requested
    /// quality, lightmaps and reflections return to what is resident, and the
    /// live quality gate follows them. The engine's own request (the player's
    /// selection) is untouched and is re-scheduled from there.
    ///
    /// Filtering and bloom are zero-resource frame gates the game loop applies
    /// directly from the settings, so they keep their live values rather than
    /// being reverted here.
    pub fn abort_resource_request(&mut self) {
        self.prepared_install = None;
        self.graphics_requested = self.graphics_applied;
        self.graphics_requested.filtering = self.filtering;
        self.graphics_requested.bloom = self.bloom_requested;
        self.quality = self.graphics_applied.quality;
        self.lighting_quality = self.graphics_applied.lighting_quality;
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("install-aborted");
    }

    /// Applies a Reflections setting to the GPU targets and the frame gates.
    ///
    /// Probe cubemaps are created at the setting's face size (or retired for
    /// Off), the planar target is dropped when the setting stops allowing it,
    /// and the frame gates follow. Creating a cubemap does not bake it; the
    /// caller's install or resource preparation bakes.
    fn apply_reflection_targets(&mut self, quality: ReflectionQuality) {
        self.reflections.set_quality(quality);
        self.reflections_enabled = quality.draws_probes();
        if !quality.draws_probes() {
            self.active_plane = None;
            self.active_probe = 0;
        }
        if !self.reflection_targets_match(quality) {
            self.reflection_targets = ReflectionTargets::for_quality(
                &self.device,
                quality,
                &self.reflections.routing.probe_points,
            );
        }
        if !quality.draws_planar() {
            self.reflection_targets.drop_planar();
        }
    }

    /// True when the resident probe cubemaps already match `quality` and the
    /// routing's probe points, so a re-apply does not recreate them.
    ///
    /// The float comparison is exact on purpose: both sides are derived from
    /// the same `probe_bake_position` input by the same code, so a mismatch is
    /// a real change (another level's routing), never rounding noise.
    #[expect(
        clippy::float_cmp,
        reason = "identical inputs re-derive identical bits"
    )] // identical inputs re-derive identical bits
    fn reflection_targets_match(&self, quality: ReflectionQuality) -> bool {
        let face_size = probe_face_size(quality);
        let wanted = if face_size.is_some() {
            self.reflections
                .routing
                .probe_points
                .len()
                .min(crate::engine::render::common::view::MAX_REFLECTION_PROBES)
        } else {
            0
        };
        self.reflection_targets.probes().len() == wanted
            && self
                .reflection_targets
                .probes()
                .iter()
                .zip(self.reflections.routing.probe_points.iter())
                .all(|(probe, point)| {
                    Some(probe.face_size) == face_size
                        && probe.position == probe_bake_position(*point)
                })
    }

    /// Identifies the resident level and its resource configuration.
    pub fn installed_identity(&self) -> (Option<&str>, QualityLevel, LightmapQuality) {
        (
            self.level_id.as_deref(),
            self.installed_quality,
            self.installed_lightmaps,
        )
    }

    /// Applies frame gates immediately when no resident resource changes are owed.
    pub fn apply_frame_graphics(&mut self) -> bool {
        if self.prepared_install.is_some()
            || self
                .graphics_applied
                .delta_from(self.graphics_requested)
                .needs_gpu_work()
        {
            return false;
        }
        self.graphics_applied = self.graphics_requested;
        true
    }

    pub fn install_prepared(
        &mut self,
        loaded: &LoadedLevel,
        build: Arc<LevelBuild>,
        assets: crate::engine::props::PropAssets,
        characters: CharacterScene,
        preserve_playback: bool,
    ) {
        if self.check_device_lost() {
            return;
        }
        let mut textures = self.textures.clone();
        if self.graphics_requested.quality == self.installed_quality {
            textures.begin_level();
        } else {
            textures.release_profile_textures();
        }
        let blacklights = crate::engine::render::common::blacklight::BlacklightSet::from_level(
            &loaded.level,
            &build.lighting,
        );
        let uv_transport = if blacklights.is_empty() {
            None
        } else {
            crate::engine::render::common::light_transport::build_transport_scene(
                &loaded.level,
                &build.mesh,
                &build.batches,
                &loaded.materials,
                &build.lighting,
                &[],
            )
            .map(|(scene, _)| Arc::new(scene))
        };
        self.prepared_install = Some(PreparedInstall {
            loaded: loaded.clone(),
            build,
            assets,
            characters,
            preserve_playback,
            textures,
            graphics: self.graphics_requested,
            materials: MaterialRenderState::from_table(&loaded.materials),
            blacklights,
            uv_transport,
            atlas: None,
            fixture_sheets: Vec::new(),
            world: None,
            world_textures: None,
            world_materials: None,
            props: None,
            prop_cursor: 0,
            phase: UploadPhase::Atlas,
            probes: crate::engine::package::world::ProbeCaptures::default(),
            precompiled_probes: false,
        });
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("install-started");
    }

    pub fn cancel_prepared_install(&mut self) {
        self.prepared_install = None;
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("install-cancelled");
    }

    /// Installs a world whose reflection probes were captured by the compiler.
    ///
    /// The player path: probes are uploaded from the package and never baked.
    /// The install otherwise behaves exactly like [`Self::install_prepared`],
    /// so a level change keeps the same staging, supersession and cache rules.
    pub fn install_prepared_precompiled(
        &mut self,
        loaded: &LoadedLevel,
        build: Arc<LevelBuild>,
        assets: crate::engine::props::PropAssets,
        characters: CharacterScene,
        preserve_playback: bool,
        probes: crate::engine::package::world::ProbeCaptures,
    ) {
        self.install_prepared(loaded, build, assets, characters, preserve_playback);
        if let Some(pending) = self.prepared_install.as_mut() {
            pending.probes = probes;
            pending.precompiled_probes = true;
        }
    }

    /// Advances one upload family, or a bounded batch of prop buffers.
    pub fn advance_prepared_install(&mut self) -> bool {
        if self.check_device_lost() {
            return false;
        }
        let Some(mut pending) = self.prepared_install.take() else {
            return false;
        };
        match pending.phase {
            UploadPhase::Atlas => {
                pending.atlas = Some(self.prepare_atlas(&pending.build));
                pending.phase = UploadPhase::World;
            }
            UploadPhase::World => {
                pending.world = Some(WgpuWorldGeometry::upload_with_blacklight(
                    &self.device,
                    &self.queue,
                    &pending.build.mesh,
                    &pending.materials,
                    pending.uv_transport.as_deref().map(|scene| {
                        crate::engine::render::common::blacklight::BlacklightVisibility {
                            sources: &pending.blacklights,
                            scene,
                            dynamic: None,
                        }
                    }),
                ));
                pending.phase = UploadPhase::Textures;
            }
            UploadPhase::Textures => {
                pending.fixture_sheets = Self::fixture_sheets_with_cache(
                    &self.device,
                    &self.queue,
                    &mut pending.textures,
                    &pending.loaded,
                    pending.graphics.quality,
                );
                if let Some(world) = &pending.world {
                    pending.world_textures = Some(WorldTextures::resolve(
                        &mut pending.textures,
                        &self.device,
                        &self.queue,
                        world.draws(),
                        &pending.materials,
                        &pending.loaded.materials,
                        &pending.fixture_sheets,
                        pending.graphics.quality,
                    ));
                }
                pending.phase = UploadPhase::Materials;
            }
            UploadPhase::Materials => {
                let animations =
                    Self::resolve_animations(&pending.loaded.level, &pending.loaded.materials);
                let reflections: Vec<_> = pending
                    .loaded
                    .materials
                    .entries()
                    .iter()
                    .map(|entry| entry.reflection)
                    .collect();
                let routing =
                    routing_from_mesh(&pending.build.mesh, &reflections, reflections.len());
                if let Some(world) = &pending.world {
                    pending.world_materials = Some(WorldMaterials::resolve(
                        &self.device,
                        &self.queue,
                        &mut pending.textures,
                        &self.material_layout,
                        WorldMaterialInputs {
                            draws: world.draws(),
                            materials: &pending.materials,
                            table: &pending.loaded.materials,
                            level: pending.graphics.quality,
                            lighting: pending.graphics.lighting_quality,
                            animations: &animations.materials,
                            fixture_animations: &animations.fixtures,
                            routing: &routing,
                        },
                    ));
                }
                pending.props = Some(PropUpload::new(
                    pending.characters.claimed_models(),
                    pending.graphics.quality,
                    pending.graphics.lighting_quality,
                ));
                pending.phase = UploadPhase::Props;
            }
            UploadPhase::Props => {
                self.upload_prop_step(&mut pending);
                if pending.prop_cursor >= pending.build.batches.len() {
                    return self.commit_prepared_install(pending);
                }
            }
        }
        self.prepared_install = Some(pending);
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("install-upload");
        false
    }

    /// Share resident handles only for the exact immutable prepared atlas.
    fn prepare_atlas(&self, build: &LevelBuild) -> LightmapAtlas {
        if resident_atlas_matches(
            self.lightmaps.is_resident(),
            self.lightmap_cpu.as_ref(),
            build.lightmaps.as_ref(),
        ) {
            logging::info(format!(
                "[lightmaps] reuse resident immutable atlas: {} bytes; no upload",
                self.lightmaps.stats().resident_bytes
            ));
            self.lightmaps.clone()
        } else {
            LightmapAtlas::upload(&self.device, &self.queue, build.lightmaps.as_deref())
        }
    }

    fn upload_prop_step(&self, pending: &mut PreparedInstall) {
        let began = std::time::Instant::now();
        while let Some(batch) = pending.build.batches.get(pending.prop_cursor) {
            if let Some(props) = &mut pending.props {
                props.push_batch_with_blacklight(
                    &self.device,
                    &self.queue,
                    &mut pending.textures,
                    &self.material_layout,
                    batch,
                    pending.uv_transport.as_deref().map(|scene| {
                        crate::engine::render::common::blacklight::BlacklightVisibility {
                            sources: &pending.blacklights,
                            scene,
                            dynamic: None,
                        }
                    }),
                );
            }
            pending.prop_cursor = pending.prop_cursor.saturating_add(1);
            if began.elapsed() >= std::time::Duration::from_millis(4) {
                break;
            }
        }
    }

    fn commit_prepared_install(&mut self, pending: PreparedInstall) -> bool {
        let PreparedInstall {
            loaded,
            build,
            assets,
            mut characters,
            preserve_playback,
            textures,
            graphics,
            atlas,
            fixture_sheets,
            world,
            world_textures,
            world_materials,
            props,
            probes,
            precompiled_probes,
            materials: _,
            blacklights,
            uv_transport,
            prop_cursor: _,
            phase: _,
        } = pending;
        let (
            Some(prepared_atlas),
            Some(prepared_world),
            Some(prepared_textures),
            Some(prepared_materials),
            Some(prepared_props),
        ) = (atlas, world, world_textures, world_materials, props)
        else {
            self.fatal = Some("Incomplete GPU level preparation".to_string());
            return false;
        };
        let inherit_playback =
            preserve_playback && self.level_id.as_deref() == Some(loaded.level.id.as_str());
        if inherit_playback {
            characters.inherit_playback_from(&mut self.characters);
        }
        let animation_seconds = self.animation_seconds;
        self.prop_catalog = loaded.catalog.as_ref().clone();
        self.prop_assets = assets;
        self.textures = textures;
        self.installed_quality = graphics.quality;
        self.quality = graphics.quality;
        self.lighting_quality = graphics.lighting_quality;
        let uploaded = UploadedLevel {
            atlas: prepared_atlas,
            fixture_sheets,
            world: prepared_world,
            world_textures: prepared_textures,
            world_materials: prepared_materials,
            props: prepared_props.finish(),
        };
        self.blacklights = blacklights;
        self.dynamic_transport = uv_transport;
        let installed_build = self.install_level_prepared(
            &loaded,
            build,
            graphics.lightmaps,
            graphics.reflections,
            inherit_playback,
            true,
            std::time::Instant::now(),
            Some((characters, uploaded)),
            precompiled_probes.then_some(probes),
        );
        if inherit_playback {
            self.animation_seconds = animation_seconds;
        }
        self.retained_build = Some(installed_build);
        self.installed_lightmaps = graphics.lightmaps;
        self.graphics_applied = graphics;
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("install-committed");
        true
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one cohesive level upload: upload and install"
    )] // one cohesive level upload: upload and install
    #[expect(
        clippy::too_many_arguments,
        reason = "one install seam: world, quality and staged GPU state"
    )] // one install seam: world, quality and staged GPU state
    fn install_level_prepared(
        &mut self,
        loaded: &LoadedLevel,
        mut build: Arc<LevelBuild>,
        lightmaps_quality: LightmapQuality,
        reflections_quality: ReflectionQuality,
        preserve_level_state: bool,
        upload_atlas: bool,
        started: std::time::Instant,
        prepared: Option<(CharacterScene, UploadedLevel)>,
        precompiled_probes: Option<crate::engine::package::world::ProbeCaptures>,
    ) -> Arc<LevelBuild> {
        self.invalidate_frame();
        let (characters, uploaded) = prepared.map_or((None, None), |(characters, uploaded)| {
            (Some(characters), Some(uploaded))
        });

        let preuploaded = uploaded.is_some();
        let (
            uploaded_atlas,
            uploaded_sheets,
            uploaded_world,
            uploaded_textures,
            uploaded_materials,
            uploaded_props,
        ) = uploaded.map_or((None, None, None, None, None, None), |uploaded_level| {
            (
                Some(uploaded_level.atlas),
                Some(uploaded_level.fixture_sheets),
                Some(uploaded_level.world),
                Some(uploaded_level.world_textures),
                Some(uploaded_level.world_materials),
                Some(uploaded_level.props),
            )
        });
        // Only a graphics transaction inherits mutable level state. Explicit
        // reloads can replace content while retaining the same authored id.
        let level_changed =
            !preserve_level_state || self.level_id.as_deref() != Some(loaded.level.id.as_str());
        let quality_changed = self.quality != self.installed_quality;
        if quality_changed && !preuploaded {
            self.textures.release_profile_textures();
        } else if level_changed && !preuploaded {
            self.textures.begin_level();
        }
        let options = LightmapBuildOptions::for_lightmaps(lightmaps_quality);
        if upload_atlas {
            let mut atlas = uploaded_atlas.unwrap_or_else(|| {
                LightmapAtlas::upload(&self.device, &self.queue, build.lightmaps.as_deref())
            });
            // A plan or fill failure never reaches this branch: the neutral build
            // has already returned a vertex-lit mesh with the same baked
            // lighting (`build.lightmaps.is_none()`). A missing atlas therefore
            // does not imply another bake is needed.
            // Rebuilding again with `LightmapMode::Off` here would re-bake with
            // `BakeConfig::HARD` and change every vertex colour. Only an upload
            // failure rebuilds, and the wgpu upload cannot fail; the check stays
            // as a defensive fallback so a future failure cannot
            // draw an atlas-less lightmapped mesh.
            if needs_upload_fallback(options.mode, build.lightmaps.is_some(), atlas.is_resident()) {
                let mut fallback = build_level_geometry_timed_with_lightmaps(
                    &loaded.level,
                    &self.prop_catalog,
                    &mut self.prop_assets,
                    &loaded.materials,
                    LightmapBuildOptions::for_lightmap_quality(
                        lightmaps_quality,
                        LightmapMode::Off,
                    ),
                    None,
                );
                fallback.lightmap_failure = Some(LightmapFailure::Upload);
                build = Arc::new(fallback);
                atlas = LightmapAtlas::upload(&self.device, &self.queue, None);
            }
            self.lightmaps = atlas;
            self.lightmaps_resident = self.lightmaps.is_resident();
        }
        let lightmap_failure = build.lightmap_failure;
        let lightmap_stats = build.lightmaps.as_deref().map(|lightmaps| lightmaps.stats);
        let blacklights = crate::engine::render::common::blacklight::BlacklightSet::from_level(
            &loaded.level,
            &build.lighting,
        );
        let uv_transport = if blacklights.is_empty() {
            None
        } else if preuploaded {
            self.dynamic_transport.clone()
        } else {
            crate::engine::render::common::light_transport::build_transport_scene(
                &loaded.level,
                &build.mesh,
                &build.batches,
                &loaded.materials,
                &build.lighting,
                &[],
            )
            .map(|(scene, _)| Arc::new(scene))
        };
        let uv_visibility = uv_transport.as_deref().map(|scene| {
            crate::engine::render::common::blacklight::BlacklightVisibility {
                sources: &blacklights,
                scene,
                dynamic: None,
            }
        });
        let materials = MaterialRenderState::from_table(&loaded.materials);
        let fixture_sheets = uploaded_sheets.unwrap_or_else(|| self.upload_fixture_sheets(loaded));
        let world = uploaded_world.unwrap_or_else(|| {
            WgpuWorldGeometry::upload_with_blacklight(
                &self.device,
                &self.queue,
                &build.mesh,
                &materials,
                uv_visibility,
            )
        });
        let world_textures = uploaded_textures.unwrap_or_else(|| {
            WorldTextures::resolve(
                &mut self.textures,
                &self.device,
                &self.queue,
                world.draws(),
                &materials,
                &loaded.materials,
                &fixture_sheets,
                self.quality,
            )
        });
        let animations = Self::resolve_animations(&loaded.level, &loaded.materials);
        // The level's reflection routing comes from the emitted geometry and
        // the material table.
        let reflections_vec: Vec<crate::engine::materials::MaterialReflection> = loaded
            .materials
            .entries()
            .iter()
            .map(|entry| entry.reflection)
            .collect();
        let routing = routing_from_mesh(&build.mesh, &reflections_vec, reflections_vec.len());
        self.reflections.routing = routing;
        // The reflection sources follow the Reflections setting, never the
        // overall quality level: a lightmap-only rebuild keeps the probe
        // cubemaps it already baked, and Off retires them. This is idempotent,
        // so an install may call it even when the setting itself did not change.
        self.apply_reflection_targets(reflections_quality);
        self.active_plane = None;
        let world_materials = uploaded_materials.unwrap_or_else(|| {
            WorldMaterials::resolve(
                &self.device,
                &self.queue,
                &mut self.textures,
                &self.material_layout,
                WorldMaterialInputs {
                    draws: world.draws(),
                    materials: &materials,
                    table: &loaded.materials,
                    level: self.quality,
                    lighting: self.lighting_quality,
                    animations: &animations.materials,
                    fixture_animations: &animations.fixtures,
                    routing: &self.reflections.routing,
                },
            )
        });
        self.log_level_resolution(&world, &world_textures, &world_materials);
        // Claim every placed skinned prop for the character path before the
        // static prop upload: the GPU prop batches of a claimed model are
        // suppressed so the bind pose and the animated pose never draw on top
        // of each other. The neutral batches stay in `build`, so the lightmap
        // bake and its occlusion are untouched.
        self.characters = characters.unwrap_or_else(|| {
            CharacterScene::spawn_characters_with_field(
                &loaded.level,
                &self.prop_catalog,
                &mut self.prop_assets,
                &build.lighting,
                build.probes.as_deref(),
            )
        });
        let claimed_characters = self.characters.claimed_models().to_vec();
        let world_props = uploaded_props.unwrap_or_else(|| {
            WgpuProps::upload_with_blacklight(
                &self.device,
                &self.queue,
                &mut self.textures,
                &self.material_layout,
                &build.batches,
                &claimed_characters,
                self.quality,
                self.lighting_quality,
                uv_visibility,
            )
        });
        let prop_stats = world_props.stats();
        let world_stats = world.stats();
        let lightmap_upload = self.lightmaps.stats();
        self.level_stats = LevelBuildStats {
            static_vertices: build.mesh.vertex_count,
            static_indices: build.mesh.index_count,
            prop_vertices: prop_stats.vertices,
            prop_indices: prop_stats.indices,
            prop_draws: prop_stats.draws,
            static_batches: world_stats.draws,
            static_chunks: world_stats.chunks,
            prop_chunks: prop_stats.chunks,
            vbo_bytes: world_stats
                .vertex_bytes
                .saturating_add(prop_stats.vertices.saturating_mul(
                    usize::try_from(super::world::WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX),
                )),
            index_bytes: world_stats.index_bytes.saturating_add(
                prop_stats
                    .indices
                    .saturating_mul(std::mem::size_of::<u16>()),
            ),
            build_millis: started.elapsed().as_secs_f64() * 1000.0,
            lighting_millis: build.timings.lighting_millis,
            props_millis: build.timings.props_millis,
            surfaces_millis: build.timings.surfaces_millis,
            lighting: build.lighting.summary(),
            lightmap_pages: lightmap_stats.map_or(0, |stats| stats.pages),
            lightmap_texels: lightmap_stats.map_or(0, |stats| stats.page_texels),
            lightmap_charts: lightmap_stats.map_or(0, |stats| stats.charts),
            lightmap_chart_texels: lightmap_stats.map_or(0, |stats| stats.texels),
            lightmap_millis: if lightmap_upload.cache_hit {
                0.0
            } else {
                build.lightmap_millis
            },
            lightmap_fallback: lightmap_failure.is_some(),
        };
        self.fixture_sheets = fixture_sheets;
        self.material_animations = animations;
        // Water and weather retain their shared world phase across live settings changes.
        if level_changed {
            self.animation_seconds = 0.0;
        }
        self.blacklights = blacklights;
        self.flashlight_characters.get_mut().clear();
        self.visibility_materials = Some(loaded.materials.clone());
        self.dynamic_lighting = Some(build.lighting.clone());
        self.entity_lighting_revision = self.entity_lighting_revision.wrapping_add(1);
        self.dynamic_field.clone_from(&build.probes);
        self.dynamic_transport = uv_transport.or_else(|| {
            build.probes.as_ref().and_then(|field| {
                let _selected_direct = field.local_direct.as_ref()?;
                crate::engine::render::common::light_transport::build_transport_scene(
                    &loaded.level,
                    &build.mesh,
                    &build.batches,
                    &loaded.materials,
                    &build.lighting,
                    &[],
                )
                .map(|(scene, _)| std::sync::Arc::new(scene))
            })
        });
        // The GPU side of the previous level's dynamic scene dies with it. The
        // neutral scene is level content too: a graphics rebuild keeps its
        // objects, while a reload starts from the newly installed definition,
        // including when its id matches the previous version.
        if level_changed {
            self.dynamic.clear_all();
            self.dynamic_visibility =
                crate::engine::render::common::dynamic_visibility::DynamicVisibility::new();
            self.dynamic_visibility_error = None;
            // A key spawned against the previous level must never resolve to
            // an object of the new one.
            self.runtime_objects.clear();
            self.effects.clear_all();
            self.world_effects = None;
            self.level_id = Some(loaded.level.id.clone());
            // The runtime spawn seams are keyed by instance id and name no
            // level, so the installed definition is retained for them: a
            // runtime placement resolves its walkable floor exactly like a
            // placed prop's.
            self.installed_level = Some(loaded.level.clone());
            // The door models need the resolved material table, which is only
            // in scope here; they are kept for `spawn_doors`, which re-runs
            // whenever the neutral scene is wiped (a demo respawn).
            self.prepare_door_sources(&loaded.level, &loaded.materials);
            // The effect emitters resolve through the same table: their images,
            // tiling and opacity are level-scoped, so they are built here and
            // dropped at the next level change.
            self.effects = EffectScene::build(&loaded.level, &loaded.materials);
        }
        self.spawn_doors();
        let _update_stats = self.refresh_dynamic_lighting(0.0);
        if let Some(lighting) = self.dynamic_lighting.as_ref() {
            self.characters.refresh_lighting_with_visibility(
                lighting,
                self.dynamic_field.as_deref(),
                self.dynamic_transport.as_deref(),
                self.dynamic_transport
                    .as_ref()
                    .filter(|_| self.dynamic_visibility_error.is_none())
                    .map(|_| &self.dynamic_visibility),
            );
        }
        self.world_dynamic = None;
        // The previous level's (or previous quality's) character GPU state is
        // dropped before the probe bake so the bake cannot draw stale
        // characters; `upload_characters_gpu` rebuilds it after the probes.
        self.world_characters = None;
        // The level's regional fog travels in its semantics: resolve it once
        // per install, before the group-3 bindings are created, so every draw's
        // environment carries the same set. A level with no regions resolves to
        // the global shipped atmosphere and an empty list.
        self.fog = LevelFog::from_level(&loaded.level);
        // The prepared CPU pages and the live lighting field the environment's
        // lightmap selection is derived from, installed before the group-3
        // bindings are created so they carry the level's real page and
        // switchable state.
        self.lightmap_cpu.clone_from(&build.lightmaps);
        // The packaged chain's top level is the shader's roughness bound; a
        // developer install bakes level 0 only and leaves it at zero. Set
        // before the environment bindings are created so the first uniform
        // already carries it.
        self.probe_max_mip = packaged_probe_max_mip(
            precompiled_probes.as_ref(),
            self.reflection_targets
                .probes()
                .first()
                .map(|probe| probe.face_size),
        );
        self.caustics = super::caustics::Caustics::upload(
            &self.device,
            &self.queue,
            &mut self.textures,
            &loaded.level,
            &build.lighting,
            self.quality,
        );
        self.caustics.update(&self.queue, self.animation_seconds);
        self.environment = Some(self.create_environment());
        self.world = Some(world);
        self.world_props = Some(world_props);
        self.world_textures = Some(world_textures);
        self.world_materials = Some(world_materials);
        // Each switchable fixture's runtime bindings, refreshed with every
        // install. This runs after the world and its materials exist, because
        // the face slot is a property of the uploaded draw set; the light
        // enable state itself lives in `dynamic_lighting`.
        self.prepare_switchable_lights(&loaded.level);
        // The decal pass runs inside the scene body, so its pipeline (and the
        // reflection-format one) must exist before any probe bake.
        self.ensure_world_pipeline();
        self.upload_decals(&build.mesh, &loaded.level);
        self.prepare_static_light_bank(&build, loaded);
        self.upload_sky(&loaded.level);
        // Reflection probes are already captured for a compiled world; a
        // developer-tool install (the offline compiler itself) bakes them here,
        // when the world, its textures, its materials and its decals are all
        // resident.
        if let Some(probes) = precompiled_probes {
            self.packaged_probes = Some(probes.clone());
            self.upload_packaged_probes(&probes);
        } else {
            self.packaged_probes = None;
            self.bake_reflection_probes();
        }
        // Characters and dynamic objects upload after the probe bake for the
        // same reason: their group-3 environments sample the probe cubemaps,
        // and binding those while they are being captured is a validation
        // error. A quality rebuild keeps the neutral character scene; the GPU
        // side is re-uploaded against the new level resources.
        self.upload_characters_gpu();
        // A quality rebuild keeps the neutral dynamic scene; re-upload it
        // against the new level resources so no draw references a stale
        // resource.
        self.upload_dynamic();
        // The effect scene rides the same lifecycle: its GPU buffers and
        // sheets are rebuilt against the resources just installed, while the
        // neutral emitters survive a quality rebuild of the same level.
        self.upload_effects();
        self.installed_quality = self.quality;
        #[cfg(feature = "visual-diagnostics")]
        self.refresh_probe_visualization();
        build
    }

    /// Static uses its compiler-prepared alternate, with shared image caches.
    fn prepare_static_light_bank(&mut self, build: &LevelBuild, loaded: &LoadedLevel) {
        self.static_light_bank = None;
        let Some(state) = &build.static_light_off else {
            return;
        };
        let materials = MaterialRenderState::from_table(&loaded.materials);
        let visibility = self.dynamic_transport.as_deref().map(|scene| {
            crate::engine::render::common::blacklight::BlacklightVisibility {
                sources: &self.blacklights,
                scene,
                dynamic: None,
            }
        });
        let world = WgpuWorldGeometry::upload_with_blacklight(
            &self.device,
            &self.queue,
            &state.mesh,
            &materials,
            visibility,
        );
        let textures = WorldTextures::resolve(
            &mut self.textures,
            &self.device,
            &self.queue,
            world.draws(),
            &materials,
            &loaded.materials,
            &self.fixture_sheets,
            self.quality,
        );
        let animations = Self::resolve_animations(&loaded.level, &loaded.materials);
        let world_materials = WorldMaterials::resolve(
            &self.device,
            &self.queue,
            &mut self.textures,
            &self.material_layout,
            WorldMaterialInputs {
                draws: world.draws(),
                materials: &materials,
                table: &loaded.materials,
                level: self.quality,
                lighting: self.lighting_quality,
                animations: &animations.materials,
                fixture_animations: &animations.fixtures,
                routing: &self.reflections.routing,
            },
        );
        let props = WgpuProps::upload_with_blacklight(
            &self.device,
            &self.queue,
            &mut self.textures,
            &self.material_layout,
            &state.batches,
            self.characters.claimed_models(),
            self.quality,
            self.lighting_quality,
            visibility,
        );
        let vertex_bytes =
            world
                .stats()
                .vertex_bytes
                .saturating_add(props.stats().vertices.saturating_mul(
                    usize::try_from(super::world::WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX),
                ));
        let index_bytes = world.stats().index_bytes.saturating_add(
            props
                .stats()
                .indices
                .saturating_mul(std::mem::size_of::<u16>()),
        );
        let main_decals = self.decals.take();
        self.upload_decals(&state.mesh, &loaded.level);
        let off_decals = self.decals.take();
        self.decals = main_decals;
        logging::info(format!(
            "[static-light-state] prepared {} fixtures; alternate geometry {vertex_bytes} vertex bytes, {index_bytes} index bytes; images shared",
            state.fixtures.len()
        ));
        self.static_light_bank = Some(StaticLightBank {
            fixtures: state.fixtures.clone(),
            off_active: false,
            world: Some(world),
            props: Some(props),
            textures: Some(textures),
            materials: Some(world_materials),
            decals: off_decals,
        });
    }

    fn select_static_light_state(&mut self, toggles: &[(usize, bool)]) {
        let Some(bank) = self.static_light_bank.as_mut() else {
            return;
        };
        let Some((_, enabled)) = toggles
            .iter()
            .find(|(fixture, _)| bank.fixtures.contains(fixture))
        else {
            return;
        };
        if bank.off_active != *enabled {
            return;
        }
        std::mem::swap(&mut self.world, &mut bank.world);
        std::mem::swap(&mut self.world_props, &mut bank.props);
        std::mem::swap(&mut self.world_textures, &mut bank.textures);
        std::mem::swap(&mut self.world_materials, &mut bank.materials);
        std::mem::swap(&mut self.decals, &mut bank.decals);
        bank.off_active = !enabled;
        logging::info(format!(
            "[static-light-state] circuit {}",
            if *enabled { "on" } else { "off" }
        ));
        // Draw topology can merge differently in each prepared state. Resolve
        // fixture face slots against the selected bank, then replay all states.
        if let Some(level) = self.installed_level.take() {
            self.prepare_switchable_lights(&level);
            self.installed_level = Some(level);
        }
        if let (Some(lighting), Some(materials)) =
            (&self.dynamic_lighting, &mut self.world_materials)
        {
            for binding in &self.switchable_lights {
                let face_enabled = lighting
                    .lights()
                    .get(binding.light_index)
                    .is_some_and(crate::engine::lighting::BakedLight::is_active);
                if let Some(slot) = binding.material_slot {
                    let _updated = materials.set_emission_scale(
                        &self.queue,
                        slot,
                        if face_enabled { 1.0 } else { 0.0 },
                    );
                }
                if let Some(props) = self.world_props.as_mut() {
                    props.set_light_enabled(
                        &self.queue,
                        u32::try_from(binding.fixture_index).unwrap_or(u32::MAX),
                        face_enabled,
                    );
                }
            }
        }
    }

    /// Uploads the level's decal batches against the current pipelines.
    fn upload_decals(
        &mut self,
        mesh: &crate::engine::render::common::mesh::LevelMesh,
        level: &crate::engine::level::LevelDef,
    ) {
        let Some(pipeline) = self.decal_pipeline.as_ref() else {
            return;
        };
        let asset_root = crate::engine::assets::resolve_asset_root();
        let decals = WgpuDecals::upload(
            &self.device,
            &self.queue,
            pipeline.sheet_layout(),
            &self.textures,
            super::decals::DecalUploadInputs {
                mesh,
                level,
                catalog: self.prop_catalog.assets(),
                asset_root: asset_root.as_deref(),
            },
            self.quality,
        );
        logging::info(format!(
            "[wgpu] decals: {} draw(s), {} chunk(s), {} sheet(s) ({} external, {} diagnostic)",
            decals.stats().draws,
            decals.stats().chunks,
            decals.stats().sheets,
            decals.stats().external_sheets,
            decals.stats().diagnostic_sheets,
        ));
        self.decals = Some(decals);
    }

    /// Resolves and uploads the level's optional sky sheet.
    ///
    /// A level without a `sky` clears the pass (`self.sky = None`), which is
    /// the historical clear-colour background.
    fn upload_sky(&mut self, level: &crate::engine::level::LevelDef) {
        let asset_root = crate::engine::assets::resolve_asset_root();
        let sky = WgpuSky::upload(
            &self.device,
            &self.queue,
            &mut self.textures,
            self.prop_catalog.assets(),
            asset_root.as_deref(),
            level,
            self.quality,
        );
        self.sky = sky;
    }

    /// The environment uniform's lightmap selection: pages per layer group,
    /// the switchable group count and the live on/off mask, from the resident
    /// CPU pages and the current baked lighting.
    ///
    /// Bit `g` is set when switchable group `g`'s light is active. The mask is
    /// four bits because the shader's packed word is; the atlas upload rejects
    /// a set with more groups, so a group beyond the mask cannot exist on the
    /// resident path.
    fn lightmap_selection(&self) -> (u32, u32, u32) {
        let Some(cpu) = self.lightmap_cpu.as_ref() else {
            return (0, 0, 0);
        };
        let page_count = u32::try_from(cpu.pages.len()).unwrap_or(u32::MAX);
        let switchable_count = u32::try_from(cpu.switchable.len()).unwrap_or(u32::MAX);
        let mut mask = 0u32;
        for (group, contribution) in cpu.switchable.iter().enumerate() {
            if group >= 4 {
                break;
            }
            let active = self
                .dynamic_lighting
                .as_ref()
                .and_then(|lighting| lighting.lights().get(contribution.light_index))
                .is_some_and(crate::engine::lighting::BakedLight::is_active);
            if active {
                mask |= 1u32 << group;
            }
        }
        (page_count, switchable_count, mask)
    }

    /// The level environment every shared uniform is derived from: unit light
    /// scale, the resident lightmap selection, the resident probe chain's top
    /// mip, the level's fog (global plus the preset's prefix of its regions)
    /// and no active mirror.
    ///
    /// The static world, the props and every per-object dynamic/character
    /// uniform build from this, so a light switch, a probe chain change and a
    /// quality preset change reach all of them at once. The region count is a
    /// uniform word recomputed here from the already-resolved level fog, so a
    /// preset switch recovers the fuller set without a rebuild.
    fn level_environment(&self) -> EnvironmentUniform {
        let (page_count, switchable_count, mask) = self.lightmap_selection();
        let fog_regions = self.fog.uploaded_region_count(self.quality);
        let mut result = static_environment(self.lightmaps_resident, self.fog.global)
            .with_fog_regions(&self.fog.regions, fog_regions)
            .with_lightmaps(page_count, switchable_count, mask)
            .with_probe_mips(self.probe_max_mip);
        result.storm = self
            .effects
            .snow()
            .map_or_else(Default::default, |snow| snow.storm);
        let (shadows, count) = self.contact_shadow_subjects();
        result = result.with_contact_shadows(shadows.get(..count).unwrap_or(&[]));
        result = result.with_blacklights(&self.blacklights, self.dynamic_lighting.as_ref());
        result
    }

    fn contact_shadow_subjects(
        &self,
    ) -> (
        [crate::engine::render::common::dynamic_lights::ContactShadowUniform; 8],
        usize,
    ) {
        let mut shadows =
            [crate::engine::render::common::dynamic_lights::ContactShadowUniform::ZERO; 8];
        let Some(level) = self.installed_level.as_ref() else {
            return (shadows, 0);
        };
        let surfaces = crate::engine::level::LevelSurfaces::new(level);
        let dynamic =
            self.dynamic
                .objects()
                .iter()
                .filter(|object| object.visible())
                .filter(|object| {
                    object.mesh().submeshes.iter().any(|submesh| {
                        submesh.alpha.mode != crate::engine::materials::AlphaMode::Blend
                    })
                })
                .map(|object| {
                    (
                        object.world_bounds(),
                        object.entity_lighting().and_then(|sample| sample.prepared),
                    )
                });
        let characters =
            self.characters
                .characters()
                .iter()
                .chain(self.characters.runtime_characters())
                .filter(|character| {
                    character.asset().model.submeshes.iter().any(|submesh| {
                        submesh.alpha.mode != crate::engine::materials::AlphaMode::Blend
                    })
                })
                .map(|character| {
                    (
                        character.contact_bounds(),
                        character.entity_lighting().prepared,
                    )
                });
        let subjects = dynamic
            .chain(characters)
            .filter_map(|(bounds, sample)| {
                let [x, _, z] = bounds.centre();
                let floor = surfaces.floor_y_at(x, z)?;
                crate::engine::render::common::dynamic_lights::contact_shadow(
                    bounds,
                    floor,
                    sample.map_or([0.0, 1.0, 0.0], |texel| texel.direction),
                )
            })
            .take(8);
        let mut count: usize = 0;
        for (slot, subject) in shadows.iter_mut().zip(subjects) {
            *slot = subject;
            count = count.saturating_add(1);
        }
        (shadows, count)
    }

    /// Creates the group-3 environment bindings from the current atlas, probe
    /// cubemaps and planar target.
    fn create_environment(&self) -> EnvironmentBindings {
        let probe_views: Vec<&wgpu::TextureView> = self
            .reflection_targets
            .probes()
            .iter()
            .map(ProbeCube::view)
            .collect();
        let planar = self
            .reflection_targets
            .planar()
            .map_or(&self.planar_fallback_view, |planar| planar.view());
        EnvironmentBindings::new(
            &self.device,
            &self.queue,
            &self.environment_layout,
            &self.textures,
            &self.lightmaps,
            &probe_views,
            planar,
            &self.probe_fallback_view,
            &self.planar_fallback_view,
            &self.caustics,
            &self.level_environment(),
        )
    }

    /// Rebuilds the environment bindings after a sampler-affecting change.
    fn refresh_environment(&mut self) {
        if self.world.is_some() {
            self.environment = Some(self.create_environment());
        }
    }

    /// Uploads one clamped GPU sheet per fixture family the level uses.
    ///
    /// Catalog sheets use the texture cache's bounded cross-level retention;
    /// pack sheets are scoped to the level. A family the level does not use keeps the
    /// shared white fallback.
    fn upload_fixture_sheets(
        &mut self,
        loaded: &LoadedLevel,
    ) -> Vec<std::sync::Arc<super::texture::GpuTexture>> {
        Self::fixture_sheets_with_cache(
            &self.device,
            &self.queue,
            &mut self.textures,
            loaded,
            self.quality,
        )
    }

    fn fixture_sheets_with_cache(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut TextureCache,
        loaded: &LoadedLevel,
        quality: QualityLevel,
    ) -> Vec<Arc<super::texture::GpuTexture>> {
        // Family sheets occupy the first slots; switched and animated faces
        // have private slots so their response cannot change another asset.
        let families = crate::engine::lighting::FixtureKind::ALL.len();
        let highest = loaded
            .light_sheets
            .iter()
            .map(|sheet| usize::try_from(sheet.slot).unwrap_or(usize::MAX))
            .max()
            .map_or(families, |slot| slot.saturating_add(1));
        let mut slots: Vec<std::sync::Arc<super::texture::GpuTexture>> =
            Vec::with_capacity(highest);
        for _ in 0..highest {
            slots.push(textures.fallback());
        }
        for sheet in &loaded.light_sheets {
            let Some(slot) = slots.get_mut(usize::try_from(sheet.slot).unwrap_or(usize::MAX))
            else {
                continue;
            };
            let (_, texture) = textures.get_or_upload_fitted(
                device,
                queue,
                &sheet.key,
                sheet.image.as_ref(),
                TextureClass::FixtureFace,
                sheet.origin,
                quality,
            );
            *slot = texture;
        }
        slots
    }

    /// Resolves authored material IDs and fixture asset IDs into their own
    /// numeric namespaces. An animated fixture has a private face slot, even
    /// without a switch, so other assets in its family keep their own response.
    ///
    /// A level that declares none leaves every entry `None` (the identity
    /// multiplier).
    fn resolve_animations(
        level: &crate::engine::level::LevelDef,
        materials: &crate::engine::materials::MaterialTable,
    ) -> LevelEmissionAnimations {
        let mut animations = LevelEmissionAnimations {
            materials: vec![None; materials.entries().len()],
            fixtures: vec![
                None;
                usize::try_from(crate::engine::level::FIXTURE_SWITCHABLE_MATERIAL_BASE)
                    .unwrap_or(usize::MAX)
                    .saturating_add(level.ceiling_lights.len())
            ],
        };
        for animation in &level.animated_emissions {
            let authored_id = animation.material.trim();
            let effect = animation
                .effect
                .as_deref()
                .and_then(AnimationEffect::parse)
                .unwrap_or(AnimationEffect::Pulse);
            let resolved = EmissionAnimation {
                effect,
                hz: animation.hz.unwrap_or_else(|| effect.default_hz()),
                depth: animation.depth.unwrap_or_else(|| effect.default_depth()),
                phase: animation.phase.unwrap_or(0.0),
            }
            .sanitized();
            let active = resolved.is_active().then_some(resolved);
            if let Some(index) = materials.index_of(authored_id)
                && let Some(slot) = animations
                    .materials
                    .get_mut(usize::try_from(index).unwrap_or(usize::MAX))
            {
                *slot = active;
            }
            for (index, fixture) in level.ceiling_lights.iter().enumerate() {
                if fixture.fixture.trim() != authored_id {
                    continue;
                }
                let face_slot = crate::engine::level::fixture_face_material_index(
                    index,
                    true,
                    crate::engine::lighting::fixture_profile(&fixture.fixture).kind,
                );
                if let Some(slot) = animations
                    .fixtures
                    .get_mut(usize::try_from(face_slot).unwrap_or(usize::MAX))
                {
                    *slot = active;
                }
            }
        }
        animations
    }

    /// Emits the three once-per-level resolution diagnostics: world geometry,
    /// textures and materials. Never per frame.
    fn log_level_resolution(
        &self,
        world: &WgpuWorldGeometry,
        textures: &WorldTextures,
        materials: &WorldMaterials,
    ) {
        let world_stats = world.stats();
        logging::info(format!(
            "[wgpu] world upload: {} vertices, {} indices, {} draws in {} chunk(s) \
             (neutral mesh: {} vertices, {} indices, {} ranges)",
            world_stats.uploaded_vertices,
            world_stats.uploaded_indices,
            world_stats.draws,
            world_stats.chunks,
            world_stats.mesh_vertices,
            world_stats.mesh_indices,
            world_stats.mesh_ranges,
        ));
        let texture_stats = textures.stats();
        logging::info(format!(
            "[wgpu] textures: {} unique, {} uploaded, {} cache hits, {} fallbacks, \
             {} missing of {} draws ({} bytes resident, max edge {}px, {} filtering)",
            texture_stats.unique,
            texture_stats.uploads,
            texture_stats.cache_hits,
            texture_stats.fallback_draws,
            texture_stats.missing,
            texture_stats.draws,
            texture_stats.resident_bytes,
            texture_stats.max_edge,
            self.filtering.name(),
        ));
        let material_stats = materials.stats();
        logging::info(format!(
            "[wgpu] materials: {} resolved ({} response, {} reflection-eligible), \
             {} normal maps ({} uploaded, {} cache hits), \
             {} opaque / {} cutout / {} translucent of {} draws ({} response, {} level)",
            material_stats.materials,
            material_stats.response_materials,
            material_stats.reflection_eligible,
            material_stats.normal_maps,
            material_stats.normal_uploads,
            material_stats.normal_cache_hits,
            material_stats.opaque_draws,
            material_stats.cutout_draws,
            material_stats.translucent_draws,
            world_stats.draws,
            if self.lighting_quality.draws_surface_response() {
                "enabled"
            } else {
                "disabled"
            },
            self.quality.name(),
        ));
    }

    /// Enables or disables the CPU frustum test on the world draw ranges.
    ///
    /// The benchmark's no-cull switch; face culling in the pipeline is a
    /// deliberate convention and is never disabled by this.
    pub const fn set_culling(&mut self, enabled: bool) {
        self.culling = enabled;
    }

    /// Selects a development view without touching authored or prepared data.
    #[cfg(feature = "visual-diagnostics")]
    pub fn set_visual_diagnostic(&mut self, mode: super::diagnostics::VisualDiagnosticMode) {
        self.visual_diagnostic = mode;
        self.refresh_probe_visualization();
        self.trace_visual_diagnostic("select", None);
    }

    #[cfg(feature = "visual-diagnostics")]
    fn refresh_probe_visualization(&mut self) {
        use super::diagnostics::VisualDiagnosticMode;
        self.probe_visualization = None;
        if !matches!(
            self.visual_diagnostic,
            VisualDiagnosticMode::ProbeField | VisualDiagnosticMode::ProbeNeighborhood
        ) {
            return;
        }
        let (Some(field), Some(lighting)) = (
            self.dynamic_field.as_deref(),
            self.dynamic_lighting.as_ref(),
        ) else {
            return;
        };
        let anchor = (self.visual_diagnostic == VisualDiagnosticMode::ProbeNeighborhood)
            .then(|| {
                self.dynamic
                    .objects()
                    .first()
                    .map(|object| object.centre().to_array())
            })
            .flatten();
        let mesh = super::probe_visualization::mesh(
            field,
            lighting,
            anchor,
            self.dynamic_transport
                .as_ref()
                .filter(|_| self.dynamic_visibility_error.is_none())
                .map(|_| &self.dynamic_visibility),
            self.dynamic
                .objects()
                .first()
                .map(crate::engine::render::DynamicObject::id),
        );
        self.probe_visualization = Some(WgpuWorldGeometry::upload(
            &self.device,
            &self.queue,
            &mesh,
            &MaterialRenderState::default(),
        ));
    }

    #[cfg(feature = "visual-diagnostics")]
    fn encode_probe_visualization<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a WorldPipeline,
        textures: &'a WorldTextures,
        materials: &'a WorldMaterials,
        environment: &'a EnvironmentBindings,
        frame: &'a super::world::WorldFrame,
    ) -> usize {
        self.probe_visualization.as_ref().map_or(0, |geometry| {
            pipeline
                .encode(
                    pass,
                    super::world::WorldEncodeInputs {
                        geometry,
                        textures,
                        materials,
                        environment: environment.bind_group_for_probe(self.active_probe),
                        props: None,
                        dynamic: None,
                        characters: None,
                        capture_plane: None,
                        filtering: self.filtering,
                        frame,
                        cull: self.culling,
                    },
                )
                .draw_calls
        })
    }

    /// The currently selected development view.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    pub const fn visual_diagnostic(&self) -> super::diagnostics::VisualDiagnosticMode {
        self.visual_diagnostic
    }

    /// Constant false in ordinary builds, so normal frame paths pay no branch.
    const fn visual_diagnostic_active(&self) -> bool {
        #[cfg(feature = "visual-diagnostics")]
        {
            !self.visual_diagnostic.is_final()
        }
        #[cfg(not(feature = "visual-diagnostics"))]
        {
            false
        }
    }

    /// Records the requested quality level.
    ///
    /// Recording changes no GPU state; the work (re-fitting the retained
    /// build's textures, or rebuilding it when the lightmaps changed too) runs
    /// in the prepared installation. The level is also recorded in the live
    /// `quality` field only when those resources commit together.
    /// It also selects matching lighting until the independent setter overrides
    /// that baseline.
    pub const fn set_quality(&mut self, quality: QualityLevel) {
        self.graphics_requested.quality = quality;
        // Preserve normal Low/Medium/High callers; settings may then apply
        // the independent Low lighting override through the explicit setter.
        self.set_lighting_quality(quality);
    }

    /// Selects lighting response without changing texture or scene budgets.
    pub const fn set_lighting_quality(&mut self, quality: QualityLevel) {
        self.graphics_requested.lighting_quality = quality;
    }

    /// Records the player's texture filtering setting.
    ///
    /// The three presets are shared samplers, so switching swaps which sampler
    /// a draw's bind group holds; no pixel data is re-uploaded, no bind group
    /// is rebuilt, and the environment binding does not follow it. An empty or
    /// unknown value keeps the default (High). A real change logs the active
    /// preset once and is otherwise zero-work — the recorded value is applied
    /// at bind time.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        let filtering = TextureFiltering::parse(mode);
        if filtering == self.filtering {
            return;
        }
        self.filtering = filtering;
        self.graphics_requested.filtering = filtering;
        logging::info(format!(
            "[wgpu] texture filtering: {} active (trilinear + {}x anisotropy requested)",
            filtering.name(),
            filtering.anisotropy()
        ));
    }

    /// Records whether bloom is enabled.
    ///
    /// The gate applies at the next frame's post settings; bloom targets stay
    /// allocated and no resource rebuild is needed.
    pub const fn set_bloom_enabled(&mut self, enabled: bool) {
        self.bloom_requested = enabled;
        self.graphics_requested.bloom = enabled;
    }

    /// Records the requested Lightmaps quality.
    ///
    /// The atlas configuration follows this value, never the overall quality
    /// level. The loader prepares the CPU build and atlas before GPU installation.
    pub const fn set_lightmap_quality(&mut self, quality: LightmapQuality) {
        self.graphics_requested.lightmaps = quality;
    }

    /// Records the requested Reflections quality.
    ///
    /// Prepared installation creates or retires probe cubemaps and the planar
    /// target; `Off` disables captures.
    pub const fn set_reflection_quality(&mut self, quality: ReflectionQuality) {
        self.graphics_requested.reflections = quality;
    }

    /// Releases the renderer's quality-fitted texture cache.
    ///
    /// Resident textures stay alive through their `Arc`s until installation
    /// replaces them, so drawing remains valid after cache release.
    pub fn release_profile_textures(&mut self) {
        self.textures.release_profile_textures();
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("texture-cache-released");
    }

    /// The last level upload's counters.
    #[must_use]
    pub const fn level_stats(&self) -> LevelBuildStats {
        self.level_stats
    }

    /// The most recently submitted frame's counters.
    #[must_use]
    pub const fn render_stats(&self) -> RenderStats {
        self.render_stats
    }

    /// Latest completed GPU render-pass sample; asynchronous readback lag is
    /// described by its unique id and measured renderer frame id.
    #[must_use]
    pub const fn gpu_timing(&self) -> GpuTiming {
        self.gpu_timer.status()
    }

    /// World draw ranges in the loaded level.
    #[must_use]
    pub fn static_batch_count(&self) -> usize {
        self.world.as_ref().map_or(0, |world| world.draws().len())
    }

    /// Prop submesh draws in the loaded level.
    #[must_use]
    pub fn prop_draw_count(&self) -> usize {
        self.world_props
            .as_ref()
            .map_or(0, |props| props.stats().draws)
    }

    /// Decoded prop-model statistics.
    #[must_use]
    pub fn prop_asset_stats(&self) -> crate::engine::props::PropAssetStats {
        self.prop_assets.stats()
    }

    /// World draw ranges per surface family, in `SurfaceKind::ALL` order.
    #[must_use]
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        self.world.as_ref().map_or(
            [0; SurfaceKind::ALL.len()],
            WgpuWorldGeometry::family_breakdown,
        )
    }

    /// Draws one frame's scene.
    ///
    /// The world path: acquire the surface texture, clear colour and depth,
    /// then submit the uploaded world passes from the shared Places camera.
    /// The camera is prepared by the renderer-neutral
    /// [`prepare_world_frame`]; the only coordinate decision made here is its
    /// one clip-space correction.
    #[expect(
        clippy::too_many_lines,
        reason = "one cohesive frame submission: prepare, encode and present"
    )] // one cohesive frame submission: prepare, encode and present
    pub fn render_scene(&mut self, camera: RenderCamera) {
        // Per-frame submission counters: RenderStats and the bench CSV report
        // this frame's reflection captures, which is what the documented
        // "submissions the frame spent on reflections" field means.
        self.reflection_passes = 0;
        if self.check_device_lost() {
            return;
        }
        // The background lightmap fill, if any, is polled here: a finished fill
        // uploads its atlas and swaps the world in on this thread, while the
        // previous world keeps rendering until then. Two `Option` checks when
        // idle.
        self.ensure_ready();
        if self.fatal.is_some() || self.needs_configure || self.drawable_size.is_empty() {
            return;
        }
        self.ensure_world_pipeline();
        if self.fatal.is_some() {
            return;
        }
        self.poll_device();
        if self.fatal.is_some() {
            return;
        }
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resource_frame();
        self.gpu_timer.begin_frame(&self.queue);
        // Animated emissions are level data; advancing them is a handful of
        // uniform writes, and a still material writes nothing.
        if let Some(materials) = self.world_materials.as_mut() {
            materials.update_animations(&self.queue, self.animation_seconds);
        }
        if let Some(props) = self.world_props.as_mut() {
            props.update_animations(&self.queue, self.animation_seconds);
        }
        let frame = prepare_world_frame(camera, self.drawable_size);
        // The planar target follows the render size; a recreate invalidates the
        // environment binding that references it.
        if self.reflections.planar_wanted()
            && self
                .reflection_targets
                .ensure_planar(&self.device, planar_size_for(self.drawable_size))
        {
            self.refresh_environment();
        }
        // At most one mirror plane per frame: the nearest one whose reflective
        // geometry survived the frustum test.
        let plane_index = if self.reflections.planar_wanted() {
            nearest_visible_reflection_plane(
                &self.reflections.routing,
                frame.eye.into(),
                &frame.frustum,
            )
        } else {
            None
        };
        self.active_plane = plane_index;
        // Select the probe nearest the camera every frame.
        self.active_probe = if self.reflections_enabled {
            crate::engine::render::common::reflections::nearest_probe(
                &self.reflections.routing.probe_points,
                frame.eye.into(),
            )
            .unwrap_or(0)
        } else {
            0
        };
        self.prepare_flashlight_shadow();
        if self.flashlight.is_some() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("places-flashlight-visibility-submit"),
                });
            self.encode_flashlight_shadow(&mut encoder);
            let _submission = self.queue.submit([encoder.finish()]);
        }
        if let Some(pipeline) = self.capture_pipeline.as_ref() {
            pipeline.update_flashlight_lights(
                &self.queue,
                self.characters.dynamic_lights(),
                self.flashlight,
            );
        }
        for pipeline in [
            &self.decal_pipeline,
            &self.decal_scene_pipeline,
            &self.decal_reflection_pipeline,
        ]
        .into_iter()
        .flatten()
        {
            pipeline.update_flashlight_lights(
                &self.queue,
                self.characters.dynamic_lights(),
                self.flashlight,
            );
        }
        let captured = plane_index.is_some_and(|index| self.encode_planar_capture(&frame, index));
        // The main pass samples the frame's real reflection sources (the
        // capture suppressed them for its own submission).
        let probes_resident = !self.reflection_targets.probes().is_empty();
        if let Some(materials) = self.world_materials.as_mut() {
            materials.update_reflection_modes(
                &self.queue,
                captured.then_some(plane_index).flatten(),
                probes_resident,
                self.reflections_enabled,
                false,
            );
        }
        let environment_template = self.level_environment();
        if let Some(environment) = self.environment.as_mut() {
            let uniform = captured.then_some(plane_index).flatten().and_then(|index| {
                self.reflections.routing.planes.get(index).map(|plane| {
                    (
                        planar_view_projection(frame.view_projection, plane),
                        [
                            plane.normal[0],
                            plane.normal[1],
                            plane.normal[2],
                            plane.offset,
                        ],
                    )
                })
            });
            let view_uniform = match uniform {
                Some((matrix, plane)) => environment_template.with_planar(matrix, plane),
                None => environment_template,
            };
            let _update_stats = environment.update(&self.queue, &view_uniform);
        }
        if let Some(pipeline) = self.world_pipeline.as_mut() {
            #[cfg(feature = "visual-diagnostics")]
            pipeline.set_visual_diagnostic(self.visual_diagnostic);
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.decal_pipeline.as_mut() {
            pipeline.upload_camera_with_storm(
                &self.queue,
                frame.view_projection,
                frame.eye,
                &environment_template.storm,
            );
        }
        if let Some(pipeline) = self.scene_pipeline.as_mut() {
            #[cfg(feature = "visual-diagnostics")]
            pipeline.set_visual_diagnostic(self.visual_diagnostic);
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.emissive_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        // The attached lights are this frame's character glows. The three
        // main-scene pipelines upload the same set (an empty one writes a zero
        // count, clearing the previous frame); the reflection-capture pipeline
        // keeps its zeroed set, so a mirror or probe never bakes a glow in.
        let lights = self.characters.dynamic_lights();
        if let Some(pipeline) = self.world_pipeline.as_ref() {
            pipeline.update_flashlight_lights(&self.queue, lights, self.flashlight);
        }
        if let Some(pipeline) = self.scene_pipeline.as_ref() {
            pipeline.update_flashlight_lights(&self.queue, lights, self.flashlight);
        }
        if let Some(pipeline) = self.emissive_pipeline.as_ref() {
            pipeline.update_flashlight_lights(&self.queue, lights, self.flashlight);
        }
        if let Some(pipeline) = self.decal_scene_pipeline.as_mut() {
            pipeline.upload_camera_with_storm(
                &self.queue,
                frame.view_projection,
                frame.eye,
                &environment_template.storm,
            );
        }
        if let Some(sky) = self.sky.as_ref()
            && let Some(pipeline) = self.sky_pipeline.as_mut()
        {
            let storm = self.effects.snow().map_or(
                [0.0; 4],
                crate::engine::render::common::snow::SnowScene::sky_storm,
            );
            pipeline.upload_camera_with_storm(
                &self.queue,
                frame.view_projection,
                sky.brightness(),
                storm,
            );
        }
        if let Some(pipeline) = self.effects_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.effects_scene_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        // The effect billboards face this frame's camera, so they are synced
        // here, where the prepared frame and the neutral scene are both in
        // scope. The vertex buffers are pre-sized; this writes only the used
        // range.
        if let Some(effects) = self.world_effects.as_mut() {
            effects.sync(
                &self.queue,
                &self.effects,
                [frame.eye.x, frame.eye.y, frame.eye.z],
                frame.view_projection,
                &frame.frustum,
                self.quality,
            );
        }
        let _ensure_post_targets_status = self.ensure_post_targets();
        self.last_frame = Some(frame);
        // A frame whose present was skipped (`PLACES_BENCH_NOSWAP`) must not
        // hold the swapchain: discard it before acquiring the next one.
        self.pending_frame = None;
        match self.acquire_frame() {
            Acquired::Frame(texture) => {
                let view = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("places-wgpu-frame"),
                        });
                let totals =
                    self.encode_frame_into(&mut encoder, &view, &frame, Some(&self.gpu_timer));
                let _submission = self.queue.submit([encoder.finish()]);
                self.pending_frame = Some(texture);
                self.record_render_stats(totals);
            }
            Acquired::Skip => {}
        }
    }

    /// Ensures the offscreen post targets match the drawable and level.
    ///
    /// Returns true when the post chain is ready to draw. A no-op while the
    /// drawable is empty or the renderer is fatal.
    fn ensure_post_targets(&mut self) -> bool {
        if self.drawable_size.is_empty() {
            return false;
        }
        let settings = self.presentation_settings();
        let Some(post) = self.post.as_mut() else {
            return false;
        };
        post.upload_settings(&self.queue, settings);
        let stats = post.ensure(&self.device, self.quality, self.drawable_size);
        let ready = post.is_ready();
        if stats.created {
            logging::info(format!(
                "[wgpu] post targets: scene {}x{}, bloom {}x{}, presented {}x{}",
                stats.scene.width,
                stats.scene.height,
                stats.bloom.width,
                stats.bloom.height,
                stats.presented.width,
                stats.presented.height,
            ));
            #[cfg(feature = "visual-diagnostics")]
            self.trace_gpu_resources("post-targets-resized");
        }
        ready
    }

    fn presentation_settings(&self) -> PostSettings {
        if self.visual_diagnostic_active() {
            PostSettings::DIAGNOSTIC
        } else {
            PostSettings::from_level(self.installed_level.as_ref()).with_bloom(self.bloom_requested)
        }
    }

    /// Encodes one complete frame into `target`: the offscreen scene pass with
    /// the full body, the emissive pass and blur when a bloom frame needs them,
    /// then the resolve (or the plain copy) into the target.
    ///
    /// Falls back to a direct-to-target scene pass when the post chain is not
    /// ready (an empty drawable or a first frame before `ensure`), which is the
    /// same image without tone, grade or bloom.
    fn encode_frame_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &super::world::WorldFrame,
        timer: Option<&GpuTimer>,
    ) -> WorldDrawTotals {
        let Some(post) = self.post.as_ref().filter(|post| post.is_ready()) else {
            return self.encode_direct(encoder, target, frame, timer);
        };
        let settings = self.presentation_settings();
        let mut totals = WorldDrawTotals::default();
        {
            let Some(scene_view) = post.scene_view() else {
                return totals;
            };
            let Some(depth_view) = post.scene_depth_view() else {
                return totals;
            };
            // Before installation this pass only clears the target. Metal's
            // vertex/fragment stage boundaries do not exist for that pass.
            let scene_timer = timer.filter(|_| {
                self.scene_pipeline.is_some()
                    && self.world.is_some()
                    && self.world_textures.is_some()
                    && self.world_materials.is_some()
                    && self.environment.is_some()
            });
            let checkpoint = scene_timer.map(GpuTimer::pass_checkpoint);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("places-wgpu-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: scene_timer.and_then(|value| value.pass_writes("scene")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let (scene_totals, pass_draws) = self.encode_scene_body(&mut pass, frame);
            totals = scene_totals;
            drop(pass);
            if let (Some(active_timer), Some(beginning_query)) = (scene_timer, checkpoint) {
                active_timer.record_pass_draws(beginning_query, pass_draws);
            }
        }
        let blooms = self.bloom_requested && settings.blooms() && totals.emissive_visible;
        if blooms && self.emissive_pipeline.is_some() {
            let checkpoint = timer.map(GpuTimer::pass_checkpoint);
            if let Some(mut pass) = post.emissive_begin(encoder, timer) {
                let emission_draws = self.encode_emissive_body(&mut pass, frame).draw_calls;
                drop(pass);
                if let (Some(active_timer), Some(beginning_query)) = (timer, checkpoint) {
                    active_timer.record_pass_draws(beginning_query, emission_draws);
                }
            }
            post.encode_blur(encoder, settings.bloom_strength, timer);
        }
        // The resolve (or present copy) lands on the presented target, the
        // backend's default-framebuffer equivalent; the UI blends into it in
        // display space and it is encoded to the sRGB surface afterwards.
        if let Some(presented) = post.presented_view() {
            post.encode_resolve(encoder, presented, settings, blooms, timer);
        }
        totals
    }

    /// Encodes the emissive pass body (static classes, props and dynamics with
    /// emission only; no decals) with the raw-format pipeline.
    fn encode_emissive_body<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        frame: &'a super::world::WorldFrame,
    ) -> WorldDrawTotals {
        let (Some(pipeline), Some(world), Some(textures), Some(materials), Some(environment)) = (
            self.emissive_pipeline.as_ref(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.world_materials.as_ref(),
            self.environment.as_ref(),
        ) else {
            return WorldDrawTotals::default();
        };
        pipeline.encode_emissive(
            pass,
            super::world::WorldEncodeInputs {
                geometry: world,
                textures,
                materials,
                environment: environment.bind_group_for_probe(self.active_probe),
                props: self.world_props.as_ref(),
                dynamic: self.world_dynamic.as_ref(),
                characters: self.world_characters.as_ref(),
                capture_plane: None,
                filtering: self.filtering,
                frame,
                cull: self.culling,
            },
        )
    }

    /// Encodes the world body (static classes, props, dynamics, decals) with
    /// the scene-format pipeline into an open pass.
    fn encode_scene_body<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        frame: &'a super::world::WorldFrame,
    ) -> (WorldDrawTotals, usize) {
        let mut totals = WorldDrawTotals::default();
        let mut additional_draws = 0_usize;
        let (Some(pipeline), Some(world), Some(textures), Some(materials), Some(environment)) = (
            self.scene_pipeline.as_ref(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.world_materials.as_ref(),
            self.environment.as_ref(),
        ) else {
            return (totals, 0);
        };
        // The night sky paints the cleared background first; the world body
        // draws over it with depth testing, so a solid ceiling always hides it.
        if !self.visual_diagnostic_active()
            && let (Some(sky), Some(sky_pipeline)) = (self.sky.as_ref(), self.sky_pipeline.as_ref())
        {
            sky_pipeline.encode_scene(pass, sky, self.filtering);
            additional_draws = additional_draws.saturating_add(1);
        }
        totals = pipeline.encode(
            pass,
            super::world::WorldEncodeInputs {
                geometry: world,
                textures,
                materials,
                environment: environment.bind_group_for_probe(self.active_probe),
                props: self.world_props.as_ref(),
                dynamic: self.world_dynamic.as_ref(),
                characters: self.world_characters.as_ref(),
                capture_plane: None,
                filtering: self.filtering,
                frame,
                cull: self.culling,
            },
        );
        #[cfg(feature = "visual-diagnostics")]
        {
            additional_draws = additional_draws.saturating_add(self.encode_probe_visualization(
                pass,
                pipeline,
                textures,
                materials,
                environment,
                frame,
            ));
        }
        if !self.visual_diagnostic_active() {
            self.encode_decals(pass, self.decal_scene_pipeline.as_ref(), frame, &mut totals);
        }
        // Ambient effects blend over the finished world body, depth-testing
        // against it with depth writes off. Drawn after the decals: a decal
        // writes depth, so a plume behind one is correctly occluded, and a
        // plume in front of one blends over it. Effects are not captured into
        // the reflection probes or the planar mirror, and they are not ordered
        // against the world's own translucent draws (see the effects module's
        // draw-order note).
        self.encode_effects(pass, self.effects_scene_pipeline.as_ref(), &mut totals);
        (totals, totals.draw_calls.saturating_add(additional_draws))
    }

    /// Encodes the effect pass and folds its counters into `totals`.
    ///
    /// `pipeline` must match the pass's colour format (the offscreen scene
    /// target or the main surface). The encode itself is one draw per distinct
    /// effect material over the pre-sized vertex/index buffers.
    fn encode_effects<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: Option<&'a EffectsPipeline>,
        totals: &mut WorldDrawTotals,
    ) {
        if self.visual_diagnostic_active() {
            return;
        }
        let (Some(effects), Some(effects_pipeline)) = (self.world_effects.as_ref(), pipeline)
        else {
            return;
        };
        let effect_totals = effects.encode(pass, effects_pipeline, self.filtering);
        totals.draw_calls = totals.draw_calls.saturating_add(effect_totals.draws);
        totals.visible_batches = totals.visible_batches.saturating_add(effect_totals.draws);
        totals.visible_vertices = totals
            .visible_vertices
            .saturating_add(effect_totals.vertices);
        totals.visible_indices = totals.visible_indices.saturating_add(effect_totals.indices);
        totals.texture_binds = totals
            .texture_binds
            .saturating_add(effect_totals.texture_binds);
    }

    /// Appends decals and their actual draws to either scene or capture totals.
    fn encode_decals<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: Option<&'a DecalPipeline>,
        frame: &'a super::world::WorldFrame,
        totals: &mut WorldDrawTotals,
    ) {
        let (Some(decals), Some(decal_pipeline)) = (self.decals.as_ref(), pipeline) else {
            return;
        };
        let decal_totals = decals.encode(
            pass,
            decal_pipeline,
            super::decals::DecalEncodeInputs {
                filtering: self.filtering,
                frame,
                cull: self.culling,
            },
        );
        totals.draw_calls = totals.draw_calls.saturating_add(decal_totals.draw_calls);
        totals.visible_batches = totals
            .visible_batches
            .saturating_add(decal_totals.visible_batches);
        totals.visible_vertices = totals
            .visible_vertices
            .saturating_add(decal_totals.visible_vertices);
        totals.visible_indices = totals
            .visible_indices
            .saturating_add(decal_totals.visible_indices);
        totals.texture_binds = totals
            .texture_binds
            .saturating_add(decal_totals.texture_binds);
    }

    /// Encodes the world body directly into `target`, without tone, grade or
    /// bloom: the fallback when no post targets could be created.
    fn encode_direct(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &super::world::WorldFrame,
        timer: Option<&GpuTimer>,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let mut additional_draws = 0_usize;
        let direct_timer = timer.filter(|_| {
            self.depth.is_some()
                && self.world_pipeline.is_some()
                && self.world.is_some()
                && self.world_textures.is_some()
                && self.world_materials.is_some()
                && self.environment.is_some()
        });
        let checkpoint = direct_timer.map(GpuTimer::pass_checkpoint);
        let depth_attachment =
            self.depth
                .as_ref()
                .map(|depth| wgpu::RenderPassDepthStencilAttachment {
                    view: &depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("places-wgpu-world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.direct_clear_color()),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_attachment,
            timestamp_writes: direct_timer.and_then(|value| value.pass_writes("direct-scene")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if self.depth.is_some()
            && let (Some(pipeline), Some(world), Some(textures), Some(materials), Some(environment)) = (
                self.world_pipeline.as_ref(),
                self.world.as_ref(),
                self.world_textures.as_ref(),
                self.world_materials.as_ref(),
                self.environment.as_ref(),
            )
        {
            // The sky background, drawn before the world exactly like the
            // offscreen scene body.
            if !self.visual_diagnostic_active()
                && let (Some(sky), Some(sky_pipeline)) =
                    (self.sky.as_ref(), self.sky_pipeline.as_ref())
            {
                sky_pipeline.encode_surface(&mut pass, sky, self.filtering);
                additional_draws = additional_draws.saturating_add(1);
            }
            totals = pipeline.encode(
                &mut pass,
                super::world::WorldEncodeInputs {
                    geometry: world,
                    textures,
                    materials,
                    environment: environment.bind_group_for_probe(self.active_probe),
                    props: self.world_props.as_ref(),
                    dynamic: self.world_dynamic.as_ref(),
                    characters: self.world_characters.as_ref(),
                    capture_plane: None,
                    filtering: self.filtering,
                    frame,
                    cull: self.culling,
                },
            );
            #[cfg(feature = "visual-diagnostics")]
            {
                additional_draws =
                    additional_draws.saturating_add(self.encode_probe_visualization(
                        &mut pass,
                        pipeline,
                        textures,
                        materials,
                        environment,
                        frame,
                    ));
            }
            if !self.visual_diagnostic_active() {
                self.encode_decals(&mut pass, self.decal_pipeline.as_ref(), frame, &mut totals);
            }
            self.encode_effects(&mut pass, self.effects_pipeline.as_ref(), &mut totals);
        }
        drop(pass);
        if let (Some(active_timer), Some(beginning_query)) = (direct_timer, checkpoint) {
            active_timer.record_pass_draws(
                beginning_query,
                totals.draw_calls.saturating_add(additional_draws),
            );
        }
        totals
    }

    const fn direct_clear_color(&self) -> wgpu::Color {
        if super::surface::target_is_raw_display(self.config.format) {
            wgpu::Color {
                r: 0.08,
                g: 0.08,
                b: 0.09,
                a: 1.0,
            }
        } else {
            CLEAR_COLOR_SRGB
        }
    }

    /// Builds the world pipeline for the configured surface format.
    ///
    /// Called before encoding a frame; rebuilds the pipeline only when the
    /// format changed (a recreated surface can legitimately report a different
    /// preferred format). The camera binding is format-independent and
    /// survives, and so are the per-texture bind groups: they are created
    /// against the texture cache's layout, which outlives every pipeline.
    fn ensure_world_pipeline(&mut self) {
        if self.world_pipeline.as_ref().map(WorldPipeline::format) == Some(self.config.format) {
            return;
        }
        self.world_pipeline = Some(WorldPipeline::new(
            &self.device,
            self.config.format,
            self.textures.layout(),
            &self.material_layout,
            &self.environment_layout,
        ));
        // The capture pipeline is format-fixed (the reflection targets) and
        // culls nothing so both sides remain visible in reflections. The reversed front
        // face compensates for the winding the probe projection's Y flip and
        // the planar mirror each reverse.
        self.capture_pipeline = Some(WorldPipeline::with_state(
            &self.device,
            super::reflections::REFLECTION_FORMAT,
            self.textures.layout(),
            &self.material_layout,
            &self.environment_layout,
            wgpu::FrontFace::Cw,
            false,
            true,
        ));
        // The decal pass runs in the same colour+depth passes as the world, so
        // it needs one pipeline per target format.
        self.decal_pipeline = Some(DecalPipeline::new(&self.device, self.config.format));
        self.decal_reflection_pipeline = Some(DecalPipeline::new(
            &self.device,
            super::reflections::REFLECTION_FORMAT,
        ));
        self.decal_scene_pipeline = Some(DecalPipeline::new(&self.device, SCENE_FORMAT));
        // The sky pass draws in the same colour+depth passes as the world (the
        // main surface and the offscreen scene target, in raw display space),
        // sampling the shared texture-cache group 1.
        self.sky_pipeline = Some(SkyPipeline::new(
            &self.device,
            self.config.format,
            SCENE_FORMAT,
            self.textures.layout(),
        ));
        // The effect billboards run in the same colour+depth passes as the
        // world (the main surface and the offscreen scene target), with the
        // shared texture-cache layout as group 1, so every uploaded effect
        // sheet binds unchanged.
        self.effects_pipeline = Some(EffectsPipeline::new(
            &self.device,
            self.config.format,
            self.textures.layout(),
        ));
        self.effects_scene_pipeline = Some(EffectsPipeline::new(
            &self.device,
            SCENE_FORMAT,
            self.textures.layout(),
        ));
        // The offscreen post chain: the scene target's format equals the
        // reflection format, the emissive target is raw, and the resolve and
        // present pipelines write the surface format.
        self.post = Some(match self.post.take() {
            Some(mut post) => {
                let _surface_format_changed =
                    post.set_surface_format(&self.device, self.config.format);
                post
            }
            None => PostProcess::new(&self.device, self.config.format),
        });
        self.scene_pipeline = Some(WorldPipeline::with_state(
            &self.device,
            SCENE_FORMAT,
            self.textures.layout(),
            &self.material_layout,
            &self.environment_layout,
            wgpu::FrontFace::Ccw,
            false,
            true,
        ));
        self.emissive_pipeline = Some(WorldPipeline::with_state(
            &self.device,
            EMISSIVE_FORMAT,
            self.textures.layout(),
            &self.material_layout,
            &self.environment_layout,
            wgpu::FrontFace::Ccw,
            false,
            true,
        ));
        logging::info(format!(
            "[wgpu] world pipeline for surface format {:?}",
            self.config.format
        ));
    }

    /// Builds the UI pipeline for the configured surface format.
    ///
    /// Called before drawing the UI; rebuilds only when the format changed (a
    /// recreated surface). The HUD atlas is re-uploaded with it; it is 256x128
    /// and created once per format.
    fn ensure_ui_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.ui.as_ref().map(UiRenderer::format) == Some(format) {
            return;
        }
        self.ui = Some(UiRenderer::new(&self.device, &self.queue, format));
    }

    /// Draws the 2D UI vertex list into the frame acquired by `render_scene`.
    ///
    /// The 480x272 logical HUD, drawn after
    /// the scene resolve, with depth testing off and straight-alpha blending.
    /// The caller must run it between `render_scene` and `present`; with no
    /// acquired frame or no vertices it is a no-op.
    pub fn render_ui(&mut self, ui_vertices: &[Vertex]) {
        if self.check_device_lost() {
            return;
        }
        if self.fatal.is_some() || self.drawable_size.is_empty() {
            return;
        }
        // The capture replays the HUD when the direct fallback is active; the
        // post path's presented target already carries it.
        self.last_ui.clear();
        self.last_ui.extend_from_slice(ui_vertices);
        let presented_ready = self
            .post
            .as_ref()
            .and_then(PostProcess::presented_view)
            .is_some();
        if presented_ready {
            self.render_ui_into_presented(ui_vertices);
        } else if !ui_vertices.is_empty() {
            self.render_ui_into_surface(ui_vertices);
        }
    }

    /// The presented target's pixel size for the active level.
    ///
    /// The UI's viewport is computed against the presented target itself, so
    /// the HUD renders at drawable
    /// resolution even when the scene target is reduced under Low.
    fn presented_drawable(&self) -> DrawableSize {
        self.post
            .as_ref()
            .and_then(PostProcess::presented_size)
            .unwrap_or(self.drawable_size)
    }

    /// The post path's UI: the HUD blends into the raw presented target, then
    /// the whole presented image is encoded to the sRGB surface.
    fn render_ui_into_presented(&mut self, ui_vertices: &[Vertex]) {
        let Some(presented) = self
            .post
            .as_ref()
            .and_then(PostProcess::presented_view)
            .cloned()
        else {
            return;
        };
        let Some(frame) = self.pending_frame.as_ref() else {
            return;
        };
        let surface = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        if !ui_vertices.is_empty() {
            self.ensure_ui_pipeline(PRESENTED_FORMAT);
            if self.fatal.is_some() {
                return;
            }
            let presented_drawable = self.presented_drawable();
            if let Some(ui) = self.ui.as_mut() {
                let _draw_stats = ui.render(
                    &self.device,
                    &self.queue,
                    &presented,
                    ui_vertices,
                    presented_drawable,
                    Some(UiTiming::Continue(&mut self.gpu_timer)),
                );
            }
        }
        let Some(post) = self.post.as_ref() else {
            return;
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-wgpu-presented"),
            });
        post.encode_present_to(&mut encoder, &surface, Some(&self.gpu_timer));
        let _submission = self.queue.submit([encoder.finish()]);
        self.gpu_timer.finish_frame_submitted(&self.queue);
    }

    /// The direct fallback path's UI: no presented target exists, so the HUD
    /// blends straight into the sRGB surface.
    fn render_ui_into_surface(&mut self, ui_vertices: &[Vertex]) {
        self.ensure_ui_pipeline(self.config.format);
        if self.fatal.is_some() {
            return;
        }
        let Some(frame) = self.pending_frame.as_ref() else {
            return;
        };
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let _draw_stats = ui.render(
            &self.device,
            &self.queue,
            &view,
            ui_vertices,
            self.drawable_size,
            Some(UiTiming::Finish(&mut self.gpu_timer)),
        );
    }

    /// Uploads a compiled world's packaged probe chains into the resident
    /// cubemaps.
    ///
    /// The points and shapes were validated against the emitted geometry when
    /// the package loaded; a mismatch here is a programming error and is
    /// reported rather than silently left black. Every packaged level is
    /// written, so the shader's roughness selection reads the compiler's
    /// offline prefilter instead of the live-capture fallback.
    fn upload_packaged_probes(&self, probes: &crate::engine::package::world::ProbeCaptures) {
        let resident = self.reflection_targets.probes();
        if resident.is_empty() {
            return;
        }
        let face_size = resident.first().map_or(0, |probe| probe.face_size);
        let Some(capture) = probes.for_face_size(face_size) else {
            logging::warn(format!(
                "[wgpu] package has no {face_size}-texel reflection captures for the resident targets"
            ));
            return;
        };
        if capture.chains.len() != resident.len() {
            logging::warn(format!(
                "[wgpu] package has {} probe capture(s) for {} resident probe(s)",
                capture.chains.len(),
                resident.len()
            ));
            return;
        }
        let mut levels = 0_u32;
        for (probe, chain) in resident.iter().zip(&capture.chains) {
            match upload_probe_chain(probe, &self.queue, chain) {
                Ok(written) => levels = levels.max(written),
                Err(error) => {
                    logging::warn(format!(
                        "[wgpu] a packaged reflection probe was not uploaded: {error}"
                    ));
                    return;
                }
            }
        }
        logging::info(format!(
            "[wgpu] uploaded {} packaged reflection probe(s) at {face_size} texels, \
             {levels} mip level(s)",
            capture.chains.len()
        ));
    }

    fn clear_capture_weather(&mut self) {
        let mut clear_atmosphere = self.level_environment();
        clear_atmosphere.storm = crate::engine::render::common::storm::StormUniform::default();
        if let Some(binding) = self.environment.as_mut() {
            let _updated = binding.update(&self.queue, &clear_atmosphere);
        }
    }

    /// Bakes every wanted probe cubemap: six full scene submissions per probe.
    ///
    /// Runs once per level load, after the world, its textures and its materials
    /// are resident. All
    /// reflection sampling is suppressed while it runs (every material mode is
    /// zero), so a probe never samples an incomplete cube.
    #[expect(
        clippy::too_many_lines,
        reason = "one cohesive bake: six face submissions per probe in one loop"
    )] // one cohesive bake: six face submissions per probe in one loop
    fn bake_reflection_probes(&mut self) {
        if self.reflection_targets.probes().is_empty() || self.check_device_lost() {
            return;
        }
        self.ensure_world_pipeline();
        if self.fatal.is_some() {
            return;
        }
        // Transient weather belongs to the presented sightline, never a bake.
        self.clear_capture_weather();
        let Some(pipeline) = self.capture_pipeline.as_mut() else {
            return;
        };
        pipeline.update_lights(
            &self.queue,
            &crate::engine::render::common::DynamicLightSet::new(),
        );
        if let Some(decals) = self.decal_reflection_pipeline.as_ref() {
            decals.update_flashlight_lights(
                &self.queue,
                &crate::engine::render::common::DynamicLightSet::new(),
                None,
            );
        }

        let Some(environment) = self.environment.as_ref() else {
            return;
        };
        let Some(materials) = self.world_materials.as_mut() else {
            return;
        };
        let Some(world) = self.world.as_ref() else {
            return;
        };
        let Some(textures) = self.world_textures.as_ref() else {
            return;
        };
        materials.update_reflection_modes(&self.queue, None, false, false, true);
        let mut baked = 0usize;
        for probe in self.reflection_targets.probes() {
            for face in 0..6 {
                let Some(color_view) = probe.face_view(face) else {
                    continue;
                };
                let Some(depth_view) = probe.depth_face_view(face) else {
                    continue;
                };
                let frame = CaptureFrame::new(
                    probe_face_view_projection(probe.position, face),
                    glam::Vec3::from_array(probe.position),
                );
                pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
                let world_frame = frame.world_frame();
                if let Some(decal_pipeline) = self.decal_reflection_pipeline.as_mut() {
                    decal_pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
                }
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("places-wgpu-probe-bake"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("places-wgpu-probe-face"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: color_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    let _draw_stats = pipeline.encode(
                        &mut pass,
                        super::world::WorldEncodeInputs {
                            geometry: world,
                            textures,
                            materials,
                            environment: environment.capture_bind_group(),
                            props: self.world_props.as_ref(),
                            dynamic: self.world_dynamic.as_ref(),
                            characters: self.world_characters.as_ref(),
                            capture_plane: None,
                            filtering: self.filtering,
                            frame: &world_frame,
                            cull: self.culling,
                        },
                    );
                    // Probe captures include the scene's decals.
                    if let (Some(decals), Some(decal_pipeline)) = (
                        self.decals.as_ref(),
                        self.decal_reflection_pipeline.as_ref(),
                    ) {
                        let _decal_draw_stats = decals.encode(
                            &mut pass,
                            decal_pipeline,
                            super::decals::DecalEncodeInputs {
                                filtering: self.filtering,
                                frame: &world_frame,
                                cull: self.culling,
                            },
                        );
                    }
                }
                let _submission = self.queue.submit([encoder.finish()]);
                baked = baked.saturating_add(1);
            }
        }
        // Restore the frame's own modes; the next `render_scene` recomputes
        // them from the active plane anyway.
        materials.update_reflection_modes(
            &self.queue,
            None,
            !self.reflection_targets.probes().is_empty(),
            self.reflections_enabled,
            false,
        );
        logging::info(format!(
            "[wgpu] reflection probes: {} probe(s), {baked} face(s), face edge {}px",
            self.reflection_targets.probes().len(),
            self.reflection_targets
                .probes()
                .first()
                .map_or(0, |probe| probe.face_size),
        ));
    }

    /// Renders one planar mirror capture into the half-size target.
    ///
    /// Uses the mirrored view-projection and mirrored eye; reflection sampling
    /// is suppressed for the duration. Returns true when
    /// a capture was submitted.
    fn encode_planar_capture(
        &mut self,
        frame: &super::world::WorldFrame,
        plane_index: usize,
    ) -> bool {
        // The main surface applies weather once after sampling its reflection.
        self.clear_capture_weather();
        let (
            Some(pipeline),
            Some(decal_pipeline),
            Some(planar),
            Some(environment),
            Some(materials),
            Some(world),
            Some(textures),
            Some(plane),
        ) = (
            self.capture_pipeline.as_mut(),
            self.decal_reflection_pipeline.as_mut(),
            self.reflection_targets.planar(),
            self.environment.as_ref(),
            self.world_materials.as_mut(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.reflections.routing.planes.get(plane_index),
        )
        else {
            return false;
        };
        let mirrored_matrix = planar_view_projection(frame.view_projection, plane);
        let mirrored_eye = glam::Vec3::from_array(mirror_point(
            plane.normal,
            plane.offset,
            frame.eye.to_array(),
        ));
        let capture = CaptureFrame::new(mirrored_matrix, mirrored_eye);
        // The mirror must not sample the image it is writing.
        materials.update_reflection_modes(&self.queue, None, false, false, true);
        pipeline.upload_camera(&self.queue, capture.view_projection, capture.eye);
        decal_pipeline.upload_camera(&self.queue, capture.view_projection, capture.eye);
        // Camera uploads are finished; encoding and the shared decal helper
        // borrow the pipelines immutably for the open render pass.
        let (Some(capture_pipeline), Some(capture_decal_pipeline), Some(capture_materials)) = (
            self.capture_pipeline.as_ref(),
            self.decal_reflection_pipeline.as_ref(),
            self.world_materials.as_ref(),
        ) else {
            return false;
        };
        let capture_frame = capture.world_frame();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-wgpu-planar-capture"),
            });
        let checkpoint = self.gpu_timer.pass_checkpoint();
        let pass_draws;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("places-wgpu-planar"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: planar.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: planar.depth_view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_timer.pass_writes("planar"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut capture_draws = capture_pipeline.encode(
                &mut pass,
                super::world::WorldEncodeInputs {
                    geometry: world,
                    textures,
                    materials: capture_materials,
                    environment: environment.capture_bind_group(),
                    props: self.world_props.as_ref(),
                    dynamic: self.world_dynamic.as_ref(),
                    characters: self.world_characters.as_ref(),
                    capture_plane: Some(plane_index),
                    filtering: self.filtering,
                    frame: &capture_frame,
                    cull: self.culling,
                },
            );
            // Planar captures include the scene's decals.
            self.encode_decals(
                &mut pass,
                Some(capture_decal_pipeline),
                &capture_frame,
                &mut capture_draws,
            );
            pass_draws = capture_draws.draw_calls;
        }
        self.gpu_timer.record_pass_draws(checkpoint, pass_draws);
        let _submission = self.queue.submit([encoder.finish()]);
        self.reflection_passes = self.reflection_passes.saturating_add(1);
        true
    }

    /// Records one frame's world submission into the neutral counters.
    fn record_render_stats(&mut self, totals: WorldDrawTotals) {
        let world = self
            .world
            .as_ref()
            .map(WgpuWorldGeometry::stats)
            .unwrap_or_default();
        let props = self
            .world_props
            .as_ref()
            .map(WgpuProps::stats)
            .unwrap_or_default();
        let dynamic = self
            .world_dynamic
            .as_ref()
            .map(WgpuDynamic::stats)
            .unwrap_or_default();
        let total_vertices = world
            .uploaded_vertices
            .saturating_add(props.vertices)
            .saturating_add(dynamic.vertices);
        let total_batches = world
            .draws
            .saturating_add(props.draws)
            .saturating_add(dynamic.draws);
        self.render_stats = RenderStats {
            total_vertices,
            visible_vertices: totals.visible_vertices,
            visible_indices: totals.visible_indices,
            culled_vertices: total_vertices.saturating_sub(totals.visible_vertices),
            total_batches,
            visible_batches: totals.visible_batches,
            draw_calls: totals.draw_calls,
            dynamic_draws: dynamic.draws,
            dynamic_vertices: dynamic.vertices,
            vbo_bytes: world
                .vertex_bytes
                .saturating_add(props.vertices.saturating_mul(
                    usize::try_from(super::world::WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX),
                ))
                .saturating_add(dynamic.vertex_bytes),
            index_bytes: world
                .index_bytes
                .saturating_add(props.indices.saturating_mul(std::mem::size_of::<u16>()))
                .saturating_add(dynamic.index_bytes),
            texture_binds: totals.texture_binds,
            material_changes: totals.material_binds,
            reflection_passes: u32::try_from(self.reflection_passes).unwrap_or(u32::MAX),
        };
    }

    /// Presents the frame acquired by [`Self::render_scene`].
    ///
    /// When nothing was rendered (`PLACES_BENCH_NORENDER=1`) the swapchain is
    /// still cycled so the presentation path and its timing remain real. A
    /// surface reported lost is recreated here, where the SDL window is
    /// available, and presentation resumes in the same call.
    pub fn present(&mut self, _window: &Window) -> bool {
        if self.check_device_lost() {
            return false;
        }
        if self.surface_lost {
            self.recreate_surface();
        }
        if let Some(texture) = self.pending_frame.take() {
            self.queue.present(texture);
            self.poll_device();
            return true;
        }
        self.ensure_ready();
        if self.fatal.is_some() || self.needs_configure || self.drawable_size.is_empty() {
            return false;
        }
        let presented = match self.acquire_frame() {
            Acquired::Frame(texture) => {
                self.queue.present(texture);
                true
            }
            Acquired::Skip => false,
        };
        self.poll_device();
        presented
    }

    /// Waits for submitted GPU work to finish (`PLACES_BENCH_FINISH`).
    pub fn finish(&self) {
        if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            logging::warn_once(
                "wgpu-finish",
                format!("[wgpu] could not finish GPU work: {error}"),
            );
        }
    }

    /// Receipts describe actual bindings at capture time, after live settings
    /// actions and resource commits, rather than treating startup as applied.
    #[cfg(feature = "visual-diagnostics")]
    fn trace_visual_diagnostic(&self, event: &str, capture_totals: Option<&WorldDrawTotals>) {
        let atlas = self.lightmaps.stats();
        let uniform = self
            .environment
            .as_ref()
            .map(EnvironmentBindings::uploaded_uniform);
        let world = self.world.as_ref().map(|geometry| {
            let stats = geometry.stats();
            serde_json::json!({
                "vertices": stats.uploaded_vertices, "indices": stats.uploaded_indices,
                "draws": stats.draws, "chunks": stats.chunks,
                "vertex_bytes": stats.vertex_bytes, "index_bytes": stats.index_bytes,
            })
        });
        let props = self.world_props.as_ref().map(|geometry| {
            let stats = geometry.stats();
            serde_json::json!({"vertices": stats.vertices, "indices": stats.indices,
                "draws": stats.draws, "chunks": stats.chunks, "texture_bytes": stats.resident_bytes})
        });
        let dynamic = self.world_dynamic.as_ref().map(|geometry| {
            let stats = geometry.stats();
            serde_json::json!({"objects": stats.objects, "meshes": stats.meshes,
                "draws": stats.draws, "vertices": stats.vertices})
        });
        let characters = self.world_characters.as_ref().map(|geometry| {
            let stats = geometry.stats();
            serde_json::json!({"characters": stats.characters, "meshes": stats.meshes,
                "draws": stats.draws, "vertices": stats.vertices})
        });
        let probes = self
            .reflection_targets
            .probes()
            .iter()
            .map(|probe| {
                serde_json::json!({"position": probe.position, "face_size": probe.face_size,
                "mip_levels": probe.mip_levels})
            })
            .collect::<Vec<_>>();
        let receipt = serde_json::json!({
            "event": event, "mode": self.visual_diagnostic.name(),
            "meaning": self.visual_diagnostic.meaning(), "level": self.level_id,
            "requested": self.graphics_requested.diagnostic_state(),
            "applied": self.graphics_applied.diagnostic_state(),
            "capture_submission": capture_totals.map(super::diagnostics::capture_submission),
            "surface_submission": {"source": "last surface RenderStats; may remain zero or stale when acquisition is skipped",
                "draw_calls": self.render_stats.draw_calls, "visible_batches": self.render_stats.visible_batches,
                "visible_vertices": self.render_stats.visible_vertices, "texture_binds": self.render_stats.texture_binds,
                "material_changes": self.render_stats.material_changes},
            "resident": {
                "quality": self.installed_quality.name(), "lightmaps": self.installed_lightmaps.name(),
                "filtering": self.filtering.name(), "reflection_enabled": self.reflections_enabled,
                "lightmaps_resident": self.lightmaps_resident,
                "atlas": {"pages": atlas.pages, "page_edge": atlas.page_edge,
                    "charts": atlas.charts, "chart_texels": atlas.chart_texels,
                    "bytes": atlas.resident_bytes, "format": "Rgba16Float irradiance+signed moment"},
                "reflection_probes": probes, "active_probe": self.active_probe,
                "active_plane": self.active_plane, "planar_resident": self.reflection_targets.planar().is_some(),
                "lighting_uniform": uniform.map(|value| serde_json::json!({
                    "light_scale": value.light_scale, "lightmap_enabled": value.lightmap_enabled,
                    "page_count": value.lightmap_page_count, "switchable_word": value.lightmap_switchable,
                    "fog_density": value.fog_density, "fog_region_count": value.fog_region_count})),
                "world": world, "props": props, "dynamic": dynamic, "characters": characters,
                "irradiance_field": self.dynamic_field.as_ref().map(|field| serde_json::json!({
                    "slots": field.probes.len(), "valid": field.probes.iter().filter(|p| p.is_valid()).count(),
                    "cell_m": field.cell_m, "dims": field.dims,
                    "selected_source_indices": field.runtime_direct_lights(),
                    "selected_direct_sidecar": field.local_direct.is_some(),
                    "serialized_bytes": field.write().ok().map(|bytes| bytes.len()),
                    "native_marker_stride": field.probes.len().div_ceil(2048).max(1),
                })),
                "runtime_visibility_scene": self.dynamic_transport.is_some(),
                "movable_visibility": self.dynamic_visibility.stats(),
                "movable_visibility_error": self.dynamic_visibility_error,
                "contact_subjects": self.contact_shadow_subjects().1,
                "entity_uniform_bytes": std::mem::size_of::<EnvironmentUniform>(),
            },
            "frame": {"drawable": [self.drawable_size.width, self.drawable_size.height],
                "scene_size": self.post.as_ref().and_then(PostProcess::diagnostic_scene_size)
                    .map(|size| [size.width, size.height]),
                "scene_format": "Rgba16Float linear HDR", "base_color_format": "Rgba8UnormSrgb decoded to linear",
                "world_uploaded_selector": self.world_pipeline.as_ref().and_then(WorldPipeline::uploaded_visual_selector),
                "scene_uploaded_selector": self.scene_pipeline.as_ref().and_then(WorldPipeline::uploaded_visual_selector),
                "culling": self.culling, "bloom_requested": self.bloom_requested,
                "bloom_allowed": self.bloom_requested && !self.visual_diagnostic_active(),
                "diagnostic_identity_post": self.visual_diagnostic_active(),
                "display_encoding": if self.visual_diagnostic_active() {
                    "numeric diagnostic RGB converted to linear with inverse sRGB, then one presentation encoding; no exposure, shoulder, grade or bloom"
                } else { "linear HDR + bloom, fixed exposure, hue-preserving shoulder, sRGB, display grade" },
                "sky_decals_effects_suppressed": self.visual_diagnostic_active()},
            "unavailable": {"decomposition": "direct/indirect/filter/fill remain offline solver exports",
                "shadow_ao": "no standalone payload", "metallic": "no material shader input",
                "charts": "atlas grid has no true chart ID/boundary/seam metadata",
                "probe_field": "native prepared-position provider; bounded markers and actual accepted contributor links"},
        });
        logging::info(format!("[visual-diagnostic] {receipt}"));
    }

    /// Reports a selected entity only on the explicit capture path:
    /// `PLACES_ENTITY_LIGHT_TRACE=all`, an instance id, or a model path.
    fn trace_entity_lighting(&self, selector: &str) {
        for (slot, object) in self.dynamic.objects().iter().enumerate() {
            let name = format!("{:?}", object.id());
            if selector != "all" && selector != name && selector != object.mesh().model_path {
                continue;
            }
            let uniform = self
                .world_dynamic
                .as_ref()
                .and_then(|gpu| gpu.diagnostic_uniform(slot));
            self.trace_lighting_sample(
                &name,
                &format!("dynamic:{}", object.mesh().model_path),
                object.transform(),
                object.centre().to_array(),
                uniform,
            );
        }
        let entries = self
            .characters
            .characters()
            .iter()
            .enumerate()
            .map(|(slot, c)| (Some(slot), c))
            .chain(
                self.characters
                    .runtime_characters()
                    .iter()
                    .map(|c| (None, c)),
            );
        for (slot, character) in entries {
            let name = character.instance_id().unwrap_or("unnamed");
            if selector != "all" && selector != name && selector != character.asset().model_path {
                continue;
            }
            let uniform = self
                .world_characters
                .as_ref()
                .and_then(|gpu| gpu.diagnostic_uniform(name, slot));
            self.trace_lighting_sample(
                name,
                &format!("character:{}", character.asset().model_path),
                character.transform(),
                character.lighting_sample_position(),
                uniform,
            );
        }
    }

    fn trace_lighting_sample(
        &self,
        name: &str,
        path: &str,
        transform: glam::Mat4,
        position: [f32; 3],
        uniform: Option<&EnvironmentUniform>,
    ) {
        let Some(lighting) = self.dynamic_lighting.as_ref() else {
            return;
        };
        let room = lighting
            .room_index_at_height(position[0], position[1], position[2])
            .filter(|r| lighting.probe_region_at(*r, position).is_some());
        let sample = crate::engine::render::common::light_transport::entity_lighting(
            lighting,
            self.dynamic_field.as_deref(),
            position,
        );
        let candidates = room.and_then(|_| {
            self.dynamic_field.as_ref().map(|field| {
                field.sample_diagnostics_with_rooms(position, None, |p, label| {
                    lighting.labelled_probe_visible_from(position, p, label)
                })
            })
        });
        let cell = self.dynamic_field.as_ref().map(|field| {
            std::array::from_fn::<_, 3, _>(|axis| {
                ((position.get(axis).copied().unwrap_or(0.0)
                    - field.min.get(axis).copied().unwrap_or(0.0))
                    / field.cell_m
                    - 0.5)
                    .floor()
            })
        });
        let region = room.and_then(|r| lighting.probe_region_at(r, position));
        let uploaded = uniform.map(|u| (u.entity_irradiance, u.entity_moment, u.light_scale));
        let spatial = uniform.map(|u| serde_json::json!({
            "enabled": u.entity_bounds_min[3] > 0.5,
            "bounds_min": u.entity_bounds_min, "bounds_extent": u.entity_bounds_extent,
            "anchors": u.entity_anchors.iter().map(|anchor| serde_json::json!({
                "residual_irradiance_validity": anchor.irradiance, "residual_moment": anchor.moment,
                "tap_visibility": anchor.visibility, "attenuation": anchor.attenuation,
            })).collect::<Vec<_>>(),
            "sources": u.entity_direct.iter().map(|source| serde_json::json!({
                "position_range": source.position_range, "color_strength": source.color_strength,
                "extent_falloff": source.extent_falloff, "taps": source.taps,
            })).collect::<Vec<_>>(),
        }));
        let normals = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let directional = sample.prepared.map(|t| normals.map(|n| (n, t.light_at(n))));
        logging::info(format!(
            "[entity-light] name={name} path={path} map={:?} graphics={:?} transform={transform:?} sample_position={position:?} cell={cell:?} room={room:?} region={region:?} sample={sample:?} candidates={candidates:?} uploaded_energy_moment_scale={uploaded:?} pre_tonemap_normal_samples={directional:?}",
            self.level_id,
            self.graphics_applied.spec()
        ));
        logging::info(format!(
            "[entity-spatial] {}",
            serde_json::json!({
                "name": name, "path": path, "position": position,
                "centre_trace_meaning": "combined legacy probe at bounds centre; not final spatial shader response",
                "always_on_source_indices": self.dynamic_field.as_ref().map(|f| f.runtime_direct_lights()),
                "source_order_meaning": "always-on selected IDs, then unique prepared switchable IDs; disabled slots retain identity with zero descriptor",
                "uniform_bytes": std::mem::size_of::<EnvironmentUniform>(),
                "spatial": spatial,
            })
        ));
    }

    /// Reject capture work after a fatal device or drawable failure.
    fn validate_capture_device(&mut self) -> Result<(), String> {
        if self.check_device_lost() {
            return Err(self
                .fatal
                .clone()
                .unwrap_or_else(|| "GPU device unavailable for capture".to_string()));
        }
        validate_drawable(
            self.drawable_size,
            self.device.limits().max_texture_dimension_2d,
        )
    }

    /// Reads back the last rendered frame as a top-down RGBA8 image.
    ///
    /// The post path's presented image already carries the resolved scene and
    /// the HUD in raw display space, so the capture re-encodes that chain — the same pipelines, bind
    /// groups, camera state and CPU frustum test the last [`Self::render_scene`]
    /// used — and copies the presented image into an offscreen `Rgba8Unorm`
    /// capture texture with no transfer function, so the measured pixels are
    /// exactly the displayed values (an sRGB capture target would add a
    /// hardware conversion to every one). The direct fallback renders through the surface-format
    /// pipeline into an sRGB capture texture instead, exactly as it presents.
    /// Rendering offscreen rather than reading the acquired surface makes both
    /// paths work with `PLACES_BENCH_NOSWAP`.
    ///
    /// This is the one-shot `PLACES_CAPTURE` developer diagnostic, not a frame
    /// path: an ordinary frame creates no capture resource, and one capture
    /// allocates only for its own duration.
    ///
    /// # Errors
    ///
    /// Returns a message when the drawable is empty, no frame has been
    /// rendered, or the world pipeline/depth target has not been built.
    pub fn capture_default_framebuffer(
        &mut self,
    ) -> Result<crate::engine::loader::RawImage, String> {
        self.validate_capture_device()?;
        if let Ok(selector) = std::env::var("PLACES_ENTITY_LIGHT_TRACE") {
            self.trace_entity_lighting(&selector);
        }
        if self.drawable_size.is_empty() {
            return Err("drawable has zero size; nothing to capture".to_string());
        }
        let Some(frame) = self.last_frame else {
            return Err("no frame has been rendered yet; nothing to capture".to_string());
        };
        let Some(pipeline) = self.world_pipeline.as_ref() else {
            return Err("the world pipeline is not built; nothing to capture".to_string());
        };
        if self.depth.is_none() {
            return Err("the depth target does not exist; nothing to capture".to_string());
        }
        let width = self.drawable_size.width.max(1);
        let height = self.drawable_size.height.max(1);
        // The post path copies the raw presented image with no transfer
        // function, so its capture texture is raw too; only the direct
        // fallback renders through the surface-format pipeline and needs the
        // surface's format here.
        let post_presented = self
            .post
            .as_ref()
            .and_then(PostProcess::presented_view)
            .is_some();
        let format = if post_presented {
            PRESENTED_FORMAT
        } else {
            capture_format(pipeline.format())
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("capture-started");
        // The presented target already carries the frame *and* the HUD (the
        // post path blends the UI into it before encoding it to the surface),
        // so the post-path capture is its raw copy: display-space bytes
        // with no sRGB round trip. The
        // direct fallback re-renders the body and replays the last UI list.
        self.submit_capture_scene(&view, &frame);
        if post_presented {
            // The capture re-renders the resolved scene into the presented
            // target, replays the HUD there, then copies the presented image into
            // the capture target. Re-rendering rather than copying the live
            // presented image keeps `PLACES_BENCH_NOSWAP` captures working even
            // when no surface texture could be acquired.
            let presented = self
                .post
                .as_ref()
                .and_then(PostProcess::presented_view)
                .cloned();
            if let Some(presented_size) = presented {
                if !self.last_ui.is_empty() {
                    self.ensure_ui_pipeline(PRESENTED_FORMAT);
                    let presented_drawable = self.presented_drawable();
                    if let Some(ui) = self.ui.as_mut() {
                        let ui_vertices = std::mem::take(&mut self.last_ui);
                        let _draw_stats = ui.render(
                            &self.device,
                            &self.queue,
                            &presented_size,
                            &ui_vertices,
                            presented_drawable,
                            None,
                        );
                        self.last_ui = ui_vertices;
                    }
                }
                let mut copy =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("places-wgpu-capture-presented"),
                        });
                if let Some(post) = self.post.as_ref() {
                    post.encode_present_raw_to(&mut copy, &view);
                }
                let _submission_2 = self.queue.submit([copy.finish()]);
            }
        } else if !self.last_ui.is_empty() {
            self.ensure_ui_pipeline(self.config.format);
            if let Some(ui) = self.ui.as_mut() {
                let ui_vertices = std::mem::take(&mut self.last_ui);
                let _draw_stats_2 = ui.render(
                    &self.device,
                    &self.queue,
                    &view,
                    &ui_vertices,
                    self.drawable_size,
                    None,
                );
                self.last_ui = ui_vertices;
            }
        }
        let captured = read_back_rgba(&self.device, &self.queue, &texture, width, height, format);
        drop(view);
        drop(texture);
        #[cfg(feature = "visual-diagnostics")]
        self.trace_gpu_resources("capture-finished");
        captured
    }

    /// Submits the capture's scene and then receipts the exact encoded totals.
    fn submit_capture_scene(&self, view: &wgpu::TextureView, frame: &super::world::WorldFrame) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-wgpu-capture"),
            });
        #[cfg(feature = "visual-diagnostics")]
        let capture_totals = self.encode_frame_into(&mut encoder, view, frame, None);
        #[cfg(not(feature = "visual-diagnostics"))]
        let _ = self.encode_frame_into(&mut encoder, view, frame, None);
        let _submission = self.queue.submit([encoder.finish()]);
        #[cfg(feature = "visual-diagnostics")]
        self.trace_visual_diagnostic("capture", Some(&capture_totals));
    }

    /// Recreates the probe targets for `quality` and re-captures them.
    ///
    /// The offline compiler calls this after installing a world at Full to
    /// produce the Medium captures from the same prepared scene.
    pub fn reprepare_reflection_probes(&mut self, quality: ReflectionQuality) {
        if self.check_device_lost() {
            return;
        }
        self.apply_reflection_targets(quality);
        self.bake_reflection_probes();
    }

    /// Reads every resident probe cubemap back as RGBA16F faces.
    ///
    /// # Errors
    ///
    /// Returns an error when a face buffer cannot be mapped or read.
    pub fn read_back_probe_faces(
        &self,
    ) -> Result<Vec<super::reflections::ProbeFaceReadback>, String> {
        if self.fatal.is_some() || self.device_lost.lock().is_ok_and(|slot| slot.is_some()) {
            return Err("the GPU device is unavailable for probe read-back".to_string());
        }
        let mut probes = Vec::with_capacity(self.reflection_targets.probes().len());
        for probe in self.reflection_targets.probes() {
            let face_size = probe.face_size;
            let mut faces = Vec::with_capacity(6);
            for face in 0..6 {
                faces.push(read_back_cube_face(
                    &self.device,
                    &self.queue,
                    probe.texture(),
                    face_size,
                    0,
                    face,
                )?);
            }
            probes.push(super::reflections::ProbeFaceReadback {
                position: probe.position,
                face_size,
                faces: faces.try_into().map_err(|decoded_faces: Vec<Vec<u8>>| {
                    format!(
                        "probe read-back produced {} faces instead of six",
                        decoded_faces.len()
                    )
                })?,
            });
        }
        Ok(probes)
    }

    /// The fatal error that stopped the renderer, if any.
    #[must_use]
    pub fn fatal_error(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// Brings the surface configuration and the depth target up to date.
    ///
    /// This is the one place size-dependent wgpu resources are rebuilt: the
    /// surface configuration follows the configured size, and the depth target
    /// is recreated only when its size actually changed. Both are skipped while
    /// the drawable is zero-sized or the renderer is fatal.
    fn ensure_ready(&mut self) {
        if self.drawable_size.is_empty() || self.fatal.is_some() {
            return;
        }
        if let Err(error) = validate_drawable(
            self.drawable_size,
            self.device.limits().max_texture_dimension_2d,
        ) {
            self.pending_frame = None;
            self.mark_fatal(error);
            return;
        }
        if self.depth_size != self.drawable_size {
            self.depth = Some(Self::create_depth(&self.device, self.drawable_size));
            self.depth_size = self.drawable_size;
        }
        if self.needs_configure {
            // `Surface::configure` must not run while a texture from the
            // surface is alive, and the window may not be zero-sized.
            self.pending_frame = None;
            self.config.width = self.drawable_size.width;
            self.config.height = self.drawable_size.height;
            if let Some(surface) = self.surface.as_ref() {
                surface.configure(&self.device, &self.config);
            }
            self.needs_configure = false;
        }
    }

    /// Creates the main depth attachment at `size`.
    fn create_depth(device: &wgpu::Device, size: DrawableSize) -> DepthTarget {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-depth"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        DepthTarget {
            _texture: texture,
            view,
        }
    }

    /// Acquires the next surface texture, applying the recovery policy.
    ///
    /// At most one recovery attempt is made per call: a status that survives
    /// its own recovery skips the frame rather than looping.
    fn acquire_frame(&mut self) -> Acquired {
        self.acquire_frame_attempt(false)
    }

    /// A second attempt preserves loss/fatal handling without another retry.
    fn acquire_frame_attempt(&mut self, reconfigured: bool) -> Acquired {
        let Some(surface) = self.surface.as_ref() else {
            // Headless (compiler) renderer: no frame is ever acquired.
            return Acquired::Skip;
        };
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Acquired::Frame(texture),
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.recover(SurfaceStatus::Timeout, reconfigured)
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.recover(SurfaceStatus::Occluded, reconfigured)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.recover(SurfaceStatus::Outdated, reconfigured)
            }
            wgpu::CurrentSurfaceTexture::Lost => self.recover(SurfaceStatus::Lost, reconfigured),
            wgpu::CurrentSurfaceTexture::Validation => {
                self.recover(SurfaceStatus::Validation, reconfigured)
            }
        }
    }

    /// Applies the surface recovery policy for one non-success status.
    fn recover(&mut self, status: SurfaceStatus, reconfigured: bool) -> Acquired {
        let recovery = if reconfigured {
            status.recovery_after_reconfigure()
        } else {
            status.recovery()
        };
        match recovery {
            SurfaceRecovery::Reconfigure => self.retry_after_reconfigure(),
            SurfaceRecovery::Recreate => {
                logging::warn_once(
                    "wgpu-surface-lost",
                    "[wgpu] surface lost; recreating it on the next presentation",
                );
                self.surface_lost = true;
                Acquired::Skip
            }
            SurfaceRecovery::Fatal => {
                self.mark_fatal(
                    "wgpu reported a validation error while acquiring a surface texture"
                        .to_string(),
                );
                Acquired::Skip
            }
            SurfaceRecovery::Skip => {
                if status == SurfaceStatus::Timeout {
                    logging::warn_once(
                        "wgpu-surface-timeout",
                        "[wgpu] surface acquisition timed out; skipping frames until it recovers",
                    );
                } else if status == SurfaceStatus::Outdated {
                    logging::warn_once(
                        "wgpu-surface-outdated-retry",
                        "[wgpu] the surface did not recover after reconfiguring; skipping the frame",
                    );
                }
                Acquired::Skip
            }
        }
    }

    /// Reconfigures once and retries the acquisition once.
    fn retry_after_reconfigure(&mut self) -> Acquired {
        logging::warn_once(
            "wgpu-surface-outdated",
            "[wgpu] surface configuration outdated; reconfiguring once",
        );
        self.needs_configure = true;
        self.ensure_ready();
        if self.fatal.is_some() || self.needs_configure || self.drawable_size.is_empty() {
            Acquired::Skip
        } else {
            self.acquire_frame_attempt(true)
        }
    }

    /// Recreates the surface from the SDL window and reconfigures it.
    fn recreate_surface(&mut self) {
        self.surface_lost = false;
        // The old surface must not be replaced while one of its textures is
        // alive; the frame is dropped first.
        self.pending_frame = None;
        let Some(window) = self.window.as_ref() else {
            self.mark_fatal(
                "Cannot recreate a surface without its retained SDL window".to_string(),
            );
            return;
        };
        // SAFETY: the renderer retains this exact window until after surface drop.
        match unsafe { surface::create(&self.instance, window) } {
            Ok(recreated) => {
                let capabilities = recreated.get_capabilities(&self.adapter);
                if let Err(error) =
                    surface::refresh_configuration(&mut self.config, &capabilities, self.vsync)
                {
                    self.mark_fatal(error);
                    return;
                }
                self.invalidate_frame();
                self.capabilities = Some(capabilities);
                self.surface = Some(recreated);
                self.needs_configure = true;
                self.ensure_ready();
                logging::info(format!(
                    "[wgpu] surface recreated for the SDL window ({}x{})",
                    self.drawable_size.width, self.drawable_size.height
                ));
                #[cfg(feature = "visual-diagnostics")]
                self.trace_gpu_resources("surface-recreated");
            }
            Err(error) => {
                self.mark_fatal(format!("wgpu could not recreate the lost surface: {error}"));
            }
        }
    }

    /// Drains callback work so device-loss and validation reports are seen.
    fn poll_device(&mut self) {
        if let Err(error) = self.device.poll(wgpu::PollType::Poll) {
            logging::warn_once("wgpu-poll", format!("[wgpu] device poll failed: {error}"));
        }
        if !self.check_device_lost() {
            self.gpu_timer.collect_ready(&self.device, &self.queue);
        }
    }

    /// Explicit stress diagnostics sample once a second at 60 fps; normal
    /// builds compile out the field, branch and all counter work.
    #[cfg(feature = "visual-diagnostics")]
    fn trace_gpu_resource_frame(&mut self) {
        let Some(trace) = self.gpu_resource_trace.as_mut() else {
            return;
        };
        trace.frame = trace.frame.saturating_add(1);
        if trace.frame == 1 || trace.frame.is_multiple_of(60) {
            self.trace_gpu_resources("frame");
        }
    }

    /// HAL counters include resources kept alive by submitted work and bind
    /// groups. Logging is limited to opt-in lifecycle and periodic samples.
    #[cfg(feature = "visual-diagnostics")]
    fn trace_gpu_resources(&mut self, event: &str) {
        let Some(trace) = self.gpu_resource_trace.as_mut() else {
            return;
        };
        let counters = self.device.get_internal_counters().hal;
        let current = [
            counters.buffers.read(),
            counters.textures.read(),
            counters.texture_views.read(),
            counters.bind_groups.read(),
            counters.bind_group_layouts.read(),
            counters.render_pipelines.read(),
            counters.compute_pipelines.read(),
            counters.pipeline_layouts.read(),
            counters.samplers.read(),
            counters.command_encoders.read(),
            counters.shader_modules.read(),
            counters.query_sets.read(),
            counters.fences.read(),
            counters.memory_allocations.read(),
            counters.buffer_memory.read(),
            counters.texture_memory.read(),
        ];
        for (peak, live) in trace.peak.iter_mut().zip(current) {
            *peak = (*peak).max(live);
        }
        let frame = trace.frame;
        let peak = trace.peak;
        let names = [
            "buffers",
            "textures",
            "texture_views",
            "bind_groups",
            "bind_group_layouts",
            "render_pipelines",
            "compute_pipelines",
            "pipeline_layouts",
            "samplers",
            "command_encoders",
            "shader_modules",
            "query_sets",
            "fences",
            "memory_allocations",
            "buffer_bytes",
            "texture_bytes",
        ];
        let describe = |values: [isize; 16]| {
            serde_json::Value::Object(
                names
                    .into_iter()
                    .zip(values)
                    .map(|(name, value)| {
                        let supported_memory = self.adapter_info.backend != wgpu::Backend::Metal;
                        let encoded_value = if !supported_memory
                            && matches!(
                                name,
                                "buffer_bytes" | "texture_bytes" | "memory_allocations"
                            ) {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(value)
                        };
                        (name.to_string(), encoded_value)
                    })
                    .collect(),
            )
        };
        let receipt = serde_json::json!({
            "event": event, "frame": frame, "level": self.level_id,
            "backend": surface::native_backend_label(),
            "live": describe(current), "observed_peak": describe(peak),
            "memory_scope": "HAL-reported bytes where supported; Metal HAL memory counters unavailable. Texel budgets below exclude driver overhead and are partial, not summed allocations; active/pending caches may share sheets.",
            "texture_cache": self.textures.diagnostic_retention(),
            "pending_texture_cache": self.prepared_install.as_ref().map(|pending| pending.textures.diagnostic_retention()),
            "post_attachment_texel_bytes": self.post.as_ref().map_or(0, PostProcess::diagnostic_attachment_bytes),
            "atlas_texel_bytes": self.lightmaps.stats().resident_bytes,
            "drawable": [self.drawable_size.width, self.drawable_size.height],
            "staging": self.prepared_install.is_some(), "fatal": self.fatal,
        });
        logging::info(format!("[gpu-resources] {receipt}"));
    }

    /// Reports and records a device loss, once.
    ///
    /// Returns `true` while the renderer is fatal, so every facade call becomes
    /// a no-op instead of issuing GPU work on a lost device.
    fn check_device_lost(&mut self) -> bool {
        if self.fatal.is_some() {
            return true;
        }
        let reported = self.device_lost.lock().ok().and_then(|slot| slot.clone());
        if let Some(reported_size) = reported {
            self.mark_fatal(format!("wgpu device lost ({reported_size})"));
            return true;
        }
        false
    }

    /// Records the first fatal error and reports it exactly once.
    fn mark_fatal(&mut self, message: String) {
        if self.fatal.is_none() {
            self.invalidate_frame();
            self.prepared_install = None;
            logging::warn(format!("[wgpu] fatal device error: {message}"));
            self.fatal = Some(message);
            #[cfg(feature = "visual-diagnostics")]
            self.trace_gpu_resources("fatal");
        }
    }
}

/// The top mip level the shader may select for the installed probes.
///
/// The packaged chain's highest level when the resident face size has a
/// capture; zero when the probes are live-baked or no chain matches, which
/// keeps the shader's two-tap rough fallback.
fn packaged_probe_max_mip(
    captures: Option<&crate::engine::package::world::ProbeCaptures>,
    resident_face_size: Option<u32>,
) -> u32 {
    let (Some(probe_captures), Some(face_size)) = (captures, resident_face_size) else {
        return 0;
    };
    probe_captures
        .for_face_size(face_size)
        .map_or(0, |capture| capture.levels.saturating_sub(1))
}

/// Writes one packaged probe's mip chain into its resident cubemap.
///
/// Returns the number of levels written. Every level must carry six faces of
/// its halved edge; a malformed chain is an error rather than a partial silent
/// upload.
fn upload_probe_chain(
    probe: &ProbeCube,
    queue: &wgpu::Queue,
    chain: &[[Vec<u8>; 6]],
) -> Result<u32, String> {
    if chain.is_empty() {
        return Err("packaged probe chain has no levels".to_string());
    }
    for (level, faces) in chain.iter().enumerate() {
        let level_u32 = u32::try_from(level)
            .map_err(|error| format!("probe mip level is too large: {error}"))?;
        let Some(edge) = probe_mip_edge(probe.face_size, level_u32) else {
            return Err(format!(
                "probe chain has level {level}, past the {}-texel base",
                probe.face_size
            ));
        };
        if level_u32 >= probe.mip_levels {
            return Err(format!(
                "the resident probe has no mip level {level} for the payload"
            ));
        }
        let expected = usize::try_from(edge)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::try_from(edge).unwrap_or(usize::MAX))
            .saturating_mul(8);
        for (face, data) in faces.iter().enumerate() {
            if data.len() != expected {
                return Err(format!(
                    "probe level {level} face {face} holds {} bytes, expected {expected}",
                    data.len()
                ));
            }
            probe.write_face(queue, level_u32, face, data);
        }
    }
    u32::try_from(chain.len()).map_err(|error| format!("probe chain is too long: {error}"))
}

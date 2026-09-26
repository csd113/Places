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
//! three material passes with the reference's alpha states and translucent
//! ordering, with the WGSL reproducing the reference's material colour and
//! preparing the material normal. The surface/device path is independent of
//! that content.
//!
//! Everything wgpu-specific lives under `render::wgpu`; the engine reaches it
//! only through `render::Renderer`. The backend draws the whole reference frame
//! (lightmaps, reflections, props, dynamics, fixtures, emission, decals, fog,
//! post-processing and the HUD).

use std::sync::{Arc, Mutex};

use sdl3::video::Window;

use super::character::{CharacterUploadContext, WgpuCharacters};
use super::decals::{DecalPipeline, WgpuDecals};
use super::dynamic::{DynamicUploadContext, WgpuDynamic};
use super::environment::{
    EnvironmentBindings, fallback_planar, fallback_probe, static_environment,
};
use super::lightmap::{LightmapAtlas, needs_upload_fallback};
use super::material::{WorldMaterialInputs, WorldMaterials, material_bind_group_layout};
use super::postprocess::{EMISSIVE_FORMAT, PostProcess, SCENE_FORMAT};
use super::props::WgpuProps;
use super::reflections::{
    CaptureFrame, ProbeCube, ReflectionTargets, planar_size_for, planar_view_projection,
    probe_bake_position, probe_face_size, probe_face_view_projection,
};
use super::surface::{
    self, CLEAR_COLOR, CLEAR_COLOR_SRGB, DEPTH_FORMAT, SurfaceRecovery, SurfaceStatus,
    present_mode_label,
};
use super::texture::{TextureCache, TextureFiltering};
use super::ui::UiRenderer;
use super::world::{
    WgpuWorldGeometry, WorldDrawTotals, WorldPipeline, WorldTextures,
    environment_bind_group_layout, prepare_world_frame,
};
use crate::game::LocomotionSnapshot;
use crate::lighting::lightmap::{LightmapCache, LightmapFailure, LightmapMode};
use crate::loader::LoadedLevel;
use crate::logging;
use crate::quality::{LightmapQuality, QualityLevel, ReflectionQuality, TextureClass};
use crate::render::RenderCamera;
use crate::render::common::SurfaceKind;
use crate::render::common::Vertex;
use crate::render::common::animation::{AnimationEffect, EmissionAnimation};
use crate::render::common::api::{
    GraphicsTransition, LevelBuild, LightmapBuildOptions, LightmapFillOutcome, LightmapFillRequest,
    LightmapFillWorker, PreparedLightmapBuild, build_level_geometry_timed_with_lightmaps,
    dump_lightmaps_for_level, fill_lightmaps, fill_may_activate, level_content_fingerprint,
    prepare_level_geometry_with_lightmaps, rebuild_vertex_lit_level, retained_build_matches_level,
};
use crate::render::common::atmosphere::FogState;
use crate::render::common::character::{CharacterScene, EntityFrame};
use crate::render::common::dynamic::{DynamicScene, DynamicUpdate};
use crate::render::common::materials::MaterialRenderState;
use crate::render::common::postprocess::PostSettings;
use crate::render::common::reflections::{
    Reflections, nearest_visible_reflection_plane, routing_from_mesh,
};
use crate::render::common::stats::{LevelBuildStats, RenderStats};
use crate::render::common::view::DrawableSize;

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
/// reference's RGBA byte order, exactly like the OpenGL capture's
/// `glReadPixels(GL_RGBA, GL_UNSIGNED_BYTE)`.
const fn capture_format(surface_format: wgpu::TextureFormat) -> wgpu::TextureFormat {
    surface_format
}

/// True when a mapped `CAPTURE_FORMAT` row stores its red and blue channels
/// swapped and must be reordered for the RGBA image.
const fn capture_needs_bgra_swizzle(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
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
/// The setters only record into this value; [`WgpuRenderer::apply_graphics`]
/// is the one place that diffs it against the applied configuration and does
/// the work. Keeping both sides as one plain `Copy` value is what makes a
/// multi-setting change (a quality preset cascades Lightmaps and Reflections)
/// one diff and, when a rebuild is needed, one CPU build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GraphicsConfig {
    /// Overall quality: texture budgets, scene target, surface response.
    quality: QualityLevel,
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
            filtering: TextureFiltering::DEFAULT,
            bloom: true,
            lightmaps: LightmapQuality::default(),
            reflections: ReflectionQuality::default(),
        }
    }
}

impl GraphicsConfig {
    /// What changed between the applied configuration and `next`.
    fn delta_from(self, next: Self) -> GraphicsDelta {
        GraphicsDelta {
            quality: self.quality != next.quality,
            filtering: self.filtering != next.filtering,
            bloom: self.bloom != next.bloom,
            lightmaps: self.lightmaps != next.lightmaps,
            reflections: self.reflections != next.reflections,
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
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GraphicsDelta {
    /// Texture budgets, scene target, surface response.
    quality: bool,
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
    /// Every flag set, for a first upload or a new level: nothing is resident.
    const fn everything() -> Self {
        Self {
            quality: true,
            filtering: true,
            bloom: true,
            lightmaps: true,
            reflections: true,
        }
    }

    /// True when no setting changed.
    const fn is_empty(self) -> bool {
        !(self.quality || self.filtering || self.bloom || self.lightmaps || self.reflections)
    }

    /// True when any GPU resource work is owed.
    ///
    /// Filtering is a sampler handle the bind groups swap at bind time, and
    /// bloom is a per-frame post gate, so neither ever rebuilds a resource:
    /// this is the predicate that proves a filtering-only or bloom-only change
    /// is zero-work.
    const fn needs_gpu_work(self) -> bool {
        self.quality || self.lightmaps || self.reflections
    }

    /// True when the CPU level build must run again (lighting, props and the
    /// plan-stamped mesh). Only the lightmap configuration changes the mesh.
    const fn needs_build(self) -> bool {
        self.lightmaps
    }

    /// True when the retained CPU build can be kept and its GPU textures
    /// re-fitted at the new quality budget.
    const fn needs_texture_refit(self) -> bool {
        self.quality
    }
}

/// A CPU level build whose lightmap fill is running on a worker.
///
/// The staged build is deliberately not the world on screen: the previous
/// configuration keeps rendering, with its own atlas, until the fill lands and
/// [`WgpuRenderer::install_level`] swaps everything in one step. A cancelled or
/// superseded stage is dropped without ever touching the GPU.
struct StagedLightmapFill {
    /// The prepared build: plan-stamped mesh, lighting and timings, no atlas.
    build: LevelBuild,
    /// The exact request the worker is filling (also its cache key).
    request: LightmapFillRequest,
    /// Which Lightmaps setting this build was prepared for.
    lightmaps: LightmapQuality,
    /// Monotonic generation this request was issued under.
    generation: u64,
    /// When the CPU build started, so the install reports the whole build cost.
    started: std::time::Instant,
    /// The loaded level the staged build belongs to, so the completion poll
    /// can install it without the game loop handing the level in again.
    loaded: LoadedLevel,
}

/// The wgpu implementation of the renderer facade.
///
/// It owns the device/surface lifecycle and the world material passes.
/// Field order is drop order: the surface (and its swapchain resources) drops
/// before the device and the instance. The SDL window is never owned here; see
/// [`surface::create`] for the lifetime invariant that keeps the raw handles
/// valid.
///
/// The flags below are independent pieces of renderer state — whether the
/// surface needs reconfiguring, the player's `VSync` preference, whether the
/// surface was reported lost, and whether the CPU frustum test runs — not a
/// state machine with one active variant, so an enum per flag would be worse.
#[allow(clippy::struct_excessive_bools)]
pub struct WgpuRenderer {
    /// The frame acquired between `render_scene` and `present`.
    ///
    /// Declared before the surface on purpose: an unpresented frame is a
    /// `SurfaceTexture` that must be discarded *before* the surface it came
    /// from is destroyed (its drop reaches back into the surface's swapchain).
    /// Rust drops fields in declaration order, so this releases the frame first
    /// on an unclean shutdown (`PLACES_BENCH_NOSWAP`, a mid-frame fatal error).
    pending_frame: Option<wgpu::SurfaceTexture>,
    surface: wgpu::Surface<'static>,
    capabilities: wgpu::SurfaceCapabilities,
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
    /// The static world mesh uploaded for the loaded level, if any.
    world: Option<WgpuWorldGeometry>,
    /// The loaded level's uploaded prop batches, if any.
    world_props: Option<WgpuProps>,
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
    /// The renderer-owned texture cache: layout, shared samplers, fallback and
    /// every uploaded texture. Renderer lifetime; pack entries are dropped per
    /// level.
    textures: TextureCache,
    /// The player's filtering setting, selecting the world's shared sampler at
    /// bind time (the lightmap sampler never follows it).
    filtering: TextureFiltering,
    /// Prop model cache used only to build the neutral level geometry.
    prop_catalog: crate::loader::PropCatalog,
    prop_assets: crate::props::PropAssets,
    /// The reference's per-level lightmap cache (memory + disk), owned exactly
    /// like the OpenGL renderer owns its own.
    lightmap_cache: LightmapCache,
    /// The graphics configuration the setters have recorded (requested).
    ///
    /// All setters are cheap writes here; nothing in this file compares
    /// individual settings outside [`Self::apply_graphics`].
    graphics_requested: GraphicsConfig,
    /// The graphics configuration whose resources are resident or scheduled.
    ///
    /// Updated at the end of [`Self::apply_graphics`], so a repeated call with
    /// the same request — even while a lightmap fill runs — is a no-op instead
    /// of restarting work.
    graphics_applied: GraphicsConfig,
    /// The quality level the resident GPU textures were fitted at.
    ///
    /// Compared against the requested level at install time to decide whether
    /// the texture caches must be released; a lightmap-only rebuild at the same
    /// quality reuses every cached texture.
    installed_quality: QualityLevel,
    /// The last completed CPU level build (mesh, batches, lighting, lightmaps,
    /// timings) and the level it belongs to.
    ///
    /// This is what lets a texture-budget-only quality change skip the
    /// expensive lighting bake: the mesh is re-uploaded with new texture
    /// budgets instead of being rebuilt.
    retained_build: Option<LevelBuild>,
    /// The Lightmaps setting `retained_build` was prepared for; a request for
    /// the same value never re-bakes.
    retained_lightmaps: LightmapQuality,
    /// The level id `retained_build` belongs to.
    retained_level_id: Option<String>,
    /// Stable fingerprint of the level definition `retained_build` was built
    /// from. A reload of the same id with edited content must rebuild instead
    /// of reusing the stale build; see [`retained_build_matches_level`].
    retained_level_fingerprint: Option<u64>,
    /// A prepared build whose atlas fill is running on [`Self::lightmap_worker`].
    staged_lightmap: Option<StagedLightmapFill>,
    /// The single live fill worker, if any. Dropping it cancels and joins.
    lightmap_worker: Option<LightmapFillWorker>,
    /// Monotonic generation of lightmap requests; a result from an older one
    /// is discarded instead of activated.
    lightmap_generation: u64,
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
    /// capture so it reads back what the window showed (the reference reads the
    /// default framebuffer after the UI). Reused capacity, never per-frame
    /// allocation.
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
    /// The frame/level environment bindings for the loaded level.
    environment: Option<EnvironmentBindings>,
    /// The 1x1 black cube bound while no probe is resident (renderer lifetime).
    _probe_fallback: wgpu::Texture,
    probe_fallback_view: wgpu::TextureView,
    /// The 1x1 white planar image bound while no mirror is active.
    _planar_fallback: wgpu::Texture,
    planar_fallback_view: wgpu::TextureView,
    /// One GPU sheet per fixture family for the loaded level, indexed by
    /// [`crate::lighting::FixtureKind::index`]; the white fallback for a family
    /// the level does not use.
    fixture_sheets: Vec<std::sync::Arc<super::texture::GpuTexture>>,
    /// The loaded level's emission animations, indexed by material index.
    material_animations: Vec<Option<EmissionAnimation>>,
    /// Seconds of emission-animation time accumulated since the level load.
    animation_seconds: f32,
    /// The level's atmosphere, applied by every world draw.
    fog: FogState,
    /// Whether the player asked for bloom. The emissive/bloom pass gates on it;
    /// recorded here so `set_bloom_enabled` can be applied without a rebuild,
    /// exactly like the reference's `bloom_enabled` flag.
    bloom_requested: bool,
    /// Whether the player asked for reflections. The probe and planar
    /// resources live in `render::wgpu::reflections`; the flag is recorded
    /// here and gates the environment's reflection sampling.
    reflections_enabled: bool,
    /// The level's reflection routing and gates (neutral data).
    reflections: Reflections,
    /// The level's probe cubemaps and planar target.
    reflection_targets: ReflectionTargets,
    /// The world pipeline set both reflection captures run through: no
    /// culling, reversed front face (the probe projection's Y flip and the
    /// planar mirror both reverse winding, like the reference's
    /// `glFrontFace(GL_CW)` during the planar capture), the reflection colour
    /// format.
    capture_pipeline: Option<WorldPipeline>,
    /// The mirror plane selected for the last rendered frame, if any.
    active_plane: Option<usize>,
    /// The probe cubemap selected for the last rendered frame (the reference's
    /// nearest-probe rule), an index into the level's probe list.
    active_probe: usize,
    /// Capture passes submitted since the level loaded, for the neutral stats.
    reflection_passes: usize,
    /// Whether the CPU frustum test is applied to the world draw ranges.
    culling: bool,
    /// The active quality level. The world geometry does not vary by level;
    /// the level gates the surface response, fits textures and sizes the
    /// offscreen targets. Kept so `set_quality` is recorded and the build can
    /// name it.
    quality: QualityLevel,
    /// The last level upload's counters.
    level_stats: LevelBuildStats,
    /// The last submitted frame's counters.
    render_stats: RenderStats,
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
    /// `set_dynamic_demo`; the GPU side lives in `world_dynamic`.
    dynamic: DynamicScene,
    /// The id of the level currently resident. A `set_level` for a different
    /// level clears the neutral dynamic scene; a quality rebuild of the same
    /// level keeps it, so its objects survive a quality change.
    level_id: Option<String>,
    /// The GPU side of the dynamic scene: one mesh per model, one environment
    /// per object. Built when the demo spawns, dropped with the level.
    world_dynamic: Option<WgpuDynamic>,
    /// The neutral character scene: every placed skinned prop, its animator
    /// and its baked per-vertex albedo. Rebuilt on level install and graphics
    /// changes; advanced by `update_characters`.
    characters: CharacterScene,
    /// The GPU side of the character scene: shared index buffers, one mutable
    /// skinned vertex buffer per character and one environment per character.
    world_characters: Option<WgpuCharacters>,
    /// The level's CPU bake, kept for the dynamic objects' light probes. The
    /// static path needs it only while building; a moving object samples it as
    /// it moves, exactly like the reference.
    dynamic_lighting: Option<crate::lighting::LevelLighting>,
    /// The frame state of the last submitted `render_scene`, so the one-shot
    /// capture can re-encode the same view. `None` until a frame has been
    /// rendered, and after a level or size change that invalidated the state.
    last_frame: Option<super::world::WorldFrame>,
    adapter_info: wgpu::AdapterInfo,
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
    #[allow(clippy::too_many_lines)] // one cohesive device/surface bootstrap
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

        // SAFETY: the SDL window outlives this renderer in `main`; see
        // `surface::create`.
        let wgpu_surface = unsafe { surface::create(&instance, window) }?;
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
        let (probe_fallback, probe_fallback_view) = fallback_probe(&device);
        let (planar_fallback, planar_fallback_view) = fallback_planar(&device, &queue);
        let lightmaps = LightmapAtlas::upload(&device, &queue, None);

        let (width, height) = window.size_in_pixels();
        let drawable_size = DrawableSize::new(width, height);
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

        let renderer = Self {
            surface: wgpu_surface,
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
            world: None,
            world_props: None,
            world_textures: None,
            world_materials: None,
            material_layout,
            environment_layout,
            textures,
            filtering: TextureFiltering::DEFAULT,
            prop_catalog: crate::loader::PropCatalog::load_default(),
            prop_assets: crate::props::PropAssets::load_default(),
            lightmap_cache: LightmapCache::with_disk(),
            graphics_requested: GraphicsConfig::default(),
            graphics_applied: GraphicsConfig::default(),
            installed_quality: QualityLevel::default(),
            retained_build: None,
            retained_lightmaps: LightmapQuality::default(),
            retained_level_id: None,
            retained_level_fingerprint: None,
            staged_lightmap: None,
            lightmap_worker: None,
            lightmap_generation: 0,
            lightmaps_resident: false,
            lightmaps,
            decals: None,
            decal_pipeline: None,
            decal_reflection_pipeline: None,
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
            material_animations: Vec::new(),
            animation_seconds: 0.0,
            fog: FogState::SHIPPED,
            bloom_requested: true,
            reflections_enabled: true,
            reflections: Reflections::default(),
            reflection_targets: ReflectionTargets::default(),
            capture_pipeline: None,
            active_plane: None,
            active_probe: 0,
            reflection_passes: 0,
            culling: true,
            quality: QualityLevel::default(),
            level_stats: LevelBuildStats::default(),
            render_stats: RenderStats::default(),
            needs_configure: !drawable_size.is_empty(),
            vsync: true,
            pending_frame: None,
            surface_lost: false,
            fatal: None,
            device_lost,
            dynamic: DynamicScene::new(),
            level_id: None,
            world_dynamic: None,
            characters: CharacterScene::new(),
            world_characters: None,
            dynamic_lighting: None,
            last_frame: None,
            adapter_info,
        };
        renderer.log_startup();
        Ok(renderer)
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
            if let Ok(mut slot) = writer.lock()
                && slot.is_none()
            {
                *slot = Some(format!("{reason:?}: {message}"));
            }
        });
        slot
    }

    /// Emits the one startup diagnostic for the wgpu path.
    ///
    /// Developer telemetry: printed only when `PLACES_VERBOSE` is set. The
    /// adapter line is what proves which native backend actually initialized.
    fn log_startup(&self) {
        logging::info(format!(
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
        // The world fragment stage assembles the reference's display-space
        // colour and relies on an sRGB target decoding it. An adapter that
        // offers only a linear format would present the linear values raw; that
        // is a visible colour difference, so it is reported rather than passed
        // over. (No shipping desktop adapter is known to hit this.)
        if !surface::surface_format_is_srgb(self.config.format) {
            logging::warn(format!(
                "[wgpu] surface format {:?} is not sRGB; the world shader writes display-space values \
                 and they will be presented as if linear (the display-space colour contract cannot be honoured)",
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
    /// synchronized FIFO path, `0` for immediate), matching the reference's
    /// reported swap interval.
    pub fn set_swap_interval(&mut self, want_vsync: bool) -> i32 {
        self.vsync = want_vsync;
        let mode = surface::select_present_mode(&self.capabilities, want_vsync);
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
        if size.is_empty() {
            // The window holds no drawable while minimized: drop any acquired
            // frame and wait for a real size.
            self.pending_frame = None;
        }
        true
    }

    /// The current dynamic scene (empty until `set_dynamic_demo` spawns
    /// objects; the static world is uploaded separately).
    #[must_use]
    pub const fn dynamic_scene(&self) -> &DynamicScene {
        &self.dynamic
    }

    /// Advances the dynamic scene and the emission-animation clock.
    ///
    /// The neutral scene updates transforms and refreshes a moved object's
    /// baked-light probe; the GPU side then writes each object's new model
    /// matrix and light scale into its environment uniform. A still scene
    /// writes nothing.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.animation_seconds += delta_seconds;
        }
        // Floats ride an absolute clock before the spin/probe pass, so a
        // moved float's probe is sampled at this frame's surface pose.
        self.dynamic.update_floats(self.animation_seconds);
        let update = self
            .dynamic
            .update(delta_seconds, self.dynamic_lighting.as_ref());
        if let Some(dynamic) = self.world_dynamic.as_mut() {
            dynamic.sync(
                &self.queue,
                &self.dynamic,
                self.lightmaps_resident,
                self.fog,
            );
        }
        update
    }

    /// Spawns the level's dynamic demonstration objects and uploads them.
    ///
    /// The reference's `set_dynamic_demo`: one spinning washer drum in front of
    /// every placed machine. Returns how many objects spawned.
    pub fn set_dynamic_demo(&mut self, level: &crate::level::LevelDef) -> usize {
        if self.check_device_lost() {
            return 0;
        }
        self.dynamic.clear_all();
        self.world_dynamic = None;
        let spawned =
            self.dynamic
                .spawn_washer_drum_demo(level, &self.prop_catalog, &mut self.prop_assets);
        if spawned > 0 {
            self.upload_dynamic();
        }
        spawned
    }

    /// Spawns every placed prop that authors `float` on its water surface.
    ///
    /// Runs for every level (a float is a level-authored prop, not a
    /// demonstration), clears any previous float set first so it is idempotent
    /// across level reloads, and uploads when anything changed. Returns how
    /// many floats spawned.
    pub fn set_floating_props(&mut self, level: &crate::level::LevelDef) -> usize {
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

    /// Uploads the dynamic scene's GPU meshes, materials and environments.
    ///
    /// Called after the static level exists, once per dynamic spawn.
    fn upload_dynamic(&mut self) {
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
        // validation error (and a feedback loop GL tolerated but this port must
        // not). Every level load already bound the fallback because the planar
        // target is created after it; this keeps that behaviour stable across a
        // later re-upload instead of depending on the target's creation timing.
        let planar = &self.planar_fallback_view;
        let mut ctx = DynamicUploadContext {
            device: &self.device,
            queue: &self.queue,
            cache: &mut self.textures,
            material_layout: &self.material_layout,
            environment_layout: &self.environment_layout,
            lightmaps: &self.lightmaps,
            lightmap_enabled: self.lightmaps_resident,
            probes: &probe_views,
            planar,
            probe_fallback: &self.probe_fallback_view,
            planar_fallback: &self.planar_fallback_view,
            level: self.quality,
            fog: self.fog,
        };
        self.world_dynamic = Some(WgpuDynamic::upload(&mut ctx, &self.dynamic));
    }

    /// Advances every character's pose and uploads the characters that moved.
    ///
    /// The neutral scene evaluates the blend weights, gait phase and clip
    /// crossfade; the GPU side re-skins only the characters whose pose
    /// revision changed and writes their vertex buffers and live transforms.
    /// A still character writes nothing. Returns how many characters moved
    /// this pass.
    pub fn update_characters(
        &mut self,
        delta_seconds: f32,
        locomotion: LocomotionSnapshot,
        frames: &[EntityFrame],
    ) -> usize {
        let update = self.characters.update(delta_seconds, locomotion, frames);
        if let Some(characters) = self.world_characters.as_mut() {
            characters.sync(&self.queue, &self.characters);
        }
        update.moved
    }

    /// The number of live characters in the current level.
    #[must_use]
    pub const fn character_count(&self) -> usize {
        self.characters.len()
    }

    /// The character scene, for diagnostics and tests.
    #[must_use]
    pub const fn character_scene(&self) -> &CharacterScene {
        &self.characters
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
        let mut ctx = CharacterUploadContext {
            device: &self.device,
            queue: &self.queue,
            cache: &mut self.textures,
            material_layout: &self.material_layout,
            environment_layout: &self.environment_layout,
            lightmaps: &self.lightmaps,
            lightmap_enabled: self.lightmaps_resident,
            probes: &probe_views,
            planar,
            probe_fallback: &self.probe_fallback_view,
            planar_fallback: &self.planar_fallback_view,
            level: self.quality,
            fog: self.fog,
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

    /// Applies every recorded graphics setting in one transaction.
    ///
    /// This is the one entry point for a settings action. It diffs the
    /// requested configuration (recorded by the setters) against the applied
    /// one and does exactly the work the difference implies:
    ///
    /// * Texture Filtering and Bloom are frame gates — the sampler handle every
    ///   bind group swaps at bind time and the post settings — so they owe no
    ///   resource work at all.
    /// * Reflections retire/create the probe cubemaps and the planar target,
    ///   rebake the probes and rebuild the environment bind groups.
    /// * Quality, when the lightmap configuration did not change, reuses the
    ///   retained CPU build and re-fits its GPU textures at the new budget.
    /// * Lightmaps rebuilds the CPU level (lighting bake, props, plan-stamped
    ///   mesh) once. A cache hit activates immediately; a miss stages it and
    ///   fills on one worker while the previous world keeps rendering.
    ///
    /// A first upload or a new level (a `retained_level_id` mismatch) forces the
    /// full synchronous path, exactly the historical `set_level`: a fresh level
    /// has no previous world to keep on screen, so its atlas fill runs inline.
    pub fn apply_graphics(&mut self, loaded: &LoadedLevel) {
        if self.check_device_lost() {
            return;
        }
        let requested = self.graphics_requested;
        // A retained build may be reused only for the exact level content it was
        // made from. The id alone is not enough: re-selecting the current level
        // after its file was edited keeps the id, and the game resets its
        // collision to the new definition, so a stale build would draw the old
        // level while the player collides with the new one.
        let force = self.retained_build.is_none()
            || !retained_build_matches_level(
                self.retained_level_id.as_deref(),
                self.retained_level_fingerprint,
                &loaded.level,
            );
        let delta = if force {
            GraphicsDelta::everything()
        } else {
            self.graphics_applied.delta_from(requested)
        };
        if delta.is_empty() {
            return;
        }
        if !force && !delta.needs_gpu_work() {
            // Filtering and Bloom are already in force — the sampler handle and
            // the post gate read the recorded values — so this is the zero-work
            // branch: recording the applied configuration is the whole change.
            self.graphics_applied = requested;
            return;
        }

        if delta.reflections {
            self.apply_reflection_targets(requested.reflections);
        }

        let build_started = force || delta.needs_build();
        if build_started {
            // A new lightmap request supersedes any fill still in flight; the
            // previous world keeps rendering until the new atlas is installed.
            self.cancel_staged_lightmap();
            let started = std::time::Instant::now();
            let PreparedLightmapBuild { build, fill } =
                self.prepare_level_build(loaded, requested.lightmaps);
            match fill {
                None => {
                    // Off, a plan failure or a cache hit: complete right now.
                    let build =
                        self.install_level(loaded, build, requested.lightmaps, true, started);
                    self.retain_build(loaded, build, requested.lightmaps);
                }
                Some(request) if force => {
                    // A fresh level has no previous world to keep drawing: the
                    // historical load path fills inline, exactly as before.
                    let build = self.finish_lightmap_fill_inline(
                        loaded,
                        build,
                        &request,
                        requested.lightmaps,
                        started,
                    );
                    self.retain_build(loaded, build, requested.lightmaps);
                }
                Some(request) => {
                    self.stage_lightmap_fill(loaded, build, request, requested.lightmaps, started);
                }
            }
        } else if delta.needs_texture_refit() {
            self.install_retained(loaded);
        } else if delta.reflections {
            // A reflection-only change: the targets are already applied; rebuild
            // the environment bind groups and rebake the probes against the
            // world that is already resident.
            self.refresh_reflection_resources();
        }

        self.graphics_applied = requested;
        logging::info(format!(
            "[settings] graphics applied: quality '{}', lightmaps '{}', reflections '{}', filtering '{}', bloom {}",
            requested.quality.name(),
            requested.lightmaps.name(),
            requested.reflections.name(),
            requested.filtering.name(),
            if requested.bloom { "on" } else { "off" }
        ));
    }

    /// Polls the one asynchronous graphics stage, if any, and installs a
    /// finished lightmap fill.
    ///
    /// Called once per frame (at the top of [`Self::render_scene`]); idling is
    /// two `Option` checks. The atlas upload and the GPU resource swap happen
    /// here, on the main thread, and a result is discarded unless its
    /// generation is the newest request, so a superseded fill can never
    /// activate.
    pub fn advance_graphics_transition(&mut self) {
        if self.staged_lightmap.is_none() && self.lightmap_worker.is_none() {
            return;
        }
        let Some(staged) = self.staged_lightmap.take() else {
            // A worker without a staged build cannot happen; drop the stale
            // worker so it cannot hold a thread.
            self.lightmap_worker = None;
            return;
        };
        let outcome = self
            .lightmap_worker
            .as_mut()
            .and_then(LightmapFillWorker::try_take);
        let Some(outcome) = outcome else {
            // Still filling: the previous world keeps rendering.
            self.staged_lightmap = Some(staged);
            return;
        };
        self.lightmap_worker = None;
        if !fill_may_activate(staged.generation, self.lightmap_generation) {
            // A superseded fill finished just before its cancellation: discard
            // it and let the newest request own the world.
            return;
        }
        match outcome {
            LightmapFillOutcome::Filled(filled) => {
                let filled = Arc::new(filled);
                self.lightmap_cache
                    .insert(&staged.request.content_key, Arc::clone(&filled));
                dump_lightmaps_for_level(&staged.loaded.level, &filled);
                let mut build = staged.build;
                let bake_millis = filled.stats.bake_millis;
                build.lightmap_millis = bake_millis;
                build.lightmaps = Some(filled);
                let build = self.install_level(
                    &staged.loaded,
                    build,
                    staged.lightmaps,
                    true,
                    staged.started,
                );
                self.retain_build(&staged.loaded, build, staged.lightmaps);
                logging::info(format!(
                    "[settings] lightmaps '{}' filled in {bake_millis:.1} ms on the worker; atlas installed",
                    staged.lightmaps.name()
                ));
            }
            LightmapFillOutcome::Failed(failure) => {
                logging::warn_once(
                    "lightmap-fill-failed",
                    format!(
                        "[lightmaps] '{}' fill failed ({}); rebuilding the vertex-lit level",
                        staged.loaded.level.id,
                        failure.name()
                    ),
                );
                let mut build = staged.build;
                build.lightmap_failure = Some(failure);
                build.lightmaps = None;
                let mesh_started = std::time::Instant::now();
                let mesh = rebuild_vertex_lit_level(
                    &staged.loaded.level,
                    &self.prop_catalog,
                    &mut self.prop_assets,
                    &staged.loaded.materials,
                    &build.lighting,
                );
                build.timings.surfaces_millis = mesh_started
                    .elapsed()
                    .as_secs_f64()
                    .mul_add(1000.0, build.timings.surfaces_millis);
                build.mesh = mesh;
                let build = self.install_level(
                    &staged.loaded,
                    build,
                    staged.lightmaps,
                    true,
                    staged.started,
                );
                self.retain_build(&staged.loaded, build, staged.lightmaps);
            }
            LightmapFillOutcome::Cancelled => {
                // Nothing to install; the newest request already replaced this
                // stage.
            }
        }
    }

    /// What the renderer is doing about a graphics change, for a status hint.
    ///
    /// Cheap: two `Option` checks. The game loop may show a subtle
    /// "Applying..." message while a background lightmap fill runs; the
    /// previous configuration keeps rendering throughout.
    #[must_use]
    pub const fn graphics_transition_status(&self) -> GraphicsTransition {
        if self.staged_lightmap.is_some() {
            GraphicsTransition::Preparing("lightmaps")
        } else {
            GraphicsTransition::Idle
        }
    }

    /// Runs the CPU half of a lightmap build: lighting bake, prop instancing,
    /// the plan-stamped mesh and the cache lookup.
    fn prepare_level_build(
        &mut self,
        loaded: &LoadedLevel,
        lightmaps: LightmapQuality,
    ) -> PreparedLightmapBuild {
        prepare_level_geometry_with_lightmaps(
            &loaded.level,
            &self.prop_catalog,
            &mut self.prop_assets,
            &loaded.materials,
            LightmapBuildOptions::for_lightmaps(lightmaps),
            Some(&mut self.lightmap_cache),
        )
    }

    /// Stages a prepared build and starts its one worker fill.
    ///
    /// The previous world keeps rendering. A refused thread falls back to the
    /// inline fill, which costs one blocking frame but never leaves the setting
    /// half applied.
    fn stage_lightmap_fill(
        &mut self,
        loaded: &LoadedLevel,
        build: LevelBuild,
        request: LightmapFillRequest,
        lightmaps: LightmapQuality,
        started: std::time::Instant,
    ) {
        if let Some(worker) = LightmapFillWorker::spawn(request.clone()) {
            self.lightmap_generation = self.lightmap_generation.saturating_add(1);
            let generation = self.lightmap_generation;
            let charts = request.charts.len();
            self.lightmap_worker = Some(worker);
            self.staged_lightmap = Some(StagedLightmapFill {
                build,
                request,
                lightmaps,
                generation,
                started,
                loaded: loaded.clone(),
            });
            logging::info(format!(
                "[settings] lightmaps '{}': {} chart(s) prepared in {:.1} ms, filling on a worker; the previous world keeps rendering",
                lightmaps.name(),
                charts,
                started.elapsed().as_secs_f64().mul_add(1000.0, 0.0)
            ));
            return;
        }
        logging::warn_once(
            "lightmap-worker-thread",
            "[settings] could not start the lightmap fill thread; filling inline this frame",
        );
        let build = self.finish_lightmap_fill_inline(loaded, build, &request, lightmaps, started);
        self.retain_build(loaded, build, lightmaps);
    }

    /// Fills a prepared build on the calling thread and installs it.
    ///
    /// The historical load path: only used for a fresh level (no previous world
    /// to keep on screen) or when no worker thread could be started. On a fill
    /// failure the historical vertex-lit mesh is rebuilt against the same
    /// lighting, exactly like the inline entry point.
    fn finish_lightmap_fill_inline(
        &mut self,
        loaded: &LoadedLevel,
        mut build: LevelBuild,
        request: &LightmapFillRequest,
        lightmaps: LightmapQuality,
        started: std::time::Instant,
    ) -> LevelBuild {
        match fill_lightmaps(request) {
            Ok(filled) => {
                let filled = Arc::new(filled);
                dump_lightmaps_for_level(&loaded.level, &filled);
                self.lightmap_cache
                    .insert(&request.content_key, Arc::clone(&filled));
                build.lightmap_millis = filled.stats.bake_millis;
                build.lightmaps = Some(filled);
                build.lightmap_failure = None;
            }
            Err(failure) => {
                build.lightmap_failure = Some(failure);
                build.lightmaps = None;
                let mesh_started = std::time::Instant::now();
                let mesh = rebuild_vertex_lit_level(
                    &loaded.level,
                    &self.prop_catalog,
                    &mut self.prop_assets,
                    &loaded.materials,
                    &build.lighting,
                );
                build.timings.surfaces_millis = mesh_started
                    .elapsed()
                    .as_secs_f64()
                    .mul_add(1000.0, build.timings.surfaces_millis);
                build.mesh = mesh;
            }
        }
        self.install_level(loaded, build, lightmaps, true, started)
    }

    /// Cancels and joins any live fill and drops the staged build.
    ///
    /// Dropping the worker sets its cancel flag and joins the thread, which
    /// returns after the chart it was filling (the fill checks between charts),
    /// so a supersede blocks for at most one chart. The generation is bumped so
    /// a result that raced the cancellation can never activate.
    fn cancel_staged_lightmap(&mut self) {
        self.lightmap_worker = None;
        self.staged_lightmap = None;
        self.lightmap_generation = self.lightmap_generation.saturating_add(1);
    }

    /// Re-uploads the retained CPU build at the current quality budget.
    ///
    /// No lighting bake, no lightmap fill and no mesh rebuild: the same mesh is
    /// re-resolved through the texture/material caches, which the install
    /// releases first because the quality level is part of a texture's fit.
    fn install_retained(&mut self, loaded: &LoadedLevel) {
        let Some(build) = self.retained_build.take() else {
            return;
        };
        let lightmaps = self.retained_lightmaps;
        let build = self.install_level(loaded, build, lightmaps, false, std::time::Instant::now());
        self.retain_build(loaded, build, lightmaps);
    }

    /// Applies a Reflections setting to the GPU targets and the frame gates.
    ///
    /// Probe cubemaps are created at the setting's face size (or retired for
    /// Off), the planar target is dropped when the setting stops allowing it,
    /// and the frame gates follow. Creating a cubemap does not bake it; the
    /// caller's install or [`Self::refresh_reflection_resources`] bakes.
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
    #[allow(clippy::float_cmp)] // identical inputs re-derive identical bits
    fn reflection_targets_match(&self, quality: ReflectionQuality) -> bool {
        let face_size = probe_face_size(quality);
        let wanted = if face_size.is_some() {
            self.reflections
                .routing
                .probe_points
                .len()
                .min(crate::render::common::view::MAX_REFLECTION_PROBES)
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

    /// Rebuilds the environment bindings and rebakes the probes after a
    /// reflection-only change.
    ///
    /// The world, textures and materials are already resident and unchanged, so
    /// only the bind group that references the (new) probe views and the probe
    /// captures themselves run. `bake_reflection_probes` is a no-op when the
    /// setting is Off, which is what stops all capture work immediately.
    fn refresh_reflection_resources(&mut self) {
        if self.world.is_none() {
            return;
        }
        self.environment = Some(self.create_environment());
        self.bake_reflection_probes();
        self.upload_characters_gpu();
        self.upload_dynamic();
    }

    /// Retains the build that just became resident, keyed by its level content
    /// (id and definition fingerprint) and lightmap configuration.
    fn retain_build(
        &mut self,
        loaded: &LoadedLevel,
        build: LevelBuild,
        lightmaps: LightmapQuality,
    ) {
        self.retained_build = Some(build);
        self.retained_lightmaps = lightmaps;
        self.retained_level_id = Some(loaded.level.id.clone());
        self.retained_level_fingerprint = Some(level_content_fingerprint(&loaded.level));
    }

    /// Uploads (or re-uploads) one prepared CPU build as the resident world.
    ///
    /// This is the single GPU-side install step: the lightmap atlas, the
    /// fixture sheets, the static geometry, the world textures and materials,
    /// the props, the decals, the environment bindings and the probe bake all
    /// come from `build`. The texture caches are released only when the new
    /// configuration really needs it: a quality change re-fits every texture,
    /// a new level drops its pack textures, and a lightmap-only rebuild at the
    /// same quality keeps both. The atlas upload happens here, on the main
    /// thread, exactly once per activation — including for a background fill,
    /// whose completion poll calls this. `build` is returned so the caller can
    /// retain the exact build that became resident (the defensive upload
    /// fallback may replace it).
    #[allow(clippy::too_many_lines)] // one cohesive level upload: upload and install
    fn install_level(
        &mut self,
        loaded: &LoadedLevel,
        mut build: LevelBuild,
        lightmaps_quality: LightmapQuality,
        upload_atlas: bool,
        started: std::time::Instant,
    ) -> LevelBuild {
        let level_changed = self.level_id.as_deref() != Some(loaded.level.id.as_str());
        let quality_changed = self.quality != self.installed_quality;
        if quality_changed {
            self.textures.release_profile_textures();
        } else if level_changed {
            self.textures.begin_level();
        }
        let options = LightmapBuildOptions::for_lightmaps(lightmaps_quality);
        if upload_atlas {
            let mut atlas =
                LightmapAtlas::upload(&self.device, &self.queue, build.lightmaps.as_deref());
            // A plan or fill failure never reaches this branch: the neutral build
            // has already returned the historical vertex-lit mesh with the *same*
            // lighting (`build.lightmaps.is_none()`), exactly like the reference,
            // which does not call `upload_level_lightmaps` for a missing atlas.
            // Rebuilding again with `LightmapMode::Off` here would re-bake with
            // `BakeConfig::HARD` and change every vertex colour. Only an upload
            // failure rebuilds, and the wgpu upload cannot fail; the check stays
            // as the reference's defensive fallback so a future failure cannot
            // draw an atlas-less lightmapped mesh.
            if needs_upload_fallback(options.mode, build.lightmaps.is_some(), atlas.is_resident()) {
                build = build_level_geometry_timed_with_lightmaps(
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
                build.lightmap_failure = Some(LightmapFailure::Upload);
                atlas = LightmapAtlas::upload(&self.device, &self.queue, None);
            }
            self.lightmaps = atlas;
            self.lightmaps_resident = self.lightmaps.is_resident();
        }
        let lightmap_failure = build.lightmap_failure;
        let lightmap_stats = build.lightmaps.as_deref().map(|lightmaps| lightmaps.stats);
        let materials = MaterialRenderState::from_table(&loaded.materials);
        let fixture_sheets = self.upload_fixture_sheets(loaded);
        let world = WgpuWorldGeometry::upload(&self.device, &self.queue, &build.mesh, &materials);
        let world_textures = WorldTextures::resolve(
            &mut self.textures,
            &self.device,
            &self.queue,
            world.draws(),
            &materials,
            &loaded.materials,
            &fixture_sheets,
            self.quality,
        );
        let animations = Self::resolve_animations(&loaded.level, &loaded.materials);
        // The level's reflection routing comes from the emitted geometry and
        // the material table, exactly like the reference's `resolve_scene_extras`.
        let reflections_vec: Vec<crate::materials::MaterialReflection> = loaded
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
        self.apply_reflection_targets(self.graphics_requested.reflections);
        self.active_plane = None;
        self.reflection_passes = 0;
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
                animations: &animations,
                routing: &self.reflections.routing,
            },
        );
        self.log_level_resolution(&world, &world_textures, &world_materials);
        // Claim every placed skinned prop for the character path before the
        // static prop upload: the GPU prop batches of a claimed model are
        // suppressed so the bind pose and the animated pose never draw on top
        // of each other. The neutral batches stay in `build`, so the lightmap
        // bake and its occlusion are untouched.
        self.characters = CharacterScene::spawn_characters(
            &loaded.level,
            &self.prop_catalog,
            &mut self.prop_assets,
            &build.lighting,
        );
        let claimed_characters = self.characters.claimed_models().to_vec();
        let world_props = WgpuProps::upload(
            &self.device,
            &self.queue,
            &mut self.textures,
            &self.material_layout,
            &build.batches,
            &claimed_characters,
            self.quality,
        );
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
        self.animation_seconds = 0.0;
        self.dynamic_lighting = Some(build.lighting.clone());
        // The GPU side of the previous level's dynamic scene dies with it. The
        // neutral scene is level content too: a quality rebuild of the same
        // level keeps its objects (the reference keeps them across one), but a
        // different level must not inherit them, so a level change clears the
        // neutral scene and it is re-uploaded against the new resources.
        if self.level_id.as_deref() != Some(loaded.level.id.as_str()) {
            self.dynamic.clear_all();
            self.level_id = Some(loaded.level.id.clone());
        }
        self.world_dynamic = None;
        // The previous level's (or previous quality's) character GPU state is
        // dropped before the probe bake so the bake cannot draw stale
        // characters; `upload_characters_gpu` rebuilds it after the probes.
        self.world_characters = None;
        self.environment = Some(self.create_environment());
        self.world = Some(world);
        self.world_props = Some(world_props);
        self.world_textures = Some(world_textures);
        self.world_materials = Some(world_materials);
        // The decal pass runs inside the scene body, so its pipeline (and the
        // reflection-format one) must exist before any probe bake.
        self.ensure_world_pipeline();
        self.upload_decals(&build.mesh, &loaded.level);
        // The probes bake last, when the world, its textures, its materials and
        // its decals are all resident: six full scene submissions per probe,
        // exactly like the reference's `bake_reflection_probes`.
        self.bake_reflection_probes();
        // Characters and dynamic objects upload after the probe bake for the
        // same reason: their group-3 environments sample the probe cubemaps,
        // and binding those while they are being captured is a validation
        // error. A quality rebuild keeps the neutral character scene; the GPU
        // side is re-uploaded against the new level resources.
        self.upload_characters_gpu();
        // A quality rebuild keeps the neutral dynamic scene; re-upload it
        // against the new level resources so no draw references a stale
        // resource (the reference's own rebuild leaves stale handles and is
        // documented as a bug this port does not reproduce).
        self.upload_dynamic();
        self.installed_quality = self.quality;
        build
    }

    /// Uploads the level's decal batches against the current pipelines.
    fn upload_decals(
        &mut self,
        mesh: &crate::render::common::mesh::LevelMesh,
        level: &crate::level::LevelDef,
    ) {
        let Some(pipeline) = self.decal_pipeline.as_ref() else {
            return;
        };
        let asset_root = crate::assets::resolve_asset_root();
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
            static_environment(self.lightmaps_resident, self.fog),
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
    /// The reference's `upload_fixture_sheets`, with the same lifetimes: a
    /// catalog sheet persists across levels through the texture cache, a pack
    /// sheet dies with the level. A family the level does not use keeps the
    /// shared white fallback.
    fn upload_fixture_sheets(
        &mut self,
        loaded: &LoadedLevel,
    ) -> Vec<std::sync::Arc<super::texture::GpuTexture>> {
        let families = crate::lighting::FixtureKind::ALL.len();
        let mut slots: Vec<std::sync::Arc<super::texture::GpuTexture>> =
            Vec::with_capacity(families);
        for _ in 0..families {
            slots.push(self.textures.fallback());
        }
        for sheet in &loaded.light_sheets {
            let Some(slot) = slots.get_mut(sheet.kind.index()) else {
                continue;
            };
            let (_, texture) = self.textures.get_or_upload_fitted(
                &self.device,
                &self.queue,
                &sheet.key,
                sheet.image.as_ref(),
                TextureClass::FixtureFace,
                sheet.origin,
                self.quality,
            );
            *slot = texture;
        }
        slots
    }

    /// Resolves the level's emission animations, indexed by material index.
    ///
    /// Exactly the reference's `resolve_scene_extras` loop: a level that
    /// declares none leaves every entry `None` (the identity multiplier).
    fn resolve_animations(
        level: &crate::level::LevelDef,
        materials: &crate::materials::MaterialTable,
    ) -> Vec<Option<EmissionAnimation>> {
        let mut animations = vec![None; materials.entries().len()];
        for animation in &level.animated_emissions {
            let Some(index) = materials.index_of(animation.material.trim()) else {
                continue;
            };
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
            if let Some(slot) = animations.get_mut(usize::from(index)) {
                *slot = resolved.is_active().then_some(resolved);
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
            if self.quality.draws_surface_response() {
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

    /// Records the requested quality level.
    ///
    /// Recording changes no GPU state; the work (re-fitting the retained
    /// build's textures, or rebuilding it when the lightmaps changed too) runs
    /// in [`Self::apply_graphics`]. The level is also recorded in the live
    /// `quality` field because the per-frame scene target and surface-response
    /// gates read it directly, exactly as they always have.
    pub const fn set_quality(&mut self, quality: QualityLevel) {
        self.quality = quality;
        self.graphics_requested.quality = quality;
    }

    /// Records the player's texture filtering setting.
    ///
    /// The three presets are shared samplers, so switching swaps which sampler
    /// a draw's bind group holds; no pixel data is re-uploaded, no bind group
    /// is rebuilt, and the environment binding does not follow it. The legacy
    /// `"nearest"` parses as Low and the legacy `"linear"` as High; an empty or
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
    /// The gate applies at the next frame's post settings; `apply_graphics`
    /// only records it, and the bloom targets stay allocated. No rebuild.
    pub const fn set_bloom_enabled(&mut self, enabled: bool) {
        self.bloom_requested = enabled;
        self.graphics_requested.bloom = enabled;
    }

    /// Records the requested Lightmaps quality.
    ///
    /// The atlas configuration follows this value, never the overall quality
    /// level. A change is applied by [`Self::apply_graphics`], which rebuilds
    /// the CPU level once and fills an uncached atlas on a worker.
    pub const fn set_lightmap_quality(&mut self, quality: LightmapQuality) {
        self.graphics_requested.lightmaps = quality;
    }

    /// Records the requested Reflections quality.
    ///
    /// A change is applied by [`Self::apply_graphics`]: probe cubemaps and the
    /// planar target are retired or created immediately, and `Off` stops all
    /// capture work rather than leaving a stale capture sampled.
    pub const fn set_reflection_quality(&mut self, quality: ReflectionQuality) {
        self.graphics_requested.reflections = quality;
    }

    /// Uploads a loaded level: the one graphics transaction entry point.
    ///
    /// A level load routes through [`Self::apply_graphics`], which forces the
    /// synchronous path for a level that is not resident (a fresh level has no
    /// previous world to keep on screen, so its atlas fill runs inline exactly
    /// like the historical load). For the same level it becomes the runtime
    /// transition: only the settings that changed owe work, and an uncached
    /// lightmap fill happens on a worker.
    pub fn set_level(&mut self, loaded: &LoadedLevel) {
        self.apply_graphics(loaded);
    }

    /// Releases the renderer's quality-fitted texture cache.
    ///
    /// The correct caller contract is the reference's: release, then upload the
    /// current level in the same frame. The loaded level's textures stay alive
    /// through their `Arc`s until `set_level` replaces them, so the frame
    /// between the two calls still draws with valid GPU resources.
    pub fn release_profile_textures(&mut self) {
        self.textures.release_profile_textures();
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
    pub fn prop_asset_stats(&self) -> crate::props::PropAssetStats {
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
    #[allow(clippy::too_many_lines)] // one cohesive frame submission: prepare, encode and present
    pub fn render_scene(&mut self, camera: RenderCamera) {
        if self.check_device_lost() {
            return;
        }
        // The background lightmap fill, if any, is polled here: a finished fill
        // uploads its atlas and swaps the world in on this thread, while the
        // previous world keeps rendering until then. Two `Option` checks when
        // idle.
        self.advance_graphics_transition();
        self.ensure_ready();
        if self.fatal.is_some() || self.needs_configure || self.drawable_size.is_empty() {
            return;
        }
        self.ensure_world_pipeline();
        if self.fatal.is_some() {
            return;
        }
        // Animated emissions are level data; advancing them is a handful of
        // uniform writes, and a still material writes nothing.
        if let Some(materials) = self.world_materials.as_mut() {
            materials.update_animations(&self.queue, self.animation_seconds);
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
        // geometry survived the frustum test, exactly like the reference.
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
        // The reference reads the probe nearest the camera every frame.
        self.active_probe = if self.reflections_enabled {
            crate::render::common::reflections::nearest_probe(
                &self.reflections.routing.probe_points,
                frame.eye.into(),
            )
            .unwrap_or(0)
        } else {
            0
        };
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
            let uniform = match uniform {
                Some((matrix, plane)) => {
                    static_environment(self.lightmaps_resident, self.fog).with_planar(matrix, plane)
                }
                None => static_environment(self.lightmaps_resident, self.fog),
            };
            environment.update(&self.queue, uniform);
        }
        if let Some(pipeline) = self.world_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.decal_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.scene_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.emissive_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        if let Some(pipeline) = self.decal_scene_pipeline.as_mut() {
            pipeline.upload_camera(&self.queue, frame.view_projection, frame.eye);
        }
        self.ensure_post_targets();
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
                let totals = self.encode_frame_into(&mut encoder, &view, &frame);
                self.queue.submit([encoder.finish()]);
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
        let Some(post) = self.post.as_mut() else {
            return false;
        };
        let stats = post.ensure(&self.device, self.quality, self.drawable_size);
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
        }
        post.is_ready()
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
    ) -> WorldDrawTotals {
        let Some(post) = self.post.as_ref().filter(|post| post.is_ready()) else {
            return self.encode_direct(encoder, target, frame);
        };
        let settings = PostSettings::for_level(self.quality).with_bloom(self.bloom_requested);
        let mut totals = WorldDrawTotals::default();
        {
            let Some(scene_view) = post.scene_view() else {
                return totals;
            };
            let Some(depth_view) = post.scene_depth_view() else {
                return totals;
            };
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            totals = self.encode_scene_body(&mut pass, frame);
        }
        let blooms = self.bloom_requested && settings.blooms() && totals.emissive_visible;
        if blooms && self.emissive_pipeline.is_some() {
            if let Some(mut pass) = post.emissive_begin(encoder) {
                let _ = self.encode_emissive_body(&mut pass, frame);
            }
            post.encode_blur(encoder, settings.bloom_strength);
        }
        // The resolve (or present copy) lands on the presented target, the
        // backend's default-framebuffer equivalent; the UI blends into it in
        // display space and it is encoded to the sRGB surface afterwards.
        if let Some(presented) = post.presented_view() {
            post.encode_resolve(encoder, presented, settings, blooms);
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
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let (Some(pipeline), Some(world), Some(textures), Some(materials), Some(environment)) = (
            self.scene_pipeline.as_ref(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.world_materials.as_ref(),
            self.environment.as_ref(),
        ) else {
            return totals;
        };
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
        if let (Some(decals), Some(decal_pipeline)) =
            (self.decals.as_ref(), self.decal_scene_pipeline.as_ref())
        {
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
            totals.texture_binds = totals
                .texture_binds
                .saturating_add(decal_totals.texture_binds);
        }
        totals
    }

    /// Encodes the world body directly into `target`, without tone, grade or
    /// bloom: the fallback when no post targets could be created.
    fn encode_direct(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &super::world::WorldFrame,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
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
                    load: wgpu::LoadOp::Clear(CLEAR_COLOR_SRGB),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_attachment,
            timestamp_writes: None,
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
            if let (Some(decals), Some(decal_pipeline)) =
                (self.decals.as_ref(), self.decal_pipeline.as_ref())
            {
                let decal_totals = decals.encode(
                    &mut pass,
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
                totals.texture_binds = totals
                    .texture_binds
                    .saturating_add(decal_totals.texture_binds);
            }
        }
        totals
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
        // culls nothing, like every reference capture pass. The reversed front
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
        // The offscreen post chain: the scene target's format equals the
        // reflection format, the emissive target is raw, and the resolve and
        // present pipelines write the surface format.
        self.post = Some(match self.post.take() {
            Some(mut post) => {
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
    /// recreated surface). The font atlas is re-uploaded with it; it is 128x64
    /// and created once per format.
    fn ensure_ui_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.ui.as_ref().map(UiRenderer::format) == Some(format) {
            return;
        }
        self.ui = Some(UiRenderer::new(&self.device, &self.queue, format));
    }

    /// Draws the 2D UI vertex list into the frame acquired by `render_scene`.
    ///
    /// The reference's `render_ui`: the same 480x272 reference HUD, drawn after
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
    /// The UI's viewport is computed against the presented target itself — the
    /// reference's default framebuffer — so the HUD renders at drawable
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
            self.ensure_ui_pipeline(SCENE_FORMAT);
            if self.fatal.is_some() {
                return;
            }
            let presented_drawable = self.presented_drawable();
            if let Some(ui) = self.ui.as_mut() {
                ui.render(
                    &self.device,
                    &self.queue,
                    &presented,
                    ui_vertices,
                    presented_drawable,
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
        post.encode_present_to(&mut encoder, &surface);
        self.queue.submit([encoder.finish()]);
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
        ui.render(
            &self.device,
            &self.queue,
            &view,
            ui_vertices,
            self.drawable_size,
        );
    }

    /// Bakes every wanted probe cubemap: six full scene submissions per probe.
    ///
    /// Runs once per level load, after the world, its textures and its materials
    /// are resident, exactly like the reference's `bake_reflection_probes`. All
    /// reflection sampling is suppressed while it runs (every material mode is
    /// zero), so a probe never samples an incomplete cube.
    #[allow(clippy::too_many_lines)] // one cohesive bake: six face submissions per probe in one loop
    fn bake_reflection_probes(&mut self) {
        if self.reflection_targets.probes().is_empty() {
            return;
        }
        self.ensure_world_pipeline();
        if self.fatal.is_some() {
            return;
        }
        let Some(pipeline) = self.capture_pipeline.as_mut() else {
            return;
        };
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
                    pipeline.encode(
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
                    // The reference probe bake is a full body draw, decals
                    // included.
                    if let (Some(decals), Some(decal_pipeline)) = (
                        self.decals.as_ref(),
                        self.decal_reflection_pipeline.as_ref(),
                    ) {
                        let _ = decals.encode(
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
                self.queue.submit([encoder.finish()]);
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
    /// The mirrored view-projection and mirrored eye are the reference's, and
    /// all reflection sampling is suppressed for the duration. Returns true when
    /// a capture was submitted.
    fn encode_planar_capture(
        &mut self,
        frame: &super::world::WorldFrame,
        plane_index: usize,
    ) -> bool {
        let Some(pipeline) = self.capture_pipeline.as_mut() else {
            return false;
        };
        let Some(decal_pipeline) = self.decal_reflection_pipeline.as_mut() else {
            return false;
        };
        let Some(planar) = self.reflection_targets.planar() else {
            return false;
        };
        let Some(environment) = self.environment.as_ref() else {
            return false;
        };
        let Some(materials) = self.world_materials.as_mut() else {
            return false;
        };
        let Some(world) = self.world.as_ref() else {
            return false;
        };
        let Some(textures) = self.world_textures.as_ref() else {
            return false;
        };
        let Some(plane) = self.reflections.routing.planes.get(plane_index) else {
            return false;
        };
        let mirrored_matrix = planar_view_projection(frame.view_projection, plane);
        let mirrored_eye =
            glam::Vec3::from_array(crate::render::common::reflections::mirror_point(
                plane.normal,
                plane.offset,
                [frame.eye.x, frame.eye.y, frame.eye.z],
            ));
        let capture = CaptureFrame::new(mirrored_matrix, mirrored_eye);
        // The mirror must not sample the image it is writing.
        materials.update_reflection_modes(&self.queue, None, false, false, true);
        pipeline.upload_camera(&self.queue, capture.view_projection, capture.eye);
        decal_pipeline.upload_camera(&self.queue, capture.view_projection, capture.eye);
        let capture_frame = capture.world_frame();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-wgpu-planar-capture"),
            });
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pipeline.encode(
                &mut pass,
                super::world::WorldEncodeInputs {
                    geometry: world,
                    textures,
                    materials,
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
            // A mirror sees the decals the reference's full body draws.
            if let Some(decals) = self.decals.as_ref() {
                let _ = decals.encode(
                    &mut pass,
                    decal_pipeline,
                    super::decals::DecalEncodeInputs {
                        filtering: self.filtering,
                        frame: &capture_frame,
                        cull: self.culling,
                    },
                );
            }
        }
        self.queue.submit([encoder.finish()]);
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
    pub fn present(&mut self, window: &Window) {
        if self.check_device_lost() {
            return;
        }
        if self.surface_lost {
            self.recreate_surface(window);
        }
        if let Some(texture) = self.pending_frame.take() {
            self.queue.present(texture);
            self.poll_device();
            return;
        }
        self.ensure_ready();
        if self.fatal.is_some() || self.needs_configure || self.drawable_size.is_empty() {
            return;
        }
        match self.acquire_frame() {
            Acquired::Frame(texture) => self.queue.present(texture),
            Acquired::Skip => {}
        }
        self.poll_device();
    }

    /// Waits for submitted GPU work to finish (`PLACES_BENCH_FINISH`).
    pub fn finish(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Reads back the last rendered frame as a top-down RGBA8 image.
    ///
    /// The post path's presented image already carries the resolved scene and
    /// the HUD in raw display space (the reference's default-framebuffer
    /// values), so the capture re-encodes that chain — the same pipelines, bind
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
    pub fn capture_default_framebuffer(&mut self) -> Result<crate::loader::RawImage, String> {
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
            SCENE_FORMAT
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
        // The presented target already carries the frame *and* the HUD (the
        // post path blends the UI into it before encoding it to the surface),
        // so the post-path capture is its raw copy — the reference's
        // framebuffer read, display-space bytes with no sRGB round trip. The
        // direct fallback re-renders the body and replays the last UI list.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-wgpu-capture"),
            });
        let _ = self.encode_frame_into(&mut encoder, &view, &frame);
        self.queue.submit([encoder.finish()]);
        if post_presented {
            // The capture re-renders the resolved scene into the presented
            // target, replays the HUD there (the reference reads the default
            // framebuffer after the UI), then copies the presented image into
            // the capture target. Re-rendering rather than copying the live
            // presented image keeps `PLACES_BENCH_NOSWAP` captures working even
            // when no surface texture could be acquired.
            let presented = self
                .post
                .as_ref()
                .and_then(PostProcess::presented_view)
                .cloned();
            if let Some(presented) = presented {
                if !self.last_ui.is_empty() {
                    self.ensure_ui_pipeline(SCENE_FORMAT);
                    let presented_drawable = self.presented_drawable();
                    if let Some(ui) = self.ui.as_mut() {
                        let ui_vertices = std::mem::take(&mut self.last_ui);
                        ui.render(
                            &self.device,
                            &self.queue,
                            &presented,
                            &ui_vertices,
                            presented_drawable,
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
                self.queue.submit([copy.finish()]);
            }
        } else if !self.last_ui.is_empty() {
            self.ensure_ui_pipeline(self.config.format);
            if let Some(ui) = self.ui.as_mut() {
                let ui_vertices = std::mem::take(&mut self.last_ui);
                ui.render(
                    &self.device,
                    &self.queue,
                    &view,
                    &ui_vertices,
                    self.drawable_size,
                );
                self.last_ui = ui_vertices;
            }
        }
        read_back_rgba(&self.device, &self.queue, &texture, width, height, format)
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
            self.surface.configure(&self.device, &self.config);
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
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Acquired::Frame(texture),
            wgpu::CurrentSurfaceTexture::Timeout => self.recover(SurfaceStatus::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => self.recover(SurfaceStatus::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => self.recover(SurfaceStatus::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => self.recover(SurfaceStatus::Lost),
            wgpu::CurrentSurfaceTexture::Validation => self.recover(SurfaceStatus::Validation),
        }
    }

    /// Applies the surface recovery policy for one non-success status.
    fn recover(&mut self, status: SurfaceStatus) -> Acquired {
        match status.recovery() {
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
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Acquired::Frame(texture),
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                logging::warn_once(
                    "wgpu-surface-outdated-retry",
                    "[wgpu] the surface did not recover after reconfiguring; skipping the frame",
                );
                Acquired::Skip
            }
        }
    }

    /// Recreates the surface from the SDL window and reconfigures it.
    fn recreate_surface(&mut self, window: &Window) {
        self.surface_lost = false;
        // The old surface must not be replaced while one of its textures is
        // alive; the frame is dropped first.
        self.pending_frame = None;
        // SAFETY: the window outlives the renderer (see `surface::create`).
        match unsafe { surface::create(&self.instance, window) } {
            Ok(recreated) => {
                self.surface = recreated;
                self.capabilities = self.surface.get_capabilities(&self.adapter);
                if !self.capabilities.formats.contains(&self.config.format)
                    && let Some(format) = surface::select_surface_format(&self.capabilities)
                {
                    self.config.format = format;
                }
                self.config.present_mode =
                    surface::select_present_mode(&self.capabilities, self.vsync);
                self.needs_configure = true;
                self.ensure_ready();
                logging::info(format!(
                    "[wgpu] surface recreated for the SDL window ({}x{})",
                    self.drawable_size.width, self.drawable_size.height
                ));
            }
            Err(error) => {
                self.mark_fatal(format!("wgpu could not recreate the lost surface: {error}"));
            }
        }
    }

    /// Drains callback work so device-loss and validation reports are seen.
    fn poll_device(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
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
        if let Some(reported) = reported {
            self.mark_fatal(format!("wgpu device lost ({reported})"));
            return true;
        }
        false
    }

    /// Records the first fatal error and reports it exactly once.
    fn mark_fatal(&mut self, message: String) {
        if self.fatal.is_none() {
            logging::warn(format!("[wgpu] fatal device error: {message}"));
            self.fatal = Some(message);
        }
    }
}

/// Copies one world-format texture into a staging buffer and returns its
/// compacted top-down RGBA rows.
///
/// `copy_texture_to_buffer` requires every row to start on a 256-byte boundary,
/// so the staging buffer is padded and each row is copied into the compact
/// image after mapping. A `Bgra8*` source has its red and blue channels swapped
/// back to RGBA order. The texture must have been rendered and submitted
/// already; this function submits only the copy and the buffer read-back.
fn read_back_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> Result<crate::loader::RawImage, String> {
    let unpadded_bytes_per_row = width.saturating_mul(4);
    let padded_bytes_per_row = unpadded_bytes_per_row
        .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .saturating_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-capture-readback"),
        size: u64::from(padded_bytes_per_row).saturating_mul(u64::from(height)),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("places-wgpu-capture-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        // A closed receiver means the caller gave up; there is nothing to
        // report it to.
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("wgpu could not wait for the capture read-back: {error}"))?;
    receiver
        .recv()
        .map_err(|_| "the wgpu capture read-back callback was lost".to_string())?
        .map_err(|error| format!("wgpu could not map the capture buffer: {error}"))?;

    let height_usize = usize::try_from(height).unwrap_or(usize::MAX);
    let row_usize = usize::try_from(unpadded_bytes_per_row).unwrap_or(usize::MAX);
    let padded_usize = usize::try_from(padded_bytes_per_row).unwrap_or(usize::MAX);
    let mut rgba = vec![0u8; row_usize.saturating_mul(height_usize)];
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("wgpu could not read the mapped capture buffer: {error}"))?;
        for row in 0..height_usize {
            let source = row.saturating_mul(padded_usize);
            let target = row.saturating_mul(row_usize);
            let (Some(from), Some(to)) = (
                mapped.get(source..source.saturating_add(row_usize)),
                rgba.get_mut(target..target.saturating_add(row_usize)),
            ) else {
                break;
            };
            to.copy_from_slice(from);
            if capture_needs_bgra_swizzle(format) {
                // `Bgra8*` stores blue at byte 0 and red at byte 2. A full row
                // is always a whole number of RGBA texels, so the chunked view
                // covers every byte that was copied.
                for texel in to.as_chunks_mut::<4>().0 {
                    texel.swap(0, 2);
                }
            }
        }
    }
    buffer.unmap();
    Ok(crate::loader::RawImage::new(width, height, rgba))
}

#[cfg(test)]
mod tests {
    // Test code: the delta fixtures are hand-built values, so unwraps and
    // infallible indexing are idiomatic here.
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    fn graphics_config(
        quality: QualityLevel,
        lightmaps: LightmapQuality,
        reflections: ReflectionQuality,
    ) -> GraphicsConfig {
        GraphicsConfig {
            quality,
            filtering: TextureFiltering::High,
            bloom: true,
            lightmaps,
            reflections,
        }
    }

    /// A filtering change is recorded, owes no GPU work, and therefore cannot
    /// rebuild anything: the bind groups swap the sampler handle at bind time.
    #[test]
    fn a_filtering_only_change_is_zero_work() {
        let applied = graphics_config(
            QualityLevel::High,
            LightmapQuality::Full,
            ReflectionQuality::Full,
        );
        let requested = GraphicsConfig {
            filtering: TextureFiltering::Low,
            ..applied
        };
        let delta = applied.delta_from(requested);
        assert!(delta.filtering);
        assert!(!delta.needs_gpu_work());
        assert!(!delta.needs_build());
        assert!(!delta.needs_texture_refit());
        assert!(
            !delta.is_empty(),
            "the change is still real; it is recorded"
        );
    }

    /// A bloom change is recorded and gated per frame; no resource work.
    #[test]
    fn a_bloom_only_change_is_zero_work() {
        let applied = graphics_config(
            QualityLevel::High,
            LightmapQuality::Full,
            ReflectionQuality::Full,
        );
        let requested = GraphicsConfig {
            bloom: false,
            ..applied
        };
        let delta = applied.delta_from(requested);
        assert!(delta.bloom);
        assert!(!delta.needs_gpu_work());
        assert!(!delta.needs_build());
        assert!(!delta.needs_texture_refit());
    }

    /// Only a lightmap change rebuilds the CPU level; it does not re-fit
    /// textures (the quality did not change).
    #[test]
    fn a_lightmap_only_change_needs_one_build_and_no_texture_refit() {
        let applied = graphics_config(
            QualityLevel::High,
            LightmapQuality::Off,
            ReflectionQuality::Full,
        );
        let requested = GraphicsConfig {
            lightmaps: LightmapQuality::Full,
            ..applied
        };
        let delta = applied.delta_from(requested);
        assert!(delta.lightmaps);
        assert!(delta.needs_build(), "the lightmap mesh must be re-emitted");
        assert!(delta.needs_gpu_work());
        assert!(!delta.needs_texture_refit());
        assert!(!delta.quality && !delta.filtering && !delta.bloom);
    }

    /// A reflection change touches the probe/planar targets only: the retained
    /// CPU build is not rebuilt and no texture is re-fitted.
    #[test]
    fn a_reflection_only_change_needs_no_build_and_no_texture_refit() {
        let applied = graphics_config(
            QualityLevel::High,
            LightmapQuality::Full,
            ReflectionQuality::Off,
        );
        let requested = GraphicsConfig {
            reflections: ReflectionQuality::Medium,
            ..applied
        };
        let delta = applied.delta_from(requested);
        assert!(delta.reflections);
        assert!(delta.needs_gpu_work(), "the targets are GPU resources");
        assert!(!delta.needs_build());
        assert!(!delta.needs_texture_refit());
    }

    /// A quality-only change with an unchanged lightmap configuration reuses
    /// the retained build: re-fit textures, no lighting bake.
    #[test]
    fn a_quality_only_change_reuses_the_retained_build() {
        let applied = graphics_config(
            QualityLevel::Low,
            LightmapQuality::Full,
            ReflectionQuality::Full,
        );
        let requested = GraphicsConfig {
            quality: QualityLevel::High,
            ..applied
        };
        let delta = applied.delta_from(requested);
        assert!(delta.quality);
        assert!(!delta.needs_build(), "must not re-bake the lightmap");
        assert!(delta.needs_texture_refit());
        assert!(delta.needs_gpu_work());
    }

    /// A combined settings action has one delta and one build decision, never
    /// one per setting.
    #[test]
    fn a_combined_change_is_classified_in_one_diff() {
        let applied = graphics_config(
            QualityLevel::Low,
            LightmapQuality::Off,
            ReflectionQuality::Off,
        );
        let requested = graphics_config(
            QualityLevel::High,
            LightmapQuality::Full,
            ReflectionQuality::Full,
        );
        let delta = applied.delta_from(requested);
        assert_eq!(
            delta,
            GraphicsDelta {
                quality: true,
                filtering: false,
                bloom: false,
                lightmaps: true,
                reflections: true,
            }
        );
        // Exactly one build result: `apply_graphics` has one
        // `delta.needs_build()` branch, so the combined action runs one
        // `prepare_level_geometry_with_lightmaps` call.
        assert!(delta.needs_build());
        assert!(delta.needs_texture_refit());
    }

    /// A re-applied identical configuration is a no-op.
    #[test]
    fn an_unchanged_configuration_has_an_empty_delta() {
        let config = graphics_config(
            QualityLevel::Medium,
            LightmapQuality::Medium,
            ReflectionQuality::Medium,
        );
        assert!(config.delta_from(config).is_empty());
        assert!(!config.delta_from(config).needs_gpu_work());
    }

    /// A first upload or new level forces every branch.
    #[test]
    fn a_forced_first_upload_asks_for_everything() {
        let delta = GraphicsDelta::everything();
        assert!(delta.needs_build());
        assert!(delta.needs_texture_refit());
        assert!(delta.needs_gpu_work());
        assert!(!delta.is_empty());
    }

    /// The next value in each selector's order, for the matrix test.
    fn other_quality(current: QualityLevel) -> QualityLevel {
        match current {
            QualityLevel::Low => QualityLevel::Medium,
            QualityLevel::Medium => QualityLevel::High,
            QualityLevel::High => QualityLevel::Low,
        }
    }

    fn other_lightmaps(current: LightmapQuality) -> LightmapQuality {
        match current {
            LightmapQuality::Off => LightmapQuality::Medium,
            LightmapQuality::Medium => LightmapQuality::Full,
            LightmapQuality::Full => LightmapQuality::Off,
        }
    }

    fn other_reflections(current: ReflectionQuality) -> ReflectionQuality {
        match current {
            ReflectionQuality::Off => ReflectionQuality::Medium,
            ReflectionQuality::Medium => ReflectionQuality::Full,
            ReflectionQuality::Full => ReflectionQuality::Off,
        }
    }

    /// The complete override matrix: every overall level combines with every
    /// Lightmaps, Reflections, Filtering and Bloom value, and each single-axis
    /// change is classified exactly once. The overall level alone never asks
    /// for a lighting rebuild; only the Lightmaps setting does.
    #[test]
    fn every_override_combination_is_classified_once() {
        for quality in QualityLevel::ALL {
            for lightmaps in LightmapQuality::ALL {
                for reflections in ReflectionQuality::ALL {
                    for filtering in [
                        TextureFiltering::Low,
                        TextureFiltering::Medium,
                        TextureFiltering::High,
                    ] {
                        for bloom in [false, true] {
                            let base = GraphicsConfig {
                                quality,
                                filtering,
                                bloom,
                                lightmaps,
                                reflections,
                            };
                            assert!(base.delta_from(base).is_empty());

                            // The overall level changes the texture budgets and
                            // the scene target: a refit, never a bake.
                            let next_quality = base.delta_from(GraphicsConfig {
                                quality: other_quality(quality),
                                ..base
                            });
                            assert!(next_quality.quality && next_quality.needs_gpu_work());
                            assert!(!next_quality.lightmaps && !next_quality.needs_build());
                            assert!(next_quality.needs_texture_refit());

                            // Lightmaps is the only setting that re-bakes the
                            // level and re-emits its mesh.
                            let next_lightmaps = base.delta_from(GraphicsConfig {
                                lightmaps: other_lightmaps(lightmaps),
                                ..base
                            });
                            assert!(next_lightmaps.lightmaps && next_lightmaps.needs_build());
                            assert!(
                                !next_lightmaps.quality && !next_lightmaps.needs_texture_refit()
                            );

                            // Reflections and the two frame gates never rebuild
                            // the CPU level or re-fit a texture.
                            let next_reflections = base.delta_from(GraphicsConfig {
                                reflections: other_reflections(reflections),
                                ..base
                            });
                            assert!(
                                next_reflections.reflections && next_reflections.needs_gpu_work()
                            );
                            assert!(!next_reflections.needs_build() && !next_reflections.quality);
                            assert!(!next_reflections.needs_texture_refit());

                            let next_filtering = base.delta_from(GraphicsConfig {
                                filtering: if filtering == TextureFiltering::High {
                                    TextureFiltering::Low
                                } else {
                                    TextureFiltering::High
                                },
                                ..base
                            });
                            assert!(next_filtering.filtering && !next_filtering.needs_gpu_work());
                            assert!(
                                !next_filtering.needs_build()
                                    && !next_filtering.needs_texture_refit()
                                    && !next_filtering.quality
                            );

                            let next_bloom = base.delta_from(GraphicsConfig {
                                bloom: !bloom,
                                ..base
                            });
                            assert!(next_bloom.bloom && !next_bloom.needs_gpu_work());
                            assert!(
                                !next_bloom.needs_build()
                                    && !next_bloom.needs_texture_refit()
                                    && !next_bloom.quality
                            );
                        }
                    }
                }
            }
        }
    }
}

//! The renderer facade: the one rendering type the engine names.
//!
//! [`Renderer`] dispatches to the implementation selected at startup by
//! [`RendererBackend`](crate::render::RendererBackend):
//!
//! * [`Renderer::Opengl`] is the complete, reference OpenGL/GLES2 renderer —
//!   the same code, shaders and output as before Stage 4.
//! * [`Renderer::Wgpu`] is the wgpu renderer: it owns the device and surface
//!   lifecycle and, since Stage 9, draws the complete Places frame — the static
//!   world, props and dynamic objects, the baked lightmap atlas and its
//!   vertex-lit fallback, reflection probes and the planar mirror, fixture
//!   emission, decals, fog, the emissive bloom chain and resolve, and the
//!   480x272 HUD. See `docs/WGPU_STAGE9.md`.
//!
//! The facade exposes exactly the operations the engine performs. Two
//! wgpu-specific differences are documented rather than silent: the benchmark's
//! indexing and vertex-layout switches measure the OpenGL path only, and the
//! `PLACES_NO_OFFSCREEN` diagnostic direct path is OpenGL-only (wgpu always
//! runs the offscreen chain, the reference's own default). Neither changes a
//! shipped frame. See `docs/RENDERER_BOUNDARY.md`, `docs/WGPU_BOOTSTRAP.md`,
//! `docs/WGPU_WORLD_GEOMETRY.md` and `docs/WGPU_STAGE9.md`.

use sdl2::VideoSubsystem;
use sdl2::video::Window;

use super::backend::RendererBackend;
use super::common::dynamic::{DynamicScene, DynamicUpdate};
use super::common::mesh::VertexLayout;
use super::common::stats::{LevelBuildStats, RenderStats};
use super::common::view::DrawableSize;
use super::{RenderCamera, SurfaceKind, Vertex, opengl, wgpu};
use crate::level::LevelDef;
use crate::loader::{LoadedLevel, RawImage};
use crate::props::PropAssetStats;
use crate::quality::QualityProfile;
use crate::spatial::CellGrid;

/// The renderer implementation chosen for this process.
///
/// Both variants present the same operations to the engine. The variant is
/// fixed for the lifetime of the process: there is no hot-swap. The variants
/// are boxed because the OpenGL renderer is by far the larger state.
pub enum Renderer {
    /// The complete OpenGL/GLES2 reference renderer.
    Opengl(Box<opengl::renderer::Renderer>),
    /// The complete wgpu renderer: the Stage 4 device lifecycle plus every
    /// rendering feature of the OpenGL reference (Stages 5-9).
    Wgpu(Box<wgpu::WgpuRenderer>),
}

impl Renderer {
    /// Builds the selected backend for `window`.
    ///
    /// The OpenGL backend additionally creates the GL context; the wgpu
    /// backend does not touch OpenGL at all. The window must already have been
    /// created for the chosen backend (see `render::apply_window_flags`).
    ///
    /// # Errors
    ///
    /// Returns the backend's initialization message: a missing GL context or
    /// shader failure for OpenGL, or a missing native adapter, device or
    /// surface for wgpu.
    pub fn new(
        window: &Window,
        video: &VideoSubsystem,
        backend: RendererBackend,
    ) -> Result<Self, String> {
        match backend {
            RendererBackend::Opengl => opengl::renderer::Renderer::new(window, video)
                .map(|renderer| Self::Opengl(Box::new(renderer))),
            RendererBackend::Wgpu => {
                wgpu::WgpuRenderer::new(window).map(|renderer| Self::Wgpu(Box::new(renderer)))
            }
        }
    }

    /// Applies the player's `VSync` preference and reports the interval in force.
    ///
    /// OpenGL sets the SDL swap interval; wgpu selects the supported
    /// presentation mode (Fifo/Immediate) and reconfigures. Returning the
    /// interval keeps the benchmark report meaningful for both backends.
    pub fn set_swap_interval(&mut self, video: &VideoSubsystem, want_vsync: bool) -> i32 {
        match self {
            Self::Opengl(_) => opengl::context::apply_swap_interval(video, want_vsync),
            Self::Wgpu(renderer) => renderer.set_swap_interval(video, want_vsync),
        }
    }

    /// Presents the frame submitted since the last call.
    ///
    /// OpenGL swaps the window buffers; wgpu presents the surface texture its
    /// last `render_scene` acquired (or cycles the swapchain when nothing was
    /// rendered).
    pub fn present(&mut self, window: &Window) {
        match self {
            Self::Opengl(_) => opengl::context::present(window),
            Self::Wgpu(renderer) => renderer.present(window),
        }
    }

    /// A fatal GPU error that stopped the renderer, if any.
    ///
    /// The OpenGL backend has no such state; the wgpu backend reports device
    /// loss here so the engine can stop through its normal shutdown path and
    /// exit with an error instead of issuing work on a lost device.
    #[must_use]
    pub fn fatal_error(&self) -> Option<&str> {
        match self {
            Self::Opengl(_) => None,
            Self::Wgpu(renderer) => renderer.fatal_error(),
        }
    }

    /// Records the physical drawable size; returns `true` when it changed.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        match self {
            Self::Opengl(renderer) => renderer.set_drawable_size(size),
            Self::Wgpu(renderer) => renderer.set_drawable_size(size),
        }
    }

    /// Selects the runtime quality profile.
    ///
    /// Both backends use the profile to fit textures to their edge budget at
    /// level upload; a profile change is applied by releasing the profile
    /// textures and re-uploading the level.
    pub fn set_quality(&mut self, quality: QualityProfile) {
        match self {
            Self::Opengl(renderer) => renderer.set_quality(quality),
            Self::Wgpu(renderer) => renderer.set_quality(quality),
        }
    }

    /// Selects whether the next level build bakes lightmaps.
    ///
    /// Both backends apply the value at the next level upload, exactly like the
    /// reference: the current frame keeps sampling the atlas it already has.
    pub fn set_lightmaps_requested(&mut self, requested: bool) {
        match self {
            Self::Opengl(renderer) => renderer.set_lightmaps_requested(requested),
            Self::Wgpu(renderer) => renderer.set_lightmaps_requested(requested),
        }
    }

    /// Switches bloom on or off.
    ///
    /// The wgpu gate is applied to the next frame's post settings and releases
    /// the bloom targets when it turns off, exactly like the reference.
    pub fn set_bloom_enabled(&mut self, enabled: bool) {
        match self {
            Self::Opengl(renderer) => renderer.set_bloom_enabled(enabled),
            Self::Wgpu(renderer) => renderer.set_bloom_enabled(enabled),
        }
    }

    /// Switches selective reflections on or off.
    ///
    /// The wgpu gate is applied to the next frame's material modes and plane
    /// selection, exactly like the reference's `reflections_supported`.
    pub fn set_reflections_enabled(&mut self, enabled: bool) {
        match self {
            Self::Opengl(renderer) => renderer.set_reflections_enabled(enabled),
            Self::Wgpu(renderer) => renderer.set_reflections_enabled(enabled),
        }
    }

    /// Releases texture caches that depend on the quality profile.
    ///
    /// The wgpu texture cache keeps the same two lifetimes as OpenGL: catalog
    /// and diagnostic textures persist for the renderer's lifetime, pack
    /// textures live for one level. A profile change drops both so the next
    /// level upload re-fits every texture at the new budget.
    pub fn release_profile_textures(&mut self) {
        match self {
            Self::Opengl(renderer) => renderer.release_profile_textures(),
            Self::Wgpu(renderer) => renderer.release_profile_textures(),
        }
    }

    /// Applies the player's texture filtering preference.
    ///
    /// wgpu swaps between its two shared world samplers; the OpenGL backend
    /// re-filters its textures in place.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        match self {
            Self::Opengl(renderer) => renderer.set_texture_filtering(mode),
            Self::Wgpu(renderer) => renderer.set_texture_filtering(mode),
        }
    }

    /// Uploads a loaded level.
    ///
    /// Both backends upload the whole level: the renderer-neutral geometry,
    /// props, dynamics, fixtures, decals, the lightmap atlas and the reflection
    /// probes, each through its own GPU resources. Since Stage 9 the wgpu path
    /// draws the complete reference frame.
    pub fn set_level(&mut self, loaded: &LoadedLevel) {
        match self {
            Self::Opengl(renderer) => renderer.set_level(loaded),
            Self::Wgpu(renderer) => renderer.set_level(loaded),
        }
    }

    /// Spawns the level's dynamic demonstration objects.
    ///
    /// Both backends spawn the same neutral `DynamicScene` demonstration; the
    /// wgpu side uploads its GPU meshes and per-object light-probe
    /// environments at the same time.
    pub fn set_dynamic_demo(&mut self, level: &LevelDef) -> usize {
        match self {
            Self::Opengl(renderer) => renderer.set_dynamic_demo(level),
            Self::Wgpu(renderer) => renderer.set_dynamic_demo(level),
        }
    }

    /// Advances the dynamic objects by `delta_seconds`.
    ///
    /// The neutral dynamic scene is real for both backends; wgpu uploads and
    /// transforms its own GPU meshes.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        match self {
            Self::Opengl(renderer) => renderer.update_dynamic(delta_seconds),
            Self::Wgpu(renderer) => renderer.update_dynamic(delta_seconds),
        }
    }

    /// The current dynamic scene, for the developer log.
    #[must_use]
    pub fn dynamic_scene(&self) -> &DynamicScene {
        match self {
            Self::Opengl(renderer) => renderer.dynamic_scene(),
            Self::Wgpu(renderer) => renderer.dynamic_scene(),
        }
    }

    /// Enables or disables frustum culling (benchmark switch).
    ///
    /// Both backends cull static world ranges against the frame frustum; the
    /// switch exists so the benchmark can measure the cost.
    pub fn set_culling(&mut self, enabled: bool) {
        match self {
            Self::Opengl(renderer) => renderer.set_culling(enabled),
            Self::Wgpu(renderer) => renderer.set_culling(enabled),
        }
    }

    /// Selects indexed or flat submission (benchmark switch).
    ///
    /// wgpu Stage 5 always submits indexed world geometry; the switch is
    /// accepted and ignored (it exists to measure the OpenGL path).
    pub fn set_indexing(&mut self, enabled: bool) {
        match self {
            Self::Opengl(renderer) => renderer.set_indexing(enabled),
            Self::Wgpu(_) => {}
        }
    }

    /// Selects the vertex layout uploaded for the scene and UI (benchmark
    /// switch).
    ///
    /// wgpu Stage 5 has one world vertex layout; the switch is accepted and
    /// ignored (it exists to measure the OpenGL path).
    pub fn set_vertex_layout(&mut self, layout: VertexLayout) {
        match self {
            Self::Opengl(renderer) => renderer.set_vertex_layout(layout),
            Self::Wgpu(_) => {}
        }
    }

    /// Renders one frame's scene from `camera`.
    ///
    /// OpenGL draws the level and its passes; wgpu clears the colour and depth
    /// attachments and submits the uploaded world passes — opaque, cut-out and
    /// translucent — from the same camera.
    pub fn render_scene(&mut self, camera: RenderCamera) {
        match self {
            Self::Opengl(renderer) => renderer.render_scene(camera),
            Self::Wgpu(renderer) => renderer.render_scene(camera),
        }
    }

    /// Renders the 2D UI vertex list.
    ///
    /// The wgpu backend draws the same 480x272 reference HUD into the frame its
    /// `render_scene` acquired, with depth testing off and straight-alpha
    /// blending, exactly like the reference.
    pub fn render_ui(&mut self, ui_vertices: &[Vertex]) {
        match self {
            Self::Opengl(renderer) => renderer.render_ui(ui_vertices),
            Self::Wgpu(renderer) => renderer.render_ui(ui_vertices),
        }
    }

    /// Waits for submitted GPU work to finish (benchmark diagnostic).
    pub fn finish(&self) {
        match self {
            Self::Opengl(renderer) => renderer.finish(),
            Self::Wgpu(renderer) => renderer.finish(),
        }
    }

    /// Reads back the last rendered scene as an image.
    ///
    /// Both backends read back the finished frame after the resolve and the
    /// HUD. The wgpu post path re-encodes the chain into its raw presented
    /// image and copies that to a raw capture texture (see
    /// [`crate::render::wgpu::WgpuRenderer::capture_default_framebuffer`]).
    ///
    /// # Errors
    ///
    /// Reports why a frame cannot be read back (an empty drawable, no rendered
    /// frame, no built world pipeline or depth target, or a GPU read-back
    /// failure).
    pub fn capture_default_framebuffer(&mut self) -> Result<RawImage, String> {
        match self {
            Self::Opengl(renderer) => renderer.capture_default_framebuffer(),
            Self::Wgpu(renderer) => renderer.capture_default_framebuffer(),
        }
    }

    /// Counters for the most recently submitted frame.
    #[must_use]
    pub fn render_stats(&self) -> RenderStats {
        match self {
            Self::Opengl(renderer) => renderer.render_stats(),
            Self::Wgpu(renderer) => renderer.render_stats(),
        }
    }

    /// Cost and shape of the most recently built level.
    #[must_use]
    pub fn level_stats(&self) -> LevelBuildStats {
        match self {
            Self::Opengl(renderer) => renderer.level_stats(),
            Self::Wgpu(renderer) => renderer.level_stats(),
        }
    }

    /// Decoded prop-model statistics.
    #[must_use]
    pub fn prop_asset_stats(&self) -> PropAssetStats {
        match self {
            Self::Opengl(renderer) => renderer.prop_asset_stats(),
            Self::Wgpu(renderer) => renderer.prop_asset_stats(),
        }
    }

    /// Real prop draw calls in the current level.
    #[must_use]
    pub fn prop_draw_count(&self) -> usize {
        match self {
            Self::Opengl(renderer) => renderer.prop_draw_count(),
            Self::Wgpu(renderer) => renderer.prop_draw_count(),
        }
    }

    /// Cullable static batches in the current level.
    #[must_use]
    pub fn static_batch_count(&self) -> usize {
        match self {
            Self::Opengl(renderer) => renderer.static_batch_count(),
            Self::Wgpu(renderer) => renderer.static_batch_count(),
        }
    }

    /// Static batches per surface family, for the developer log.
    #[must_use]
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        match self {
            Self::Opengl(renderer) => renderer.static_batch_family_breakdown(),
            Self::Wgpu(renderer) => renderer.static_batch_family_breakdown(),
        }
    }

    /// The spatial grid the current level was partitioned with.
    ///
    /// The wgpu Stage 5 renderer preserves the neutral build's draw order and
    /// has no per-cell grid of its own; the empty grid is the honest answer.
    #[must_use]
    pub fn spatial_grid(&self) -> CellGrid {
        match self {
            Self::Opengl(renderer) => renderer.spatial_grid(),
            Self::Wgpu(_) => CellGrid::default(),
        }
    }
}

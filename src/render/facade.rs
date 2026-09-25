//! The renderer facade: the one rendering type the engine names.
//!
//! [`Renderer`] owns the wgpu renderer — the complete Places frame: the static
//! world, props and dynamic objects, the baked lightmap atlas and its
//! vertex-lit fallback, reflection probes and the planar mirror, fixture
//! emission, decals, fog, the emissive bloom chain and resolve, and the 480x272
//! HUD. It exposes exactly the operations the engine performs; every parameter
//! and return value is engine data, never a GPU object.
//!
//! The facade is deliberately narrow: it is the engine/renderer seam described
//! by `docs/RENDERER_BOUNDARY.md`, not a multi-backend abstraction. See
//! `docs/WGPU_STAGE9.md` for the ported features and
//! `docs/RENDERER_REFERENCE.md` for the preserved OpenGL reference.

use sdl2::VideoSubsystem;
use sdl2::video::Window;

use super::common::dynamic::{DynamicScene, DynamicUpdate};
use super::common::stats::{LevelBuildStats, RenderStats};
use super::common::view::DrawableSize;
use super::wgpu::WgpuRenderer;
use super::{RenderCamera, SurfaceKind, Vertex};
use crate::level::LevelDef;
use crate::loader::{LoadedLevel, RawImage};
use crate::props::PropAssetStats;
use crate::quality::QualityProfile;

/// The Places renderer: the wgpu implementation behind a narrow seam.
pub struct Renderer {
    renderer: WgpuRenderer,
}

impl Renderer {
    /// Builds the renderer for `window`.
    ///
    /// The window must already have been created with the flags
    /// [`crate::render::apply_window_flags`] applies.
    ///
    /// # Errors
    ///
    /// Returns the backend's initialization message when no native adapter,
    /// device or surface can be created.
    pub fn new(window: &Window) -> Result<Self, String> {
        WgpuRenderer::new(window).map(|renderer| Self { renderer })
    }

    /// Applies the player's `VSync` preference and reports the interval in force.
    ///
    /// The renderer selects the supported presentation mode (Fifo/Immediate)
    /// and reconfigures; returning the interval keeps the benchmark report
    /// meaningful.
    pub fn set_swap_interval(&mut self, video: &VideoSubsystem, want_vsync: bool) -> i32 {
        self.renderer.set_swap_interval(video, want_vsync)
    }

    /// Presents the frame submitted since the last call.
    pub fn present(&mut self, window: &Window) {
        self.renderer.present(window);
    }

    /// A fatal GPU error that stopped the renderer, if any.
    ///
    /// The backend reports device loss here so the engine can stop through its
    /// normal shutdown path and exit with an error instead of issuing work on a
    /// lost device.
    #[must_use]
    pub fn fatal_error(&self) -> Option<&str> {
        self.renderer.fatal_error()
    }

    /// Records the physical drawable size; returns `true` when it changed.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        self.renderer.set_drawable_size(size)
    }

    /// Selects the runtime quality profile.
    ///
    /// The profile is applied by releasing the profile textures and
    /// re-uploading the level.
    pub const fn set_quality(&mut self, quality: QualityProfile) {
        self.renderer.set_quality(quality);
    }

    /// Selects whether the next level build bakes lightmaps.
    ///
    /// The value applies at the next level upload: the current frame keeps
    /// sampling the atlas it already has.
    pub const fn set_lightmaps_requested(&mut self, requested: bool) {
        self.renderer.set_lightmaps_requested(requested);
    }

    /// Switches bloom on or off.
    ///
    /// The gate is applied to the next frame's post settings and releases the
    /// bloom targets when it turns off.
    pub const fn set_bloom_enabled(&mut self, enabled: bool) {
        self.renderer.set_bloom_enabled(enabled);
    }

    /// Switches selective reflections on or off.
    ///
    /// The gate is applied to the next frame's material modes and plane
    /// selection.
    pub const fn set_reflections_enabled(&mut self, enabled: bool) {
        self.renderer.set_reflections_enabled(enabled);
    }

    /// Releases texture caches that depend on the quality profile.
    ///
    /// A profile change drops both the persistent and the per-level caches so
    /// the next level upload re-fits every texture at the new budget.
    pub fn release_profile_textures(&mut self) {
        self.renderer.release_profile_textures();
    }

    /// Applies the player's texture filtering preference.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        self.renderer.set_texture_filtering(mode);
    }

    /// Uploads a loaded level.
    ///
    /// The renderer uploads the renderer-neutral geometry, props, dynamics,
    /// fixtures, decals, the lightmap atlas and the reflection probes.
    pub fn set_level(&mut self, loaded: &LoadedLevel) {
        self.renderer.set_level(loaded);
    }

    /// Spawns the level's dynamic demonstration objects.
    pub fn set_dynamic_demo(&mut self, level: &LevelDef) -> usize {
        self.renderer.set_dynamic_demo(level)
    }

    /// Advances the dynamic objects by `delta_seconds`.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        self.renderer.update_dynamic(delta_seconds)
    }

    /// The current dynamic scene, for the developer log.
    #[must_use]
    pub const fn dynamic_scene(&self) -> &DynamicScene {
        self.renderer.dynamic_scene()
    }

    /// Enables or disables frustum culling (benchmark switch).
    pub const fn set_culling(&mut self, enabled: bool) {
        self.renderer.set_culling(enabled);
    }

    /// Renders one frame's scene from `camera`.
    pub fn render_scene(&mut self, camera: RenderCamera) {
        self.renderer.render_scene(camera);
    }

    /// Renders the 2D UI vertex list.
    ///
    /// The HUD draws into the frame `render_scene` acquired, with depth testing
    /// off and straight-alpha blending.
    pub fn render_ui(&mut self, ui_vertices: &[Vertex]) {
        self.renderer.render_ui(ui_vertices);
    }

    /// Waits for submitted GPU work to finish (benchmark diagnostic).
    pub fn finish(&self) {
        self.renderer.finish();
    }

    /// Reads back the last rendered scene as an image.
    ///
    /// The post path re-encodes the finished frame after the resolve and the
    /// HUD and copies it to a raw capture texture.
    ///
    /// # Errors
    ///
    /// Reports why a frame cannot be read back (an empty drawable, no rendered
    /// frame, no built world pipeline or depth target, or a GPU read-back
    /// failure).
    pub fn capture_default_framebuffer(&mut self) -> Result<RawImage, String> {
        self.renderer.capture_default_framebuffer()
    }

    /// Counters for the most recently submitted frame.
    #[must_use]
    pub const fn render_stats(&self) -> RenderStats {
        self.renderer.render_stats()
    }

    /// Cost and shape of the most recently built level.
    #[must_use]
    pub const fn level_stats(&self) -> LevelBuildStats {
        self.renderer.level_stats()
    }

    /// Decoded prop-model statistics.
    #[must_use]
    pub fn prop_asset_stats(&self) -> PropAssetStats {
        self.renderer.prop_asset_stats()
    }

    /// Real prop draw calls in the current level.
    #[must_use]
    pub fn prop_draw_count(&self) -> usize {
        self.renderer.prop_draw_count()
    }

    /// Cullable static batches in the current level.
    #[must_use]
    pub fn static_batch_count(&self) -> usize {
        self.renderer.static_batch_count()
    }

    /// Static batches per surface family, for the developer log.
    #[must_use]
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        self.renderer.static_batch_family_breakdown()
    }
}

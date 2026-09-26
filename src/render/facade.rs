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
//! by `docs/ARCHITECTURE.md`, not a multi-backend abstraction. See
//! `docs/RENDERER.md` for the current contracts and `docs/RENDERER_REFERENCE.md`
//! for the preserved reference implementation.

use sdl3::video::Window;

use super::common::api::GraphicsTransition;
use super::common::character::CharacterScene;
use super::common::dynamic::{DynamicScene, DynamicUpdate};
use super::common::stats::{LevelBuildStats, RenderStats};
use super::common::view::DrawableSize;
use super::wgpu::WgpuRenderer;
use super::{RenderCamera, SurfaceKind, Vertex};
use crate::game::LocomotionSnapshot;
use crate::level::LevelDef;
use crate::loader::{LoadedLevel, RawImage};
use crate::props::PropAssetStats;
use crate::quality::{LightmapQuality, QualityLevel, ReflectionQuality};

/// The Places renderer: the wgpu implementation behind a narrow seam.
pub struct Renderer {
    renderer: WgpuRenderer,
}

impl Renderer {
    /// Builds the renderer for `window`.
    ///
    /// The window is a plain SDL3 window; the raw-window-handle path needs no
    /// platform window flag.
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
    /// meaningful. Presentation is wgpu's surface configuration, not an SDL
    /// swap interval, so no SDL handle is needed here.
    pub fn set_swap_interval(&mut self, want_vsync: bool) -> i32 {
        self.renderer.set_swap_interval(want_vsync)
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

    /// Selects the runtime quality level.
    ///
    /// Recording only: [`Self::apply_graphics`] re-fits the retained level's
    /// textures at the new budget (and, when the lightmap configuration changed
    /// in the same settings action, rebuilds the level once).
    pub const fn set_quality(&mut self, quality: QualityLevel) {
        self.renderer.set_quality(quality);
    }

    /// Selects the Lightmaps quality.
    ///
    /// Recording only: [`Self::apply_graphics`] rebuilds the CPU lighting and
    /// mesh once for the new configuration. A cache hit activates immediately;
    /// a miss fills on one worker thread while the previous atlas keeps
    /// rendering, and the frame loop never blocks on it.
    pub const fn set_lightmap_quality(&mut self, quality: LightmapQuality) {
        self.renderer.set_lightmap_quality(quality);
    }

    /// Selects the Reflections quality.
    ///
    /// Recording only: [`Self::apply_graphics`] retires or creates the probe
    /// cubemaps and the planar target, rebakes the probes and rebuilds the
    /// environment bind groups. `Off` drops both sources and stops all capture
    /// work.
    pub const fn set_reflection_quality(&mut self, quality: ReflectionQuality) {
        self.renderer.set_reflection_quality(quality);
    }

    /// Switches bloom on or off.
    ///
    /// The gate applies at the next frame's post settings: with bloom off no
    /// emissive or blur pass is submitted, and the bloom targets stay allocated
    /// but unused. No resources are rebuilt.
    pub const fn set_bloom_enabled(&mut self, enabled: bool) {
        self.renderer.set_bloom_enabled(enabled);
    }

    /// Applies every graphics setting recorded since the last call.
    ///
    /// This is the one entry point for a settings action: the renderer diffs
    /// the requested configuration against the applied one and does exactly the
    /// work the difference implies, as one transaction. A filtering-only or
    /// bloom-only change touches no GPU resource; a quality-only change reuses
    /// the retained CPU build; a lightmap change rebuilds the CPU level once
    /// and defers an uncached fill to a worker.
    ///
    /// `loaded` is the level already resident; the game world — player,
    /// camera, pause state — is not touched.
    pub fn apply_graphics(&mut self, loaded: &LoadedLevel) {
        self.renderer.apply_graphics(loaded);
    }

    /// Polls the asynchronous graphics stage, if any.
    ///
    /// The frame loop may call this (or rely on `render_scene`, which polls) to
    /// install a finished background lightmap fill. It is a couple of branches
    /// when idle.
    pub fn advance_graphics_transition(&mut self) {
        self.renderer.advance_graphics_transition();
    }

    /// A cheap status: whether a graphics transition is in flight.
    ///
    /// `Preparing("lightmaps")` means an uncached atlas is filling on a worker
    /// while the previous configuration keeps rendering; a status hint may be
    /// shown until it returns to `Idle`.
    #[must_use]
    pub const fn graphics_transition_status(&self) -> GraphicsTransition {
        self.renderer.graphics_transition_status()
    }

    /// Releases texture caches that depend on the quality level.
    ///
    /// A level change drops both the persistent and the per-level caches so the
    /// next level upload re-fits every texture at the new budget.
    pub fn release_profile_textures(&mut self) {
        self.renderer.release_profile_textures();
    }

    /// Applies the player's texture filtering preference.
    ///
    /// The three levels are trilinear with anisotropic filtering (`low` 4x,
    /// `medium` 8x, `high` 16x); the legacy `linear`/`nearest` names map to
    /// High/Low, and an empty or unknown value keeps the default (High).
    /// Switching is live: the world, material and decal bind groups swap the
    /// sampler handle at bind time, with no texture re-upload and no resource
    /// rebuild. The baked lightmap atlas keeps its own fixed clamped linear
    /// sampler and never follows this setting.
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

    /// Advances every animated character's pose and re-uploads the ones that
    /// moved.
    ///
    /// `locomotion` is the player's state for this frame; each character's
    /// blend weights and phase follow it. Returns how many characters moved
    /// (and were therefore re-skinned and uploaded this frame).
    pub fn update_characters(
        &mut self,
        delta_seconds: f32,
        locomotion: LocomotionSnapshot,
    ) -> usize {
        self.renderer.update_characters(delta_seconds, locomotion)
    }

    /// The number of live animated characters in the current level.
    #[must_use]
    pub const fn character_count(&self) -> usize {
        self.renderer.character_count()
    }

    /// The current character scene, for the developer log.
    #[must_use]
    pub const fn character_scene(&self) -> &CharacterScene {
        self.renderer.character_scene()
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

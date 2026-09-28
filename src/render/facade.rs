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
use super::common::character::{CharacterScene, EntityFrame};
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

    /// Builds a window-free renderer for the offline probe capture.
    ///
    /// Used only by `places-compile`: the player always owns a window. The
    /// returned renderer can install a prepared world, capture its reflection
    /// probes and read them back; it never presents.
    ///
    /// # Errors
    ///
    /// Returns the backend's initialization message when no native adapter or
    /// device can be created.
    pub fn new_headless(drawable: DrawableSize) -> Result<Self, String> {
        WgpuRenderer::new_headless(drawable).map(|renderer| Self { renderer })
    }

    /// Re-captures the resident world's reflection probes.
    ///
    /// The offline compiler calls this after installation, and again after
    /// changing the reflection quality, so both packaged face sizes are the
    /// proven capture result.
    pub fn capture_reflection_probes(&mut self) {
        self.renderer.capture_reflection_probes();
    }

    /// Recreates the probe targets at `quality` and re-captures them.
    pub fn reprepare_reflection_probes(&mut self, quality: ReflectionQuality) {
        self.renderer.reprepare_reflection_probes(quality);
    }

    /// Reads the resident probe cubemaps back as RGBA8 base faces.
    ///
    /// The compiler turns each readback into the packaged roughness mip chain
    /// (`ProbeFaceReadback::packaged_mips`); the resident texture is sampled
    /// at level 0 until such a chain is uploaded back into it.
    ///
    /// # Errors
    ///
    /// Returns an error when a face buffer cannot be mapped or read.
    pub fn read_back_probe_faces(&mut self) -> Result<Vec<super::ProbeFaceReadback>, String> {
        self.renderer.read_back_probe_faces()
    }

    /// Applies sampler/post gates without rebuilding the world.
    pub fn apply_frame_graphics(&mut self) -> bool {
        self.renderer.apply_frame_graphics()
    }

    /// Identity and effective preparation quality of the resident world.
    pub fn installed_identity(
        &self,
    ) -> (
        Option<&str>,
        crate::quality::QualityLevel,
        crate::quality::LightmapQuality,
    ) {
        self.renderer.installed_identity()
    }

    /// Installs a complete CPU bundle prepared by the loading worker.
    pub fn install_prepared(
        &mut self,
        loaded: &LoadedLevel,
        build: std::sync::Arc<super::LevelBuild>,
        assets: crate::props::PropAssets,
        characters: CharacterScene,
        preserve_playback: bool,
    ) {
        self.renderer
            .install_prepared(loaded, build, assets, characters, preserve_playback);
    }

    /// Installs a complete decoded package variant whose reflection probes
    /// were captured by the compiler.
    ///
    /// The player path: packaged probes are uploaded and never baked.
    pub fn install_prepared_precompiled(
        &mut self,
        loaded: &LoadedLevel,
        build: std::sync::Arc<super::LevelBuild>,
        assets: crate::props::PropAssets,
        characters: CharacterScene,
        preserve_playback: bool,
        probes: crate::package::world::ProbeCaptures,
    ) {
        self.renderer.install_prepared_precompiled(
            loaded,
            build,
            assets,
            characters,
            preserve_playback,
            probes,
        );
    }

    /// Advances bounded GPU preparation; true only once the whole world is installed.
    pub fn advance_prepared_install(&mut self) -> bool {
        self.renderer.advance_prepared_install()
    }

    /// Discards an upload superseded by a newer request.
    pub fn cancel_prepared_install(&mut self) {
        self.renderer.cancel_prepared_install();
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
    pub fn present(&mut self, window: &Window) -> bool {
        self.renderer.present(window)
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

    /// A cheap status: whether a graphics transition is in flight.
    ///
    /// GPU resources are prepared while the previous world keeps rendering.
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
    /// `medium` 8x, `high` 16x). An empty or unknown value keeps the default
    /// (High).
    /// Switching is live: the world, material and decal bind groups swap the
    /// sampler handle at bind time, with no texture re-upload and no resource
    /// rebuild. The baked lightmap atlas keeps its own fixed clamped linear
    /// sampler and never follows this setting.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        self.renderer.set_texture_filtering(mode);
    }

    /// Spawns the level's dynamic demonstration objects.
    pub fn set_dynamic_demo(&mut self, level: &LevelDef) -> usize {
        self.renderer.set_dynamic_demo(level)
    }

    /// Advances the dynamic objects by `delta_seconds`.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        self.renderer.update_dynamic(delta_seconds)
    }

    /// Spawns one runtime entity's model as a dynamic object.
    ///
    /// `key` is the engine's stable runtime token; spawning a live key replaces
    /// the previous object. `model` is a catalogue registry id (such as
    /// `core:crate`) or a direct model path. Returns `false` when the model
    /// cannot be resolved or the scene is full; an unresolvable model is
    /// reported once per model path.
    pub fn spawn_runtime_model(
        &mut self,
        key: u64,
        model: &str,
        position: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
    ) -> bool {
        self.renderer
            .spawn_runtime_model(key, model, position, yaw_degrees, scale)
    }

    /// Despawns the runtime object a key spawned; returns whether it was live.
    ///
    /// The key map is cleared with the neutral dynamic scene by a level
    /// change or a demonstration respawn, so a stale key never resolves to an
    /// object of another install.
    pub fn despawn_runtime_model(&mut self, key: u64) -> bool {
        self.renderer.despawn_runtime_model(key)
    }

    /// Moves a live runtime object; returns whether it was live.
    ///
    /// The object keeps its mesh, scale and material. The write reaches the
    /// GPU through [`Self::update_dynamic`]'s sync, so it forces no geometry
    /// re-upload.
    pub fn set_runtime_transform(
        &mut self,
        key: u64,
        position: [f32; 3],
        yaw_degrees: f32,
    ) -> bool {
        self.renderer
            .set_runtime_transform(key, position, yaw_degrees)
    }

    /// Selects the object's emission scale (the runtime material-variant
    /// effect).
    ///
    /// The scale multiplies every primitive emission the object draws, so
    /// `0.0` switches its emission off. Returns whether the key held a live
    /// object and the scale was accepted.
    pub fn set_runtime_emission(&mut self, key: u64, scale: f32) -> bool {
        self.renderer.set_runtime_emission(key, scale)
    }

    /// Live runtime-spawned object count (spawned-object budget
    /// diagnostics/tests).
    #[must_use]
    pub fn runtime_spawn_count(&self) -> usize {
        self.renderer.runtime_spawn_count()
    }

    /// Republishes every door leaf's angle from the gameplay state.
    ///
    /// The drawn slab and the physical collider read the same door angle, so
    /// this is the one hand-off that keeps them in agreement.
    pub fn sync_doors(&mut self, doors: &crate::door::Doors) {
        self.renderer.sync_doors(doors);
    }

    /// Applies this frame's fixture switches: illumination and face emission.
    ///
    /// The gameplay side owns the switch state; the renderer re-fills the
    /// fixture's lightmap charts, re-uploads their pages and scales its face
    /// emission, so a light that is off neither illuminates nor glows.
    pub fn apply_light_toggles(&mut self, toggles: &[(usize, bool)]) {
        self.renderer.apply_light_toggles(toggles);
    }

    /// The installed level's door count and the number that spawned to draw.
    #[must_use]
    pub const fn door_render_counts(&self) -> (usize, usize) {
        self.renderer.door_render_counts()
    }

    /// Spawns every placed prop that authors `float` on its water surface.
    pub fn set_floating_props(&mut self, level: &LevelDef) -> usize {
        self.renderer.set_floating_props(level)
    }

    /// Installs (or re-installs) the level's ambient effect emitters.
    ///
    /// Idempotent: the level install already built and uploaded the steam
    /// plumes from the resolved material table; this call re-uploads only if
    /// the GPU side is missing and ignores a level other than the installed
    /// one. Returns the number of live emitters.
    pub fn set_level_effects(&mut self, level: &LevelDef) -> usize {
        self.renderer.set_level_effects(level)
    }

    /// Enables or disables one authored effect emitter at runtime.
    pub fn set_effect_enabled(&mut self, authored_index: usize, enabled: bool) -> bool {
        self.renderer.set_effect_enabled(authored_index, enabled)
    }

    /// Advances every animated character's pose and re-uploads the ones that
    /// moved.
    ///
    /// `locomotion` is the player's state for this frame; each character's
    /// blend weights and phase follow it unless `frames` addresses that
    /// character by instance id (a route's live transform and pose cue); a
    /// runtime-spawned actor is driven by its frame alone and holds its pose
    /// without one. The caller owns the frame list
    /// ([`crate::game::Game::entity_frames`]). Returns how many characters
    /// moved (and were therefore re-skinned). A runtime spawn or despawn since
    /// the last call rebuilds the character GPU state before it syncs.
    pub fn update_characters(
        &mut self,
        delta_seconds: f32,
        locomotion: LocomotionSnapshot,
        frames: &[EntityFrame],
    ) -> crate::render::CharacterUpdate {
        self.renderer
            .update_characters(delta_seconds, locomotion, frames)
    }

    /// Spawns an animatable runtime actor through the character path.
    ///
    /// `model` is a catalogue registry id (such as `rat`) or a direct model
    /// path, resolved exactly like [`Self::spawn_runtime_model`]. The actor is
    /// lit by the installed level's baked lighting and irradiance field and
    /// animates through the ordinary clip path, driven by an [`EntityFrame`]
    /// addressed to `instance_id`. Spawning a live instance id replaces that
    /// actor. The actor becomes visible on the next [`Self::update_characters`]
    /// sync.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the model cannot be animated (the caller falls back
    /// to the dynamic path) or the scene refused the placement: no installed
    /// level or lighting, a non-finite position or a non-positive scale, or a
    /// full character budget.
    pub fn spawn_runtime_character(
        &mut self,
        instance_id: &str,
        model: &str,
        position: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
    ) -> Result<(), String> {
        self.renderer
            .spawn_runtime_character(instance_id, model, position, yaw_degrees, scale)
    }

    /// Removes a runtime actor; true when one existed.
    ///
    /// The GPU side is rebuilt by the next [`Self::update_characters`].
    pub fn despawn_runtime_character(&mut self, instance_id: &str) -> bool {
        self.renderer.despawn_runtime_character(instance_id)
    }

    /// Moves a live runtime actor; true when one existed.
    ///
    /// The actor keeps its model, scale and playback; the write reaches the
    /// GPU through the next [`Self::update_characters`] sync.
    pub fn set_runtime_character_transform(
        &mut self,
        instance_id: &str,
        position: [f32; 3],
        yaw_degrees: f32,
    ) -> bool {
        self.renderer
            .set_runtime_character_transform(instance_id, position, yaw_degrees)
    }

    /// True when a runtime actor with this id is live.
    #[must_use]
    pub fn has_runtime_character(&self, instance_id: &str) -> bool {
        self.renderer.has_runtime_character(instance_id)
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

    /// Mutable access to the current character scene (gameplay and tests).
    pub const fn character_scene_mut(&mut self) -> &mut CharacterScene {
        self.renderer.character_scene_mut()
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

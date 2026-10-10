//! Ambient effects: bounded stateless steam and shared snow billboards.
//!
//! An effect is presentation-only level content: it never collides, never
//! occludes baked light and is not part of the lighting bake. The neutral side
//! of the renderer resolves one [`EffectScene`] per level from
//! [`LevelDef::effects`](crate::level::LevelDef::effects) and the level's
//! resolved [`MaterialTable`], exactly the way
//! [`build_door_models`](crate::render::common::doors::build_door_models)
//! resolves a door's sheet: the material's decoded image, tiling period and
//! alpha contract. The backend side (`src/render/wgpu/effects.rs`) owns the GPU
//! buffers and pipeline for the billboards this module produces.
//!
//! Optional weather retains only deterministic seeds in [`super::snow::SnowScene`],
//! using this same texture/vertex contract without becoming localized steam.
//!
//! Particle model (steam)
//! --------------
//! A particle has **no per-frame state**. Its world position, size and alpha at
//! absolute animation clock `t` are a pure function of `t`, the emitter's
//! parameters and the particle's own golden-ratio phase
//! ([`particle_pose`]) — the same no-drift trick the floating props use. The
//! scene stores no particles at all: [`EffectScene::update`] only records the
//! clock and reports which emitters it advanced, and
//! [`EffectScene::build_billboards`] evaluates the poses on demand. Two frames
//! at the same clock always produce byte-identical vertices, and the motion
//! cannot accumulate error at any frame rate.
//!
//! Bounds
//! ------
//! Every particle's centre stays inside the emitter's own volume: `x` within
//! `width / 2` (plus `drift`), `z` within `depth / 2` (plus `drift`) and `y`
//! between the plume base and the plume base plus `height`. All arithmetic
//! sanitises malformed inputs, so a degenerate emitter yields a finite
//! (usually invisible) quad rather than a `NaN` vertex.
//!
//! Budget
//! ------
//! A valid level can author at most
//! [`MAX_LEVEL_EFFECTS`](crate::level::MAX_LEVEL_EFFECTS) emitters of at most
//! [`MAX_EFFECT_PARTICLES`](crate::level::MAX_EFFECT_PARTICLES) particles, so
//! [`MAX_EFFECT_PARTICLES_PER_LEVEL`] is the exact worst case: 8192 particles,
//! [`MAX_EFFECT_VERTICES_PER_LEVEL`] (32768) vertices and
//! [`MAX_EFFECT_INDICES_PER_LEVEL`] (49152) indices in one draw family. The
//! scene never grows its emitter or texture lists after [`EffectScene::build`],
//! and [`EffectScene::build_billboards`] writes only into the caller's
//! pre-reserved buffer. A level with no effects costs nothing.
//!
//! Draw order
//! ----------
//! Billboards are emitted in deterministic emitter order, grouped by material
//! so each distinct steam material is one contiguous draw. They are *not*
//! sorted back to front: steam is a soft, low-opacity plume a few tens of
//! centimetres deep, and the pass depth-tests (`LessEqual`) without depth
//! writes, so an order error between two overlapping puffs is far below the
//! visible threshold. Sorting an 8192-particle list every frame would cost
//! more than it could show.

use glam::Vec3;

use crate::level::{
    EFFECT_KIND_STEAM, EffectDef, LevelDef, LevelSurfaces, MAX_EFFECT_PARTICLES, MAX_LEVEL_EFFECTS,
};
use crate::materials::{MaterialAlpha, MaterialTable, ResolvedTexture};

/// Particles one level's effect scene can draw, at the level schema's own
/// maximum: [`MAX_LEVEL_EFFECTS`] emitters times [`MAX_EFFECT_PARTICLES`].
#[expect(
    clippy::as_conversions,
    reason = "The schema particle cap is 128 and fits every supported usize; TryFrom is unavailable in a Rust 1.99 const initializer."
)]
pub const MAX_EFFECT_PARTICLES_PER_LEVEL: usize =
    MAX_LEVEL_EFFECTS.saturating_mul(MAX_EFFECT_PARTICLES as usize);

/// Vertices one particle's billboard costs.
pub const VERTS_PER_PARTICLE: usize = 4;

/// Indices one particle's billboard costs.
pub const INDICES_PER_PARTICLE: usize = 6;

/// The scene's documented per-frame vertex budget: 32768 vertices.
pub const MAX_EFFECT_VERTICES_PER_LEVEL: usize =
    MAX_EFFECT_PARTICLES_PER_LEVEL.saturating_mul(VERTS_PER_PARTICLE);

/// The scene's documented per-frame index budget: 49152 indices.
pub const MAX_EFFECT_INDICES_PER_LEVEL: usize =
    MAX_EFFECT_PARTICLES_PER_LEVEL.saturating_mul(INDICES_PER_PARTICLE);

/// Largest UV span one billboard samples.
///
/// A particle's UVs run `0..size / tile_metres`; a tiny authored tile with a
/// huge particle could otherwise repeat the sheet hundreds of times and
/// alias. The cap is generous for real artwork and keeps the span finite.
const MAX_UV_SPAN: f32 = 8.0;

/// Golden-ratio conjugate `(sqrt(5) - 1) / 2`, the low-discrepancy phase step
/// the floating props also use.
const GOLDEN_RATIO_CONJUGATE: f32 = 0.618_034;

/// Two further low-discrepancy constants, used to decorrelate the horizontal
/// spawn phases and the drift phase from the life phase.
const PLASTIC_RATIO: f32 = 0.754_877_6;
const SILVER_RATIO: f32 = 0.569_840_3;
const PHASE_RATIO: f32 = 0.381_966;

/// Fraction of a particle's life spent fading in and, again, fading out. With
/// `0.18` the alpha is zero at both ends and full across the middle.
const FADE_FRACTION: f32 = 0.18;

/// Horizontal wander rate, radians per second.
const DRIFT_RATE: f32 = 0.65;

/// How much larger a particle is at the end of its life than at birth.
const GROWTH: f32 = 0.35;

/// One phase step between consecutive emitters: a fresh block of golden-ratio
/// phases, so two identical emitters never pulse in lockstep.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "`MAX_EFFECT_PARTICLES` is 128, exactly representable"
)] // `MAX_EFFECT_PARTICLES` is 128, exactly representable
const EMITTER_PHASE_STRIDE: f32 = MAX_EFFECT_PARTICLES as f32;

/// One particle's evaluated pose at the scene clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectPose {
    /// World-space centre of the billboard.
    pub position: [f32; 3],
    /// Billboard edge length in metres.
    pub size: f32,
    /// Alpha multiplier, `0.0..=1.0`; already includes the material opacity.
    pub alpha: f32,
}

/// One resolved effect material.
///
/// The material is resolved through the level's [`MaterialTable`] exactly like
/// a door material: its decoded image and cache identity
/// (`texture: ResolvedTexture`), its tiling period and its alpha contract.
#[derive(Clone, Debug)]
pub struct EffectTexture {
    /// The resolved texture: GPU cache identity, origin and decoded image.
    pub texture: ResolvedTexture,
    /// World metres covered by one repeat of the sheet.
    pub tile_metres: f32,
    /// The material's alpha contract; only its opacity is used (the effect
    /// pass is always a blended billboard pass, never cut-out).
    pub alpha: MaterialAlpha,
}

impl EffectTexture {
    /// The alpha multiplier a particle using this material starts from.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        if self.alpha.opacity.is_finite() {
            self.alpha.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

/// One resolved emitter: a plume footprint, its budget and its material slot.
#[derive(Clone, Debug)]
pub struct EffectEmitter {
    /// The authored index of this emitter in the level's `effects` array.
    ///
    /// Emitters are sorted by material after resolution, so the authored index
    /// is the stable handle a `steam` component uses to enable or disable one.
    pub authored_index: usize,
    /// False suppresses this emitter's billboards without removing it.
    pub enabled: bool,
    /// World position of the plume base: the walkable floor under `(x, z)`
    /// plus the authored `y` offset.
    pub base: [f32; 3],
    /// Footprint extent along X, in metres.
    pub width: f32,
    /// Footprint extent along Z, in metres.
    pub depth: f32,
    /// Plume height above the base, in metres.
    pub height: f32,
    /// Live particles, from `1` to `MAX_EFFECT_PARTICLES`.
    pub count: usize,
    /// Particle billboard size at birth, in metres.
    pub size: f32,
    /// Horizontal wander amplitude, in metres.
    pub drift: f32,
    /// Seconds one particle takes to cross the plume.
    pub lifetime_seconds: f32,
    /// Particle alpha multiplier from the material's opacity.
    pub opacity: f32,
    /// World metres covered by one sheet repeat.
    pub tile_metres: f32,
    /// Index into [`EffectScene::textures`].
    pub texture: usize,
    /// Golden-ratio seed; assigned after emitters are grouped by material.
    pub seed: f32,
}

/// One contiguous draw range: every particle that shares one texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectDrawGroup {
    /// Index into [`EffectScene::textures`].
    pub texture: usize,
    /// Particles in this group.
    pub particles: usize,
}

impl EffectDrawGroup {
    /// Vertices this group's particles occupy.
    #[must_use]
    pub const fn vertex_count(self) -> usize {
        self.particles.saturating_mul(VERTS_PER_PARTICLE)
    }

    /// Indices this group's particles occupy.
    #[must_use]
    pub const fn index_count(self) -> usize {
        self.particles.saturating_mul(INDICES_PER_PARTICLE)
    }
}

/// One billboard vertex: world position, colour/alpha and UV.
///
/// The layout is byte-identical to the wgpu pipeline's vertex buffer layout:
/// `position` at offset 0, `color` at 12, `uv` at 28, stride 36.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EffectVertex {
    /// World-space position.
    pub position: [f32; 3],
    /// Linear colour and alpha (`rgb` multiplies the sheet, `a` its alpha).
    pub color: [f32; 4],
    /// Sheet UV.
    pub uv: [f32; 2],
}

/// What one [`EffectScene::update`] pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectUpdate {
    /// One bit per emitter: bit `i` is set when emitter `i` advanced this
    /// pass. The scene holds at most [`MAX_LEVEL_EFFECTS`] (64) emitters, so a
    /// `u64` addresses every one exactly.
    pub moved: u64,
}

/// The level's resolved effect emitters and materials.
///
/// Built once per level; the emitter, snow seed and texture lists never grow
/// afterwards. Poses are pure functions of the clock and camera volume.
#[derive(Debug)]
pub struct EffectScene {
    emitters: Vec<EffectEmitter>,
    textures: Vec<EffectTexture>,
    groups: Vec<EffectDrawGroup>,
    clock: f32,
    snow: Option<super::snow::SnowScene>,
    alternate_snow: Option<super::snow::SnowScene>,
    weather: crate::weather::WeatherPlayback,
    weather_blendable: bool,
}

impl Default for EffectScene {
    /// An empty scene whose clock has no value yet.
    ///
    /// The first [`EffectScene::update`] therefore always reports every live
    /// emitter as advanced, even when it is called with `0.0`; a derived
    /// `0.0` default would make the first frame look like a re-evaluation of
    /// an already-established clock.
    fn default() -> Self {
        Self {
            emitters: Vec::new(),
            textures: Vec::new(),
            groups: Vec::new(),
            clock: f32::NAN,
            snow: None,
            alternate_snow: None,
            weather: crate::weather::WeatherPlayback::default(),
            weather_blendable: false,
        }
    }
}

impl EffectScene {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves every authored effect in `level` against `materials`.
    ///
    /// A non-steam kind is skipped (the loader rejects it before a level
    /// ships), and an emitter whose material has no decoded image is skipped
    /// with one warning, exactly like a door with an unresolved material.
    /// Particle budgets are clamped to the level-wide cap so a scene built from
    /// unvalidated data still cannot exceed the GPU buffers the renderer sizes
    /// for it.
    #[must_use]
    pub fn build(level: &LevelDef, materials: &MaterialTable) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut scene = Self::default();
        let mut remaining = MAX_EFFECT_PARTICLES_PER_LEVEL;
        for (authored_index, def) in level.effects.iter().enumerate() {
            if scene.emitters.len() >= MAX_LEVEL_EFFECTS || remaining == 0 {
                break;
            }
            if !def.kind.eq_ignore_ascii_case(EFFECT_KIND_STEAM) {
                continue;
            }
            let material_id = def
                .material
                .as_deref()
                .unwrap_or(crate::level::DEFAULT_STEAM_MATERIAL);
            let Some(material) =
                resolve_effect_material(&mut scene.textures, materials, material_id)
            else {
                crate::logging::warn(format!(
                    "[effects] emitter `{}` material `{material_id}` has no decoded texture; it will not draw",
                    def.id.as_deref().unwrap_or("(unnamed)")
                ));
                continue;
            };
            let Some(mut emitter) = build_emitter(def, &surfaces, material, remaining) else {
                continue;
            };
            emitter.authored_index = authored_index;
            emitter.enabled = def.enabled;
            remaining = remaining.saturating_sub(emitter.count);
            scene.emitters.push(emitter);
        }
        // Grouping emitters by material keeps each distinct sheet in one
        // contiguous vertex range, so the encode is one draw per material.
        scene.emitters.sort_by_key(|emitter| emitter.texture);
        for (index, emitter) in scene.emitters.iter_mut().enumerate() {
            emitter.seed = index_to_f32(index) * EMITTER_PHASE_STRIDE;
        }
        scene.rebuild_groups();
        let disabled = std::env::var("PLACES_BENCH").as_deref() == Ok("1")
            && std::env::var("PLACES_BENCH_WEATHER_OFF").as_deref() == Ok("1");
        if !disabled && let Some(weather) = &level.weather {
            scene.snow = scene.resolve_snow(level, weather, materials);
            if scene.snow.is_some()
                && let Some(alternate) = &level.weather_alternate
            {
                scene.alternate_snow = scene.resolve_snow(level, alternate, materials);
                scene.weather_blendable = crate::weather::strength_compatible(weather, alternate);
            }
        }
        scene.weather = crate::weather::WeatherPlayback::new(level.weather_cycle);
        scene
    }

    fn resolve_snow(
        &mut self,
        level: &LevelDef,
        weather: &crate::weather::WeatherDef,
        materials: &MaterialTable,
    ) -> Option<super::snow::SnowScene> {
        let config = weather.snowfall();
        config.validate().ok()?;
        let material =
            resolve_effect_material(&mut self.textures, materials, config.material.trim())?;
        Some(super::snow::SnowScene::build(
            level,
            config,
            material.slot,
            material.opacity,
        ))
    }

    /// Number of emitters.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.emitters.len()
    }

    /// True when the level authors no drawable effect.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.emitters.is_empty() && self.snow.is_none()
    }

    /// The resolved emitters, in material-grouped order.
    #[must_use]
    pub fn emitters(&self) -> &[EffectEmitter] {
        &self.emitters
    }

    /// Enables or disables the emitter authored at `authored_index`.
    ///
    /// Returns whether the state changed; an index no emitters carry (an
    /// effect whose material failed to resolve) returns false. The next
    /// [`Self::build_billboards`] pass simply omits a disabled emitter.
    pub fn set_enabled(&mut self, authored_index: usize, enabled: bool) -> bool {
        let Some(emitter) = self
            .emitters
            .iter_mut()
            .find(|emitter| emitter.authored_index == authored_index)
        else {
            return false;
        };
        if emitter.enabled == enabled {
            return false;
        }
        emitter.enabled = enabled;
        self.rebuild_groups();
        true
    }

    /// The distinct effect materials, in first-reference order.
    #[must_use]
    pub fn textures(&self) -> &[EffectTexture] {
        &self.textures
    }

    /// The contiguous draw ranges, one per distinct material in use.
    #[must_use]
    pub fn draw_groups(&self) -> &[EffectDrawGroup] {
        &self.groups
    }

    /// All possible steam and snow particles, including disabled emitters.
    #[must_use]
    pub fn buffer_particles(&self) -> usize {
        let snow_budget = self
            .snow
            .as_ref()
            .map_or(0, super::snow::SnowScene::budget)
            .max(
                self.alternate_snow
                    .as_ref()
                    .map_or(0, super::snow::SnowScene::budget),
            );
        self.emitters.iter().fold(snow_budget, |total, emitter| {
            total.saturating_add(emitter.count)
        })
    }

    #[must_use]
    pub const fn snow(&self) -> Option<&super::snow::SnowScene> {
        if !self.weather_blendable && self.weather.strength() >= 1.0 {
            self.alternate_snow.as_ref()
        } else {
            self.snow.as_ref()
        }
    }

    /// Selects a prepared weather scene without rebuilding seeds or resources.
    /// The scene itself survives live quality changes, including this selection.
    pub fn set_weather_alternate(&mut self, alternate: bool) -> bool {
        if self.alternate_snow.is_none() {
            return false;
        }
        let strength: f32 = if alternate { 1.0 } else { 0.0 };
        let changed = self.weather.strength().to_bits() != strength.to_bits()
            || self.weather.target().to_bits() != strength.to_bits()
            || self.weather.cycle_enabled();
        let _selected = self.weather.set_strength(strength, 0.0);
        self.blend_weather();
        changed
    }

    pub fn set_weather_strength(&mut self, strength: f32, transition_seconds: f32) -> bool {
        if !self.weather_blendable || !self.weather.set_strength(strength, transition_seconds) {
            return false;
        }
        self.blend_weather();
        true
    }

    pub fn set_weather_cycle(&mut self, enabled: bool) -> bool {
        self.weather_blendable && self.weather.set_cycle(enabled)
    }

    /// Weather playback follows playing simulation time, including pauses.
    pub fn update_weather(&mut self, delta_seconds: f32) {
        self.weather.advance(delta_seconds);
        self.blend_weather();
    }

    fn blend_weather(&mut self) {
        if self.weather_blendable
            && let (Some(snow), Some(alternate)) = (&mut self.snow, &self.alternate_snow)
        {
            snow.apply_strength(alternate, self.weather.strength(), self.clock);
        }
    }

    #[must_use]
    pub const fn weather_state(&self) -> (f32, f32, bool) {
        (
            self.weather.strength(),
            self.weather.target(),
            self.weather.cycle_enabled(),
        )
    }

    #[must_use]
    pub const fn weather_control_seconds(&self) -> f32 {
        self.weather.control_seconds()
    }

    #[must_use]
    pub const fn clock(&self) -> f32 {
        if self.clock.is_finite() {
            self.clock
        } else {
            0.0
        }
    }

    /// Total live particles across every enabled emitter.
    #[must_use]
    pub fn particle_count(&self) -> usize {
        self.emitters
            .iter()
            .filter(|emitter| emitter.enabled)
            .fold(0_usize, |total, emitter| {
                total.saturating_add(emitter.count)
            })
    }

    /// Total vertices one [`Self::build_billboards`] pass writes.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.particle_count().saturating_mul(VERTS_PER_PARTICLE)
    }

    /// Empties the emitters but keeps the resolved materials, mirroring
    /// [`DynamicScene::clear`](crate::render::common::dynamic::DynamicScene::clear).
    ///
    /// The engine rebuilds a scene wholesale on every level change
    /// ([`Self::clear_all`]); this half of the pair stays for the tests that
    /// exercise "emitters gone, textures kept" independently of the loader.
    #[cfg(test)]
    pub fn clear(&mut self) {
        if self.is_empty() {
            return;
        }
        self.emitters.clear();
        self.snow = None;
        self.alternate_snow = None;
        self.weather = crate::weather::WeatherPlayback::default();
        self.weather_blendable = false;
        self.groups.clear();
    }

    /// Drops the emitters and every resolved material. Called when a level is
    /// replaced, mirroring
    /// [`DynamicScene::clear_all`](crate::render::common::dynamic::DynamicScene::clear_all).
    pub fn clear_all(&mut self) {
        self.emitters.clear();
        self.snow = None;
        self.alternate_snow = None;
        self.weather = crate::weather::WeatherPlayback::default();
        self.weather_blendable = false;
        self.textures.clear();
        self.groups.clear();
    }

    /// Records the absolute animation clock and reports which emitters moved.
    ///
    /// Stateless by construction: the emitters' poses are a pure function of
    /// the clock, so this only compares the new clock with the previous one.
    /// Every emitter with live particles moved when the clock changed; an
    /// unchanged clock moves nothing.
    pub fn update(&mut self, seconds: f32) -> EffectUpdate {
        let clock = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let changed = clock.to_bits() != self.clock.to_bits();
        self.clock = clock;
        let mut moved = 0_u64;
        if changed {
            for (index, emitter) in self.emitters.iter().enumerate() {
                if emitter.count == 0 {
                    continue;
                }
                if let Some(bit) = 1_u64.checked_shl(u32::try_from(index).unwrap_or(u32::MAX)) {
                    moved |= bit;
                }
            }
        }
        EffectUpdate { moved }
    }

    /// Writes every emitter's camera-facing billboards into `out`.
    ///
    /// Each particle contributes exactly four vertices, in deterministic
    /// emitter and particle order; the caller's fixed index pattern addresses
    /// particle `n` as vertices `4n..4n + 3`. The quads are in world space and
    /// face `camera_position`; the return value is the number of particles
    /// written. `out` is cleared first and never grows past the level budget.
    #[must_use]
    pub fn build_billboards(
        &self,
        camera_position: [f32; 3],
        out: &mut Vec<EffectVertex>,
    ) -> usize {
        out.clear();
        out.reserve(self.vertex_count());
        let camera = source_position(camera_position);
        let mut written = 0_usize;
        for emitter in &self.emitters {
            if !emitter.enabled {
                continue;
            }
            for particle in 0..emitter.count {
                let pose = particle_pose(emitter, self.clock, particle);
                let tile = positive_or(emitter.tile_metres, 1.0);
                let span = (pose.size / tile).clamp(0.0, MAX_UV_SPAN);
                append_billboard(camera, pose, span, out);
                written = written.saturating_add(1);
            }
        }
        written
    }

    /// Rebuilds the contiguous per-material draw ranges from the (already
    /// material-grouped) emitter list.
    fn rebuild_groups(&mut self) {
        self.groups.clear();
        for emitter in &self.emitters {
            if !emitter.enabled || emitter.count == 0 {
                continue;
            }
            match self.groups.last_mut() {
                Some(group) if group.texture == emitter.texture => {
                    group.particles = group.particles.saturating_add(emitter.count);
                }
                _ => self.groups.push(EffectDrawGroup {
                    texture: emitter.texture,
                    particles: emitter.count,
                }),
            }
        }
    }
}

/// Shared world-space billboard geometry for steam and weather.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Bounded finite billboard geometry"
)]
pub(super) fn append_billboard(
    camera: Vec3,
    pose: EffectPose,
    span: f32,
    out: &mut Vec<EffectVertex>,
) {
    let (right, up) = billboard_basis(camera, pose.position);
    let half = pose.size * 0.5;
    let centre = Vec3::from_array(pose.position);
    for (corner, uv) in [
        (centre - right * half - up * half, [0.0, span]),
        (centre + right * half - up * half, [span, span]),
        (centre + right * half + up * half, [span, 0.0]),
        (centre - right * half + up * half, [0.0, 0.0]),
    ] {
        out.push(EffectVertex {
            position: corner.to_array(),
            color: [1.0, 1.0, 1.0, pose.alpha.clamp(0.0, 1.0)],
            uv,
        });
    }
}

/// Resolves one effect material into the scene's material list, deduplicating
/// by texture key, and returns the resolved values for one emitter.
fn resolve_effect_material(
    textures: &mut Vec<EffectTexture>,
    materials: &MaterialTable,
    material_id: &str,
) -> Option<ResolvedEffectMaterial> {
    let entry = materials.entry_of(material_id)?;
    // A material without a decoded image cannot draw; this mirrors
    // `build_door_models`, whose `material_image` requires the image.
    let _configured_as_ref = entry.image.as_ref()?;
    let texture = materials
        .textures()
        .get(usize::try_from(entry.texture_index).unwrap_or(usize::MAX))?
        .clone();
    let slot = if let Some(index) = textures
        .iter()
        .position(|existing| existing.texture.key == texture.key)
    {
        index
    } else {
        textures.push(EffectTexture {
            tile_metres: positive_or(entry.tile_metres, 1.0),
            alpha: entry.alpha.sanitized(),
            texture,
        });
        textures.len().checked_sub(1)?
    };
    let resolved = textures.get(slot)?;
    Some(ResolvedEffectMaterial {
        slot,
        tile_metres: resolved.tile_metres,
        opacity: resolved.opacity(),
    })
}

/// The resolved material facts one emitter copies: its texture slot, tiling
/// and alpha multiplier.
#[derive(Clone, Copy, Debug)]
struct ResolvedEffectMaterial {
    slot: usize,
    tile_metres: f32,
    opacity: f32,
}

/// One authored emitter resolved against the level's walkable floor.
fn build_emitter(
    def: &EffectDef,
    surfaces: &LevelSurfaces<'_>,
    material: ResolvedEffectMaterial,
    budget: usize,
) -> Option<EffectEmitter> {
    let count = usize::try_from(def.count)
        .unwrap_or(0)
        .min(usize::try_from(MAX_EFFECT_PARTICLES).unwrap_or(usize::MAX))
        .min(budget);
    if count == 0 {
        return None;
    }
    let base_y = surfaces.floor_y_at(def.x, def.z).unwrap_or(0.0) + def.y;
    Some(EffectEmitter {
        authored_index: 0,
        enabled: true,
        base: [
            finite_or(def.x, 0.0),
            finite_or(base_y, 0.0),
            finite_or(def.z, 0.0),
        ],
        width: positive_or(def.width, 1.0),
        depth: positive_or(def.depth, 1.0),
        height: positive_or(def.height, 1.0),
        count,
        size: positive_or(def.size, 0.0),
        drift: non_negative_or(def.drift, 0.0),
        lifetime_seconds: positive_or(def.lifetime_seconds, 1.0),
        opacity: material.opacity,
        tile_metres: material.tile_metres,
        texture: material.slot,
        // Assigned after the material grouping sort; a placeholder keeps the
        // struct complete until then.
        seed: 0.0,
    })
}

/// The particle's pose at absolute `seconds`: a pure function of the clock,
/// the emitter's authored parameters and the particle's own golden-ratio
/// phase.
///
/// Every input is sanitised, so a malformed emitter (a `NaN` base, a zero
/// lifetime, a negative size) still yields finite values. The centre is bounded
/// by the footprint plus `drift` horizontally and by `height` vertically, and
/// the alpha includes the material's opacity.
#[must_use]
// bounded particle arithmetic on sanitised f32 inputs
pub fn particle_pose(emitter: &EffectEmitter, seconds: f32, particle: usize) -> EffectPose {
    let clock = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let lifetime = positive_or(emitter.lifetime_seconds, 1.0);
    let width = non_negative_or(emitter.width, 0.0);
    let depth = non_negative_or(emitter.depth, 0.0);
    let height = non_negative_or(emitter.height, 0.0);
    let drift = non_negative_or(emitter.drift, 0.0);
    let size = non_negative_or(emitter.size, 0.0);
    let opacity = if emitter.opacity.is_finite() {
        emitter.opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let [base_x, base_y, base_z] = emitter.base.map(|value| finite_or(value, 0.0));

    let seed = finite_or(emitter.seed, 0.0) + index_to_f32(particle);
    let life_phase = (seed * GOLDEN_RATIO_CONJUGATE).fract();
    let x_phase = (seed * PLASTIC_RATIO).fract();
    let z_phase = (seed * SILVER_RATIO).fract();
    let drift_phase = (seed * PHASE_RATIO).fract();

    // `fract` keeps the life in `[0, 1)`: the particle is reborn exactly one
    // lifetime after the previous birth.
    let life = (clock / lifetime + life_phase).fract();
    let alpha = fade(life, FADE_FRACTION) * opacity;
    // The multi-term sums are evaluated with `mul_add`: fewer roundings, one
    // fused operation, and the same deterministic result on every platform
    // that implements IEEE 754 fused multiply-add (every wgpu target does).
    let wobble_x = clock
        .mul_add(DRIFT_RATE, drift_phase * std::f32::consts::TAU)
        .sin();
    let wobble_z = clock
        .mul_add(DRIFT_RATE * 0.83, drift_phase * 1.7 * std::f32::consts::TAU)
        .cos();

    EffectPose {
        position: [
            wobble_x.mul_add(drift, (x_phase - 0.5).mul_add(width, base_x)),
            life.mul_add(height, base_y),
            wobble_z.mul_add(drift, (z_phase - 0.5).mul_add(depth, base_z)),
        ],
        size: GROWTH.mul_add(life, 1.0) * size,
        alpha: alpha.clamp(0.0, 1.0),
    }
}

/// The fade envelope over a particle's life: zero at `life = 0` and at
/// `life = 1`, one across the middle.
#[must_use]
// bounded f32 envelope arithmetic
const fn fade(life: f32, fade_fraction: f32) -> f32 {
    smooth01(life / fade_fraction) * smooth01((1.0 - life) / fade_fraction)
}

/// A Hermite ramp clamped to `[0, 1]`.
#[must_use]
// bounded f32 polynomial
const fn smooth01(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The camera-facing `(right, up)` basis of a billboard at `particle`.
///
/// The quad faces the camera; a camera exactly above or below the particle
/// (where the usual cross product degenerates) falls back to a fixed
/// horizontal pair, so the quad is always finite and perpendicular to the
/// view.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "one bounded vector subtraction"
)] // one bounded vector subtraction
fn billboard_basis(camera_position: Vec3, particle: [f32; 3]) -> (Vec3, Vec3) {
    let forward = (camera_position - Vec3::from_array(particle)).normalize_or_zero();
    let right = forward.cross(Vec3::Y);
    if right.length_squared() > 1.0e-8 {
        let unit_right = right.normalize();
        (unit_right, unit_right.cross(forward))
    } else {
        (Vec3::X, Vec3::Z)
    }
}

/// A finite value, or `fallback` when it is not finite.
#[must_use]
const fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// A finite, strictly positive value, or `fallback`.
#[must_use]
const fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

/// A finite, non-negative value, or `fallback`.
#[must_use]
const fn non_negative_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

/// A finite world-space camera position; a malformed one becomes the origin.
#[must_use]
fn source_position(position: [f32; 3]) -> Vec3 {
    Vec3::from_array(position.map(|value| finite_or(value, 0.0)))
}

/// Converts a bounded count to `f32`.
///
/// Every index this module converts is a particle or emitter index below 2^24,
/// where the conversion is exact.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "indices are below 2^24, where f32 is exact"
)] // indices are below 2^24, where f32 is exact
const fn index_to_f32(value: usize) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing, loose casts and permissive arithmetic
    // are idiomatic in tests; the production lints stay enforced everywhere
    // else in the crate.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::assets::AssetCatalog;
    use serde_json::json;

    fn level_with_effects(effects: &[serde_json::Value]) -> LevelDef {
        let document = json!({
            "format_version": 3_i32,
            "id": "effects_test",
            "name": "Effects Test",
            "spawn": { "x": 1.0_f64, "z": 1.0_f64 },
            "rooms": [ { "x": 0.0_f64, "z": 0.0_f64, "width": 6.0_f64, "depth": 6.0_f64, "height": 2.7_f64 } ],
            "effects": effects
        });
        LevelDef::from_json(&document.to_string()).expect("test level parses")
    }

    fn steam_effect(x: f32, z: f32) -> serde_json::Value {
        json!({
            "kind": "steam",
            "x": x,
            "y": 0.0,
            "z": z,
            "width": 1.0,
            "depth": 0.8,
            "height": 1.5,
            "count": 24,
            "size": 0.35,
            "drift": 0.2,
            "lifetime_seconds": 3.0
        })
    }

    fn materials(level: &LevelDef) -> MaterialTable {
        let catalog = AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let mut cache = crate::materials::TextureCache::new();
        crate::materials::resolve_materials(level, &catalog, None, Some(&root), &mut cache)
    }

    fn scene(effects: &[serde_json::Value]) -> EffectScene {
        let level = level_with_effects(effects);
        let materials = materials(&level);
        EffectScene::build(&level, &materials)
    }

    fn one_emitter() -> EffectScene {
        scene(&[steam_effect(3.0, 3.0)])
    }

    fn weather_scene() -> EffectScene {
        let mut level = level_with_effects(&[]);
        level.weather = Some(crate::weather::WeatherDef::Snow(
            crate::weather::SnowfallDef {
                count: 64,
                wind: [0.3, -0.1],
                opacity: 0.6,
                ..crate::weather::SnowfallDef::default()
            },
        ));
        level.weather_alternate = Some(crate::weather::WeatherDef::Snow(
            crate::weather::SnowfallDef {
                count: 2048,
                ..crate::weather::SnowfallDef::blizzard()
            },
        ));
        let materials = materials(&level);
        let mut scene = EffectScene::build(&level, &materials);
        let _clock_changed = scene.update(2.0);
        scene
    }

    fn graded_weather_scene() -> EffectScene {
        let mut level = level_with_effects(&[]);
        level.weather = Some(crate::weather::WeatherDef::Snow(
            crate::weather::SnowfallDef {
                count: 2048,
                ..crate::weather::SnowfallDef::default()
            },
        ));
        level.weather_alternate = Some(crate::weather::WeatherDef::Snow(
            crate::weather::SnowfallDef {
                count: 2048,
                ..crate::weather::SnowfallDef::blizzard()
            },
        ));
        let materials = materials(&level);
        let mut scene = EffectScene::build(&level, &materials);
        let _clock_changed = scene.update(2.0);
        scene
    }

    fn append_weather(
        scene: &EffectScene,
        quality: crate::quality::QualityLevel,
        out: &mut Vec<EffectVertex>,
    ) -> super::super::snow::SnowStats {
        let camera = Vec3::new(12.0, 1.6, 3.0);
        let projection = glam::Mat4::perspective_rh(60_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0)
            * glam::Mat4::look_at_rh(camera, Vec3::new(12.0, 1.6, 2.0), Vec3::Y);
        let frustum = crate::spatial::Frustum::from_view_projection(
            &projection,
            crate::spatial::DepthRange::ZeroToOne,
        );
        out.clear();
        scene.snow().expect("weather prepared").append(
            scene.clock(),
            camera,
            Some(projection),
            &frustum,
            quality,
            out,
        )
    }

    #[test]
    fn weather_selection_changes_sky_and_extinction_without_losing_shelters() {
        let mut scene = weather_scene();
        let calm = scene.snow().expect("fresh calm scene");
        let calm_storm = calm.storm;
        let calm_sky = calm.sky_storm();
        assert_eq!(calm_sky[3], 0.0, "fresh scene starts calm");
        assert!(
            scene.set_weather_alternate(true),
            "first press selects severe weather"
        );
        let storm = scene.snow().expect("severe scene prepared");
        assert_eq!(
            storm.sky_storm()[3],
            1.0,
            "the sky uses active severe configuration"
        );
        let indoor = Vec3::new(3.0, 1.6, 3.0);
        assert_eq!(
            storm.storm.transmission(indoor, Vec3::new(5.0, 1.6, 3.0)),
            1.0,
            "the room shelters indoor sightlines"
        );
        assert!(
            storm.storm.transmission(indoor, Vec3::new(12.0, 1.6, 3.0)) < 0.2,
            "exterior distance contributes to whiteout"
        );
        assert!(
            scene.set_weather_alternate(false),
            "second press restores calm"
        );
        let restored = scene.snow().expect("calm retained");
        assert_eq!(restored.storm, calm_storm, "no stale storm extinction");
        assert_eq!(restored.sky_storm(), calm_sky, "no stale sky blend");
    }

    #[test]
    fn weather_selection_restores_exact_calm_and_retains_bounded_resources_at_every_quality() {
        let mut scene = weather_scene();
        let calm_address = std::ptr::from_ref(scene.snow().expect("calm prepared"));
        let material_capacity = scene.textures.capacity();
        let particle_capacity = scene.buffer_particles();
        assert_eq!(
            particle_capacity, 2048,
            "reserve the larger alternate budget once"
        );
        let mut vertices = Vec::with_capacity(particle_capacity.saturating_mul(4));
        let vertex_capacity = vertices.capacity();
        for (quality, expected_prefix) in [
            (crate::quality::QualityLevel::Low, 1024),
            (crate::quality::QualityLevel::Medium, 1536),
            (crate::quality::QualityLevel::High, 2048),
        ] {
            let calm_stats = append_weather(&scene, quality, &mut vertices);
            assert!(
                calm_stats.submitted > 0,
                "fixture compares real calm billboards"
            );
            let calm_vertices = vertices.clone();
            for _ in 0_u8..16 {
                assert!(
                    scene.set_weather_alternate(true),
                    "select prepared alternate"
                );
                assert!(
                    !scene.set_weather_alternate(true),
                    "unchanged selection is idempotent"
                );
                let storm_stats = append_weather(&scene, quality, &mut vertices);
                assert_eq!(
                    storm_stats.evaluated, expected_prefix,
                    "quality uses active seed prefix"
                );
                assert!(scene.set_weather_alternate(false), "restore authored calm");
                assert_eq!(
                    std::ptr::from_ref(scene.snow().expect("calm retained")),
                    calm_address,
                    "restore exact original scene"
                );
                let restored_stats = append_weather(&scene, quality, &mut vertices);
                assert_eq!(
                    restored_stats.evaluated, calm_stats.evaluated,
                    "calm count returns exactly"
                );
                assert_eq!(
                    bytemuck::cast_slice::<EffectVertex, u8>(&vertices),
                    bytemuck::cast_slice::<EffectVertex, u8>(&calm_vertices),
                    "calm wind, sizes, opacity and phase return exactly"
                );
                assert_eq!(vertices.capacity(), vertex_capacity, "scratch never grows");
                assert_eq!(
                    scene.textures.capacity(),
                    material_capacity,
                    "materials never accumulate"
                );
                assert_eq!(
                    scene.buffer_particles(),
                    particle_capacity,
                    "GPU budget remains fixed"
                );
            }
        }
    }

    #[test]
    fn weather_strength_morphs_prepared_values_and_preserves_exact_calm_storage() {
        let mut scene = graded_weather_scene();
        let budget = scene.buffer_particles();
        let textures = scene.textures.len();
        let seeds = scene.snow.as_ref().expect("base prepared").budget();
        let mut vertices = Vec::with_capacity(budget.saturating_mul(4));
        let _baseline_stats =
            append_weather(&scene, crate::quality::QualityLevel::High, &mut vertices);
        let calm_config = serde_json::to_value(scene.snow().expect("base").configuration())
            .expect("authored finite calm config serializes");
        let calm_storm = scene.snow().expect("base").storm;
        let calm_sky = scene.snow().expect("base").sky_storm();
        let capacity = vertices.capacity();
        assert!(scene.set_weather_strength(0.35, 2.0));
        let _moved = scene.update(3.0);
        scene.update_weather(1.0);
        assert!((scene.weather_state().0 - 0.175).abs() < 1.0e-6);
        let snow = scene.snow().expect("active scene");
        assert!((snow.wind()[0] - (8.0_f32 - 0.18).mul_add(0.175, 0.18)).abs() < 1.0e-6);
        assert!((snow.storm.color_density[3] - 0.07).abs() < 1.0e-6);
        assert!((snow.sky_storm()[3] - 0.175_f32.sqrt()).abs() < 1.0e-6);
        assert!(
            snow.storm.count[0] > 0,
            "intermediate weather retains prepared roof planes"
        );
        scene.update_weather(0.0);
        assert!(
            (scene.weather_state().0 - 0.175).abs() < 1.0e-6,
            "paused playback does not advance"
        );
        let _moved_again = scene.update(4.0);
        scene.update_weather(1.0);
        for quality in [
            crate::quality::QualityLevel::Low,
            crate::quality::QualityLevel::Medium,
            crate::quality::QualityLevel::High,
        ] {
            let _stats = append_weather(&scene, quality, &mut vertices);
            assert_eq!(
                scene.weather_state().0.to_bits(),
                0.35_f32.to_bits(),
                "quality selection preserves strength and target"
            );
            assert_eq!(vertices.capacity(), capacity);
        }
        assert!(scene.set_weather_strength(0.0, 2.0));
        let _restoring_motion = scene.update(5.0);
        scene.update_weather(1.0);
        let _restored_motion = scene.update(6.0);
        scene.update_weather(1.0);
        let _restored = append_weather(&scene, crate::quality::QualityLevel::High, &mut vertices);
        assert_eq!(
            serde_json::to_value(scene.snow().expect("base retained").configuration())
                .expect("restored finite calm config serializes"),
            calm_config,
            "all authored numeric calm settings return exactly while motion retains history"
        );
        assert_eq!(scene.snow().expect("base retained").storm, calm_storm);
        assert_eq!(scene.snow().expect("base retained").sky_storm(), calm_sky);
        assert_eq!(scene.snow.as_ref().expect("seeds retained").budget(), seeds);
        assert_eq!(scene.buffer_particles(), budget);
        assert_eq!(scene.textures.len(), textures);
        assert_eq!(vertices.capacity(), capacity);
    }

    #[test]
    fn weather_cycle_keeps_prepared_resources_and_manual_override_across_quality_prefixes() {
        let mut scene = graded_weather_scene();
        scene.weather =
            crate::weather::WeatherPlayback::new(Some(crate::weather::WeatherCycleDef {
                period_seconds: 8.0,
                transition_seconds: 2.0,
                max_strength: 0.45,
                ..crate::weather::WeatherCycleDef::default()
            }));
        let budget = scene.buffer_particles();
        let mut vertices = Vec::with_capacity(budget.saturating_mul(4));
        assert!(scene.set_weather_cycle(true));
        for _ in 0_u8..3 {
            scene.update_weather(1.0);
        }
        assert!((scene.weather_state().0 - 0.225).abs() < 1.0e-6);
        let retained_state = scene.weather_state();
        for quality in [
            crate::quality::QualityLevel::Low,
            crate::quality::QualityLevel::Medium,
            crate::quality::QualityLevel::High,
        ] {
            let _stats = append_weather(&scene, quality, &mut vertices);
            assert_eq!(scene.weather_state(), retained_state);
        }
        assert!(scene.set_weather_strength(0.15, 2.0));
        for _ in 0_u8..32 {
            scene.update_weather(1.0);
        }
        assert_eq!(scene.weather_state().0.to_bits(), 0.15_f32.to_bits());
        assert!(
            !scene.weather_state().2,
            "automatic timer never overrides the manual selection"
        );
        assert_eq!(scene.buffer_particles(), budget);
        assert_eq!(vertices.capacity(), budget.saturating_mul(4));
        assert!(scene.set_weather_cycle(true));
        assert!(scene.set_weather_alternate(false));
        assert_eq!(
            scene.weather_state(),
            (0.0, 0.0, false),
            "legacy toggle also cancels timer"
        );
    }

    // ------------------------------------------------------------- resolution

    #[test]
    fn an_empty_level_produces_nothing() {
        let mut scene = scene(&[]);
        assert!(scene.is_empty());
        assert_eq!(scene.len(), 0);
        assert_eq!(scene.particle_count(), 0);
        assert!(
            scene.draw_groups().is_empty(),
            "scene.draw_groups() must be empty"
        );
        let mut vertices = Vec::new();
        assert_eq!(scene.build_billboards([0.0, 1.6, 0.0], &mut vertices), 0);
        assert!(vertices.is_empty(), "vertices must be empty");
        assert_eq!(scene.update(0.0), EffectUpdate::default());
    }

    #[test]
    fn the_default_steam_material_resolves_with_its_tiling_and_opacity() {
        let scene = one_emitter();
        assert_eq!(scene.textures().len(), 1);
        let texture = &scene.textures()[0];
        assert!(
            texture.texture.key.starts_with("core:tex_steam_01"),
            "the sheet identity carries its content revision: {}",
            texture.texture.key
        );
        assert_eq!(texture.tile_metres, 0.5);
        assert_eq!(texture.alpha.mode, crate::materials::AlphaMode::Blend);
        assert!(texture.opacity() > 0.0 && texture.opacity() < 1.0);
        let emitter = &scene.emitters()[0];
        assert_eq!(emitter.opacity, texture.opacity());
        assert_eq!(emitter.tile_metres, texture.tile_metres);
        // The emitter's base is the walkable floor under it plus the authored
        // `y` offset (0 here, and the room floor is 0).
        assert_eq!(emitter.base, [3.0, 0.0, 3.0]);
    }

    #[test]
    fn emitters_sharing_a_material_form_one_draw_group() {
        let scene = scene(&[steam_effect(1.0, 1.0), steam_effect(2.0, 2.0)]);
        assert_eq!(scene.len(), 2);
        assert_eq!(scene.textures().len(), 1);
        assert_eq!(scene.draw_groups().len(), 1);
        assert_eq!(
            scene.draw_groups()[0].particles,
            scene.particle_count(),
            "one material must be one contiguous draw"
        );
    }

    #[test]
    fn distinct_materials_form_distinct_contiguous_groups() {
        let mut first = steam_effect(1.0, 1.0);
        first["material"] = json!("core:steam_01");
        let mut second = steam_effect(2.0, 2.0);
        second["material"] = json!("core:steam_01");
        let shared = scene(&[first, second]);
        assert_eq!(shared.draw_groups().len(), 1);

        let mut alternate = steam_effect(3.0, 3.0);
        alternate["material"] = json!("core:glass_clear_01");
        let mixed = scene(&[steam_effect(1.0, 1.0), alternate]);
        assert_eq!(mixed.textures().len(), 2);
        assert_eq!(mixed.draw_groups().len(), 2);
        assert_eq!(
            mixed
                .draw_groups()
                .iter()
                .map(|group| group.particles)
                .sum::<usize>(),
            mixed.particle_count()
        );
    }

    #[test]
    fn the_shipped_demo_resolves_its_authored_plumes() {
        let level = LevelDef::from_json(include_str!("../../../assets/levels/places_demo.json"))
            .expect("the shipped demo parses");
        let materials = materials(&level);
        let scene = EffectScene::build(&level, &materials);
        // The demo authors the two sauna plumes first, then the hot tub's
        // gentle surface haze; every one of them resolves.
        assert!(
            scene.len() >= 2,
            "the demo authors at least the two sauna plumes"
        );
        let sauna: Vec<&EffectEmitter> = scene
            .emitters()
            .iter()
            .filter(|emitter| emitter.authored_index < 2)
            .collect();
        assert_eq!(sauna.len(), 2, "the sauna pair is authored first");
        assert!(
            sauna.iter().all(|emitter| !emitter.enabled),
            "the sauna plumes start off until their switch enables them"
        );
        assert_eq!(
            sauna.iter().map(|emitter| emitter.count).sum::<usize>(),
            44,
            "the pair's authored particle budget"
        );
        assert_eq!(scene.textures().len(), 1, "both default to core:steam_01");
        assert_eq!(scene.draw_groups().len(), 1, "one material is one draw");
        assert_eq!(
            scene.draw_groups()[0].particles,
            scene.particle_count(),
            "the enabled plumes form one contiguous draw"
        );
    }

    // ---------------------------------------------------------------- budget

    #[test]
    fn the_scene_never_exceeds_the_level_particle_budget() {
        let effects: Vec<serde_json::Value> = (0..MAX_LEVEL_EFFECTS)
            .map(|index| {
                let mut effect = steam_effect(crate::test_support::exact_f32(index), 0.0);
                effect["count"] = json!(MAX_EFFECT_PARTICLES);
                effect
            })
            .collect();
        let scene = scene(&effects);
        assert_eq!(scene.len(), MAX_LEVEL_EFFECTS);
        assert_eq!(scene.particle_count(), MAX_EFFECT_PARTICLES_PER_LEVEL);
        assert_eq!(scene.vertex_count(), MAX_EFFECT_VERTICES_PER_LEVEL);
        assert_eq!(
            scene.particle_count() * INDICES_PER_PARTICLE,
            MAX_EFFECT_INDICES_PER_LEVEL
        );
        let mut vertices = Vec::new();
        assert_eq!(
            scene.build_billboards([0.0, 1.6, 0.0], &mut vertices),
            MAX_EFFECT_PARTICLES_PER_LEVEL
        );
        assert_eq!(vertices.len(), MAX_EFFECT_VERTICES_PER_LEVEL);
        // The fixed u16 index pattern addresses every vertex.
        assert!(MAX_EFFECT_VERTICES_PER_LEVEL <= usize::from(u16::MAX) + 1);
    }

    #[test]
    fn an_unvalidated_level_still_stays_inside_the_budget() {
        let level = level_with_effects(&[
            json!({ "kind": "steam", "count": 10_000_i32, "width": 1.0_f64, "depth": 1.0_f64, "height": 1.0_f64, "size": 0.3_f64 }),
        ]);
        let materials = materials(&level);
        let scene = EffectScene::build(&level, &materials);
        assert_eq!(scene.len(), 1);
        assert_eq!(
            scene.particle_count(),
            usize::try_from(MAX_EFFECT_PARTICLES).expect("fixture integer fits usize")
        );
    }

    // ----------------------------------------------------------------- bounds

    #[test]
    fn positions_stay_inside_the_emitter_volume() {
        let scene = one_emitter();
        let emitter = &scene.emitters()[0];
        let [base_x, base_y, base_z] = emitter.base;
        let max_x = emitter.width.mul_add(0.5, emitter.drift) + 1.0e-3;
        let max_z = emitter.depth.mul_add(0.5, emitter.drift) + 1.0e-3;
        for step in 0_i32..400_i32 {
            let seconds = crate::test_support::exact_f32(step) * 0.05;
            for particle in 0..emitter.count {
                let pose = particle_pose(emitter, seconds, particle);
                let [x, y, z] = pose.position;
                assert!(
                    (x - base_x).abs() <= max_x,
                    "x escaped the footprint at t={seconds}: {x}"
                );
                assert!(
                    (z - base_z).abs() <= max_z,
                    "z escaped the footprint at t={seconds}: {z}"
                );
                assert!(
                    y >= base_y - 1.0e-3 && y <= base_y + emitter.height + 1.0e-3,
                    "y escaped the plume at t={seconds}: {y}"
                );
                assert!(pose.alpha >= 0.0 && pose.alpha <= 1.0);
                assert!(pose.size >= emitter.size);
            }
        }
    }

    /// The bounds hold at every clock, including with a large drift and a
    /// non-lattice time step: the centre stays inside the authored
    /// width/depth/height box plus the drift, and nothing ever goes non-finite.
    #[test]
    fn drift_keeps_every_centre_inside_the_authored_bounds_at_every_clock() {
        let mut effect = steam_effect(3.0, 3.0);
        effect["drift"] = json!(0.6_f64);
        effect["count"] = json!(MAX_EFFECT_PARTICLES);
        let scene = scene(&[effect]);
        let emitter = &scene.emitters()[0];
        let max_x = emitter.width.mul_add(0.5, emitter.drift) + 1.0e-3;
        let max_z = emitter.depth.mul_add(0.5, emitter.drift) + 1.0e-3;
        let mut seconds = 0.0_f32;
        for _ in 0_i32..1_500_i32 {
            // An irrational-ish step so the sampling never aligns with a
            // particle's life or drift phase.
            seconds += 0.013_717;
            for particle in 0..emitter.count {
                let pose = particle_pose(emitter, seconds, particle);
                let [x, y, z] = pose.position;
                assert!(pose.position.iter().all(|value| value.is_finite()));
                assert!(
                    (x - emitter.base[0]).abs() <= max_x,
                    "x escaped at t={seconds}: {x}"
                );
                assert!(
                    (z - emitter.base[2]).abs() <= max_z,
                    "z escaped at t={seconds}: {z}"
                );
                assert!(
                    y >= emitter.base[1] - 1.0e-3 && y <= emitter.base[1] + emitter.height + 1.0e-3,
                    "y escaped at t={seconds}: {y}"
                );
                assert!(pose.alpha.is_finite() && (0.0..=1.0).contains(&pose.alpha));
                assert!(pose.size.is_finite() && pose.size >= emitter.size);
            }
        }
    }

    /// Disabling an emitter is an instant clear: it draws nothing at every
    /// clock, and re-enabling it draws the same stateless pose the clock
    /// implies — there is no lingering plume to fade out.
    #[test]
    fn a_disabled_emitter_draws_nothing_and_re_enables_at_the_live_clock() {
        let mut scene = one_emitter();
        assert_eq!(scene.particle_count(), scene.emitters()[0].count);
        assert!(scene.vertex_count() > 0);
        assert_eq!(scene.draw_groups().len(), 1);
        let mut vertices = Vec::new();
        let drawn = scene.build_billboards([3.0, 1.6, 3.0], &mut vertices);
        assert!(drawn > 0 && !vertices.is_empty());

        // Disable: nothing draws, no group is submitted, from any clock.
        assert!(scene.set_enabled(0, false));
        assert_eq!(scene.particle_count(), 0);
        assert_eq!(scene.vertex_count(), 0);
        assert!(
            scene.draw_groups().is_empty(),
            "scene.draw_groups() must be empty"
        );
        for clock in [0.0, 1.0, 30.0, 1_000.0] {
            let _update_stats = scene.update(clock);
            let disabled_draws = scene.build_billboards([3.0, 1.6, 3.0], &mut vertices);
            assert_eq!(
                disabled_draws, 0,
                "a disabled emitter draws nothing at {clock}s"
            );
            assert!(
                vertices.is_empty(),
                "a disabled emitter leaves no vertices at {clock}s"
            );
        }
        // The state change is reported once, not per frame.
        assert!(!scene.set_enabled(0, false), "already disabled");
        assert!(!scene.set_enabled(99, true), "no such emitter");

        // Re-enable: the very next evaluation is the live clock's pose, with
        // no leftover particles or fade-in from nowhere.
        assert!(scene.set_enabled(0, true));
        assert_eq!(scene.particle_count(), scene.emitters()[0].count);
        assert_eq!(scene.draw_groups().len(), 1);
        let enabled_draws = scene.build_billboards([3.0, 1.6, 3.0], &mut vertices);
        assert_eq!(
            enabled_draws,
            scene.particle_count(),
            "every particle draws again on the re-enable frame"
        );
        // The four corners of the first billboard average to its centre, which
        // is the live clock's pose: no restart at zero and no fade-in.
        let expected = particle_pose(&scene.emitters()[0], 1_000.0, 0);
        let centre = vertices[..4].iter().fold([0.0_f32; 3], |total, vertex| {
            [
                vertex.position[0].mul_add(0.25, total[0]),
                vertex.position[1].mul_add(0.25, total[1]),
                vertex.position[2].mul_add(0.25, total[2]),
            ]
        });
        for axis in 0..3 {
            assert!(
                (centre[axis] - expected.position[axis]).abs() < 1.0e-5,
                "axis {axis}: {:?} vs {:?}",
                centre,
                expected.position
            );
        }
    }

    #[test]
    fn different_particles_do_not_share_a_phase() {
        let scene = one_emitter();
        let emitter = &scene.emitters()[0];
        let first = particle_pose(emitter, 1.0, 0);
        let second = particle_pose(emitter, 1.0, 1);
        assert_ne!(first.position, second.position);
    }

    // ----------------------------------------------------------- determinism

    #[test]
    fn the_pose_is_a_pure_function_of_the_clock() {
        let scene = one_emitter();
        let emitter = &scene.emitters()[0];
        for (seconds, particle) in [(0.0, 0), (1.234, 7), (3.0, 23), (97.5, 4)] {
            let first = particle_pose(emitter, seconds, particle);
            let second = particle_pose(emitter, seconds, particle);
            assert_eq!(first, second, "the same clock must give the same pose");
        }
        let mut first = Vec::new();
        let mut second = Vec::new();
        let _billboard_stats = scene.build_billboards([2.0, 1.7, 5.0], &mut first);
        let _billboard_stats_2 = scene.build_billboards([2.0, 1.7, 5.0], &mut second);
        assert_eq!(first, second, "the same clock and camera must be identical");
    }

    #[test]
    fn the_clock_advances_the_pose() {
        let scene = one_emitter();
        let emitter = &scene.emitters()[0];
        assert_ne!(
            particle_pose(emitter, 0.0, 0).position,
            particle_pose(emitter, 1.5, 0).position
        );
    }

    #[test]
    fn update_reports_each_live_emitter_once_per_clock_change() {
        let mut scene = one_emitter();
        let first = scene.update(0.0);
        assert_eq!(first.moved, 1, "the first clock records the emitter");
        assert_eq!(first.moved.count_ones(), 1);
        assert_ne!(first.moved, 0);
        assert_eq!(
            scene.update(0.0).moved,
            0,
            "an unchanged clock moves nothing"
        );
        assert_eq!(scene.update(0.5).moved, 1);
        assert_eq!(scene.update(0.5).moved, 0);
    }

    // ------------------------------------------------------------------ fade

    #[test]
    fn alpha_fades_to_zero_at_both_ends_of_the_life() {
        assert_eq!(fade(0.0, FADE_FRACTION), 0.0);
        assert_eq!(fade(1.0, FADE_FRACTION), 0.0);
        assert_eq!(fade(0.5, FADE_FRACTION), 1.0);

        let scene = one_emitter();
        let emitter = &scene.emitters()[0];
        assert_eq!(emitter.lifetime_seconds, 3.0);
        assert_eq!(emitter.seed, 0.0);
        // Particle zero of the first emitter has phase zero, so `t = 0` is
        // exactly the start of its life and `t = lifetime` its wrap point.
        assert_eq!(particle_pose(emitter, 0.0, 0).alpha, 0.0);
        assert_eq!(particle_pose(emitter, 3.0, 0).alpha, 0.0);
        assert!(particle_pose(emitter, 1.5, 0).alpha > 0.0);
    }

    #[test]
    fn a_fully_transparent_material_produces_invisible_particles() {
        let mut effect = steam_effect(1.0, 1.0);
        effect["material"] = json!("core:steam_01");
        let level = level_with_effects(&[effect]);
        let materials = materials(&level);
        let mut scene = EffectScene::build(&level, &materials);
        for emitter in &mut scene.emitters {
            emitter.opacity = 0.0;
        }
        let pose = particle_pose(&scene.emitters[0], 1.5, 0);
        assert_eq!(pose.alpha, 0.0);
    }

    // ------------------------------------------------------------- malformed

    #[test]
    fn a_degenerate_emitter_never_produces_nan() {
        let emitter = EffectEmitter {
            authored_index: 0,
            enabled: true,
            base: [f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
            width: f32::NAN,
            depth: -3.0,
            height: f32::INFINITY,
            count: 4,
            size: f32::INFINITY,
            drift: f32::NAN,
            lifetime_seconds: 0.0,
            opacity: f32::NAN,
            tile_metres: 0.0,
            texture: 0,
            seed: f32::NAN,
        };
        for particle in 0..emitter.count {
            let pose = particle_pose(&emitter, f32::NAN, particle);
            assert!(pose.position.iter().all(|value| value.is_finite()));
            assert!(pose.size.is_finite());
            assert!(pose.alpha.is_finite());
        }
        let scene = EffectScene {
            emitters: vec![emitter],
            textures: Vec::new(),
            groups: Vec::new(),
            clock: f32::NAN,
            snow: None,
            alternate_snow: None,
            weather: crate::weather::WeatherPlayback::default(),
            weather_blendable: false,
        };
        let mut vertices = Vec::new();
        let _billboard_stats =
            scene.build_billboards([f32::NAN, 0.0, f32::INFINITY], &mut vertices);
        assert_eq!(vertices.len(), 4 * 4);
        for vertex in &vertices {
            assert!(vertex.position.iter().all(|value| value.is_finite()));
            assert!(vertex.uv.iter().all(|value| value.is_finite()));
            assert!(vertex.color.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn a_table_without_decoded_images_skips_emitters_instead_of_drawing_them() {
        // A logical-only table (no images) is what a geometry test builds; the
        // scene must skip an emitter it cannot texture instead of drawing a
        // blank sheet.
        let level = level_with_effects(&[steam_effect(1.0, 1.0)]);
        let catalog = AssetCatalog::load_default();
        let logical = crate::materials::MaterialTable::logical(&level, &catalog, None);
        let scene = EffectScene::build(&level, &logical);
        assert!(scene.is_empty());
    }

    #[test]
    fn clear_and_clear_all_release_their_lists() {
        let mut scene = one_emitter();
        assert!(!scene.textures().is_empty());
        scene.clear();
        assert!(scene.is_empty());
        assert!(!scene.textures().is_empty(), "clear keeps the materials");
        scene.clear_all();
        assert!(scene.textures().is_empty());
        assert!(
            scene.draw_groups().is_empty(),
            "scene.draw_groups() must be empty"
        );
    }
}

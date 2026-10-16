//! Ambient effects: bounded stateless steam and shared snow billboards.
//!
//! An effect is presentation-only level content: it never collides, never
//! occludes baked light and is not part of the lighting bake. The neutral side
//! of the renderer resolves one [`EffectScene`] per level from
//! [`LevelDef::effects`](crate::engine::level::LevelDef::effects) and the level's
//! resolved [`MaterialTable`], exactly the way
//! [`build_door_models`](crate::engine::render::common::doors::build_door_models)
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
//! [`MAX_LEVEL_EFFECTS`](crate::engine::level::MAX_LEVEL_EFFECTS) emitters of at most
//! [`MAX_EFFECT_PARTICLES`](crate::engine::level::MAX_EFFECT_PARTICLES) particles, so
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

use crate::engine::level::{
    EFFECT_KIND_STEAM, EffectDef, LevelDef, LevelSurfaces, MAX_EFFECT_PARTICLES, MAX_LEVEL_EFFECTS,
};
use crate::engine::materials::{MaterialAlpha, MaterialTable, ResolvedTexture};

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
    weather: crate::engine::weather::WeatherPlayback,
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
            weather: crate::engine::weather::WeatherPlayback::default(),
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
                .unwrap_or(crate::engine::level::DEFAULT_STEAM_MATERIAL);
            let Some(material) =
                resolve_effect_material(&mut scene.textures, materials, material_id)
            else {
                crate::engine::logging::warn(format!(
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
                scene.weather_blendable =
                    crate::engine::weather::strength_compatible(weather, alternate);
            }
        }
        scene.weather = crate::engine::weather::WeatherPlayback::new(level.weather_cycle);
        scene
    }

    fn resolve_snow(
        &mut self,
        level: &LevelDef,
        weather: &crate::engine::weather::WeatherDef,
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

    /// Drops the emitters and every resolved material. Called when a level is
    /// replaced, mirroring
    /// [`DynamicScene::clear_all`](crate::engine::render::common::dynamic::DynamicScene::clear_all).
    pub fn clear_all(&mut self) {
        self.emitters.clear();
        self.snow = None;
        self.alternate_snow = None;
        self.weather = crate::engine::weather::WeatherPlayback::default();
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
                let mut pose = particle_pose(emitter, self.clock, particle);
                if let Some(weather) = self.snow() {
                    // Like snow, a soft plume loses contrast through the
                    // exterior sightline; sheltered steam stays clear.
                    pose.alpha *= weather
                        .storm
                        .transmission(camera, Vec3::from_array(pose.position));
                }
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

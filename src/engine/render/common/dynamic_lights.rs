//! Renderer-neutral attached lights: the bounded, keyed set a frame's glow
//! cues resolve to.
//!
//! An animated entity may attach a light to a named socket (a carved pumpkin's
//! candle flame, a sheet ghost's core). The character path resolves those cues
//! into [`DynamicLight`] values in world space, and the backend uploads at most
//! [`MAX_DYNAMIC_LIGHTS`] of them once per frame as one bounded uniform array.
//!
//! The set is deliberately not a scene: it is a replace-by-key table whose
//! iteration order is the insertion order, so two frames that produce the same
//! cues upload byte-identical data. A value is refused, never clamped, when it
//! is non-finite or outside its documented range: a NaN position or colour
//! would otherwise spread through the whole additive light term (and a NaN
//! `radius` through the falloff), so the neutral layer keeps the GPU data
//! provably finite. The shader's own falloff is a windowed quadratic limited by
//! `radius`; the set guarantees only that `radius` is finite and positive and
//! that the value can be represented in an `f32` uniform.

use glam::Vec3;

/// A handheld spotlight sharing the attached-light material term. The beam is
/// renderer neutral; gameplay samples the same angular and distance response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flashlight {
    pub position: Vec3,
    pub direction: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub inner_cos: f32,
    pub outer_cos: f32,
}

impl Flashlight {
    #[must_use]
    pub fn is_well_formed(self) -> bool {
        self.position.is_finite()
            && self.direction.is_finite()
            && (self.direction.length_squared() - 1.0).abs() < 0.001
            && self
                .color
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.intensity.is_finite()
            && (0.0..=MAX_DYNAMIC_LIGHT_INTENSITY).contains(&self.intensity)
            && self.range.is_finite()
            && (0.1..=MAX_DYNAMIC_LIGHT_RADIUS).contains(&self.range)
            && self.inner_cos.is_finite()
            && self.outer_cos.is_finite()
            && (0.1..1.0).contains(&self.outer_cos)
            && self.inner_cos > self.outer_cos
            && self.inner_cos <= 1.0
    }

    /// Incident linear light before material cosine and scene visibility.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Validated spotlight inputs use finite glam floating-point vector arithmetic."
    )]
    pub fn illumination(self, point: [f32; 3]) -> f32 {
        if !self.is_well_formed() || !point.iter().all(|value| value.is_finite()) {
            return 0.0;
        }
        let offset = Vec3::from_array(point) - self.position;
        let distance = offset.length();
        if !distance.is_finite() || distance >= self.range || distance <= 0.000_001 {
            return 0.0;
        }
        let angular = ((self.direction.dot(offset / distance) - self.outer_cos)
            / (self.inner_cos - self.outer_cos))
            .clamp(0.0, 1.0);
        let cone = angular * angular * 2.0_f32.mul_add(-angular, 3.0);
        let window = (1.0 - (distance / self.range).powi(4)).max(0.0);
        self.intensity * cone * window * window / distance.mul_add(distance, 1.0)
    }
}

/// Practical sources re-evaluated for movable receivers. The identities are
/// chosen once per compiled field, so movement cannot reorder the live set.
pub const MAX_ENTITY_DIRECT_LIGHTS: usize =
    crate::engine::lighting::probes::MAX_RUNTIME_PROBE_LIGHTS;

/// Select the strongest always-on sources without discarding weaker baked
/// sources. Switchable prepared groups reserve live slots because their direct
/// contribution is absent from the base field. Ties retain source order.
#[must_use]
pub fn select_baked_direct_lights(
    lighting: &crate::engine::lighting::LevelLighting,
    switchable: &[usize],
) -> Vec<u32> {
    let mut candidates = lighting
        .lights()
        .iter()
        .enumerate()
        .filter(|(index, light)| light.is_active() && !switchable.contains(index))
        .filter_map(|(index, light)| {
            let color = light.color();
            let strength = light.intensity() * light.height_factor * (color.r + color.g + color.b);
            Some((u32::try_from(index).ok()?, strength))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let reserved = switchable
        .iter()
        .enumerate()
        .filter(|(slot, index)| {
            lighting.lights().get(**index).is_some()
                && !switchable.iter().take(*slot).any(|other| other == *index)
        })
        .take(MAX_ENTITY_DIRECT_LIGHTS)
        .count();
    candidates.truncate(MAX_ENTITY_DIRECT_LIGHTS.saturating_sub(reserved));
    candidates.sort_unstable_by_key(|candidate| candidate.0);
    candidates
        .into_iter()
        .map(|candidate| candidate.0)
        .collect()
}

/// A soft projected bounds shadow for a movable opaque subject. This is a
/// floor receiver approximation, not a mesh silhouette or shadow map.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ContactShadowUniform {
    pub position_floor: [f32; 4],
    pub half_height: [f32; 4],
    pub incoming: [f32; 4],
}

impl ContactShadowUniform {
    pub const ZERO: Self = Self {
        position_floor: [0.0; 4],
        half_height: [0.0; 4],
        incoming: [0.0; 4],
    };
}

/// Resolve actual transformed bounds against the authored walkable floor.
/// Suspended objects fade their contact term rather than grounding in mid-air.
#[must_use]
pub fn contact_shadow(
    bounds: crate::engine::spatial::Aabb,
    floor_y: f32,
    incoming: [f32; 3],
) -> Option<ContactShadowUniform> {
    let minimum = Vec3::from_array(bounds.min);
    let maximum = Vec3::from_array(bounds.max);
    if !minimum.is_finite() || !maximum.is_finite() || !floor_y.is_finite() {
        return None;
    }
    let half = Vec3::new(
        (maximum.x - minimum.x) * 0.5,
        (maximum.y - minimum.y) * 0.5,
        (maximum.z - minimum.z) * 0.5,
    );
    if !half.is_finite() || half.min_element() <= 0.0 {
        return None;
    }
    let centre = minimum.lerp(maximum, 0.5);
    let direction = Vec3::from_array(incoming).normalize_or(Vec3::Y);
    let grounded = (1.0 - (minimum.y - floor_y).max(0.0) / 0.5).clamp(0.0, 1.0);
    if grounded <= 0.0 {
        return None;
    }
    Some(ContactShadowUniform {
        position_floor: [centre.x, centre.y, centre.z, floor_y],
        half_height: [half.x, half.z, minimum.y, maximum.y],
        incoming: [direction.x, direction.y, direction.z, grounded],
    })
}

/// Most attached lights one frame draws.
///
/// The GPU side is a fixed `array<DynamicLight, 8>`; a ninth key is refused so
/// the binding is complete at every adapter, and the refusal is deterministic
/// (the earlier keys keep their slots).
pub const MAX_DYNAMIC_LIGHTS: usize = 8;

/// Largest accepted intensity, in the shader's additive linear light units.
///
/// The authoring schema caps an authored glow at 8; the renderer's own bound
/// leaves headroom for a future range without letting a typo produce an
/// additively blinding surface.
pub const MAX_DYNAMIC_LIGHT_INTENSITY: f32 = 32.0;

/// Largest accepted radius, in metres.
pub const MAX_DYNAMIC_LIGHT_RADIUS: f32 = 256.0;

/// One attached light the shader sums into its lit term.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicLight {
    /// Stable identity, `"glow:<instance id>"` for a character glow. An
    /// insert with the same key replaces the previous value in place.
    pub key: String,
    /// World-space position, in metres.
    pub position: Vec3,
    /// Linear colour factor, each channel `0..=1`.
    pub color: [f32; 3],
    /// Additive intensity; a light effectively off is removed, not stored.
    pub intensity: f32,
    /// The falloff window's radius, in metres, strictly positive.
    pub radius: f32,
}

impl DynamicLight {
    /// True when every field is finite and inside the shader's usable domain.
    ///
    /// The bounds are the renderer's, not the authoring schema's: they exist so
    /// a malformed cue is refused at the neutral boundary instead of reaching
    /// the uniform. An empty key is refused too: two keyless lights could not
    /// be replaced deterministically.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        !self.key.is_empty()
            && self.position.is_finite()
            && self
                .color
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
            && self.intensity.is_finite()
            && (0.0..=MAX_DYNAMIC_LIGHT_INTENSITY).contains(&self.intensity)
            && self.radius.is_finite()
            && self.radius > 0.0
            && self.radius <= MAX_DYNAMIC_LIGHT_RADIUS
    }
}

/// A bounded, keyed, insertion-ordered table of attached lights.
///
/// One frame's glow cues resolve into this set; the GPU upload reads it in
/// order. Insertion replaces by key (the slot and therefore the iteration
/// position are kept), refuses an ill-formed value, and refuses a *new* key
/// once [`MAX_DYNAMIC_LIGHTS`] are live. Existing keys still replace at the cap,
/// so a full set tracks its own lights rather than freezing in place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DynamicLightSet {
    lights: Vec<DynamicLight>,
}

impl DynamicLightSet {
    /// An empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self { lights: Vec::new() }
    }

    /// Number of live lights, never above [`MAX_DYNAMIC_LIGHTS`].
    #[must_use]
    pub const fn len(&self) -> usize {
        self.lights.len()
    }

    /// True when no light is attached.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// True when the set is at its capacity and a new key would be refused.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.lights.len() >= MAX_DYNAMIC_LIGHTS
    }

    /// The live lights, in insertion order.
    #[must_use]
    pub fn lights(&self) -> &[DynamicLight] {
        &self.lights
    }

    /// The light under `key`, if one is live.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&DynamicLight> {
        self.lights.iter().find(|light| light.key == key)
    }

    /// Inserts or replaces one light; false when the value was refused.
    ///
    /// A refused value leaves the set untouched: an invalid replacement does
    /// not erase the previous valid light with the same key. A new key is
    /// refused once the set is full.
    pub fn insert(&mut self, light: DynamicLight) -> bool {
        if !light.is_well_formed() {
            return false;
        }
        if let Some(slot) = self
            .lights
            .iter_mut()
            .find(|existing| existing.key == light.key)
        {
            *slot = light;
            return true;
        }
        if self.is_full() {
            return false;
        }
        self.lights.push(light);
        true
    }

    /// Removes one key; true when a light was live.
    pub fn remove(&mut self, key: &str) -> bool {
        let Some(index) = self.lights.iter().position(|light| light.key == key) else {
            return false;
        };
        drop(self.lights.remove(index));
        true
    }

    /// Removes every light.
    pub fn clear(&mut self) {
        self.lights.clear();
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and permissive arithmetic are
    // idiomatic here; the production lints stay enforced above.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    /// Protects the gameplay/render beam contract with six scalar samples; no
    /// map, assets, GPU or transport solve is needed.
    #[test]
    fn flashlight_profile_falls_with_distance_has_a_soft_edge_and_rejects_invalid_input() {
        let light = Flashlight {
            position: Vec3::ZERO,
            direction: -Vec3::Z,
            color: [1.0; 3],
            intensity: 20.0,
            range: 24.0,
            inner_cos: 12.0_f32.to_radians().cos(),
            outer_cos: 24.0_f32.to_radians().cos(),
        };
        assert!(light.is_well_formed(), "valid unit-direction beam");
        let point = |angle: f32| (Vec3::new(angle.sin(), 0.0, -angle.cos()) * 5.0).to_array();
        let centre = light.illumination(point(0.0));
        let edge = light.illumination(point(18.0_f32.to_radians()));
        assert!(
            edge > 0.0 && edge < centre,
            "the cone edge must fade continuously"
        );
        assert!(
            light.illumination([0.0, 0.0, -2.0]) > centre,
            "near light must be brighter"
        );
        assert!(
            light.illumination([0.0, 0.0, -12.0]) < centre,
            "distant light must decay"
        );
        assert_eq!(
            light.illumination(point(25.0_f32.to_radians())),
            0.0,
            "outside the cone"
        );
        assert_eq!(
            light.illumination([0.0, 0.0, -24.0]),
            0.0,
            "outside the range"
        );
        assert_eq!(
            light.illumination([f32::NAN, 0.0, 0.0]),
            0.0,
            "non-finite receiver"
        );
        assert!(
            !Flashlight {
                direction: Vec3::ZERO,
                ..light
            }
            .is_well_formed(),
            "unit direction required"
        );
        assert!(
            !Flashlight {
                outer_cos: light.inner_cos,
                ..light
            }
            .is_well_formed(),
            "nonzero edge width required"
        );
    }

    #[test]
    fn practical_selection_is_stable_bounded_and_excludes_switchable_sources() {
        let fixtures = (0_u16..10)
            .map(|index| {
                serde_json::json!({
                    "fixture":"core:fluorescent_panel_01", "x":f32::from(index), "z":0.0_f32,
                    "align":"none", "brightness":0.5_f32 * f32::from(index + 1),
                })
            })
            .collect::<Vec<_>>();
        let level = crate::engine::level::LevelDef::from_json(
            &serde_json::json!({
                "format_version":3_u16,"id":"practical_selection","name":"Practical selection",
                "spawn":{"x":0.0_f32,"z":0.0_f32},
                "rooms":[{"x":-1.0_f32,"z":-1.0_f32,"width":12.0_f32,"depth":3.0_f32,"height":3.0_f32}],
                "ceiling_lights":fixtures,
            })
            .to_string(),
        )
        .expect("selection fixture");
        let lighting = crate::engine::lighting::LevelLighting::bake(&level);
        assert_eq!(
            lighting.lights().first().expect("first source").intensity(),
            0.5
        );
        assert_eq!(
            lighting.lights().last().expect("last source").intensity(),
            5.0
        );
        assert_eq!(
            select_baked_direct_lights(&lighting, &[]),
            (2_u32..10).collect::<Vec<_>>()
        );
        assert_eq!(
            select_baked_direct_lights(&lighting, &[9]),
            (2_u32..9).collect::<Vec<_>>()
        );
        assert_eq!(
            select_baked_direct_lights(&lighting, &[9, 9, usize::MAX]),
            (2_u32..9).collect::<Vec<_>>(),
            "duplicate or missing IDs cannot consume extra reserved slots"
        );
    }

    #[test]
    fn grounding_follows_actual_bounds_floor_and_source_direction() {
        let bounds = crate::engine::spatial::Aabb {
            min: [1.0, 0.0, 2.0],
            max: [3.0, 2.0, 4.0],
        };
        let shadow = contact_shadow(bounds, 0.0, [1.0, 2.0, 0.0]).expect("grounded subject");
        assert_eq!(shadow.position_floor, [2.0, 1.0, 3.0, 0.0]);
        assert_eq!(shadow.half_height, [1.0, 1.0, 0.0, 2.0]);
        assert!(shadow.incoming[0] > 0.0 && shadow.incoming[1] > shadow.incoming[0]);
        assert!(
            contact_shadow(bounds, -1.0, [0.0, 1.0, 0.0]).is_none(),
            "suspended object cannot ground on a distant floor"
        );
        assert!(
            contact_shadow(bounds, f32::NAN, [0.0; 3]).is_none(),
            "invalid floors are refused"
        );
    }

    fn light(key: &str, intensity: f32) -> DynamicLight {
        DynamicLight {
            key: key.to_string(),
            position: Vec3::new(1.0, 2.0, 3.0),
            color: [0.5, 0.75, 1.0],
            intensity,
            radius: 3.0,
        }
    }

    #[test]
    fn the_budget_refuses_new_keys_and_keeps_replacements() {
        let mut set = DynamicLightSet::new();
        for index in 0..MAX_DYNAMIC_LIGHTS {
            assert!(set.insert(light(&format!("glow:{index}"), 1.0)));
        }
        assert_eq!(set.len(), MAX_DYNAMIC_LIGHTS);
        assert!(set.is_full());
        // A ninth key is refused; the live set is untouched.
        assert!(!set.insert(light("glow:overflow", 1.0)));
        assert_eq!(set.len(), MAX_DYNAMIC_LIGHTS);
        assert!(set.get("glow:overflow").is_none());
        // An existing key still replaces at the cap, in its original slot.
        let replaced = light("glow:3", 4.0);
        assert!(set.insert(replaced.clone()));
        assert_eq!(set.len(), MAX_DYNAMIC_LIGHTS);
        assert_eq!(set.lights()[3], replaced);
        assert_eq!(set.lights()[0].key, "glow:0");
    }

    #[test]
    fn replacement_keeps_insertion_order_and_removal_shifts_it() {
        let mut set = DynamicLightSet::new();
        assert!(set.insert(light("a", 1.0)));
        assert!(set.insert(light("b", 1.0)));
        assert!(set.insert(light("c", 1.0)));
        assert!(set.insert(light("b", 2.0)));
        let keys: Vec<&str> = set
            .lights()
            .iter()
            .map(|light| light.key.as_str())
            .collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
        assert_eq!(set.get("b").expect("live").intensity, 2.0);
        assert!(set.remove("b"));
        assert!(!set.remove("b"));
        let remaining_keys: Vec<&str> = set
            .lights()
            .iter()
            .map(|light| light.key.as_str())
            .collect();
        assert_eq!(remaining_keys, vec!["a", "c"]);
        set.clear();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn ill_formed_values_are_refused_without_touching_the_set() {
        let mut set = DynamicLightSet::new();
        assert!(set.insert(light("glow:a", 1.0)));
        // A non-finite position, colour, intensity or radius never lands.
        let mut bad = light("glow:a", 2.0);
        bad.position = Vec3::new(f32::NAN, 0.0, 0.0);
        assert!(!set.insert(bad.clone()));
        assert_eq!(set.get("glow:a").expect("kept").intensity, 1.0);
        bad.position = Vec3::new(0.0, 0.0, f32::INFINITY);
        assert!(!set.insert(bad.clone()));
        bad.position = Vec3::new(0.0, 0.0, 0.0);
        bad.color = [f32::NAN, 0.0, 0.0];
        assert!(!set.insert(bad.clone()));
        bad.color = [0.0, 1.5, 0.0];
        assert!(!set.insert(bad.clone()));
        bad.color = [0.0, -0.5, 0.0];
        assert!(!set.insert(bad.clone()));
        bad.color = [0.0, 0.0, 1.0];
        bad.intensity = f32::NAN;
        assert!(!set.insert(bad.clone()));
        bad.intensity = -0.5;
        assert!(!set.insert(bad.clone()));
        bad.intensity = MAX_DYNAMIC_LIGHT_INTENSITY + 0.5;
        assert!(!set.insert(bad.clone()));
        bad.intensity = 1.0;
        bad.radius = 0.0;
        assert!(!set.insert(bad.clone()));
        bad.radius = -1.0;
        assert!(!set.insert(bad.clone()));
        bad.radius = f32::INFINITY;
        assert!(!set.insert(bad.clone()));
        bad.radius = MAX_DYNAMIC_LIGHT_RADIUS + 1.0;
        assert!(!set.insert(bad));
        // An empty key could not be replaced deterministically.
        assert!(!set.insert(light("", 1.0)));
        // The one valid light is still exactly what it was.
        assert_eq!(set.len(), 1);
        assert_eq!(set.get("glow:a").expect("kept").intensity, 1.0);
    }

    #[test]
    fn zero_intensity_and_zero_frame_lights_are_storable_but_removable() {
        // The set itself allows an off light (a caller decides whether to keep
        // one); opacity-driven removal happens in the character scene.
        let mut set = DynamicLightSet::new();
        assert!(set.insert(light("glow:a", 0.0)));
        assert_eq!(set.len(), 1);
        assert!(set.remove("glow:a"));
        assert!(set.is_empty());
    }
}

//! Validated, insertion-ordered light sources for animated and moving attachments.

use glam::Vec3;

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
    /// Opaque passenger-cabin shell and the authoritative moving door aperture.
    /// Ordinary attached glows retain their existing unrestricted behavior.
    pub enclosure: Option<super::elevator::LightEnclosure>,
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
            && self
                .enclosure
                .is_none_or(super::elevator::LightEnclosure::is_well_formed)
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

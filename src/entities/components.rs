//! Typed runtime components and their storage.
//!
//! One table per component kind, aligned with the entity store's slot indices.
//! Every entry records the generation it was inserted with, so a table answers
//! `get(stale_handle) == None` for any handle whose slot has been reused since
//! the write. Component lifetime follows slot lifetime: the owner
//! ([`crate::entities::EntityWorld`]) removes every component before it
//! releases a slot, so a removed entity can never be read through a recycled
//! handle.
//!
//! The component set is deliberately small and purpose-built. Static props,
//! doors, light fixtures, trigger volumes, effect emitters, timers, spawn
//! points and routed characters each resolve one entity with the components
//! the engine actually implements; there is no reflection, no string field
//! setter and no generic script bag.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::level::ProximityFade;

use super::id::EntityHandle;

/// One typed state value. Authored JSON is `true`, `3`, `2.5` or `"on"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateValue {
    /// A boolean state.
    Bool(bool),
    /// An integer state.
    Int(i64),
    /// A floating-point state.
    Float(f32),
    /// A text state.
    Text(String),
}

impl StateValue {
    /// The value as a boolean, when it is one.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Int(_) | Self::Float(_) | Self::Text(_) => None,
        }
    }

    /// The value as an integer, when it is one.
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Bool(_) | Self::Float(_) | Self::Text(_) => None,
        }
    }

    /// The value as `f32`, when it is a number.
    ///
    /// An integer loses precision only beyond 2^24, where a state value is
    /// already outside any gameplay use.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // documented integer-to-float view
    pub const fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Int(value) => Some(*value as f32),
            Self::Bool(_) | Self::Text(_) => None,
        }
    }

    /// The value as text, when it is one.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Bool(_) | Self::Int(_) | Self::Float(_) => None,
        }
    }

    /// Stable kind name, for diagnostics.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Text(_) => "text",
        }
    }
}

/// One component slot: the generation it was written with plus its value.
#[derive(Clone, Debug, PartialEq)]
struct Entry<T> {
    generation: u32,
    value: T,
}

/// A sparse table of one component kind, aligned with the entity store.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentTable<T> {
    slots: Vec<Option<Entry<T>>>,
}

impl<T> Default for ComponentTable<T> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<T> ComponentTable<T> {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Writes `value` for `handle`, returning the previous value.
    pub fn insert(&mut self, handle: EntityHandle, value: T) -> Option<T> {
        let index = handle.index() as usize;
        while self.slots.len() <= index {
            self.slots.push(None);
        }
        let previous = self
            .slots
            .get_mut(index)
            .and_then(std::option::Option::take)
            .filter(|entry| entry.generation == handle.generation())
            .map(|entry| entry.value);
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = Some(Entry {
                generation: handle.generation(),
                value,
            });
        }
        previous
    }

    /// The value for `handle`, when it is live at that generation.
    #[must_use]
    pub fn get(&self, handle: EntityHandle) -> Option<&T> {
        self.slots
            .get(handle.index() as usize)
            .and_then(Option::as_ref)
            .filter(|entry| entry.generation == handle.generation())
            .map(|entry| &entry.value)
    }

    /// The value for `handle`, mutably, when it is live at that generation.
    pub fn get_mut(&mut self, handle: EntityHandle) -> Option<&mut T> {
        self.slots
            .get_mut(handle.index() as usize)
            .and_then(Option::as_mut)
            .filter(|entry| entry.generation == handle.generation())
            .map(|entry| &mut entry.value)
    }

    /// Removes the value for `handle`, returning it when it was live.
    pub fn remove(&mut self, handle: EntityHandle) -> Option<T> {
        let slot = self.slots.get_mut(handle.index() as usize)?;
        let matches = slot
            .as_ref()
            .is_some_and(|entry| entry.generation == handle.generation());
        if !matches {
            return None;
        }
        slot.take().map(|entry| entry.value)
    }

    /// True when `handle` has a live value.
    #[must_use]
    pub fn contains(&self, handle: EntityHandle) -> bool {
        self.get(handle).is_some()
    }

    /// Every live entry, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityHandle, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.as_ref().and_then(|entry| {
                u32::try_from(index).ok().map(|index| {
                    (
                        EntityHandle::from_parts(index, entry.generation),
                        &entry.value,
                    )
                })
            })
        })
    }

    /// The number of live entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// True when no entry is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Drops every entry.
    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

/// A world-space pose: position, yaw and uniform scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// World position of the entity's base (feet or model origin).
    pub position: Vec3,
    /// Yaw in degrees; `0` faces `+X` for doors, `-Z` for characters.
    pub yaw_degrees: f32,
    /// Uniform scale.
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            yaw_degrees: 0.0,
            scale: 1.0,
        }
    }
}

/// What an entity draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renderable {
    /// Catalogue model id or path the renderer resolves.
    pub model: String,
    /// Material override authored on the entity, when it has one.
    pub material: Option<String>,
    /// False hides the entity without removing it.
    pub visible: bool,
    /// True when the entity is drawn through the dynamic-object path, so a
    /// transform change or a despawn is expressible at runtime. A baked static
    /// entity is `false`.
    pub dynamic: bool,
}

/// Authored collision for an entity that is not baked into the static world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    /// `[width, height, depth]` in metres.
    pub size: [f32; 3],
    /// True when the collider blocks movement.
    pub solid: bool,
}

/// Playback state of one named clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    /// Clip name resolved against the model's animation set.
    pub clip: String,
    /// Playback rate multiplier.
    pub speed: f32,
    /// True loops, false holds the last pose.
    pub looped: bool,
    /// True while the clip is advancing.
    pub playing: bool,
    /// Normalized progress in `0..=1`; `Scrub` targets ease this.
    pub progress: f32,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            clip: String::new(),
            speed: 1.0,
            looped: false,
            playing: false,
            progress: 0.0,
        }
    }
}

/// An aimable instance and what its interaction emits.
#[derive(Clone, Debug, PartialEq)]
pub struct Interactable {
    /// Prompt shown while aimed at. Empty means the entity's own default: a
    /// door shows its phase-appropriate open/close prompt, anything else shows
    /// `DEFAULT_INTERACTION_PROMPT`.
    pub prompt: String,
    /// Reach in metres, already capped.
    pub reach: f32,
    /// False disables aiming without removing the entity.
    pub enabled: bool,
    /// Optional display name a `toggle_label` action shows and hides.
    pub label: Option<String>,
    /// Whether the floating display name is currently shown.
    pub label_visible: bool,
}

/// An audio emitter's typed state.
///
/// `playing`/`looped`/`enabled` are the behaviour the audio route implements;
/// the backend that turns them into samples is the audio subsystem (see
/// `crate::audio`).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioEmitter {
    /// Authored sound asset id.
    pub sound: String,
    /// Linear gain.
    pub gain: f32,
    /// True loops until stopped.
    pub looped: bool,
    /// False mutes the emitter without clearing its state.
    pub enabled: bool,
    /// True once a `play_sound` action asked for playback.
    pub playing: bool,
}

/// One switchable light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Current on/off state.
    pub enabled: bool,
    /// Whether an action may switch it.
    pub switchable: bool,
    /// Emissive scale for the visible face.
    pub emission_scale: f32,
    /// Index of the prepared switchable lightmap group this light drives: a
    /// ceiling fixture's position in the level's `ceiling_lights`. `None` for a
    /// light with no prepared layers (a prop light or a spawned light), whose
    /// state is still real but changes only its own emission.
    pub fixture: Option<u32>,
    /// True when the state changed and the renderer has not applied it yet.
    pub dirty: bool,
}

/// One named material variant and the per-object emission scale it selects.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// The selected variant name.
    pub variant: String,
    /// Every variant the entity can select, with its emission scale.
    pub variants: Vec<(String, f32)>,
}

impl Material {
    /// The emission scale of the selected variant, or `1.0` when unknown.
    #[must_use]
    pub fn emission_scale(&self) -> f32 {
        self.variants
            .iter()
            .find(|(name, _)| name == &self.variant)
            .map_or(1.0, |(_, scale)| *scale)
    }

    /// Selects `variant`, returning whether it changed.
    pub fn select(&mut self, variant: &str) -> bool {
        if !self.variants.iter().any(|(name, _)| name == variant) || self.variant == variant {
            return false;
        }
        variant.clone_into(&mut self.variant);
        true
    }
}

/// One entity's typed states, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectState {
    /// `(name, value)` pairs in authored order.
    pub values: Vec<(String, StateValue)>,
}

impl ObjectState {
    /// The value of `name`, if it exists.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&StateValue> {
        self.values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// Writes `name`, returning true only when the stored value changed.
    ///
    /// A condition evaluation and a state event both read this: a `set_state`
    /// that writes the value it already holds is not a state change and does
    /// not emit an event.
    pub fn set(&mut self, name: &str, value: StateValue) -> bool {
        if let Some((_, current)) = self.values.iter_mut().find(|(key, _)| key == name) {
            if *current == value {
                return false;
            }
            *current = value;
            return true;
        }
        self.values.push((name.to_string(), value));
        true
    }
}

/// A trigger volume and its occupancy edge state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriggerVolume {
    /// Footprint bounds `(x0, x1, z0, z1)`, normalised.
    pub bounds: [f32; 4],
    /// Lowest world Y of the volume.
    pub bottom_y: f32,
    /// Highest world Y of the volume.
    pub top_y: f32,
    /// True while the player's feet were inside at the last update.
    pub inside: bool,
}

impl TriggerVolume {
    /// True when `(x, z, y)` lies inside the volume.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32, y: f32) -> bool {
        x >= self.bounds[0]
            && x <= self.bounds[1]
            && z >= self.bounds[2]
            && z <= self.bounds[3]
            && y >= self.bottom_y
            && y <= self.top_y
    }
}

/// The sequence controller attached to the entity that started a sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequenceCtl {
    /// Id of the running sequence; empty when idle.
    pub sequence: String,
    /// True while a sequence is running on this entity.
    pub running: bool,
}

/// How long a spawned entity lives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lifetime {
    /// Seconds until expiry; `None` never expires.
    pub remaining: Option<f32>,
    /// True removes the entity from the world when the timer reaches zero.
    pub despawn: bool,
}

/// One spawn point: where a template spawns and which group owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnPoint {
    /// Template id this point instantiates.
    pub template: String,
    /// Optional group id whose at-most-one rule applies.
    pub group: Option<String>,
}

/// One steam/effect emitter's enabled state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Steam {
    /// False suppresses the emitter's billboards without removing the level
    /// effect.
    pub enabled: bool,
    /// Authored index in the level's `effects` array, so an enable/disable
    /// reaches the renderer's resolved emitter. `None` for a component with no
    /// matching level effect.
    pub effect: Option<u32>,
}

/// One water volume's enabled state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaterVolumeCtl {
    /// False removes the volume from the water sampling: the baked surface
    /// still draws, but the controller walks or falls through the footprint.
    pub enabled: bool,
}

/// The physical body an entity navigates with.
///
/// The AI runtime selects the baked agent class that matches this profile
/// exactly; the same class drives the bake's clearance test, so a cell marked
/// walkable is traversable by exactly this body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavAgent {
    /// Body radius in metres.
    pub radius: f32,
    /// Preferred speed in m/s.
    pub speed_mps: f32,
    /// Body height in metres.
    pub height: f32,
    /// Largest surface rise the body walks up, in metres.
    pub step_height: f32,
    /// Largest walkable rise per metre of run.
    pub max_slope: f32,
}

impl NavAgent {
    /// The navigation profile this body selects.
    #[must_use]
    pub const fn profile(&self, can_open_doors: bool) -> crate::nav::NavAgentProfile {
        crate::nav::NavAgentProfile {
            radius: self.radius,
            height: self.height,
            step_height: self.step_height,
            max_slope: self.max_slope,
            can_open_doors,
        }
    }
}

/// An explicit navigation obstacle box read by the offline bake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavObstacle {
    /// `[width, height, depth]` in metres.
    pub size: [f32; 3],
    /// False marks a purely decorative obstacle.
    pub affects_nav: bool,
}

/// One opacity fade: the authored cycle or proximity contract plus the live
/// proximity controller state.
///
/// The cycle half is a pure function of the simulation clock
/// ([`Fade::opacity_at`]). The proximity half is *stateful*: the runtime
/// controller keeps a live opacity and the direction the hysteresis band last
/// decided, and advances the opacity from its current value in fixed substeps
/// ([`Fade::advance`]). Reversing mid-fade therefore continues from wherever
/// the opacity is; nothing snaps to an endpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    /// Cycle length in seconds, `> 0`; ignored in proximity mode.
    pub period_seconds: f32,
    /// Cycle phase in `0..1`.
    pub phase: f32,
    /// Lowest opacity of the range, `0..=1`.
    pub min_opacity: f32,
    /// Highest opacity of the range, `0..=1`, `>= min_opacity`.
    pub max_opacity: f32,
    /// False holds `max_opacity` instead of cycling or fading.
    pub enabled: bool,
    /// The proximity contract; `None` keeps the cosine cycle.
    pub proximity: Option<ProximityFade>,
    /// Live opacity of the proximity controller, in
    /// `min_opacity..=max_opacity`; starts at `max_opacity`.
    pub live_opacity: f32,
    /// Direction the hysteresis band holds between the two radii: `true`
    /// while fading out (the player is inside `near_radius`), `false` while
    /// fading back in (the player is beyond `far_radius`).
    pub fading_out: bool,
}

impl Fade {
    /// The opacity at simulation time `seconds`.
    ///
    /// A cycle uses a full cosine interpolation between the two ends, so the
    /// fade eases in and out with no cusp where a cycle wraps; a proximity
    /// fade ignores `seconds` and reports the live controller opacity, which
    /// [`Fade::advance`] integrates; a disabled fade is constant at
    /// `max_opacity`. A non-finite time or period is treated as zero so a
    /// corrupt value can never poison the frame.
    #[must_use]
    pub fn opacity_at(&self, seconds: f32) -> f32 {
        if !self.enabled {
            return self.max_opacity;
        }
        if self.proximity.is_some() {
            // The live controller already keeps this inside the range (it
            // starts at `max_opacity` and `advance` clamps every step), so no
            // `clamp` here: an unvalidated inverted range must not panic.
            return self.live_opacity;
        }
        let period = if self.period_seconds.is_finite() && self.period_seconds > 0.0 {
            self.period_seconds
        } else {
            1.0
        };
        let seconds = if seconds.is_finite() { seconds } else { 0.0 };
        let angle = std::f32::consts::TAU * (self.phase + seconds / period);
        let swing = 0.5 * (1.0 - angle.cos());
        (self.max_opacity - self.min_opacity).mul_add(swing, self.min_opacity)
    }

    /// True when the player's distance drives this fade instead of the cycle.
    #[must_use]
    pub const fn is_proximity(&self) -> bool {
        self.proximity.is_some()
    }

    /// Holds the direction the hysteresis band decides for `distance`, in
    /// metres.
    ///
    /// The entity begins fading out once the player is strictly inside
    /// `near_radius` and begins fading back in once the player is strictly
    /// beyond `far_radius`; between the two, the current direction holds, so
    /// walking across the band never flaps. A non-finite distance holds too.
    pub fn hold_direction(&mut self, distance: f32) {
        let Some(proximity) = self.proximity else {
            return;
        };
        if !distance.is_finite() {
            return;
        }
        if distance < proximity.near_radius {
            self.fading_out = true;
        } else if distance > proximity.far_radius {
            self.fading_out = false;
        }
    }

    /// Advances the live opacity by `seconds` from its **current** value.
    ///
    /// The rate is `1 / (fade_out_seconds | fade_in_seconds)` of the full
    /// `min_opacity..=max_opacity` range per second, so the same elapsed time
    /// produces the same opacity change whatever the frame rate, and an
    /// interrupted fade reverses with no jump. The caller integrates in fixed
    /// substeps; this is one substep. A cycle fade, a disabled fade, a
    /// zero-width range or a malformed time changes nothing.
    pub fn advance(&mut self, seconds: f32) {
        if self.proximity.is_none() {
            return;
        }
        if !self.enabled {
            // A disabled fade holds max_opacity at every instant; re-enabling
            // resumes the controller from the visible end.
            self.fading_out = false;
            self.live_opacity = self.max_opacity;
            return;
        }
        let range = self.max_opacity - self.min_opacity;
        if !(range.is_finite() && range > 0.0 && seconds.is_finite() && seconds > 0.0) {
            return;
        }
        let seconds_per_range = match self.proximity {
            Some(proximity) if self.fading_out => proximity.fade_out_seconds,
            Some(proximity) => proximity.fade_in_seconds,
            None => return,
        };
        if !(seconds_per_range.is_finite() && seconds_per_range > 0.0) {
            return;
        }
        let step = range * (seconds / seconds_per_range);
        if self.fading_out {
            self.live_opacity = (self.live_opacity - step).max(self.min_opacity);
        } else {
            self.live_opacity = (self.live_opacity + step).min(self.max_opacity);
        }
    }
}

/// One attached dynamic light: the runtime of the `glow` component.
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    /// Linear colour, each channel in `0..=1`.
    pub color: [f32; 3],
    /// Intensity in `0..=8`.
    pub intensity: f32,
    /// Reach in metres, `0.05..=64`.
    pub range: f32,
    /// Animated joint/node name the light attaches to.
    pub socket: Option<String>,
    /// Entity-local offset in metres.
    pub offset: [f32; 3],
    /// True multiplies the intensity by the entity's fade opacity.
    pub fade_with_opacity: bool,
}

/// Every component table of one world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentTables {
    /// World pose.
    pub transforms: ComponentTable<Transform>,
    /// What the entity draws.
    pub renderables: ComponentTable<Renderable>,
    /// Authored collision for runtime entities.
    pub colliders: ComponentTable<Collider>,
    /// Clip playback state.
    pub animations: ComponentTable<Animation>,
    /// Aimable instances.
    pub interactables: ComponentTable<Interactable>,
    /// Audio emitters.
    pub audio: ComponentTable<AudioEmitter>,
    /// Switchable lights.
    pub lights: ComponentTable<Light>,
    /// Per-object material variants.
    pub materials: ComponentTable<Material>,
    /// Typed state bags.
    pub states: ComponentTable<ObjectState>,
    /// Trigger volumes.
    pub volumes: ComponentTable<TriggerVolume>,
    /// Sequence controllers.
    pub sequences: ComponentTable<SequenceCtl>,
    /// Spawn lifetimes.
    pub lifetimes: ComponentTable<Lifetime>,
    /// Spawn points.
    pub spawn_points: ComponentTable<SpawnPoint>,
    /// Steam/effect emitters.
    pub steam: ComponentTable<Steam>,
    /// Water volume controls.
    pub water: ComponentTable<WaterVolumeCtl>,
    /// Navigation agent bodies.
    pub nav_agents: ComponentTable<NavAgent>,
    /// Explicit navigation obstacles.
    pub nav_obstacles: ComponentTable<NavObstacle>,
    /// AI behavior definitions.
    pub ais: ComponentTable<crate::ai::AiDef>,
    /// Opacity fade cycles.
    pub fades: ComponentTable<Fade>,
    /// Attached dynamic lights.
    pub glows: ComponentTable<Glow>,
}

impl ComponentTables {
    /// An empty component set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            transforms: ComponentTable::new(),
            renderables: ComponentTable::new(),
            colliders: ComponentTable::new(),
            animations: ComponentTable::new(),
            interactables: ComponentTable::new(),
            audio: ComponentTable::new(),
            lights: ComponentTable::new(),
            materials: ComponentTable::new(),
            states: ComponentTable::new(),
            volumes: ComponentTable::new(),
            sequences: ComponentTable::new(),
            lifetimes: ComponentTable::new(),
            spawn_points: ComponentTable::new(),
            steam: ComponentTable::new(),
            water: ComponentTable::new(),
            nav_agents: ComponentTable::new(),
            nav_obstacles: ComponentTable::new(),
            ais: ComponentTable::new(),
            fades: ComponentTable::new(),
            glows: ComponentTable::new(),
        }
    }

    /// Drops every component of every kind.
    pub fn clear(&mut self) {
        self.transforms.clear();
        self.renderables.clear();
        self.colliders.clear();
        self.animations.clear();
        self.interactables.clear();
        self.audio.clear();
        self.lights.clear();
        self.materials.clear();
        self.states.clear();
        self.volumes.clear();
        self.sequences.clear();
        self.lifetimes.clear();
        self.spawn_points.clear();
        self.steam.clear();
        self.water.clear();
        self.nav_agents.clear();
        self.nav_obstacles.clear();
        self.ais.clear();
        self.fades.clear();
        self.glows.clear();
    }

    /// Total entries across every table, for the diagnostics summary.
    #[must_use]
    #[allow(clippy::arithmetic_side_effects)] // bounded per-table entry counts
    pub fn len(&self) -> usize {
        self.transforms.len()
            + self.renderables.len()
            + self.colliders.len()
            + self.animations.len()
            + self.interactables.len()
            + self.audio.len()
            + self.lights.len()
            + self.materials.len()
            + self.states.len()
            + self.volumes.len()
            + self.sequences.len()
            + self.lifetimes.len()
            + self.spawn_points.len()
            + self.steam.len()
            + self.water.len()
            + self.nav_agents.len()
            + self.nav_obstacles.len()
            + self.ais.len()
            + self.fades.len()
            + self.glows.len()
    }

    /// True when no table holds an entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes every component of `handle`.
    pub fn remove_all(&mut self, handle: EntityHandle) {
        self.transforms.remove(handle);
        self.renderables.remove(handle);
        self.colliders.remove(handle);
        self.animations.remove(handle);
        self.interactables.remove(handle);
        self.audio.remove(handle);
        self.lights.remove(handle);
        self.materials.remove(handle);
        self.states.remove(handle);
        self.volumes.remove(handle);
        self.sequences.remove(handle);
        self.lifetimes.remove(handle);
        self.spawn_points.remove(handle);
        self.steam.remove(handle);
        self.water.remove(handle);
        self.nav_agents.remove(handle);
        self.nav_obstacles.remove(handle);
        self.ais.remove(handle);
        self.fades.remove(handle);
        self.glows.remove(handle);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing, clippy::float_cmp)]

    use super::*;
    use crate::entities::id::EntityStore;

    #[test]
    fn a_stale_handle_reads_nothing_from_any_table() {
        let mut store = EntityStore::new();
        let handle = store.insert();
        let mut tables = ComponentTables::new();
        tables.transforms.insert(
            handle,
            Transform {
                position: Vec3::new(1.0, 2.0, 3.0),
                yaw_degrees: 90.0,
                scale: 1.0,
            },
        );
        assert!(tables.transforms.contains(handle));
        // The owner removes the components before it releases the slot, which
        // is what makes the stale handle read nothing.
        tables.remove_all(handle);
        assert!(store.remove(handle));
        let reused = store.insert();
        assert_eq!(reused.index(), handle.index());
        assert_ne!(reused.generation(), handle.generation());
        assert!(tables.transforms.get(handle).is_none());
        assert!(tables.transforms.get(reused).is_none());
        // Writing through the reused handle is what attaches a component to
        // the new occupant; the old handle still reads nothing.
        tables.transforms.insert(
            reused,
            Transform {
                position: Vec3::new(1.0, 2.0, 3.0),
                yaw_degrees: 0.0,
                scale: 1.0,
            },
        );
        assert!(tables.transforms.get(reused).is_some());
        assert!(tables.transforms.get(handle).is_none());
    }

    #[test]
    fn iteration_yields_live_handles_in_slot_order() {
        // Slot order, not insertion order: a recycled slot sorts where the
        // slot is, which is what makes iteration deterministic.
        let mut store = EntityStore::new();
        let a = store.insert();
        let b = store.insert();
        store.remove(a);
        let c = store.insert();
        let mut tables = ComponentTables::new();
        tables.interactables.insert(
            b,
            Interactable {
                prompt: "B".into(),
                reach: 1.0,
                enabled: true,
                label: None,
                label_visible: false,
            },
        );
        tables.interactables.insert(
            c,
            Interactable {
                prompt: "C".into(),
                reach: 1.0,
                enabled: true,
                label: None,
                label_visible: false,
            },
        );
        let prompts: Vec<&str> = tables
            .interactables
            .iter()
            .map(|(_, item)| item.prompt.as_str())
            .collect();
        assert_eq!(
            prompts,
            vec!["C", "B"],
            "slot order: the recycled slot is first"
        );
    }

    #[test]
    fn state_writes_report_only_real_changes() {
        let mut state = ObjectState::default();
        assert!(state.set("on", StateValue::Bool(false)));
        assert!(!state.set("on", StateValue::Bool(false)), "no change");
        assert!(state.set("on", StateValue::Bool(true)), "changed");
        assert_eq!(state.get("on").and_then(StateValue::as_bool), Some(true));
        assert!(state.set("level", StateValue::Int(2)));
        assert_eq!(state.get("level").and_then(StateValue::as_int), Some(2));
    }

    #[test]
    fn state_values_expose_their_kind() {
        assert_eq!(StateValue::Bool(true).kind(), "bool");
        assert_eq!(StateValue::Int(3).as_float(), Some(3.0));
        assert_eq!(StateValue::Float(1.5).as_int(), None);
        assert_eq!(StateValue::Text("on".into()).as_text(), Some("on"));
        assert_eq!(StateValue::Text("on".into()).as_bool(), None);
    }

    #[test]
    fn material_variants_select_by_name() {
        let mut material = Material {
            variant: "off".into(),
            variants: vec![("off".into(), 0.0), ("on".into(), 1.0)],
        };
        assert_eq!(material.emission_scale(), 0.0);
        assert!(material.select("on"));
        assert!(!material.select("on"), "already selected");
        assert!(!material.select("missing"), "unknown variant");
        assert_eq!(material.emission_scale(), 1.0);
    }

    #[test]
    fn fade_opacity_cycles_between_min_and_max() {
        let fade = Fade {
            period_seconds: 4.0,
            phase: 0.0,
            min_opacity: 0.2,
            max_opacity: 1.0,
            enabled: true,
            proximity: None,
            live_opacity: 1.0,
            fading_out: false,
        };
        assert!((fade.opacity_at(0.0) - 0.2).abs() < 1e-6, "cycle start");
        assert!((fade.opacity_at(1.0) - 0.6).abs() < 1e-6, "quarter cycle");
        assert!((fade.opacity_at(2.0) - 1.0).abs() < 1e-6, "half cycle");
        assert!((fade.opacity_at(3.0) - 0.6).abs() < 1e-6, "three quarters");
        assert!((fade.opacity_at(4.0) - 0.2).abs() < 1e-5, "wraps to start");
        let disabled = Fade {
            enabled: false,
            ..fade
        };
        for seconds in [0.0, 1.0, 2.0, 3.0, 100.0] {
            assert!(
                (disabled.opacity_at(seconds) - 1.0).abs() < 1e-6,
                "a disabled fade holds max_opacity at {seconds}s"
            );
        }
    }

    #[test]
    fn fade_opacity_is_deterministic_and_bounded() {
        let fade = Fade {
            period_seconds: 6.0,
            phase: 0.25,
            min_opacity: 0.0,
            max_opacity: 0.8,
            enabled: true,
            proximity: None,
            live_opacity: 0.8,
            fading_out: false,
        };
        for seconds in [0.0, 0.5, 3.0, 12.0, 3600.0] {
            let first = fade.opacity_at(seconds);
            let second = fade.opacity_at(seconds);
            assert_eq!(first.to_bits(), second.to_bits(), "same input, same bits");
            assert!((0.0..=0.8).contains(&first), "{first} at {seconds}s");
        }
    }

    /// A fade with no proximity fields is the historical cycle, bit for bit,
    /// and its live proximity state stays inert.
    #[test]
    fn a_fade_without_proximity_fields_keeps_the_historical_cycle_bits() {
        let fade = Fade {
            period_seconds: 6.0,
            phase: 0.25,
            min_opacity: 0.1,
            max_opacity: 0.9,
            enabled: true,
            proximity: None,
            live_opacity: 0.9,
            fading_out: false,
        };
        assert!(!fade.is_proximity());
        for seconds in [0.0, 0.5, 1.234, 7.5, 1000.0] {
            let angle = std::f32::consts::TAU * (0.25 + seconds / 6.0);
            let swing = 0.5 * (1.0 - angle.cos());
            let expected = (0.9_f32 - 0.1).mul_add(swing, 0.1);
            assert_eq!(
                fade.opacity_at(seconds).to_bits(),
                expected.to_bits(),
                "the cycle formula at {seconds}s"
            );
        }
    }

    /// A proximity fade with the documented values: near 3 m, far 6 m, out
    /// in 1 s, back in 2 s over a full `0..=1` range.
    fn proximity_fade() -> Fade {
        Fade {
            period_seconds: crate::level::DEFAULT_FADE_PERIOD_SECONDS,
            phase: 0.0,
            min_opacity: 0.0,
            max_opacity: 1.0,
            enabled: true,
            proximity: Some(ProximityFade {
                near_radius: 3.0,
                far_radius: 6.0,
                fade_out_seconds: 1.0,
                fade_in_seconds: 2.0,
            }),
            live_opacity: 1.0,
            fading_out: false,
        }
    }

    #[test]
    fn a_proximity_fade_ignores_the_cycle() {
        let fade = proximity_fade();
        assert!(fade.is_proximity());
        // The live opacity is what reports, whatever the clock value.
        for seconds in [0.0, 3.0, 1_000.0] {
            assert_eq!(fade.opacity_at(seconds), 1.0);
        }
        let mut fading = fade;
        fading.fading_out = true;
        for seconds in [0.0, 120.0] {
            assert_eq!(
                fading.opacity_at(seconds),
                1.0,
                "no cycle may move a proximity fade before it advances"
            );
        }
    }

    #[test]
    fn proximity_hysteresis_holds_the_direction_between_the_radii() {
        let mut fade = proximity_fade();
        // Outside far: the direction is fade-in, and the full opacity holds.
        fade.hold_direction(8.0);
        assert!(!fade.fading_out);
        // Inside near: fade out, one second crosses the whole range.
        fade.hold_direction(2.0);
        assert!(fade.fading_out);
        fade.advance(1.0);
        assert!((fade.opacity_at(0.0) - 0.0).abs() < 1e-6, "fully hidden");
        // Between the radii the direction keeps fading out: the ghost stays
        // hidden until the player is beyond far_radius.
        fade.hold_direction(4.5);
        assert!(fade.fading_out);
        fade.advance(5.0);
        assert_eq!(fade.opacity_at(0.0), 0.0);
        // Beyond far: fade in at the in-time, from the current value.
        fade.hold_direction(7.0);
        assert!(!fade.fading_out);
        fade.advance(0.5);
        assert!(
            (fade.opacity_at(0.0) - 0.25).abs() < 1e-6,
            "half of the 2 s fade-in"
        );
        // Ambushing back inside near mid-fade reverses from where it is.
        fade.hold_direction(1.0);
        assert!(fade.fading_out);
        fade.advance(0.125);
        assert!(
            (fade.opacity_at(0.0) - 0.125).abs() < 1e-6,
            "an interrupted fade reverses with no jump"
        );
    }

    #[test]
    fn a_disabled_proximity_fade_holds_max_opacity() {
        let mut fade = proximity_fade();
        fade.fading_out = true;
        fade.live_opacity = 0.2;
        fade.enabled = false;
        fade.advance(10.0);
        assert_eq!(fade.opacity_at(0.0), 1.0);
        assert!(
            !fade.fading_out,
            "a disabled fade re-arms at the visible end"
        );
    }

    /// The total opacity change over the same elapsed time is the same at 30,
    /// 60 and 144 fps and over one large delta.
    #[test]
    fn proximity_fade_is_frame_rate_independent() {
        const SUBSTEP: f32 = 1.0 / 120.0;
        let run = |frames_per_second: f32| {
            let mut fade = proximity_fade();
            fade.hold_direction(1.0);
            let total = 0.75_f32;
            let frame = if frames_per_second > 0.0 {
                1.0 / frames_per_second
            } else {
                total
            };
            let mut remaining = total;
            loop {
                if remaining <= 0.0 {
                    break;
                }
                let frame_time = frame.min(remaining);
                remaining -= frame_time;
                let mut chunk = frame_time;
                loop {
                    if chunk <= 0.0 {
                        break;
                    }
                    let step = chunk.min(SUBSTEP);
                    chunk -= step;
                    fade.advance(step);
                }
            }
            fade.opacity_at(0.0)
        };
        let at_30 = run(30.0);
        let at_60 = run(60.0);
        let at_144 = run(144.0);
        let one_delta = run(0.0);
        for value in [at_30, at_60, at_144, one_delta] {
            assert!(
                (value - 0.25).abs() < 1e-4,
                "0.75 s of a 1 s fade-out leaves 0.25: {value}"
            );
        }
        assert!(
            (at_30 - at_144).abs() < 1e-5,
            "30 fps {at_30} vs 144 fps {at_144}"
        );
    }

    #[test]
    fn default_fade_phase_is_stable_and_instance_dependent() {
        let first = crate::level::default_fade_phase("sheet_ghost_1");
        assert!((0.0..1.0).contains(&first), "{first}");
        assert_eq!(
            first.to_bits(),
            crate::level::default_fade_phase("sheet_ghost_1").to_bits(),
            "one id always gets the same phase"
        );
        assert_ne!(
            first.to_bits(),
            crate::level::default_fade_phase("sheet_ghost_2").to_bits(),
            "two ids desynchronise without an authored phase"
        );
    }

    #[test]
    fn removing_a_handle_clears_every_component_kind() {
        let mut store = EntityStore::new();
        let handle = store.insert();
        let mut tables = ComponentTables::new();
        tables.steam.insert(
            handle,
            Steam {
                enabled: true,
                effect: None,
            },
        );
        tables.lights.insert(
            handle,
            Light {
                enabled: false,
                switchable: true,
                emission_scale: 1.0,
                fixture: Some(0),
                dirty: false,
            },
        );
        assert!(!tables.is_empty());
        assert_eq!(tables.len(), 2);
        tables.remove_all(handle);
        assert!(tables.is_empty());
    }
}

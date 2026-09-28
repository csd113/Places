//! The component-oriented entity runtime.
//!
//! One authored object (a prop, a door, a light fixture, a trigger volume, a
//! timer, a spawn point, an effect emitter) becomes one entity with a stable
//! authored id, a generation-checked runtime handle and a small set of typed
//! components. The world owns the component tables, the event queue, the
//! timers, the sequences and the spawn groups; the frame loop feeds it a delta
//! and player samples and drains the render commands it produces.
//!
//! ```text
//! LevelDef (authored)             EntityWorld (runtime)
//!   props/doors/lights/...  --->    EntityStore + component tables
//!   components/bindings        |    Events -> conditions -> Actions -> Commands
//!   sequences/timers/spawns    |    Doors / Interactables / Routes / Lights
//!                              v
//!                     render commands + dirty state -> frame loop
//! ```
//!
//! Identity is the authored id; a runtime [`EntityHandle`] is only ever
//! resolved through the store that issued it, so a queued event from an
//! unloaded world cannot address a coincidentally equal slot in a new one.
//! Actions never mutate while collections are being iterated: they enqueue
//! typed commands that the frame loop drains, and a despawn leaves its binding
//! entries inert (their owner handle goes stale) until the end-of-tick
//! compaction, so a binding index can never be invalidated mid-wave.

pub mod components;
pub mod events;
pub mod id;
pub mod sequences;
pub mod spawn;
pub mod timers;

use glam::Vec3;

use crate::ai::{AiDef, AiOutcome, AiStimulus, AiTarget, AiTickContext, AiWorld};
use crate::collision::{
    DoorCollider, WallAabb, resolve_player_collision_for_body_indexed, segment_overlaps_aabb,
};
use crate::collision_index::CollisionIndex;
use crate::door::{DoorPhase, Doors};
use crate::entity::{
    ENTITY_FACE_EPS_RAD, ENTITY_MIN_RADIUS_M, ENTITY_STEP_HEIGHT_M,
    ENTITY_TURN_RATE_DEGREES_PER_SECOND, EntityFrame, EntityRoute, EntityRoutes, PoseCue,
    RouteState, RouteWorld, angle_difference, turn_toward,
};
use crate::interact::{
    DEFAULT_INTERACTION_REACH_M, Interactable, InteractableSync, Interactables,
    MAX_INTERACTION_REACH_M, referenced_targets, rotated_half_extents,
};
use crate::level::{
    ActionDef, AnimatedEmissionDef, ComponentDef, ConditionDef, EventBindingDef, Ladders, LevelDef,
    LevelSurfaces, PROP_FALLBACK_SIZE, WalkableCeiling, WalkableFloor, WaterVolumes,
};
use crate::logging;
use crate::nav::NavMesh;

use components::{
    Animation, AudioEmitter, Collider, ComponentTables, Interactable as InteractableComponent,
    Lifetime, Light, Material, NavAgent, NavObstacle, ObjectState, Renderable, SequenceCtl,
    SpawnPoint as SpawnPointComponent, StateValue, Steam, Transform, TriggerVolume, WaterVolumeCtl,
};
use events::{ConditionView, EventKind, EventQueue, EventRecord};
use id::{EntityHandle, EntityId, EntityNames, EntityStore};
use sequences::{MAX_ACTIVE_SEQUENCES, SequenceRuntime, SequenceStepDef, Sequences};
use spawn::{SpawnGroups, SpawnPointDef, SpawnTemplateDef};
use timers::Timers;

/// Largest number of event waves one tick may process before the queue is
/// treated as a runaway chain.
pub const MAX_CHAIN_DEPTH: u32 = 16;

/// Largest number of events one tick may process across every wave.
pub const MAX_EVENTS_PER_TICK: usize = 256;

/// Default movement radius of a runtime entity that carries no collider.
pub const ENTITY_MOVE_RADIUS_M: f32 = 0.2;

/// Default speed of a `move_object` action with no authored speed, in m/s.
pub const DEFAULT_MOVE_SPEED_MPS: f32 = 1.5;

/// The world's view of the player and collision world for one tick.
pub struct WorldContext<'a> {
    /// Simulation delta in seconds, already clamped.
    pub delta_seconds: f32,
    /// The player's feet at the start of the frame.
    pub feet_from: Vec3,
    /// The player's feet now.
    pub feet: Vec3,
    /// The player's eye now (stance-aware).
    pub eye: Vec3,
    /// The player's collision body height.
    pub body_height: f32,
    /// The static collision boxes.
    pub walls: &'a [WallAabb],
    /// The spatial index over `walls`.
    pub index: &'a CollisionIndex,
    /// The walkable floor sampler.
    pub floor: &'a WalkableFloor,
    /// The baked navigation mesh, when the installed package carries one.
    pub nav: Option<&'a NavMesh>,
}

/// One renderer/audio command produced by the world.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldCommand {
    /// Spawn a runtime dynamic object for `entity` (its runtime key).
    SpawnDynamic {
        /// The entity's runtime render key.
        entity: u64,
        /// Catalogue model id.
        model: String,
        /// World position of the base.
        position: Vec3,
        /// Yaw in degrees.
        yaw_degrees: f32,
        /// Uniform scale.
        scale: f32,
    },
    /// Remove a runtime dynamic object.
    DespawnDynamic {
        /// The entity's runtime render key.
        entity: u64,
        /// The runtime instance id, so the frame loop can also release a
        /// runtime character the key belonged to.
        instance_id: String,
    },
    /// Move an existing runtime dynamic object.
    SetDynamicTransform {
        /// The entity's runtime render key.
        entity: u64,
        /// New world position.
        position: Vec3,
        /// New yaw in degrees.
        yaw_degrees: f32,
    },
    /// Select a runtime dynamic object's emission scale.
    SetDynamicEmission {
        /// The entity's runtime render key.
        entity: u64,
        /// Emission scale; `0.0` turns the object's emission off.
        scale: f32,
    },
    /// Start or update a sound.
    PlaySound {
        /// The emitter entity's runtime key.
        entity: u64,
        /// Sound asset id.
        sound: String,
        /// Linear gain.
        gain: f32,
        /// Loop until stopped.
        looped: bool,
    },
    /// Stop a sound.
    StopSound {
        /// The emitter entity's runtime key.
        entity: u64,
    },
    /// Enable or disable one authored effect emitter.
    SetEffectEnabled {
        /// Authored index in the level's `effects` array.
        index: u32,
        /// New state.
        enabled: bool,
    },
    /// Enable or disable one authored water volume's sampling.
    SetWaterEnabled {
        /// Authored index in the level's `water` array.
        index: u32,
        /// New state.
        enabled: bool,
    },
}

/// What one action batch or one tick did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchReport {
    /// Actions executed.
    pub actions_run: usize,
    /// Labels toggled on, summed over the batch.
    pub labels_shown: usize,
    /// Labels toggled off, summed over the batch.
    pub labels_hidden: usize,
    /// True when an action returned the player to the authored spawn.
    pub player_reset: bool,
    /// Actions skipped because their explicit `target` names no entity.
    pub missing_targets: usize,
    /// Actions skipped because the engine cannot perform them on that target.
    pub unsupported: usize,
    /// Animation cues started on placed entities, summed over the batch.
    pub animations_started: usize,
    /// Door leaves the batch requested to open, close or flip.
    pub doors_acted: usize,
    /// Light fixtures the batch switched on or off.
    pub lights_toggled: usize,
    /// Entities spawned by the batch.
    pub spawned: usize,
    /// Entities despawned by the batch.
    pub despawned: usize,
    /// Sequences started by the batch.
    pub sequences_started: usize,
    /// Timers started or stopped by the batch.
    pub timers_changed: usize,
    /// Object moves started by the batch.
    pub objects_moved: usize,
    /// Material variants selected by the batch.
    pub materials_changed: usize,
    /// Sound requests issued by the batch.
    pub sounds: usize,
}

impl DispatchReport {
    /// Number of label toggles, whichever direction.
    #[must_use]
    pub const fn labels_toggled(&self) -> usize {
        self.labels_shown.saturating_add(self.labels_hidden)
    }
}

/// One binding resolved against its owner entity.
#[derive(Clone, Debug, PartialEq)]
struct ResolvedBinding {
    /// The entity whose events this binding listens for.
    owner: EntityHandle,
    /// Authored binding id, for diagnostics.
    id: Option<String>,
    /// Event kind.
    on: EventKind,
    /// Optional event-key filter.
    key: Option<String>,
    /// Every condition must hold.
    when: Vec<ConditionDef>,
    /// Fire at most once per run.
    once: bool,
    /// Seconds between fires.
    cooldown_seconds: f32,
    /// The actions one fire runs.
    actions: Vec<ActionDef>,
    /// True once a `once` binding fired.
    fired: bool,
    /// Seconds until this binding may fire again.
    cooldown_remaining: f32,
}

/// One collision-respecting move in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MoveGoal {
    /// The moving entity.
    entity: EntityHandle,
    /// Target world position.
    target: Vec3,
    /// Speed in m/s.
    speed: f32,
}

/// Counters a tick reports to the frame loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldTick {
    /// Events processed this tick.
    pub events_processed: usize,
    /// Events dropped because a budget was exhausted.
    pub events_dropped: usize,
    /// Actions executed this tick.
    pub actions_run: usize,
    /// Sequences that completed this tick.
    pub sequences_completed: usize,
    /// Entities spawned this tick.
    pub spawned: usize,
    /// Entities despawned this tick.
    pub despawned: usize,
    /// Actions skipped because their explicit target named no live entity.
    pub missing_targets: usize,
    /// Actions the engine could not perform on their target.
    pub unsupported: usize,
    /// Labels shown by bindings this tick.
    pub labels_shown: usize,
    /// Labels hidden by bindings this tick.
    pub labels_hidden: usize,
    /// True when an action asked to return the player to the authored spawn.
    pub player_reset: bool,
    /// True when an entity frame changed and the renderer must be fed again.
    pub frames_dirty: bool,
}

/// The whole runtime for one loaded world.
pub struct EntityWorld {
    generation: u64,
    store: EntityStore,
    names: EntityNames,
    components: ComponentTables,
    interactables: Interactables,
    doors: Doors,
    door_colliders: Vec<DoorCollider>,
    routes: EntityRoutes,
    route_states: Vec<RouteState>,
    animation_overrides: Vec<(String, PoseCue)>,
    entity_frames: Vec<EntityFrame>,
    timers: Timers,
    sequences: Sequences,
    sequence_runs: Vec<SequenceRuntime>,
    spawn_groups: SpawnGroups,
    templates: Vec<SpawnTemplateDef>,
    spawn_points: Vec<SpawnPointDef>,
    bindings: Vec<ResolvedBinding>,
    events: EventQueue,
    commands: Vec<WorldCommand>,
    moves: Vec<MoveGoal>,
    /// Entities spawned at runtime and still alive, in spawn order.
    live_spawns: Vec<EntityHandle>,
    /// Runtime render keys issued to spawned entities, in spawn order.
    dynamic_keys: Vec<(EntityHandle, u64)>,
    /// Next runtime render key.
    next_dynamic_key: u64,
    /// Runtime spawns refused because the renderer could not take the model.
    spawn_failures: u64,
    /// Total event-queue refusals already reported to the frame loop.
    reported_drops: usize,
    /// Spawns admitted by the current tick (reset per tick).
    spawns_this_tick: usize,
    /// Authored light states, restored by a reset.
    authored_lights: Vec<(EntityHandle, bool)>,
    /// Authored state bags, restored by a reset.
    authored_states: Vec<(EntityHandle, ObjectState)>,
    water: WaterVolumes,
    ladders: Ladders,
    floor: WalkableFloor,
    ceiling: WalkableCeiling,
    animated_emissions: Vec<AnimatedEmissionDef>,
    /// The shared AI runtime of this world.
    ai: AiWorld,
    /// Bounded gameplay stimuli the AI can hear.
    stimuli: std::collections::VecDeque<AiStimulus>,
    /// Simulation seconds since the world was resolved.
    sim_time: f32,
}

impl std::fmt::Debug for EntityWorld {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EntityWorld")
            .field("generation", &self.generation)
            .field("entities", &self.store.len())
            .field("component_entries", &self.components.len())
            .field("bindings", &self.bindings.len())
            .field("timers", &self.timers.len())
            .field("sequences", &self.sequences.len())
            .field("spawn_groups", &self.spawn_groups.len())
            .field("live_spawns", &self.live_spawns.len())
            .finish_non_exhaustive()
    }
}

impl Default for EntityWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityWorld {
    /// An empty world: nothing exists until a level is resolved into it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: 0,
            store: EntityStore::new(),
            names: EntityNames::new(),
            components: ComponentTables::new(),
            interactables: Interactables::new(),
            doors: Doors::default(),
            door_colliders: Vec::new(),
            routes: EntityRoutes::new(),
            route_states: Vec::new(),
            animation_overrides: Vec::new(),
            entity_frames: Vec::new(),
            timers: Timers::new(),
            sequences: Sequences::new(),
            sequence_runs: Vec::new(),
            spawn_groups: SpawnGroups::new(),
            templates: Vec::new(),
            spawn_points: Vec::new(),
            bindings: Vec::new(),
            events: EventQueue::new(),
            commands: Vec::new(),
            moves: Vec::new(),
            live_spawns: Vec::new(),
            dynamic_keys: Vec::new(),
            next_dynamic_key: 1,
            spawn_failures: 0,
            reported_drops: 0,
            spawns_this_tick: 0,
            authored_lights: Vec::new(),
            authored_states: Vec::new(),
            water: WaterVolumes::new(),
            ladders: Ladders::new(),
            floor: WalkableFloor::default(),
            ceiling: WalkableCeiling::default(),
            animated_emissions: Vec::new(),
            ai: AiWorld::new(),
            stimuli: std::collections::VecDeque::new(),
            sim_time: 0.0,
        }
    }

    /// Resolves a whole level into a fresh world.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let mut world = Self::new();
        world.resolve(level);
        world
    }

    /// Rebuilds this world from a level, invalidating every previous handle.
    pub fn resolve(&mut self, level: &LevelDef) {
        self.store.clear();
        self.names.clear();
        self.components.clear();
        self.generation = self.store.generation();
        self.animation_overrides.clear();
        self.entity_frames.clear();
        self.sequence_runs.clear();
        self.events.clear();
        self.commands.clear();
        self.moves.clear();
        self.live_spawns.clear();
        self.dynamic_keys.clear();
        self.next_dynamic_key = 1;
        self.spawn_failures = 0;
        self.reported_drops = 0;
        self.spawns_this_tick = 0;
        self.authored_lights.clear();
        self.authored_states.clear();
        self.ai.clear();
        self.stimuli.clear();
        self.sim_time = 0.0;
        self.water = WaterVolumes::from_level(level);
        self.ladders = Ladders::from_level(level);
        self.floor = WalkableFloor::from_level(level);
        self.ceiling = WalkableCeiling::from_level(level);
        self.animated_emissions
            .clone_from(&level.animated_emissions);
        self.doors = Doors::from_level(level);
        self.timers = Timers::from_level(level);
        self.sequences = Sequences::from_level(level);
        self.spawn_groups = SpawnGroups::from_level(level);
        self.templates.clone_from(&level.spawn_templates);
        self.spawn_points.clone_from(&level.spawn_points);
        self.bindings.clear();

        let surfaces = LevelSurfaces::new(level);
        self.resolve_props(level, &surfaces);
        self.resolve_doors(level);
        self.resolve_fixtures(level);
        self.resolve_volumes(level);
        self.resolve_effects(level);
        self.resolve_water(level);
        self.resolve_timers(level);
        self.resolve_spawn_points(level, &surfaces);
        self.resolve_ai();
        self.resolve_routes(level);
        self.authored_states = self
            .components
            .states
            .iter()
            .map(|(handle, state)| (handle, state.clone()))
            .collect();
        self.rebuild_interactables(level);
        self.rebuild_door_colliders();
        self.rebuild_entity_frames();
    }

    /// Re-seeds every runtime to its authored start state without replacing the
    /// world: the `reset_to_start` contract.
    pub fn reset_runtime(&mut self) {
        for (handle, authored) in &self.authored_lights {
            if let Some(light) = self.components.lights.get_mut(*handle)
                && light.enabled != *authored
            {
                light.enabled = *authored;
                light.dirty = true;
            }
        }
        for (handle, authored) in &self.authored_states {
            if let Some(state) = self.components.states.get_mut(*handle) {
                state.clone_from(authored);
            }
        }
        self.sequence_runs.clear();
        let controls: Vec<EntityHandle> = self
            .components
            .sequences
            .iter()
            .map(|(handle, _)| handle)
            .collect();
        for handle in controls {
            if let Some(control) = self.components.sequences.get_mut(handle) {
                control.running = false;
                control.sequence.clear();
            }
        }
        for index in 0..self.routes.routes().len() {
            let Some(route) = self.routes.routes().get(index) else {
                continue;
            };
            let state = route.new_state();
            if let Some(slot) = self.route_states.get_mut(index) {
                *slot = state;
            }
        }
        self.doors.reset();
        self.timers.reset();
        self.spawn_groups.reset();
        self.reset_bindings();
        self.animation_overrides.clear();
        self.events.clear();
        self.stimuli.clear();
        self.sim_time = 0.0;
        self.ai.reset();
        self.ai.release_caught();
        let live: Vec<EntityHandle> = self.live_spawns.clone();
        for handle in live {
            let _ = self.despawn_handle(handle);
        }
        self.rebuild_door_colliders();
        self.sync_routed_interactables();
        self.rebuild_entity_frames();
    }

    /// Re-arms every binding: `once` bindings may fire again and cooldowns are
    /// cleared. A reset is a fresh run for map wiring, exactly as it is for
    /// doors, timers and spawn groups.
    fn reset_bindings(&mut self) {
        for binding in &mut self.bindings {
            binding.fired = false;
            binding.cooldown_remaining = 0.0;
        }
    }

    // ---- level resolution ---------------------------------------------

    fn resolve_props(&mut self, level: &LevelDef, surfaces: &LevelSurfaces<'_>) {
        let ids = level.prop_instance_ids();
        for (index, prop) in level.props.iter().enumerate() {
            let Some(id) = ids
                .get(index)
                .map(String::as_str)
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(prop.x, base_y, prop.z),
                    yaw_degrees: prop.rotation_degrees,
                    scale: prop.scale,
                },
            );
            self.components.renderables.insert(
                handle,
                Renderable {
                    model: prop.model.clone(),
                    material: None,
                    visible: true,
                    // A floating prop is drawn through the dynamic path; every
                    // other prop is baked into the static world.
                    dynamic: prop.float.is_some(),
                },
            );
            if prop.solid {
                self.components
                    .colliders
                    .insert(handle, Collider { size, solid: true });
            }
            self.apply_component_defs(handle, &prop.components);
            if let Some(interactable) = self.components.interactables.get_mut(handle)
                && interactable.label.is_none()
            {
                interactable.label = prop
                    .display_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string);
            }
            self.resolve_bindings_for(handle, &prop.bindings);
        }
    }

    fn resolve_doors(&mut self, level: &LevelDef) {
        for def in &level.doors {
            let id = def.id.trim();
            if id.is_empty() {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let base_y = def.base_y(level);
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, base_y, def.z),
                    yaw_degrees: def.rotation_degrees,
                    scale: 1.0,
                },
            );
            self.components.renderables.insert(
                handle,
                Renderable {
                    model: String::new(),
                    material: def.material.clone(),
                    visible: true,
                    dynamic: true,
                },
            );
            self.apply_component_defs(handle, &def.components);
            // A door carries its locked state on the door runtime, not on the
            // component, so a condition can read it through `Doors`.
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    fn resolve_fixtures(&mut self, level: &LevelDef) {
        let ids = level.light_instance_ids();
        for (index, fixture) in level.ceiling_lights.iter().enumerate() {
            let Some(id) = ids
                .get(index)
                .map(String::as_str)
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let y = fixture.y.filter(|y| y.is_finite()).unwrap_or(0.0);
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(fixture.x, y, fixture.z),
                    yaw_degrees: fixture.rotation_degrees,
                    scale: 1.0,
                },
            );
            self.components.lights.insert(
                handle,
                Light {
                    enabled: fixture.enabled,
                    switchable: fixture.switchable,
                    emission_scale: 1.0,
                    fixture: Some(u32::try_from(index).unwrap_or(u32::MAX)),
                    dirty: false,
                },
            );
            self.authored_lights.push((handle, fixture.enabled));
            self.resolve_bindings_for(handle, &fixture.bindings);
        }
    }

    fn resolve_volumes(&mut self, level: &LevelDef) {
        let ids = level.volume_instance_ids();
        for (index, def) in level.volumes.iter().enumerate() {
            let Some(id) = ids
                .get(index)
                .map(String::as_str)
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };
            let (x0, x1, z0, z1) = def.bounds();
            let (bottom_y, top_y) = def.resolved_y_bounds(level);
            if !x0.is_finite()
                || !x1.is_finite()
                || !z0.is_finite()
                || !z1.is_finite()
                || !bottom_y.is_finite()
                || !top_y.is_finite()
                || top_y <= bottom_y
            {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            self.components.volumes.insert(
                handle,
                TriggerVolume {
                    bounds: [x0, x1, z0, z1],
                    bottom_y,
                    top_y,
                    inside: false,
                },
            );
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    fn resolve_effects(&mut self, level: &LevelDef) {
        for (index, def) in level.effects.iter().enumerate() {
            let id = def
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map_or_else(
                    || format!("effect_{}", index.saturating_add(1)),
                    str::to_string,
                );
            if self.names.contains(&id) {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, def.y, def.z),
                    yaw_degrees: 0.0,
                    scale: 1.0,
                },
            );
            self.components.steam.insert(
                handle,
                Steam {
                    enabled: def.enabled,
                    effect: Some(u32::try_from(index).unwrap_or(u32::MAX)),
                },
            );
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    /// Creates one entity per authored water volume, so a `disable` action has
    /// a stable target and reaches the controller's sampling.
    fn resolve_water(&mut self, level: &LevelDef) {
        let ids = level.water_instance_ids();
        let volumes = crate::level::WaterVolumes::from_level(level);
        for (index, id) in ids.iter().enumerate() {
            if id.trim().is_empty() || self.names.contains(id) {
                continue;
            }
            let Some(volume) = volumes.volumes().get(index) else {
                continue;
            };
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(
                        f32::midpoint(volume.x0, volume.x1),
                        volume.surface_y,
                        f32::midpoint(volume.z0, volume.z1),
                    ),
                    yaw_degrees: 0.0,
                    scale: 1.0,
                },
            );
            self.components.water.insert(
                handle,
                WaterVolumeCtl {
                    enabled: volume.enabled,
                },
            );
        }
    }

    fn resolve_timers(&mut self, level: &LevelDef) {
        for def in &level.timers {
            let id = def.id.trim();
            if id.is_empty() || self.names.contains(id) {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    fn resolve_spawn_points(&mut self, level: &LevelDef, surfaces: &LevelSurfaces<'_>) {
        for def in &level.spawn_points {
            let id = def.id.trim();
            if id.is_empty() || self.names.contains(id) {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let y = def
                .y
                .filter(|y| y.is_finite())
                .unwrap_or_else(|| surfaces.floor_y_at(def.x, def.z).unwrap_or(0.0));
            self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, y, def.z),
                    yaw_degrees: def.yaw_degrees,
                    scale: 1.0,
                },
            );
            self.components.spawn_points.insert(
                handle,
                SpawnPointComponent {
                    template: def.template.clone(),
                    group: def.group.clone(),
                },
            );
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    /// Registers every authored AI agent from the resolved component tables.
    fn resolve_ai(&mut self) {
        let agents = self.ai_candidates();
        for handle in agents {
            let Some(def) = self.components.ais.get(handle).cloned() else {
                continue;
            };
            let Some(id) = self.id_of(handle).map(str::to_string) else {
                continue;
            };
            self.register_agent(handle, &id, def, false);
        }
    }

    /// Registers one runtime-spawned entity's AI agent, when its template
    /// carries both an `ai` definition and a navigation body.
    fn register_spawned_ai(&mut self, handle: EntityHandle, instance_id: &str) {
        let Some(def) = self.components.ais.get(handle).cloned() else {
            return;
        };
        self.register_agent(handle, instance_id, def, true);
    }

    /// Registers one agent from its resolved components.
    fn register_agent(
        &mut self,
        handle: EntityHandle,
        instance_id: &str,
        def: AiDef,
        spawned: bool,
    ) {
        let Some(transform) = self.components.transforms.get(handle).copied() else {
            return;
        };
        let profile = self.components.nav_agents.get(handle).map_or_else(
            || {
                warn_once(
                    "ai-without-nav-agent",
                    format!(
                        "[entities] `{instance_id}` authors an `ai` component without a \
                         `nav_agent`; the reference humanoid body is used"
                    ),
                );
                crate::nav::NavAgentProfile::reference()
            },
            |agent| agent.profile(def.can_open_doors),
        );
        self.ai.register(
            handle,
            instance_id,
            def,
            profile,
            transform.position,
            transform.yaw_degrees,
            spawned,
        );
    }

    /// Every entity carrying an authored AI definition, in table order.
    fn ai_candidates(&self) -> Vec<EntityHandle> {
        self.components
            .ais
            .iter()
            .map(|(handle, _)| handle)
            .collect()
    }

    fn resolve_routes(&mut self, level: &LevelDef) {
        self.routes = EntityRoutes::from_level(level);
        self.route_states = self
            .routes
            .routes()
            .iter()
            .map(EntityRoute::new_state)
            .collect();
        self.rebuild_entity_frames();
    }

    /// Applies a list of authored components to one entity.
    #[allow(clippy::too_many_lines)] // one cohesive component dispatcher
    fn apply_component_defs(&mut self, handle: EntityHandle, defs: &[ComponentDef]) {
        for def in defs {
            match def {
                ComponentDef::Interactable {
                    prompt,
                    reach,
                    enabled,
                    label,
                } => {
                    let prompt = prompt
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let reach = reach
                        .filter(|reach| reach.is_finite() && *reach > 0.0)
                        .map_or(DEFAULT_INTERACTION_REACH_M, |reach| {
                            reach.min(MAX_INTERACTION_REACH_M)
                        });
                    self.components.interactables.insert(
                        handle,
                        InteractableComponent {
                            prompt,
                            reach,
                            enabled: *enabled,
                            label: label
                                .as_deref()
                                .map(str::trim)
                                .filter(|name| !name.is_empty())
                                .map(str::to_string),
                            label_visible: false,
                        },
                    );
                }
                ComponentDef::Animation {
                    clip,
                    speed,
                    looped,
                    playing,
                } => {
                    self.components.animations.insert(
                        handle,
                        Animation {
                            clip: clip.clone(),
                            speed: if speed.is_finite() && *speed > 0.0 {
                                *speed
                            } else {
                                1.0
                            },
                            looped: *looped,
                            playing: *playing,
                            progress: 0.0,
                        },
                    );
                }
                ComponentDef::Audio {
                    sound,
                    gain,
                    looped,
                    enabled,
                    playing,
                } => {
                    self.components.audio.insert(
                        handle,
                        AudioEmitter {
                            sound: sound.clone(),
                            gain: if gain.is_finite() && *gain >= 0.0 {
                                *gain
                            } else {
                                1.0
                            },
                            looped: *looped,
                            enabled: *enabled,
                            playing: *playing,
                        },
                    );
                }
                ComponentDef::Light {
                    enabled,
                    switchable,
                    emission_scale,
                } => {
                    self.components.lights.insert(
                        handle,
                        Light {
                            enabled: *enabled,
                            switchable: *switchable,
                            emission_scale: *emission_scale,
                            fixture: None,
                            dirty: false,
                        },
                    );
                }
                ComponentDef::Material { variants, current } => {
                    let variant = current
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map_or_else(
                            || variants.first().map(|v| v.name.clone()).unwrap_or_default(),
                            str::to_string,
                        );
                    self.components.materials.insert(
                        handle,
                        Material {
                            variant,
                            variants: variants
                                .iter()
                                .map(|v| (v.name.clone(), v.emission_scale))
                                .collect(),
                        },
                    );
                }
                ComponentDef::State { name, value } => {
                    if let Some(state) = self.components.states.get_mut(handle) {
                        state.set(name, value.clone());
                    } else {
                        let mut state = ObjectState::default();
                        state.set(name, value.clone());
                        self.components.states.insert(handle, state);
                    }
                }
                ComponentDef::Lifetime { seconds } => {
                    self.components.lifetimes.insert(
                        handle,
                        Lifetime {
                            remaining: seconds
                                .is_finite()
                                .then_some(*seconds)
                                .filter(|seconds| *seconds > 0.0),
                            despawn: true,
                        },
                    );
                }
                ComponentDef::Steam { enabled } => {
                    self.components.steam.insert(
                        handle,
                        Steam {
                            enabled: *enabled,
                            effect: None,
                        },
                    );
                }
                ComponentDef::Water { enabled } => {
                    self.components
                        .water
                        .insert(handle, WaterVolumeCtl { enabled: *enabled });
                }
                ComponentDef::NavAgent {
                    radius,
                    speed_mps,
                    height,
                    step_height,
                    max_slope,
                } => {
                    self.components.nav_agents.insert(
                        handle,
                        NavAgent {
                            radius: *radius,
                            speed_mps: *speed_mps,
                            height: *height,
                            step_height: *step_height,
                            max_slope: *max_slope,
                        },
                    );
                }
                ComponentDef::Ai(def) => {
                    self.components.ais.insert(handle, def.clone());
                }
                ComponentDef::NavObstacle { size, affects_nav } => {
                    let size = size.unwrap_or_else(|| {
                        self.components
                            .colliders
                            .get(handle)
                            .map_or([0.5, 0.5, 0.5], |collider| collider.size)
                    });
                    self.components.nav_obstacles.insert(
                        handle,
                        NavObstacle {
                            size,
                            affects_nav: *affects_nav,
                        },
                    );
                }
            }
        }
    }

    /// Turns authored bindings into resolved runtime bindings for one entity.
    fn resolve_bindings_for(&mut self, handle: EntityHandle, defs: &[EventBindingDef]) {
        for def in defs {
            if def.actions.is_empty() {
                continue;
            }
            self.bindings.push(ResolvedBinding {
                owner: handle,
                id: def.id.clone(),
                on: EventKind::parse(def.on),
                key: def
                    .key
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string),
                when: def.when.clone(),
                once: def.once,
                cooldown_seconds: if def.cooldown_seconds.is_finite() {
                    def.cooldown_seconds.max(0.0)
                } else {
                    0.0
                },
                actions: def.actions.clone(),
                fired: false,
                cooldown_remaining: 0.0,
            });
        }
    }

    /// Builds the aiming table from the loaded level.
    fn rebuild_interactables(&mut self, level: &LevelDef) {
        let referenced = referenced_targets(level);
        let ids = level.prop_instance_ids();
        let surfaces = LevelSurfaces::new(level);
        let mut items: Vec<Interactable> = Vec::new();
        for (index, prop) in level.props.iter().enumerate() {
            let Some(id) = ids.get(index).map(String::as_str) else {
                continue;
            };
            let Some(handle) = self.handle_of(id) else {
                continue;
            };
            let component = self.components.interactables.get(handle);
            let is_target = referenced.iter().any(|target| target == id);
            if component.is_none() && !is_target {
                continue;
            }
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            let [size_x, size_y, size_z] = size;
            if !size.iter().all(|value| value.is_finite() && *value > 0.0)
                || !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
                || !prop.rotation_degrees.is_finite()
            {
                continue;
            }
            let display_name = prop
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(prop.model.as_str())
                .to_string();
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            let (extent_x, extent_z) =
                rotated_half_extents(size_x * 0.5, size_z * 0.5, prop.rotation_degrees);
            #[allow(clippy::arithmetic_side_effects)] // bounded world coordinates
            let (bounds, own_box, anchor) = {
                let own_box = prop.solid.then(|| {
                    WallAabb::with_y(
                        size_x.mul_add(-0.5, prop.x),
                        base_y,
                        size_z.mul_add(-0.5, prop.z),
                        size_x,
                        size_y,
                        size_z,
                    )
                });
                (
                    crate::spatial::Aabb {
                        min: [prop.x - extent_x, base_y, prop.z - extent_z],
                        max: [prop.x + extent_x, base_y + size_y, prop.z + extent_z],
                    },
                    own_box,
                    Vec3::new(prop.x, base_y + size_y + 0.28, prop.z),
                )
            };
            let (prompt, reach, enabled) = component.map_or_else(
                || (String::new(), DEFAULT_INTERACTION_REACH_M, false),
                |component| (component.prompt.clone(), component.reach, component.enabled),
            );
            items.push(Interactable {
                id: id.to_string(),
                display_name,
                prompt,
                reach,
                anchor,
                bounds,
                size,
                own_box,
                door_index: None,
                enabled,
            });
        }
        for (door_index, door) in self.doors.iter().enumerate() {
            let Some(handle) = self.handle_of(&door.def.id) else {
                continue;
            };
            let Some(component) = self.components.interactables.get(handle) else {
                continue;
            };
            let sync = InteractableSync::from_door_collider(&door.collider());
            items.push(Interactable {
                id: door.def.id.clone(),
                display_name: door.def.id.clone(),
                prompt: component.prompt.clone(),
                reach: component.reach,
                anchor: sync.anchor,
                bounds: sync.bounds,
                size: [door.def.width, door.def.height, door.def.thickness],
                own_box: None,
                door_index: Some(door_index),
                enabled: component.enabled,
            });
        }
        self.interactables = Interactables::with_items(items);
    }

    /// Adds, drops and refreshes aiming entries after runtime spawns,
    /// despawns and enable/disable changes.
    fn rebuild_interactables_from_tables(&mut self) {
        if self.interactables.is_empty() && self.components.interactables.is_empty() {
            return;
        }
        // Refresh enable flags and drop entries whose entity is gone.
        let mut items: Vec<Interactable> = self
            .interactables
            .items()
            .iter()
            .filter(|item| self.handle_of(&item.id).is_some())
            .cloned()
            .collect();
        for item in &mut items {
            if let Some(handle) = self.handle_of(&item.id)
                && let Some(component) = self.components.interactables.get(handle)
            {
                item.enabled = component.enabled;
                item.prompt.clone_from(&component.prompt);
                item.reach = component.reach;
            }
        }
        // Append new entities (runtime spawns) that carry an interactable and
        // are not represented yet.
        let mut additions: Vec<(EntityHandle, InteractableComponent)> = Vec::new();
        for (handle, component) in self.components.interactables.iter() {
            let Some(id) = self.id_of(handle) else {
                continue;
            };
            if items.iter().any(|item| item.id == id) {
                continue;
            }
            additions.push((handle, component.clone()));
        }
        for (handle, component) in additions {
            let Some(id) = self.id_of(handle).map(str::to_string) else {
                continue;
            };
            let position = self
                .components
                .transforms
                .get(handle)
                .map_or(Vec3::ZERO, |transform| transform.position);
            let size = self
                .components
                .colliders
                .get(handle)
                .map_or(PROP_FALLBACK_SIZE, |collider| collider.size);
            let yaw = self
                .components
                .transforms
                .get(handle)
                .map_or(0.0, |transform| transform.yaw_degrees);
            let (extent_x, extent_z) = rotated_half_extents(size[0] * 0.5, size[2] * 0.5, yaw);
            items.push(Interactable {
                id: id.clone(),
                display_name: id,
                prompt: component.prompt,
                reach: component.reach,
                anchor: Vec3::new(position.x, position.y + size[1] + 0.28, position.z),
                bounds: crate::spatial::Aabb {
                    min: [position.x - extent_x, position.y, position.z - extent_z],
                    max: [
                        position.x + extent_x,
                        position.y + size[1],
                        position.z + extent_z,
                    ],
                },
                size,
                own_box: None,
                door_index: None,
                enabled: component.enabled,
            });
        }
        self.interactables = Interactables::with_items(items);
    }

    fn rebuild_door_colliders(&mut self) {
        self.door_colliders = self.doors.colliders();
    }

    fn sync_door_interactables(&mut self) {
        if self.doors.is_empty() {
            return;
        }
        for index in 0..self.doors.len() {
            let (Some(collider), Some(door)) = (
                self.door_colliders.get(index).copied(),
                self.doors.get(index),
            ) else {
                continue;
            };
            let phase = door.phase();
            let id = door.def.id.clone();
            let prompt = self
                .components
                .interactables
                .get(
                    self.handle_of(&id)
                        .unwrap_or(EntityHandle::from_parts(0, 0)),
                )
                .map(|component| component.prompt.clone())
                .unwrap_or_default();
            self.interactables
                .sync_door(index, &collider, phase, Some(&prompt));
        }
    }

    // ---- accessors -----------------------------------------------------

    /// The world generation every queued reference records.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The entity store.
    #[must_use]
    pub const fn entities(&self) -> &EntityStore {
        &self.store
    }

    /// The authored id table.
    #[must_use]
    pub const fn names(&self) -> &EntityNames {
        &self.names
    }

    /// The component tables.
    #[must_use]
    pub const fn components(&self) -> &ComponentTables {
        &self.components
    }

    /// The component tables, mutably.
    ///
    /// The ordinary mutators (actions, sequences, spawns) own every change a
    /// map can express; this exists for tests and for engine code that must
    /// seed a component the authored schema has no action for.
    pub const fn components_mut(&mut self) -> &mut ComponentTables {
        &mut self.components
    }

    /// Number of sequences running right now.
    #[must_use]
    pub const fn sequence_count(&self) -> usize {
        self.sequence_runs.len()
    }

    /// Number of resolved event bindings (diagnostics and tests).
    #[must_use]
    pub const fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// The queued (unprocessed) event count, for diagnostics and tests.
    #[must_use]
    pub const fn events(&self) -> &EventQueue {
        &self.events
    }

    /// Number of runtime render keys currently bound to live spawns.
    #[must_use]
    pub const fn dynamic_key_entries(&self) -> usize {
        self.dynamic_keys.len()
    }

    /// Every door runtime of the level.
    #[must_use]
    pub const fn doors(&self) -> &Doors {
        &self.doors
    }

    /// The door runtimes, mutably.
    pub const fn doors_mut(&mut self) -> &mut Doors {
        &mut self.doors
    }

    /// The door leaf colliders at the current angles.
    #[must_use]
    pub fn door_colliders(&self) -> &[DoorCollider] {
        &self.door_colliders
    }

    /// The aiming table.
    #[must_use]
    pub const fn interactables(&self) -> &Interactables {
        &self.interactables
    }

    /// Every authored route.
    #[must_use]
    pub const fn routes(&self) -> &EntityRoutes {
        &self.routes
    }

    /// The per-route runtime states.
    #[must_use]
    pub fn route_states(&self) -> &[RouteState] {
        &self.route_states
    }

    /// Per-route state, mutably.
    pub fn route_states_mut(&mut self) -> &mut [RouteState] {
        &mut self.route_states
    }

    /// The timers.
    #[must_use]
    pub const fn timers(&self) -> &Timers {
        &self.timers
    }

    /// The authored sequences.
    #[must_use]
    pub const fn sequences(&self) -> &Sequences {
        &self.sequences
    }

    /// The spawn groups.
    #[must_use]
    pub const fn spawn_groups(&self) -> &SpawnGroups {
        &self.spawn_groups
    }

    /// The entities spawned at runtime and still alive, in spawn order.
    #[must_use]
    pub fn live_spawns(&self) -> &[EntityHandle] {
        &self.live_spawns
    }

    /// Water volumes sampled from the loaded level.
    #[must_use]
    pub const fn water(&self) -> &WaterVolumes {
        &self.water
    }

    /// Ladder volumes sampled from the loaded level.
    #[must_use]
    pub const fn ladders(&self) -> &Ladders {
        &self.ladders
    }

    /// The level's animated emission records.
    #[must_use]
    pub fn animated_emissions(&self) -> &[AnimatedEmissionDef] {
        &self.animated_emissions
    }

    /// The handle bound to an authored id, if the entity is alive.
    #[must_use]
    pub fn handle_of(&self, id: &str) -> Option<EntityHandle> {
        self.names
            .get(id)
            .filter(|handle| self.store.contains(*handle))
    }

    /// The authored id of a live handle.
    #[must_use]
    pub fn id_of(&self, handle: EntityHandle) -> Option<&str> {
        if !self.store.contains(handle) {
            return None;
        }
        self.names.id_of(handle).map(EntityId::as_str)
    }

    /// Runtime spawns the renderer could not take.
    #[must_use]
    pub const fn spawn_failures(&self) -> u64 {
        self.spawn_failures
    }

    /// Records that the frame loop could not spawn one runtime object.
    pub const fn note_spawn_failure(&mut self) {
        self.spawn_failures = self.spawn_failures.saturating_add(1);
    }

    /// Drains the render/audio commands produced since the last call.
    pub fn take_commands(&mut self) -> Vec<WorldCommand> {
        std::mem::take(&mut self.commands)
    }

    /// The label visibility of one aiming-table entry.
    #[must_use]
    pub fn is_label_visible(&self, index: usize) -> bool {
        self.interactables
            .get(index)
            .and_then(|item| self.handle_of(&item.id))
            .and_then(|handle| self.components.interactables.get(handle))
            .is_some_and(|component| component.label_visible)
    }

    /// Every switch state that changed and has not been applied yet, as
    /// `(fixture index, enabled)` pairs.
    pub fn take_light_toggles(&mut self) -> Vec<(usize, bool)> {
        let mut toggles = Vec::new();
        let mut dirty: Vec<(EntityHandle, usize, bool)> = Vec::new();
        for (handle, light) in self.components.lights.iter() {
            if !light.dirty {
                continue;
            }
            let Some(fixture) = light.fixture else {
                continue;
            };
            dirty.push((
                handle,
                usize::try_from(fixture).unwrap_or(usize::MAX),
                light.enabled,
            ));
        }
        for (handle, fixture, enabled) in dirty {
            if let Some(entry) = self.components.lights.get_mut(handle) {
                entry.dirty = false;
            }
            toggles.push((fixture, enabled));
        }
        toggles
    }

    /// Every switchable fixture's state, for a renderer rebuild.
    #[must_use]
    pub fn light_states(&self) -> Vec<(usize, bool)> {
        let mut states: Vec<(usize, bool)> = self
            .components
            .lights
            .iter()
            .filter_map(|(_, light)| {
                light.fixture.map(|fixture| {
                    (
                        usize::try_from(fixture).unwrap_or(usize::MAX),
                        light.enabled,
                    )
                })
            })
            .collect();
        states.sort_by_key(|(fixture, _)| *fixture);
        states
    }

    /// The per-frame character handoff to the renderer.
    #[must_use]
    pub fn entity_frames(&self) -> &[EntityFrame] {
        &self.entity_frames
    }

    /// The live animation override for an instance id, if any.
    #[must_use]
    pub fn animation_override(&self, instance_id: &str) -> Option<&PoseCue> {
        self.animation_overrides
            .iter()
            .rev()
            .find(|(id, _)| id == instance_id)
            .map(|(_, cue)| cue)
    }

    /// One route's runtime state by instance id.
    #[must_use]
    pub fn route_state(&self, instance_id: &str) -> Option<&RouteState> {
        let index = self.routes.index_of(instance_id)?;
        self.route_states.get(index)
    }

    // ---- events --------------------------------------------------------

    /// Emits one event from `subject` at chain depth 0.
    pub fn emit(
        &mut self,
        kind: EventKind,
        subject: EntityHandle,
        key: &str,
        actor: Option<EntityHandle>,
    ) -> bool {
        self.emit_depth(kind, subject, key, actor, 0)
    }

    /// Emits an event with an explicit chain depth.
    fn emit_depth(
        &mut self,
        kind: EventKind,
        subject: EntityHandle,
        key: &str,
        actor: Option<EntityHandle>,
        depth: u8,
    ) -> bool {
        self.events.push(EventRecord {
            generation: self.generation,
            kind,
            subject,
            key: key.to_string(),
            actor,
            depth,
        })
    }

    /// Runs the interaction on the aimed-at instance, if any.
    pub fn dispatch_interaction(&mut self, target: Option<usize>) -> Option<DispatchReport> {
        let index = target?;
        let id = self.interactables.get(index)?.id.clone();
        let handle = self.handle_of(&id)?;
        let position = self
            .components
            .transforms
            .get(handle)
            .map_or(Vec3::ZERO, |transform| transform.position);
        self.note_stimulus(position, 8.0, "interact", Some(handle));
        self.emit(EventKind::Interact, handle, "", Some(handle));
        let mut tick = WorldTick::default();
        self.pump_events(&mut tick);
        self.report_queue_refusals(&mut tick);
        if tick.frames_dirty {
            self.rebuild_entity_frames();
        }
        Some(DispatchReport {
            actions_run: tick.actions_run,
            player_reset: tick.player_reset,
            spawned: tick.spawned,
            despawned: tick.despawned,
            missing_targets: tick.missing_targets,
            unsupported: tick.unsupported,
            labels_shown: tick.labels_shown,
            labels_hidden: tick.labels_hidden,
            ..DispatchReport::default()
        })
    }

    /// Runs one ordered batch of actions programmatically.
    pub fn dispatch_actions(
        &mut self,
        actions: &[ActionDef],
        actor: Option<EntityHandle>,
    ) -> DispatchReport {
        let mut report = DispatchReport::default();
        if actions.len() > crate::level::MAX_ACTIONS_PER_SOURCE {
            let refused = actions
                .len()
                .saturating_sub(crate::level::MAX_ACTIONS_PER_SOURCE);
            report.unsupported = report.unsupported.saturating_add(refused);
            warn_once(
                "action-batch-truncated",
                format!(
                    "[entities] an action batch held {} actions; the bound is {} and the tail \
                     was refused",
                    actions.len(),
                    crate::level::MAX_ACTIONS_PER_SOURCE
                ),
            );
        }
        for action in actions.iter().take(crate::level::MAX_ACTIONS_PER_SOURCE) {
            self.run_action(action, actor, &mut report, 0);
            if report.player_reset {
                break;
            }
        }
        if report.animations_started > 0 || report.spawned > 0 || report.despawned > 0 {
            self.rebuild_entity_frames();
        }
        report
    }

    /// Runs an action batch on the entity with the given authored id.
    pub fn dispatch_on(&mut self, id: &str, actions: &[ActionDef]) -> DispatchReport {
        let actor = self.handle_of(id);
        self.dispatch_actions(actions, actor)
    }

    /// Resolves an action target: an explicit id must resolve on its own, an
    /// omitted target is the acting entity.
    fn resolve_target(&self, target: Option<&str>, actor: Option<EntityHandle>) -> Option<String> {
        match target {
            Some(id) => {
                let trimmed = id.trim();
                if trimmed.is_empty() {
                    return None;
                }
                self.handle_of(trimmed).map(|_| trimmed.to_string())
            }
            None => actor.and_then(|handle| self.id_of(handle).map(str::to_string)),
        }
    }

    /// Runs one action.
    #[allow(clippy::too_many_lines)] // one cohesive typed action dispatcher
    fn run_action(
        &mut self,
        action: &ActionDef,
        actor: Option<EntityHandle>,
        report: &mut DispatchReport,
        depth: u8,
    ) {
        match action {
            ActionDef::ResetToStart => {
                report.actions_run = report.actions_run.saturating_add(1);
                report.player_reset = true;
            }
            ActionDef::Open { target } | ActionDef::Close { target } => {
                let request_open = matches!(action, ActionDef::Open { .. });
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.doors.index_of(&id).is_none() {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "door-action-unsupported",
                        format!(
                            "[entities] action `{}` names `{id}`, which is not a door",
                            action.kind()
                        ),
                    );
                    return;
                }
                let changed = if request_open {
                    self.doors.request_open(&id)
                } else {
                    self.doors.request_close(&id)
                };
                report.actions_run = report.actions_run.saturating_add(1);
                if changed {
                    report.doors_acted = report.doors_acted.saturating_add(1);
                }
            }
            ActionDef::Toggle { target } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.doors.index_of(&id).is_some() {
                    if self.doors.toggle(&id) {
                        report.actions_run = report.actions_run.saturating_add(1);
                        report.doors_acted = report.doors_acted.saturating_add(1);
                    }
                    return;
                }
                if self.toggle_light_by_id(&id) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.lights_toggled = report.lights_toggled.saturating_add(1);
                    return;
                }
                report.unsupported = report.unsupported.saturating_add(1);
                warn_once(
                    "toggle-unsupported",
                    format!("[entities] `toggle` cannot act on `{id}`"),
                );
            }
            ActionDef::SetLight { target, on } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                match self.set_light_by_id(&id, *on) {
                    LightOutcome::Changed => {
                        report.actions_run = report.actions_run.saturating_add(1);
                        report.lights_toggled = report.lights_toggled.saturating_add(1);
                    }
                    LightOutcome::Unchanged => {
                        report.actions_run = report.actions_run.saturating_add(1);
                    }
                    LightOutcome::NotALight | LightOutcome::NotSwitchable => {
                        report.unsupported = report.unsupported.saturating_add(1);
                        warn_once(
                            "set-light-unsupported",
                            format!(
                                "[entities] `set_light` names `{id}`, which has no switchable \
                                 light"
                            ),
                        );
                    }
                }
            }
            ActionDef::Enable { target } | ActionDef::Disable { target } => {
                let enable = matches!(action, ActionDef::Enable { .. });
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.set_enabled_by_id(&id, enable) {
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "enable-unsupported",
                        format!(
                            "[entities] `{}` cannot change `{id}`'s enabled state",
                            action.kind()
                        ),
                    );
                }
            }
            ActionDef::Lock { target } | ActionDef::Unlock { target } => {
                let locked = matches!(action, ActionDef::Lock { .. });
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.doors.set_locked(&id, locked) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    self.emit_depth(
                        EventKind::ObjectState,
                        self.handle_of(&id)
                            .unwrap_or(EntityHandle::from_parts(0, 0)),
                        "locked",
                        actor,
                        depth,
                    );
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "lock-unsupported",
                        format!(
                            "[entities] `{}` names `{id}`, which is not a door",
                            action.kind()
                        ),
                    );
                }
            }
            ActionDef::PlayAnimation {
                target,
                clip,
                looped,
            } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(name) = clip
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "play-animation-no-clip",
                        "[entities] `play_animation` has no clip".to_string(),
                    );
                    return;
                };
                self.set_animation_cue(&id, name, *looped, false);
                report.actions_run = report.actions_run.saturating_add(1);
                report.animations_started = report.animations_started.saturating_add(1);
            }
            ActionDef::ToggleAnimation { target, clip } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(name) = clip
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "toggle-animation-no-clip",
                        "[entities] `toggle_animation` has no clip".to_string(),
                    );
                    return;
                };
                self.set_animation_cue(&id, name, false, true);
                report.actions_run = report.actions_run.saturating_add(1);
                report.animations_started = report.animations_started.saturating_add(1);
            }
            ActionDef::SetState {
                target,
                name,
                value,
            } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.emit_state(&id, name, value.clone(), actor, depth.saturating_add(1)) {
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                }
            }
            ActionDef::ToggleLabel { target } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(handle) = self.handle_of(&id) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(component) = self.components.interactables.get_mut(handle) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                component.label_visible = !component.label_visible;
                let visible = component.label_visible;
                report.actions_run = report.actions_run.saturating_add(1);
                if visible {
                    report.labels_shown = report.labels_shown.saturating_add(1);
                } else {
                    report.labels_hidden = report.labels_hidden.saturating_add(1);
                }
            }
            ActionDef::StartSequence { sequence, target } => {
                let id = self
                    .resolve_target(target.as_deref(), actor)
                    .or_else(|| self.id_of(actor?).map(str::to_string));
                let Some(id) = id else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.start_sequence(&id, sequence.trim()) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.sequences_started = report.sequences_started.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "start-sequence-unknown",
                        format!("[entities] sequence `{sequence}` cannot start on `{id}`"),
                    );
                }
            }
            ActionDef::StopSequence { target } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.stop_sequence(&id) {
                    report.actions_run = report.actions_run.saturating_add(1);
                } else if self
                    .handle_of(&id)
                    .is_some_and(|handle| self.components.sequences.get(handle).is_none())
                {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "stop-sequence-unsupported",
                        format!("[entities] `stop_sequence` names `{id}`, which runs no sequence"),
                    );
                }
            }
            ActionDef::StartTimer {
                target,
                seconds,
                repeat,
            } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.timers.start(&id, *seconds, *repeat) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.timers_changed = report.timers_changed.saturating_add(1);
                } else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                }
            }
            ActionDef::StopTimer { target } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.timers.stop(&id) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.timers_changed = report.timers_changed.saturating_add(1);
                } else if self.timers.index_of(&id).is_some() {
                    // A known timer that is already stopped.
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "stop-timer-unsupported",
                        format!("[entities] `stop_timer` names `{id}`, which is not a timer"),
                    );
                }
            }
            ActionDef::SpawnEntity {
                template,
                point,
                group,
                name,
            } => match self.spawn_entity(
                point.as_deref(),
                template.as_deref(),
                group.as_deref(),
                name.as_deref(),
                depth,
            ) {
                Ok(handle) => {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.spawned = report.spawned.saturating_add(1);
                    if self.components.interactables.contains(handle) {
                        self.rebuild_interactables_from_tables();
                    }
                }
                Err(SpawnError::MissingTarget) => {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                }
                Err(SpawnError::Refused(reason)) => {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "spawn-refused",
                        format!("[entities] spawn refused: {reason}"),
                    );
                }
            },
            ActionDef::DespawnEntity { target } => {
                if self.despawn_target(target.trim()) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.despawned = report.despawned.saturating_add(1);
                } else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                }
            }
            ActionDef::MoveObject {
                target,
                x,
                y,
                z,
                speed,
            } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(handle) = self.handle_of(&id) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let dynamic = self
                    .components
                    .renderables
                    .get(handle)
                    .is_some_and(|renderable| renderable.dynamic)
                    && self.doors.index_of(&id).is_none();
                if !dynamic || !x.is_finite() || !z.is_finite() {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "move-unsupported",
                        format!(
                            "[entities] `move_object` cannot move `{id}`: only a runtime-spawned \
                             instance moves (a door uses open/close/toggle)"
                        ),
                    );
                    return;
                }
                let target_y = y
                    .filter(|value| value.is_finite())
                    .or_else(|| {
                        self.components
                            .transforms
                            .get(handle)
                            .map(|transform| transform.position.y)
                    })
                    .unwrap_or(0.0);
                let speed = speed
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(DEFAULT_MOVE_SPEED_MPS);
                self.moves.retain(|goal| goal.entity != handle);
                self.moves.push(MoveGoal {
                    entity: handle,
                    target: Vec3::new(*x, target_y, *z),
                    speed,
                });
                report.actions_run = report.actions_run.saturating_add(1);
                report.objects_moved = report.objects_moved.saturating_add(1);
            }
            ActionDef::ChangeMaterial { target, variant } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(handle) = self.handle_of(&id) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let selected = self
                    .components
                    .materials
                    .get_mut(handle)
                    .is_some_and(|material| material.select(variant));
                if !selected {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "material-unsupported",
                        format!("[entities] `change_material` cannot select `{variant}` on `{id}`"),
                    );
                    return;
                }
                // Only a runtime-spawned object has a material the renderer can
                // re-bind; a baked static prop's material is prepared geometry.
                let Some(key) = self.dynamic_key(handle) else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "material-static",
                        format!(
                            "[entities] `change_material` on `{id}` has no effect: only a \
                             runtime-spawned instance re-binds its material"
                        ),
                    );
                    return;
                };
                let scale = self
                    .components
                    .materials
                    .get(handle)
                    .map_or(1.0, Material::emission_scale);
                self.commands
                    .push(WorldCommand::SetDynamicEmission { entity: key, scale });
                report.actions_run = report.actions_run.saturating_add(1);
                report.materials_changed = report.materials_changed.saturating_add(1);
            }
            ActionDef::PlaySound {
                target,
                sound,
                looped,
            } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(handle) = self.handle_of(&id) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(emitter) = self.components.audio.get(handle).cloned() else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "sound-unsupported",
                        format!("[entities] `play_sound` names `{id}`, which has no audio emitter"),
                    );
                    return;
                };
                let sound = sound
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| emitter.sound.clone(), str::to_string);
                if sound.is_empty() {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                if let Some(entry) = self.components.audio.get_mut(handle) {
                    entry.playing = true;
                    entry.looped = *looped;
                }
                let Some(key) = self.dynamic_key(handle).or_else(|| self.stable_key(handle)) else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                };
                self.commands.push(WorldCommand::PlaySound {
                    entity: key,
                    sound,
                    gain: emitter.gain,
                    looped: *looped,
                });
                report.actions_run = report.actions_run.saturating_add(1);
                report.sounds = report.sounds.saturating_add(1);
            }
            ActionDef::StopSound { target } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let Some(handle) = self.handle_of(&id) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.components.audio.get(handle).is_none() {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                if let Some(entry) = self.components.audio.get_mut(handle) {
                    entry.playing = false;
                }
                if let Some(key) = self.dynamic_key(handle).or_else(|| self.stable_key(handle)) {
                    self.commands.push(WorldCommand::StopSound { entity: key });
                }
                report.actions_run = report.actions_run.saturating_add(1);
            }
        }
    }

    /// Writes a typed state, emitting `object_state` only on a real change.
    ///
    /// A name the entity does not carry yet is created: a headless entity (a
    /// timer, a volume) has no authored component list, so a `set_state` on it
    /// starts its state bag. Validation still requires a `set_state` on a
    /// component-carrying entity to name a state that exists.
    fn emit_state(
        &mut self,
        entity_id: &str,
        name: &str,
        value: StateValue,
        actor: Option<EntityHandle>,
        depth: u8,
    ) -> bool {
        let Some(handle) = self.handle_of(entity_id) else {
            return false;
        };
        if let Some(state) = self.components.states.get_mut(handle) {
            if !state.set(name, value) {
                return true;
            }
        } else {
            let mut state = ObjectState::default();
            state.set(name, value);
            self.components.states.insert(handle, state);
        }
        self.emit_depth(EventKind::ObjectState, handle, name, actor, depth);
        true
    }

    fn toggle_light_by_id(&mut self, id: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let Some(light) = self.components.lights.get(handle).copied() else {
            return false;
        };
        if !light.switchable {
            return false;
        }
        self.set_light(handle, !light.enabled)
    }

    fn set_light_by_id(&mut self, id: &str, on: bool) -> LightOutcome {
        let Some(handle) = self.handle_of(id) else {
            return LightOutcome::NotALight;
        };
        match self.components.lights.get(handle) {
            Some(light) if light.switchable => {}
            Some(_) => return LightOutcome::NotSwitchable,
            None => return LightOutcome::NotALight,
        }
        if self.set_light(handle, on) {
            LightOutcome::Changed
        } else {
            LightOutcome::Unchanged
        }
    }

    /// Sets one light component, marking it dirty on a real change.
    fn set_light(&mut self, handle: EntityHandle, on: bool) -> bool {
        let Some(light) = self.components.lights.get(handle).copied() else {
            return false;
        };
        if !light.switchable {
            warn_once(
                "light-not-switchable",
                "[entities] `set_light` names a light that is not switchable; a \
                 non-switchable light is baked once and never changes"
                    .to_string(),
            );
            return false;
        }
        if light.enabled == on {
            return false;
        }
        let id = self.id_of(handle).map(str::to_string).unwrap_or_default();
        if let Some(entry) = self.components.lights.get_mut(handle) {
            entry.enabled = on;
            entry.dirty = true;
        }
        if light.fixture.is_none() {
            warn_once(
                "prop-light-state",
                format!(
                    "[entities] light `{id}` has no prepared switchable layers: its state and \
                     emission change, the baked illumination does not"
                ),
            );
        }
        self.emit_state(&id, "light", StateValue::Bool(on), None, 1);
        true
    }

    fn set_enabled_by_id(&mut self, id: &str, enable: bool) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let mut changed = false;
        changed |= self
            .components
            .interactables
            .get_mut(handle)
            .is_some_and(|entry| {
                let before = entry.enabled;
                entry.enabled = enable;
                before != enable
            });
        changed |= self.components.audio.get_mut(handle).is_some_and(|entry| {
            let before = entry.enabled;
            entry.enabled = enable;
            entry.playing = enable && entry.playing;
            before != enable
        });
        changed |= self.components.steam.get_mut(handle).is_some_and(|entry| {
            let before = entry.enabled;
            entry.enabled = enable;
            before != enable
        });
        if let Some(effect) = self
            .components
            .steam
            .get(handle)
            .and_then(|steam| steam.effect)
        {
            self.commands.push(WorldCommand::SetEffectEnabled {
                index: effect,
                enabled: enable,
            });
        }
        let water_changed = self
            .components
            .water
            .get(handle)
            .is_some_and(|entry| entry.enabled != enable);
        changed |= self.components.water.get_mut(handle).is_some_and(|entry| {
            let before = entry.enabled;
            entry.enabled = enable;
            before != enable
        });
        if water_changed && let Some(index) = self.water_volume_index(handle) {
            self.commands.push(WorldCommand::SetWaterEnabled {
                index,
                enabled: enable,
            });
        }
        changed |= self
            .components
            .animations
            .get_mut(handle)
            .is_some_and(|entry| {
                let before = entry.playing;
                entry.playing = enable;
                before != enable
            });
        if self
            .components
            .lights
            .get(handle)
            .is_some_and(|light| light.switchable && light.enabled != enable)
            && self.set_light(handle, enable)
        {
            changed = true;
        }
        if changed {
            self.rebuild_interactables_from_tables();
            self.emit_state(id, "enabled", StateValue::Bool(enable), None, 0);
        }
        changed
    }

    fn set_animation_cue(&mut self, instance_id: &str, clip: &str, looped: bool, toggle: bool) {
        let next = if toggle {
            let current = self
                .animation_overrides
                .iter()
                .rev()
                .find(|(id, _)| id == instance_id)
                .and_then(|(_, cue)| match cue {
                    PoseCue::Scrub { target, .. } => Some(*target),
                    PoseCue::Idle | PoseCue::Walk { .. } | PoseCue::Clip { .. } => None,
                });
            let target = match current {
                Some(target) if target >= 0.5 => 0.0,
                Some(_) | None => 1.0,
            };
            PoseCue::Scrub {
                name: clip.to_string(),
                target,
            }
        } else {
            PoseCue::Clip {
                name: clip.to_string(),
                once: !looped,
                paused: false,
            }
        };
        if let Some(entry) = self
            .animation_overrides
            .iter_mut()
            .rev()
            .find(|(id, _)| id == instance_id)
        {
            entry.1 = next;
        } else {
            self.animation_overrides
                .push((instance_id.to_string(), next));
        }
        if let Some(handle) = self.handle_of(instance_id)
            && let Some(animation) = self.components.animations.get_mut(handle)
        {
            clip.clone_into(&mut animation.clip);
            animation.looped = looped;
            animation.playing = true;
        }
    }

    // ---- sequences -----------------------------------------------------

    fn start_sequence(&mut self, id: &str, sequence: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let Some(def) = self.sequences.get(sequence).cloned() else {
            return false;
        };
        if self.sequence_runs.len() >= MAX_ACTIVE_SEQUENCES
            && !self.sequence_runs.iter().any(|run| run.owner == handle)
        {
            warn_once(
                "sequence-budget",
                format!(
                    "[entities] sequence `{sequence}` refused: {MAX_ACTIVE_SEQUENCES} sequences \
                     are already running"
                ),
            );
            return false;
        }
        self.sequence_runs.retain(|run| run.owner != handle);
        self.sequence_runs.push(SequenceRuntime::new(&def, handle));
        if let Some(control) = self.components.sequences.get_mut(handle) {
            control.sequence = def.id;
            control.running = true;
        } else {
            self.components.sequences.insert(
                handle,
                SequenceCtl {
                    sequence: def.id,
                    running: true,
                },
            );
        }
        true
    }

    fn stop_sequence(&mut self, id: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let mut stopped = false;
        for run in &mut self.sequence_runs {
            if run.owner == handle && !run.finished && !run.stopped {
                run.stop();
                stopped = true;
            }
        }
        if let Some(control) = self.components.sequences.get_mut(handle) {
            control.running = false;
        }
        stopped
    }

    /// Advances every running sequence by as many steps as it can complete.
    #[allow(clippy::too_many_lines)] // one cohesive step machine
    fn tick_sequences(&mut self, ctx: &WorldContext<'_>, tick: &mut WorldTick) {
        if self.sequence_runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.sequence_runs);
        let mut keep: Vec<SequenceRuntime> = Vec::with_capacity(runs.len());
        for mut run in runs {
            let Some(def) = self.sequences.get(&run.sequence).cloned() else {
                continue;
            };
            if run.finished || run.stopped || !self.store.contains(run.owner) {
                continue;
            }
            run.step_time += ctx.delta_seconds;
            let mut steps = 0usize;
            loop {
                steps = steps.saturating_add(1);
                let Some(step) = run.current(&def).cloned() else {
                    run.finished = true;
                    break;
                };
                match step {
                    SequenceStepDef::Action { action } => {
                        if run.step_ran {
                            break;
                        }
                        run.step_ran = true;
                        let mut report = DispatchReport::default();
                        self.run_action(&action, Some(run.owner), &mut report, 1);
                        tick.actions_run = tick.actions_run.saturating_add(report.actions_run);
                        tick.missing_targets =
                            tick.missing_targets.saturating_add(report.missing_targets);
                        tick.unsupported = tick.unsupported.saturating_add(report.unsupported);
                        run.advance(&def);
                    }
                    SequenceStepDef::Wait { seconds } => {
                        if run.step_time >= seconds {
                            run.advance(&def);
                        } else {
                            break;
                        }
                    }
                    SequenceStepDef::Move { x, y, z, speed } => {
                        let target = Vec3::new(x, y.unwrap_or(f32::NAN), z);
                        if self.sequence_move_step(&mut run, target, speed, ctx) {
                            run.advance(&def);
                        } else {
                            break;
                        }
                    }
                    SequenceStepDef::Face { yaw_degrees } => {
                        if self.sequence_face_step(&run, yaw_degrees, ctx.delta_seconds) {
                            run.advance(&def);
                        } else {
                            break;
                        }
                    }
                    SequenceStepDef::WaitAnimation { clip, timeout } => {
                        let complete = run.animation_complete
                            || (timeout > 0.0 && run.step_time >= timeout)
                            || clip.as_deref().is_some_and(|clip| {
                                self.components
                                    .animations
                                    .get(run.owner)
                                    .is_some_and(|animation| {
                                        !animation.playing
                                            && animation.clip == clip
                                            && animation.progress >= 1.0
                                    })
                            });
                        if complete {
                            run.advance(&def);
                        } else {
                            break;
                        }
                    }
                    SequenceStepDef::Emit { on, key } => {
                        if run.step_ran {
                            break;
                        }
                        run.step_ran = true;
                        self.emit_depth(
                            EventKind::parse(on),
                            run.owner,
                            key.as_deref().unwrap_or_default(),
                            Some(run.owner),
                            1,
                        );
                        run.advance(&def);
                    }
                    SequenceStepDef::SetState { name, value } => {
                        if run.step_ran {
                            break;
                        }
                        run.step_ran = true;
                        if let Some(id) = self.id_of(run.owner).map(str::to_string) {
                            self.emit_state(&id, &name, value, Some(run.owner), 1);
                        }
                        run.advance(&def);
                    }
                    SequenceStepDef::Stop => {
                        run.stop();
                        run.finished = true;
                        break;
                    }
                }
                if steps >= 64 || run.finished || run.stopped {
                    break;
                }
            }
            if run.finished && !run.stopped {
                if let Some(control) = self.components.sequences.get_mut(run.owner) {
                    control.running = false;
                }
                self.emit_depth(
                    EventKind::SequenceComplete,
                    run.owner,
                    "",
                    Some(run.owner),
                    1,
                );
                tick.sequences_completed = tick.sequences_completed.saturating_add(1);
            } else if !run.stopped {
                keep.push(run);
            }
        }
        self.sequence_runs = keep;
    }

    /// One `move` step of a running sequence. Returns true when it moved on.
    #[allow(clippy::arithmetic_side_effects)] // bounded flat movement arithmetic
    fn sequence_move_step(
        &mut self,
        run: &mut SequenceRuntime,
        target: Vec3,
        speed: f32,
        ctx: &WorldContext<'_>,
    ) -> bool {
        if !target.x.is_finite() || !target.z.is_finite() || !speed.is_finite() || speed <= 0.0 {
            return true;
        }
        let Some(transform) = self.components.transforms.get(run.owner).copied() else {
            return true;
        };
        let here = transform.position;
        let flat = glam::Vec2::new(target.x - here.x, target.z - here.z);
        let distance = flat.length();
        if !distance.is_finite() {
            return true;
        }
        if distance <= sequences::SEQUENCE_ARRIVE_EPS_M {
            return true;
        }
        let direction = flat / distance;
        let travel = (speed * ctx.delta_seconds).min(distance);
        let candidate = glam::Vec2::new(here.x, here.z) + direction * travel;
        let (radius, height) = self.body_of(run.owner);
        let resolved = resolve_player_collision_for_body_indexed(
            ctx.index, candidate, radius, here.y, height, ctx.walls,
        );
        if (resolved - candidate).length() > crate::entity::ENTITY_BLOCK_EPS_M {
            warn_once(
                "sequence-move-blocked",
                format!(
                    "[entities] sequence `{}` is blocked by a wall",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        }
        let Some(floor_y) = ctx.floor.walk_height_at(resolved.x, resolved.y) else {
            warn_once(
                "sequence-move-void",
                format!(
                    "[entities] sequence `{}` leaves every walkable floor",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        };
        if (floor_y - here.y).abs() > ENTITY_STEP_HEIGHT_M + crate::collision::STEP_EPS {
            warn_once(
                "sequence-move-step",
                format!(
                    "[entities] sequence `{}` cannot climb the step on its path",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        }
        let next = Vec3::new(resolved.x, floor_y, resolved.y);
        if let Some(entry) = self.components.transforms.get_mut(run.owner) {
            entry.position = next;
        }
        if let Some(key) = self.dynamic_key(run.owner) {
            self.commands.push(WorldCommand::SetDynamicTransform {
                entity: key,
                position: next,
                yaw_degrees: transform.yaw_degrees,
            });
        }
        false
    }

    /// One `face` step. Returns true when the entity reached the yaw.
    ///
    /// The turn is delta-scaled, exactly like a route's `face` step, so the
    /// authored degrees-per-second rate holds at any frame rate.
    fn sequence_face_step(
        &mut self,
        run: &SequenceRuntime,
        yaw_degrees: f32,
        delta_seconds: f32,
    ) -> bool {
        if !yaw_degrees.is_finite() {
            return true;
        }
        let Some(transform) = self.components.transforms.get(run.owner).copied() else {
            return true;
        };
        let target = yaw_degrees.to_radians();
        let current = transform.yaw_degrees.to_radians();
        if angle_difference(current, target).abs() <= ENTITY_FACE_EPS_RAD {
            return true;
        }
        let delta = if delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, crate::game::MAX_SIM_DELTA)
        } else {
            0.0
        };
        let step = ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * delta;
        let next = turn_toward(current, target, step);
        if let Some(entry) = self.components.transforms.get_mut(run.owner) {
            entry.yaw_degrees = next.to_degrees();
        }
        false
    }

    /// The authored water-volume index of a water entity, when it has one.
    fn water_volume_index(&self, handle: EntityHandle) -> Option<u32> {
        self.components.water.get(handle)?;
        let id = self.id_of(handle)?;
        let index = id.strip_prefix("water_")?.parse::<u32>().ok()?;
        index.checked_sub(1)
    }

    /// The movement radius and body height of an entity.
    fn body_of(&self, handle: EntityHandle) -> (f32, f32) {
        self.components
            .colliders
            .get(handle)
            .map_or((ENTITY_MOVE_RADIUS_M, 1.0), |collider| {
                (
                    (collider.size[0].min(collider.size[2]) * 0.5).clamp(ENTITY_MIN_RADIUS_M, 0.5),
                    collider.size[1].clamp(0.1, 2.0),
                )
            })
    }

    // ---- timers and lifetimes -----------------------------------------

    fn tick_timers(&mut self, delta: f32, _tick: &mut WorldTick) {
        if self.timers.is_empty() {
            return;
        }
        let mut fires: Vec<usize> = Vec::new();
        self.timers.tick(delta, |index| fires.push(index));
        for index in fires {
            let Some(runtime) = self.timers.at(index) else {
                continue;
            };
            let id = runtime.def.id.clone();
            let Some(handle) = self.handle_of(&id) else {
                continue;
            };
            self.emit_depth(EventKind::Timer, handle, &id, None, 1);
        }
    }

    fn tick_lifetimes(&mut self, delta: f32, tick: &mut WorldTick) {
        if self.components.lifetimes.is_empty() {
            return;
        }
        let mut updates: Vec<(EntityHandle, f32)> = Vec::new();
        let mut expired: Vec<EntityHandle> = Vec::new();
        for (handle, lifetime) in self.components.lifetimes.iter() {
            let Some(remaining) = lifetime.remaining else {
                continue;
            };
            let next = remaining - delta;
            if next <= 0.0 {
                expired.push(handle);
            } else {
                updates.push((handle, next));
            }
        }
        for (handle, next) in updates {
            if let Some(entry) = self.components.lifetimes.get_mut(handle) {
                entry.remaining = Some(next);
            }
        }
        for handle in expired {
            if self.despawn_handle(handle) {
                tick.despawned = tick.despawned.saturating_add(1);
            }
        }
    }

    fn tick_bindings(&mut self, delta: f32) {
        if delta <= 0.0 {
            return;
        }
        for binding in &mut self.bindings {
            binding.cooldown_remaining = (binding.cooldown_remaining - delta).max(0.0);
        }
    }

    #[allow(clippy::arithmetic_side_effects)] // bounded flat movement arithmetic
    fn tick_moves(&mut self, ctx: &WorldContext<'_>, tick: &mut WorldTick) {
        if self.moves.is_empty() {
            return;
        }
        let goals = std::mem::take(&mut self.moves);
        let mut keep: Vec<MoveGoal> = Vec::with_capacity(goals.len());
        for goal in goals {
            let Some(transform) = self.components.transforms.get(goal.entity).copied() else {
                continue;
            };
            let here = transform.position;
            let flat = glam::Vec2::new(goal.target.x - here.x, goal.target.z - here.z);
            let distance = flat.length();
            if !distance.is_finite() {
                continue;
            }
            if distance <= sequences::SEQUENCE_ARRIVE_EPS_M {
                continue;
            }
            let direction = flat / distance;
            let travel = (goal.speed * ctx.delta_seconds).min(distance);
            let candidate = glam::Vec2::new(here.x, here.z) + direction * travel;
            let (radius, height) = self.body_of(goal.entity);
            let resolved = resolve_player_collision_for_body_indexed(
                ctx.index, candidate, radius, here.y, height, ctx.walls,
            );
            if (resolved - candidate).length() > crate::entity::ENTITY_BLOCK_EPS_M {
                warn_once(
                    "move-blocked",
                    format!(
                        "[entities] `{}` cannot reach its move target: a wall blocks the path",
                        self.id_of(goal.entity).unwrap_or("entity")
                    ),
                );
                continue;
            }
            let next = Vec3::new(resolved.x, here.y, resolved.y);
            if let Some(entry) = self.components.transforms.get_mut(goal.entity) {
                entry.position = next;
            }
            if let Some(key) = self.dynamic_key(goal.entity) {
                self.commands.push(WorldCommand::SetDynamicTransform {
                    entity: key,
                    position: next,
                    yaw_degrees: transform.yaw_degrees,
                });
            }
            tick.frames_dirty = true;
            keep.push(goal);
        }
        self.moves = keep;
    }

    // ---- events --------------------------------------------------------

    fn pump_events(&mut self, tick: &mut WorldTick) {
        let mut depth = 0u32;
        while !self.events.is_empty() {
            if depth >= MAX_CHAIN_DEPTH {
                let dropped = self.events.len();
                tick.events_dropped = tick.events_dropped.saturating_add(dropped);
                warn_once(
                    "event-chain-depth",
                    format!(
                        "[entities] event chain reached the depth limit ({MAX_CHAIN_DEPTH}); \
                         {dropped} event(s) dropped this tick"
                    ),
                );
                self.events.clear();
                break;
            }
            let wave = self.events.len();
            let mut processed = 0usize;
            for _ in 0..wave {
                if tick.events_processed >= MAX_EVENTS_PER_TICK {
                    let dropped = self.events.len();
                    tick.events_dropped = tick.events_dropped.saturating_add(dropped);
                    warn_once(
                        "event-budget",
                        format!(
                            "[entities] event budget ({MAX_EVENTS_PER_TICK}/tick) exhausted; \
                             {dropped} event(s) dropped this tick"
                        ),
                    );
                    self.events.clear();
                    break;
                }
                let Some(event) = self.events.pop() else {
                    break;
                };
                tick.events_processed = tick.events_processed.saturating_add(1);
                processed = processed.saturating_add(1);
                if event.generation != self.generation || !self.store.contains(event.subject) {
                    continue;
                }
                self.run_bindings(&event, tick, event.depth);
            }
            if processed == 0 {
                break;
            }
            depth = depth.saturating_add(1);
        }
        self.compact_bindings();
    }

    fn run_bindings(&mut self, event: &EventRecord, tick: &mut WorldTick, depth: u8) {
        let mut matched: Vec<usize> = Vec::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            if binding.owner != event.subject || binding.on != event.kind {
                continue;
            }
            if let Some(key) = &binding.key
                && key != &event.key
            {
                continue;
            }
            matched.push(index);
        }
        for index in matched {
            let Some(binding) = self.bindings.get(index).cloned() else {
                continue;
            };
            if binding.fired && binding.once {
                continue;
            }
            if binding.cooldown_remaining > 0.0 {
                continue;
            }
            if !binding
                .when
                .iter()
                .all(|condition| self.condition_holds(condition))
            {
                continue;
            }
            let mut report = DispatchReport::default();
            for action in &binding.actions {
                self.run_action(action, Some(event.subject), &mut report, depth);
                if report.player_reset {
                    tick.player_reset = true;
                    break;
                }
            }
            tick.actions_run = tick.actions_run.saturating_add(report.actions_run);
            tick.spawned = tick.spawned.saturating_add(report.spawned);
            tick.despawned = tick.despawned.saturating_add(report.despawned);
            tick.missing_targets = tick.missing_targets.saturating_add(report.missing_targets);
            tick.unsupported = tick.unsupported.saturating_add(report.unsupported);
            tick.labels_shown = tick.labels_shown.saturating_add(report.labels_shown);
            tick.labels_hidden = tick.labels_hidden.saturating_add(report.labels_hidden);
            if report.animations_started > 0 {
                tick.frames_dirty = true;
            }
            if let Some(entry) = self.bindings.get_mut(index) {
                entry.fired = true;
                entry.cooldown_remaining = binding.cooldown_seconds;
            }
            if tick.player_reset {
                return;
            }
        }
    }

    /// Evaluates one condition against the current world.
    #[must_use]
    pub fn condition_holds(&self, condition: &ConditionDef) -> bool {
        events::evaluate(condition, &WorldConditionView { world: self })
    }

    fn compact_bindings(&mut self) {
        let store = &self.store;
        if self
            .bindings
            .iter()
            .all(|binding| store.contains(binding.owner))
        {
            return;
        }
        self.bindings
            .retain(|binding| store.contains(binding.owner));
    }

    // ---- spawns --------------------------------------------------------

    #[allow(clippy::too_many_lines)] // one cohesive spawn validation path
    fn spawn_entity(
        &mut self,
        point: Option<&str>,
        template: Option<&str>,
        group: Option<&str>,
        name: Option<&str>,
        depth: u8,
    ) -> Result<EntityHandle, SpawnError> {
        let point_id = point
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or(SpawnError::MissingTarget)?;
        let Some(authored_point) = self
            .spawn_points
            .iter()
            .find(|candidate| candidate.id == point_id)
            .cloned()
        else {
            return Err(SpawnError::MissingTarget);
        };
        let template_id = template
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(authored_point.template.as_str());
        let Some(template) = self
            .templates
            .iter()
            .find(|candidate| candidate.id == template_id)
            .cloned()
        else {
            return Err(SpawnError::MissingTarget);
        };
        let group_id = group
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .or_else(|| authored_point.group.clone());
        let group_index = match group_id.as_deref() {
            Some(id) => Some(
                self.spawn_groups
                    .index_of(id)
                    .ok_or(SpawnError::MissingTarget)?,
            ),
            None => None,
        };
        if let Some(index) = group_index
            && !self.spawn_groups.admits(index)
        {
            let group = self
                .spawn_groups
                .at(index)
                .map_or("?", |group| group.def.id.as_str());
            return Err(SpawnError::Refused(format!(
                "spawn group `{group}` already has a live member"
            )));
        }
        if self.live_spawns.len() >= spawn::MAX_LIVE_SPAWNS {
            return Err(SpawnError::Refused(format!(
                "the live-spawn budget ({}) is exhausted",
                spawn::MAX_LIVE_SPAWNS
            )));
        }
        if self.spawns_this_tick >= spawn::MAX_SPAWNS_PER_TICK {
            return Err(SpawnError::Refused(format!(
                "the per-tick spawn budget ({}) is exhausted",
                spawn::MAX_SPAWNS_PER_TICK
            )));
        }
        if template.model.trim().is_empty() || !template.scale.is_finite() || template.scale <= 0.0
        {
            return Err(SpawnError::Refused(
                "the template has no usable model or scale".to_string(),
            ));
        }
        let base_y = authored_point
            .y
            .filter(|y| y.is_finite())
            .unwrap_or_else(|| {
                self.floor
                    .walk_height_at(authored_point.x, authored_point.z)
                    .unwrap_or(0.0)
            });
        let runtime_name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(
                || {
                    format!(
                        "{}#{}",
                        authored_point.id,
                        self.live_spawns.len().saturating_add(1)
                    )
                },
                str::to_string,
            );
        if self.names.contains(&runtime_name) {
            return Err(SpawnError::Refused(format!(
                "runtime name `{runtime_name}` is already live"
            )));
        }
        let handle = self.store.insert();
        let _ = self
            .names
            .insert(EntityId::new(runtime_name.clone()), handle);
        self.components.transforms.insert(
            handle,
            Transform {
                position: Vec3::new(authored_point.x, base_y, authored_point.z),
                yaw_degrees: authored_point.yaw_degrees,
                scale: template.scale,
            },
        );
        self.components.renderables.insert(
            handle,
            Renderable {
                model: template.model.clone(),
                material: None,
                visible: true,
                dynamic: true,
            },
        );
        if let Some(seconds) = template
            .lifetime_seconds
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        {
            self.components.lifetimes.insert(
                handle,
                Lifetime {
                    remaining: Some(seconds),
                    despawn: true,
                },
            );
        }
        self.apply_component_defs(handle, &template.components);
        self.resolve_bindings_for(handle, &template.bindings);
        self.resolve_bindings_for(handle, &authored_point.bindings);
        self.spawns_this_tick = self.spawns_this_tick.saturating_add(1);
        let key = self.next_dynamic_key;
        self.next_dynamic_key = self.next_dynamic_key.saturating_add(1);
        self.dynamic_keys.push((handle, key));
        self.live_spawns.push(handle);
        if let Some(index) = group_index {
            let _ = self.spawn_groups.occupy(index, handle);
        }
        self.commands.push(WorldCommand::SpawnDynamic {
            entity: key,
            model: template.model.clone(),
            position: Vec3::new(authored_point.x, base_y, authored_point.z),
            yaw_degrees: authored_point.yaw_degrees,
            scale: template.scale,
        });
        self.emit_depth(
            EventKind::Spawn,
            handle,
            &runtime_name,
            Some(handle),
            depth.saturating_add(1),
        );
        self.register_spawned_ai(handle, &runtime_name);
        self.note_stimulus(
            Vec3::new(authored_point.x, base_y, authored_point.z),
            7.0,
            "spawn",
            Some(handle),
        );
        Ok(handle)
    }

    fn despawn_target(&mut self, target: &str) -> bool {
        if target.is_empty() {
            return false;
        }
        if let Some(handle) = self.handle_of(target) {
            return self.despawn_handle(handle);
        }
        if let Some(index) = self.spawn_groups.index_of(target)
            && let Some(handle) = self.spawn_groups.at(index).and_then(|group| group.live)
        {
            return self.despawn_handle(handle);
        }
        false
    }

    /// Removes one runtime entity and every trace of it.
    fn despawn_handle(&mut self, handle: EntityHandle) -> bool {
        if !self.store.contains(handle) {
            return false;
        }
        let id = self.id_of(handle).map(str::to_string);
        let dynamic = self
            .components
            .renderables
            .get(handle)
            .is_some_and(|renderable| renderable.dynamic);
        if !dynamic {
            warn_once(
                "despawn-static",
                format!(
                    "[entities] `{}` is baked static geometry and cannot be despawned",
                    id.as_deref().unwrap_or("entity")
                ),
            );
            return false;
        }
        if let Some(key) = self.dynamic_key(handle) {
            self.commands.push(WorldCommand::DespawnDynamic {
                entity: key,
                instance_id: id.clone().unwrap_or_default(),
            });
        }
        self.sequence_runs.retain(|run| run.owner != handle);
        self.moves.retain(|goal| goal.entity != handle);
        self.dynamic_keys
            .retain(|(candidate, _)| *candidate != handle);
        self.live_spawns.retain(|candidate| *candidate != handle);
        self.spawn_groups.release(handle);
        self.ai.remove(handle);
        if let Some(id) = id {
            self.names.remove(&id);
        }
        self.components.remove_all(handle);
        self.rebuild_interactables_from_tables();
        self.store.remove(handle)
    }

    /// The render key of a runtime-spawned entity, if it has one.
    #[must_use]
    pub fn dynamic_key(&self, handle: EntityHandle) -> Option<u64> {
        self.dynamic_keys
            .iter()
            .find(|(candidate, _)| *candidate == handle)
            .map(|(_, key)| *key)
    }

    /// The entity a runtime render key belongs to, if it is still alive.
    #[must_use]
    pub fn handle_for_dynamic_key(&self, key: u64) -> Option<EntityHandle> {
        let handle = self
            .dynamic_keys
            .iter()
            .find(|(_, candidate)| *candidate == key)
            .map(|(handle, _)| *handle)?;
        self.store.contains(handle).then_some(handle)
    }

    /// The authored or runtime id of an entity, when it is alive.
    #[must_use]
    pub fn instance_id_of(&self, handle: EntityHandle) -> Option<&str> {
        self.id_of(handle)
    }

    /// A stable non-zero key for a non-dynamic entity (audio emitters).
    fn stable_key(&self, handle: EntityHandle) -> Option<u64> {
        self.store.contains(handle).then(|| {
            (u64::from(handle.index()) << 32) | u64::from(handle.generation()) | (1_u64 << 63)
        })
    }

    // ---- tick ----------------------------------------------------------

    /// Advances the whole runtime by one frame.
    pub fn tick(&mut self, ctx: &WorldContext<'_>) -> WorldTick {
        let mut tick = WorldTick::default();
        self.spawns_this_tick = 0;
        self.sim_time += ctx.delta_seconds;
        self.tick_bindings(ctx.delta_seconds);
        self.tick_timers(ctx.delta_seconds, &mut tick);
        self.tick_sequences(ctx, &mut tick);
        self.tick_ai(ctx, &mut tick);
        self.tick_moves(ctx, &mut tick);
        self.tick_lifetimes(ctx.delta_seconds, &mut tick);
        self.pump_events(&mut tick);
        self.report_queue_refusals(&mut tick);
        if tick.frames_dirty {
            // A binding or sequence cued an animation this tick: the renderer
            // handoff is rebuilt even when the level authors no routes.
            self.rebuild_entity_frames();
        }
        tick
    }

    /// Advances the shared AI runtime: perception, decisions, locomotion,
    /// catch detection and the stimuli and commands the tick produced.
    fn tick_ai(&mut self, ctx: &WorldContext<'_>, tick: &mut WorldTick) {
        self.age_stimuli(ctx.delta_seconds);
        if self.ai.is_empty() {
            return;
        }
        let targets: Vec<AiTarget> = self
            .ai
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect();
        let scripted: Vec<EntityHandle> = self.sequence_runs.iter().map(|run| run.owner).collect();
        let stimuli: Vec<AiStimulus> = self.stimuli.iter().cloned().collect();
        let ai_ctx = AiTickContext {
            delta: ctx.delta_seconds,
            sim_time: self.sim_time,
            nav: ctx.nav,
            doors: &self.doors,
            door_version: self.doors.version(),
            walls: ctx.walls,
            index: ctx.index,
            floor: ctx.floor,
            leaves: &self.door_colliders,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &scripted,
        };
        let mut outcome = AiOutcome::default();
        self.ai.tick(&ai_ctx, &mut outcome);
        for (handle, position, yaw_degrees, spawned) in outcome.moved {
            if let Some(transform) = self.components.transforms.get_mut(handle) {
                transform.position = position;
                transform.yaw_degrees = yaw_degrees;
            }
            if spawned && let Some(key) = self.dynamic_key(handle) {
                self.commands.push(WorldCommand::SetDynamicTransform {
                    entity: key,
                    position,
                    yaw_degrees,
                });
            }
            tick.frames_dirty = true;
        }
        for (kind, subject, key, actor) in outcome.events {
            self.emit_depth(kind, subject, &key, actor, 0);
        }
        for (door_id, _agent) in outcome.door_requests {
            if let Some(index) = self.doors.index_of(&door_id)
                && let Some(door) = self.doors.get(index)
            {
                self.note_stimulus(
                    Vec3::new(door.def.x, door.base_y(), door.def.z),
                    5.0,
                    "door",
                    self.handle_of(&door_id),
                );
            }
            let _ = self.doors.request_open(&door_id);
        }
        for stimulus in outcome.stimuli {
            self.push_stimulus(stimulus);
        }
        tick.events_processed = tick
            .events_processed
            .saturating_add(outcome.transitions.min(MAX_EVENTS_PER_TICK));
    }

    /// Ages every queued stimulus and drops the stale ones.
    fn age_stimuli(&mut self, delta: f32) {
        for stimulus in &mut self.stimuli {
            stimulus.age += delta;
        }
        while self
            .stimuli
            .front()
            .is_some_and(|stimulus| !stimulus.is_fresh(0.0))
        {
            let _ = self.stimuli.pop_front();
        }
        while self.stimuli.len() > crate::ai::perception::MAX_STIMULI {
            let _ = self.stimuli.pop_front();
        }
    }

    /// Publishes one gameplay stimulus for AI hearing.
    pub fn push_stimulus(&mut self, stimulus: AiStimulus) {
        if !stimulus.position.is_finite()
            || !stimulus.radius.is_finite()
            || stimulus.radius <= 0.0
            || !stimulus.loudness.is_finite()
            || stimulus.loudness <= 0.0
        {
            return;
        }
        while self.stimuli.len() >= crate::ai::perception::MAX_STIMULI {
            let _ = self.stimuli.pop_front();
        }
        self.stimuli.push_back(stimulus);
    }

    /// Convenience publisher for a stimulus at a position.
    pub fn note_stimulus(
        &mut self,
        position: Vec3,
        radius: f32,
        category: &str,
        source: Option<EntityHandle>,
    ) {
        self.push_stimulus(AiStimulus {
            position,
            radius,
            loudness: 1.0,
            category: category.to_string(),
            source,
            age: 0.0,
        });
    }

    /// The shared AI runtime.
    #[must_use]
    pub const fn ai(&self) -> &AiWorld {
        &self.ai
    }

    /// The shared AI runtime, mutable (tests and scripted encounters).
    pub const fn ai_mut(&mut self) -> &mut AiWorld {
        &mut self.ai
    }

    /// Every live stimulus.
    #[must_use]
    pub fn stimuli(&self) -> Vec<AiStimulus> {
        self.stimuli.iter().cloned().collect()
    }

    /// Simulation seconds since the world was resolved.
    #[must_use]
    pub const fn sim_time(&self) -> f32 {
        self.sim_time
    }

    /// How many authored sequences are running right now.
    #[must_use]
    pub const fn active_sequences(&self) -> usize {
        self.sequence_runs.len()
    }

    /// Folds every event-queue refusal since the last report into the tick,
    /// so a producer that ran before the pump (a volume edge, a timer, a
    /// sequence step, an interaction) is never silently dropped.
    fn report_queue_refusals(&mut self, tick: &mut WorldTick) {
        let dropped = self.events.dropped();
        let refused = dropped.saturating_sub(self.reported_drops);
        if refused == 0 {
            return;
        }
        self.reported_drops = dropped;
        tick.events_dropped = tick.events_dropped.saturating_add(refused);
        warn_once(
            "event-queue-full",
            format!(
                "[entities] the event queue refused {refused} record(s) this tick; the \
                 producers' work was dropped and reported"
            ),
        );
    }

    /// Re-baselines every trigger volume's occupancy from `feet` without
    /// emitting an enter or exit event.
    ///
    /// A level load and a teleport use this so the volume the player lands in
    /// is simply the baseline, never a fabricated crossing.
    pub fn seed_volumes(&mut self, feet: Vec3) {
        let updates: Vec<(EntityHandle, bool)> = self
            .components
            .volumes
            .iter()
            .map(|(handle, volume)| (handle, volume.contains(feet.x, feet.z, feet.y)))
            .collect();
        for (handle, inside) in updates {
            if let Some(volume) = self.components.volumes.get_mut(handle) {
                volume.inside = inside;
            }
        }
    }

    /// Alias of [`Self::seed_volumes`] for the stance-animation path, where
    /// the feet move without locomotion.
    pub fn reseed_volumes(&mut self, feet: Vec3) {
        self.seed_volumes(feet);
    }

    /// Updates every trigger volume against the player's swept feet segment.
    pub fn update_volumes(&mut self, from: Vec3, to: Vec3) {
        if self.components.volumes.is_empty() {
            return;
        }
        let mut edges: Vec<(EntityHandle, bool, bool)> = Vec::new();
        for (handle, volume) in self.components.volumes.iter() {
            // The point decides occupancy; the swept segment only catches an
            // entry that happened between two frames (a fast fall through a
            // thin band). Leaving uses the point, so a segment that merely
            // grazes the volume on the way out is not a re-entry.
            let point_inside = volume.contains(to.x, to.z, to.y);
            let swept = segment_overlaps_aabb(
                from,
                to,
                [volume.bounds[0], volume.bottom_y, volume.bounds[2]],
                [volume.bounds[1], volume.top_y, volume.bounds[3]],
            );
            let entered = !volume.inside && (point_inside || swept);
            let exited = volume.inside && !point_inside;
            if entered || exited {
                edges.push((handle, point_inside, entered));
            }
        }
        for (handle, point_inside, entered) in edges {
            if let Some(entry) = self.components.volumes.get_mut(handle) {
                // Occupancy tracks the point; a swept entry that ends outside
                // the band still counts as one entry edge.
                entry.inside = point_inside;
            }
            self.emit(
                if entered {
                    EventKind::EnterVolume
                } else {
                    EventKind::ExitVolume
                },
                handle,
                "",
                None,
            );
        }
    }

    /// Advances every moving door and republishes its collider and aim bound.
    pub fn update_doors(&mut self, ctx: &WorldContext<'_>) -> usize {
        if !self.doors.any_moving() {
            return 0;
        }
        let player = ctx.feet;
        let body = ctx.body_height;
        let index = ctx.index;
        let walls = ctx.walls;
        let moved = self.doors.advance(ctx.delta_seconds, |_, candidate| {
            door_pose_hits_player(candidate, player, body)
                || door_pose_hits_static(index, walls, candidate)
        });
        if moved > 0 {
            self.rebuild_door_colliders();
            self.sync_door_interactables();
        }
        moved
    }

    /// Notifies the world that one character finished a one-shot animation.
    ///
    /// `clip` is the clip that completed; an empty name falls back to the
    /// entity's `animation` component, so an `animation_complete` binding
    /// filtered by clip matches the live event.
    pub fn notify_animation_complete(&mut self, instance_id: &str, clip: &str) {
        let Some(handle) = self.handle_of(instance_id) else {
            return;
        };
        let clip = if clip.trim().is_empty() {
            // The renderer reports completion by instance id, not clip name.
            // The authoritative name is the entity's `animation` component
            // when it has one, otherwise the cue the entity runtime cued last
            // (`play_animation`), so a sequence can wait for a named clip.
            self.components
                .animations
                .get(handle)
                .map(|animation| animation.clip.clone())
                .or_else(|| {
                    self.animation_override(instance_id)
                        .and_then(PoseCue::clip_name)
                        .map(str::to_string)
                })
                .unwrap_or_default()
        } else {
            clip.to_string()
        };
        if let Some(animation) = self.components.animations.get_mut(handle) {
            animation.playing = false;
            animation.progress = 1.0;
        }
        for run in &mut self.sequence_runs {
            if run.owner != handle {
                continue;
            }
            let waiting = self
                .sequences
                .get(&run.sequence)
                .and_then(|def| run.current(def))
                .is_some_and(|step| match step {
                    SequenceStepDef::WaitAnimation {
                        clip: Some(wait_clip),
                        ..
                    } => *wait_clip == clip,
                    SequenceStepDef::WaitAnimation { clip: None, .. } => true,
                    SequenceStepDef::Action { .. }
                    | SequenceStepDef::Wait { .. }
                    | SequenceStepDef::Move { .. }
                    | SequenceStepDef::Face { .. }
                    | SequenceStepDef::Emit { .. }
                    | SequenceStepDef::SetState { .. }
                    | SequenceStepDef::Stop => false,
                });
            if waiting {
                run.animation_complete = true;
            }
        }
        self.emit(EventKind::AnimationComplete, handle, &clip, Some(handle));
    }

    /// Advances every authored route and republishes the entity frames.
    #[allow(clippy::arithmetic_side_effects)] // bounded pose comparisons
    pub fn update_entities(&mut self, delta: f32, world: &RouteWorld<'_>) {
        if self.routes.is_empty() {
            return;
        }
        let mut moved = false;
        for index in 0..self.routes.routes().len() {
            let Some(route) = self.routes.routes().get(index) else {
                continue;
            };
            let Some(state) = self.route_states.get_mut(index) else {
                continue;
            };
            let before = state.position;
            let before_yaw = state.yaw;
            route.advance(state, delta, world);
            if (state.position - before).length_squared() > f32::EPSILON
                || (state.yaw - before_yaw).abs() > f32::EPSILON
                || state.blocked
            {
                moved = true;
            }
        }
        if moved {
            self.sync_routed_interactables();
        }
        self.rebuild_entity_frames();
    }

    /// Rebuilds the per-frame character handoff.
    ///
    /// A routed entity's frame carries its live transform and pose; an
    /// animation override that names a non-routed placed instance (an animated
    /// prop the renderer claims as a rigid character, such as a wall switch)
    /// gets a cue-only frame, exactly as the pre-runtime handoff did, so
    /// `play_animation`/`toggle_animation` still reach it.
    pub fn rebuild_entity_frames(&mut self) {
        self.entity_frames.clear();
        // AI agents own their locomotion: their frame carries the live
        // transform and the state-driven pose, and wins over a route with the
        // same id (validation rejects authoring both).
        for agent in self.ai.agents() {
            // A scripted performance (a sequence's `play_animation`) owns the
            // pose while the agent is frozen in a catch or scripted state;
            // once ordinary locomotion resumes, the state-driven gait wins so
            // a finished one-shot cannot pin the agent in a held pose.
            let cue = if agent.frozen() {
                self.animation_override(&agent.instance_id)
                    .cloned()
                    .unwrap_or_else(|| agent.cue())
            } else {
                agent.cue()
            };
            self.entity_frames.push(EntityFrame {
                instance_id: agent.instance_id.clone(),
                transform: Some((agent.position, agent.yaw_degrees)),
                cue,
            });
        }
        let ai_owned: Vec<&str> = self
            .ai
            .agents()
            .iter()
            .map(|agent| agent.instance_id.as_str())
            .collect();
        for (index, route) in self.routes.routes().iter().enumerate() {
            if ai_owned.contains(&route.instance_id.as_str()) {
                continue;
            }
            let Some(state) = self.route_states.get(index) else {
                continue;
            };
            let cue = self
                .animation_override(&route.instance_id)
                .cloned()
                .unwrap_or_else(|| state.cue.clone());
            self.entity_frames.push(EntityFrame {
                instance_id: route.instance_id.clone(),
                transform: Some((state.position, state.yaw)),
                cue,
            });
        }
        let routed: Vec<String> = self
            .routes
            .routes()
            .iter()
            .map(|route| route.instance_id.clone())
            .collect();
        for (instance_id, cue) in &self.animation_overrides {
            if routed.iter().any(|id| id == instance_id) {
                continue;
            }
            if ai_owned.contains(&instance_id.as_str()) {
                continue;
            }
            // The renderer matches frames to the characters it claimed; a
            // frame for an instance it does not draw is ignored there.
            self.entity_frames.push(EntityFrame {
                instance_id: instance_id.clone(),
                transform: None,
                cue: cue.clone(),
            });
        }
    }

    /// Republishes every routed interactable's live pose.
    pub fn sync_routed_interactables(&mut self) {
        for (route_index, route) in self.routes.routes().iter().enumerate() {
            let Some(state) = self.route_states.get(route_index) else {
                continue;
            };
            let Some(item_index) = self.interactables.index_of(&route.instance_id) else {
                continue;
            };
            self.interactables
                .set_live_pose(item_index, state.position, state.yaw.to_degrees());
        }
    }

    /// Every navigation agent, for the navigation upgrade.
    #[must_use]
    pub fn nav_agents(&self) -> Vec<(EntityHandle, NavAgent)> {
        self.components
            .nav_agents
            .iter()
            .map(|(handle, agent)| (handle, *agent))
            .collect()
    }

    /// Every navigation obstacle, for the navigation upgrade.
    #[must_use]
    pub fn nav_obstacles(&self) -> Vec<(EntityHandle, NavObstacle)> {
        self.components
            .nav_obstacles
            .iter()
            .map(|(handle, obstacle)| (handle, *obstacle))
            .collect()
    }

    /// The navigation shape of every door leaf, for the navigation upgrade.
    #[must_use]
    pub fn door_blockers(&self) -> Vec<(String, bool, DoorCollider)> {
        self.doors
            .iter()
            .enumerate()
            .map(|(index, door)| {
                let collider = self
                    .door_colliders
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| door.collider());
                let passable = door.phase() == DoorPhase::Open && !door.def.locked;
                (door.def.id.clone(), !passable, collider)
            })
            .collect()
    }
}

/// How a light request resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LightOutcome {
    /// The light changed state.
    Changed,
    /// The request was valid but the state already matched.
    Unchanged,
    /// The target has a light that cannot be switched.
    NotSwitchable,
    /// The target is not a light.
    NotALight,
}

/// Why a spawn did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SpawnError {
    /// A named point, template or group does not exist.
    MissingTarget,
    /// The spawn is well-formed but the world refused it.
    Refused(String),
}

/// The condition view the world answers through.
struct WorldConditionView<'a> {
    world: &'a EntityWorld,
}

impl ConditionView for WorldConditionView<'_> {
    fn state(&self, target: &str, name: &str) -> Option<StateValue> {
        let handle = self.world.handle_of(target)?;
        self.world
            .components
            .states
            .get(handle)
            .and_then(|state| state.get(name).cloned())
    }

    fn enabled(&self, target: &str) -> Option<bool> {
        let handle = self.world.handle_of(target)?;
        if let Some(interactable) = self.world.components.interactables.get(handle) {
            return Some(interactable.enabled);
        }
        if let Some(light) = self.world.components.lights.get(handle) {
            return Some(light.enabled);
        }
        if let Some(steam) = self.world.components.steam.get(handle) {
            return Some(steam.enabled);
        }
        if let Some(water) = self.world.components.water.get(handle) {
            return Some(water.enabled);
        }
        if let Some(animation) = self.world.components.animations.get(handle) {
            return Some(animation.playing);
        }
        if let Some(audio) = self.world.components.audio.get(handle) {
            return Some(audio.enabled);
        }
        None
    }

    fn locked(&self, target: &str) -> Option<bool> {
        self.world.doors.is_locked(target)
    }

    fn door_open(&self, target: &str) -> Option<bool> {
        let index = self.world.doors.index_of(target)?;
        let door = self.world.doors.get(index)?;
        Some(door.phase() == DoorPhase::Open)
    }

    fn sequence_running(&self, target: &str) -> Option<bool> {
        let handle = self.world.handle_of(target)?;
        Some(
            self.world
                .components
                .sequences
                .get(handle)
                .is_some_and(|control| control.running),
        )
    }
}

/// Logs once per process, keyed like the engine's other diagnostics.
fn warn_once(key: &str, message: String) {
    logging::warn_once(format!("entities:{key}"), message);
}

/// True when a candidate door pose would overlap the player's body.
fn door_pose_hits_player(candidate: &DoorCollider, player_feet: Vec3, body_height: f32) -> bool {
    candidate.overlaps_body_y(player_feet.y, body_height)
        && candidate.overlaps_disc(
            player_feet.x,
            player_feet.z,
            crate::collision::PLAYER_RADIUS,
        )
}

/// True when a candidate door pose enters static collision.
fn door_pose_hits_static(
    index: &CollisionIndex,
    walls: &[WallAabb],
    candidate: &DoorCollider,
) -> bool {
    let low = 0.05_f32
        .min(candidate.height * 0.25)
        .mul_add(1.0, candidate.hinge_y);
    let high = (candidate.height - 0.05)
        .max(0.0)
        .mul_add(1.0, candidate.hinge_y);
    let middle = candidate.height.mul_add(0.5, candidate.hinge_y);
    for t in [0.0_f32, 0.5, 1.0] {
        for side in [-1.0_f32, 1.0] {
            let (px, pz) = candidate.point_at(t, side);
            for y in [low, middle, high] {
                let mut blocked = false;
                index.for_each_point(px, pz, walls, |wall| {
                    if blocked {
                        return;
                    }
                    if y > wall.min_y + crate::collision::STEP_EPS
                        && y < wall.max_y - crate::collision::STEP_EPS
                        && px > wall.min_x
                        && px < wall.max_x
                        && pz > wall.min_z
                        && pz < wall.max_z
                    {
                        blocked = true;
                    }
                });
                if blocked {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::collision_index::CollisionIndex;
    use crate::level::LevelDef;

    /// A 20x20 room with the given extra JSON sections.
    fn base_level(extra: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 3,
                "id": "entities_test",
                "name": "Entities Test",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }} ],
                {extra}
            }}"#
        );
        LevelDef::from_json(&json).expect("the test level parses")
    }

    /// One tick with a stationary player at `feet`.
    #[allow(clippy::arithmetic_side_effects)] // test fixture: fixed finite values
    fn tick_at(world: &mut EntityWorld, level: &LevelDef, feet: Vec3, delta: f32) -> WorldTick {
        let walls = level.collision_aabbs();
        let index = CollisionIndex::build(&walls);
        let floor = WalkableFloor::from_level(level);
        let ctx = WorldContext {
            delta_seconds: delta,
            feet_from: feet,
            feet,
            eye: feet + Vec3::Y,
            body_height: 1.8,
            walls: &walls,
            index: &index,
            floor: &floor,
            nav: None,
        };
        world.tick(&ctx)
    }

    #[test]
    fn a_level_resolves_every_authored_object_into_one_entity() {
        let level = base_level(
            r#""props": [
                { "id": "crate", "model": "core:crate", "x": 2.0, "z": 2.0,
                  "components": [ { "component": "interactable", "prompt": "Open" },
                                  { "component": "state", "name": "open", "value": false } ],
                  "bindings": [ { "on": "interact", "actions": [ { "action": "toggle_label" } ] } ] }
            ],
            "doors": [ { "id": "hall_door", "x": 3.0, "z": 3.0, "width": 1.0, "height": 2.1,
                         "components": [ { "component": "interactable" } ],
                         "bindings": [ { "on": "interact", "actions": [ { "action": "toggle" } ] } ] } ],
            "ceiling_lights": [ { "id": "lamp", "fixture": "core:fluorescent_panel_01",
                                  "x": 4.0, "z": 4.0, "switchable": true } ],
            "volumes": [ { "id": "pit", "x": 6.0, "z": 6.0, "width": 1.0, "depth": 1.0,
                           "bottom_y": 0.0, "top_y": 2.0 } ],
            "timers": [ { "id": "warmup", "seconds": 2.0 } ],
            "spawn_templates": [ { "id": "crate_template", "model": "core:crate" } ],
            "spawn_points": [ { "id": "crate_point", "x": 8.0, "z": 8.0,
                                "template": "crate_template" } ],
            "spawn_groups": [ { "id": "crates", "at_most_one_active": true } ]"#,
        );
        let world = EntityWorld::from_level(&level);
        assert_eq!(
            world.entities().len(),
            6,
            "prop, door, fixture, volume, timer, point"
        );
        for id in ["crate", "hall_door", "lamp", "pit", "warmup", "crate_point"] {
            let handle = world.handle_of(id).expect("the entity resolves");
            assert_eq!(world.id_of(handle), Some(id));
        }
        let crate_handle = world.handle_of("crate").expect("crate");
        assert!(world.components().interactables.contains(crate_handle));
        assert!(world.components().states.contains(crate_handle));
        let lamp = world.handle_of("lamp").expect("lamp");
        let light = world.components().lights.get(lamp).expect("light");
        assert!(light.switchable);
        assert_eq!(light.fixture, Some(0));
        assert!(
            world
                .components()
                .volumes
                .contains(world.handle_of("pit").expect("pit"))
        );
        assert!(
            world
                .components()
                .spawn_points
                .contains(world.handle_of("crate_point").expect("point"))
        );
        assert_eq!(
            world.interactables().len(),
            2,
            "the prop and the door are aimable"
        );
    }

    #[test]
    fn an_interact_event_runs_the_actors_own_bindings() {
        let level = base_level(
            r#""props": [
                { "id": "plant", "model": "core:plant", "x": 2.0, "z": 2.0,
                  "size": [0.6, 1.8, 0.6],
                  "components": [ { "component": "interactable", "prompt": "Toggle name" } ],
                  "bindings": [ { "on": "interact", "actions": [ { "action": "toggle_label" } ] } ] }
            ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let index = world
            .interactables()
            .index_of("plant")
            .expect("plant is aimable");
        assert!(!world.is_label_visible(index));
        let report = world
            .dispatch_interaction(Some(index))
            .expect("the interaction resolves");
        assert_eq!(report.labels_shown, 1);
        assert!(world.is_label_visible(index));
        world.dispatch_interaction(Some(index));
        assert!(!world.is_label_visible(index));
        assert!(world.dispatch_interaction(None).is_none());
    }

    #[test]
    fn an_action_chain_reaches_a_door_and_a_light_through_conditions() {
        let level = base_level(
            r#""props": [
                { "id": "switch", "model": "home:wall_switch", "x": 2.0, "z": 2.0,
                  "components": [ { "component": "interactable", "prompt": "Switch" },
                                  { "component": "state", "name": "on", "value": false } ],
                  "bindings": [
                    { "on": "interact",
                      "actions": [ { "action": "set_state", "name": "on", "value": true },
                                   { "action": "toggle", "target": "hall_door" },
                                   { "action": "set_light", "target": "lamp", "on": false } ] },
                    { "on": "object_state", "key": "on",
                      "when": [ { "check": "disabled", "target": "lamp" } ],
                      "actions": [ { "action": "toggle_label", "target": "switch" } ] } ] }
            ],
            "doors": [ { "id": "hall_door", "x": 3.0, "z": 3.0, "width": 1.0, "height": 2.1 } ],
            "ceiling_lights": [ { "id": "lamp", "fixture": "core:fluorescent_panel_01",
                                  "x": 4.0, "z": 4.0, "switchable": true } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let switch = world.handle_of("switch").expect("switch");
        let actions: Vec<ActionDef> = vec![
            ActionDef::SetState {
                target: None,
                name: "on".into(),
                value: StateValue::Bool(true),
            },
            ActionDef::Toggle {
                target: Some("hall_door".into()),
            },
            ActionDef::SetLight {
                target: Some("lamp".into()),
                on: false,
            },
        ];
        let report = world.dispatch_actions(&actions, Some(switch));
        assert_eq!(report.actions_run, 3);
        assert_eq!(
            world.doors().get(0).expect("door").phase(),
            DoorPhase::Opening
        );
        assert!(
            !world
                .components()
                .lights
                .get(world.handle_of("lamp").expect("lamp"))
                .expect("light")
                .enabled
        );
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        let index = world
            .interactables()
            .index_of("switch")
            .expect("switch aimable");
        assert!(
            world.is_label_visible(index),
            "the state change ran the conditioned binding"
        );

        // The same state again is not a change, so the chain does not rerun.
        let actions = vec![ActionDef::SetState {
            target: None,
            name: "on".into(),
            value: StateValue::Bool(true),
        }];
        world.dispatch_actions(&actions, Some(switch));
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        assert!(
            world.is_label_visible(index),
            "an unchanged state emits nothing"
        );
    }

    #[test]
    fn volume_edges_fire_enter_and_exit_once_per_crossing() {
        let level = base_level(
            r#""volumes": [
                { "id": "pit", "x": 5.0, "z": 5.0, "width": 2.0, "depth": 2.0,
                  "bottom_y": 0.0, "top_y": 2.0,
                  "bindings": [ { "on": "enter_volume",
                                  "actions": [ { "action": "set_state", "target": "counter",
                                                 "name": "falls", "value": 1 } ] } ] }
            ],
            "props": [ { "id": "counter", "model": "core:crate", "x": 1.0, "z": 1.0,
                         "components": [ { "component": "state", "name": "falls", "value": 0 } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let counter = world.handle_of("counter").expect("counter");
        let outside = Vec3::new(1.0, 0.0, 1.0);
        let inside = Vec3::new(5.5, 0.0, 5.5);
        let falls = |world: &EntityWorld| {
            world
                .components()
                .states
                .get(counter)
                .and_then(|state| state.get("falls").cloned())
        };
        world.update_volumes(outside, outside);
        world.update_volumes(outside, inside);
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        assert_eq!(
            falls(&world),
            Some(StateValue::Int(1)),
            "the enter binding ran"
        );

        // Staying inside fires nothing new.
        world
            .components_mut()
            .states
            .get_mut(counter)
            .expect("state")
            .set("falls", StateValue::Int(0));
        world.update_volumes(inside, inside);
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        assert_eq!(
            falls(&world),
            Some(StateValue::Int(0)),
            "no enter edge while inside"
        );

        // Leaving and re-entering fires again.
        world.update_volumes(inside, outside);
        world.update_volumes(outside, inside);
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        assert_eq!(falls(&world), Some(StateValue::Int(1)));
    }

    #[test]
    fn a_timer_fires_its_binding_and_a_reset_re_arms_it() {
        let level = base_level(
            r#""props": [ { "id": "lamp_prop", "model": "core:lamp", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "state", "name": "ticks", "value": 0 } ] } ],
            "timers": [ { "id": "ticker", "seconds": 1.0, "autostart": true,
                          "bindings": [ { "on": "timer",
                                          "actions": [ { "action": "set_state", "target": "lamp_prop",
                                                         "name": "ticks", "value": 1 } ] } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let prop = world.handle_of("lamp_prop").expect("prop");
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let ticks = |world: &EntityWorld| {
            world
                .components()
                .states
                .get(prop)
                .and_then(|state| state.get("ticks").cloned())
        };
        tick_at(&mut world, &level, feet, 0.5);
        assert_eq!(ticks(&world), Some(StateValue::Int(0)), "not yet due");
        tick_at(&mut world, &level, feet, 0.6);
        assert_eq!(
            ticks(&world),
            Some(StateValue::Int(1)),
            "the timer fired once"
        );
        world
            .components_mut()
            .states
            .get_mut(prop)
            .expect("state")
            .set("ticks", StateValue::Int(0));
        tick_at(&mut world, &level, feet, 2.0);
        assert_eq!(
            ticks(&world),
            Some(StateValue::Int(0)),
            "a one-shot timer does not re-fire"
        );
        world.reset_runtime();
        tick_at(&mut world, &level, feet, 1.1);
        assert_eq!(ticks(&world), Some(StateValue::Int(1)));
    }

    #[test]
    fn a_sequence_runs_its_steps_and_emits_completion_once() {
        let level = base_level(
            r#""props": [ { "id": "controller", "model": "core:crate", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "state", "name": "phase", "value": "idle" } ] } ],
            "sequences": [ { "id": "warmup", "steps": [
                { "step": "wait", "seconds": 0.5 },
                { "step": "set_state", "name": "phase", "value": "warm" },
                { "step": "emit", "on": "timer", "key": "warm" }
            ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let controller = world.handle_of("controller").expect("controller");
        assert!(world.start_sequence("controller", "warmup"));
        assert_eq!(world.sequence_count(), 1);
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let phase = |world: &EntityWorld| {
            world
                .components()
                .states
                .get(controller)
                .and_then(|state| state.get("phase").cloned())
        };
        tick_at(&mut world, &level, feet, 0.2);
        assert_eq!(
            phase(&world),
            Some(StateValue::Text("idle".into())),
            "still waiting"
        );
        let tick = tick_at(&mut world, &level, feet, 0.4);
        assert_eq!(tick.sequences_completed, 1, "the sequence completed");
        assert_eq!(phase(&world), Some(StateValue::Text("warm".into())));
        assert_eq!(world.sequence_count(), 0);
        assert!(
            !world
                .components()
                .sequences
                .get(controller)
                .expect("controller")
                .running
        );
    }

    #[test]
    fn a_despawned_sequence_owner_is_cancelled_without_hanging() {
        let level = base_level(
            r#""props": [ { "id": "runner", "model": "core:crate", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "state", "name": "phase", "value": "idle" } ] } ],
            "spawn_templates": [ { "id": "beacon", "model": "core:crate", "lifetime_seconds": 0.1 } ],
            "spawn_points": [ { "id": "beacon_point", "x": 5.0, "z": 5.0, "template": "beacon" } ],
            "sequences": [ { "id": "long_wait", "steps": [
                { "step": "wait", "seconds": 30.0 },
                { "step": "set_state", "target": "runner", "name": "phase", "value": "done" }
            ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let report = world.dispatch_on(
            "runner",
            &[ActionDef::SpawnEntity {
                template: None,
                point: Some("beacon_point".into()),
                group: None,
                name: Some("beacon".into()),
            }],
        );
        assert_eq!(report.spawned, 1);
        assert!(world.start_sequence("beacon", "long_wait"));
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let tick = tick_at(&mut world, &level, feet, 0.2);
        assert_eq!(tick.despawned, 1, "the lifetime expired");
        assert_eq!(world.sequence_count(), 0, "the sequence was cancelled");
        assert!(world.handle_of("beacon").is_none());
        let tick = tick_at(&mut world, &level, feet, 1.0);
        assert_eq!(
            tick.sequences_completed, 0,
            "nothing completes after the cancel"
        );
    }

    #[test]
    fn spawning_is_group_bounded_named_and_released() {
        let level = base_level(
            r#""spawn_templates": [ { "id": "crate_template", "model": "core:crate",
                                      "lifetime_seconds": 5.0,
                                      "components": [ { "component": "state", "name": "phase",
                                                        "value": "spawned" } ] } ],
            "spawn_points": [ { "id": "crate_point", "x": 4.0, "z": 4.0,
                                "template": "crate_template", "group": "crates" } ],
            "spawn_groups": [ { "id": "crates", "at_most_one_active": true } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let spawn = ActionDef::SpawnEntity {
            template: None,
            point: Some("crate_point".into()),
            group: None,
            name: Some("first".into()),
        };
        let report = world.dispatch_actions(std::slice::from_ref(&spawn), None);
        assert_eq!(report.spawned, 1);
        let first = world.handle_of("first").expect("the spawn is named");
        assert_eq!(world.live_spawns().len(), 1);
        assert!(world.components().lifetimes.contains(first));
        let commands = world.take_commands();
        assert!(matches!(
            commands.first(),
            Some(WorldCommand::SpawnDynamic { model, .. }) if model == "core:crate"
        ));
        // The at-most-one group refuses a second member while the first lives.
        let report = world.dispatch_actions(std::slice::from_ref(&spawn), None);
        assert_eq!(report.spawned, 0);
        assert_eq!(report.unsupported, 1);
        assert!(
            world.despawn_target("first"),
            "the runtime name despawns it"
        );
        assert_eq!(world.live_spawns().len(), 0);
        assert_eq!(
            world.spawn_groups().at(0).and_then(|group| group.live),
            None
        );
        assert!(
            world
                .take_commands()
                .iter()
                .any(|command| matches!(command, WorldCommand::DespawnDynamic { .. }))
        );
        let report = world.dispatch_actions(&[spawn], None);
        assert_eq!(report.spawned, 1, "the group released");
    }

    #[test]
    fn a_locked_door_refuses_to_open_and_unlock_allows_it() {
        let level = base_level(
            r#""doors": [ { "id": "vault", "x": 3.0, "z": 3.0, "width": 1.0, "height": 2.1,
                            "locked": true } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let open = ActionDef::Open {
            target: Some("vault".into()),
        };
        let report = world.dispatch_actions(std::slice::from_ref(&open), None);
        assert_eq!(report.doors_acted, 0, "a locked door does not open");
        assert_eq!(
            world.doors().get(0).expect("door").phase(),
            DoorPhase::Closed
        );
        assert_eq!(world.doors().is_locked("vault"), Some(true));
        let report = world.dispatch_actions(
            &[ActionDef::Unlock {
                target: Some("vault".into()),
            }],
            None,
        );
        assert_eq!(report.actions_run, 1);
        let report = world.dispatch_actions(&[open], None);
        assert_eq!(report.doors_acted, 1);
        assert_eq!(
            world.doors().get(0).expect("door").phase(),
            DoorPhase::Opening
        );
    }

    #[test]
    fn light_switches_report_changed_fixtures_once() {
        let level = base_level(
            r#""ceiling_lights": [
                { "id": "lamp_a", "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0,
                  "switchable": true },
                { "id": "lamp_b", "fixture": "core:fluorescent_panel_01", "x": 6.0, "z": 4.0,
                  "switchable": true } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let off = ActionDef::SetLight {
            target: Some("lamp_b".into()),
            on: false,
        };
        let report = world.dispatch_actions(std::slice::from_ref(&off), None);
        assert_eq!(report.lights_toggled, 1);
        assert_eq!(world.take_light_toggles(), vec![(1, false)]);
        assert!(world.take_light_toggles().is_empty(), "drained once");
        let report = world.dispatch_actions(&[off], None);
        assert_eq!(report.lights_toggled, 0);
        assert!(world.take_light_toggles().is_empty());
        assert_eq!(world.light_states(), vec![(0, true), (1, false)]);
    }

    #[test]
    fn a_stale_event_from_an_old_world_never_fires_in_the_new_one() {
        let level = base_level(
            r#""props": [ { "id": "plant", "model": "core:plant", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "interactable" } ],
                            "bindings": [ { "on": "interact",
                                            "actions": [ { "action": "toggle_label" } ] } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let plant = world.handle_of("plant").expect("plant");
        world.emit(EventKind::Interact, plant, "", Some(plant));
        world.resolve(&level);
        let feet = Vec3::new(1.0, 0.0, 1.0);
        tick_at(&mut world, &level, feet, 1.0 / 60.0);
        let index = world.interactables().index_of("plant").expect("plant");
        assert!(
            !world.is_label_visible(index),
            "the record's world generation no longer matches"
        );
    }

    #[test]
    fn a_mismatched_generation_record_is_dropped_even_without_a_clear() {
        let level = base_level(
            r#""props": [ { "id": "plant", "model": "core:plant", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "interactable" } ],
                            "bindings": [ { "on": "interact",
                                            "actions": [ { "action": "toggle_label" } ] } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let plant = world.handle_of("plant").expect("plant");
        // A record from another world generation, queued without any clear.
        world.events.push(EventRecord {
            generation: world.generation().wrapping_add(1),
            kind: EventKind::Interact,
            subject: plant,
            key: String::new(),
            actor: Some(plant),
            depth: 0,
        });
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let tick = tick_at(&mut world, &level, feet, 1.0 / 60.0);
        assert_eq!(tick.events_processed, 1, "the record was processed");
        let index = world.interactables().index_of("plant").expect("plant");
        assert!(
            !world.is_label_visible(index),
            "a record from another generation never fires"
        );
    }

    #[test]
    fn an_invalid_spawn_target_is_reported_not_ignored() {
        let level = base_level(r#""props": []"#);
        let mut world = EntityWorld::from_level(&level);
        let report = world.dispatch_on(
            "missing",
            &[ActionDef::SpawnEntity {
                template: None,
                point: Some("nowhere".into()),
                group: None,
                name: None,
            }],
        );
        assert_eq!(report.missing_targets, 1);
        assert_eq!(report.spawned, 0);
        assert!(world.live_spawns().is_empty());
    }

    #[test]
    fn a_move_object_walks_a_runtime_entity_respecting_walls() {
        let level = base_level(
            r#""props": [ { "id": "anchor", "model": "core:crate", "x": 1.0, "z": 1.0 } ],
            "spawn_templates": [ { "id": "mover", "model": "core:crate", "scale": 0.4 } ],
            "spawn_points": [ { "id": "mover_point", "x": 5.0, "z": 5.0, "template": "mover" } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let report = world.dispatch_on(
            "anchor",
            &[ActionDef::SpawnEntity {
                template: None,
                point: Some("mover_point".into()),
                group: None,
                name: Some("mover".into()),
            }],
        );
        assert_eq!(report.spawned, 1);
        let handle = world.handle_of("mover").expect("mover");
        let start = world
            .components()
            .transforms
            .get(handle)
            .expect("transform")
            .position;
        assert_eq!((start.x, start.z), (5.0, 5.0));
        let report = world.dispatch_on(
            "mover",
            &[ActionDef::MoveObject {
                target: None,
                x: 8.0,
                y: None,
                z: 5.0,
                speed: Some(2.0),
            }],
        );
        assert_eq!(report.objects_moved, 1);
        let feet = Vec3::new(1.0, 0.0, 1.0);
        // 1.5 s at 2 m/s covers the three metres.
        tick_at(&mut world, &level, feet, 0.5);
        for _ in 0..60 {
            tick_at(&mut world, &level, feet, 1.0 / 60.0);
        }
        let end = world
            .components()
            .transforms
            .get(handle)
            .expect("transform")
            .position;
        assert!((end.x - 8.0).abs() < 0.05, "arrived: {end:?}");
        assert!((end.z - 5.0).abs() < 1.0e-3);
        // A static baked prop refuses the move with a named outcome.
        let report = world.dispatch_on(
            "anchor",
            &[ActionDef::MoveObject {
                target: None,
                x: 2.0,
                y: None,
                z: 2.0,
                speed: None,
            }],
        );
        assert_eq!(report.objects_moved, 0);
        assert_eq!(report.unsupported, 1, "a baked static prop cannot move");
    }

    #[test]
    fn a_wait_animation_step_completes_on_the_renderer_notification() {
        let level = base_level(
            r#""props": [ { "id": "actor", "model": "core:crate", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "animation", "clip": "wave",
                                              "playing": true },
                                            { "component": "state", "name": "phase",
                                              "value": "idle" } ] } ],
            "sequences": [ { "id": "wave_then_done", "steps": [
                { "step": "wait_animation", "clip": "wave", "timeout": 0.0 },
                { "step": "set_state", "name": "phase", "value": "done" }
            ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let actor = world.handle_of("actor").expect("actor");
        assert!(world.start_sequence("actor", "wave_then_done"));
        let feet = Vec3::new(1.0, 0.0, 1.0);
        tick_at(&mut world, &level, feet, 0.1);
        assert_eq!(
            world
                .components()
                .states
                .get(actor)
                .and_then(|state| state.get("phase").cloned()),
            Some(StateValue::Text("idle".into())),
            "the wait is not satisfied by time alone"
        );
        world.notify_animation_complete("actor", "wave");
        tick_at(&mut world, &level, feet, 0.1);
        assert_eq!(
            world
                .components()
                .states
                .get(actor)
                .and_then(|state| state.get("phase").cloned()),
            Some(StateValue::Text("done".into()))
        );
    }

    #[test]
    fn a_self_referential_chain_is_cut_at_the_depth_limit() {
        let level = base_level(
            r#""props": [ { "id": "switch", "model": "home:wall_switch", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "interactable" } ],
                            "bindings": [ { "on": "interact",
                                            "actions": [ { "action": "toggle", "target": "lamp" } ] } ] } ],
            "ceiling_lights": [ { "id": "lamp", "fixture": "core:fluorescent_panel_01",
                                  "x": 4.0, "z": 4.0, "switchable": true,
                                  "bindings": [ { "on": "object_state", "key": "light",
                                                  "actions": [ { "action": "toggle", "target": "lamp" } ] } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let switch = world.handle_of("switch").expect("switch");
        world.dispatch_actions(
            &[ActionDef::Toggle {
                target: Some("lamp".into()),
            }],
            Some(switch),
        );
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let tick = tick_at(&mut world, &level, feet, 1.0 / 60.0);
        assert!(
            tick.events_dropped > 0,
            "the runaway chain was cut, not recursed"
        );
        assert!(tick.events_processed <= MAX_EVENTS_PER_TICK);
        // The world is still usable afterwards.
        let report = world.dispatch_actions(
            &[ActionDef::Toggle {
                target: Some("lamp".into()),
            }],
            Some(switch),
        );
        assert_eq!(report.lights_toggled, 1);
    }

    #[test]
    fn a_despawned_entity_leaves_no_binding_or_group_behind() {
        let level = base_level(
            r#""spawn_templates": [ { "id": "crate_template", "model": "core:crate" } ],
            "spawn_points": [ { "id": "crate_point", "x": 4.0, "z": 4.0,
                                "template": "crate_template", "group": "crates",
                                "bindings": [ { "on": "spawn",
                                                "actions": [ { "action": "set_state",
                                                               "name": "phase", "value": "fresh" } ] } ] } ],
            "spawn_groups": [ { "id": "crates", "at_most_one_active": true } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let before = world.binding_count();
        let report = world.dispatch_on(
            "crate_point",
            &[ActionDef::SpawnEntity {
                template: None,
                point: None,
                group: None,
                name: Some("crate".into()),
            }],
        );
        assert_eq!(report.missing_targets, 1, "a point is required");
        let report = world.dispatch_on(
            "crate_point",
            &[ActionDef::SpawnEntity {
                template: None,
                point: Some("crate_point".into()),
                group: None,
                name: Some("crate".into()),
            }],
        );
        assert_eq!(report.spawned, 1);
        assert!(
            world.binding_count() > before,
            "the spawn added its bindings"
        );
        // The `spawn` event the spawn queued is processed by the next tick,
        // exactly as a frame-loop spawn would be.
        let feet = Vec3::new(1.0, 0.0, 1.0);
        tick_at(&mut world, &level, feet, 1.0 / 60.0);
        let handle = world.handle_of("crate").expect("crate");
        assert_eq!(
            world
                .components()
                .states
                .get(handle)
                .and_then(|state| state.get("phase").cloned()),
            Some(StateValue::Text("fresh".into())),
            "the point's own spawn binding ran"
        );
        assert!(world.despawn_target("crate"));
        let mut tick = WorldTick::default();
        world.pump_events(&mut tick);
        assert_eq!(
            world.binding_count(),
            before,
            "the despawn compacted its bindings"
        );
        assert_eq!(
            world.spawn_groups().at(0).and_then(|group| group.live),
            None
        );
        assert_eq!(world.dynamic_key_entries(), 0);
    }

    #[test]
    fn a_static_prop_cannot_be_despawned() {
        let level = base_level(
            r#""props": [ { "id": "plant", "model": "core:plant", "x": 2.0, "z": 2.0 } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let report = world.dispatch_on(
            "plant",
            &[ActionDef::DespawnEntity {
                target: "plant".into(),
            }],
        );
        assert_eq!(report.despawned, 0);
        assert_eq!(report.missing_targets, 1);
        assert!(
            world.handle_of("plant").is_some(),
            "the prop is still there"
        );
    }

    #[test]
    fn pre_pump_refusals_are_reported_by_the_tick() {
        let level = base_level(r#""props": []"#);
        let mut world = EntityWorld::from_level(&level);
        let subject = world.handle_of("missing");
        let _ = subject;
        // Emit more records than the queue holds, all unknown-subject records
        // that are never bound: the producers' refusals must still be reported.
        for _ in 0..(EventQueue::MAX_QUEUED_EVENTS + 64) {
            let _ = world.emit(EventKind::Timer, EntityHandle::from_parts(0, 0), "t", None);
        }
        let feet = Vec3::new(1.0, 0.0, 1.0);
        let tick = tick_at(&mut world, &level, feet, 1.0 / 60.0);
        assert!(
            tick.events_dropped > 0,
            "the queue's refusals are folded into the tick report"
        );
        assert_eq!(world.events().len(), 0, "and the queue is drained");
    }

    #[test]
    fn a_binding_cued_animation_rebuilds_the_renderer_handoff() {
        let level = base_level(
            r#""props": [ { "id": "lever", "model": "home:wall_switch", "x": 2.0, "z": 2.0,
                            "components": [ { "component": "interactable" },
                                            { "component": "animation", "clip": "toggle" } ],
                            "bindings": [ { "on": "interact",
                                            "actions": [ { "action": "toggle_animation",
                                                           "clip": "toggle" } ] } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let index = world.interactables().index_of("lever").expect("aimable");
        assert!(
            world.entity_frames().is_empty(),
            "no routes and no cues yet"
        );
        world.dispatch_interaction(Some(index));
        let frames = world.entity_frames();
        assert_eq!(frames.len(), 1, "the cued prop reaches the renderer");
        assert_eq!(frames[0].instance_id, "lever");
        assert!(frames[0].transform.is_none(), "a cue-only frame");
        assert_eq!(
            frames[0].cue,
            PoseCue::Scrub {
                name: "toggle".into(),
                target: 1.0,
            }
        );
    }

    #[test]
    fn a_water_volume_reports_its_authored_index_and_disables_sampling() {
        let level = base_level(
            r#""water": [ { "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0, "surface_y": 0.0,
                            "bottom_y": -2.0 } ],
            "effects": [ { "kind": "steam", "x": 8.0, "z": 8.0 } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        assert!(
            world.handle_of("water_1").is_some(),
            "the volume is an entity"
        );
        let report = world.dispatch_on("water_1", &[ActionDef::Disable { target: None }]);
        assert_eq!(report.actions_run, 1);
        assert!(matches!(
            world.take_commands().as_slice(),
            [WorldCommand::SetWaterEnabled {
                index: 0,
                enabled: false
            }]
        ));
        // The steam emitter reports its authored index too.
        let report = world.dispatch_on("effect_1", &[ActionDef::Disable { target: None }]);
        assert_eq!(report.actions_run, 1);
        assert!(matches!(
            world.take_commands().as_slice(),
            [WorldCommand::SetEffectEnabled {
                index: 0,
                enabled: false
            }]
        ));
    }

    #[test]
    fn a_route_turn_reorients_the_live_aim_bounds() {
        let level = base_level(
            r#""props": [ { "id": "turner", "model": "core:crate", "x": 2.0, "z": 2.0,
                            "size": [1.0, 0.5, 0.4],
                            "components": [ { "component": "interactable", "prompt": "Turn" } ] } ],
            "routes": [ { "id": "turner", "steps": [ { "step": "face", "yaw_degrees": 90.0 },
                                                     { "step": "wait", "seconds": 1.0 } ] } ]"#,
        );
        let mut world = EntityWorld::from_level(&level);
        let walls = level.collision_aabbs();
        let index = CollisionIndex::build(&walls);
        let floor = WalkableFloor::from_level(&level);
        let route_world = RouteWorld {
            walls: &walls,
            floor: &floor,
            index: &index,
        };
        for _ in 0..60 {
            world.update_entities(1.0 / 60.0, &route_world);
        }
        let item_index = world.interactables().index_of("turner").expect("aimable");
        let item = world.interactables().get(item_index).expect("item");
        let width = item.bounds.max[0] - item.bounds.min[0];
        let depth = item.bounds.max[2] - item.bounds.min[2];
        assert!(
            depth > width,
            "the turned box is long in z: {width} x {depth}"
        );
    }
}

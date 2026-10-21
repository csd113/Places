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

mod audio;
pub mod balloon;
pub mod components;
pub mod developer;
pub mod events;
pub mod id;
mod sequence_runtime;
pub mod sequences;
pub mod spawn;
pub mod timers;
pub mod volumes;

pub use volumes::TriggerItem;

use glam::Vec3;

use crate::engine::ai::{AiDef, AiOutcome, AiStimulus, AiTarget, AiTickContext, AiWorld};
use crate::engine::collision::{DoorCollider, WallAabb, resolve_player_collision_for_body_indexed};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::{DoorPhase, Doors};
use crate::engine::entity::{
    ENTITY_MIN_RADIUS_M, EntityFrame, EntityRoute, EntityRoutes, GlowCue, PoseCue, RouteState,
    RouteWorld,
};
use crate::engine::interact::{
    DEFAULT_INTERACTION_REACH_M, Interactable, InteractableSync, Interactables,
    MAX_INTERACTION_REACH_M, referenced_targets,
};
use crate::engine::level::{
    ActionDef, AnimatedEmissionDef, ComponentDef, ConditionDef, EventBindingDef, Ladders, LevelDef,
    LevelSurfaces, PROP_FALLBACK_SIZE, WalkableCeiling, WalkableFloor, WaterVolumes,
};
use crate::engine::logging;
use crate::engine::nav::NavMesh;
use crate::engine::spatial::rotated_half_extents;

use components::{
    Animation, AudioEmitter, Collider, ComponentTables, Fade, Glow,
    Interactable as InteractableComponent, Lifetime, Light, LightResponse, Material, NavAgent,
    NavObstacle, ObjectState, Renderable, SpawnPoint as SpawnPointComponent, StateValue, Steam,
    Transform, TriggerVolume, WaterVolumeCtl,
};
use events::{ConditionView, EventKind, EventQueue, EventRecord};
use id::{EntityHandle, EntityId, EntityNames, EntityStore};
use sequences::{SequenceRuntime, SequenceStepDef, Sequences};
use spawn::{SpawnGroups, SpawnPointDef, SpawnTemplateDef};
use timers::Timers;

/// Largest event-chain depth accepted, preserved across deferred tick tails.
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

/// A renderer-backed gameplay light observation, exposed for native diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct LightResponseObservation<'a> {
    /// Authored or runtime instance id.
    pub instance_id: &'a str,
    /// Current base position.
    pub position: Vec3,
    /// Strongest visible body sample in incident-intensity units.
    pub intensity: f32,
    /// True while the beam or bounded release grace holds locomotion.
    pub active: bool,
}

/// One renderer/audio command produced by the world.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldCommand {
    /// Bounded explanation which the player can dismiss without resetting.
    ShowMessage { title: String, lines: Vec<String> },
    /// Concise authored win/death feedback, consumed by the native player.
    EndRun {
        won: bool,
        title: String,
        detail: String,
    },
    /// Request a same-package destination; the player owns asynchronous loading.
    ChangeLevel { level: String },
    /// Restore one authored physics group's starting poses and clear momentum.
    ResetPhysicsGroup { group: String },
    /// Activate dormant members of an authored physical group.
    ActivatePhysicsGroup { group: String },
    /// Atomically claim the authored carry contract's exact held item.
    HandoffItem { target: String },
    /// Hide an accepted carry assembly only after whole-geometry occlusion.
    RetireHidden { target: String },
    /// Select one of the two prepared, map-authored weather configurations.
    SetWeatherAlternate {
        /// True selects `weather_alternate`; false restores `weather`.
        alternate: bool,
    },
    /// Ease between the prepared weather endpoints; manual changes stop cycles.
    SetWeatherStrength {
        strength: f32,
        transition_seconds: f32,
    },
    /// Resume or stop the level's optional authored cycle.
    SetWeatherCycle { enabled: bool },
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
        /// Whether playback follows simulation time or real time.
        time_domain: crate::engine::audio::TimeDomain,
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
    /// Door state requests changed by bindings this tick.
    pub doors_acted: usize,
    /// Light controllers changed by bindings this tick, for action feedback.
    pub lights_toggled: usize,
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

impl WorldTick {
    /// Keep automatic callbacks and event bindings on the same action accounting.
    const fn record_dispatch(&mut self, report: &DispatchReport) {
        self.actions_run = self.actions_run.saturating_add(report.actions_run);
        self.doors_acted = self.doors_acted.saturating_add(report.doors_acted);
        self.lights_toggled = self.lights_toggled.saturating_add(report.lights_toggled);
        self.spawned = self.spawned.saturating_add(report.spawned);
        self.despawned = self.despawned.saturating_add(report.despawned);
        self.missing_targets = self.missing_targets.saturating_add(report.missing_targets);
        self.unsupported = self.unsupported.saturating_add(report.unsupported);
        self.labels_shown = self.labels_shown.saturating_add(report.labels_shown);
        self.labels_hidden = self.labels_hidden.saturating_add(report.labels_hidden);
        self.frames_dirty |=
            report.animations_started > 0 || report.spawned > 0 || report.despawned > 0;
        self.player_reset |= report.player_reset;
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum WeatherEndpoints {
    #[default]
    Absent,
    Discrete,
    Continuous,
}

/// The whole runtime for one loaded world.
pub struct EntityWorld {
    transition_targets: std::collections::BTreeSet<String>,
    scripts: crate::engine::scripting::ScriptRuntime,
    script_event: Option<EventRecord>,
    pub(crate) script_player: Vec3,
    pub(crate) script_held_item: Option<String>,
    pub(crate) script_visible: std::collections::BTreeSet<EntityHandle>,
    generation: u64,
    store: EntityStore,
    names: EntityNames,
    components: ComponentTables,
    interactables: Interactables,
    doors: Doors,
    animated_doors: Vec<crate::engine::animated_door::AnimatedDoor>,
    door_colliders: Vec<DoorCollider>,
    routes: EntityRoutes,
    route_states: Vec<RouteState>,
    animation_overrides: Vec<(String, PoseCue)>,
    entity_frames: Vec<EntityFrame>,
    timers: Timers,
    sequences: Sequences,
    sequence_runs: Vec<SequenceRuntime>,
    /// Monotonic identity separates a replacement from its in-flight predecessor.
    next_sequence_revision: u64,
    spawn_groups: SpawnGroups,
    templates: Vec<SpawnTemplateDef>,
    spawn_points: Vec<SpawnPointDef>,
    bindings: Vec<ResolvedBinding>,
    bindings_by_owner: std::collections::HashMap<EntityHandle, Vec<usize>>,
    cooling_bindings: Vec<usize>,
    binding_index_state: BindingIndexState,
    events: EventQueue,
    commands: Vec<WorldCommand>,
    /// Weather selection belongs to this loaded session, never saved settings.
    weather_strength: f32,
    weather_target: f32,
    weather_cycle_enabled: bool,
    weather_cycle_default: bool,
    weather_cycle_available: bool,
    weather_endpoints: WeatherEndpoints,
    /// Authored physical reset groups addressable by the generic action.
    physics_reset_groups: Vec<String>,
    /// Movable authored props whose render/body lifetime the native loop owns.
    authored_physics: Vec<EntityHandle>,
    /// Current bounded physical-item bounds, shared by AI queries and motion.
    navigation_obstacles: Vec<WallAabb>,
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
    /// Fixtures on one authored circuit change state atomically.
    light_circuits: Vec<Vec<EntityHandle>>,
    /// Authored state bags, restored by a reset.
    authored_states: Vec<(EntityHandle, ObjectState)>,
    /// Authored interaction prompts, restored with the other map controls.
    authored_prompts: Vec<(EntityHandle, String)>,
    /// Authored emitter state, restored on reset.
    authored_audio: Vec<(EntityHandle, AudioEmitter)>,
    /// Reused bounded trigger work buffers; stationary occupancy allocates no strings.
    trigger_sweeps: Vec<volumes::Sweep>,
    trigger_edges: Vec<volumes::Edge>,
    trigger_index: volumes::TriggerIndex,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum BindingIndexState {
    Current,
    NeedsCompaction,
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
            transition_targets: std::collections::BTreeSet::new(),
            scripts: crate::engine::scripting::ScriptRuntime::default(),
            script_event: None,
            script_player: Vec3::ZERO,
            script_held_item: None,
            script_visible: std::collections::BTreeSet::new(),
            generation: 0,
            store: EntityStore::new(),
            names: EntityNames::new(),
            components: ComponentTables::new(),
            interactables: Interactables::new(),
            doors: Doors::default(),
            door_colliders: Vec::new(),
            animated_doors: Vec::new(),
            routes: EntityRoutes::new(),
            route_states: Vec::new(),
            animation_overrides: Vec::new(),
            entity_frames: Vec::new(),
            timers: Timers::new(),
            sequences: Sequences::new(),
            sequence_runs: Vec::new(),
            next_sequence_revision: 1,
            spawn_groups: SpawnGroups::new(),
            templates: Vec::new(),
            spawn_points: Vec::new(),
            bindings: Vec::new(),
            bindings_by_owner: std::collections::HashMap::new(),
            cooling_bindings: Vec::new(),
            binding_index_state: BindingIndexState::Current,
            events: EventQueue::new(),
            commands: Vec::new(),
            weather_strength: 0.0,
            weather_target: 0.0,
            weather_cycle_enabled: false,
            weather_cycle_default: false,
            weather_cycle_available: false,
            weather_endpoints: WeatherEndpoints::Absent,
            physics_reset_groups: Vec::new(),
            authored_physics: Vec::new(),
            navigation_obstacles: Vec::new(),
            moves: Vec::new(),
            live_spawns: Vec::new(),
            dynamic_keys: Vec::new(),
            next_dynamic_key: 1,
            spawn_failures: 0,
            reported_drops: 0,
            spawns_this_tick: 0,
            authored_lights: Vec::new(),
            light_circuits: Vec::new(),
            authored_states: Vec::new(),
            authored_prompts: Vec::new(),
            authored_audio: Vec::new(),
            trigger_sweeps: Vec::new(),
            trigger_edges: Vec::new(),
            trigger_index: volumes::TriggerIndex::default(),
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
        self.transition_targets = level.transitions.iter().cloned().collect();
        self.scripts = crate::engine::scripting::ScriptRuntime::new(level);
        self.script_event = None;
        self.store.clear();
        self.names.clear();
        self.components.clear();
        self.generation = self.store.generation();
        self.animation_overrides.clear();
        self.entity_frames.clear();
        self.sequence_runs.clear();
        self.events.clear();
        self.commands.clear();
        self.weather_strength = 0.0;
        self.weather_target = 0.0;
        self.weather_cycle_available = level.weather_cycle.is_some();
        self.weather_cycle_default = level.weather_cycle.is_some_and(|cycle| cycle.enabled);
        self.weather_cycle_enabled = self.weather_cycle_default;
        let continuous_weather = level
            .weather
            .as_ref()
            .zip(level.weather_alternate.as_ref())
            .is_some_and(|(base, alternate)| {
                crate::engine::weather::strength_compatible(base, alternate)
            });
        self.weather_endpoints = if continuous_weather {
            WeatherEndpoints::Continuous
        } else if level.weather.is_some() && level.weather_alternate.is_some() {
            WeatherEndpoints::Discrete
        } else {
            WeatherEndpoints::Absent
        };
        self.moves.clear();
        self.physics_reset_groups = level.physics_reset_groups().map(str::to_string).collect();
        self.authored_physics.clear();
        self.navigation_obstacles.clear();
        self.live_spawns.clear();
        self.dynamic_keys.clear();
        self.next_dynamic_key = 1;
        self.spawn_failures = 0;
        self.reported_drops = 0;
        self.spawns_this_tick = 0;
        self.authored_lights.clear();
        self.authored_states.clear();
        self.authored_prompts.clear();
        self.authored_audio.clear();
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
        self.animated_doors = crate::engine::animated_door::load(level).unwrap_or_else(|error| {
            warn_once("animated-door-load", error);
            Vec::new()
        });
        self.timers = Timers::from_level(level);
        self.sequences = Sequences::from_level(level);
        self.spawn_groups = SpawnGroups::from_level(level);
        self.templates.clone_from(&level.spawn_templates);
        self.spawn_points.clone_from(&level.spawn_points);
        self.clear_bindings();

        let surfaces = LevelSurfaces::new(level);
        self.resolve_props(level, &surfaces);
        self.resolve_doors(level);
        self.resolve_fixtures(level);
        self.resolve_volumes(level);
        self.resolve_effects(level);
        self.resolve_water(level);
        self.resolve_timers(level);
        self.resolve_spawn_points(level, &surfaces);
        self.resolve_sound_emitters(level);
        self.resolve_ai();
        self.resolve_routes(level);
        self.trigger_index = volumes::TriggerIndex::build(&self.components.volumes);
        self.authored_states = self
            .components
            .states
            .iter()
            .map(|(handle, state)| (handle, state.clone()))
            .collect();
        self.authored_prompts = self
            .components
            .interactables
            .iter()
            .map(|(handle, component)| (handle, component.prompt.clone()))
            .collect();
        self.rebuild_interactables(level);
        self.rebuild_door_colliders();
        self.rebuild_entity_frames();
        self.authored_audio = self
            .components
            .audio
            .iter()
            .map(|(handle, emitter)| (handle, emitter.clone()))
            .collect();
        self.queue_initial_audio();
    }

    fn queue_initial_audio(&mut self) {
        let starts: Vec<EntityHandle> = self
            .components
            .audio
            .iter()
            .map(|(handle, _)| handle)
            .collect();
        for handle in starts {
            self.queue_audio_start(handle);
        }
    }

    /// Re-seeds every runtime to its authored start state without replacing the
    /// world: the `reset_to_start` contract.
    pub fn reset_runtime(&mut self) {
        self.discard_scene_commands();
        self.scripts.reload();
        self.script_event = None;
        if self.weather_strength > 0.0
            || self.weather_target > 0.0
            || self.weather_cycle_enabled
            || self.weather_cycle_default
        {
            self.weather_strength = 0.0;
            self.weather_target = 0.0;
            self.weather_cycle_enabled = self.weather_cycle_default;
            self.commands
                .push(WorldCommand::SetWeatherAlternate { alternate: false });
            if self.weather_cycle_default {
                self.commands
                    .push(WorldCommand::SetWeatherCycle { enabled: true });
            }
        }
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
        for (handle, authored) in &self.authored_prompts {
            if let Some(component) = self.components.interactables.get_mut(*handle) {
                component.prompt.clone_from(authored);
            }
        }
        self.rebuild_interactables_from_tables();
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
        self.reset_audio();
        self.doors.reset();
        for door in &mut self.animated_doors {
            door.reset();
        }
        self.timers.reset();
        self.spawn_groups.reset();
        self.reset_bindings();
        self.animation_overrides.clear();
        self.events.clear();
        self.stimuli.clear();
        self.sim_time = 0.0;
        self.ai.reset();
        // AI and scripts/light anchors share transforms. Reset both views even
        // when the freshly idle actor will not produce a movement handoff.
        for agent in self.ai.agents() {
            if let Some(transform) = self.components.transforms.get_mut(agent.handle) {
                transform.position = agent.position;
                transform.yaw_degrees = agent.yaw_degrees;
            }
        }
        self.reset_carry_ownership();
        for (_, response) in self.components.light_responses.iter_mut() {
            response.active = false;
            response.release_remaining = 0.0;
            response.intensity = 0.0;
        }
        let live: Vec<EntityHandle> = self.live_spawns.clone();
        for handle in live {
            let _ = self.despawn_handle(handle);
        }
        self.rebuild_door_colliders();
        self.sync_transformed_interactables();
        self.sync_routed_interactables();
        self.rebuild_interactables_from_tables();
        self.rebuild_entity_frames();
    }

    fn discard_scene_commands(&mut self) {
        self.commands.retain(|command| {
            !matches!(
                command,
                WorldCommand::ShowMessage { title: _, lines: _ }
                    | WorldCommand::HandoffItem { target: _ }
                    | WorldCommand::RetireHidden { target: _ }
                    | WorldCommand::ChangeLevel { level: _ }
                    | WorldCommand::EndRun {
                        won: _,
                        title: _,
                        detail: _
                    }
            )
        });
    }

    fn reset_carry_ownership(&mut self) {
        self.script_visible.clear();
        self.script_held_item = None;
        for (_, carry) in self.components.carries.iter_mut() {
            carry.item = None;
            carry.retired = false;
        }
        self.ai.release_caught();
    }

    fn reset_audio(&mut self) {
        let audio: Vec<EntityHandle> = self
            .components
            .audio
            .iter()
            .map(|(handle, _)| handle)
            .collect();
        for handle in audio {
            self.queue_audio_stop(handle);
        }
        for (handle, authored) in &self.authored_audio {
            if let Some(emitter) = self.components.audio.get_mut(*handle) {
                emitter.clone_from(authored);
            }
        }
        let starts: Vec<EntityHandle> = self
            .authored_audio
            .iter()
            .map(|(handle, _)| *handle)
            .collect();
        for handle in starts {
            self.queue_audio_start(handle);
        }
    }

    /// Scalar renderer feedback keeps legacy toggles consistent with the
    /// strength actually displayed after transitions and automatic cycles.
    pub const fn set_weather_state(&mut self, state: (f32, f32, bool)) {
        (
            self.weather_strength,
            self.weather_target,
            self.weather_cycle_enabled,
        ) = state;
    }

    fn clear_bindings(&mut self) {
        self.bindings.clear();
        self.bindings_by_owner.clear();
        self.cooling_bindings.clear();
        self.binding_index_state = BindingIndexState::Current;
    }

    /// Re-arms every binding: `once` bindings may fire again and cooldowns are
    /// cleared. A reset is a fresh run for map wiring, exactly as it is for
    /// doors, timers and spawn groups.
    fn reset_bindings(&mut self) {
        self.cooling_bindings.clear();
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
            if prop.is_physics_movable() {
                self.authored_physics.push(handle);
            }
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            let _previous_value = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(prop.x, base_y, prop.z),
                    yaw_degrees: prop.rotation_degrees,
                    scale: prop.scale,
                },
            );
            drop(self.components.renderables.insert(
                handle,
                Renderable {
                    model: prop.model.clone(),
                    material: None,
                    visible: true,
                    shadow_only: prop.shadow_only,
                    // A floating prop is drawn through the dynamic path; every
                    // other prop is baked into the static world.
                    dynamic: prop.float.is_some(),
                },
            ));
            if prop.solid {
                let _previous_value_2 = self
                    .components
                    .colliders
                    .insert(handle, Collider { size, solid: true });
            }
            self.apply_component_defs(handle, id, &prop.components);
            if let Some(light) = self.components.lights.get_mut(handle) {
                light.fixture =
                    u32::try_from(level.ceiling_lights.len().saturating_add(index)).ok();
                self.authored_lights.push((handle, light.enabled));
            }
            if let Some(response) = self.components.light_responses.get_mut(handle) {
                response.sample_size = Some(size);
            }
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
            let _previous_value = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, base_y, def.z),
                    yaw_degrees: def.rotation_degrees,
                    scale: 1.0,
                },
            );
            drop(self.components.renderables.insert(
                handle,
                Renderable {
                    model: String::new(),
                    material: def.material.clone(),
                    visible: true,
                    shadow_only: false,
                    dynamic: true,
                },
            ));
            self.apply_component_defs(handle, id, &def.components);
            // A door carries its locked state on the door runtime, not on the
            // component, so a condition can read it through `Doors`.
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    fn resolve_fixtures(&mut self, level: &LevelDef) {
        self.light_circuits.clear();
        let mut circuits = std::collections::BTreeMap::<&str, Vec<EntityHandle>>::new();
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
            let _previous_value = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(fixture.x, y, fixture.z),
                    yaw_degrees: fixture.rotation_degrees,
                    scale: 1.0,
                },
            );
            let _previous_value_2 = self.components.lights.insert(
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
            if let Some(group) = fixture.switch_group.as_deref() {
                circuits.entry(group).or_default().push(handle);
            }
            self.resolve_bindings_for(handle, &fixture.bindings);
        }
        let prop_ids = level.prop_instance_ids();
        for (index, prop) in level.props.iter().enumerate() {
            if let Some(group) = prop.switch_group.as_deref()
                && let Some(handle) = prop_ids.get(index).and_then(|id| self.handle_of(id))
                && self
                    .components
                    .lights
                    .get(handle)
                    .is_some_and(|light| light.switchable)
            {
                circuits.entry(group).or_default().push(handle);
            }
        }
        self.light_circuits = circuits.into_values().collect();
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
            let _previous_value = self.components.volumes.insert(
                handle,
                TriggerVolume {
                    bounds: [x0, x1, z0, z1],
                    bottom_y,
                    top_y,
                    inside: false,
                    filter: def.filter.clone(),
                    occupancy: def.occupancy,
                    item_receipt: def.item_receipt,
                    occupants: Vec::new(),
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
            let _previous_value = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, def.y, def.z),
                    yaw_degrees: 0.0,
                    scale: 1.0,
                },
            );
            let _previous_value_2 = self.components.steam.insert(
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
        let volumes = crate::engine::level::WaterVolumes::from_level(level);
        for (index, id) in ids.iter().enumerate() {
            if id.trim().is_empty() || self.names.contains(id) {
                continue;
            }
            let Some(volume) = volumes.volumes().get(index) else {
                continue;
            };
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let _previous_value = self.components.transforms.insert(
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
            let _previous_value_2 = self.components.water.insert(
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
            let _previous_value = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, y, def.z),
                    yaw_degrees: def.yaw_degrees,
                    scale: 1.0,
                },
            );
            drop(self.components.spawn_points.insert(
                handle,
                SpawnPointComponent {
                    template: def.template.clone(),
                    group: def.group.clone(),
                },
            ));
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
                crate::engine::nav::NavAgentProfile::reference()
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
    ///
    /// `instance_id` is the entity's stable instance id; it resolves the
    /// deterministic default phase of a `fade` component whose `phase` is
    /// omitted.
    #[expect(clippy::too_many_lines, reason = "one cohesive component dispatcher")] // one cohesive component dispatcher
    fn apply_component_defs(
        &mut self,
        handle: EntityHandle,
        instance_id: &str,
        defs: &[ComponentDef],
    ) {
        for def in defs {
            match def {
                ComponentDef::Interactable {
                    prompt,
                    reach,
                    enabled,
                    label,
                } => {
                    let prompt_text = prompt
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let reach_m = reach
                        .filter(|candidate_reach| {
                            candidate_reach.is_finite() && *candidate_reach > 0.0
                        })
                        .map_or(DEFAULT_INTERACTION_REACH_M, |valid_reach| {
                            valid_reach.min(MAX_INTERACTION_REACH_M)
                        });
                    drop(
                        self.components.interactables.insert(
                            handle,
                            InteractableComponent {
                                prompt: prompt_text,
                                reach: reach_m,
                                enabled: *enabled,
                                label: label
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|name| !name.is_empty())
                                    .map(str::to_string),
                                label_visible: false,
                            },
                        ),
                    );
                }
                ComponentDef::Animation {
                    clip,
                    speed,
                    looped,
                    playing,
                } => {
                    drop(self.components.animations.insert(
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
                    ));
                }
                ComponentDef::Audio {
                    sound,
                    gain,
                    looped,
                    enabled,
                    playing,
                    time_domain,
                } => {
                    drop(self.components.audio.insert(
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
                            time_domain: *time_domain,
                            spatial: None,
                        },
                    ));
                }
                ComponentDef::Light {
                    enabled,
                    switchable,
                    emission_scale,
                } => {
                    let _previous_value = self.components.lights.insert(
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
                    drop(
                        self.components.materials.insert(
                            handle,
                            Material {
                                variant,
                                variants: variants
                                    .iter()
                                    .map(|v| (v.name.clone(), v.emission_scale))
                                    .collect(),
                            },
                        ),
                    );
                }
                ComponentDef::State { name, value } => {
                    if let Some(state) = self.components.states.get_mut(handle) {
                        let _set_status = state.set(name, value.clone());
                    } else {
                        let mut state = ObjectState::default();
                        let _set_status_2 = state.set(name, value.clone());
                        drop(self.components.states.insert(handle, state));
                    }
                }
                ComponentDef::Lifetime { seconds } => {
                    let _previous_value_2 = self.components.lifetimes.insert(
                        handle,
                        Lifetime {
                            remaining: seconds
                                .is_finite()
                                .then_some(*seconds)
                                .filter(|lifetime| *lifetime > 0.0),
                            despawn: true,
                        },
                    );
                }
                ComponentDef::Steam { enabled } => {
                    let _previous_value_3 = self.components.steam.insert(
                        handle,
                        Steam {
                            enabled: *enabled,
                            effect: None,
                        },
                    );
                }
                ComponentDef::Water { enabled } => {
                    let _previous_value_4 = self
                        .components
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
                    let _previous_value_5 = self.components.nav_agents.insert(
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
                ComponentDef::Ai(ai_def) => {
                    drop(self.components.ais.insert(handle, ai_def.as_ref().clone()));
                }
                ComponentDef::Carry(carry_def) => {
                    drop(self.components.carries.insert(
                        handle,
                        components::Carry {
                            def: carry_def.clone(),
                            item: None,
                            retired: false,
                        },
                    ));
                }
                ComponentDef::LightResponse(response_def) => {
                    let _previous_response = self.components.light_responses.insert(
                        handle,
                        LightResponse {
                            def: *response_def,
                            active: false,
                            release_remaining: 0.0,
                            intensity: 0.0,
                            sample_size: None,
                        },
                    );
                }
                ComponentDef::NavObstacle { size, affects_nav } => {
                    let obstacle_size = size.unwrap_or_else(|| {
                        self.components
                            .colliders
                            .get(handle)
                            .map_or([0.5, 0.5, 0.5], |collider| collider.size)
                    });
                    let _previous_value_6 = self.components.nav_obstacles.insert(
                        handle,
                        NavObstacle {
                            size: obstacle_size,
                            affects_nav: *affects_nav,
                        },
                    );
                }
                ComponentDef::Fade(fade_def) => {
                    // A validated level has already rejected a bad phase; a
                    // non-finite one is defensive here and falls back to the
                    // deterministic per-instance default, exactly like an
                    // omitted phase.
                    let phase = fade_def
                        .phase
                        .filter(|phase| phase.is_finite())
                        .unwrap_or_else(|| crate::engine::level::default_fade_phase(instance_id));
                    let proximity = fade_def.proximity().filter(|proximity| {
                        proximity.near_radius.is_finite()
                            && proximity.near_radius > 0.0
                            && proximity.far_radius.is_finite()
                            && proximity.far_radius > proximity.near_radius
                            && proximity.fade_out_seconds.is_finite()
                            && proximity.fade_out_seconds > 0.0
                            && proximity.fade_in_seconds.is_finite()
                            && proximity.fade_in_seconds > 0.0
                    });
                    let max_opacity = fade_def.max_opacity.clamp(0.0, 1.0);
                    let _previous_value_7 = self.components.fades.insert(
                        handle,
                        Fade {
                            period_seconds: fade_def.period_seconds,
                            phase: phase.clamp(0.0, 1.0),
                            min_opacity: fade_def.min_opacity.clamp(0.0, 1.0),
                            max_opacity,
                            enabled: fade_def.enabled,
                            proximity,
                            // A proximity fade starts visible and fades out
                            // once the first update sees the player inside
                            // its near radius.
                            live_opacity: max_opacity,
                            fading_out: false,
                        },
                    );
                }
                ComponentDef::Glow(glow_def) => {
                    drop(
                        self.components.glows.insert(
                            handle,
                            Glow {
                                enabled: glow_def.enabled,
                                color: glow_def.color,
                                intensity: glow_def.intensity,
                                range: glow_def.range,
                                socket: glow_def
                                    .socket
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|socket| !socket.is_empty())
                                    .map(str::to_string),
                                offset: glow_def.offset,
                                fade_with_opacity: glow_def.fade,
                            },
                        ),
                    );
                }
                ComponentDef::Balloon(balloon_def) => {
                    let mut resolved = *balloon_def;
                    resolved.phase = Some(balloon_def.phase.unwrap_or_else(|| {
                        crate::engine::level::default_fade_phase(
                            self.names.id_of(handle).map_or("", EntityId::as_str),
                        )
                    }));
                    let _previous_balloon = self.components.balloons.insert(handle, resolved);
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
            self.bindings_by_owner
                .entry(handle)
                .or_default()
                .push(self.bindings.len());
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
            if !size.iter().all(|value| value.is_finite() && *value > 0.0)
                || !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
                || !prop.rotation_degrees.is_finite()
                || !prop.pitch_degrees.is_finite()
                || !prop.roll_degrees.is_finite()
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
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let bounds = prop.placement_bounds(base_y, prop.size.unwrap_or(PROP_FALLBACK_SIZE));
            let own_box = prop.solid.then(|| {
                WallAabb::with_y(
                    bounds.min[0],
                    bounds.min[1],
                    bounds.min[2],
                    bounds.max[0] - bounds.min[0],
                    bounds.max[1] - bounds.min[1],
                    bounds.max[2] - bounds.min[2],
                )
            });
            let anchor = Vec3::new(bounds.centre()[0], bounds.max[1] + 0.28, bounds.centre()[2]);
            let (prompt, reach, enabled) = component.map_or_else(
                || (String::new(), DEFAULT_INTERACTION_REACH_M, false),
                |item| (item.prompt.clone(), item.reach, item.enabled),
            );
            items.push(Interactable {
                id: id.to_string(),
                display_name,
                prompt,
                reach,
                anchor,
                bounds,
                yaw_degrees: prop.rotation_degrees,
                size,
                own_box,
                door_index: None,
                leaf_range: None,
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
                yaw_degrees: 0.0,
                size: [door.def.width, door.def.height, door.def.thickness],
                own_box: None,
                door_index: Some(door_index),
                leaf_range: None,
                enabled: component.enabled,
            });
        }
        self.interactables = Interactables::with_items(items);
        self.sync_animated_door_interactables();
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
                item.enabled = component.enabled
                    && !self
                        .components
                        .carries
                        .get(handle)
                        .is_some_and(|carry| carry.retired);
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
                bounds: crate::engine::spatial::Aabb {
                    min: [position.x - extent_x, position.y, position.z - extent_z],
                    max: [
                        position.x + extent_x,
                        position.y + size[1],
                        position.z + extent_z,
                    ],
                },
                size,
                yaw_degrees: yaw,
                own_box: None,
                door_index: None,
                leaf_range: None,
                enabled: component.enabled,
            });
        }
        self.interactables = Interactables::with_items(items);
        self.sync_animated_door_interactables();
    }

    fn rebuild_door_colliders(&mut self) {
        self.door_colliders = self.doors.colliders();
        self.door_colliders.extend(
            self.animated_doors
                .iter()
                .flat_map(|door| door.colliders().iter().copied()),
        );
        self.sync_animated_door_interactables();
    }

    fn sync_animated_door_interactables(&mut self) {
        let mut start = self.doors.len();
        for door in &self.animated_doors {
            let end = start.saturating_add(door.colliders().len());
            self.interactables
                .sync_leaves(&door.id, start, end, &self.door_colliders);
            start = end;
        }
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
            let _sync_door_status =
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

    /// Explicit trace of accepted compound mechanism poses and obstruction.
    #[must_use]
    pub fn animated_door_diagnostics(&self) -> serde_json::Value {
        self.animated_doors
            .iter()
            .map(|door| {
                let mut value = door.diagnostics();
                if let Some(fields) = value.as_object_mut() {
                    drop(
                        fields.insert(
                            "obstacles".into(),
                            self.navigation_obstacles
                                .iter()
                                .map(|bounds| {
                                    serde_json::json!([
                                        [bounds.min_x, bounds.min_y, bounds.min_z],
                                        [bounds.max_x, bounds.max_y, bounds.max_z]
                                    ])
                                })
                                .collect::<Vec<_>>()
                                .into(),
                        ),
                    );
                }
                value
            })
            .collect::<Vec<_>>()
            .into()
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
    #[must_use]
    pub fn permits_transition(&self, level: &str) -> bool {
        self.transition_targets.contains(level)
    }

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
    /// `(controller index, enabled)` pairs: fixtures first, then placed props.
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
            origin: actor
                .and_then(|handle| self.components.transforms.get(handle))
                .map(|pose| pose.position),
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
        self.dispatch_interaction_from(target, None)
    }

    /// Shared interaction dispatch with the interacting occupant's world
    /// position. The action owner remains the aimed entity.
    pub fn dispatch_interaction_from(
        &mut self,
        target: Option<usize>,
        origin: Option<Vec3>,
    ) -> Option<DispatchReport> {
        let index = target?;
        let id = self.interactables.get(index)?.id.clone();
        let handle = self.handle_of(&id)?;
        let position = self
            .components
            .transforms
            .get(handle)
            .map_or(Vec3::ZERO, |transform| transform.position);
        self.note_stimulus(position, 8.0, "interact", Some(handle));
        let _event_queued = self.events.push(EventRecord {
            generation: self.generation,
            kind: EventKind::Interact,
            subject: handle,
            key: String::new(),
            actor: Some(crate::engine::ai::PLAYER_TARGET_HANDLE),
            origin: origin.filter(|point| point.is_finite()),
            depth: 0,
        });
        Some(self.dispatch_queued_events())
    }

    /// Runs one ordered batch of actions programmatically.
    pub fn dispatch_actions(
        &mut self,
        actions: &[ActionDef],
        actor: Option<EntityHandle>,
    ) -> DispatchReport {
        let origin = actor
            .and_then(|handle| self.components.transforms.get(handle))
            .map(|pose| pose.position);
        self.dispatch_actions_from(actions, actor, origin)
    }

    /// Programmatic action surface for game modes and future script bindings.
    /// `actor` owns omitted targets; `origin` is the occupant that caused the
    /// action and lets bidirectional doors swing away from that occupant.
    pub fn dispatch_actions_from(
        &mut self,
        actions: &[ActionDef],
        actor: Option<EntityHandle>,
        origin: Option<Vec3>,
    ) -> DispatchReport {
        let mut report = DispatchReport::default();
        if actions.len() > crate::engine::level::MAX_ACTIONS_PER_SOURCE {
            let refused = actions
                .len()
                .saturating_sub(crate::engine::level::MAX_ACTIONS_PER_SOURCE);
            report.unsupported = report.unsupported.saturating_add(refused);
            warn_once(
                "action-batch-truncated",
                format!(
                    "[entities] an action batch held {} actions; the bound is {} and the tail \
                     was refused",
                    actions.len(),
                    crate::engine::level::MAX_ACTIONS_PER_SOURCE
                ),
            );
        }
        for action in actions
            .iter()
            .take(crate::engine::level::MAX_ACTIONS_PER_SOURCE)
        {
            self.run_action(action, actor, origin, &mut report, 0);
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
    #[expect(
        clippy::too_many_lines,
        reason = "one cohesive typed action dispatcher"
    )] // one cohesive typed action dispatcher
    fn run_action(
        &mut self,
        action: &ActionDef,
        actor: Option<EntityHandle>,
        origin: Option<Vec3>,
        report: &mut DispatchReport,
        depth: u8,
    ) {
        match action {
            ActionDef::ShowMessage { title, lines } => {
                if crate::engine::level::validate_message(title, lines).is_ok() {
                    self.commands.push(WorldCommand::ShowMessage {
                        title: title.clone(),
                        lines: lines.clone(),
                    });
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                }
            }
            ActionDef::HandoffItem { target } | ActionDef::RetireHidden { target } => {
                if self
                    .handle_of(target)
                    .is_some_and(|h| self.components.carries.contains(h))
                {
                    self.commands
                        .push(if matches!(action, ActionDef::HandoffItem { target: _ }) {
                            WorldCommand::HandoffItem {
                                target: target.clone(),
                            }
                        } else {
                            WorldCommand::RetireHidden {
                                target: target.clone(),
                            }
                        });
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                }
            }
            ActionDef::Navigate { target, position } => {
                if self
                    .ai
                    .set_navigation_goal(target, Vec3::from_array(*position))
                {
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                }
            }
            ActionDef::EndRun { won, title, detail } => {
                if crate::engine::level::validate_run_feedback(title, detail).is_ok() {
                    self.commands.push(WorldCommand::EndRun {
                        won: *won,
                        title: title.clone(),
                        detail: detail.clone(),
                    });
                    report.actions_run = report.actions_run.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                }
            }
            ActionDef::RunScript { script, handler } => {
                if u32::from(depth) >= MAX_CHAIN_DEPTH {
                    return;
                }
                let mut runtime = std::mem::take(&mut self.scripts);
                let actions = runtime.call(
                    script,
                    handler,
                    self,
                    actor.and_then(|h| self.id_of(h)),
                    origin,
                    0.0,
                );
                self.scripts = runtime;
                for next in actions {
                    self.run_action(&next, actor, origin, report, depth.saturating_add(1));
                    if report.player_reset {
                        break;
                    }
                }
            }
            ActionDef::ChangeLevel { level } => {
                if self.permits_transition(level)
                    && !self
                        .commands
                        .iter()
                        .any(|command| matches!(command, WorldCommand::ChangeLevel { level: _ }))
                {
                    self.commands.push(WorldCommand::ChangeLevel {
                        level: level.clone(),
                    });
                    report.actions_run = report.actions_run.saturating_add(1);
                }
            }
            ActionDef::ResetToStart => {
                report.actions_run = report.actions_run.saturating_add(1);
                report.player_reset = true;
            }
            ActionDef::ResetPhysicsGroup { group } | ActionDef::ActivatePhysicsGroup { group } => {
                if crate::engine::physics::config::validate_reset_group(group).is_err()
                    || !self.physics_reset_groups.contains(group)
                {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                self.commands.push(
                    if matches!(action, ActionDef::ActivatePhysicsGroup { group: _ }) {
                        WorldCommand::ActivatePhysicsGroup {
                            group: group.clone(),
                        }
                    } else {
                        WorldCommand::ResetPhysicsGroup {
                            group: group.clone(),
                        }
                    },
                );
                report.actions_run = report.actions_run.saturating_add(1);
            }
            ActionDef::ToggleWeather => {
                if self.weather_endpoints == WeatherEndpoints::Absent {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                let alternate = self.weather_strength <= 0.0;
                self.weather_strength = if alternate { 1.0 } else { 0.0 };
                self.weather_target = self.weather_strength;
                self.weather_cycle_enabled = false;
                self.commands
                    .push(WorldCommand::SetWeatherAlternate { alternate });
                report.actions_run = report.actions_run.saturating_add(1);
            }
            ActionDef::SetWeatherStrength {
                strength,
                transition_seconds,
            } => {
                if self.weather_endpoints != WeatherEndpoints::Continuous
                    || crate::engine::weather::validate_strength(*strength, *transition_seconds)
                        .is_err()
                {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                self.weather_target = *strength;
                if *transition_seconds <= 0.0 {
                    self.weather_strength = *strength;
                }
                self.weather_cycle_enabled = false;
                self.commands.push(WorldCommand::SetWeatherStrength {
                    strength: *strength,
                    transition_seconds: *transition_seconds,
                });
                report.actions_run = report.actions_run.saturating_add(1);
            }
            ActionDef::SetWeatherCycle { enabled } => {
                if !self.weather_cycle_available
                    || self.weather_endpoints != WeatherEndpoints::Continuous
                {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                self.weather_cycle_enabled = *enabled;
                self.commands
                    .push(WorldCommand::SetWeatherCycle { enabled: *enabled });
                report.actions_run = report.actions_run.saturating_add(1);
            }
            ActionDef::SetPrompt { target, prompt } => {
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                let target_handle = self.handle_of(&id);
                let prompt_component = target_handle.and_then(|resolved_handle| {
                    self.components.interactables.get_mut(resolved_handle)
                });
                let prompt_text = prompt.trim();
                let Some(component) = prompt_component.filter(|_| !prompt_text.is_empty()) else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                };
                prompt_text.clone_into(&mut component.prompt);
                let _prompt_published = self.interactables.set_prompt(&id, prompt_text);
                report.actions_run = report.actions_run.saturating_add(1);
            }
            ActionDef::Open { target } | ActionDef::Close { target } => {
                let request_open = matches!(action, ActionDef::Open { target: _ });
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
                    self.doors.request_open_from(&id, origin)
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
                    if self.doors.toggle_from(&id, origin) {
                        report.actions_run = report.actions_run.saturating_add(1);
                        report.doors_acted = report.doors_acted.saturating_add(1);
                    }
                    return;
                }
                if self.toggle_light_by_id(&id, depth.saturating_add(1)) {
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
                match self.set_light_by_id(&id, *on, depth.saturating_add(1)) {
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
                let enable = matches!(action, ActionDef::Enable { target: _ });
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.set_enabled_by_id(&id, enable, depth.saturating_add(1)) {
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
                let locked = matches!(action, ActionDef::Lock { target: _ });
                let Some(id) = self.resolve_target(target.as_deref(), actor) else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.doors.set_locked(&id, locked) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    let _event_queued = self.emit_depth(
                        EventKind::ObjectState,
                        self.handle_of(&id)
                            .unwrap_or(EntityHandle::from_parts(0, 0)),
                        "locked",
                        actor,
                        depth.saturating_add(1),
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
                let Some(target_id) = id else {
                    report.missing_targets = report.missing_targets.saturating_add(1);
                    return;
                };
                if self.start_sequence(&target_id, sequence.trim()) {
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.sequences_started = report.sequences_started.saturating_add(1);
                } else {
                    report.unsupported = report.unsupported.saturating_add(1);
                    warn_once(
                        "start-sequence-unknown",
                        format!("[entities] sequence `{sequence}` cannot start on `{target_id}`"),
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
                let speed_mps = speed
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(DEFAULT_MOVE_SPEED_MPS);
                self.moves.retain(|goal| goal.entity != handle);
                self.moves.push(MoveGoal {
                    entity: handle,
                    target: Vec3::new(*x, target_y, *z),
                    speed: speed_mps,
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
                let sound_name = sound
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| emitter.sound.clone(), str::to_string);
                if sound_name.is_empty() {
                    report.unsupported = report.unsupported.saturating_add(1);
                    return;
                }
                if let Some(entry) = self.components.audio.get_mut(handle) {
                    entry.playing = true;
                    entry.looped = *looped;
                    entry.sound = sound_name;
                }
                self.queue_audio_start(handle);
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
            let _set_status = state.set(name, value);
            drop(self.components.states.insert(handle, state));
        }
        let _event_queued = self.emit_depth(EventKind::ObjectState, handle, name, actor, depth);
        true
    }

    fn toggle_light_by_id(&mut self, id: &str, depth: u8) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let Some(light) = self.components.lights.get(handle).copied() else {
            return false;
        };
        if !light.switchable {
            return false;
        }
        self.set_light(handle, !light.enabled, depth)
    }

    fn set_light_by_id(&mut self, id: &str, on: bool, depth: u8) -> LightOutcome {
        let Some(handle) = self.handle_of(id) else {
            return LightOutcome::NotALight;
        };
        match self.components.lights.get(handle) {
            Some(light) if light.switchable => {}
            Some(_) => return LightOutcome::NotSwitchable,
            None => return LightOutcome::NotALight,
        }
        if self.set_light(handle, on, depth) {
            LightOutcome::Changed
        } else {
            LightOutcome::Unchanged
        }
    }

    /// Sets one light component, marking it dirty on a real change.
    fn set_light(&mut self, handle: EntityHandle, on: bool, depth: u8) -> bool {
        let members = self
            .light_circuits
            .iter()
            .find(|members| members.contains(&handle))
            .cloned()
            .unwrap_or_else(|| vec![handle]);
        let mut changed = false;
        for member in members {
            changed |= self.set_single_light(member, on, depth);
        }
        changed
    }

    fn set_single_light(&mut self, handle: EntityHandle, on: bool, depth: u8) -> bool {
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
        let _event_queued = self.emit_state(&id, "light", StateValue::Bool(on), None, depth);
        true
    }

    fn set_enabled_by_id(&mut self, id: &str, enable: bool, depth: u8) -> bool {
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
        let audio_changed = self.components.audio.get_mut(handle).is_some_and(|entry| {
            let before = entry.enabled;
            entry.enabled = enable;
            before != enable
        });
        if audio_changed {
            if enable {
                self.queue_audio_start(handle);
            } else {
                self.queue_audio_stop(handle);
            }
        }
        changed |= audio_changed;
        changed |= self
            .components
            .balloons
            .get_mut(handle)
            .is_some_and(|entry| {
                let before = entry.enabled;
                entry.enabled = enable;
                before != enable
            });
        changed |= self.components.glows.get_mut(handle).is_some_and(|entry| {
            let before = entry.enabled;
            entry.enabled = enable;
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
            && self.set_light(handle, enable, depth)
        {
            changed = true;
        }
        if changed {
            self.rebuild_interactables_from_tables();
            self.rebuild_entity_frames();
            let _event_queued =
                self.emit_state(id, "enabled", StateValue::Bool(enable), None, depth);
        }
        changed
    }

    fn set_animation_cue(&mut self, instance_id: &str, clip: &str, looped: bool, toggle: bool) {
        if let Some(door) = self
            .animated_doors
            .iter_mut()
            .find(|door| door.id == instance_id)
        {
            let _accepted = door.request(clip, toggle);
            return;
        }
        let next = if toggle {
            let current = self
                .animation_overrides
                .iter()
                .rev()
                .find(|(id, _)| id == instance_id)
                .and_then(|(_, cue)| match cue {
                    PoseCue::Scrub { target, name: _ } => Some(*target),
                    PoseCue::Idle
                    | PoseCue::Sample {
                        name: _,
                        progress: _,
                    }
                    | PoseCue::Walk { speed_mps: _ }
                    | PoseCue::Clip {
                        name: _,
                        once: _,
                        paused: _,
                    } => None,
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

    /// The authored water-volume index of a water entity, when it has one.
    fn water_volume_index(&self, handle: EntityHandle) -> Option<u32> {
        let _configured_get = self.components.water.get(handle)?;
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
        let _tick_report = self.timers.tick(delta, |index| fires.push(index));
        for index in fires {
            let Some(runtime) = self.timers.at(index) else {
                continue;
            };
            let id = runtime.def.id.clone();
            let Some(handle) = self.handle_of(&id) else {
                continue;
            };
            let _event_queued = self.emit_depth(EventKind::Timer, handle, &id, None, 1);
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
        self.cooling_bindings.retain(|index| {
            let Some(binding) = self.bindings.get_mut(*index) else {
                return false;
            };
            binding.cooldown_remaining = (binding.cooldown_remaining - delta).max(0.0);
            binding.cooldown_remaining > 0.0
        });
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "bounded flat movement arithmetic"
    )] // bounded flat movement arithmetic
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
            if (resolved - candidate).length() > crate::engine::entity::ENTITY_BLOCK_EPS_M {
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
        while !self.events.is_empty() && tick.events_processed < MAX_EVENTS_PER_TICK {
            let wave = self.events.len();
            for _ in 0..wave {
                if tick.events_processed >= MAX_EVENTS_PER_TICK {
                    // Keep the bounded tail for the next pump. In particular an
                    // already accepted exit must not lose its cleanup action.
                    break;
                }
                let Some(event) = self.events.pop() else {
                    break;
                };
                tick.events_processed = tick.events_processed.saturating_add(1);
                if event.generation != self.generation || !self.store.contains(event.subject) {
                    continue;
                }
                if u32::from(event.depth) >= MAX_CHAIN_DEPTH {
                    tick.events_dropped = tick.events_dropped.saturating_add(1);
                    warn_once(
                        "event-chain-depth",
                        format!(
                            "[entities] event chain reached the depth limit ({MAX_CHAIN_DEPTH}); offending occurrence dropped"
                        ),
                    );
                    continue;
                }
                if !self.scripts.is_empty() {
                    self.script_event = Some(event.clone());
                }
                self.run_bindings(&event, tick, event.depth);
                self.script_event = None;
                if tick.player_reset {
                    return;
                }
            }
        }
        self.compact_bindings();
    }

    fn run_bindings(&mut self, event: &EventRecord, tick: &mut WorldTick, depth: u8) {
        // Resolve only this owner's bounded authored binding list. Conditions
        // are captured before actions mutate state, preserving one-toggle rules.
        let mut matched = [None; crate::engine::level::MAX_BINDINGS_PER_ENTITY];
        let mut next = 0usize;
        for index in self
            .bindings_by_owner
            .get(&event.subject)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .copied()
        {
            let Some(binding) = self.bindings.get(index) else {
                continue;
            };
            if binding.on != event.kind
                || binding.key.as_ref().is_some_and(|key| key != &event.key)
                || !binding
                    .when
                    .iter()
                    .all(|condition| self.condition_holds(condition))
            {
                continue;
            }
            if let Some(slot) = matched.get_mut(next) {
                *slot = Some(index);
                next = next.saturating_add(1);
            }
        }
        for index in matched.into_iter().flatten() {
            let Some(binding) = self.bindings.get(index).cloned() else {
                continue;
            };
            if binding.fired && binding.once {
                continue;
            }
            if binding.cooldown_remaining > 0.0 {
                continue;
            }
            let mut report = DispatchReport::default();
            for action in &binding.actions {
                self.run_action(
                    action,
                    Some(event.subject),
                    event.origin,
                    &mut report,
                    depth,
                );
                if report.player_reset {
                    tick.player_reset = true;
                    break;
                }
            }
            tick.record_dispatch(&report);
            if let Some(entry) = self.bindings.get_mut(index) {
                entry.fired = true;
                entry.cooldown_remaining = binding.cooldown_seconds;
                if binding.cooldown_seconds > 0.0 {
                    self.cooling_bindings.push(index);
                }
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
        if self.binding_index_state == BindingIndexState::Current {
            return;
        }
        self.binding_index_state = BindingIndexState::Current;
        self.bindings
            .retain(|binding| self.store.contains(binding.owner));
        self.bindings_by_owner.clear();
        self.cooling_bindings.clear();
        for (index, binding) in self.bindings.iter().enumerate() {
            self.bindings_by_owner
                .entry(binding.owner)
                .or_default()
                .push(index);
            if binding.cooldown_remaining > 0.0 {
                self.cooling_bindings.push(index);
            }
        }
    }

    // ---- spawns --------------------------------------------------------

    #[expect(clippy::too_many_lines, reason = "one cohesive spawn validation path")] // one cohesive spawn validation path
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
        let Some(spawn_template) = self
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
            let group_name = self
                .spawn_groups
                .at(index)
                .map_or("?", |spawn_group| spawn_group.def.id.as_str());
            return Err(SpawnError::Refused(format!(
                "spawn group `{group_name}` already has a live member"
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
        if spawn_template.model.trim().is_empty()
            || !spawn_template.scale.is_finite()
            || spawn_template.scale <= 0.0
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
            .filter(|candidate_name| !candidate_name.is_empty())
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
        let _previous_value = self.components.transforms.insert(
            handle,
            Transform {
                position: Vec3::new(authored_point.x, base_y, authored_point.z),
                yaw_degrees: authored_point.yaw_degrees,
                scale: spawn_template.scale,
            },
        );
        drop(self.components.renderables.insert(
            handle,
            Renderable {
                model: spawn_template.model.clone(),
                material: None,
                visible: true,
                shadow_only: spawn_template.shadow_only,
                dynamic: true,
            },
        ));
        if let Some(seconds) = spawn_template
            .lifetime_seconds
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        {
            let _previous_value_2 = self.components.lifetimes.insert(
                handle,
                Lifetime {
                    remaining: Some(seconds),
                    despawn: true,
                },
            );
        }
        self.apply_component_defs(handle, &runtime_name, &spawn_template.components);
        self.resolve_bindings_for(handle, &spawn_template.bindings);
        self.resolve_bindings_for(handle, &authored_point.bindings);
        self.spawns_this_tick = self.spawns_this_tick.saturating_add(1);
        let key = self.next_dynamic_key;
        self.next_dynamic_key = self.next_dynamic_key.saturating_add(1);
        self.dynamic_keys.push((handle, key));
        self.live_spawns.push(handle);
        self.queue_audio_start(handle);
        if let Some(index) = group_index {
            let _ = self.spawn_groups.occupy(index, handle);
        }
        self.commands.push(WorldCommand::SpawnDynamic {
            entity: key,
            model: spawn_template.model.clone(),
            position: Vec3::new(authored_point.x, base_y, authored_point.z),
            yaw_degrees: authored_point.yaw_degrees,
            scale: spawn_template.scale,
        });
        let _event_queued = self.emit_depth(
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
                instance_id: id.unwrap_or_default(),
            });
        }
        self.remove_entity_handle(handle)
    }

    /// Releases component ownership after the caller handles render lifetime.
    /// Binding slots stay stable until the current event wave has finished.
    fn remove_entity_handle(&mut self, handle: EntityHandle) -> bool {
        let id = self.id_of(handle).map(str::to_string);
        if self.components.audio.contains(handle)
            && let Some(key) = self.dynamic_key(handle).or_else(|| self.stable_key(handle))
        {
            self.commands.push(WorldCommand::StopSound { entity: key });
        }
        self.sequence_runs.retain(|run| run.owner != handle);
        self.moves.retain(|goal| goal.entity != handle);
        self.dynamic_keys
            .retain(|(candidate, _)| *candidate != handle);
        self.live_spawns.retain(|candidate| *candidate != handle);
        self.authored_physics
            .retain(|candidate| *candidate != handle);
        self.authored_lights
            .retain(|(candidate, _)| *candidate != handle);
        self.authored_states
            .retain(|(candidate, _)| *candidate != handle);
        self.authored_prompts
            .retain(|(candidate, _)| *candidate != handle);
        self.authored_audio
            .retain(|(candidate, _)| *candidate != handle);
        for circuit in &mut self.light_circuits {
            circuit.retain(|candidate| *candidate != handle);
        }
        self.light_circuits.retain(|circuit| !circuit.is_empty());
        self.stimuli
            .retain(|stimulus| stimulus.source != Some(handle));
        let _release_status = self.spawn_groups.release(handle);
        let _removed_value = self.ai.remove(handle);
        if let Some(removed_id) = id {
            self.animation_overrides
                .retain(|(candidate, _)| candidate != &removed_id);
            let _removed_value_2 = self.names.remove(&removed_id);
        }
        if self.bindings_by_owner.contains_key(&handle) {
            self.binding_index_state = BindingIndexState::NeedsCompaction;
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

    // ---- tick ----------------------------------------------------------

    /// Samples prepared handheld lighting for the following AI tick.
    ///
    /// `sample` returns incident intensity using the renderer's same cone,
    /// distance falloff and static/live geometry visibility. Its instance id
    /// lets the shared visibility query exclude the receiving mesh itself;
    /// walls, floors, doors and other objects must remain occluders. Five
    /// torso/head anchors make a genuinely visible partial body responsive.
    /// No geometry or visibility rule is duplicated in the entity runtime.
    pub fn update_light_observations(
        &mut self,
        delta_seconds: f32,
        player_eye: Vec3,
        mut sample: impl FnMut(&str, Vec3) -> f32,
    ) {
        for (handle, response) in self.components.light_responses.iter_mut() {
            let Some(instance_id) = self.names.id_of(handle).map(EntityId::as_str) else {
                continue;
            };
            let Some(transform) = self.components.transforms.get(handle) else {
                continue;
            };
            let size = response.sample_size.unwrap_or_else(|| {
                self.components
                    .nav_agents
                    .get(handle)
                    .map_or(PROP_FALLBACK_SIZE, |body| {
                        [body.radius * 2.0, body.height, body.radius * 2.0]
                    })
            });
            let yaw = transform.yaw_degrees.to_radians();
            let side_x = yaw.cos() * (size[0] * 0.32);
            let side_z = -yaw.sin() * (size[0] * 0.32);
            let torso = Vec3::new(
                transform.position.x,
                size[1].mul_add(0.62, transform.position.y),
                transform.position.z,
            );
            let samples = [
                torso,
                Vec3::new(torso.x + side_x, torso.y, torso.z + side_z),
                Vec3::new(torso.x - side_x, torso.y, torso.z - side_z),
                Vec3::new(
                    transform.position.x,
                    size[1].mul_add(0.88, transform.position.y),
                    transform.position.z,
                ),
                Vec3::new(
                    transform.position.x,
                    size[1].mul_add(0.36, transform.position.y),
                    transform.position.z,
                ),
            ];
            let intensity = samples.into_iter().fold(0.0_f32, |strongest, point| {
                let measured = sample(instance_id, point);
                if measured.is_finite() {
                    strongest.max(measured)
                } else {
                    strongest
                }
            });
            response.observe(intensity, delta_seconds);
            let target = (response.active && player_eye.is_finite()).then_some(player_eye);
            let _response_changed = self.ai.set_light_response(handle, target);
        }
        if self
            .components
            .light_responses
            .iter()
            .any(|(_, response)| response.def.reveal_only)
        {
            self.rebuild_entity_frames();
        }
    }

    /// Current direct-light observations for trace and acceptance tooling.
    pub fn light_response_observations(
        &self,
    ) -> impl Iterator<Item = LightResponseObservation<'_>> {
        self.components
            .light_responses
            .iter()
            .filter_map(|(handle, response)| {
                let instance_id = self.id_of(handle)?;
                let transform = self.components.transforms.get(handle)?;
                Some(LightResponseObservation {
                    instance_id,
                    position: transform.position,
                    intensity: response.intensity,
                    active: response.active,
                })
            })
    }

    pub(crate) fn script_event_context(&self) -> Option<(&str, &str, Option<&str>)> {
        self.script_event.as_ref().map(|event| {
            (
                event.kind.name(),
                event.key.as_str(),
                event.actor.and_then(|handle| self.id_of(handle)),
            )
        })
    }

    /// Publishes a bounded owned identity snapshot before script invocation.
    pub fn observe_held_item(&mut self, item: Option<String>) {
        self.script_held_item = item;
    }

    /// Completes ownership only after the physics item was removed successfully.
    pub fn confirm_handoff(&mut self, target: &str, item: &str) {
        if let Some(handle) = self.handle_of(target)
            && let Some(carry) = self.components.carries.get_mut(handle)
        {
            carry.item = Some(item.to_string());
            self.script_held_item = None;
            self.rebuild_entity_frames();
        }
    }

    /// Retires locomotion/interaction after the presentation guard hid both meshes.
    pub fn confirm_carry_retired(&mut self, target: &str) {
        if let Some(handle) = self.handle_of(target) {
            if let Some(carry) = self.components.carries.get_mut(handle) {
                carry.retired = true;
            }
            let _frozen = self.ai.mark_caught(handle);
            self.rebuild_interactables_from_tables();
            self.rebuild_entity_frames();
        }
    }

    /// Explicit lifetime evidence; ordinary ticks do not allocate this snapshot.
    #[must_use]
    pub fn script_resources(&self) -> serde_json::Value {
        self.scripts.resource_snapshot()
    }

    /// Bounded diagnostics for the current script run.
    #[must_use]
    pub fn script_diagnostics(&self) -> &[String] {
        self.scripts.diagnostics()
    }

    /// Optional player-requested guidance uses the same scoped VM/action budgets.
    /// True requests the caller's shared player/world reset lifecycle.
    pub fn request_hint(&mut self) -> bool {
        let mut runtime = std::mem::take(&mut self.scripts);
        let mut player_reset = false;
        for path in runtime.hints() {
            let actions = runtime.call(&path, "hint", self, None, None, 0.0);
            if self.dispatch_actions(&actions, None).player_reset {
                player_reset = true;
                break;
            }
        }
        self.scripts = runtime;
        player_reset
    }

    /// Advances the whole runtime by one frame.
    pub fn tick(&mut self, ctx: &WorldContext<'_>) -> WorldTick {
        let mut tick = WorldTick::default();
        self.script_player = ctx.feet;
        self.script_visible.clear();
        for agent in self.ai.agents() {
            let torso = Vec3::new(
                agent.position.x,
                agent.profile.height.mul_add(0.6, agent.position.y),
                agent.position.z,
            );
            if crate::engine::ai::sight_clear(ctx.index, ctx.walls, &self.doors, ctx.eye, torso)
                && !self.navigation_obstacles.iter().any(|wall| {
                    crate::engine::collision::segment_overlaps_aabb(
                        ctx.eye,
                        torso,
                        [wall.min_x, wall.min_y, wall.min_z],
                        [wall.max_x, wall.max_y, wall.max_z],
                    )
                })
            {
                let _added = self.script_visible.insert(agent.handle);
            }
        }
        self.spawns_this_tick = 0;
        self.scripts.begin_tick();
        if ctx.delta_seconds.is_finite() && ctx.delta_seconds > 0.0 {
            let mut runtime = std::mem::take(&mut self.scripts);
            let automatic = runtime.automatic();
            for (path, handler) in automatic {
                let actions = runtime.call(&path, handler, self, None, None, ctx.delta_seconds);
                let report = self.dispatch_actions(&actions, None);
                tick.record_dispatch(&report);
                if tick.player_reset {
                    break;
                }
            }
            self.scripts = runtime;
            if tick.player_reset {
                return tick;
            }
        }
        if ctx.delta_seconds.is_finite() && ctx.delta_seconds > 0.0 {
            self.sim_time += ctx.delta_seconds;
            self.tick_bindings(ctx.delta_seconds);
            self.tick_timers(ctx.delta_seconds, &mut tick);
            self.tick_sequences(ctx, &mut tick);
            if tick.player_reset {
                return tick;
            }
            self.tick_ai(ctx, &mut tick);
            self.tick_moves(ctx, &mut tick);
            self.tick_lifetimes(ctx.delta_seconds, &mut tick);
        }
        // Player interactions may still enqueue actions while world time is
        // stopped. Dispatch those without advancing any simulation controller.
        self.pump_events(&mut tick);
        self.report_queue_refusals(&mut tick);
        if tick.frames_dirty {
            self.sync_transformed_interactables();
            self.sync_routed_interactables();
        }
        if tick.frames_dirty {
            // Routes and fades publish after this tick in update_entities;
            // only action/AI changes require an earlier handoff here.
            self.rebuild_entity_frames();
        } else {
            self.refresh_balloon_frames();
        }
        tick
    }

    /// Publishes current physical solids before the next AI simulation tick.
    pub fn set_navigation_obstacles(&mut self, obstacles: impl Iterator<Item = WallAabb>) {
        self.navigation_obstacles.clear();
        self.navigation_obstacles
            .extend(obstacles.take(crate::engine::physics::MAX_BODIES));
    }

    /// Assigns a developer inspection goal to a live agent using normal motion.
    pub fn set_ai_navigation_goal(&mut self, id: &str, goal: Vec3) -> bool {
        self.ai.set_navigation_goal(id, goal)
    }

    /// Advances the shared AI runtime: perception, decisions, locomotion,
    /// catch detection and the stimuli and commands the tick produced.
    fn tick_ai(&mut self, ctx: &WorldContext<'_>, tick: &mut WorldTick) {
        self.age_stimuli(ctx.delta_seconds);
        if self.ai.is_empty() {
            return;
        }
        let mut targets: Vec<AiTarget> = self
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
                caught: agent.caught || matches!(agent.state, crate::engine::ai::AiState::Hidden),
            })
            .collect();
        targets.push(AiTarget {
            handle: crate::engine::ai::PLAYER_TARGET_HANDLE,
            instance_id: "player".to_string(),
            role: Some("player".to_string()),
            behavior: crate::engine::ai::AiBehavior::Idler,
            position: ctx.feet,
            radius: crate::engine::collision::PLAYER_RADIUS,
            height: ctx.body_height,
            caught: false,
        });
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
            ceiling: &self.ceiling,
            leaves: &self.door_colliders,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &scripted,
            obstacles: &self.navigation_obstacles,
        };
        let mut outcome = AiOutcome::default();
        self.ai.tick(&ai_ctx, &mut outcome);
        tick.frames_dirty |= outcome.transitions > 0 || outcome.cues_changed;
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
            let _event_queued = self.emit_depth(kind, subject, &key, actor, 0);
        }
        for (door_id, agent) in outcome.door_requests {
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
            let origin = self
                .components
                .transforms
                .get(agent)
                .map(|pose| pose.position);
            let _ = self.doors.request_open_from(&door_id, origin);
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
        while self.stimuli.len() > crate::engine::ai::perception::MAX_STIMULI {
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
        while self.stimuli.len() >= crate::engine::ai::perception::MAX_STIMULI {
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

    /// Advances every moving door and republishes its collider and aim bound.
    pub fn update_doors(&mut self, ctx: &WorldContext<'_>) -> usize {
        if !self.doors.any_moving()
            && !self
                .animated_doors
                .iter()
                .any(crate::engine::animated_door::AnimatedDoor::is_moving)
        {
            return 0;
        }
        let player = ctx.feet;
        let body = ctx.body_height;
        let index = ctx.index;
        let walls = ctx.walls;
        let agents = self.ai.agents();
        let obstacles = &self.navigation_obstacles;
        let mut moved = self.doors.advance(ctx.delta_seconds, |_, candidate| {
            door_pose_hits_player(candidate, player, body)
                || door_pose_hits_static(index, walls, candidate)
                || agents.iter().any(|agent| {
                    !matches!(agent.state, crate::engine::ai::AiState::Hidden)
                        && candidate.overlaps_body_y(agent.position.y, agent.profile.height)
                        && candidate.overlaps_disc(
                            agent.position.x,
                            agent.position.z,
                            agent.profile.radius,
                        )
                })
                || obstacles
                    .iter()
                    .any(|solid| door_pose_hits_box(candidate, solid))
        });
        for slot in 0..self.animated_doors.len() {
            if self
                .animated_doors
                .get(slot)
                .is_none_or(|door| !door.is_moving())
            {
                continue;
            }
            let other: Vec<DoorCollider> = self
                .doors
                .colliders()
                .into_iter()
                .chain(
                    self.animated_doors
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != slot)
                        .flat_map(|(_, door)| door.colliders().iter().copied()),
                )
                .collect();
            let Some(door) = self.animated_doors.get_mut(slot) else {
                continue;
            };
            if door.advance(ctx.delta_seconds, |candidate| {
                door_pose_hits_player(candidate, player, body)
                    || walls.iter().any(|wall| door_pose_hits_box(candidate, wall))
                    || obstacles
                        .iter()
                        .any(|solid| door_pose_hits_box(candidate, solid))
                    || agents.iter().any(|agent| {
                        !matches!(agent.state, crate::engine::ai::AiState::Hidden)
                            && candidate.overlaps_body_y(agent.position.y, agent.profile.height)
                            && candidate.overlaps_disc(
                                agent.position.x,
                                agent.position.z,
                                agent.profile.radius,
                            )
                    })
                    || other.iter().any(|leaf| candidate.overlaps_leaf(leaf))
            }) {
                moved = moved.saturating_add(1);
                self.doors.notify_clearance_changed();
            }
        }
        if moved > 0 {
            self.rebuild_door_colliders();
            self.sync_door_interactables();
            self.rebuild_entity_frames();
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
        let completed_clip = if clip.trim().is_empty() {
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
                        timeout: _,
                    } => *wait_clip == completed_clip,
                    SequenceStepDef::WaitAnimation {
                        clip: None,
                        timeout: _,
                    } => true,
                    SequenceStepDef::Action { action: _ }
                    | SequenceStepDef::Wait { seconds: _ }
                    | SequenceStepDef::Move {
                        x: _,
                        y: _,
                        z: _,
                        speed: _,
                    }
                    | SequenceStepDef::Face { yaw_degrees: _ }
                    | SequenceStepDef::Emit { on: _, key: _ }
                    | SequenceStepDef::SetState { name: _, value: _ }
                    | SequenceStepDef::Stop => false,
                });
            if waiting {
                run.animation_complete = true;
            }
        }
        let _event_queued = self.emit(
            EventKind::AnimationComplete,
            handle,
            &completed_clip,
            Some(handle),
        );
    }

    /// Advances every authored route and proximity fade, then republishes the
    /// entity frames.
    ///
    /// `player` is the player's current world position: the horizontal
    /// distance from it drives every proximity fade. `None` (load seeding and
    /// resets) holds the controllers, so no entity fades until the first frame
    /// the player is known.
    #[expect(clippy::arithmetic_side_effects, reason = "bounded pose comparisons")] // bounded pose comparisons
    pub fn update_entities(&mut self, delta: f32, world: &RouteWorld<'_>, player: Option<Vec3>) {
        if !self.route_states.iter().any(|state| !state.finished) && !self.any_enabled_fade() {
            // Completed routes retain their last handoff; no frame strings or
            // pose overrides need rebuilding until a reset or action changes them.
            return;
        }
        if !self.routes.is_empty() {
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
        }
        self.advance_proximity_fades(delta, player);
        self.rebuild_entity_frames();
    }

    /// Fixed substep the proximity fade controller integrates in, in seconds.
    ///
    /// Matches the frame loop's own fixed vertical substep
    /// (`VERTICAL_SUBSTEP`, 1/120 s), so a fade is frame-rate independent
    /// exactly like the jump and swim integration. A test pins the two
    /// constants together.
    const FADE_SUBSTEP_SECONDS: f32 = 1.0 / 120.0;

    /// Upper bound on one call's fade time, in seconds.
    ///
    /// A guard against a corrupt caller, not a tuning knob: the frame loop's
    /// own simulation delta is clamped to 0.1 s, so a real call is far below
    /// this and a pathological one cannot spin the substep loop.
    const MAX_FADE_STEP_SECONDS: f32 = 60.0;

    /// Upper bound on the substeps one [`Self::advance_proximity_fades`] call
    /// integrates: [`Self::MAX_FADE_STEP_SECONDS`] (60 s) divided by
    /// [`Self::FADE_SUBSTEP_SECONDS`] (1/120 s).
    const MAX_FADE_SUBSTEPS: usize = 7_200;

    /// Advances every proximity fade by `delta`, in fixed substeps.
    ///
    /// The direction each controller holds is decided once per call from the
    /// player's horizontal distance to the entity's **live** position (routes
    /// and AI advanced above, so a moving ghost fades against where it is);
    /// the live opacity then advances from its current value in
    /// [`Self::FADE_SUBSTEP_SECONDS`] substeps, so 30, 60 and 144 fps produce
    /// the same opacity for the same elapsed time and an interrupted fade
    /// never jumps.
    fn advance_proximity_fades(&mut self, delta: f32, player: Option<Vec3>) {
        if !self
            .components
            .fades
            .iter()
            .any(|(_, fade)| fade.is_proximity())
        {
            return;
        }
        let Some(player_position) = player.filter(|position| position.is_finite()) else {
            return;
        };
        let handles: Vec<(EntityHandle, f32)> = self
            .components
            .fades
            .iter()
            .filter(|(_, fade)| fade.is_proximity())
            .map(|(handle, _)| {
                let distance = self
                    .live_position(handle)
                    .map_or(f32::INFINITY, |position| {
                        let dx = position.x - player_position.x;
                        let dz = position.z - player_position.z;
                        dx.hypot(dz)
                    });
                (handle, distance)
            })
            .collect();
        for (handle, distance) in &handles {
            if let Some(fade) = self.components.fades.get_mut(*handle) {
                fade.hold_direction(*distance);
            }
        }
        let step_delta = if delta.is_finite() {
            delta.clamp(0.0, Self::MAX_FADE_STEP_SECONDS)
        } else {
            0.0
        };
        let mut remaining = step_delta;
        // A bounded float countdown: at most the 60 s guard divided by the
        // substep, so a corrupt caller can never spin this loop.
        for _ in 0..Self::MAX_FADE_SUBSTEPS {
            if remaining <= 0.0 {
                break;
            }
            let step = remaining.min(Self::FADE_SUBSTEP_SECONDS);
            remaining -= step;
            for (handle, _) in &handles {
                if let Some(fade) = self.components.fades.get_mut(*handle) {
                    fade.advance(step);
                }
            }
        }
    }

    /// The live world position of one entity, when it has one.
    ///
    /// An AI agent or routed entity reports its live state; everything else
    /// reports its transform. A proximity fade measures against this point, so
    /// it tracks the entity wherever its route, AI or spawn took it.
    fn live_position(&self, handle: EntityHandle) -> Option<Vec3> {
        let instance_id = self.id_of(handle)?;
        if let Some(agent) = self
            .ai
            .agents()
            .iter()
            .find(|agent| agent.instance_id == instance_id)
        {
            return Some(agent.position);
        }
        for (index, route) in self.routes.routes().iter().enumerate() {
            if route.instance_id == instance_id {
                return self.route_states.get(index).map(|state| state.position);
            }
        }
        self.components
            .transforms
            .get(handle)
            .map(|item| item.position)
    }

    /// The fade opacity of one entity at the current simulation time; `1.0`
    /// when it has no `fade` component.
    fn frame_opacity(&self, handle: EntityHandle) -> f32 {
        if self
            .components
            .light_responses
            .get(handle)
            .is_some_and(|response| {
                response.def.reveal_only && response.intensity < response.def.exit_intensity
            })
        {
            return 0.0;
        }
        self.components
            .fades
            .get(handle)
            .map_or(1.0, |fade| fade.opacity_at(self.sim_time))
    }

    /// The attached-light cue of one entity, when it carries a `glow`.
    fn frame_glow(&self, handle: EntityHandle) -> Option<GlowCue> {
        self.components
            .glows
            .get(handle)
            .filter(|glow| glow.enabled)
            .map(|glow| GlowCue {
                socket: glow.socket.clone(),
                offset: glow.offset,
                color: glow.color,
                intensity: glow.intensity,
                range: glow.range,
                fade_with_opacity: glow.fade_with_opacity,
            })
    }

    fn frame_shadow_only(&self, instance_id: &str) -> bool {
        self.handle_of(instance_id)
            .and_then(|handle| self.components.renderables.get(handle))
            .is_some_and(|renderable| renderable.shadow_only)
    }

    fn frame_balloon(&self, handle: EntityHandle) -> Option<balloon::BalloonPose> {
        self.components
            .balloons
            .get(handle)
            .map(|def| balloon::sample(def, self.sim_time))
    }

    /// Refreshes retained balloon cues without rebuilding frame ids or vectors.
    fn refresh_balloon_frames(&mut self) {
        for (handle, def) in self.components.balloons.iter() {
            let Some(id) = self.names.id_of(handle).map(EntityId::as_str) else {
                continue;
            };
            if let Some(frame) = self
                .entity_frames
                .iter_mut()
                .find(|frame| frame.instance_id == id)
            {
                frame.balloon = Some(balloon::sample(def, self.sim_time));
            }
        }
    }

    /// The fade opacity and attached-light cue of one placed instance, at the
    /// current simulation time.
    fn frame_visuals(&self, instance_id: &str) -> (f32, Option<GlowCue>) {
        self.handle_of(instance_id).map_or((1.0, None), |handle| {
            (self.frame_opacity(handle), self.frame_glow(handle))
        })
    }

    /// True when any entity's fade is enabled: the frame list then has to be
    /// rebuilt every tick, even when the level authors no routes.
    fn any_enabled_fade(&self) -> bool {
        self.components.fades.iter().any(|(_, fade)| fade.enabled)
    }

    /// Rebuilds the per-frame character handoff.
    ///
    /// A routed entity's frame carries its live transform and pose; an
    /// animation override that names a non-routed placed instance (an animated
    /// prop the renderer claims as a rigid character, such as a wall switch)
    /// gets a cue-only frame, exactly as the pre-runtime handoff did, so
    /// `play_animation`/`toggle_animation` still reach it. Every frame also
    /// carries the entity's fade opacity and attached glow at the current
    /// simulation time, and a placed entity that carries either component
    /// without a route/AI/override still gets a frame.
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
            let carrying_cue = self
                .handle_of(&agent.instance_id)
                .and_then(|h| self.components.carries.get(h))
                .filter(|carry| carry.item.is_some())
                .map_or_else(
                    || cue.clone(),
                    |carry| PoseCue::Clip {
                        name: if matches!(cue, PoseCue::Walk { speed_mps: _ }) {
                            carry.def.walk_clip.clone()
                        } else {
                            carry.def.idle_clip.clone()
                        },
                        once: false,
                        paused: false,
                    },
                );
            let (opacity, glow) = self.frame_visuals(&agent.instance_id);
            self.entity_frames.push(EntityFrame {
                instance_id: agent.instance_id.clone(),
                // The frame contract is radians with 0 facing world +Z, the
                // same unit as `RouteState::yaw` and `Character::set_pose`.
                // The AI stores degrees, so this handoff is the one place
                // that converts; nothing downstream may re-interpret the
                // value as degrees.
                transform: Some((agent.position, agent.yaw_degrees.to_radians())),
                cue: Some(carrying_cue),
                opacity: opacity * agent.opacity(),
                glow,
                shadow_only: self.frame_shadow_only(&agent.instance_id),
                balloon: None,
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
            let (opacity, glow) = self.frame_visuals(&route.instance_id);
            self.entity_frames.push(EntityFrame {
                instance_id: route.instance_id.clone(),
                transform: Some((state.position, state.yaw)),
                cue: Some(cue),
                opacity,
                glow,
                shadow_only: self.frame_shadow_only(&route.instance_id),
                balloon: None,
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
            let (opacity, glow) = self.frame_visuals(instance_id);
            // The renderer matches frames to the characters it claimed; a
            // frame for an instance it does not draw is ignored there.
            self.entity_frames.push(EntityFrame {
                instance_id: instance_id.clone(),
                transform: None,
                cue: Some(cue.clone()),
                opacity,
                glow,
                shadow_only: self.frame_shadow_only(instance_id),
                balloon: self
                    .handle_of(instance_id)
                    .and_then(|handle| self.frame_balloon(handle)),
            });
        }
        self.append_visual_frames();
        self.append_animated_door_frames();
    }

    fn append_animated_door_frames(&mut self) {
        for door in &self.animated_doors {
            self.entity_frames
                .retain(|frame| frame.instance_id != door.id);
            self.entity_frames.push(EntityFrame {
                instance_id: door.id.clone(),
                transform: None,
                cue: Some(door.cue()),
                opacity: 1.0,
                shadow_only: false,
                glow: None,
                balloon: None,
            });
        }
    }

    fn append_visual_frames(&mut self) {
        // A placed entity that carries a fade or a glow but no route, AI or
        // animation override still needs a frame: its opacity and its attached
        // light change every tick even though nothing moves. Without this a
        // fade-only character would never fade.
        let mut visual_handles: Vec<EntityHandle> = self
            .components
            .fades
            .iter()
            .map(|(handle, _)| handle)
            .collect();
        for (handle, _) in self.components.glows.iter() {
            if !visual_handles.contains(&handle) {
                visual_handles.push(handle);
            }
        }
        for (handle, _) in self.components.balloons.iter() {
            if !visual_handles.contains(&handle) {
                visual_handles.push(handle);
            }
        }
        for (handle, renderable) in self.components.renderables.iter() {
            if renderable.shadow_only && !visual_handles.contains(&handle) {
                visual_handles.push(handle);
            }
        }
        for handle in visual_handles {
            let Some(instance_id) = self.id_of(handle).map(str::to_string) else {
                continue;
            };
            if self
                .entity_frames
                .iter()
                .any(|frame| frame.instance_id == instance_id)
            {
                continue;
            }
            let opacity = self.frame_opacity(handle);
            let glow = self.frame_glow(handle);
            self.entity_frames.push(EntityFrame {
                shadow_only: self.frame_shadow_only(&instance_id),
                instance_id,
                transform: None,
                // Visibility alone must not seize animation ownership. Keep
                // the historical idle cue for existing visual components.
                cue: (self.components.fades.contains(handle)
                    || self.components.glows.contains(handle)
                    || self.components.balloons.contains(handle))
                .then_some(PoseCue::Idle),
                opacity,
                glow,
                balloon: self.frame_balloon(handle),
            });
        }
    }

    /// Republishes AI, spawned and sequence-controlled aim poses from transforms.
    fn sync_transformed_interactables(&mut self) {
        for index in 0..self.interactables.len() {
            let Some(item) = self.interactables.get(index) else {
                continue;
            };
            if item.door_index.is_some() {
                continue;
            }
            let Some(transform) = self
                .handle_of(&item.id)
                .and_then(|handle| self.components.transforms.get(handle))
                .copied()
            else {
                continue;
            };
            let _live_pose_changed =
                self.interactables
                    .set_live_pose(index, transform.position, transform.yaw_degrees);
        }
    }

    /// Republishes every routed interactable's live pose.
    pub fn sync_routed_interactables(&mut self) {
        for (route_index, route) in self.routes.routes().iter().enumerate() {
            let Some(state) = self.route_states.get(route_index) else {
                continue;
            };
            if let Some(handle) = self.names.get(&route.instance_id)
                && let Some(transform) = self.components.transforms.get_mut(handle)
            {
                transform.position = state.position;
                transform.yaw_degrees = state.yaw.to_degrees();
            }
            let Some(item_index) = self.interactables.index_of(&route.instance_id) else {
                continue;
            };
            let _live_pose_changed = self.interactables.set_live_pose(
                item_index,
                state.position,
                state.yaw.to_degrees(),
            );
        }
    }

    /// Every navigation agent, for the navigation bake.
    #[must_use]
    pub fn nav_agents(&self) -> Vec<(EntityHandle, NavAgent)> {
        self.components
            .nav_agents
            .iter()
            .map(|(handle, agent)| (handle, *agent))
            .collect()
    }

    /// Every navigation obstacle, for the navigation bake.
    #[must_use]
    pub fn nav_obstacles(&self) -> Vec<(EntityHandle, NavObstacle)> {
        self.components
            .nav_obstacles
            .iter()
            .map(|(handle, obstacle)| (handle, *obstacle))
            .collect()
    }

    /// The navigation shape of every door leaf, for the navigation bake.
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
            crate::engine::collision::PLAYER_RADIUS,
        )
}

/// Separating-axis overlap of a live rectangular leaf and a physics item bound.
/// The four footprint axes retain leaf thickness, including small item contacts.
pub(crate) fn door_pose_hits_box(leaf: &DoorCollider, solid: &WallAabb) -> bool {
    if !leaf.overlaps_body_y(solid.min_y, solid.max_y - solid.min_y) {
        return false;
    }
    let half_run = leaf.width * 0.5;
    let half_side = leaf.thickness * 0.5;
    let leaf_x = leaf.dir_x.mul_add(half_run, leaf.hinge_x);
    let leaf_z = leaf.dir_z.mul_add(half_run, leaf.hinge_z);
    let dx = f32::midpoint(solid.min_x, solid.max_x) - leaf_x;
    let dz = f32::midpoint(solid.min_z, solid.max_z) - leaf_z;
    let box_x = (solid.max_x - solid.min_x) * 0.5;
    let box_z = (solid.max_z - solid.min_z) * 0.5;
    let ux = leaf.dir_x.abs();
    let uz = leaf.dir_z.abs();
    dx.abs() < half_side.mul_add(uz, half_run.mul_add(ux, box_x))
        && dz.abs() < half_side.mul_add(ux, half_run.mul_add(uz, box_z))
        && dx.mul_add(leaf.dir_x, dz * leaf.dir_z).abs()
            < box_z.mul_add(uz, box_x.mul_add(ux, half_run))
        && dx.mul_add(-leaf.dir_z, dz * leaf.dir_x).abs()
            < box_z.mul_add(ux, box_x.mul_add(uz, half_side))
}

/// True when a candidate door pose enters static collision.
///
/// The leaf is sampled on its **centre plane** along the run (five points at
/// three heights). The leaf's two faces are deliberately not sampled: the
/// documented authoring fixes the hinge on the opening's edge, so as the leaf
/// turns, its own thickness near the hinge sweeps through the frame's rebate
/// and, at the end of the swing, lies against the jamb wall the hinge is
/// fixed to. Face samples report that as a collision at every real swing
/// angle, which froze the shipped demo doors; a blocker thick enough to stop
/// a leaf (walls, props and jambs here are all several centimetres thick)
/// still crosses the centre plane, and the worst case is a leaf whose face
/// grazes a parallel wall by half its own thickness — the same place a real
/// door comes to rest.
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
    for t in [0.125_f32, 0.25, 0.5, 0.75, 1.0] {
        let (px, pz) = candidate.point_at(t, 0.0);
        for y in [low, middle, high] {
            let mut blocked = false;
            index.for_each_point(px, pz, walls, |wall| {
                if blocked {
                    return;
                }
                if y > wall.min_y + crate::engine::collision::STEP_EPS
                    && y < wall.max_y - crate::engine::collision::STEP_EPS
                    && px > wall.min_x + crate::engine::collision::STEP_EPS
                    && px < wall.max_x - crate::engine::collision::STEP_EPS
                    && pz > wall.min_z + crate::engine::collision::STEP_EPS
                    && pz < wall.max_z - crate::engine::collision::STEP_EPS
                {
                    blocked = true;
                }
            });
            if blocked {
                return true;
            }
        }
    }
    false
}

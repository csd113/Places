//! The shared AI framework: typed behavior definitions, perception, a state
//! machine, and navigation-assisted locomotion.
//!
//! One authored `ai` component turns any entity with a `nav_agent` into an
//! agent. The framework never branches on a model, a map or an instance id:
//! behavior comes from the authored definition, threat/target relationships
//! from role tags matched against `reacts_to`, and movement from the baked
//! navigation mesh plus the same collision rules the player uses.
//!
//! ```text
//! authored AiDef ──► AiWorld (one AiAgent per entity)
//!                       │ perception: sight (range/FOV/LOS) + hearing (stimuli)
//!                       │ states: Idle Wander Follow Flee Investigate Pursue Catch
//!                       │ queries: NavMesh path/nearest, doors through Doors
//!                       ▼
//!              AiOutcome: transform updates, typed events, stimuli, door requests
//! ```
//!
//! The world owns [`AiWorld`] and applies its outcome; the AI itself never
//! touches component tables or the renderer, which keeps a decision, a move
//! and an event reproducible from the same inputs.

// Preserve exact sentinel comparisons, floating-point operation order and
// cohesive geometry/query stages. Numeric conversions and integer arithmetic
// are audited at their local expressions instead of exempting the module.
#![allow(
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "Preserve exact sentinel comparisons and established floating-point operation order; named geometry stages and cohesive query parameters keep these numeric kernels readable. Numeric conversions and integer arithmetic exceptions are documented locally."
)]

pub mod movement;
pub mod perception;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::engine::collision::WallAabb;
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::entities::events::EventKind;
use crate::engine::entities::id::EntityHandle;
use crate::engine::entity::PoseCue;
use crate::engine::level::{WalkableCeiling, WalkableFloor};
use crate::engine::nav::{
    NavAgentProfile, NavBounds, NavDoorState, NavMesh, NavScratch, Path, PathQuery,
};

use movement::{AgentMove, MoveStep};
use perception::Stimulus;

pub use perception::{Perceived, Stimulus as AiStimulus, sight_clear};

/// What an agent does when it is not reacting to anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiBehavior {
    /// Stands and looks around; reacts to perception.
    #[default]
    Idler,
    /// Picks navigable destinations around its post and walks to them.
    Wanderer,
    /// Wanders, retreats to an authored shelter, then returns after a hold.
    Skittish,
    /// Retreats from any agent tagged in `reacts_to`.
    Prey,
    /// Pursues and catches any agent tagged in `reacts_to`.
    Predator,
    /// Follows an agent tagged in `reacts_to` at a distance.
    Follower,
}

impl AiBehavior {
    /// The stable serialized name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idler => "idler",
            Self::Wanderer => "wanderer",
            Self::Skittish => "skittish",
            Self::Prey => "prey",
            Self::Predator => "predator",
            Self::Follower => "follower",
        }
    }
}

/// Authored world-space area containing an agent's entire roaming body.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AiHomeBounds {
    /// Minimum X.
    pub x: f32,
    /// Minimum Z.
    pub z: f32,
    /// X extent.
    pub width: f32,
    /// Z extent.
    pub depth: f32,
    /// Lowest permitted feet surface.
    pub min_y: f32,
    /// Highest permitted feet surface.
    pub max_y: f32,
}

impl AiHomeBounds {
    /// True when the rectangle and vertical band are finite and nonempty.
    #[must_use]
    pub fn is_valid(self) -> bool {
        [
            self.x, self.z, self.width, self.depth, self.min_y, self.max_y,
        ]
        .iter()
        .all(|value| value.is_finite())
            && self.width > 0.0
            && self.depth > 0.0
            && self.min_y <= self.max_y
            && (self.x + self.width).is_finite()
            && (self.z + self.depth).is_finite()
    }

    /// Navigation volume inset so the whole body remains in the rectangle.
    #[must_use]
    pub fn nav_bounds(self, radius: f32) -> NavBounds {
        NavBounds {
            min: Vec3::new(self.x + radius, self.min_y, self.z + radius),
            max: Vec3::new(
                self.x + self.width - radius,
                self.max_y,
                self.z + self.depth - radius,
            ),
        }
    }
}

/// The authored AI definition of one entity.
///
/// Every field has a default, so `{ "component": "ai" }` is a valid idler and
/// a map only authors what it means to change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiDef {
    /// The baseline behavior.
    #[serde(default)]
    pub behavior: AiBehavior,
    /// Free-form tag this agent advertises to others.
    #[serde(default)]
    pub role: Option<String>,
    /// Role tags this agent reacts to (threat for prey, target for
    /// predator/follower).
    #[serde(default)]
    pub reacts_to: Vec<String>,
    /// Ordinary walking speed, in m/s.
    #[serde(default = "default_walk_speed")]
    pub walk_speed: f32,
    /// Flee/pursue speed, in m/s.
    #[serde(default = "default_run_speed")]
    pub run_speed: f32,
    /// Sight range in metres; `0` disables sight.
    #[serde(default = "default_sight_range")]
    pub sight_range: f32,
    /// Full horizontal field of view, in degrees.
    #[serde(default = "default_sight_fov")]
    pub sight_fov_degrees: f32,
    /// Hearing range in metres; `0` disables hearing.
    #[serde(default = "default_hearing_range")]
    pub hearing_range: f32,
    /// Preferred flee separation from a threat, in metres.
    #[serde(default = "default_flee_distance")]
    pub flee_distance: f32,
    /// Pursue only while the target is within this range, in metres.
    #[serde(default = "default_pursue_distance")]
    pub pursue_distance: f32,
    /// Reach radius for a catch, in metres.
    #[serde(default = "default_catch_radius")]
    pub catch_radius: f32,
    /// Largest vertical separation a catch accepts, in metres.
    #[serde(default = "default_catch_height")]
    pub catch_height: f32,
    /// Wander destinations are sampled within this radius, in metres.
    #[serde(default = "default_wander_radius")]
    pub wander_radius: f32,
    /// Seconds an idler or a wanderer holds still between destinations.
    #[serde(default = "default_idle_seconds")]
    pub idle_seconds: f32,
    /// True when the agent may open an unlocked door on its route.
    #[serde(default)]
    pub can_open_doors: bool,
    /// Optional world area and feet-height band applied to every route.
    #[serde(default)]
    pub home_bounds: Option<AiHomeBounds>,
    /// Reachable world-space shelters for skittish agents, at most sixteen.
    #[serde(default)]
    pub hide_anchors: Vec<[f32; 3]>,
    /// Hidden hold, in seconds, bounded to `0.1..=60`.
    #[serde(default = "default_hide_seconds")]
    pub hide_seconds: f32,
    /// Optional named looping clip while physically stopped.
    #[serde(default)]
    pub idle_clip: Option<String>,
    /// Optional named looping clip while physically moving.
    #[serde(default)]
    pub move_clip: Option<String>,
}

impl Default for AiDef {
    fn default() -> Self {
        Self {
            behavior: AiBehavior::Idler,
            role: None,
            reacts_to: Vec::new(),
            walk_speed: default_walk_speed(),
            run_speed: default_run_speed(),
            sight_range: default_sight_range(),
            sight_fov_degrees: default_sight_fov(),
            hearing_range: default_hearing_range(),
            flee_distance: default_flee_distance(),
            pursue_distance: default_pursue_distance(),
            catch_radius: default_catch_radius(),
            catch_height: default_catch_height(),
            wander_radius: default_wander_radius(),
            idle_seconds: default_idle_seconds(),
            can_open_doors: false,
            home_bounds: None,
            hide_anchors: Vec::new(),
            hide_seconds: default_hide_seconds(),
            idle_clip: None,
            move_clip: None,
        }
    }
}

impl AiDef {
    /// True when every numeric field is finite and inside its usable range.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let speeds = self.walk_speed.is_finite()
            && self.walk_speed > 0.0
            && self.run_speed.is_finite()
            && self.run_speed > 0.0
            && self.sight_range.is_finite()
            && self.sight_range >= 0.0
            && self.sight_fov_degrees.is_finite()
            && self.sight_fov_degrees >= 0.0
            && self.sight_fov_degrees <= 360.0
            && self.hearing_range.is_finite()
            && self.hearing_range >= 0.0;
        let reactions = self.flee_distance.is_finite()
            && self.flee_distance >= 0.0
            && self.pursue_distance.is_finite()
            && self.pursue_distance >= 0.0
            && self.catch_radius.is_finite()
            && self.catch_radius > 0.0
            && self.catch_height.is_finite()
            && self.catch_height > 0.0
            && self.wander_radius.is_finite()
            && self.wander_radius >= 0.0
            && self.idle_seconds.is_finite()
            && self.idle_seconds >= 0.0;
        let tags = self
            .reacts_to
            .iter()
            .all(|tag| !tag.trim().is_empty() && tag.len() <= 64)
            && self
                .role
                .as_deref()
                .is_none_or(|role| !role.trim().is_empty() && role.len() <= 64);
        let shelter = self.home_bounds.is_none_or(AiHomeBounds::is_valid)
            && self.hide_anchors.len() <= 16
            && self
                .hide_anchors
                .iter()
                .flatten()
                .all(|value| value.is_finite())
            && self.home_bounds.is_none_or(|area| {
                self.hide_anchors
                    .iter()
                    .all(|anchor| area.nav_bounds(0.0).contains(Vec3::from_array(*anchor)))
            })
            && (self.behavior != AiBehavior::Skittish || !self.hide_anchors.is_empty())
            && self.hide_seconds.is_finite()
            && (0.1..=60.0).contains(&self.hide_seconds);
        let clips = [&self.idle_clip, &self.move_clip].iter().all(|clip| {
            clip.as_deref()
                .is_none_or(|name| !name.trim().is_empty() && name.len() <= 128)
        });
        speeds && reactions && tags && shelter && clips
    }

    /// True when `role` is one this agent reacts to.
    #[must_use]
    pub fn reacts_to_role(&self, role: Option<&str>) -> bool {
        let Some(authored_role) = role else {
            return false;
        };
        self.reacts_to.iter().any(|tag| tag == authored_role)
    }
}

fn default_walk_speed() -> f32 {
    1.0
}
fn default_run_speed() -> f32 {
    2.0
}
fn default_sight_range() -> f32 {
    8.0
}
fn default_sight_fov() -> f32 {
    200.0
}
fn default_hearing_range() -> f32 {
    6.0
}
fn default_flee_distance() -> f32 {
    5.0
}
fn default_pursue_distance() -> f32 {
    12.0
}
fn default_catch_radius() -> f32 {
    0.45
}
fn default_catch_height() -> f32 {
    0.8
}
fn default_wander_radius() -> f32 {
    4.0
}
fn default_idle_seconds() -> f32 {
    2.5
}
fn default_hide_seconds() -> f32 {
    8.0
}

/// One agent's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiState {
    /// Nothing to do.
    Idle,
    /// Walking to a chosen destination, then idling.
    Wander {
        /// Current destination.
        destination: Vec3,
    },
    /// Following a target at a distance.
    Follow {
        /// The followed entity.
        target: EntityHandle,
    },
    /// Running from a threat.
    Flee {
        /// The threat, when one is perceived.
        threat: Option<EntityHandle>,
        /// Current escape destination.
        destination: Vec3,
    },
    /// Running along a verified route to an authored shelter.
    Hide {
        /// Reachable feet position at the shelter.
        destination: Vec3,
    },
    /// At the shelter, invisible for a bounded hold.
    Hidden,
    /// Walking back to the point where the shelter retreat began.
    Return {
        /// Reachable pre-retreat feet position.
        destination: Vec3,
    },
    /// Walking to the last heard or seen point.
    Investigate {
        /// Where to look.
        point: Vec3,
    },
    /// Closing on a target.
    Pursue {
        /// The pursued entity.
        target: EntityHandle,
    },
    /// The catch has fired; locomotion is frozen for the presentation.
    Catch {
        /// The caught entity.
        target: EntityHandle,
    },
    /// A running sequence owns this entity's motion.
    Scripted,
    /// Caught by a predator; immobilized.
    Caught,
}

impl AiState {
    /// The stable name, used by `ai_state` events and diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Wander { destination: _ } => "wander",
            Self::Follow { target: _ } => "follow",
            Self::Flee {
                threat: _,
                destination: _,
            } => "flee",
            Self::Hide { destination: _ } => "hide",
            Self::Hidden => "hidden",
            Self::Return { destination: _ } => "return",
            Self::Investigate { point: _ } => "investigate",
            Self::Pursue { target: _ } => "pursue",
            Self::Catch { target: _ } => "catch",
            Self::Scripted => "scripted",
            Self::Caught => "caught",
        }
    }

    /// True when the state moves the agent.
    #[must_use]
    pub const fn is_moving(self) -> bool {
        matches!(
            self,
            Self::Wander { destination: _ }
                | Self::Follow { target: _ }
                | Self::Flee {
                    threat: _,
                    destination: _
                }
                | Self::Investigate { point: _ }
                | Self::Pursue { target: _ }
                | Self::Hide { destination: _ }
                | Self::Return { destination: _ }
        )
    }
}

/// One agent's runtime state.
#[derive(Debug)]
pub struct AiAgent {
    /// The entity this agent drives.
    pub handle: EntityHandle,
    /// The authored id, for events and the renderer handoff.
    pub instance_id: String,
    /// Authored definition.
    pub def: AiDef,
    /// Physical navigation profile.
    pub profile: NavAgentProfile,
    /// Baked class index once a mesh is available.
    pub class: Option<usize>,
    /// Current state.
    pub state: AiState,
    /// Live position (feet).
    pub position: Vec3,
    /// Live yaw in degrees.
    pub yaw_degrees: f32,
    /// Authored position, restored by a reset.
    pub home: Vec3,
    /// Authored yaw, restored by a reset.
    pub home_yaw: f32,
    /// The current route, reused across frames.
    pub path: Path,
    /// The goal the current path was built for.
    pub path_goal: Option<Vec3>,
    /// Seconds since the path was built or the last attempt started.
    pub path_age: f32,
    /// When the last path attempt failed, on the agent clock.
    pub path_failed_at: Option<f32>,
    /// Agent clock of the last actual search, independent of route invalidation.
    pub last_path_attempt_at: f32,
    /// Number of bounded searches performed for diagnostics.
    pub path_queries: u64,
    /// Explicit developer inspection destination, using the ordinary planner.
    pub navigation_goal: Option<Vec3>,
    /// The goal whose route was already walked to its end. An empty path for
    /// this goal is arrival, not a reason to plan again.
    pub path_done_goal: Option<Vec3>,
    /// The waypoint index the follower is walking toward.
    pub current_waypoint: usize,
    /// True when the entity was spawned at runtime and moves through the
    /// dynamic render key rather than the baked prop transform.
    pub spawned: bool,
    /// The role tag this agent advertises (owned copy of `def.role`).
    pub role: Option<String>,
    /// A one-time report flag for an agent that cannot navigate.
    pub reported_unplaced: bool,
    /// Current locomotion speed, for the animation bridge.
    pub speed_mps: f32,
    /// Seconds stuck against collision while trying to move.
    pub stuck_seconds: f32,
    /// Seconds until a wander/idle agent picks another destination.
    pub wait_remaining: f32,
    /// Seconds until this agent may catch again.
    pub catch_cooldown: f32,
    /// True once this agent has been caught.
    pub caught: bool,
    /// Agent-local clock, advanced by the simulation delta.
    pub clock: f32,
    /// When perception last ran, on the agent clock.
    pub last_perception_at: f32,
    /// When movement noise was last released, on the agent clock.
    pub last_noise_at: f32,
    /// Last perceived contact, refreshed by staggered checks.
    pub contact: Option<Perceived>,
    /// A remembered point of interest (a heard stimulus).
    pub interest: Option<Vec3>,
    /// The door version the current path was planned against.
    pub path_door_version: u64,
    /// Agent-local pathfinding scratch, reused across queries.
    pub scratch: NavScratch,
    /// Deterministic phase for staggered perception.
    pub perception_phase: u32,
    /// Position the shelter retreat returns to without teleporting.
    pub shelter_return: Option<Vec3>,
    /// Seconds left in a bounded hidden hold.
    pub hidden_remaining: f32,
    /// Prevents immediate re-entry while emerging from a shelter.
    pub shelter_cooldown: f32,
    /// Shared visible-light response temporarily holds locomotion and faces this point.
    /// The original behavior state, path and timers remain intact.
    pub light_look_target: Option<Vec3>,
}

impl AiAgent {
    /// The cue this agent's locomotion state maps to.
    #[must_use]
    pub fn cue(&self) -> PoseCue {
        let clip = if self.speed_mps > 0.01 {
            self.def.move_clip.as_ref()
        } else {
            self.def.idle_clip.as_ref()
        };
        clip.map_or_else(
            || movement::cue_for(self.state, self.speed_mps),
            |name| PoseCue::Clip {
                name: name.clone(),
                once: false,
                paused: false,
            },
        )
    }

    /// Opacity contributed by a completed shelter retreat.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        if matches!(self.state, AiState::Hidden) {
            0.0
        } else {
            1.0
        }
    }

    fn bounds(&self) -> Option<NavBounds> {
        self.def
            .home_bounds
            .map(|area| area.nav_bounds(self.profile.radius))
    }

    /// True when this agent should be immobilized.
    #[must_use]
    pub const fn frozen(&self) -> bool {
        matches!(
            self.state,
            AiState::Catch { target: _ } | AiState::Caught | AiState::Scripted
        )
    }
}

/// One other agent, as the AI needs to see it.
#[derive(Debug, Clone)]
pub struct AiTarget {
    /// Entity handle.
    pub handle: EntityHandle,
    /// Authored/runtime id.
    pub instance_id: String,
    /// Advertised role tag.
    pub role: Option<String>,
    /// The target's behavior.
    pub behavior: AiBehavior,
    /// Live feet position.
    pub position: Vec3,
    /// Body radius.
    pub radius: f32,
    /// Body height.
    pub height: f32,
    /// True when the target has already been caught.
    pub caught: bool,
}

/// Everything one AI tick reads.
pub struct AiTickContext<'a> {
    /// Simulation delta in seconds.
    pub delta: f32,
    /// Simulation time in seconds, for stimulus ages.
    pub sim_time: f32,
    /// The installed navigation mesh, when the package has one.
    pub nav: Option<&'a NavMesh>,
    /// Live door state.
    pub doors: &'a Doors,
    /// Bumped whenever a door phase or lock changes, for cheap invalidation.
    pub door_version: u64,
    /// Static collision boxes.
    pub walls: &'a [WallAabb],
    /// Live door leaf colliders, already rebuilt for this tick.
    pub leaves: &'a [crate::engine::collision::DoorCollider],
    /// Spatial index over `walls`.
    pub index: &'a CollisionIndex,
    /// The walkable floor model the mover follows.
    pub floor: &'a WalkableFloor,
    /// Cached authored ceilings for exact off-centre body clearance.
    pub ceiling: &'a WalkableCeiling,
    /// Every live agent, including this world's, as targets.
    pub targets: &'a [AiTarget],
    /// Live stimuli.
    pub stimuli: &'a [Stimulus],
    /// Entities whose motion a running sequence owns this tick.
    pub scripted: &'a [EntityHandle],
    /// Current bounded physics item boxes, collected once before this tick.
    pub obstacles: &'a [WallAabb],
}

impl AiTickContext<'_> {
    /// The target with `handle`, if any.
    #[must_use]
    pub fn target(&self, handle: EntityHandle) -> Option<&AiTarget> {
        self.targets.iter().find(|target| target.handle == handle)
    }
}

impl NavDoorState for AiTickContext<'_> {
    fn obstacle_segment_blocked(&self, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
        crate::engine::collision::disc_segment_blocked(
            glam::Vec2::new(from.x, from.z),
            glam::Vec2::new(to.x, to.z),
            radius,
            from.y.min(to.y),
            height + (to.y - from.y).abs(),
            0.0,
            &CollisionIndex::empty(),
            self.obstacles,
        )
    }

    fn door_obstructed(&self, id: &str) -> bool {
        self.doors.door_obstructed(id)
    }

    fn door_open(&self, id: &str) -> bool {
        self.doors.door_open(id)
    }

    fn door_locked(&self, id: &str) -> Option<bool> {
        self.doors.door_locked(id)
    }

    fn door_leaf(&self, id: &str) -> Option<crate::engine::collision::DoorCollider> {
        self.doors.door_leaf(id)
    }

    fn obstacle_blocked(&self, position: Vec3, radius: f32, height: f32) -> bool {
        self.obstacles.iter().any(|solid| {
            solid.blocks_body(position.y, height)
                && solid.overlaps_disc(position.x, position.z, radius)
        })
    }
}

/// What one AI tick produced; the world applies it.
#[derive(Debug, Default)]
pub struct AiOutcome {
    /// A locomotion speed changed and the renderer must refresh its pose cue.
    pub cues_changed: bool,
    /// Transform updates: `(handle, position, yaw degrees, spawned)`.
    pub moved: Vec<(EntityHandle, Vec3, f32, bool)>,
    /// Typed events: `(kind, subject, key, actor)`.
    pub events: Vec<(EventKind, EntityHandle, String, Option<EntityHandle>)>,
    /// Movement noise released this tick.
    pub stimuli: Vec<Stimulus>,
    /// Doors the AI asked to open: `(door id, agent handle)`.
    pub door_requests: Vec<(String, EntityHandle)>,
    /// `(predator, prey)` pairs that caught this tick.
    pub catches: Vec<(EntityHandle, EntityHandle)>,
    /// `ai_state` transitions this tick.
    pub transitions: usize,
    /// Path queries performed this tick.
    pub path_queries: usize,
    /// Agents that could not navigate this tick.
    pub unplaced: usize,
    /// Catches fired since the world was built.
    pub catches_total: u64,
}

/// How often perception runs for one agent, in seconds.
pub const PERCEPTION_INTERVAL_S: f32 = 0.20;

/// How far a target may move before pursuit repaths, in metres.
pub const REPLAN_TARGET_M: f32 = 0.60;

/// Hard bound on how often a moving agent repaths, in seconds.
pub const REPLAN_INTERVAL_S: f32 = 0.50;

/// Whole-world search budget per tick; agent processing rotates for fairness.
pub const MAX_PATH_QUERIES_PER_TICK: usize = 8;

/// Expansion budget of one runtime search; partial routes retain bounded retry.
pub const RUNTIME_PATH_EXPANSIONS: usize = 8192;

/// How long an agent waits before retrying a goal whose route was
/// unreachable, in seconds. It bounds retries without letting a state's
/// completion timeout stall.
pub const PATH_RETRY_S: f32 = 0.75;

/// Seconds without progress before a mover is considered stuck.
pub const STUCK_SECONDS: f32 = 0.55;

/// How often a moving agent releases a noise stimulus, in seconds.
pub const NOISE_INTERVAL_S: f32 = 0.60;

/// Speed above which movement is noisy, in m/s.
pub const NOISE_SPEED_MPS: f32 = 0.55;

/// Distance from its authored post at which an idle agent walks back, in
/// metres.
pub const POST_RETURN_M: f32 = 1.5;

/// Largest number of candidate directions considered when fleeing.
pub const FLEE_CANDIDATES: usize = 16;

/// Largest number of directions sampled when picking a wander destination.
pub const WANDER_CANDIDATES: usize = 8;

/// Reserved contact identity for the human player; never an entity-store slot.
pub(crate) const PLAYER_TARGET_HANDLE: EntityHandle = EntityHandle::from_parts(u32::MAX, u32::MAX);

/// Largest number of real path queries one flee replan spends, trying the
/// best-scoring destinations in order.
pub const FLEE_PATH_ATTEMPTS: usize = 3;

/// The AI runtime of one world.
#[derive(Debug, Default)]
pub struct AiWorld {
    agents: Vec<AiAgent>,
    next_phase: u32,
    stimuli_released: u64,
    catches_total: u64,
    next_tick_agent: usize,
}

impl AiWorld {
    /// An empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Registered agent count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Every agent.
    #[must_use]
    pub fn agents(&self) -> &[AiAgent] {
        &self.agents
    }

    /// One agent's runtime state by instance id.
    #[must_use]
    pub fn agent(&self, instance_id: &str) -> Option<&AiAgent> {
        self.agents
            .iter()
            .find(|agent| agent.instance_id == instance_id)
    }

    /// Removes every agent.
    pub fn clear(&mut self) {
        self.agents.clear();
        self.next_phase = 0;
        self.stimuli_released = 0;
        self.catches_total = 0;
        self.next_tick_agent = 0;
    }

    /// Registers one agent, replacing any previous registration of `handle`.
    pub fn register(
        &mut self,
        handle: EntityHandle,
        instance_id: &str,
        def: AiDef,
        profile: NavAgentProfile,
        position: Vec3,
        yaw_degrees: f32,
        spawned: bool,
    ) {
        let phase = self.next_phase;
        self.next_phase = self.next_phase.wrapping_add(1);
        let role = def
            .role
            .as_deref()
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .map(str::to_string);
        self.agents.retain(|agent| agent.handle != handle);
        self.agents.push(AiAgent {
            handle,
            instance_id: instance_id.to_string(),
            def,
            profile,
            class: None,
            state: AiState::Idle,
            position,
            yaw_degrees,
            home: position,
            home_yaw: yaw_degrees,
            path: Path::new(),
            path_goal: None,
            path_age: 0.0,
            path_failed_at: None,
            last_path_attempt_at: -PATH_RETRY_S,
            path_queries: 0,
            navigation_goal: None,
            path_done_goal: None,
            current_waypoint: 0,
            spawned,
            role,
            reported_unplaced: false,
            speed_mps: 0.0,
            stuck_seconds: 0.0,
            wait_remaining: 0.0,
            catch_cooldown: 0.0,
            caught: false,
            clock: 0.0,
            last_perception_at: perception_baseline(phase),
            last_noise_at: f32::NEG_INFINITY,
            contact: None,
            interest: None,
            path_door_version: 0,
            scratch: NavScratch::new(),
            perception_phase: phase,
            shelter_return: None,
            hidden_remaining: 0.0,
            shelter_cooldown: 0.0,
            light_look_target: None,
        });
    }

    /// Removes one agent; true when it existed.
    pub fn remove(&mut self, handle: EntityHandle) -> bool {
        let before = self.agents.len();
        self.agents.retain(|agent| agent.handle != handle);
        self.agents.len() != before
    }

    /// Marks an agent as caught and frozen.
    pub fn mark_caught(&mut self, handle: EntityHandle) -> bool {
        let Some(agent) = self.agents.iter_mut().find(|agent| agent.handle == handle) else {
            return false;
        };
        agent.caught = true;
        agent.state = AiState::Caught;
        agent.speed_mps = 0.0;
        agent.path.clear();
        agent.path_goal = None;
        true
    }

    /// Restores every placed agent to its authored pose and clears runtime
    /// state; spawned agents keep their position (their entity is gone).
    pub fn reset(&mut self) {
        for agent in &mut self.agents {
            if !agent.spawned {
                agent.position = agent.home;
                agent.yaw_degrees = agent.home_yaw;
            }
            agent.state = AiState::Idle;
            agent.path.clear();
            agent.path_goal = None;
            agent.path_age = 0.0;
            agent.path_failed_at = None;
            agent.last_path_attempt_at = -PATH_RETRY_S;
            agent.path_queries = 0;
            agent.navigation_goal = None;
            agent.path_done_goal = None;
            agent.current_waypoint = 0;
            agent.speed_mps = 0.0;
            agent.stuck_seconds = 0.0;
            agent.wait_remaining = 0.0;
            agent.catch_cooldown = 0.0;
            agent.caught = false;
            agent.contact = None;
            agent.interest = None;
            agent.clock = 0.0;
            agent.last_perception_at = perception_baseline(agent.perception_phase);
            agent.last_noise_at = f32::NEG_INFINITY;
            agent.path_door_version = 0;
            agent.shelter_return = None;
            agent.hidden_remaining = 0.0;
            agent.shelter_cooldown = 0.0;
            agent.light_look_target = None;
        }
    }

    /// Shares a successful sequence pose with AI perception and renderer frames.
    /// Moving the feet invalidates a route from the old position; a pure turn
    /// preserves it. Authored home, behavior and temporary light holds survive.
    pub(crate) fn apply_sequence_transform(
        &mut self,
        handle: EntityHandle,
        position: Vec3,
        yaw_degrees: f32,
    ) -> bool {
        if !position.is_finite() || !yaw_degrees.is_finite() {
            return false;
        }
        let Some(agent) = self.agents.iter_mut().find(|agent| agent.handle == handle) else {
            return false;
        };
        let moved = agent.position != position;
        let changed = moved || agent.yaw_degrees != yaw_degrees;
        agent.position = position;
        agent.yaw_degrees = yaw_degrees;
        if moved {
            agent.path.clear();
            agent.path_goal = None;
            agent.path_age = 0.0;
            agent.path_failed_at = None;
            agent.path_done_goal = None;
            agent.current_waypoint = 0;
            agent.stuck_seconds = 0.0;
            agent.speed_mps = 0.0;
        }
        changed
    }

    /// Applies an opt-in renderer-backed light hold without replacing AI state.
    pub fn set_light_response(&mut self, handle: EntityHandle, target: Option<Vec3>) -> bool {
        let Some(agent) = self.agents.iter_mut().find(|agent| agent.handle == handle) else {
            return false;
        };
        let finite_target = target.filter(|position| position.is_finite());
        let changed = agent.light_look_target != finite_target;
        agent.light_look_target = finite_target;
        changed
    }

    /// Releases every caught mark (a world reset or encounter restart).
    pub fn release_caught(&mut self) {
        for agent in &mut self.agents {
            if agent.caught {
                agent.caught = false;
                agent.state = AiState::Idle;
                agent.path.clear();
                agent.path_goal = None;
            }
        }
    }

    /// Forces an agent into a state, for tests and scripted encounters.
    pub fn force_state(&mut self, instance_id: &str, state: AiState) -> bool {
        let Some(agent) = self
            .agents
            .iter_mut()
            .find(|agent| agent.instance_id == instance_id)
        else {
            return false;
        };
        agent.state = state;
        agent.navigation_goal = None;
        agent.path.clear();
        agent.path_goal = None;
        true
    }

    /// Installs a finite inspection goal without bypassing collision or routing.
    /// Reset, scripted ownership and caught state continue to take precedence.
    pub fn set_navigation_goal(&mut self, instance_id: &str, goal: Vec3) -> bool {
        if !goal.is_finite() {
            return false;
        }
        let Some(agent) = self
            .agents
            .iter_mut()
            .find(|agent| agent.instance_id == instance_id)
        else {
            return false;
        };
        if agent.bounds().is_some_and(|bounds| !bounds.contains(goal)) {
            return false;
        }
        agent.navigation_goal = Some(goal);
        agent.state = AiState::Investigate { point: goal };
        agent.path.clear();
        agent.path_goal = None;
        agent.path_done_goal = None;
        agent.path_failed_at = None;
        agent.stuck_seconds = 0.0;
        true
    }

    /// The total stimuli released by movement since the world was built.
    #[must_use]
    pub const fn stimuli_released(&self) -> u64 {
        self.stimuli_released
    }

    /// Total catches since the world was built.
    #[must_use]
    pub const fn catches_total(&self) -> u64 {
        self.catches_total
    }

    /// One line per live agent: the same state, path, goals and contacts the
    /// runtime acts on, for the developer inspection path.
    #[must_use]
    pub fn debug_report(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for agent in &self.agents {
            let contact = agent.contact.as_ref().map_or_else(
                || "-".to_string(),
                |contact| {
                    format!(
                        "{}{}@({:.2},{:.2})",
                        if contact.heard_only { "heard:" } else { "saw:" },
                        contact.role,
                        contact.position.x,
                        contact.position.z
                    )
                },
            );
            let _formatted_text = writeln!(
                out,
                "{} state={} pos=({:.2},{:.2},{:.2}) speed={:.2} goal={:?} waypoints={} \
                 doors={:?} contact={} catch_radius={:.2} caught={} stuck={:.1}",
                agent.instance_id,
                agent.state.name(),
                agent.position.x,
                agent.position.y,
                agent.position.z,
                agent.speed_mps,
                agent.path_goal,
                agent.path.waypoints.len(),
                agent.path.door_requests,
                contact,
                agent.def.catch_radius,
                agent.caught,
                agent.stuck_seconds,
            );
        }
        out
    }

    /// Advances every agent once.
    pub fn tick(&mut self, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
        if !ctx.delta.is_finite() || ctx.delta <= 0.0 {
            out.catches_total = self.catches_total;
            return;
        }
        let mut noise = 0u64;
        let count = self.agents.len();
        let first = self.next_tick_agent.checked_rem(count).unwrap_or(0);
        self.next_tick_agent = first.saturating_add(1).checked_rem(count).unwrap_or(0);
        let (before, after) = self.agents.split_at_mut(first);
        for agent in after.iter_mut().chain(before.iter_mut()) {
            let previous_speed = agent.speed_mps;
            if let Some(target) = agent.light_look_target
                && !agent.caught
                && !ctx.scripted.contains(&agent.handle)
                && !matches!(agent.state, AiState::Catch { target: _ })
            {
                let previous_yaw = agent.yaw_degrees;
                let direction = Vec3::new(
                    target.x - agent.position.x,
                    target.y - agent.position.y,
                    target.z - agent.position.z,
                );
                if direction.x * direction.x + direction.z * direction.z > 0.0001 {
                    agent.yaw_degrees = direction.x.atan2(direction.z).to_degrees();
                }
                agent.speed_mps = 0.0;
                out.cues_changed |= agent.speed_mps != previous_speed;
                if agent.yaw_degrees != previous_yaw {
                    out.moved.push((
                        agent.handle,
                        agent.position,
                        agent.yaw_degrees,
                        agent.spawned,
                    ));
                }
                continue;
            }
            agent.clock += ctx.delta;
            agent.path_age += ctx.delta;
            agent.catch_cooldown = (agent.catch_cooldown - ctx.delta).max(0.0);
            agent.shelter_cooldown = (agent.shelter_cooldown - ctx.delta).max(0.0);
            if agent.class.is_none() {
                agent.class = ctx
                    .nav
                    .and_then(|nav| nav.class_index(&agent.profile.class()));
            }
            if agent.caught {
                agent.speed_mps = 0.0;
                out.cues_changed |= agent.speed_mps != previous_speed;
                continue;
            }
            let scripted = ctx.scripted.contains(&agent.handle);
            if scripted && !matches!(agent.state, AiState::Catch { target: _ }) {
                if !matches!(agent.state, AiState::Scripted) {
                    set_state(agent, AiState::Scripted, out);
                }
                agent.speed_mps = 0.0;
                out.cues_changed |= agent.speed_mps != previous_speed;
                agent.path.clear();
                agent.path_goal = None;
                continue;
            }
            if matches!(agent.state, AiState::Scripted) {
                set_state(agent, AiState::Idle, out);
            }
            if !matches!(agent.state, AiState::Hidden) {
                perceive(agent, ctx);
            }
            if let Some(goal) = agent.navigation_goal {
                set_state(agent, AiState::Investigate { point: goal }, out);
            } else {
                decide(agent, ctx, out);
            }
            advance(agent, ctx, out);
            out.cues_changed |= agent.speed_mps != previous_speed;
            noise = noise.saturating_add(release_noise(agent, out));
        }
        self.stimuli_released = self.stimuli_released.saturating_add(noise);
        // Apply catches after the iteration so the caught agent is frozen
        // without a second mutable borrow of the list.
        for (predator, prey) in &out.catches {
            let _mark_caught_status = self.mark_caught(*prey);
            if let Some(agent) = self
                .agents
                .iter_mut()
                .find(|agent| agent.handle == *predator)
                && matches!(agent.state, AiState::Catch { target: _ })
            {
                agent.catch_cooldown = agent.def.idle_seconds.max(1.0);
            }
        }
        self.catches_total = self
            .catches_total
            .saturating_add(u64::try_from(out.catches.len()).unwrap_or(u64::MAX));
        out.catches_total = self.catches_total;
    }
}

fn path_attempt_due(agent: &AiAgent) -> bool {
    agent.clock - agent.last_path_attempt_at >= REPLAN_INTERVAL_S
}

fn reserve_path_query(agent: &mut AiAgent, out: &mut AiOutcome) -> bool {
    if out.path_queries >= MAX_PATH_QUERIES_PER_TICK {
        return false;
    }
    out.path_queries = out.path_queries.saturating_add(1);
    agent.last_path_attempt_at = agent.clock;
    agent.path_queries = agent.path_queries.saturating_add(1);
    true
}

/// Keeps the first perception checks spread over eight stable time slots.
fn perception_baseline(phase: u32) -> f32 {
    let slot = u8::try_from(phase % 8).unwrap_or(0);
    PERCEPTION_INTERVAL_S * (f32::from(slot) / 8.0 - 1.0)
}

/// Updates one agent's perception on its staggered interval.
fn perceive(agent: &mut AiAgent, ctx: &AiTickContext<'_>) {
    let interval = PERCEPTION_INTERVAL_S.max(0.02);
    if agent.clock - agent.last_perception_at < interval {
        if let Some(contact) = agent.contact.as_mut() {
            contact.age = (contact.age + ctx.delta).min(600.0);
        }
        return;
    }
    // Advance on the original phase rather than drifting toward frame times.
    let elapsed = agent.clock - agent.last_perception_at;
    agent.last_perception_at += (elapsed / interval).floor() * interval;
    agent.contact = perception::perceive(agent, ctx);
    if let Some(contact) = agent.contact.as_ref()
        && contact.heard_only
    {
        // A predator investigates what it hears; prey investigates only what
        // it does not recognize as a threat (it flees or freezes instead of
        // walking toward its own predator).
        let interesting = match agent.def.behavior {
            AiBehavior::Prey | AiBehavior::Skittish => {
                !agent.def.reacts_to_role(Some(&contact.role))
            }
            AiBehavior::Idler
            | AiBehavior::Wanderer
            | AiBehavior::Predator
            | AiBehavior::Follower => true,
        };
        if interesting {
            agent.interest = Some(contact.position);
        }
    }
}

/// Chooses the agent's next state from its perception and behavior.
fn decide(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if let AiState::Catch { target } = agent.state {
        let target_gone = ctx.target(target).is_none() && !ctx.scripted.contains(&agent.handle);
        if target_gone {
            let cooldown = agent.catch_cooldown.max(1.0);
            set_state(agent, AiState::Idle, out);
            agent.catch_cooldown = cooldown;
        }
        return;
    }
    match agent.def.behavior {
        AiBehavior::Predator => decide_predator(agent, ctx, out),
        AiBehavior::Prey => decide_prey(agent, ctx, out),
        AiBehavior::Follower => decide_follower(agent, ctx, out),
        AiBehavior::Wanderer => decide_wanderer(agent, ctx, out),
        AiBehavior::Skittish => decide_skittish(agent, ctx, out),
        AiBehavior::Idler => decide_idler(agent, ctx, out),
    }
}

/// Predator: sight means pursue, a heard stimulus means investigate.
fn decide_predator(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if let Some(contact) = agent.contact.clone()
        && agent.def.reacts_to_role(Some(&contact.role))
    {
        let target_ok = ctx
            .target(contact.handle)
            .is_some_and(|target| !target.caught);
        if target_ok && contact.position.distance(agent.position) <= agent.def.pursue_distance {
            if !matches!(agent.state, AiState::Pursue { target } if target == contact.handle) {
                set_state(
                    agent,
                    AiState::Pursue {
                        target: contact.handle,
                    },
                    out,
                );
                agent.path.clear();
                agent.path_goal = None;
            }
            return;
        }
    }
    if let AiState::Pursue { target } = agent.state {
        if ctx.target(target).is_none() {
            set_state(agent, AiState::Idle, out);
        }
        return;
    }
    if matches!(agent.state, AiState::Investigate { point: _ }) {
        if investigation_done(agent) {
            set_state(agent, AiState::Idle, out);
            agent.interest = None;
        }
        return;
    }
    if let Some(point) = agent.interest.take() {
        set_state(agent, AiState::Investigate { point }, out);
        agent.path.clear();
        agent.path_goal = None;
        agent.path_age = 0.0;
        agent.path_failed_at = None;
        agent.path_done_goal = None;
        return;
    }
    decide_idler(agent, ctx, out);
}

/// True when an investigate goal has been reached or abandoned.
fn investigation_done(agent: &AiAgent) -> bool {
    let AiState::Investigate { point } = agent.state else {
        return false;
    };
    agent.position.distance(point) < 1.0
        || (agent.path.waypoints.is_empty() && agent.path_age > 1.5)
}

/// Prey: any reacting contact within the flee distance starts a retreat.
fn decide_prey(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    let threat = agent.contact.clone().and_then(|contact| {
        let close = contact.position.distance(agent.position)
            <= agent.def.flee_distance.max(contact.radius + 1.0);
        let reacts = agent.def.reacts_to_role(Some(&contact.role));
        (reacts && close).then_some((contact.handle, contact.position))
    });
    let Some((threat_handle, threat_position)) = threat else {
        if matches!(
            agent.state,
            AiState::Flee {
                threat: _,
                destination: _
            }
        ) && (agent.path.waypoints.is_empty()
            || agent
                .path
                .reached()
                .is_none_or(|reached| reached.distance(agent.position) < 0.35))
        {
            set_state(agent, AiState::Idle, out);
        } else if matches!(agent.state, AiState::Investigate { point: _ }) {
            if investigation_done(agent) {
                set_state(agent, AiState::Idle, out);
                agent.interest = None;
            }
        } else if let Some(point) = agent.interest.take() {
            set_state(agent, AiState::Investigate { point }, out);
            agent.path.clear();
            agent.path_goal = None;
            agent.path_age = 0.0;
            agent.path_failed_at = None;
            agent.path_done_goal = None;
        }
        return;
    };
    if matches!(agent.state, AiState::Flee { threat: Some(previous), destination: _ } if previous == threat_handle)
        && agent.path_door_version == ctx.door_version
        && agent
            .path_failed_at
            .is_some_and(|failed| agent.clock - failed < PATH_RETRY_S)
    {
        return;
    }
    let stale = match agent.state {
        AiState::Flee {
            destination,
            threat: previous,
        } => {
            let threat_changed = previous != Some(threat_handle);
            let destination_lost = agent.path_goal != Some(destination);
            let done = agent.path.waypoints.is_empty()
                || (agent.path.complete
                    && agent
                        .path
                        .reached()
                        .is_none_or(|reached| reached.distance(agent.position) < 0.35));
            let threatened =
                threat_position.distance(agent.position) < agent.def.catch_radius * 3.0;
            threat_changed
                || destination_lost
                || done
                || (threatened && agent.path_age >= REPLAN_INTERVAL_S)
        }
        AiState::Idle
        | AiState::Wander { destination: _ }
        | AiState::Hide { destination: _ }
        | AiState::Hidden
        | AiState::Return { destination: _ }
        | AiState::Follow { target: _ }
        | AiState::Investigate { point: _ }
        | AiState::Pursue { target: _ }
        | AiState::Catch { target: _ }
        | AiState::Scripted
        | AiState::Caught => true,
    };
    if !stale {
        return;
    }
    if let Some(destination) = choose_flee_destination(agent, threat_position, ctx, out) {
        // The chooser already built and validated the route; only the state
        // needs the destination so a later tick can tell it apart.
        set_state(
            agent,
            AiState::Flee {
                threat: Some(threat_handle),
                destination,
            },
            out,
        );
    } else {
        // No reachable escape: stop and face the threat without oscillating;
        // a later replan retries when the world changes.
        set_state(
            agent,
            AiState::Flee {
                threat: Some(threat_handle),
                destination: agent.position,
            },
            out,
        );
        agent.path.clear();
        agent.path_goal = Some(agent.position);
        agent.path_done_goal = Some(agent.position);
        agent.path_failed_at = Some(agent.clock);
        agent.path_door_version = ctx.door_version;
        agent.interest = None;
    }
}

/// Sheltered prey roams until threatened, hides only after arriving, and
/// returns along a verified route after a bounded hold.
fn decide_skittish(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    match agent.state {
        AiState::Hidden => {
            agent.hidden_remaining = (agent.hidden_remaining - ctx.delta).max(0.0);
            if agent.hidden_remaining <= 0.0 {
                let destination = agent.shelter_return.unwrap_or(agent.home);
                set_state(agent, AiState::Return { destination }, out);
                agent.path.clear();
                agent.path_goal = None;
                agent.path_done_goal = None;
                agent.shelter_cooldown = agent.def.idle_seconds.max(1.0);
            }
            return;
        }
        AiState::Hide { destination } => {
            if route_completed_at(agent, destination) {
                set_state(agent, AiState::Hidden, out);
                agent.speed_mps = 0.0;
                agent.hidden_remaining = agent.def.hide_seconds;
            } else if agent
                .path_failed_at
                .is_some_and(|failed| agent.clock - failed >= PATH_RETRY_S)
            {
                // A newly blocked shelter is abandoned visibly. A failed or
                // partial path can never make the entity disappear.
                set_state(agent, AiState::Idle, out);
                agent.shelter_cooldown = agent.def.idle_seconds.max(1.0);
            }
            return;
        }
        AiState::Return { destination } => {
            if route_completed_at(agent, destination) {
                set_state(agent, AiState::Idle, out);
                agent.shelter_return = None;
                agent.wait_remaining = agent.def.idle_seconds;
                agent.shelter_cooldown = agent.def.idle_seconds.max(1.0);
            }
            // A closed door may temporarily block emergence. Stay visible
            // and retain the return goal; advance retries on its bounded
            // path cadence rather than teleporting or losing the destination.
            return;
        }
        AiState::Idle
        | AiState::Wander { destination: _ }
        | AiState::Follow { target: _ }
        | AiState::Flee {
            threat: _,
            destination: _,
        }
        | AiState::Investigate { point: _ }
        | AiState::Pursue { target: _ }
        | AiState::Catch { target: _ }
        | AiState::Scripted
        | AiState::Caught => {}
    }
    let threatened = agent.shelter_cooldown <= 0.0
        && agent.contact.as_ref().is_some_and(|contact| {
            agent.def.reacts_to_role(Some(&contact.role))
                && contact.position.distance(agent.position) <= agent.def.flee_distance
        });
    if threatened {
        let return_position = agent.position;
        if let Some(destination) = choose_shelter(agent, ctx, out) {
            agent.shelter_return = Some(return_position);
            set_state(agent, AiState::Hide { destination }, out);
            return;
        }
        agent.shelter_cooldown = PATH_RETRY_S;
    }
    decide_wanderer(agent, ctx, out);
}

fn route_completed_at(agent: &AiAgent, destination: Vec3) -> bool {
    agent
        .path_done_goal
        .is_some_and(|goal| goal.distance(destination) < 0.05)
        && agent.position.distance(destination)
            < movement::arrival_radius(agent.profile.radius) + 0.01
}

/// Evaluates at most sixteen authored shelters. Only complete routes count;
/// the shortest reachable walking distance wins, with authored order on ties.
fn choose_shelter(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    out: &mut AiOutcome,
) -> Option<Vec3> {
    if !path_attempt_due(agent) {
        return None;
    }
    let nav = ctx.nav?;
    let class = agent.class?;
    let bounds = agent.bounds();
    let mut trial = Path::new();
    let mut best: Option<(f32, Vec3, Path)> = None;
    for anchor_index in 0..agent.def.hide_anchors.len() {
        let Some(anchor) = agent.def.hide_anchors.get(anchor_index).copied() else {
            continue;
        };
        let authored = Vec3::from_array(anchor);
        let Some(point) = nav.nearest(
            class,
            authored,
            nav.grid().cell_m.max(0.25),
            0.05,
            ctx,
            agent.profile.can_open_doors,
        ) else {
            continue;
        };
        if bounds.is_some_and(|volume| !volume.contains(point.position)) {
            continue;
        }
        let query = PathQuery {
            class,
            start: agent.position,
            goal: point.position,
            can_open_doors: agent.profile.can_open_doors,
            max_expansions: RUNTIME_PATH_EXPANSIONS,
            doors: ctx,
        };
        if !reserve_path_query(agent, out) {
            break;
        }
        let result = nav.path_into_bounded(&query, &mut agent.scratch, &mut trial, bounds);
        if result.is_err() || !trial.complete {
            continue;
        }
        let distance = trial
            .waypoints
            .windows(2)
            .filter_map(|pair| pair.first().zip(pair.get(1)).map(|(a, b)| a.distance(*b)))
            .sum::<f32>();
        if best
            .as_ref()
            .is_none_or(|(length, _, _)| distance < *length)
        {
            let selected = std::mem::take(&mut trial);
            if let Some((_, _, previous)) = best.replace((distance, point.position, selected)) {
                trial = previous;
            }
        }
    }
    let (_, destination, path) = best?;
    agent.path = path;
    agent.path_age = 0.0;
    agent.path_door_version = ctx.door_version;
    agent.path_goal = Some(destination);
    agent.path_done_goal = None;
    agent.path_failed_at = None;
    agent.current_waypoint = 0;
    Some(destination)
}

/// Follower: keeps a target tag within a comfortable distance.
fn decide_follower(agent: &mut AiAgent, _ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    let contact = agent
        .contact
        .clone()
        .filter(|contact| !contact.heard_only && agent.def.reacts_to_role(Some(&contact.role)));
    let Some(active_contact) = contact else {
        if matches!(agent.state, AiState::Follow { target: _ }) {
            set_state(agent, AiState::Idle, out);
        }
        return;
    };
    let desired = (active_contact.radius + 1.2).max(1.0);
    let distance = active_contact.position.distance(agent.position);
    if distance > desired * 1.6
        && !matches!(agent.state, AiState::Follow { target } if target == active_contact.handle)
    {
        set_state(
            agent,
            AiState::Follow {
                target: active_contact.handle,
            },
            out,
        );
        agent.path.clear();
        agent.path_goal = None;
    }
}

/// Wanderer: picks navigable destinations around its post.
fn decide_wanderer(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if let AiState::Wander { destination } = agent.state {
        let done = agent.path.waypoints.is_empty()
            || (agent.path.complete
                && agent
                    .path
                    .reached()
                    .is_none_or(|reached| reached.distance(agent.position) < 0.45));
        if done && agent.path_age >= agent.def.idle_seconds {
            set_state(agent, AiState::Idle, out);
            agent.wait_remaining = agent.def.idle_seconds;
        } else if agent.path_goal != Some(destination) {
            agent.path.clear();
            agent.path_goal = None;
        }
        return;
    }
    decide_idler(agent, ctx, out);
}

/// Idler: after a wait, walks back to its post, then wanders (a wanderer) or
/// stands (an idler).
///
/// Returning to the authored post is what makes an encounter repeatable: a
/// predator that chased its prey across the map walks back to where the map
/// placed it, so a second activation of the same switch finds it in place.
fn decide_idler(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    match agent.state {
        AiState::Wander { destination: _ } => {
            let done = agent.path.waypoints.is_empty()
                || (agent.path.complete
                    && agent
                        .path
                        .reached()
                        .is_none_or(|reached| reached.distance(agent.position) < 0.45));
            if done && agent.path_age >= agent.def.idle_seconds {
                set_state(agent, AiState::Idle, out);
                agent.wait_remaining = agent.def.idle_seconds;
            }
            return;
        }
        AiState::Idle => {}
        AiState::Follow { target: _ }
        | AiState::Hide { destination: _ }
        | AiState::Hidden
        | AiState::Return { destination: _ }
        | AiState::Flee {
            threat: _,
            destination: _,
        }
        | AiState::Investigate { point: _ }
        | AiState::Pursue { target: _ }
        | AiState::Catch { target: _ }
        | AiState::Scripted
        | AiState::Caught => return,
    }
    agent.wait_remaining = (agent.wait_remaining - ctx.delta).max(0.0);
    if agent.wait_remaining > 0.0 {
        return;
    }
    if agent.position.distance(agent.home) > POST_RETURN_M {
        set_state(
            agent,
            AiState::Wander {
                destination: agent.home,
            },
            out,
        );
        agent.path.clear();
        agent.path_goal = None;
        return;
    }
    if matches!(
        agent.def.behavior,
        AiBehavior::Wanderer | AiBehavior::Skittish
    ) && let Some(destination) = choose_wander_destination(agent, ctx, out)
    {
        set_state(agent, AiState::Wander { destination }, out);
        return;
    }
    agent.wait_remaining = agent.def.idle_seconds.max(0.5);
}

/// Moves one agent along its route, replanning when needed.
fn advance(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    let Some(nav) = ctx.nav else {
        if !agent.reported_unplaced {
            agent.reported_unplaced = true;
            out.unplaced = out.unplaced.saturating_add(1);
        }
        agent.speed_mps = 0.0;
        return;
    };
    if agent.class.is_none() {
        agent.class = nav.class_index(&agent.profile.class());
        if agent.class.is_none() && !agent.reported_unplaced {
            agent.reported_unplaced = true;
            out.unplaced = out.unplaced.saturating_add(1);
        }
    }
    let Some(class) = agent.class else {
        agent.speed_mps = 0.0;
        return;
    };
    let goal = match agent.state {
        AiState::Wander { destination }
        | AiState::Hide { destination }
        | AiState::Return { destination }
        | AiState::Flee {
            destination,
            threat: _,
        } => Some(destination),
        AiState::Investigate { point } => Some(point),
        AiState::Follow { target } | AiState::Pursue { target } => {
            ctx.target(target).map(|target_actor| target_actor.position)
        }
        AiState::Idle
        | AiState::Hidden
        | AiState::Catch { target: _ }
        | AiState::Scripted
        | AiState::Caught => None,
    };
    let Some(goal_position) = goal else {
        agent.speed_mps = 0.0;
        return;
    };
    let speed = match agent.state {
        AiState::Flee {
            threat: _,
            destination: _,
        }
        | AiState::Hide { destination: _ }
        | AiState::Pursue { target: _ } => agent.def.run_speed,
        AiState::Idle
        | AiState::Wander { destination: _ }
        | AiState::Return { destination: _ }
        | AiState::Hidden
        | AiState::Follow { target: _ }
        | AiState::Investigate { point: _ }
        | AiState::Catch { target: _ }
        | AiState::Scripted
        | AiState::Caught => agent.def.walk_speed,
    };
    let goal_changed = agent
        .path_goal
        .is_none_or(|planned| planned.distance(goal_position) > REPLAN_TARGET_M);
    let door_changed = agent.path_door_version != ctx.door_version;
    let close_range_replan =
        matches!(agent.state, AiState::Pursue { target: _ }) && agent.path_age >= REPLAN_INTERVAL_S;
    // A failed attempt is retried on a bounded cadence, not every tick: the
    // attempted goal is remembered so the same failure cannot become a
    // per-frame scan, and the age keeps growing so a state's own completion
    // rule (the investigate and wander timeouts) can still fire.
    let cooling_down = agent.path_failed_at.is_some_and(|failed| {
        agent.clock - failed < PATH_RETRY_S && !goal_changed && !door_changed
    });
    let already_done = agent
        .path_done_goal
        .is_some_and(|done| done.distance(goal_position) <= 0.25);
    let planner_interval = if matches!(agent.state, AiState::Pursue { target: _ }) {
        REPLAN_INTERVAL_S
    } else {
        PATH_RETRY_S
    };
    let needs_path = path_attempt_due(agent)
        && !cooling_down
        && if agent.path.waypoints.is_empty() {
            !already_done || goal_changed || door_changed
        } else if agent.path.complete {
            goal_changed || door_changed || close_range_replan || agent.path_age >= 8.0
        } else {
            // A partial route (the goal is not reachable right now): walk it,
            // then re-ask on the cadence in case the world changed.
            goal_changed || door_changed || agent.path_age >= planner_interval
        };
    if needs_path && reserve_path_query(agent, out) {
        request_obstructed_closing_door(agent, ctx, out);
        let query = PathQuery {
            class,
            start: agent.position,
            goal: goal_position,
            can_open_doors: agent.profile.can_open_doors,
            max_expansions: RUNTIME_PATH_EXPANSIONS,
            doors: ctx,
        };
        let mut path = std::mem::take(&mut agent.path);
        let bounds = agent.bounds();
        let result = nav.path_into_bounded(&query, &mut agent.scratch, &mut path, bounds);
        agent.path = path;
        agent.path_door_version = ctx.door_version;
        if result.is_ok() {
            if agent.path.complete {
                agent.path_age = 0.0;
                agent.path_failed_at = None;
                agent.path_done_goal = None;
            } else {
                // A partial route is a bounded retry, not a fresh plan: keep
                // the age so a state's completion timeout can still fire.
                agent.path_failed_at = Some(agent.clock);
            }
            agent.path_goal = Some(goal_position);
            agent.current_waypoint = 0;
        } else {
            // Keep the age and the attempted goal: the caller's timeout
            // rules and the retry cadence both read them.
            agent.path_failed_at = Some(agent.clock);
            agent.path_goal = Some(goal_position);
            agent.path_done_goal = None;
            agent.path.clear();
            agent.current_waypoint = 0;
            agent.speed_mps = 0.0;
            return;
        }
    }
    if agent.path.waypoints.is_empty() {
        agent.speed_mps = 0.0;
        return;
    }
    // A closed door on the route: ask for it, then wait for the leaf.
    request_route_doors(agent, ctx, out);
    if route_waits_for_door(agent, ctx) {
        agent.speed_mps = 0.0;
        if agent
            .path
            .door_requests
            .iter()
            .any(|door| ctx.door_obstructed(door))
        {
            agent.stuck_seconds += ctx.delta;
            if agent.stuck_seconds >= STUCK_SECONDS {
                agent.path.clear();
                agent.path_failed_at = Some(agent.clock - PATH_RETRY_S);
                agent.current_waypoint = 0;
                agent.stuck_seconds = 0.0;
            }
        } else {
            agent.stuck_seconds = 0.0;
        }
        return;
    }
    let waypoint = agent
        .path
        .waypoints
        .get(agent.current_waypoint)
        .copied()
        .or_else(|| agent.path.waypoints.last().copied());
    let Some(next_waypoint) = waypoint else {
        agent.speed_mps = 0.0;
        return;
    };
    match local_move_step(agent, ctx, out, nav, class, next_waypoint, speed) {
        MoveStep::Moved {
            position,
            yaw_degrees,
            speed_mps,
        } => {
            if agent
                .bounds()
                .is_some_and(|bounds| !bounds.contains(position))
            {
                agent.speed_mps = 0.0;
                agent.path.clear();
                agent.path_goal = None;
                return;
            }
            let progress =
                next_waypoint.distance(agent.position) - next_waypoint.distance(position);
            if progress <= 0.0005 {
                agent.stuck_seconds += ctx.delta;
            } else {
                agent.stuck_seconds = 0.0;
            }
            agent.position = position;
            agent.yaw_degrees = yaw_degrees;
            agent.speed_mps = speed_mps;
            if agent.stuck_seconds >= STUCK_SECONDS {
                agent.stuck_seconds = 0.0;
                agent.path.clear();
                agent.path_failed_at = Some(agent.clock - PATH_RETRY_S);
                agent.current_waypoint = 0;
            }
            if next_waypoint.distance(agent.position)
                < movement::arrival_radius(agent.profile.radius)
            {
                agent.current_waypoint = agent.current_waypoint.saturating_add(1);
            }
            out.moved.push((
                agent.handle,
                agent.position,
                agent.yaw_degrees,
                agent.spawned,
            ));
            if agent.current_waypoint >= agent.path.waypoints.len() {
                agent.path_done_goal = agent.path_goal.filter(|_| agent.path.complete);
                agent.path.clear();
                agent.current_waypoint = 0;
                if matches!(agent.state, AiState::Wander { destination: _ }) {
                    agent.wait_remaining = agent.def.idle_seconds;
                }
            }
        }
        MoveStep::Blocked => {
            agent.speed_mps = 0.0;
            agent.stuck_seconds += ctx.delta;
            if agent.stuck_seconds >= STUCK_SECONDS {
                agent.stuck_seconds = 0.0;
                agent.path.clear();
                agent.path_failed_at = Some(agent.clock - PATH_RETRY_S);
                agent.current_waypoint = 0;
            }
        }
        MoveStep::Arrived => {
            agent.speed_mps = 0.0;
            agent.current_waypoint = agent.current_waypoint.saturating_add(1);
            if agent.current_waypoint >= agent.path.waypoints.len() {
                agent.path_done_goal = agent.path_goal.filter(|_| agent.path.complete);
                agent.path.clear();
                agent.current_waypoint = 0;
                if matches!(agent.state, AiState::Wander { destination: _ }) {
                    agent.wait_remaining = agent.def.idle_seconds;
                }
            }
        }
    }
    try_catch(agent, ctx, out);
}

/// Short, collision-checked probes give opposing actors a consistent right
/// shoulder. Actors stay out of the global topology; a narrow passage waits
/// instead of inventing a route through another body.
fn local_move_step(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    out: &AiOutcome,
    nav: &NavMesh,
    class: usize,
    waypoint: Vec3,
    speed: f32,
) -> MoveStep {
    let base = AgentMove {
        position: agent.position,
        yaw_degrees: agent.yaw_degrees,
        radius: agent.profile.radius,
        height: agent.profile.height,
        step_height: agent.profile.step_height,
        max_slope: agent.profile.max_slope,
        speed_mps: speed,
    };
    if waypoint.distance(agent.position) <= movement::arrival_radius(agent.profile.radius) {
        return MoveStep::Arrived;
    }
    let flat = glam::Vec2::new(waypoint.x - agent.position.x, waypoint.z - agent.position.z);
    let Some(direction) = flat.try_normalize() else {
        return MoveStep::Blocked;
    };
    let lookahead = (agent.profile.radius * 2.0 + speed * 0.35)
        .clamp(0.35, 1.2)
        .min(flat.length());
    // A long avoidance probe may leave a tight doorway's valid corridor even
    // though a short shoulder adjustment is safe. Try the ordinary horizon
    // first, then bounded within-cell corrections before abandoning the route.
    for horizon in [lookahead, lookahead.min(0.25), lookahead.min(0.10)] {
        for degrees in [0.0_f32, 35.0, -35.0, 65.0, -65.0, 90.0, -90.0] {
            let (sin, cos) = degrees.to_radians().sin_cos();
            let steering = glam::Vec2::new(
                direction.x.mul_add(cos, direction.y * sin),
                direction.y.mul_add(cos, -direction.x * sin),
            );
            let x = steering.x.mul_add(horizon, agent.position.x);
            let z = steering.y.mul_add(horizon, agent.position.z);
            let Some(y) = movement::surface_along(
                &base,
                glam::Vec2::new(x, z),
                ctx.floor,
                ctx.ceiling,
                ctx.index,
                ctx.walls,
            ) else {
                continue;
            };
            let probe = Vec3::new(x, y, z);
            if agent.bounds().is_some_and(|bounds| !bounds.contains(probe))
                || !nav.segment_clear(class, agent.position, probe, ctx, false)
                || !actor_segment_clear(agent, agent.position, probe, ctx, out)
            {
                continue;
            }
            let mut mover = base;
            let result = mover.step_with_solids(
                probe,
                ctx.delta,
                ctx.walls,
                ctx.index,
                ctx.floor,
                ctx.ceiling,
                ctx.leaves,
                ctx.obstacles,
            );
            if let MoveStep::Moved {
                position,
                yaw_degrees: _,
                speed_mps: _,
            } = result
                && nav.segment_clear(class, agent.position, position, ctx, false)
                && actor_segment_clear(agent, agent.position, position, ctx, out)
                && agent
                    .bounds()
                    .is_none_or(|bounds| bounds.contains(position))
            {
                return result;
            }
        }
    }
    MoveStep::Blocked
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite body positions use vector subtraction and scaling; projection is clamped to the bounded walked segment."
)]
fn actor_segment_clear(
    agent: &AiAgent,
    from: Vec3,
    to: Vec3,
    ctx: &AiTickContext<'_>,
    out: &AiOutcome,
) -> bool {
    let origin = glam::Vec2::new(from.x, from.z);
    let motion = glam::Vec2::new(to.x - from.x, to.z - from.z);
    ctx.targets.iter().all(|target| {
        if target.handle == agent.handle || target.caught {
            return true;
        }
        // Earlier movers in this tick have already published their pose. Use
        // that pose so processing order cannot let two bodies enter one gap.
        let target_position = out
            .moved
            .iter()
            .rev()
            .find(|(handle, _, _, _)| *handle == target.handle)
            .map_or(target.position, |(_, position, _, _)| *position);
        if target_position.y + target.height <= from.y.min(to.y) + 0.01
            || target_position.y >= from.y.max(to.y) + agent.profile.height - 0.01
        {
            return true;
        }
        let center = glam::Vec2::new(target_position.x, target_position.z);
        let t =
            ((center - origin).dot(motion) / motion.length_squared().max(1.0e-8)).clamp(0.0, 1.0);
        let closest = origin + motion * t;
        let radius = agent.profile.radius + target.radius + 0.015;
        let distance = closest.distance_squared(center);
        // An externally placed overlapping body may escape, but cannot deepen
        // the overlap. Normal movement always requires a clear swept disc.
        distance >= radius * radius
            || ((origin - center).length_squared() < radius * radius
                && motion.dot(origin - center) > 0.0
                && (origin + motion - center).length_squared() > (origin - center).length_squared())
    })
}

/// Fires a catch when the predator is genuinely on top of a visible target.
fn try_catch(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if !matches!(agent.state, AiState::Pursue { target: _ }) || agent.catch_cooldown > 0.0 {
        return;
    }
    let AiState::Pursue {
        target: target_handle,
    } = agent.state
    else {
        return;
    };
    let Some(target) = ctx.target(target_handle) else {
        return;
    };
    if target.caught || !agent.def.reacts_to_role(target.role.as_deref()) {
        return;
    }
    if !can_catch(agent, target, ctx) {
        return;
    }
    set_state(
        agent,
        AiState::Catch {
            target: target_handle,
        },
        out,
    );
    agent.speed_mps = 0.0;
    agent.path.clear();
    agent.path_goal = None;
    out.events.push((
        EventKind::Caught,
        agent.handle,
        target.instance_id.clone(),
        Some(agent.handle),
    ));
    out.catches.push((agent.handle, target_handle));
}

/// True when the predator may catch this target right now.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam vector addition, subtraction and scaling intentionally use ordinary f32 arithmetic; no integer sizing or indexing is performed here."
)]
fn can_catch(agent: &AiAgent, target: &AiTarget, ctx: &AiTickContext<'_>) -> bool {
    if target.handle == PLAYER_TARGET_HANDLE {
        return false;
    }
    let dx = target.position.x - agent.position.x;
    let dz = target.position.z - agent.position.z;
    let horizontal = dx.mul_add(dx, dz * dz).sqrt();
    let reach = agent.def.catch_radius + target.radius;
    if horizontal > reach {
        return false;
    }
    if (target.position.y - agent.position.y).abs() > agent.def.catch_height {
        return false;
    }
    // No catch through a wall or a floor: the predator must see its target.
    let from = agent.position + Vec3::new(0.0, (agent.profile.height * 0.5).max(0.1), 0.0);
    let to = target.position + Vec3::new(0.0, (target.height * 0.5).max(0.1), 0.0);
    perception::sight_clear_with_leaves(ctx.index, ctx.walls, ctx.leaves, from, to)
}

/// Picks a flee destination from navigable candidates around the threat.
fn choose_flee_destination(
    agent: &mut AiAgent,
    threat: Vec3,
    ctx: &AiTickContext<'_>,
    out: &mut AiOutcome,
) -> Option<Vec3> {
    if !path_attempt_due(agent) {
        return None;
    }
    let nav = ctx.nav?;
    let class = agent.class?;
    let away = {
        let dx = agent.position.x - threat.x;
        let dz = agent.position.z - threat.z;
        let length = dx.hypot(dz);
        if length <= 1.0e-3 {
            (1.0, 0.0)
        } else {
            (dx / length, dz / length)
        }
    };
    let base_angle = away.1.atan2(away.0);
    let mut candidates: Vec<(f32, Vec3)> = Vec::new();
    for index in 0..FLEE_CANDIDATES {
        // The first candidate is straight away from the threat; the rest
        // sweep the full circle so a cornered agent always has choices.
        #[expect(
            clippy::cast_precision_loss,
            clippy::as_conversions,
            reason = "The fixed candidate count is 16 and the loop index is smaller; both integers have exact f32 representations."
        )]
        let turn = (index as f32 / FLEE_CANDIDATES as f32) * std::f32::consts::TAU;
        let (sin, cos) = (base_angle + turn).sin_cos();
        for distance in [
            agent.def.flee_distance * 0.6,
            agent.def.flee_distance,
            agent.def.flee_distance * 1.5,
        ] {
            let candidate = Vec3::new(
                agent.position.x + cos * distance,
                agent.position.y,
                agent.position.z + sin * distance,
            );
            let Some(point) = nav.nearest(
                class,
                candidate,
                nav.grid().cell_m.max(1.2),
                1.5,
                ctx,
                agent.profile.can_open_doors,
            ) else {
                continue;
            };
            if point.position.distance(agent.position)
                <= movement::arrival_radius(agent.profile.radius)
            {
                continue;
            }
            let Some(region) = nav.grid().region_of(class, point.cell) else {
                continue;
            };
            let separation = threat.distance(point.position);
            let region_cells =
                f32::from(u16::try_from(nav.region_cells(class, region).min(4000)).unwrap_or(4000));
            // Never pick a destination that starts by running at the threat:
            // the farther side of a big region is useless if reaching it means
            // passing through the predator.
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "glam vector addition, subtraction and scaling intentionally use ordinary f32 arithmetic; no integer sizing or indexing is performed here."
            )]
            let (to_candidate, to_threat) =
                (point.position - agent.position, threat - agent.position);
            let approach =
                if to_candidate.length_squared() > 1.0e-6 && to_threat.length_squared() > 1.0e-6 {
                    to_candidate.normalize().dot(to_threat.normalize())
                } else {
                    0.0
                };
            // Prefer separation, then a large region (fewer blind ends), then
            // open space (a corner has fewer walkable neighbours), then
            // proximity to the current position so the run stays readable.
            let openness = f32::from(nav.open_neighbours(class, point.cell));
            let score = separation + (region_cells.min(4000.0) / 4000.0) * 0.8 + openness * 0.12
                - point.position.distance(agent.position) * 0.05
                - approach.max(0.0) * 3.0;
            candidates.push((score, point.position));
        }
    }
    if candidates.is_empty() {
        return None;
    }
    // Highest score first; ties break on position so the choice is
    // deterministic. A candidate whose real path is unreachable falls through
    // to the next one instead of leaving the agent frozen.
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.x.total_cmp(&b.1.x))
            .then_with(|| a.1.z.total_cmp(&b.1.z))
    });
    for (_, destination) in candidates.into_iter().take(FLEE_PATH_ATTEMPTS) {
        let query = PathQuery {
            class,
            start: agent.position,
            goal: destination,
            can_open_doors: agent.profile.can_open_doors,
            max_expansions: RUNTIME_PATH_EXPANSIONS,
            doors: ctx,
        };
        if !reserve_path_query(agent, out) {
            break;
        }
        let mut path = std::mem::take(&mut agent.path);
        let bounds = agent.bounds();
        let result = nav.path_into_bounded(&query, &mut agent.scratch, &mut path, bounds);
        agent.path = path;
        if result.is_ok() {
            agent.path_age = 0.0;
            agent.path_failed_at = None;
            agent.path_done_goal = None;
            agent.path_door_version = ctx.door_version;
            agent.path_goal = Some(destination);
            agent.current_waypoint = 0;
            return Some(destination);
        }
    }
    None
}

/// Picks a navigable wander destination near the agent.
///
/// Several directions around the post are tried before giving up, so a large
/// wander radius next to a wall does not make the agent stand still forever.
#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "The eight candidate indices convert exactly; the u32 perception phase intentionally becomes a rounded finite f32 angular offset without changing its stored counter."
)]
fn choose_wander_destination(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    out: &mut AiOutcome,
) -> Option<Vec3> {
    if !path_attempt_due(agent) {
        return None;
    }
    let nav = ctx.nav?;
    let class = agent.class?;
    let base = agent.clock.mul_add(0.7, agent.perception_phase as f32);
    let bounds = agent.bounds();
    for index in 0..WANDER_CANDIDATES {
        let angle = base + index as f32 * std::f32::consts::TAU / WANDER_CANDIDATES as f32;
        let (sin, cos) = angle.sin_cos();
        let distance = agent.def.wander_radius.max(1.0) * (0.4 + 0.6 * sin.abs());
        let candidate = bounds.map_or_else(
            || {
                Vec3::new(
                    agent.position.x + cos * distance,
                    agent.position.y,
                    agent.position.z + sin * distance,
                )
            },
            |volume| {
                let radius = agent.def.wander_radius;
                let x0 = volume.min.x.max(agent.home.x - radius);
                let x1 = volume.max.x.min(agent.home.x + radius);
                let z0 = volume.min.z.max(agent.home.z - radius);
                let z1 = volume.max.z.min(agent.home.z + radius);
                Vec3::new(
                    (x1 - x0).mul_add(0.5 + 0.45 * cos, x0),
                    agent.home.y,
                    (z1 - z0).mul_add(0.5 + 0.45 * sin, z0),
                )
            },
        );
        if bounds.is_some() && candidate.distance(agent.home) > agent.def.wander_radius {
            continue;
        }
        let Some(point) = nav.nearest(
            class,
            candidate,
            2.5,
            1.5,
            ctx,
            agent.profile.can_open_doors,
        ) else {
            continue;
        };
        if bounds.is_none_or(|volume| {
            volume.contains(point.position)
                && point.position.distance(agent.home) <= agent.def.wander_radius
        }) && point.position.distance(agent.position) > 0.5
        {
            let query = PathQuery {
                class,
                start: agent.position,
                goal: point.position,
                can_open_doors: agent.profile.can_open_doors,
                max_expansions: RUNTIME_PATH_EXPANSIONS,
                doors: ctx,
            };
            if !reserve_path_query(agent, out) {
                break;
            }
            let result = nav.path_into_bounded(&query, &mut agent.scratch, &mut agent.path, bounds);
            if result.is_ok() && agent.path.complete {
                agent.path_goal = Some(point.position);
                agent.path_door_version = ctx.door_version;
                agent.path_age = 0.0;
                agent.path_failed_at = None;
                agent.path_done_goal = None;
                agent.current_waypoint = 0;
                return Some(point.position);
            }
        }
    }
    None
}

/// A stopped closing leaf can cover the actor's starting portal. Planning
/// correctly refuses it, but a capable nearby actor must still be able to
/// request the open end after unlock. This runs only with a budgeted search,
/// and asks for at most one physically nearby, obstructed closing leaf.
fn request_obstructed_closing_door(agent: &AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if !agent.profile.can_open_doors {
        return;
    }
    let Some(door) = ctx.doors.iter().find(|door| {
        door.phase() == crate::engine::door::DoorPhase::Closing
            && door.obstructed
            && !door.is_locked()
            && door
                .collider()
                .overlaps_body_y(agent.position.y, agent.profile.height)
            && door.collider().overlaps_disc(
                agent.position.x,
                agent.position.z,
                agent.profile.radius + 0.5,
            )
    }) else {
        return;
    };
    if !out.door_requests.iter().any(|(id, _)| id == &door.def.id) {
        out.door_requests.push((door.def.id.clone(), agent.handle));
    }
}

/// Asks for a closed door on the route.
fn request_route_doors(agent: &AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    if !agent.profile.can_open_doors || agent.path.door_requests.is_empty() {
        return;
    }
    for door in &agent.path.door_requests {
        if ctx.doors.door_open(door) || ctx.doors.is_locked(door).unwrap_or(false) {
            continue;
        }
        if !out
            .door_requests
            .iter()
            .any(|(requested, _)| requested == door)
        {
            out.door_requests.push((door.clone(), agent.handle));
        }
    }
}

/// True when a requested door is still closed under the agent's feet.
fn route_waits_for_door(agent: &AiAgent, ctx: &AiTickContext<'_>) -> bool {
    agent.profile.can_open_doors
        && agent
            .path
            .door_requests
            .iter()
            .any(|door| !ctx.doors.door_open(door) && !ctx.doors.is_locked(door).unwrap_or(false))
}

/// Releases movement noise for one agent; returns the stimuli count.
fn release_noise(agent: &mut AiAgent, out: &mut AiOutcome) -> u64 {
    if agent.speed_mps < NOISE_SPEED_MPS {
        return 0;
    }
    if agent.clock - agent.last_noise_at < NOISE_INTERVAL_S {
        return 0;
    }
    agent.last_noise_at = agent.clock;
    let radius = 3.0 + agent.speed_mps;
    out.stimuli.push(Stimulus {
        position: agent.position,
        radius,
        loudness: 1.0,
        category: "movement".to_string(),
        source: Some(agent.handle),
        age: 0.0,
    });
    1
}

/// Writes one state transition and reports it once.
fn set_state(agent: &mut AiAgent, state: AiState, out: &mut AiOutcome) {
    if agent.state == state {
        return;
    }
    agent.state = state;
    out.transitions = out.transitions.saturating_add(1);
    out.events.push((
        EventKind::AiState,
        agent.handle,
        state.name().to_string(),
        Some(agent.handle),
    ));
}

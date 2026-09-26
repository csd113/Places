use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::collision::{
    CONTACT_EPS, CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, STEP_EPS,
    WallAabb, highest_support_top, lowest_underside, resolve_player_collision_for_body,
    segment_overlaps_aabb,
};
use crate::entity::{EntityFrame, EntityRoutes, PoseCue, RouteState, RouteWorld};
use crate::input::{Control, InputState};
use crate::interact::{Interactables, nearest_target};
use crate::level::{
    ActionDef, AreaTriggers, Ladder, Ladders, LevelDef, LevelSurfaces, WalkableCeiling,
    WalkableFloor, WaterSample, WaterVolumes,
};
use crate::settings::Settings;

pub const TWO_PI: f32 = std::f32::consts::TAU;
pub const EYE_HEIGHT: f32 = 1.6;

/// Eye offset above the feet while crouched: exactly half the standing offset,
/// matching the crouched body height being half the standing height.
pub const CROUCH_EYE_HEIGHT: f32 = EYE_HEIGHT * 0.5;

pub const MAX_PITCH: f32 = 1.4835; // ~85 degrees in radians

/// Downward acceleration applied to the vertical velocity while airborne, in
/// m/s^2. One world unit is one metre, so this is the physical 9.8 m/s^2.
pub const GRAVITY: f32 = 9.8;

/// Height of a full jump's apex above the take-off floor, in metres.
///
/// The office desk's top surface in the demo is 0.75 m above its floor, and a
/// standing jump must clear it with margin to land on it: the apex adds 0.10 m
/// of clearance rather than making the desk exactly unreachable.
pub const JUMP_CLEARANCE_M: f32 = 0.10;

/// Height of the office desk top this jump is sized against, in metres.
pub const OFFICE_DESK_TOP_M: f32 = 0.75;

pub const JUMP_APEX_M: f32 = OFFICE_DESK_TOP_M + JUMP_CLEARANCE_M;

/// Take-off speed of a jump, in m/s: the f32 value of
/// `sqrt(2.0 * GRAVITY * JUMP_APEX_M)` (the square root is not available in a
/// `const` initializer). A test re-derives it from the formula.
pub const JUMP_VELOCITY: f32 = 4.081_666_5;

/// Fixed vertical integration step, in seconds.
///
/// Vertical motion runs in this fixed substep regardless of the frame rate, so
/// a jump's apex is frame-rate independent; at most twelve of them fit in
/// [`MAX_SIM_DELTA`].
pub const VERTICAL_SUBSTEP: f32 = 1.0 / 120.0;

/// Upper bound on vertical substeps consumed in one frame.
const MAX_VERTICAL_SUBSTEPS: usize = 12;

/// Water depth at the player's feet at or below which the player wades: the
/// historical walking controller, including jumping, applies unchanged.
pub const WADE_DEPTH: f32 = 0.55;

/// How far the underwater swimmer's eye may descend toward the pool floor, in
/// metres.
///
/// Swimming is horizontal, so the eye sits close to the body's underside
/// rather than a standing eye height above the floor. This is what lets a
/// swimmer fully submerge in a pool shallower than `EYE_HEIGHT` (the Places
/// Demo basin is 1.35 m deep) while the body still never passes through the
/// floor. It is a buoyancy constant, deliberately separate from the land eye
/// offset so a stance change never rescales the swim pose.
pub const SWIM_FLOOR_CLEARANCE: f32 = 0.55;

/// Horizontal speed multiplier while swimming.
pub const SWIM_SPEED_FACTOR: f32 = 0.55;

/// Upward speed while Jump is held in deep water, in m/s.
pub const SWIM_RISE_SPEED: f32 = 1.1;

/// How far above the water surface the eye floats while Jump is held, in
/// metres.
pub const FLOAT_EYE_MARGIN: f32 = 0.12;

/// Amplitude of the idle bob at the float line, in metres.
pub const SWIM_BOB_AMPLITUDE: f32 = 0.03;

/// Angular speed of the float bob, in radians per second.
pub const SWIM_BOB_SPEED: f32 = 2.4;

/// Reduced gravity while swimming, in m/s^2 (negative is downward).
pub const SWIM_GRAVITY: f32 = -2.2;

/// Terminal sink speed in water, in m/s (positive magnitude).
pub const SWIM_SINK_TERMINAL: f32 = 0.5;

/// Maximum water depth below the surface at which standing up is allowed, in
/// metres: the standing eye offset minus the top of the swim band.
///
/// At exactly this depth the standing eye sits at the band's top
/// ([`SURFACE_SWIM_EYE_MARGIN`] + [`FLOAT_EYE_MARGIN`] above the surface), so a
/// stand-up can never immediately re-enter swimming. This replaces the
/// historical `WADE_DEPTH` equality, which refused standing in chest-deep
/// water a 1.8 m body can plainly stand in; the per-stance check scales it for
/// the crouched eye.
pub const EXIT_DEPTH: f32 = EYE_HEIGHT - SWIM_BAND_MARGIN;

/// The top of the swim band above the free surface, in metres: how far above
/// the waterline the eye may be while the body still counts as in the water.
const SWIM_BAND_MARGIN: f32 = SURFACE_SWIM_EYE_MARGIN + FLOAT_EYE_MARGIN;

/// How far below the water surface the eye may be and still stand up, in
/// metres.
const EXIT_EYE_MARGIN: f32 = 0.6;

/// Eye height above which the swimming pose is the surface pose, in metres.
const SURFACE_SWIM_EYE_MARGIN: f32 = 0.25;

/// Vertical climbing speed on a ladder, in m/s.
pub const LADDER_CLIMB_SPEED: f32 = 2.2;

/// How much of the movement direction must point along a ladder's facing
/// before the input counts as climbing up (and, negated, as climbing down).
pub const LADDER_INTENT_THRESHOLD: f32 = 0.3;

/// Fraction of the walking speed available for sideways movement while
/// attached to a ladder.
pub const LADDER_SIDE_SPEED_FACTOR: f32 = 0.5;

/// Horizontal speed above which a grounded player counts as walking rather
/// than idle, in m/s.
///
/// A speed threshold (not a per-frame displacement) keeps the classification
/// frame-rate independent: a player walking the default 3 m/s must read as
/// walking at 30, 60 or 144 fps alike, while collision jitter around zero —
/// pressed into a wall, sliding to a stop — stays idle.
const WALKING_SPEED_EPSILON_M_PER_S: f32 = 0.05;

/// Upper bound applied to the delta time used for gameplay simulation.
///
/// Protects movement and collision from exploding into excessive subdivision
/// after a temporary stall (level load, pack import, OS hiccup). Real elapsed
/// time is still tracked separately for the FPS/performance overlay.
pub const MAX_SIM_DELTA: f32 = 0.1;

/// Clamps a frame delta for simulation use, tolerating non-finite input.
const fn clamp_sim_delta(delta: f32) -> f32 {
    if delta.is_nan() {
        0.0
    } else {
        delta.clamp(0.0, MAX_SIM_DELTA)
    }
}

/// High-level application/menu lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    MainMenu,
    LevelSelect,
    Settings,
    Playing,
    Paused,
    PauseSettings,
}

/// The player's locomotion pose, derived once per Playing update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocomotionState {
    /// Grounded and standing still.
    #[default]
    Idle,
    /// Grounded and moving horizontally.
    Walking,
    /// Off the floor: rising in a jump, falling, or leaving the water.
    Airborne,
    /// In deep water with the eye below the surface line.
    Swimming,
    /// In deep water with the eye at or above the surface line.
    SurfaceSwimming,
}

/// The player's stance: a crouch halves the collision body height and the eye
/// offset above the feet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stance {
    #[default]
    Standing,
    Crouched,
}

impl Stance {
    /// Collision cylinder height for this stance, in metres.
    #[must_use]
    pub const fn height(self) -> f32 {
        match self {
            Self::Standing => PLAYER_HEIGHT,
            Self::Crouched => CROUCH_HEIGHT,
        }
    }

    /// Eye offset above the feet for this stance, in metres.
    #[must_use]
    pub const fn eye_offset(self) -> f32 {
        match self {
            Self::Standing => EYE_HEIGHT,
            Self::Crouched => CROUCH_EYE_HEIGHT,
        }
    }

    /// The other stance.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Standing => Self::Crouched,
            Self::Crouched => Self::Standing,
        }
    }
}

/// What a character animation system needs to know about the player.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LocomotionSnapshot {
    pub state: LocomotionState,
    /// Actual horizontal speed for the frame, in m/s.
    pub speed: f32,
}

/// The static, level-derived collision world the controller queries.
///
/// This is the reusable bundle later systems hand to [`Game::new`] and
/// [`Game::reset_level`]: walls and solid props as axis-aligned boxes, the
/// walkable floor and ceiling samplers, the water volumes, the ladder volumes,
/// the area triggers and the interactable instances. Building it once per
/// level load is what keeps the per-frame query surface a fixed set of
/// samplers rather than a mesh walk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollisionWorld {
    pub walls: Vec<WallAabb>,
    pub floor: WalkableFloor,
    pub water: WaterVolumes,
    pub ceiling: WalkableCeiling,
    pub ladders: Ladders,
    /// Authored area triggers, resolved with their ids and vertical bounds.
    pub triggers: AreaTriggers,
    /// Placed props/entities that declare a map-authored interaction.
    pub interactables: Interactables,
    /// Authored movement/pose routes for placed entities.
    pub routes: EntityRoutes,
}

impl CollisionWorld {
    /// Resolves every collision sampler against a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        Self {
            walls: level.collision_aabbs(),
            floor: WalkableFloor::from_level(level),
            water: WaterVolumes::from_level(level),
            ceiling: WalkableCeiling::from_level(level),
            ladders: Ladders::from_level(level),
            triggers: AreaTriggers::from_level(level),
            interactables: Interactables::from_level(level),
            routes: EntityRoutes::from_level(level),
        }
    }
}

/// Eye position for a level's authored spawn.
///
/// The spawn is resolved against the *actual* walkable floor under it — a room
/// base elevation plus any floor region — so a player is never left beneath an
/// elevated floor, embedded in one, or floating above a recessed region. A
/// spawn outside every room (which legacy levels are allowed to have) falls
/// back to the historical world floor at `0.0`.
#[must_use]
pub fn spawn_position(level: &LevelDef) -> Vec3 {
    let floor_y = LevelSurfaces::new(level)
        .floor_y_at(level.spawn.x, level.spawn.z)
        .unwrap_or(0.0);
    Vec3::new(level.spawn.x, floor_y + EYE_HEIGHT, level.spawn.z)
}

/// Eye Y for a world position: the walkable floor under it plus the standard
/// eye height, falling back to the historical world floor at `0.0` outside
/// every room.
#[must_use]
pub fn spawn_eye_y(floor: &WalkableFloor, x: f32, z: f32) -> f32 {
    floor.height_at(x, z).unwrap_or(0.0) + EYE_HEIGHT
}

/// What one dispatched action batch did.
///
/// Returned by [`Game::dispatch_actions`] and [`Game::dispatch_interaction`] so
/// the frame loop can report what happened without reaching into run state. A
/// `reset_to_start` stops the batch (safe ordering), so `actions_run` counts the
/// actions actually executed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchReport {
    /// Actions executed, including the one that reset the player.
    pub actions_run: usize,
    /// Labels toggled on, summed over the batch.
    pub labels_shown: usize,
    /// Labels toggled off, summed over the batch.
    pub labels_hidden: usize,
    /// True when an action returned the player to the authored spawn.
    pub player_reset: bool,
    /// Actions skipped because their explicit `target` names no instance.
    pub missing_targets: usize,
    /// Actions skipped because the engine does not implement them (validation
    /// rejects these for loaded levels; programmatic use stays safe).
    pub unsupported: usize,
    /// Animation cues started on placed entities, summed over the batch.
    pub animations_started: usize,
}

impl DispatchReport {
    /// Number of label toggles, whichever direction.
    #[must_use]
    pub const fn labels_toggled(&self) -> usize {
        self.labels_shown.saturating_add(self.labels_hidden)
    }
}

/// One area trigger's runtime state, parallel to [`Game::triggers`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TriggerState {
    /// True while the player's feet were inside the volume at the last update:
    /// leaving re-arms the trigger.
    inside: bool,
    /// Seconds until the trigger may fire again.
    cooldown_remaining: f32,
    /// True once a `once` trigger has fired in this run.
    fired_once: bool,
    /// True when an entry (or swept crossing) happened but another trigger
    /// already dispatched this frame: the batch runs on a later frame instead
    /// of being lost.
    pending: bool,
}

/// Manages game loop timing, player state, and menu lifecycle.
// The flags are independent, documented state machines (Rust's enum-per-flag
// would obscure the existing public fields), not interchangeable booleans.
#[allow(clippy::struct_excessive_bools)]
pub struct Game {
    running: bool,
    app_state: AppState,
    last_frame_time: Instant,
    /// Real elapsed time since the previous frame (used for FPS measurement).
    delta_seconds: f32,
    /// Clamped delta used for gameplay simulation (see [`MAX_SIM_DELTA`]).
    sim_delta_seconds: f32,
    frame_count: u64,
    /// World Y of the eye. `feet_y() + eye_offset()` while grounded and while
    /// walking; off that line while airborne, swimming or climbing.
    pub player_position: Vec3,
    /// World Y of the walkable floor the player is standing on: the pitch line
    /// across a staircase, the exact rendered surface everywhere else (see
    /// [`crate::level::WalkableFloor::walk_height_at`]). This is the value
    /// collision filters against, so the camera and the collision band always
    /// agree about the local floor.
    pub player_floor_y: f32,
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub walls: Vec<WallAabb>,
    /// The level's walkable floor surfaces (rooms + local floor regions).
    pub floor: WalkableFloor,
    /// The level's walkable ceilings, sampled to clamp a jumping head.
    pub ceiling: WalkableCeiling,
    /// The level's water volumes, sampled every Playing update.
    pub water: WaterVolumes,
    /// The level's climbable ladder volumes.
    pub ladders: Ladders,
    /// The level's area triggers, sampled every Playing update. Private with
    /// [`Game::triggers`] so it can never be replaced without its parallel
    /// runtime state (`trigger_states`) being re-seeded.
    triggers: AreaTriggers,
    /// The level's interactable placed instances (props/entities with an
    /// authored interaction, plus label-only targets), resolved once at load.
    /// Private with [`Game::interactables`] so it can never be replaced without
    /// its parallel label state (`label_visible`) being re-sized.
    interactables: Interactables,
    /// Vertical speed in m/s, positive upward. Zero while grounded.
    pub vertical_velocity: f32,
    /// True while the player stands on the walkable floor or a solid prop top.
    pub grounded: bool,
    /// The locomotion pose and speed reported to animation, refreshed by every
    /// Playing update.
    locomotion: LocomotionSnapshot,
    /// True while the water under the player is deep enough to swim.
    swimming: bool,
    /// The current stance (standing or crouched).
    stance: Stance,
    /// Index into [`Game::ladders`] while attached to a ladder.
    climbing: Option<usize>,
    /// Set on the first frame Jump is held and cleared on release: one press is
    /// one jump, and landing (or standing up in water) while holding the key
    /// never bounces.
    jump_latched: bool,
    /// Set on the first frame Crouch is held and cleared on release: one press
    /// is one stance toggle.
    crouch_latched: bool,
    /// Set on the first frame Interact is held and cleared on release: one
    /// press is one interaction, never a held-key repeat.
    interact_latched: bool,
    /// Set for the frame Interact was first pressed; consumed by
    /// [`Game::take_interact_press`] so dispatch happens exactly once.
    interact_pressed: bool,
    /// Per-trigger runtime state, parallel to [`Game::triggers`].
    trigger_states: Vec<TriggerState>,
    /// Authored routes, private with [`Game::routes`] so they can never be
    /// replaced without the parallel runtime state being re-seeded.
    routes: EntityRoutes,
    /// Per-route runtime state, parallel to [`Game::routes`].
    route_states: Vec<RouteState>,
    /// Live `play_animation` overrides per instance id, in dispatch order.
    /// An override wins over the route's own cue until a reset or another
    /// override replaces it.
    animation_overrides: Vec<(String, PoseCue)>,
    /// The per-frame entity handoff to the renderer, rebuilt by every
    /// [`Game::update_entities`] pass.
    entity_frames: Vec<EntityFrame>,
    /// Label visibility per interactable index; parallel to
    /// [`Game::interactables`]. Reset on level load, preserved by
    /// `reset_to_start`.
    label_visible: Vec<bool>,
    /// The authored spawn this run resets to: `reset_to_start`, and the point
    /// the trigger sweep is re-seeded from after a teleport.
    spawn_position: Vec3,
    /// The authored spawn yaw, in radians.
    spawn_yaw: f32,
    /// How many times this run has returned the player to the spawn.
    reset_count: u64,
    /// Vertical simulation time not yet consumed by a fixed
    /// [`VERTICAL_SUBSTEP`].
    vertical_accumulator: f32,
    /// Phase of the idle bob at the water's float line, in radians.
    bob_phase: f32,
}

/// What one fixed vertical substep resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalStep {
    /// Still in the air; keep integrating.
    Airborne,
    /// Landed on the support; the rest of the frame's time is on the floor.
    Landed,
    /// The head hit the ceiling and the upward velocity is spent: surface the
    /// zeroed velocity to the frame boundary and let the fall resume next
    /// frame.
    Bumped,
}

/// How one frame's horizontal move treats floors and the collision band.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HorizontalMode {
    /// Feet on the walkable floor: rises stay bounded by the step rule, drops
    /// of any size are walked off and become a fall.
    Walk,
    /// Off the floor: a floor above the live feet refuses the step, while the
    /// void and any lower floor are crossed (so a jump can leave a ledge).
    Airborne,
    /// Floating: wall collision uses the band one step below the surface and
    /// any floor at or below the surface is reachable.
    Swim { surface_y: f32 },
}

/// What one horizontal sub-step resolved to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StepOutcome {
    /// The step is accepted onto `floor`; `dropped` marks a fall larger than a
    /// walkable step, which loses support at the end of the sweep.
    Accepted { floor: f32, dropped: bool },
    /// The step is refused.
    Refused,
    /// The step is accepted with no floor under it (the historical void).
    Void,
}

impl Game {
    #[must_use]
    pub fn new(spawn_pos: Vec3, spawn_yaw: f32, world: CollisionWorld) -> Self {
        let grounded = world
            .floor
            .walk_height_at(spawn_pos.x, spawn_pos.z)
            .is_some();
        let mut game = Self {
            running: true,
            app_state: AppState::MainMenu,
            last_frame_time: Instant::now(),
            delta_seconds: 0.0,
            sim_delta_seconds: 0.0,
            frame_count: 0,
            player_floor_y: spawn_pos.y - EYE_HEIGHT,
            player_position: spawn_pos,
            player_yaw: spawn_yaw.rem_euclid(TWO_PI),
            player_pitch: 0.0,
            walls: world.walls,
            floor: world.floor,
            ceiling: world.ceiling,
            water: world.water,
            ladders: world.ladders,
            triggers: world.triggers,
            interactables: world.interactables,
            vertical_velocity: 0.0,
            grounded,
            locomotion: LocomotionSnapshot::default(),
            swimming: false,
            stance: Stance::Standing,
            climbing: None,
            jump_latched: false,
            crouch_latched: false,
            interact_latched: false,
            interact_pressed: false,
            trigger_states: Vec::new(),
            routes: world.routes,
            route_states: Vec::new(),
            animation_overrides: Vec::new(),
            entity_frames: Vec::new(),
            label_visible: Vec::new(),
            spawn_position: spawn_pos,
            spawn_yaw: spawn_yaw.rem_euclid(TWO_PI),
            reset_count: 0,
            vertical_accumulator: 0.0,
            bob_phase: 0.0,
        };
        game.label_visible = vec![false; game.interactables.len()];
        game.seed_trigger_states(spawn_pos);
        game.seed_entity_state();
        game
    }

    /// The level-derived collision world currently in force.
    #[must_use]
    pub fn collision_world(&self) -> CollisionWorld {
        CollisionWorld {
            walls: self.walls.clone(),
            floor: self.floor.clone(),
            water: self.water.clone(),
            ceiling: self.ceiling.clone(),
            ladders: self.ladders.clone(),
            triggers: self.triggers.clone(),
            interactables: self.interactables.clone(),
            routes: self.routes.clone(),
        }
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    pub const fn stop(&mut self) {
        self.running = false;
    }

    #[must_use]
    pub const fn app_state(&self) -> AppState {
        self.app_state
    }

    pub fn set_app_state(&mut self, new_state: AppState) {
        // If resuming to Playing, reset timing to prevent a delta-time jump
        if new_state == AppState::Playing && self.app_state != AppState::Playing {
            self.last_frame_time = Instant::now();
            self.delta_seconds = 0.0;
            self.sim_delta_seconds = 0.0;
        }
        self.app_state = new_state;
    }

    /// Resets player position, orientation, collision walls, walkable floor,
    /// ceilings, water, ladders, area triggers and interactables when loading a
    /// level.
    ///
    /// The player spawns grounded when a walkable floor exists under the spawn
    /// point; outside every room a legacy spawn settles on the historical floor
    /// at its own height on the first update. Vertical velocity starts at zero,
    /// the jump and crouch latches are released, and the stance returns to
    /// standing. A fresh level also clears every label, re-seeds trigger states
    /// at the spawn and starts the run's reset counter at zero.
    pub fn reset_level(&mut self, spawn_pos: Vec3, spawn_yaw: f32, world: CollisionWorld) {
        self.walls = world.walls;
        self.floor = world.floor;
        self.water = world.water;
        self.ceiling = world.ceiling;
        self.ladders = world.ladders;
        self.triggers = world.triggers;
        self.interactables = world.interactables;
        self.routes = world.routes;
        self.spawn_position = spawn_pos;
        self.spawn_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.label_visible = vec![false; self.interactables.len()];
        self.reset_count = 0;
        self.clear_run_state(spawn_pos, self.spawn_yaw, false);
        self.seed_trigger_states(spawn_pos);
        self.seed_entity_state();
    }

    /// Returns the player to the level's authored spawn without rebuilding the
    /// collision world.
    ///
    /// This is the `reset_to_start` action: ground, water, ladder, stance and
    /// velocity state are reconciled to a clean standing spawn, trigger volumes
    /// are re-armed from the spawn position (so nothing between the old and new
    /// positions is swept), and keys held across the reset cannot fire again
    /// until released. Label visibility is preserved: it is a view toggle, not
    /// movement state.
    pub fn reset_to_spawn(&mut self) {
        self.clear_run_state(self.spawn_position, self.spawn_yaw, true);
        self.seed_trigger_states(self.spawn_position);
        // Routed entities return to their authored spawns and any
        // `play_animation` override is cleared, so a reset is a coherent
        // return to the authored start for the whole level. A
        // `toggle_animation` scrub is retargeted to its rest end instead of
        // dropped: a rigid prop has no locomotion state to fall back to.
        self.seed_entity_state();
        self.reset_count = self.reset_count.saturating_add(1);
    }

    /// How many times this run has returned the player to the spawn.
    #[must_use]
    pub const fn reset_count(&self) -> u64 {
        self.reset_count
    }

    /// The authored spawn this run resets to.
    #[must_use]
    pub const fn spawn_position(&self) -> Vec3 {
        self.spawn_position
    }

    /// Clears every piece of per-run player state and places the player at
    /// `spawn_pos` facing `spawn_yaw` with a level pitch.
    ///
    /// `suppress_held` latches Jump, Crouch and Interact so a key held across a
    /// teleport cannot fire on the first post-reset frame; each latch clears on
    /// release, so a fresh press works immediately. A level load passes
    /// `false` because the frame loop has already released gameplay inputs.
    fn clear_run_state(&mut self, spawn_pos: Vec3, spawn_yaw: f32, suppress_held: bool) {
        self.player_floor_y = spawn_pos.y - EYE_HEIGHT;
        self.player_position = spawn_pos;
        self.player_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.player_pitch = 0.0;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;
        self.grounded = self
            .floor
            .walk_height_at(spawn_pos.x, spawn_pos.z)
            .is_some();
        self.swimming = false;
        self.stance = Stance::Standing;
        self.climbing = None;
        self.jump_latched = suppress_held;
        self.crouch_latched = suppress_held;
        self.interact_latched = suppress_held;
        self.interact_pressed = false;
        self.bob_phase = 0.0;
        self.locomotion = LocomotionSnapshot::default();
        self.last_frame_time = Instant::now();
        self.delta_seconds = 0.0;
        self.sim_delta_seconds = 0.0;
    }

    /// Seeds every trigger's runtime state from the player's feet at
    /// `spawn_pos`.
    ///
    /// A trigger whose volume already contains the spawn starts *inside* it, so
    /// the standard enter semantics apply: the player must leave and re-enter
    /// before it fires. That is what stops a reset to a spawn inside a volume
    /// from looping immediately.
    fn seed_trigger_states(&mut self, spawn_pos: Vec3) {
        let feet_y = spawn_pos.y - EYE_HEIGHT;
        self.trigger_states.clear();
        self.trigger_states.reserve(self.triggers.len());
        for trigger in self.triggers.triggers() {
            self.trigger_states.push(TriggerState {
                inside: trigger.contains(spawn_pos.x, spawn_pos.z, feet_y),
                cooldown_remaining: 0.0,
                fired_once: false,
                pending: false,
            });
        }
    }

    /// Seeds every route's runtime state at its authored spawn, clears any
    /// `play_animation` overrides and republishes each routed instance's live
    /// anchor and bounds from its spawn (so a reset restores the authored
    /// values rather than keeping a stale live offset).
    fn seed_entity_state(&mut self) {
        self.route_states.clear();
        self.route_states.reserve(self.routes.len());
        for route in self.routes.routes() {
            self.route_states.push(route.new_state());
        }
        // A `play_animation` override is dropped so a reset is a coherent
        // return to the authored start. A `toggle_animation` scrub is kept and
        // re-targeted to its rest end: a rigid prop holds its last pose with
        // no cue, so clearing it would leave a switch where its last press
        // put it instead of at the authored start.
        self.animation_overrides.retain_mut(|(_, cue)| {
            if let PoseCue::Scrub { target, .. } = cue {
                *target = 0.0;
                true
            } else {
                false
            }
        });
        self.sync_routed_interactables();
        self.rebuild_entity_frames();
    }

    /// Rebuilds the per-frame entity handoff from the current route state.
    ///
    /// An interaction override wins over the route's own cue; an override that
    /// names a routed instance is folded into that instance's frame, and one
    /// that names a non-routed placed entity gets a cue-only frame.
    fn rebuild_entity_frames(&mut self) {
        self.entity_frames.clear();
        for (route, state) in self.routes.routes().iter().zip(self.route_states.iter()) {
            let cue = self
                .animation_overrides
                .iter()
                .rev()
                .find(|(id, _)| id == &route.instance_id)
                .map_or_else(|| state.cue.clone(), |(_, cue)| cue.clone());
            self.entity_frames.push(EntityFrame {
                instance_id: route.instance_id.clone(),
                transform: Some((state.position, state.yaw)),
                cue,
            });
        }
        for (id, cue) in &self.animation_overrides {
            if self.routes.get(id).is_none() {
                self.entity_frames.push(EntityFrame {
                    instance_id: id.clone(),
                    transform: None,
                    cue: cue.clone(),
                });
            }
        }
    }

    /// Advances every route and rebuilds the renderer handoff.
    ///
    /// Runs once per Playing frame, after the player's movement and triggers,
    /// so an interaction or trigger that starts an animation takes effect on
    /// the same frame's character pass.
    fn update_entities(&mut self) {
        if self.routes.is_empty() && self.animation_overrides.is_empty() {
            return;
        }
        let delta = self.sim_delta_seconds;
        let world = RouteWorld {
            walls: &self.walls,
            floor: &self.floor,
        };
        for (route, state) in self
            .routes
            .routes()
            .iter()
            .zip(self.route_states.iter_mut())
        {
            route.advance(state, delta, &world);
        }
        self.sync_routed_interactables();
        self.rebuild_entity_frames();
    }

    /// Republishes each routed entity's live anchor and bounds, so aiming and
    /// floating labels follow a character that walked away from its spawn (and
    /// turn with it).
    ///
    /// Values are fully derived from the live route state and the instance's
    /// own size contract, so repeated frames never accumulate drift and a
    /// reset — which re-seeds the route state — always restores the authored
    /// values.
    fn sync_routed_interactables(&mut self) {
        for (route, state) in self.routes.routes().iter().zip(self.route_states.iter()) {
            let Some(index) = self.interactables.index_of(&route.instance_id) else {
                continue;
            };
            self.interactables
                .set_live_pose(index, state.position, state.yaw.to_degrees());
        }
    }

    /// The per-frame entity handoff for the character renderer.
    #[must_use]
    pub fn entity_frames(&self) -> &[EntityFrame] {
        &self.entity_frames
    }

    /// The authored entity routes resident for this level.
    #[must_use]
    pub const fn routes(&self) -> &EntityRoutes {
        &self.routes
    }

    /// One route's runtime state, for diagnostics and tests.
    #[must_use]
    pub fn route_state(&self, instance_id: &str) -> Option<&RouteState> {
        self.routes
            .index_of(instance_id)
            .and_then(|index| self.route_states.get(index))
    }

    /// The live animation override on `instance_id`, if one is set.
    #[must_use]
    pub fn animation_override(&self, instance_id: &str) -> Option<&PoseCue> {
        self.animation_overrides
            .iter()
            .rev()
            .find(|(id, _)| id == instance_id)
            .map(|(_, cue)| cue)
    }

    /// Handles Escape key in gameplay / pause states.
    pub fn handle_escape(&mut self) {
        match self.app_state {
            AppState::Playing | AppState::PauseSettings => {
                self.set_app_state(AppState::Paused);
            }
            AppState::Paused => {
                self.set_app_state(AppState::Playing);
            }
            AppState::LevelSelect | AppState::Settings => {
                self.set_app_state(AppState::MainMenu);
            }
            AppState::MainMenu => {}
        }
    }

    /// Updates loop timing and calculates delta time between frames.
    ///
    /// `delta_seconds` keeps the real elapsed time for FPS measurement, while
    /// `sim_delta_seconds` is clamped for gameplay simulation.
    pub fn update_timing(&mut self) {
        let now = Instant::now();
        self.delta_seconds = now.duration_since(self.last_frame_time).as_secs_f32();
        self.sim_delta_seconds = clamp_sim_delta(self.delta_seconds);
        self.last_frame_time = now;
        self.frame_count = self.frame_count.saturating_add(1);
    }

    #[must_use]
    pub const fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    /// Discards the accumulated frame time without advancing the simulation.
    /// Used when frames are skipped (e.g. a minimized window) so that resuming
    /// does not apply a huge delta-time step to movement or looking.
    pub fn reset_timing(&mut self) {
        self.last_frame_time = Instant::now();
        self.delta_seconds = 0.0;
        self.sim_delta_seconds = 0.0;
    }

    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// The current stance.
    #[must_use]
    pub const fn stance(&self) -> Stance {
        self.stance
    }

    /// True while the crouched stance is active.
    #[must_use]
    pub const fn is_crouched(&self) -> bool {
        matches!(self.stance, Stance::Crouched)
    }

    /// True while the player is attached to a ladder.
    #[must_use]
    pub const fn is_climbing(&self) -> bool {
        self.climbing.is_some()
    }

    /// Collision cylinder height for the current stance, in metres.
    #[must_use]
    pub const fn body_height(&self) -> f32 {
        self.stance.height()
    }

    /// Eye offset above the feet for the current stance, in metres.
    #[must_use]
    pub const fn eye_offset(&self) -> f32 {
        self.stance.eye_offset()
    }

    /// World Y of the feet for the current eye position and stance.
    #[must_use]
    pub fn feet_y(&self) -> f32 {
        self.player_position.y - self.eye_offset()
    }

    /// Updates first-person movement, look, jumping, crouching, interaction
    /// latching, ladder climbing, swimming and area triggers for one frame.
    ///
    /// Keyboard look stays delta-time scaled; relative mouse motion is applied
    /// in pixels (no delta-time factor) at [`Settings::mouse_sensitivity`].
    /// Vertical motion integrates in fixed [`VERTICAL_SUBSTEP`] steps;
    /// horizontal collision switches between the walking step rule (drops
    /// become falls), an airborne rule (no floor above the live feet, the void
    /// is crossable) and the swimming band. Ladder attachment runs before all
    /// of them and owns the frame while attached. Area triggers run last, on
    /// the swept segment from the frame's start feet to the resolved feet.
    pub fn update_player_movement(&mut self, input: &mut InputState, settings: &Settings) {
        // A press is consumed exactly once and never survives a non-Playing
        // frame: the interaction latch is re-derived below, and the frame loop
        // owns dispatch through [`Game::take_interact_press`].
        self.interact_pressed = false;

        // The frame's relative motion is always consumed, even while paused or
        // in a menu, so motion collected around a pause is never applied later.
        let motion = input.take_mouse_motion();

        // While paused or in menus, do not update player movement or looking
        if self.app_state != AppState::Playing {
            return;
        }

        let trigger_origin = self.update_playing_frame(input, settings, motion);
        self.update_triggers(trigger_origin);
        self.update_entities();
    }

    /// One Playing frame: look, stance, interact latch, movement,
    /// water/ladder resolution and vertical integration.
    ///
    /// Returns the feet position the frame's *movement* starts from: the swept
    /// origin for area triggers, captured after the stance edge. A stance
    /// change anchors the feet on land, but in water the eye stays while the
    /// feet move, so capturing before the stance edge could fabricate a
    /// vertical crossing of a trigger band with no player movement.
    fn update_playing_frame(
        &mut self,
        input: &InputState,
        settings: &Settings,
        motion: (f32, f32),
    ) -> Vec3 {
        let delta = self.sim_delta_seconds;
        self.update_look(input, settings, motion, delta);

        // Stance changes anchor the feet and are clearance-checked before the
        // frame's movement, so the body height used for collision is current.
        self.update_crouch(input);
        // The Interact latch is an edge exactly like Jump and Crouch: one press
        // is one interaction, and a held key never repeats.
        self.update_interact(input);
        let trigger_origin = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );

        // Planar horizontal movement (independent of pitch). The held
        // directions keep the accumulation order the movement keys have always
        // had (forward, back, left, right).
        let sin_yaw = self.player_yaw.sin();
        let cos_yaw = self.player_yaw.cos();
        let held_directions = [
            (Control::MoveForward, Vec3::new(sin_yaw, 0.0, -cos_yaw)),
            (Control::MoveBackward, Vec3::new(-sin_yaw, 0.0, cos_yaw)),
            (Control::StrafeLeft, Vec3::new(-cos_yaw, 0.0, -sin_yaw)),
            (Control::StrafeRight, Vec3::new(cos_yaw, 0.0, sin_yaw)),
        ];
        let move_dir: Vec3 = held_directions
            .iter()
            .filter(|(control, _)| input.is_held(*control))
            .map(|(_, direction)| *direction)
            .sum();

        // One press is one jump: the latch is set on the first frame Jump is
        // held and cleared only on release, so a held key never double-jumps
        // and a landing (or a stand-up out of water) while holding never
        // bounces.
        let jump_held = input.is_held(Control::Jump);
        let jump_pressed = jump_held && !self.jump_latched;
        if !jump_held {
            self.jump_latched = false;
        } else if jump_pressed {
            self.jump_latched = true;
        }

        // A ladder owns the frame while attached (or on the frame it attaches):
        // it resolves climb motion and the top landing itself.
        if self.update_ladder(move_dir, jump_pressed, delta, settings) {
            self.refresh_locomotion(0.0, delta);
            return trigger_origin;
        }

        // Deep water at the feet decides this frame's horizontal mode and
        // speed; the post-move sample decides the vertical behaviour.
        let feet = self.feet_y();
        let sample = self
            .water
            .sample(self.player_position.x, self.player_position.z, feet);
        let wet = self.is_deep_water(sample);

        let previous = Vec2::new(self.player_position.x, self.player_position.z);
        if move_dir.length_squared() > 0.0 {
            let mode = if wet {
                HorizontalMode::Swim {
                    surface_y: sample.map_or(feet, |s| s.surface_y),
                }
            } else if self.grounded {
                HorizontalMode::Walk
            } else {
                HorizontalMode::Airborne
            };
            let speed = if wet {
                settings.walk_speed * SWIM_SPEED_FACTOR
            } else {
                settings.walk_speed
            };
            self.move_horizontal(move_dir, speed, delta, mode);
        }
        let horizontal_distance =
            previous.distance(Vec2::new(self.player_position.x, self.player_position.z));

        // The water at the post-move position decides the vertical behaviour.
        let feet = self.feet_y();
        let sample = self
            .water
            .sample(self.player_position.x, self.player_position.z, feet);
        let swimming_now = self.is_deep_water(sample);

        if self.swimming && swimming_now {
            if let Some(sample) = sample {
                self.swim_vertical(sample, jump_held);
            }
        } else if self.swimming {
            // The water ended or became shallow under the player.
            self.leave_water(sample);
        } else if swimming_now {
            // Entering deep water: the swimmer keeps no carry-over velocity and
            // rises to the float line on the next hold.
            self.swimming = true;
            self.grounded = false;
            self.climbing = None;
            self.vertical_velocity = 0.0;
            self.vertical_accumulator = 0.0;
            if let Some(sample) = sample {
                self.swim_vertical(sample, jump_held);
            }
        } else {
            self.land_vertical(jump_pressed);
        }

        self.refresh_locomotion(horizontal_distance, delta);
        trigger_origin
    }

    /// The locomotion pose and horizontal speed the last Playing update
    /// resolved.
    #[must_use]
    pub const fn locomotion_snapshot(&self) -> LocomotionSnapshot {
        self.locomotion
    }

    /// True while the player is in deep water, at or below the surface.
    #[must_use]
    pub const fn is_swimming(&self) -> bool {
        matches!(
            self.locomotion.state,
            LocomotionState::Swimming | LocomotionState::SurfaceSwimming
        )
    }

    /// True while the eye is below the water surface under it.
    #[must_use]
    pub fn is_underwater(&self) -> bool {
        let eye = self.player_position.y;
        self.water
            .sample(self.player_position.x, self.player_position.z, eye)
            .is_some_and(|sample| eye < sample.surface_y)
    }

    /// True when a water sample puts the player into the swim state: a
    /// swimming volume deeper than [`WADE_DEPTH`] at the feet, with the eye at
    /// or below the surface's float band.
    ///
    /// The eye band is what lets a climber release a ladder above the waterline
    /// and jump clear instead of being dragged back down to the float line by
    /// the feet-deep sample, while a floating or sinking swimmer (eye inside
    /// the band) keeps the swim state.
    fn is_deep_water(&self, sample: Option<WaterSample>) -> bool {
        sample.is_some_and(|sample| {
            sample.swimming
                && sample.surface_y - self.feet_y() > WADE_DEPTH
                && self.player_position.y <= sample.surface_y + SWIM_BAND_MARGIN
        })
    }

    /// Applies keyboard and mouse camera rotation for one frame.
    fn update_look(
        &mut self,
        input: &InputState,
        settings: &Settings,
        motion: (f32, f32),
        delta: f32,
    ) {
        let (horizontal, vertical) = motion;
        let look_speed_h = settings.look_speed_h.to_radians();
        let look_speed_v = settings.look_speed_v.to_radians();

        // Horizontal camera turn (yaw)
        if input.is_held(Control::LookLeft) {
            self.player_yaw = look_speed_h.mul_add(-delta, self.player_yaw);
        }
        if input.is_held(Control::LookRight) {
            self.player_yaw = look_speed_h.mul_add(delta, self.player_yaw);
        }
        // Mouse yaw: `mouse_sensitivity` degrees per pixel. Pixel motion is
        // already frame-rate independent, so it is applied without `delta`.
        let mouse_yaw = (horizontal * settings.mouse_sensitivity).to_radians();
        self.player_yaw = (self.player_yaw + mouse_yaw).rem_euclid(TWO_PI);

        // Vertical camera look (pitch) with clamping to prevent camera
        // flipping. `invert_look` flips only the vertical direction; the
        // horizontal turn and every movement key are unaffected. Mouse motion
        // moves the pitch down for a downward motion, like LookDown, and
        // follows the inversion preference.
        let pitch_sign = if settings.invert_look { -1.0 } else { 1.0 };
        if input.is_held(Control::LookUp) {
            self.player_pitch = (look_speed_v * pitch_sign).mul_add(delta, self.player_pitch);
        }
        if input.is_held(Control::LookDown) {
            self.player_pitch = (look_speed_v * -pitch_sign).mul_add(delta, self.player_pitch);
        }
        let mouse_pitch = -(vertical * settings.mouse_sensitivity * pitch_sign).to_radians();
        self.player_pitch = (self.player_pitch + mouse_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Toggles the crouch request on the rising edge of the bound key.
    fn update_crouch(&mut self, input: &InputState) {
        let held = input.is_held(Control::Crouch);
        let pressed = held && !self.crouch_latched;
        if !held {
            self.crouch_latched = false;
        } else if pressed {
            self.crouch_latched = true;
        }
        if pressed {
            self.toggle_stance();
        }
    }

    /// Latches one interaction press on the rising edge of the bound key.
    ///
    /// Unlike Jump and Crouch this performs no action itself: it records that a
    /// press happened this frame, and the frame loop consumes it with
    /// [`Game::take_interact_press`] once movement and the scene transforms for
    /// the frame are final.
    const fn update_interact(&mut self, input: &InputState) {
        let held = input.is_held(Control::Interact);
        let pressed = held && !self.interact_latched;
        if !held {
            self.interact_latched = false;
        } else if pressed {
            self.interact_latched = true;
        }
        self.interact_pressed = pressed;
    }

    /// Consumes the frame's interaction press, if there was one.
    ///
    /// Exactly one caller per frame: the press is cleared so a target resolved
    /// on a later frame can never act on an old key edge.
    pub const fn take_interact_press(&mut self) -> bool {
        let pressed = self.interact_pressed;
        self.interact_pressed = false;
        pressed
    }

    /// The interactable instance the player is currently looking at, if any.
    ///
    /// Uses the actual eye position (stance-aware, so a crouched player aims
    /// from the crouched eye), each instance's authored reach, and the
    /// collision world as occluders: no target through a wall or behind a
    /// nearer obstruction.
    #[must_use]
    pub fn interaction_target(&self) -> Option<usize> {
        nearest_target(
            self.player_position,
            self.view_direction(),
            self.interactables.items(),
            &self.walls,
        )
    }

    /// The current eye direction, matching the render camera.
    #[must_use]
    pub fn view_direction(&self) -> Vec3 {
        crate::interact::view_direction(self.player_yaw, self.player_pitch)
    }

    /// Runs the interaction on the currently aimed-at instance, if any.
    ///
    /// Returns `None` when nothing is in reach; otherwise the batch report for
    /// the instance's types actions, with the instance itself as the implicit
    /// `toggle_label` target.
    pub fn dispatch_interaction(&mut self) -> Option<DispatchReport> {
        let index = self.interaction_target()?;
        let actions = self.interactables.get(index)?.actions.clone();
        Some(self.dispatch_actions(&actions, Some(index)))
    }

    /// Runs one ordered batch of map-authored actions through the single
    /// dispatcher.
    ///
    /// The batch is bounded by [`crate::level::MAX_ACTIONS_PER_SOURCE`], and a
    /// `reset_to_start` ends it: the remaining actions do not run, because the
    /// player is now at the spawn and later effects must wait for a later
    /// dispatch rather than execute against a teleported player. Actions the
    /// engine does not implement are counted as `unsupported` instead of
    /// failing silently (validation rejects them for loaded maps; direct
    /// construction stays safe).
    pub fn dispatch_actions(
        &mut self,
        actions: &[ActionDef],
        actor: Option<usize>,
    ) -> DispatchReport {
        let mut report = DispatchReport::default();
        let mut frames_dirty = false;
        for action in actions.iter().take(crate::level::MAX_ACTIONS_PER_SOURCE) {
            match action {
                ActionDef::ToggleLabel { target } => {
                    // An explicit target must resolve on its own: only an
                    // omitted target means the acting instance. Falling back
                    // on a failed explicit target would toggle the wrong
                    // instance (or silently no-op from a trigger).
                    let resolved = match target.as_deref() {
                        Some(id) => self.interactables.index_of(id.trim()),
                        None => actor,
                    };
                    let Some(index) = resolved else {
                        report.missing_targets = report.missing_targets.saturating_add(1);
                        continue;
                    };
                    let Some(visible) = self.toggle_label(index) else {
                        report.missing_targets = report.missing_targets.saturating_add(1);
                        continue;
                    };
                    report.actions_run = report.actions_run.saturating_add(1);
                    if visible {
                        report.labels_shown = report.labels_shown.saturating_add(1);
                    } else {
                        report.labels_hidden = report.labels_hidden.saturating_add(1);
                    }
                }
                ActionDef::ResetToStart => {
                    self.reset_to_spawn();
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.player_reset = true;
                    // Safe ordering: the remainder of the batch is dropped
                    // rather than run against the teleported player.
                    return report;
                }
                ActionDef::PlayAnimation {
                    target,
                    clip,
                    looped,
                } => {
                    // Same target rule as `toggle_label`: an omitted target is
                    // the acting instance, an explicit target must resolve on
                    // its own.
                    let resolved = match target.as_deref() {
                        Some(id) => self.interactables.index_of(id.trim()),
                        None => actor,
                    };
                    let Some(index) = resolved else {
                        report.missing_targets = report.missing_targets.saturating_add(1);
                        continue;
                    };
                    let Some(instance_id) =
                        self.interactables.get(index).map(|item| item.id.clone())
                    else {
                        report.missing_targets = report.missing_targets.saturating_add(1);
                        continue;
                    };
                    let Some(name) = clip
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                    else {
                        report.unsupported = report.unsupported.saturating_add(1);
                        continue;
                    };
                    let cue = PoseCue::Clip {
                        name: name.to_string(),
                        once: !looped,
                        paused: false,
                    };
                    if let Some(entry) = self
                        .animation_overrides
                        .iter_mut()
                        .find(|(id, _)| id == &instance_id)
                    {
                        entry.1 = cue;
                    } else {
                        self.animation_overrides.push((instance_id, cue));
                    }
                    report.actions_run = report.actions_run.saturating_add(1);
                    report.animations_started = report.animations_started.saturating_add(1);
                    frames_dirty = true;
                }
                ActionDef::ToggleAnimation { target, clip } => {
                    if self.stage_toggle_animation(
                        target.as_deref(),
                        clip.as_deref(),
                        actor,
                        &mut report,
                    ) {
                        frames_dirty = true;
                    }
                }
                ActionDef::PlayAudio { .. } => {
                    report.unsupported = report.unsupported.saturating_add(1);
                }
            }
        }
        if frames_dirty {
            self.rebuild_entity_frames();
        }
        report
    }

    /// Stages one `toggle_animation` action and returns whether it cued an
    /// instance (so the caller rebuilds the entity frames).
    ///
    /// Same target rule as `toggle_label`: an omitted target is the acting
    /// instance, an explicit target must resolve on its own. The scrub
    /// direction flips from wherever the instance is now aiming, so a press
    /// mid-movement reverses instead of restarting; the first press on an
    /// instance moves to the far end. State stays on the instance's own
    /// override entry, so two switches never share a target.
    fn stage_toggle_animation(
        &mut self,
        target: Option<&str>,
        clip: Option<&str>,
        actor: Option<usize>,
        report: &mut DispatchReport,
    ) -> bool {
        let resolved = match target {
            Some(id) => self.interactables.index_of(id.trim()),
            None => actor,
        };
        let Some(index) = resolved else {
            report.missing_targets = report.missing_targets.saturating_add(1);
            return false;
        };
        let Some(instance_id) = self.interactables.get(index).map(|item| item.id.clone()) else {
            report.missing_targets = report.missing_targets.saturating_add(1);
            return false;
        };
        let Some(name) = clip.map(str::trim).filter(|name| !name.is_empty()) else {
            report.unsupported = report.unsupported.saturating_add(1);
            return false;
        };
        let current = self
            .animation_overrides
            .iter()
            .rev()
            .find(|(id, _)| id == &instance_id)
            .and_then(|(_, cue)| match cue {
                PoseCue::Scrub { target, .. } => Some(*target),
                PoseCue::Idle | PoseCue::Walk { .. } | PoseCue::Clip { .. } => None,
            });
        let next = match current {
            Some(target) if target >= 0.5 => 0.0,
            Some(_) | None => 1.0,
        };
        let cue = PoseCue::Scrub {
            name: name.to_string(),
            target: next,
        };
        if let Some(entry) = self
            .animation_overrides
            .iter_mut()
            .rev()
            .find(|(id, _)| id == &instance_id)
        {
            entry.1 = cue;
        } else {
            self.animation_overrides.push((instance_id, cue));
        }
        report.actions_run = report.actions_run.saturating_add(1);
        report.animations_started = report.animations_started.saturating_add(1);
        true
    }

    /// Flips one instance's label, returning the new visibility.
    ///
    /// State is indexed by placed instance, so two copies of the same model
    /// toggle independently.
    fn toggle_label(&mut self, index: usize) -> Option<bool> {
        let visible = self.label_visible.get_mut(index)?;
        *visible = !*visible;
        Some(*visible)
    }

    /// True when `index`'s floating label is currently shown.
    #[must_use]
    pub fn is_label_visible(&self, index: usize) -> bool {
        self.label_visible.get(index).copied().unwrap_or(false)
    }

    /// The interactable instances resident for this level.
    #[must_use]
    pub const fn interactables(&self) -> &Interactables {
        &self.interactables
    }

    /// The area triggers resident for this level, in authored order.
    #[must_use]
    pub const fn triggers(&self) -> &AreaTriggers {
        &self.triggers
    }

    /// The collision world's boxes, for presentation-side occlusion tests.
    #[must_use]
    pub fn walls(&self) -> &[WallAabb] {
        &self.walls
    }

    /// Runs this frame's area triggers from the feet position at the frame's
    /// start to the resolved feet position now.
    ///
    /// Each trigger fires once per entry: the player must leave the volume to
    /// re-arm, `cooldown_seconds` bounds repeats and `once` latches per run.
    /// The swept segment catches a fast fall through a thin band. At most one
    /// trigger batch runs per frame, so a reset can never chain into a second
    /// volume in the same frame; any other trigger whose entry (or swept
    /// crossing) happened this frame is marked pending and dispatched on a
    /// later frame, so a one-frame crossing of a later volume is deferred
    /// rather than lost.
    fn update_triggers(&mut self, origin: Vec3) {
        if self.triggers.is_empty() {
            return;
        }
        let delta = self.sim_delta_seconds;
        let to = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        for state in &mut self.trigger_states {
            state.cooldown_remaining = (state.cooldown_remaining - delta).max(0.0);
        }
        let mut dispatched = false;
        for index in 0..self.triggers.len() {
            let Some(trigger) = self.triggers.get(index) else {
                continue;
            };
            let inside = trigger.contains(to.x, to.z, to.y);
            let (bottom_y, top_y) = trigger.y_bounds();
            let swept = segment_overlaps_aabb(
                origin,
                to,
                [trigger.x0, bottom_y, trigger.z0],
                [trigger.x1, top_y, trigger.z1],
            );
            let cooldown_seconds = trigger.cooldown_seconds;
            let once = trigger.once;
            // Copy the previous state so the immutable borrow ends before the
            // dispatch; `TriggerState` is `Copy`.
            let Some(previous) = self.trigger_states.get(index).copied() else {
                continue;
            };
            let entered = !previous.inside && (inside || swept);
            let ready = previous.cooldown_remaining <= 0.0 && !(once && previous.fired_once);
            let will_fire = !dispatched && ready && (entered || previous.pending);
            // The batch is cloned only on the frames it actually runs.
            let actions = if will_fire {
                trigger.actions.clone()
            } else {
                Vec::new()
            };
            let fired = {
                let Some(state) = self.trigger_states.get_mut(index) else {
                    continue;
                };
                state.inside = inside;
                if entered {
                    // The entry is remembered: if another trigger already ran
                    // this frame, this one dispatches on the next one.
                    state.pending = true;
                }
                if will_fire {
                    state.pending = false;
                    state.cooldown_remaining = cooldown_seconds;
                    state.fired_once = true;
                    true
                } else {
                    false
                }
            };
            if fired {
                dispatched = true;
                let report = self.dispatch_actions(&actions, None);
                if report.player_reset {
                    // The reset re-seeded every trigger state (clearing pending
                    // flags) from the spawn; nothing else may run this frame.
                    return;
                }
            }
        }
    }

    /// Crouches, or stands back up when there is headroom.
    ///
    /// A stance change anchors the feet by shifting the eye by the offset
    /// difference, so the body never gains or loses height for free. Standing
    /// is refused (and the player remains crouched) when the current stance's
    /// head would meet a ceiling, a frame or a prop underside.
    fn toggle_stance(&mut self) {
        let target = self.stance.next();
        if target == Stance::Standing && !self.head_clear_for(self.feet_y(), PLAYER_HEIGHT) {
            return;
        }
        let previous = self.stance.eye_offset();
        self.stance = target;
        if !self.swimming {
            self.player_position.y += self.stance.eye_offset() - previous;
        }
    }

    /// The lowest overhead limit above `feet`: the room ceiling or a box
    /// underside (a door header, a window frame, a prop), whichever is lower.
    fn head_limit(&self, feet: f32) -> Option<f32> {
        let ceiling = self
            .ceiling
            .ceiling_y_at(self.player_position.x, self.player_position.z);
        let underside = lowest_underside(
            self.player_position.x,
            self.player_position.z,
            PLAYER_RADIUS,
            feet,
            &self.walls,
        );
        match (ceiling, underside) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// The deepest water the current stance can stand up in, in metres.
    ///
    /// The standing value is [`EXIT_DEPTH`]; the crouched stance scales it with
    /// its lower eye, so a crouched swimmer in chest-deep water stands by
    /// standing up first.
    fn stand_depth_limit(&self) -> f32 {
        match self.stance {
            Stance::Standing => EXIT_DEPTH,
            Stance::Crouched => CROUCH_EYE_HEIGHT - SWIM_BAND_MARGIN,
        }
    }

    /// True when a body of `height` whose feet stand at `feet` fits under the
    /// local ceiling and overhead boxes.
    fn head_clear_for(&self, feet: f32, height: f32) -> bool {
        self.head_limit(feet)
            .is_none_or(|limit| limit >= feet + height - STEP_EPS)
    }

    /// The highest support at `(x, z)` whose top is not above `max_top`: the
    /// rendered walkable floor, a solid prop or architecture top, or the
    /// historical world floor outside every room.
    ///
    /// Landing resolves against the *rendered* floor (`height_at`), not the
    /// staircase pitch line: a falling player lands on the tread underfoot.
    /// The pitch line is a grounded walking surface, applied only while the
    /// player is actually walking, so an airborne player is never pulled up
    /// onto a staircase.
    ///
    /// The world-floor fallback only applies at or above Y 0 and exists for
    /// legacy levels whose spawn sits outside every room. It is never the
    /// player's last floor: a fall under an open hole keeps falling past the
    /// hole's rim instead of stopping on an invisible plane.
    fn support_at(&self, x: f32, z: f32, max_top: f32) -> Option<f32> {
        // A floor above the reference is not a support: the feet would be
        // snapping *up* through it. A step-height allowance lets a fall land
        // on a step edge it is already within one step of.
        let floor = self
            .floor
            .height_at(x, z)
            .filter(|floor| *floor <= max_top + PLAYER_STEP_HEIGHT + STEP_EPS);
        let top = highest_support_top(x, z, max_top, &self.walls);
        match (floor, top) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => {
                if max_top >= -STEP_EPS {
                    Some(0.0)
                } else {
                    None
                }
            }
        }
    }

    /// Moves the player horizontally with the sub-stepped collision rule of the
    /// given mode, updating `player_floor_y` and (while walking) the eye line.
    ///
    /// Walking keeps the historical step rule for *rises*: a rise larger than
    /// [`PLAYER_STEP_HEIGHT`] is refused, so a cliff, a wall and a tall
    /// obstacle stay impassable. A *drop* of any size is walked off instead of
    /// refused: the player crosses the boundary, loses support and falls from
    /// the ledge. The height applied while still supported is the walking
    /// surface: identical to the rendered floor on ramps, regions and room
    /// floors, but the line through a staircase's nosings rather than the
    /// individual treads.
    ///
    /// The step rule runs *per sub-step* (each at most half a player radius,
    /// 0.15 m): at the loader's maximum ramp slope a sub-step rises at most
    /// 0.3 m, and the loader bounds a staircase's riser by `PLAYER_STEP_HEIGHT`
    /// and its tread by `MIN_STAIR_TREAD_M`, so every legal slope and flight is
    /// climbable at any frame rate.
    fn move_horizontal(&mut self, move_dir: Vec3, speed: f32, delta: f32, mode: HorizontalMode) {
        // Component-wise, with the original `(n * walk_speed) * delta`
        // association so the walking movement stays bit-for-bit identical.
        let total_delta = Vec3::from_array(
            move_dir
                .normalize()
                .to_array()
                .map(|component| component * speed * delta),
        );
        let total_dist = total_delta.length();
        let max_step = PLAYER_RADIUS * 0.5;
        // Clamped up to at least one sub-step (as the historical `max(1)` did)
        // and down to a million: the ceiling is far above anything
        // `MAX_SIM_DELTA` can produce, and it keeps the count exactly
        // representable so the cast below cannot truncate.
        let step_count = (total_dist / max_step).ceil().clamp(1.0, 1_048_576.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = step_count as usize;
        let step_delta = Vec3::from_array(
            total_delta
                .to_array()
                .map(|component| component / step_count),
        );
        let previous = Vec2::new(self.player_position.x, self.player_position.z);
        let mut current_pos = previous;
        let mut current_floor = self.player_floor_y;
        let mut on_a_floor = self.floor.walk_height_at(previous.x, previous.y).is_some();
        let mut lost_support = false;
        // The airborne wall band follows the live foot height, which does not
        // change during a horizontal sweep: vertical motion runs after it.
        let live_feet = self.player_position.y - self.eye_offset();
        let body_height = self.body_height();

        for _ in 0..steps {
            let foot_y = match mode {
                HorizontalMode::Walk => current_floor,
                HorizontalMode::Airborne => live_feet,
                HorizontalMode::Swim { surface_y } => surface_y - PLAYER_STEP_HEIGHT,
            };
            let raw = Vec2::new(current_pos.x + step_delta.x, current_pos.y + step_delta.z);
            let candidate = resolve_player_collision_for_body(
                raw,
                PLAYER_RADIUS,
                foot_y,
                body_height,
                &self.walls,
            );
            // A depenetration deeper than the body radius is a teleport (the
            // centre was inside a box, or several boxes pushed at once): refuse
            // the step instead of snapping the player across the opening.
            if candidate.distance(raw) > PLAYER_RADIUS + CONTACT_EPS {
                break;
            }
            let outcome = match mode {
                HorizontalMode::Walk => {
                    self.walk_step(current_pos, current_floor, candidate, on_a_floor)
                }
                HorizontalMode::Airborne => self.airborne_step(candidate, live_feet),
                HorizontalMode::Swim { surface_y } => self.swim_step(candidate, surface_y),
            };
            match outcome {
                StepOutcome::Accepted { floor, dropped } => {
                    current_pos = candidate;
                    current_floor = floor;
                    on_a_floor = true;
                    lost_support = lost_support || dropped;
                }
                StepOutcome::Refused => break,
                StepOutcome::Void => current_pos = candidate,
            }
        }
        self.player_floor_y = current_floor;
        self.player_position.x = current_pos.x;
        self.player_position.z = current_pos.y;
        if matches!(mode, HorizontalMode::Walk) {
            if lost_support {
                // The feet left the floor this frame: fall from the ledge at
                // the eye line they had, never snap down to the floor below.
                self.grounded = false;
                self.vertical_velocity = 0.0;
                self.vertical_accumulator = 0.0;
            } else {
                // Maintain grounded eye height regardless of pitch.
                self.player_position.y = self.player_floor_y + self.eye_offset();
            }
        }
    }

    /// Resolves one walking sub-step: rises stay bounded by the step rule, a
    /// drop of any size is accepted and reported as lost support.
    fn walk_step(
        &self,
        from: Vec2,
        current_floor: f32,
        candidate: Vec2,
        on_a_floor: bool,
    ) -> StepOutcome {
        let current_rendered = self
            .floor
            .height_at(from.x, from.y)
            .unwrap_or(current_floor);
        match self.floor.height_at(candidate.x, candidate.y) {
            Some(y) if y <= current_rendered + PLAYER_STEP_HEIGHT + STEP_EPS => {
                // Stand on the walking surface. It equals the rendered floor
                // everywhere except on a staircase, where it is within one
                // riser of it by construction.
                let floor = self
                    .floor
                    .walk_height_at(candidate.x, candidate.y)
                    .unwrap_or(y);
                StepOutcome::Accepted {
                    floor,
                    dropped: current_rendered - y > PLAYER_STEP_HEIGHT + STEP_EPS,
                }
            }
            Some(_) => StepOutcome::Refused,
            None => {
                // Outside every room: keep the historical freedom to walk over
                // the void, but never step off a real floor into it.
                if on_a_floor {
                    StepOutcome::Refused
                } else {
                    StepOutcome::Void
                }
            }
        }
    }

    /// Resolves one airborne sub-step: a floor above the live feet refuses the
    /// step, the void and any lower floor are crossed.
    fn airborne_step(&self, candidate: Vec2, live_feet: f32) -> StepOutcome {
        match self.floor.height_at(candidate.x, candidate.y) {
            Some(y) if y <= live_feet + STEP_EPS => StepOutcome::Accepted {
                floor: self
                    .floor
                    .walk_height_at(candidate.x, candidate.y)
                    .unwrap_or(y),
                dropped: false,
            },
            Some(_) => StepOutcome::Refused,
            None => StepOutcome::Void,
        }
    }

    /// Resolves one swimming sub-step: any floor at or below the surface is
    /// reachable, a floor above it is a ledge the swimmer cannot cross, and the
    /// void is open water.
    fn swim_step(&self, candidate: Vec2, surface_y: f32) -> StepOutcome {
        match self.floor.height_at(candidate.x, candidate.y) {
            Some(y) if y <= surface_y + STEP_EPS => StepOutcome::Accepted {
                floor: self
                    .floor
                    .walk_height_at(candidate.x, candidate.y)
                    .unwrap_or(y),
                dropped: false,
            },
            Some(_) => StepOutcome::Refused,
            None => StepOutcome::Void,
        }
    }

    /// Vertical step for a grounded or airborne player: the grounded snap, a
    /// launch on a fresh Jump press, and fixed-substep gravity integration
    /// while airborne.
    fn land_vertical(&mut self, jump_pressed: bool) {
        if self.grounded {
            self.player_position.y = self.player_floor_y + self.eye_offset();
            self.vertical_velocity = 0.0;
            self.vertical_accumulator = 0.0;
            if !jump_pressed {
                return;
            }
            self.vertical_velocity = JUMP_VELOCITY;
            self.grounded = false;
        }

        // Accumulate this frame's time and consume it at most
        // `MAX_VERTICAL_SUBSTEPS` times: at most twelve substeps fit in
        // `MAX_SIM_DELTA`, so a normal frame consumes all available time and
        // the leftover is always below one substep.
        self.vertical_accumulator =
            (self.vertical_accumulator + self.sim_delta_seconds).min(MAX_SIM_DELTA);
        let mut steps = 0_usize;
        while self.vertical_accumulator >= VERTICAL_SUBSTEP && steps < MAX_VERTICAL_SUBSTEPS {
            self.vertical_accumulator -= VERTICAL_SUBSTEP;
            steps = steps.saturating_add(1);
            match self.integrate_vertical_substep() {
                VerticalStep::Airborne => {}
                VerticalStep::Landed => {
                    // Landed: the rest of the accumulated time is spent on the
                    // floor.
                    self.vertical_accumulator = 0.0;
                    break;
                }
                VerticalStep::Bumped => break,
            }
        }
    }

    /// Advances the airborne player by exactly one fixed vertical substep.
    ///
    /// The position uses the average of the substep's start and end velocities
    /// (the trapezoidal form), which reproduces the exact ballistic parabola at
    /// every substep boundary: the apex is therefore frame-rate independent
    /// rather than depending on where a frame boundary lands.
    ///
    /// The head is clamped by the room ceiling and by every overhead box
    /// (headers, frames, prop undersides) above the feet, consuming the upward
    /// velocity. Landing is checked while descending against the highest
    /// support under the centre: the walkable floor or a solid prop top.
    ///
    /// Returns what the substep resolved to: still airborne, landed, or a
    /// ceiling bump.
    fn integrate_vertical_substep(&mut self) -> VerticalStep {
        let eye_offset = self.eye_offset();
        let height = self.body_height();
        let start_feet = self.player_position.y - eye_offset;
        let next_velocity = GRAVITY.mul_add(-VERTICAL_SUBSTEP, self.vertical_velocity);
        let rise = f32::midpoint(self.vertical_velocity, next_velocity) * VERTICAL_SUBSTEP;
        self.vertical_velocity = next_velocity;
        let mut eye = self.player_position.y + rise;
        let mut bumped = false;

        // Ceiling and overhead boxes: the top of the head is what bumps, and
        // the upward velocity is consumed by the impact instead of being
        // applied again. Clamping a hair under the limit keeps the horizontal
        // pass from re-reading the same box as a wall at the contact plane.
        if let Some(limit) = self.head_limit(start_feet)
            && eye > limit - (height - eye_offset) - CONTACT_EPS
        {
            eye = limit - (height - eye_offset) - CONTACT_EPS;
            if self.vertical_velocity > 0.0 {
                self.vertical_velocity = 0.0;
                bumped = true;
            }
        }
        self.player_position.y = eye;

        // Landing: only while descending, on the highest support under the
        // centre that was not above the feet at the substep's start. Outside
        // every room the historical world floor stands in at Y 0, so a legacy
        // off-room spawn never falls forever, but a hole in a real room has no
        // invisible floor.
        if self.vertical_velocity <= 0.0
            && let Some(support) =
                self.support_at(self.player_position.x, self.player_position.z, start_feet)
            && eye - eye_offset <= support + STEP_EPS
        {
            self.player_position.y = support + eye_offset;
            self.player_floor_y = support;
            self.vertical_velocity = 0.0;
            self.grounded = true;
            return VerticalStep::Landed;
        }
        if bumped {
            VerticalStep::Bumped
        } else {
            VerticalStep::Airborne
        }
    }

    /// Vertical step while in deep water: hold Jump to rise to the float line,
    /// release to sink under the reduced underwater gravity at the terminal
    /// sink speed, and stand up when the floor underfoot reaches exit depth.
    ///
    /// The swim pose uses [`SWIM_FLOOR_CLEARANCE`] and [`FLOAT_EYE_MARGIN`],
    /// buoyancy constants that are deliberately separate from the land eye
    /// offset; the stance only decides the body used for the head clamp and
    /// the height the eyes sit at once the player stands up.
    fn swim_vertical(&mut self, sample: WaterSample, jump_held: bool) {
        let delta = self.sim_delta_seconds;
        let surface_y = sample.surface_y;
        let floor = self
            .floor
            .walk_height_at(self.player_position.x, self.player_position.z);
        let min_eye = floor.map_or(f32::NEG_INFINITY, |support| support + SWIM_FLOOR_CLEARANCE);

        if jump_held {
            // Rise directly to the line and hold there: no launch velocity is
            // accumulated, so the hold never pops the player out of the water.
            // The bob is a deterministic function of the advanced phase.
            self.bob_phase = SWIM_BOB_SPEED.mul_add(delta, self.bob_phase) % TWO_PI;
            let float_line = surface_y + FLOAT_EYE_MARGIN;
            let risen = SWIM_RISE_SPEED.mul_add(delta, self.player_position.y);
            self.player_position.y = if risen >= float_line {
                SWIM_BOB_AMPLITUDE.mul_add(self.bob_phase.sin(), float_line)
            } else {
                risen
            };
            self.vertical_velocity = 0.0;
        } else {
            let next = SWIM_GRAVITY
                .mul_add(delta, self.vertical_velocity)
                .max(-SWIM_SINK_TERMINAL);
            self.player_position.y = next.mul_add(delta, self.player_position.y);
            self.vertical_velocity = next;
        }

        // The body can rest on the pool floor but never sink through it.
        if self.player_position.y <= min_eye {
            self.player_position.y = min_eye;
            self.vertical_velocity = 0.0;
        }

        // The head may still bump a ceiling or a frame while rising in water.
        // The reference is the eye, not the swimmer's virtual feet: a floor
        // rim whose underside is below the eye is a wall beside the water, not
        // an overhead, and must not drag a surface swimmer down.
        if let Some(limit) = self.head_limit(self.player_position.y) {
            let head_offset = self.body_height() - self.eye_offset();
            let max_eye = limit - head_offset - CONTACT_EPS;
            if self.player_position.y > max_eye {
                self.player_position.y = max_eye;
                if self.vertical_velocity > 0.0 {
                    self.vertical_velocity = 0.0;
                }
            }
        }

        // Exit: the walkable floor is within standing depth of the surface,
        // the eye is near the top of the water and the stance's body fits
        // above the floor. The exited feet are shallow enough that
        // [`WADE_DEPTH`] cannot immediately re-enter swimming, so a pool edge
        // never oscillates.
        let can_stand = floor.is_some_and(|support| {
            support <= surface_y + STEP_EPS
                && surface_y - support <= self.stand_depth_limit()
                && self.player_position.y >= surface_y - EXIT_EYE_MARGIN
                && self.head_clear_for(support, self.body_height())
        });
        if let Some(support) = floor.filter(|_| can_stand) {
            self.player_floor_y = support;
            self.player_position.y = support + self.eye_offset();
            self.vertical_velocity = 0.0;
            self.grounded = true;
            self.swimming = false;
            self.bob_phase = 0.0;
        } else {
            self.player_floor_y = floor.unwrap_or(self.player_floor_y);
        }
    }

    /// Leaves the swimming state when the water under the player ends or
    /// becomes shallow.
    ///
    /// The swimmer's eye is not a standing eye height above anything: it is
    /// buoyed near the surface, and its virtual feet can sit below the pool
    /// floor. The exit therefore only re-derives the standing body when there
    /// is somewhere coherent to put it: the water is shallow enough to stand
    /// in, the eye is near the surface, and the stance's body fits under the
    /// local ceiling. When the body's virtual feet are inside the floor (a
    /// submerged volume boundary) it stands on the floor if the clearance
    /// allows; otherwise the water under it can no longer be swum and the
    /// player is simply airborne, falling from the eye line they had.
    fn leave_water(&mut self, sample: Option<WaterSample>) {
        self.swimming = false;
        self.bob_phase = 0.0;
        let floor = self
            .floor
            .walk_height_at(self.player_position.x, self.player_position.z);
        let surface = sample.map(|sample| sample.surface_y);
        let can_stand = floor.zip(surface).is_some_and(|(support, surface)| {
            support <= surface + STEP_EPS
                && surface - support <= self.stand_depth_limit()
                && self.player_position.y >= surface - EXIT_EYE_MARGIN
                && self.head_clear_for(support, self.body_height())
        });
        if let Some(support) = floor.filter(|_| can_stand) {
            self.player_floor_y = support;
            self.player_position.y = support + self.eye_offset();
            self.vertical_velocity = 0.0;
            self.grounded = true;
            return;
        }
        if let Some(support) = floor {
            self.player_floor_y = support;
            if self.player_position.y - self.eye_offset() < support - STEP_EPS {
                // The virtual feet are inside the floor. Leaving the water must
                // not leave the body embedded in it: stand on the floor when
                // the stance fits, otherwise stay swimming until the player
                // moves somewhere with headroom (never pushed through a box to
                // make standing possible).
                if self.head_clear_for(support, self.body_height()) {
                    self.player_position.y = support + self.eye_offset();
                    self.vertical_velocity = 0.0;
                    self.grounded = true;
                    return;
                }
                self.swimming = true;
                return;
            }
        }
        self.grounded = false;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;
    }

    /// Attaches to and climbs a ladder, or reports that the frame was not a
    /// climbing frame.
    ///
    /// Attachment needs all of: the player's disc over the ladder footprint,
    /// the body vertically over its authored reach, the player on the approach
    /// side (behind the facing direction) and movement input pointing along
    /// the climb direction. There is no climb key: walking into the face with
    /// movement intent is the input. While attached, holding the toward input
    /// climbs up, releasing holds position, backing away detaches, Jump
    /// detaches with a launch, and an overhead stops the rise. Horizontal
    /// movement stays collision-checked, so the deck is reached by rising to
    /// the ladder top and stepping onto the real floor.
    fn update_ladder(
        &mut self,
        move_dir: Vec3,
        jump_pressed: bool,
        delta: f32,
        settings: &Settings,
    ) -> bool {
        let (x, z) = (self.player_position.x, self.player_position.z);
        let height = self.body_height();
        if let Some(index) = self.climbing {
            let Some(ladder) = self.ladders.get(index).copied() else {
                self.climbing = None;
                return false;
            };
            if jump_pressed {
                // Jumping off the ladder: launch with the ordinary jump so the
                // rest of the vertical integration is the standard ballistic
                // one, and let normal physics own the frame.
                self.climbing = None;
                self.grounded = false;
                self.swimming = false;
                self.vertical_velocity = JUMP_VELOCITY;
                self.vertical_accumulator = 0.0;
                return false;
            }
            if !ladder.overlaps_disc(x, z, PLAYER_RADIUS)
                || !ladder.overlaps_body_y(self.player_position.y, height)
            {
                self.climbing = None;
                return false;
            }
            let along = normalized_climb_intent(move_dir, &ladder);
            if along < -LADDER_INTENT_THRESHOLD {
                // Backing away releases the ladder instead of climbing down.
                self.climbing = None;
                return false;
            }
            self.climb_step(ladder, move_dir, along, delta, settings);
            return true;
        }

        if move_dir.length_squared() <= 0.0 {
            return false;
        }
        let Some(index) = self.ladders.overlapping(x, z, PLAYER_RADIUS) else {
            return false;
        };
        let Some(ladder) = self.ladders.get(index).copied() else {
            return false;
        };
        if !ladder.approach_side(x, z) || !ladder.overlaps_body_y(self.player_position.y, height) {
            return false;
        }
        let along = normalized_climb_intent(move_dir, &ladder);
        if along <= LADDER_INTENT_THRESHOLD {
            return false;
        }
        self.climbing = Some(index);
        self.grounded = false;
        self.swimming = false;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;
        self.climb_step(ladder, move_dir, along, delta, settings);
        true
    }

    /// One attached climbing step: collision-checked horizontal movement plus
    /// the vertical climb, head clamp and safe top landing.
    fn climb_step(
        &mut self,
        ladder: Ladder,
        move_dir: Vec3,
        along: f32,
        delta: f32,
        settings: &Settings,
    ) {
        if move_dir.length_squared() > 0.0 {
            self.move_horizontal(
                move_dir,
                settings.walk_speed * LADDER_SIDE_SPEED_FACTOR,
                delta,
                HorizontalMode::Airborne,
            );
        }
        let eye_offset = self.eye_offset();
        let height = self.body_height();
        let feet = self.player_position.y - eye_offset;
        let mut target = if along > LADDER_INTENT_THRESHOLD {
            LADDER_CLIMB_SPEED.mul_add(delta, feet).min(ladder.top_y)
        } else {
            feet
        };
        if let Some(limit) = self.head_limit(feet) {
            // An obstruction stops the rise: the player stays attached at the
            // height they reached instead of clipping through the frame.
            target = target.min(limit - height - CONTACT_EPS);
        }
        // An obstruction or a clamp never pushes the climber down: holding the
        // current height is the worst case.
        self.player_position.y = target.min(ladder.top_y).max(feet) + eye_offset;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;

        // Safe top landing: once the feet reach the authored top, a real
        // walkable surface within a step of them takes over. This is a normal
        // grounded transition (the deck under the player), never a snap
        // through the rim: the feet are already at the top.
        if target >= ladder.top_y - STEP_EPS
            && let Some(support) = self
                .floor
                .walk_height_at(self.player_position.x, self.player_position.z)
            && (support - target).abs() <= PLAYER_STEP_HEIGHT + STEP_EPS
            && self.head_clear_for(support, height)
        {
            self.player_position.y = support + eye_offset;
            self.player_floor_y = support;
            self.grounded = true;
            self.swimming = false;
            self.climbing = None;
        }
    }

    /// Refreshes the locomotion pose and speed for this frame.
    fn refresh_locomotion(&mut self, horizontal_distance: f32, delta: f32) {
        let speed = if delta > 0.0 {
            horizontal_distance / delta
        } else {
            0.0
        };
        let state = if self.swimming {
            let eye = self.player_position.y;
            let surface = self
                .water
                .sample(
                    self.player_position.x,
                    self.player_position.z,
                    self.feet_y(),
                )
                .map_or(eye, |sample| sample.surface_y);
            if eye >= surface - SURFACE_SWIM_EYE_MARGIN {
                LocomotionState::SurfaceSwimming
            } else {
                LocomotionState::Swimming
            }
        } else if self.grounded {
            if speed > WALKING_SPEED_EPSILON_M_PER_S {
                LocomotionState::Walking
            } else {
                LocomotionState::Idle
            }
        } else {
            LocomotionState::Airborne
        };
        self.locomotion = LocomotionSnapshot { state, speed };
    }
}

/// The component of `move_dir` along a ladder's facing, with zero for no
/// input so a released key never counts as intent.
fn normalized_climb_intent(move_dir: Vec3, ladder: &Ladder) -> f32 {
    if move_dir.length_squared() <= 0.0 {
        return 0.0;
    }
    let normalized = move_dir.normalize();
    ladder.climb_intent(normalized.x, normalized.z)
}

#[cfg(test)]
mod tests;

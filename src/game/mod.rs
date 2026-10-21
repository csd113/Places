//! Places gameplay, player controls, menus and native application.

mod app;
pub mod bench;
pub mod console;
pub mod display;
pub mod gameplay_actions;
pub mod gameplay_runtime;
pub mod input;
pub mod perf_overlay;
pub mod pickup_catalog;
pub mod representation;
mod representation_camera;
pub mod settings;
pub mod ui;
pub mod world_labels;

pub use app::run;

use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::engine::collision::{
    CONTACT_EPS, CROUCH_HEIGHT, DoorCollider, PLAYER_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT,
    STEP_EPS, WallAabb, body_support_top, lowest_door_underside, lowest_underside_indexed,
};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext, WorldTick};
use crate::engine::entity::{EntityFrame, EntityRoutes, PoseCue, RouteState, RouteWorld};
use crate::engine::interact::{Interactables, nearest_target_indexed};
use crate::engine::level::GroundSurfaces;
use crate::engine::level::{
    ActionDef, Ladder, Ladders, LevelDef, LevelSurfaces, WalkableCeiling, WalkableFloor,
    WaterSample, WaterVolumes,
};
use crate::engine::materials::GroundSurface;
use crate::engine::scene::{LocomotionSnapshot, LocomotionState};
use crate::game::input::{Control, InputState};
use crate::game::settings::Settings;

mod developer;
mod recovery;
mod water;
use recovery::{PlayableSpace, RecoveryDestination};

pub const TWO_PI: f32 = std::f32::consts::TAU;
pub const EYE_HEIGHT: f32 = 1.6;

/// Eye offset above the feet while crouched: exactly half the standing offset,
/// matching the crouched body height being half the standing height.
pub const CROUCH_EYE_HEIGHT: f32 = EYE_HEIGHT * 0.5;

/// Seconds the animated eye offset takes to travel between the standing and
/// crouched offsets.
///
/// The transition rate is the full offset difference divided by this time, so
/// a crouch or stand always takes the same wall-clock time regardless of the
/// frame rate. The collision body still switches instantly (see
/// `Game::body_height`); only the camera eases.
pub const CROUCH_TRANSITION_SECONDS: f32 = 0.15;

pub const MAX_PITCH: f32 = 1.4835; // ~85 degrees in radians

/// Downward acceleration applied to the vertical velocity while airborne, in
/// m/s^2. One world unit is one metre, so this is the physical 9.8 m/s^2.
pub use crate::engine::physics::GRAVITY;

/// Height of a full jump's apex above the take-off floor, in metres.
///
/// Ordinary standable furniture in the demo is 0.9 m (the kitchen counter,
/// stove and sink deck, and the taller chairs and couch backs), and a standing
/// jump must clear the tallest of it with margin to land on it: the apex adds
/// [`JUMP_CLEARANCE_M`] rather than making the counter exactly unreachable.
/// This is not a stronger arbitrary jump: the office desk is still the
/// documented 0.75 m test target, and the sink's own collider fault is fixed
/// in the level data rather than by this height.
pub const JUMP_CLEARANCE_M: f32 = 0.10;

/// Height of the office desk top, in metres.
///
/// The desk remains the documented test target — it is the cleanest small
/// landing surface in the demo — even though the jump is now sized against the
/// taller 0.9 m kitchen counter ([`KITCHEN_COUNTER_TOP_M`]).
pub const OFFICE_DESK_TOP_M: f32 = 0.75;

/// Height of the kitchen counter top the jump is sized against, in metres.
///
/// The demo kitchen's counter, stove and sink deck all top out at 0.9 m above
/// their floor: the tallest ordinary standable furniture in the demo, so it is
/// the surface a standing jump must reach.
pub const KITCHEN_COUNTER_TOP_M: f32 = 0.9;

/// Height of a full jump's apex above the take-off floor, in metres: the
/// tallest ordinary standable furniture plus the shared clearance margin.
pub const JUMP_APEX_M: f32 = KITCHEN_COUNTER_TOP_M + JUMP_CLEARANCE_M;

/// Take-off speed of a jump, in m/s: the f32 value of
/// `sqrt(2.0 * GRAVITY * JUMP_APEX_M)` (the square root is not available in a
/// `const` initializer). A test re-derives it from the formula.
pub const JUMP_VELOCITY: f32 = 4.427_189;

/// Maximum land simulation interval and fixed water step, in seconds.
///
/// Land intervals are at most this long, with an exact ballistic update even
/// for fractional 144 Hz intervals. At most twelve of them fit in
/// [`MAX_SIM_DELTA`].
pub const VERTICAL_SUBSTEP: f32 = 1.0 / 120.0;

/// Ice approaches requested walk velocity in about half a second and coasts
/// roughly 1.8 m from ordinary speed. Rates are per second, not per frame.
const ICE_CONTROL_RESPONSE: f32 = 4.5;
const ICE_FRICTION: f32 = 1.6;
const ICE_AIR_CONTROL_RESPONSE: f32 = 1.2;
const ICE_STOP_SPEED: f32 = 0.025;

/// Water depth at the player's feet at or below which the player wades: the
/// historical walking controller, including jumping, applies unchanged.
pub const WADE_DEPTH: f32 = 0.55;

/// Eye-height band used only for the surface swimming animation.
const SURFACE_SWIM_EYE_MARGIN: f32 = 0.25;

/// How far below the water surface the eye may be and still stand up, in
/// metres.
const EXIT_EYE_MARGIN: f32 = 0.6;

/// How far above the water surface a floor may be and still be a bounded
/// water exit, in metres.
///
/// The swimming horizontal step may cross onto a real walkable floor whose
/// top is at most this far above the free surface, and the stand-up path
/// accepts it, so a pool deck slightly above the waterline is climbable.
/// Anything higher is a wall: this is not general climbing. Real walls and
/// solid props never carry this allowance (only floor rims do), so a solid
/// barrier at the water's edge still blocks.
pub const WATER_EXIT_STEP_M: f32 = 0.5;

/// Vertical climbing speed on a ladder, in m/s.
pub const LADDER_CLIMB_SPEED: f32 = 2.2;

/// How far past the player radius the pressed-exit probe reaches, in metres.
///
/// The body must actually touch a raised rim before the pull-up can begin;
/// the small overshoot makes the probe robust exactly at the boundary where
/// the disc grazes the rim face.
const EXIT_PRESS_PROBE_M: f32 = PLAYER_RADIUS + 0.05;

/// How far above a raised water-exit rim the rendered eye must clear before
/// the swim step may carry the body across it, in metres.
///
/// The projection's near plane is `SCENE_NEAR_M` (0.1 m, in
/// `src/render/common/mod.rs`): with the eye any closer to the rim top, the
/// camera clips into the deck surface the player is climbing onto. The
/// upright pull-up clears the feet over the rim. The swimming disc also
/// keeps its distance from any box whose vertical span is within this
/// clearance of the animated eye.
pub const WATER_EXIT_EYE_CLEARANCE_M: f32 = 0.1;

/// Margin around the physical body in which the pressed-exit probe keeps
/// recognising a rim, in metres. The rim remains pressed until the full disc
/// crosses the support, including diagonal contacts after the eye clears it.
const EXIT_PRESS_MARGIN_M: f32 = PLAYER_RADIUS;

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

/// The static, level-derived collision world the controller queries, plus the
/// level's whole entity runtime.
///
/// This is the reusable bundle [`Game::new`] and [`Game::reset_level`] take:
/// walls and solid props as axis-aligned boxes, the walkable floor and ceiling
/// samplers, the water volumes, the ladder volumes, and the [`EntityWorld`]
/// that owns every authored object, its components, events, timers, sequences
/// and spawn groups. Building it once per level load is what keeps the
/// per-frame query surface a fixed set of samplers rather than a mesh walk.
#[derive(Debug, Default)]
pub struct CollisionWorld {
    /// Physical material properties installed alongside collision geometry.
    pub ground_surfaces: GroundSurfaces,
    pub walls: Vec<WallAabb>,
    pub floor: WalkableFloor,
    pub water: WaterVolumes,
    pub ceiling: WalkableCeiling,
    pub ladders: Ladders,
    /// Every authored entity and the runtime that drives it.
    pub world: EntityWorld,
    /// The baked navigation mesh, when the source package carries one.
    ///
    /// `from_level` leaves this `None`: the player never derives navigation
    /// from geometry; the offline compiler bakes it and the loader installs
    /// the decoded record.
    pub navigation: Option<crate::engine::nav::NavMesh>,
    /// Local authored volumes used to recognise a genuine escape from a map.
    playable_space: PlayableSpace,
}

impl CollisionWorld {
    /// Installs material traction without changing the compiled geometry.
    #[must_use]
    pub fn with_ground_materials(
        mut self,
        level: &LevelDef,
        materials: &crate::engine::materials::MaterialTable,
    ) -> Self {
        self.ground_surfaces = GroundSurfaces::from_level(level, materials);
        self
    }

    /// Resolves every collision sampler and the entity runtime against a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        Self {
            ground_surfaces: GroundSurfaces::default(),
            walls: level.collision_aabbs(),
            floor: WalkableFloor::from_level(level),
            water: WaterVolumes::from_level(level),
            ceiling: WalkableCeiling::from_level(level),
            ladders: Ladders::from_level(level),
            world: EntityWorld::from_level(level),
            navigation: None,
            playable_space: PlayableSpace::from_level(level),
        }
    }

    /// [`Self::from_level`] plus a navigation mesh, for tests and tools that
    /// bake directly from a level source.
    #[must_use]
    pub fn from_level_with_navigation(
        level: &LevelDef,
        navigation: crate::engine::nav::NavMesh,
    ) -> Self {
        let mut world = Self::from_level(level);
        world.navigation = Some(navigation);
        world
    }

    /// Builds the collision world from a compiled static record plus the
    /// level's entity runtime.
    #[must_use]
    pub fn from_compiled(
        level: &LevelDef,
        statics: crate::engine::package::collision::CompiledCollision,
        navigation: Option<crate::engine::nav::NavMesh>,
    ) -> Self {
        Self {
            ground_surfaces: GroundSurfaces::default(),
            walls: statics.walls,
            floor: statics.floor,
            water: statics.water.with_motion(level),
            ceiling: statics.ceiling,
            ladders: statics.ladders,
            world: EntityWorld::from_level(level),
            navigation,
            playable_space: PlayableSpace::from_level(level),
        }
    }
}

/// Eye position for a level's authored spawn.
///
/// The spawn is resolved against the *actual* walkable floor under it — a room
/// base elevation plus any floor region — so a player is never left beneath an
/// elevated floor, embedded in one, or floating above a recessed region. A
/// spawn outside every room falls back to the global ground plane at `0.0`.
#[must_use]
pub fn spawn_position(level: &LevelDef) -> Vec3 {
    let floor_y = LevelSurfaces::new(level)
        .floor_y_at(level.spawn.x, level.spawn.z)
        .unwrap_or(0.0);
    Vec3::new(level.spawn.x, floor_y + EYE_HEIGHT, level.spawn.z)
}

/// Eye Y for a world position: the walkable floor under it plus the standard
/// eye height, falling back to the global ground plane at `0.0` outside
/// every room.
#[must_use]
pub fn spawn_eye_y(floor: &WalkableFloor, x: f32, z: f32) -> f32 {
    floor.height_at(x, z).unwrap_or(0.0) + EYE_HEIGHT
}

/// What one dispatched action batch did.
///
/// The report is produced by the entity runtime ([`crate::engine::entities`]) and
/// re-exported here so the frame loop and tests keep one name for it.
pub use crate::engine::entities::DispatchReport;

/// Manages game loop timing, player state, and menu lifecycle.
// The flags are independent, documented state machines (Rust's enum-per-flag
// would obscure the existing public fields), not interchangeable booleans.
#[expect(
    clippy::struct_excessive_bools,
    reason = "The flags are independent, documented state machines (Rust's enum-per-flag would obscure the existing public fields), not interchangeable booleans."
)]
pub struct Game {
    playable_space: PlayableSpace,
    /// Time continuously outside every authored movement volume.
    outside_map_seconds: f32,
    /// A validated spawn pose, kept separately from an invalid authored spawn.
    recovery_spawn: Option<RecoveryDestination>,
    noclip: Option<developer::ReturnPose>,
    ground_surfaces: GroundSurfaces,
    /// Last collision-constrained planar velocity. Normal walking still uses
    /// immediate input; only ice (and its airborne takeoff) integrates it.
    horizontal_velocity: Vec2,
    ice_airborne: bool,
    running: bool,
    app_state: AppState,
    last_frame_time: Instant,
    /// Real elapsed time since the previous frame (used for FPS measurement).
    delta_seconds: f32,
    /// Clamped delta used for gameplay simulation (see [`MAX_SIM_DELTA`]).
    sim_delta_seconds: f32,
    world_time_scale: f32,
    frame_count: u64,
    /// World Y of the rendered eye: exactly `feet_y + eye_offset_current`.
    ///
    /// Kept as a public field for the camera and tests. Every mutator in the
    /// controller preserves the invariant; the buoyant swim pose moves the eye
    /// and derives the feet, every other state moves the feet and derives the
    /// eye.
    pub player_position: Vec3,
    /// Authoritative world Y of the player's physical feet.
    ///
    /// The vertical simulation advances this value: falling, landing, walking
    /// and climbing all reason about the feet, and the rendered eye follows
    /// through [`Self::eye_offset_current`]. It is the single source of truth
    /// the old `player_position.y - eye_offset()` derivations read.
    feet_y: f32,
    /// The animated eye offset above the feet, in metres.
    ///
    /// Starts at [`EYE_HEIGHT`] and eases toward the current stance's offset at
    /// the fixed [`CROUCH_TRANSITION_SECONDS`] rate; the collision body uses
    /// the target stance immediately (`Game::body_height`).
    eye_offset_current: f32,
    /// World Y of the walkable floor the player is standing on: the pitch line
    /// across a staircase, the exact rendered surface everywhere else (see
    /// [`crate::engine::level::WalkableFloor::walk_height_at`]). This is the value
    /// collision filters against, so the camera and the collision band always
    /// agree about the local floor.
    pub player_floor_y: f32,
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub walls: Vec<WallAabb>,
    /// The spatial index over [`Game::walls`], rebuilt whenever the wall list
    /// is replaced. It only narrows the candidate set for movement, support,
    /// headroom, routes and aiming; the exact predicates still decide the
    /// answer, so its contents can never change gameplay.
    collision_index: CollisionIndex,
    /// The level's walkable floor surfaces (rooms + local floor regions).
    pub floor: WalkableFloor,
    /// The level's walkable ceilings, sampled to clamp a jumping head.
    pub ceiling: WalkableCeiling,
    /// The level's water volumes, sampled every Playing update.
    pub water: WaterVolumes,
    /// The level's climbable ladder volumes.
    pub ladders: Ladders,
    /// The level's whole entity runtime: authored ids, typed components,
    /// doors, routes, volumes, lights, events, timers, sequences and spawns.
    /// Private with [`Game::world`] so no caller can replace it without the
    /// player-side state being re-seeded.
    world: EntityWorld,
    /// The installed baked navigation mesh, queried by the AI runtime. This
    /// is uploaded from the package; the player never builds or repairs it.
    navigation: Option<crate::engine::nav::NavMesh>,
    /// Vertical speed in m/s, positive upward. Zero while grounded.
    pub vertical_velocity: f32,
    /// True while the player stands on the walkable floor or a solid prop top.
    pub grounded: bool,
    /// The locomotion pose and speed reported to animation, refreshed by every
    /// Playing update.
    locomotion: LocomotionSnapshot,
    /// True while the water under the player is deep enough to swim.
    swimming: bool,
    /// A checked, timed ledge jump in progress.
    water_exit: Option<WaterExit>,
    /// Walkable rim touched by the current horizontal sweep.
    pressed_exit_support: Option<f32>,
    /// Reused contact probes; ordinary swimming performs no per-step allocation.
    water_exit_probes: Vec<Vec2>,
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
    /// The authored spawn this run resets to: `reset_to_start`, and the point
    /// the trigger sweep is re-seeded from after a teleport.
    spawn_position: Vec3,
    /// The authored spawn yaw, in radians.
    spawn_yaw: f32,
    /// How many times this run has returned the player to the spawn.
    reset_count: u64,
    world_reset_count: u64,
    /// Player sweep awaiting the once-per-frame combined physical trigger update.
    trigger_sweep: Option<(Vec3, Vec3)>,
    /// Vertical simulation time not yet consumed by a fixed
    /// [`VERTICAL_SUBSTEP`].
    vertical_accumulator: f32,
    /// A real upward impact during the latest rendered movement frame.
    /// Falling may already have resumed by the frame's final substep.
    ceiling_contact_this_frame: bool,
    /// Explicit opt-in frame/contact diagnostics, silent during normal play.
    movement_debug: bool,
    /// Counters of the most recent entity-world tick.
    last_tick: WorldTick,
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
    /// The step is accepted with no floor under it; walking loses support.
    Void,
}

/// A checked ballistic ledge exit, limited to a short forward arc.
/// Physical sweeps and the shared vertical solver own all motion and landing.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WaterExit {
    /// World Y of the validated destination floor.
    support_y: f32,
    /// World Y of the water surface the exit is measured against.
    surface_y: f32,
    direction: Vec2,
    remaining: f32,
}

/// The walking support under one candidate position, relative to the surface
/// currently underfoot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkSupport {
    /// World Y of the surface the foot lands on: the walking floor or a solid
    /// top, whichever is higher.
    surface_y: f32,
    /// True when the surface is more than a walkable step below
    /// `current_surface`: the step is accepted and loses support at the end of
    /// the sweep (the existing ledge fall).
    dropped: bool,
}

impl Game {
    #[must_use]
    pub fn new(spawn_pos: Vec3, spawn_yaw: f32, world: CollisionWorld) -> Self {
        let grounded = false;
        let mut game = Self {
            playable_space: world.playable_space,
            outside_map_seconds: 0.0,
            recovery_spawn: None,
            noclip: None,
            running: true,
            app_state: AppState::MainMenu,
            last_frame_time: Instant::now(),
            delta_seconds: 0.0,
            sim_delta_seconds: 0.0,
            world_time_scale: 1.0,
            frame_count: 0,
            player_floor_y: spawn_pos.y - EYE_HEIGHT,
            player_position: spawn_pos,
            feet_y: spawn_pos.y - EYE_HEIGHT,
            eye_offset_current: EYE_HEIGHT,
            player_yaw: spawn_yaw.rem_euclid(TWO_PI),
            player_pitch: 0.0,
            collision_index: CollisionIndex::build(&world.walls),
            walls: world.walls,
            floor: world.floor,
            ceiling: world.ceiling,
            water: world.water,
            ladders: world.ladders,
            world: world.world,
            navigation: world.navigation,
            ground_surfaces: world.ground_surfaces,
            horizontal_velocity: Vec2::ZERO,
            ice_airborne: false,
            vertical_velocity: 0.0,
            grounded,
            locomotion: LocomotionSnapshot::default(),
            swimming: false,
            water_exit: None,
            pressed_exit_support: None,
            water_exit_probes: Vec::with_capacity(8),
            stance: Stance::Standing,
            climbing: None,
            jump_latched: false,
            crouch_latched: false,
            interact_latched: false,
            interact_pressed: false,
            spawn_position: spawn_pos,
            spawn_yaw: spawn_yaw.rem_euclid(TWO_PI),
            reset_count: 0,
            world_reset_count: 0,
            trigger_sweep: None,
            vertical_accumulator: 0.0,
            ceiling_contact_this_frame: false,
            movement_debug: std::env::var("PLACES_MOVEMENT_DEBUG").is_ok_and(|value| value == "1"),
            last_tick: WorldTick::default(),
        };
        game.recover_spawn_overlap();
        game.refresh_ground_support();
        game.remember_recovery_spawn();
        game.world.seed_volumes(Vec3::new(
            game.player_position.x,
            game.feet_y(),
            game.player_position.z,
        ));
        let routes = RouteWorld {
            walls: &game.walls,
            floor: &game.floor,
            index: &game.collision_index,
        };
        game.world.update_entities(0.0, &routes, None);
        game
    }

    /// The level's entity runtime.
    #[must_use]
    pub const fn world(&self) -> &EntityWorld {
        &self.world
    }

    /// The installed baked navigation mesh, when the package carries one.
    #[must_use]
    pub const fn navigation(&self) -> Option<&crate::engine::nav::NavMesh> {
        self.navigation.as_ref()
    }

    /// The level's entity runtime, mutably.
    pub const fn world_mut(&mut self) -> &mut EntityWorld {
        &mut self.world
    }

    /// Replaces the authored spawn without rebuilding the collision world or
    /// the entity runtime.
    ///
    /// Used by the `PLACES_SPAWN` developer override before play begins: the
    /// player, the reset target and the volume baseline all move to the new
    /// point, and nothing else about the level changes.
    pub fn reset_spawn_point(&mut self, spawn_pos: Vec3, spawn_yaw: f32) {
        self.spawn_position = spawn_pos;
        self.spawn_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.clear_run_state(spawn_pos, self.spawn_yaw, false);
        self.remember_recovery_spawn();
        self.world
            .reseed_volumes(Vec3::new(spawn_pos.x, self.feet_y(), spawn_pos.z));
        self.seed_routes_at_rest();
    }

    /// Repositions the benchmark player while retaining entity routes and state.
    /// Collision support and trigger baselines follow the ordinary spawn path.
    pub fn set_benchmark_player_position(&mut self, eye: Vec3, yaw: f32, pitch: f32) {
        let frame_time = self.last_frame_time;
        let real_delta = self.delta_seconds;
        let simulation_delta = self.sim_delta_seconds;
        self.clear_run_state(eye, yaw, false);
        self.player_pitch = pitch;
        self.reseed_trigger_inside();
        self.last_frame_time = frame_time;
        self.delta_seconds = real_delta;
        self.sim_delta_seconds = simulation_delta;
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
    /// ceilings, water, ladders and the entity runtime when loading a level.
    ///
    /// The player spawns grounded when a walkable floor exists under the spawn
    /// point; outside every room a spawn settles on the global ground plane
    /// at its own height on the first update. Vertical velocity starts at zero,
    /// the jump and crouch latches are released, and the stance returns to
    /// standing. A fresh level also starts the run's reset counter at zero and
    /// re-seeds every trigger volume from the spawn position.
    pub fn reset_level(&mut self, spawn_pos: Vec3, spawn_yaw: f32, world: CollisionWorld) {
        self.ground_surfaces = world.ground_surfaces;
        self.walls = world.walls;
        self.collision_index = CollisionIndex::build(&self.walls);
        self.floor = world.floor;
        self.water = world.water;
        self.ceiling = world.ceiling;
        self.ladders = world.ladders;
        self.world = world.world;
        self.navigation = world.navigation;
        self.playable_space = world.playable_space;
        self.spawn_position = spawn_pos;
        self.spawn_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.reset_count = 0;
        self.world_reset_count = 0;
        self.clear_run_state(spawn_pos, self.spawn_yaw, false);
        self.remember_recovery_spawn();
        self.world
            .seed_volumes(Vec3::new(spawn_pos.x, self.feet_y(), spawn_pos.z));
        self.seed_routes_at_rest();
    }

    /// Returns the player to the level's authored spawn without rebuilding the
    /// collision world.
    ///
    /// This is the `reset_to_start` action: ground, water, ladder, stance and
    /// velocity state are reconciled to a clean standing spawn, the entity
    /// runtime re-seeds every door, route, timer, spawn group and volume from
    /// the authored start, and keys held across the reset cannot fire again
    /// until released. Label visibility is preserved: it is a view toggle, not
    /// movement state.
    pub fn reset_to_spawn(&mut self) {
        self.trigger_sweep = None;
        self.clear_run_state(self.spawn_position, self.spawn_yaw, true);
        self.world.reset_runtime();
        self.world.seed_volumes(Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        ));
        self.reset_count = self.reset_count.saturating_add(1);
        self.world_reset_count = self.world_reset_count.saturating_add(1);
    }

    /// How many times this run has returned the player to the spawn.
    #[must_use]
    pub const fn reset_count(&self) -> u64 {
        self.reset_count
    }

    /// Full authored runtime resets, distinct from player-only respawn/recovery.
    #[must_use]
    pub const fn world_reset_count(&self) -> u64 {
        self.world_reset_count
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
        self.trigger_sweep = None;
        self.noclip = None;
        self.player_floor_y = spawn_pos.y - EYE_HEIGHT;
        self.player_position = spawn_pos;
        self.feet_y = spawn_pos.y - EYE_HEIGHT;
        self.eye_offset_current = EYE_HEIGHT;
        self.player_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.player_pitch = 0.0;
        self.horizontal_velocity = Vec2::ZERO;
        self.ice_airborne = false;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;
        self.grounded = false;
        self.swimming = false;
        self.water_exit = None;
        self.pressed_exit_support = None;
        self.ceiling_contact_this_frame = false;
        self.outside_map_seconds = 0.0;
        self.stance = Stance::Standing;
        self.climbing = None;
        self.recover_spawn_overlap();
        self.refresh_ground_support();
        self.jump_latched = suppress_held;
        self.crouch_latched = suppress_held;
        self.interact_latched = suppress_held;
        self.interact_pressed = false;
        self.locomotion = LocomotionSnapshot::default();
        self.last_frame_time = Instant::now();
        self.delta_seconds = 0.0;
        self.sim_delta_seconds = 0.0;
    }

    /// Re-baselines every volume's `inside` flag at the player's feet without
    /// firing an edge.
    ///
    /// Used when the body's reference feet move without locomotion (the swim
    /// stance animation) and after a level load: the volume the player is now
    /// in becomes the new baseline, so the change itself is never reported as
    /// a swept entry.
    fn reseed_trigger_inside(&mut self) {
        self.trigger_sweep = None;
        let feet = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        self.world.reseed_volumes(feet);
    }

    /// Re-seeds every route's state without advancing time.
    fn seed_routes_at_rest(&mut self) {
        let routes = RouteWorld {
            walls: &self.walls,
            floor: &self.floor,
            index: &self.collision_index,
        };
        self.world.update_entities(0.0, &routes, None);
    }

    /// Advances the entity runtime by one frame and applies its outcome.
    fn update_world(&mut self, feet_from: Vec3) {
        let delta = self.world_delta_seconds();
        let feet = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        let ctx = WorldContext {
            delta_seconds: delta,
            feet_from,
            feet,
            eye: self.player_position,
            body_height: self.body_height(),
            walls: &self.walls,
            index: &self.collision_index,
            floor: &self.floor,
            nav: self.navigation.as_ref(),
        };
        self.trigger_sweep = Some((ctx.feet_from, ctx.feet));
        let tick = self.world.tick(&ctx);
        self.last_tick = tick;
        if tick.player_reset {
            // `reset_to_start` ends the frame: the player is at the spawn and
            // nothing else this frame may act on the teleported state.
            self.reset_to_spawn();
            return;
        }
        let routes = RouteWorld {
            walls: &self.walls,
            floor: &self.floor,
            index: &self.collision_index,
        };
        self.world
            .update_entities(delta, &routes, Some(self.player_position));
    }

    /// Advances a non-playable menu world through the normal scripts, AI and door runtime.
    /// The fixed camera/player has no locomotion or gameplay input.
    pub(crate) fn update_menu_world(&mut self) {
        let feet = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        self.update_world(feet);
    }

    /// Samples triggers once after the native item step, then dispatches their actions.
    /// The frame loop must call this after each Playing movement update. A skipped
    /// update consumes no sweep, so paused frames never replay an earlier crossing.
    pub fn update_volume_occupants(&mut self, items: &[crate::engine::entities::TriggerItem<'_>]) {
        self.update_volume_occupants_iter(items.iter().copied());
    }

    /// Iterator variant for the native loop's persistent physics snapshot buffer.
    pub fn update_volume_occupants_iter<'a>(
        &mut self,
        items: impl Iterator<Item = crate::engine::entities::TriggerItem<'a>> + Clone,
    ) {
        let Some((from, to)) = self.trigger_sweep.take() else {
            return;
        };
        self.world.update_volume_occupants_iter(from, to, items);
        let report = self.world.dispatch_queued_events();
        if report.player_reset {
            self.reset_to_spawn();
        }
    }

    /// Baselines physical trigger occupancy after loading/resetting item bodies.
    pub fn seed_volume_occupants(&mut self, items: &[crate::engine::entities::TriggerItem<'_>]) {
        self.seed_volume_occupants_iter(items.iter().copied());
    }

    /// Iterator variant for initial physical item occupancy.
    pub fn seed_volume_occupants_iter<'a>(
        &mut self,
        items: impl Iterator<Item = crate::engine::entities::TriggerItem<'a>> + Clone,
    ) {
        self.trigger_sweep = None;
        let feet = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        self.world.seed_volume_occupants_iter(feet, items);
    }

    /// World simulation time is independent of the player's movement clock.
    /// # Errors
    /// Rejects nonfinite and out-of-range values without changing the clock.
    pub fn set_world_time_scale(&mut self, value: f32) -> Result<(), String> {
        if !value.is_finite() || !(0.0..=4.0).contains(&value) {
            return Err("World time scale must be finite and from 0 through 4.".into());
        }
        self.world_time_scale = value;
        Ok(())
    }

    #[must_use]
    pub fn scale_world_delta(&self, delta: f32) -> f32 {
        if delta.is_finite() && delta > 0.0 {
            (delta * self.world_time_scale).min(MAX_SIM_DELTA)
        } else {
            0.0
        }
    }

    #[must_use]
    pub fn world_delta_seconds(&self) -> f32 {
        self.scale_world_delta(self.sim_delta_seconds)
    }

    /// The tick report of the last [`Game::update_world`], for diagnostics.
    #[must_use]
    pub const fn last_world_tick(&self) -> WorldTick {
        self.last_tick
    }

    /// The per-frame entity handoff for the character renderer.
    #[must_use]
    pub fn entity_frames(&self) -> &[EntityFrame] {
        self.world.entity_frames()
    }

    /// The authored entity routes resident for this level.
    #[must_use]
    pub const fn routes(&self) -> &EntityRoutes {
        self.world.routes()
    }

    /// One route's runtime state, for diagnostics and tests.
    #[must_use]
    pub fn route_state(&self, instance_id: &str) -> Option<&RouteState> {
        self.world.route_state(instance_id)
    }

    /// The live animation override on `instance_id`, if one is set.
    #[must_use]
    pub fn animation_override(&self, instance_id: &str) -> Option<&PoseCue> {
        self.world.animation_override(instance_id)
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

    /// The real elapsed frame time, in seconds, for FPS measurement.
    #[must_use]
    pub const fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    /// The clamped delta the gameplay simulation consumed this frame, in
    /// seconds.
    ///
    /// The same value [`Self::update_player_movement`] integrates; the
    /// developer move script sums these so its scheduling matches the
    /// simulated world at any frame rate.
    #[must_use]
    pub const fn sim_delta_seconds(&self) -> f32 {
        self.sim_delta_seconds
    }

    /// Sets a controlled simulation step for an explicitly enabled benchmark.
    /// Real frame timing remains available for telemetry and presentation costs.
    pub const fn set_benchmark_simulation_delta(&mut self, delta: f32) {
        self.sim_delta_seconds = clamp_sim_delta(delta);
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
    ///
    /// The target stance owns the body immediately: crouching shrinks the
    /// collider on the press frame, so the transition is always safe.
    #[must_use]
    pub const fn body_height(&self) -> f32 {
        self.stance.height()
    }

    /// The animated eye offset above the feet, in metres.
    ///
    /// Eases toward the current stance's offset at the
    /// [`CROUCH_TRANSITION_SECONDS`] rate; equal to the stance's offset at the
    /// ends of the transition.
    #[must_use]
    pub const fn eye_offset(&self) -> f32 {
        self.eye_offset_current
    }

    /// World Y of the player's physical feet: the authoritative vertical
    /// coordinate, with the rendered eye always `feet_y() + eye_offset()`.
    #[must_use]
    pub const fn feet_y(&self) -> f32 {
        self.feet_y
    }

    /// Writes the rendered eye back onto the invariant line.
    fn sync_eye(&mut self) {
        self.player_position.y = self.feet_y + self.eye_offset_current;
    }

    /// Move physical feet and derive the eye from the live stance.
    fn set_feet_y(&mut self, feet_y: f32) {
        self.feet_y = feet_y;
        self.sync_eye();
    }

    /// Feet anchor the stance in water as well as on land: a crouch eases the
    /// camera without moving the supporting body or fabricating a volume entry.
    fn advance_eye_offset(&mut self, delta: f32) {
        let target = self.stance.eye_offset();
        let remaining = target - self.eye_offset_current;
        if remaining != 0.0 {
            let rate = (EYE_HEIGHT - CROUCH_EYE_HEIGHT) / CROUCH_TRANSITION_SECONDS;
            let step = (rate * delta).min(remaining.abs());
            if step >= remaining.abs() {
                self.eye_offset_current = target;
            } else {
                self.eye_offset_current = remaining.signum().mul_add(step, self.eye_offset_current);
            }
        }
        self.sync_eye();
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
        self.update_player_movement_constrained(input, settings, |game| game.player_position);
    }

    /// Applies an optional collision constraint to the resolved planar motion
    /// before triggers and recovery see it. Vertical movement, stance and the
    /// existing controller retain ownership of height and gravity.
    pub fn update_player_movement_constrained(
        &mut self,
        input: &mut InputState,
        settings: &Settings,
        constrain: impl FnOnce(&Self) -> Vec3,
    ) {
        self.update_player_movement_with_constraints(input, settings, constrain, |_| true);
    }

    /// Extends movement collision with a live-body stance clearance check.
    /// A blocked standing expansion retains crouch before physical sweeping.
    pub fn update_player_movement_with_constraints(
        &mut self,
        input: &mut InputState,
        settings: &Settings,
        constrain: impl FnOnce(&Self) -> Vec3,
        stance_clear: impl FnOnce(&Self) -> bool,
    ) {
        self.ceiling_contact_this_frame = false;
        // A press is consumed exactly once and never survives a non-Playing
        // frame: the interaction latch is re-derived below, and the frame loop
        // owns dispatch through [`Game::take_interact_press`].
        self.interact_pressed = false;

        // The frame's relative motion is always consumed, even while paused or
        // in a menu, so motion collected around a pause is never applied later.
        let motion = input.take_mouse_motion();
        let frame_input = *input;
        input.clear_presses();
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] frame_start feet={:?} body_height={} radius={} input={frame_input:?} velocity_y={} grounded={} delta={}",
                Vec3::new(self.player_position.x, self.feet_y, self.player_position.z),
                self.body_height(),
                PLAYER_RADIUS,
                self.vertical_velocity,
                self.grounded,
                self.sim_delta_seconds
            ));
        }

        // While paused or in menus, do not update player movement or looking
        if self.app_state != AppState::Playing {
            return;
        }

        if self.noclip_enabled() {
            self.update_noclip(&frame_input, settings, motion);
            return;
        }

        // Non-finite positions must be restored before collision queries can
        // clamp or integrate them. Finite escapes receive the grace interval
        // after this frame's ordinary locomotion has had a chance to return.
        if !self.player_motion_is_finite() {
            let _recovered = self.recover_out_of_map();
            return;
        }

        let previous_stance = self.stance;
        let previous_eye_offset = self.eye_offset_current;
        let trigger_origin = self.update_playing_frame(&frame_input, settings, motion);
        if previous_stance == Stance::Crouched
            && self.stance == Stance::Standing
            && !stance_clear(self)
        {
            self.stance = Stance::Crouched;
            self.eye_offset_current = previous_eye_offset;
            self.sync_eye();
        }
        let constrained = constrain(self);
        let correction = Vec3::new(
            constrained.x - self.player_position.x,
            constrained.y - self.player_position.y,
            constrained.z - self.player_position.z,
        );
        if constrained.is_finite()
            && correction.length_squared() > 0.0
            && self.body_fits_with_height(
                Vec3::new(constrained.x, self.feet_y + correction.y, constrained.z),
                self.body_height(),
            )
        {
            self.player_position = constrained;
            self.feet_y += correction.y;
            if correction.y.abs() > 1.0e-5 {
                self.vertical_velocity = 0.0;
                self.vertical_accumulator = 0.0;
                self.grounded = correction.y > 0.0;
                if self.grounded {
                    self.player_floor_y = self.feet_y;
                }
            }
        }
        if self.recover_out_of_map() {
            return;
        }
        self.update_world(trigger_origin);
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] frame_end feet={:?} velocity_y={} grounded={} ceiling_contact={}",
                Vec3::new(self.player_position.x, self.feet_y, self.player_position.z),
                self.vertical_velocity,
                self.grounded,
                self.ceiling_contact_this_frame
            ));
        }
    }

    /// One Playing frame: look, stance, interact latch, movement,
    /// water/ladder resolution and vertical integration.
    ///
    /// Returns the feet position the frame's *movement* starts from: the swept
    /// origin for area triggers, captured after the stance edge. A stance
    /// change anchors the feet in every locomotion mode.
    fn update_playing_frame(
        &mut self,
        input: &InputState,
        settings: &Settings,
        motion: (f32, f32),
    ) -> Vec3 {
        let delta = self.sim_delta_seconds;
        self.update_look(input, settings, motion, delta);

        // Stance changes own the collision body before the frame's movement,
        // and the animated eye offset eases toward the new target before the
        // trigger origin is captured, so the stance anchor is never swept.
        self.update_crouch(input);
        self.advance_eye_offset(delta);
        // The Interact latch is an edge exactly like Jump and Crouch: one press
        // is one interaction, and a held key never repeats.
        self.update_interact(input);
        // Doors advance before the player moves, so this frame's movement reads
        // the leaf colliders at this frame's angles (the slab the player sees).
        self.update_doors(delta);
        if self.grounded && !self.swimming {
            let from = Vec2::new(self.player_position.x, self.player_position.z);
            let recovered =
                self.resolve_horizontal_contact(from, from, self.feet_y, HorizontalMode::Walk);
            self.player_position.x = recovered.x;
            self.player_position.z = recovered.y;
            // An externally placed body can intersect a wall beside a ramp.
            // The bounded spawn correction must finish on that ramp's actual
            // support rather than placing its feet beneath the surface.
            if recovered.distance_squared(from) > 1e-10
                && let Some(support) =
                    self.walking_support_at(recovered.x, recovered.y, self.feet_y)
                && !support.dropped
                && self.head_clear_for(support.surface_y, self.body_height())
            {
                self.player_floor_y = support.surface_y;
                self.set_feet_y(support.surface_y);
            }
        }
        let trigger_origin = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );

        // Collision and gravity share a bounded time subdivision. Horizontal
        // movement for a long render frame must not finish before the entire
        // vertical path is considered (especially at ledges and ceilings).
        let step_count = (delta / VERTICAL_SUBSTEP).ceil().clamp(1.0, 12.0);
        // The clamp is finite and bounded by the twelve simulation substeps.
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Finite frame deltas produce 1..=12 integral substeps; the saturating cast preserves the existing zero-step behavior for a malformed NaN delta"
        )]
        let steps = step_count as usize;
        let step_delta = delta / step_count;
        let mut substep_input = *input;
        self.sim_delta_seconds = step_delta;
        for _ in 0..steps {
            self.update_locomotion_step(&substep_input, settings, step_delta);
            substep_input.clear_presses();
        }
        self.sim_delta_seconds = delta;
        trigger_origin
    }

    /// Applies material traction to planar velocity, then uses the established
    /// walk/air/swim collision sweep. Normal movement keeps its original path.
    fn move_on_surface(
        &mut self,
        move_dir: Vec3,
        settings: &Settings,
        delta: f32,
        before_sample: Option<WaterSample>,
        on_ice: bool,
    ) -> f32 {
        let previous = Vec2::new(self.player_position.x, self.player_position.z);
        let in_water = self.swimming || self.water_exit.is_some();
        let feet = self.feet_y;
        let ice_motion = on_ice || (!self.grounded && !in_water && self.ice_airborne);
        let direction = Vec2::new(move_dir.x, move_dir.z).normalize_or_zero();
        let target = Vec2::new(
            direction.x * settings.walk_speed,
            direction.y * settings.walk_speed,
        );
        let velocity = if in_water {
            self.horizontal_velocity
        } else if ice_motion {
            let response = if on_ice {
                if target.length_squared() > 0.0 {
                    ICE_CONTROL_RESPONSE
                } else {
                    ICE_FRICTION
                }
            } else if target.length_squared() > 0.0 {
                ICE_AIR_CONTROL_RESPONSE
            } else {
                0.0
            };
            let retention = (-response * delta).exp();
            self.horizontal_velocity = target.lerp(self.horizontal_velocity, retention);
            if target == Vec2::ZERO
                && self.horizontal_velocity.length_squared() < ICE_STOP_SPEED * ICE_STOP_SPEED
            {
                self.horizontal_velocity = Vec2::ZERO;
            }
            self.horizontal_velocity
        } else {
            target
        };
        let drift = if in_water || ice_motion || before_sample.is_none() {
            Vec2::ZERO
        } else {
            let flow = self.player_fluid_velocity();
            let immersion = self.player_immersion();
            Vec2::new(flow.x * immersion, flow.y * immersion)
        };
        let effective_velocity = if in_water || ice_motion {
            velocity
        } else {
            Vec2::new(target.x + drift.x, target.y + drift.y)
        };
        if effective_velocity.length_squared() > 0.0 {
            let mode = if let Some(exit) = self.water_exit {
                // A checked exit retains its mean-water collision reference.
                HorizontalMode::Swim {
                    surface_y: exit.surface_y,
                }
            } else if self.swimming && !self.grounded {
                HorizontalMode::Swim {
                    surface_y: before_sample.map_or(feet, |s| s.surface_y),
                }
            } else if self.grounded {
                HorizontalMode::Walk
            } else {
                HorizontalMode::Airborne
            };
            if ice_motion || in_water || drift.length_squared() > 0.0 {
                self.move_horizontal(
                    Vec3::new(effective_velocity.x, 0.0, effective_velocity.y),
                    effective_velocity.length(),
                    delta,
                    mode,
                );
            } else {
                // Dry land retains its original direction/speed path exactly.
                self.move_horizontal(move_dir, settings.walk_speed, delta, mode);
            }
        }
        let current = Vec2::new(self.player_position.x, self.player_position.z);
        let horizontal_distance = previous.distance(current);
        if delta > 0.0 {
            // Collision spends blocked momentum; it never stores pressure that
            // could launch the player when a wall or pond rim ends.
            self.horizontal_velocity = Vec2::new(
                (current.x - previous.x) / delta,
                (current.y - previous.y) / delta,
            );
        }

        horizontal_distance
    }

    /// One short locomotion interval, with current contacts and velocity.
    /// Input edges, stance, looking and doors remain frame-owned.
    fn update_locomotion_step(&mut self, input: &InputState, settings: &Settings, delta: f32) {
        // Rim contact is recomputed for this locomotion interval.
        self.pressed_exit_support = None;

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
        let jump_pressed = input.was_pressed(Control::Jump) || (jump_held && !self.jump_latched);
        if !jump_held {
            self.jump_latched = false;
        } else if jump_pressed {
            self.jump_latched = true;
        }

        // A ladder owns the frame while attached (or on the frame it attaches):
        // it resolves climb motion and the top landing itself.
        if self.update_ladder(move_dir, jump_pressed, delta, settings) {
            self.horizontal_velocity = Vec2::ZERO;
            self.ice_airborne = false;
            self.refresh_locomotion(0.0, delta);
            return;
        }

        if self.water_exit.is_some() {
            let distance = self.advance_water_exit(delta);
            self.refresh_locomotion(distance, delta);
            return;
        }
        let before_sample = self.water.sample(
            self.player_position.x,
            self.player_position.z,
            self.feet_y(),
        );
        self.swimming = self.deep_water_under(before_sample);
        if self.swimming {
            self.climbing = None;
            self.water_velocity(input, settings, delta);
        } else if self.grounded {
            self.refresh_ground_support();
        }
        let on_ice = !self.swimming
            && self.grounded
            && self
                .ground_surfaces
                .at(self.player_position.x, self.player_position.z, self.feet_y)
                == GroundSurface::Ice;
        if self.grounded || self.swimming {
            self.ice_airborne = on_ice;
        }
        if !self.swimming && jump_pressed && self.grounded {
            self.vertical_velocity = JUMP_VELOCITY;
            self.grounded = false;
            self.vertical_accumulator = 0.0;
        }
        let horizontal_distance =
            self.move_on_surface(move_dir, settings, delta, before_sample, on_ice);
        let after_sample = self.water.sample(
            self.player_position.x,
            self.player_position.z,
            self.feet_y(),
        );
        let now_swimming = self.deep_water_under(after_sample);
        if now_swimming {
            if !self.swimming {
                self.swimming = true;
                self.water_velocity(input, settings, delta);
            }
            // The ledge check precedes vertical rise, like the reference: a
            // held rise may otherwise cross the waist boundary before it runs.
            self.try_water_exit(move_dir);
            if self.water_exit.is_none() {
                self.advance_swimming(delta);
            }
        } else {
            self.swimming = false;
            self.land_vertical(jump_pressed);
        }

        self.refresh_locomotion(horizontal_distance, delta);
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
        let pressed = input.was_pressed(Control::Crouch) || (held && !self.crouch_latched);
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
        let pressed = input.was_pressed(Control::Interact) || (held && !self.interact_latched);
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
        nearest_target_indexed(
            self.player_position,
            self.view_direction(),
            self.world.interactables().items(),
            &self.collision_index,
            &self.walls,
            self.world.door_colliders(),
        )
    }

    /// While carrying, E uses a reachable aimed door or carry-capable NPC.
    /// Other physical bodies and the ordinary wall/leaf checks still obstruct
    /// interaction; looking away deliberately selects safe set-down.
    pub(crate) fn carried_door_target(&self, body_distance: Option<f32>) -> Option<(usize, f32)> {
        let index = self.interaction_target()?;
        let item = self.interactables().get(index)?;
        if let Some(door_index) = item.door_index {
            if self.doors().get(door_index)?.is_locked() {
                return None;
            }
        } else if !self
            .world
            .handle_of(&item.id)
            .is_some_and(|handle| self.world.components().carries.contains(handle))
        {
            return None;
        }
        let distance = crate::engine::interact::target_distance(
            self.player_position,
            self.view_direction(),
            item,
            self.door_colliders(),
        )?;
        body_distance
            .is_none_or(|body| body >= distance)
            .then_some((index, distance))
    }

    /// The spatial index over the collision walls, for the label-occlusion
    /// pass and other callers that already hold a `Game`.
    #[must_use]
    pub const fn collision_index(&self) -> &CollisionIndex {
        &self.collision_index
    }

    /// Replaces the wall set and rebuilds the index in one step.
    ///
    /// Any caller that edits the collision world after construction (tests and
    /// future level mutations) must use this rather than pushing onto
    /// [`Game::walls`], so the index can never go stale.
    pub fn set_walls(&mut self, walls: Vec<WallAabb>) {
        self.walls = walls;
        self.collision_index = CollisionIndex::build(&self.walls);
    }

    /// The current eye direction, matching the render camera.
    #[must_use]
    pub fn view_direction(&self) -> Vec3 {
        crate::engine::interact::view_direction(self.player_yaw, self.player_pitch)
    }

    /// Runs the interaction on the currently aimed-at instance, if any.
    ///
    /// The press becomes an `interact` event on the aimed entity; the entity's
    /// own bindings decide what it does. Returns `None` when nothing is in
    /// reach.
    pub fn dispatch_interaction(&mut self) -> Option<DispatchReport> {
        let target = self.interaction_target();
        self.dispatch_interaction_at(target)
    }

    /// Runs optional authored guidance, applying reset through the same lifecycle
    /// as interactions and automatic callbacks.
    pub fn request_hint(&mut self) {
        if self.world.request_hint() {
            self.reset_to_spawn();
        }
    }

    /// Dispatches the native loop's already selected, occlusion-checked target.
    /// This keeps the action and hand affordance on the same world instance.
    pub fn dispatch_interaction_at(&mut self, target: Option<usize>) -> Option<DispatchReport> {
        let report = self
            .world
            .dispatch_interaction_from(target, Some(self.player_position))?;
        if report.player_reset {
            self.reset_to_spawn();
        }
        Some(report)
    }

    /// Runs one ordered batch of map-authored actions through the dispatcher.
    ///
    /// `actor` is the aiming-table index of the acting instance, exactly as
    /// before the entity runtime landed: the world resolves it to the entity
    /// that owns that target. Authored map wiring goes through events and
    /// bindings; this direct entry point serves tests and developer tooling.
    pub fn dispatch_actions(
        &mut self,
        actions: &[ActionDef],
        actor: Option<usize>,
    ) -> DispatchReport {
        let handle = actor
            .and_then(|index| self.world.interactables().get(index))
            .and_then(|item| self.world.handle_of(&item.id));
        let report = self.world.dispatch_actions(actions, handle);
        if report.player_reset {
            self.reset_to_spawn();
        }
        report
    }

    /// Every switchable fixture's current state, in fixture order, for a
    /// renderer rebuild after a graphics change.
    #[must_use]
    pub fn light_states(&self) -> Vec<(usize, bool)> {
        self.world.light_states()
    }

    /// Drains the fixture switches whose state changed since the last call.
    pub fn take_light_toggles(&mut self) -> Vec<(usize, bool)> {
        self.world.take_light_toggles()
    }

    /// True when `index`'s floating label is currently shown.
    #[must_use]
    pub fn is_label_visible(&self, index: usize) -> bool {
        self.world.is_label_visible(index)
    }

    /// The interactable instances resident for this level.
    #[must_use]
    pub const fn interactables(&self) -> &Interactables {
        self.world.interactables()
    }

    /// Every door resident for this level, in authored order.
    #[must_use]
    pub const fn doors(&self) -> &crate::engine::door::Doors {
        self.world.doors()
    }

    /// The level's entity runtime.
    #[must_use]
    pub const fn entities(&self) -> &EntityWorld {
        &self.world
    }

    /// The level's entity runtime, mutably.
    pub const fn entities_mut(&mut self) -> &mut EntityWorld {
        &mut self.world
    }

    /// Enables or disables one authored water volume's sampling.
    ///
    /// The baked surface keeps drawing; a disabled volume simply stops
    /// answering the controller's water queries.
    pub fn set_water_enabled(&mut self, index: usize, enabled: bool) {
        let _ = self.water.set_enabled(index, enabled);
    }

    /// The door leaf colliders at the current angles.
    #[must_use]
    pub fn door_colliders(&self) -> &[DoorCollider] {
        self.world.door_colliders()
    }

    /// The number of authored trigger volumes.
    #[must_use]
    pub fn volume_count(&self) -> usize {
        self.world.components().volumes.len()
    }

    /// Advances every moving door by one frame and republishes its collider.
    fn update_doors(&mut self, unscaled_delta: f32) {
        let delta = self.scale_world_delta(unscaled_delta);
        let feet = Vec3::new(
            self.player_position.x,
            self.feet_y(),
            self.player_position.z,
        );
        let ctx = WorldContext {
            delta_seconds: delta,
            feet_from: feet,
            feet,
            eye: self.player_position,
            body_height: self.body_height(),
            nav: self.navigation.as_ref(),
            walls: &self.walls,
            index: &self.collision_index,
            floor: &self.floor,
        };
        let _ = self.world.update_doors(&ctx);
    }

    /// The collision world's boxes, for presentation-side occlusion tests.
    #[must_use]
    pub fn walls(&self) -> &[WallAabb] {
        &self.walls
    }

    /// Toggles the requested stance.
    ///
    /// The target stance owns the collision body immediately: crouching
    /// shrinks the body on the press frame (always safe), and standing is
    /// refused (and the player remains crouched) when the standing head would
    /// meet a ceiling, a frame or a prop underside at the current feet. The
    /// animated eye offset eases toward the new target afterwards (see
    /// `Game::advance_eye_offset`); nothing moves here.
    fn toggle_stance(&mut self) {
        let target = self.stance.next();
        if target == Stance::Standing && !self.head_clear_for(self.feet_y, PLAYER_HEIGHT) {
            return;
        }
        self.stance = target;
        // Anchor the feet during stance easing, then re-acquire the float
        // line at the ordinary rise/sink rate instead of moving the body by
        // the changed eye offset.
    }

    /// The lowest overhead limit above `feet`: the room ceiling or a box
    /// underside (a door header, a window frame, a prop), whichever is lower.
    fn head_limit(&self, feet: f32) -> Option<f32> {
        self.head_limit_at(self.player_position.x, self.player_position.z, feet)
    }

    /// Samples overhead clearance at a candidate before committing a step.
    fn head_limit_at(&self, x: f32, z: f32, feet: f32) -> Option<f32> {
        let ceiling = self
            .ceiling
            .ceiling_above_disc(x, z, PLAYER_RADIUS - CONTACT_EPS, feet);
        // A tangent side wall is contact, not an overhead obstruction.
        let underside = lowest_underside_indexed(
            &self.collision_index,
            x,
            z,
            PLAYER_RADIUS - CONTACT_EPS,
            feet,
            &self.walls,
        );
        let door_underside = lowest_door_underside(
            self.world.door_colliders(),
            x,
            z,
            PLAYER_RADIUS - CONTACT_EPS,
            feet,
        );
        // Keep the established ladder clearance reference during attachment.
        // Swimming uses the same physical feet/head clearance as walking.
        let floor_reference = if self.climbing.is_some() {
            feet.max(self.player_position.y)
        } else {
            feet
        };
        let floor_underside = self.floor.lowest_room_underside(
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            floor_reference,
        );
        [ceiling, underside, door_underside, floor_underside]
            .into_iter()
            .flatten()
            .min_by(f32::total_cmp)
    }

    /// True when a body of `height` whose feet stand at `feet` fits under the
    /// local ceiling and overhead boxes.
    fn head_clear_for(&self, feet: f32, height: f32) -> bool {
        self.head_limit(feet)
            .is_none_or(|limit| limit >= feet + height - CONTACT_EPS)
    }

    /// The highest support at `(x, z)` at or below `max_top`: the rendered
    /// floor or a solid prop/architecture top. Empty-floor test worlds retain
    /// the legacy Y=0 plane; authored worlds have no off-room support.
    ///
    /// Landing resolves against the *rendered* floor (`height_at`), not the
    /// staircase pitch line: a falling player lands on the tread underfoot.
    /// The pitch line is a grounded walking surface, applied only while the
    /// player is actually walking, so an airborne player is never pulled up
    /// onto a staircase.
    ///
    /// Landing never uses the walking step allowance: a top above the start
    /// feet would pull the player up through a solid rather than catch a fall.
    fn support_at(&self, x: f32, z: f32, max_top: f32) -> Option<f32> {
        // A floor above the reference is not a support: the feet would be
        // snapping *up* through it.
        let floor = self.floor.landing_height_below_disc(
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            max_top + CONTACT_EPS,
        );
        let ceiling_top = self.ceiling.surface_below_disc(
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            max_top + CONTACT_EPS,
        );
        let support_floor = match (floor, ceiling_top) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        // The shared top query includes the walking allowance. Landing must
        // only select a surface actually crossed by the descending feet.
        let mut top = body_support_top(
            &self.collision_index,
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            max_top + CONTACT_EPS,
            max_top + CONTACT_EPS,
            &self.walls,
        );
        for door in self.world.door_colliders() {
            let height = door.hinge_y + door.height;
            if height <= max_top + CONTACT_EPS
                && door.overlaps_disc(x, z, PLAYER_RADIUS - CONTACT_EPS)
            {
                top = Some(top.map_or(height, |current| current.max(height)));
            }
        }
        match (support_floor, top) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => {
                if self.floor.is_empty() && max_top >= -STEP_EPS {
                    Some(0.0)
                } else {
                    None
                }
            }
        }
    }

    /// The walking support at `(x, z)` relative to the surface currently
    /// underfoot: the rendered walkable floor or a solid prop/architecture
    /// top, whichever is higher, within the walking step allowance.
    ///
    /// A support above `current_surface + PLAYER_STEP_HEIGHT + STEP_EPS` is out
    /// of reach and returns `None` (the caller refuses the step: a cliff, a
    /// wall or a table side). Anything at or below is returned; one more than
    /// a step below `current_surface` is marked `dropped`, which switches the
    /// remaining sweep to airborne collision. Outside every room with no solid
    /// top underfoot the query returns `None` (no
    /// invisible world floor appears under a walking player).
    ///
    /// The returned surface is the *walking* surface: on a staircase it is the
    /// pitch line through the nosings, everywhere else the rendered surface.
    /// A solid top always wins when it is higher, so a player on a table walks
    /// on the table, never on the room floor beneath it.
    fn walking_support_at(&self, x: f32, z: f32, current_surface: f32) -> Option<WalkSupport> {
        let floor_ceiling = current_surface + PLAYER_STEP_HEIGHT + STEP_EPS;
        let floor = self.floor.height_below_disc(
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            floor_ceiling,
        );
        let ceiling_top = self.ceiling.surface_below_disc(
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            floor_ceiling,
        );
        let support_floor = match (floor, ceiling_top) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let top = body_support_top(
            &self.collision_index,
            Vec2::new(x, z),
            PLAYER_RADIUS - CONTACT_EPS,
            floor_ceiling,
            current_surface + CONTACT_EPS,
            &self.walls,
        );
        let surface_y = match (support_floor, top) {
            (Some(floor_y), Some(prop_top)) if prop_top > floor_y => prop_top,
            (Some(floor_y), _) => self
                .floor
                .walk_height_below(x, z, floor_ceiling)
                .unwrap_or(floor_y)
                .max(floor_y),
            (None, Some(prop_top)) => prop_top,
            (None, None) => return None,
        };
        Some(WalkSupport {
            surface_y,
            dropped: surface_y < current_surface - PLAYER_STEP_HEIGHT - STEP_EPS,
        })
    }

    /// Moves the player horizontally with the sub-stepped collision rule of the
    /// given mode, updating `player_floor_y` and (while walking) the feet line.
    ///
    /// Walking keeps the historical step rule for *rises*: a rise larger than
    /// [`PLAYER_STEP_HEIGHT`] is refused, so a cliff, a wall and a tall
    /// obstacle stay impassable. A *drop* of any size is walked off instead of
    /// refused: the player crosses the boundary, loses support and falls from
    /// the ledge. Rises and drops reason about the highest support under the
    /// candidate — the rendered floor or a solid prop top — so walking on a
    /// table stays on the table and stepping off its edge is a real fall. The
    /// height applied while still supported is the walking surface: identical
    /// to the rendered floor on ramps, regions and room floors, but the line
    /// through a staircase's nosings rather than the individual treads.
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
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] requested displacement={total_delta:?} mode={mode:?} feet={} velocity_y={}",
                self.feet_y, self.vertical_velocity
            ));
        }
        let max_step = PLAYER_RADIUS * 0.5;
        let (steps, step_count) = horizontal_step_counts(total_dist, max_step);
        let step_delta = Vec3::from_array(
            total_delta
                .to_array()
                .map(|component| component / step_count),
        );
        let previous = Vec2::new(self.player_position.x, self.player_position.z);
        let mut current_pos = previous;
        let mut current_floor = self.player_floor_y;
        let mut lost_support = false;
        // The airborne wall band follows the live foot height, which does not
        // change during a horizontal sweep: vertical motion runs after it.
        let mut live_feet = self.feet_y;
        let mut sweep_mode = mode;
        // The swimmer's leading edge is what touches a raised rim before the
        // body can cross it; the pressed-exit probe follows this direction.
        let step_dir = Vec2::new(step_delta.x, step_delta.z);

        for _ in 0..steps {
            let foot_y = match sweep_mode {
                HorizontalMode::Walk => current_floor,
                HorizontalMode::Airborne | HorizontalMode::Swim { surface_y: _ } => live_feet,
            };
            if let Some(surface_y) = self.waterline_for_exit(sweep_mode, current_pos, live_feet) {
                self.note_pressed_water_exit(current_pos, step_dir, surface_y);
            }
            let raw = Vec2::new(current_pos.x + step_delta.x, current_pos.y + step_delta.z);
            let mut candidate =
                self.resolve_horizontal_contact(current_pos, raw, foot_y, sweep_mode);
            if sweep_mode == HorizontalMode::Walk
                && candidate.distance_squared(raw) > 1e-10
                && let Some(step_y) = self.solid_step_height(current_pos, raw, foot_y)
            {
                candidate = raw;
                current_floor = step_y;
                live_feet = step_y;
            }
            if matches!(sweep_mode, HorizontalMode::Swim { surface_y: _ }) {
                // Physical body sweeps also retain the animated camera's
                // near-plane clearance while changing stance at a rim.
                candidate = self.clear_swim_camera(candidate);
            }
            // A depenetration deeper than the body radius is a teleport (the
            // centre was inside a box, or several boxes pushed at once): refuse
            // the step instead of snapping the player across the opening.
            if candidate.distance(raw) > PLAYER_RADIUS + CONTACT_EPS {
                break;
            }
            let outcome = match sweep_mode {
                HorizontalMode::Walk => self.walk_step(current_floor, current_pos, candidate),
                HorizontalMode::Airborne => self.airborne_step(candidate, live_feet),
                HorizontalMode::Swim { surface_y } => self.swim_step(candidate, surface_y),
            };
            match outcome {
                StepOutcome::Accepted { floor, dropped } => {
                    current_pos = candidate;
                    if sweep_mode == HorizontalMode::Walk {
                        if dropped {
                            lost_support = true;
                            live_feet = current_floor;
                            sweep_mode = HorizontalMode::Airborne;
                        } else {
                            live_feet = floor;
                        }
                    }
                    current_floor = floor;
                }
                StepOutcome::Refused => break,
                StepOutcome::Void => {
                    current_pos = candidate;
                    if sweep_mode == HorizontalMode::Walk {
                        lost_support = true;
                        live_feet = current_floor;
                        sweep_mode = HorizontalMode::Airborne;
                    }
                }
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
                self.set_feet_y(live_feet);
            } else {
                // Maintain the grounded line regardless of pitch: the feet
                // stand on the walking surface and the eye follows.
                self.set_feet_y(self.player_floor_y);
            }
        }
    }

    /// Swimming and deep wading can discover a real rim contact.
    fn waterline_for_exit(&self, mode: HorizontalMode, position: Vec2, feet: f32) -> Option<f32> {
        match mode {
            HorizontalMode::Swim { surface_y } => Some(surface_y),
            HorizontalMode::Walk => self
                .water
                .sample(position.x, position.y, feet)
                .filter(|water| water.swimming && water.surface_y - feet > WADE_DEPTH)
                .map(|water| water.surface_y),
            HorizontalMode::Airborne => None,
        }
    }

    /// Every mode sweeps the upright body at its physical feet.
    fn resolve_horizontal_contact(
        &self,
        from: Vec2,
        to: Vec2,
        foot_y: f32,
        mode: HorizontalMode,
    ) -> Vec2 {
        let mut height = if matches!(mode, HorizontalMode::Swim { surface_y: _ }) {
            // A crouch switches the body immediately while the eye eases;
            // retain camera clearance until that animation completes.
            self.body_height()
                .max(self.eye_offset() + WATER_EXIT_EYE_CLEARANCE_M)
        } else {
            self.body_height()
        };
        if let HorizontalMode::Swim { surface_y } = mode
            && let Some(support) = self.pressed_exit_support
            && self.water_support_reachable(support, surface_y)
            && (self.player_position.y >= surface_y - EXIT_EYE_MARGIN
                || support <= surface_y + STEP_EPS)
        {
            // Keep the whole disc outside the exit edge while pulling up.
            // Otherwise a head below a raised floor can slip underneath its
            // lip, where the underside correctly prevents further ascent.
            height = height.max(2.0_f32.mul_add(CONTACT_EPS, support - foot_y));
        }
        crate::engine::collision::sweep_horizontal(
            &self.horizontal_world(),
            (from, to),
            (foot_y, height, PLAYER_RADIUS),
            mode == HorizontalMode::Walk,
        )
    }

    fn horizontal_world(&self) -> crate::engine::collision::HorizontalWorld<'_> {
        crate::engine::collision::HorizontalWorld {
            index: &self.collision_index,
            walls: &self.walls,
            doors: self.world.door_colliders(),
            ceiling: &self.ceiling,
            floor: &self.floor,
            debug: self.movement_debug,
        }
    }

    /// Separate placement recovery, used only at spawn/reset. Ordinary
    /// movement never searches for a distant escape position. Candidates are
    /// actual solid faces and the closest fully clear one wins deterministically.
    /// Recovery is capped at two standing heights (3.6 m), even for corrupt
    /// placements; a deeply enclosed spawn cannot become an unbounded teleport.
    fn recover_spawn_overlap(&mut self) {
        let origin = Vec3::new(self.player_position.x, self.feet_y, self.player_position.z);
        if self.body_fits_at(origin) {
            return;
        }
        let mut candidates = Vec::new();
        let horizontal = crate::engine::collision::resolve_airborne_player_collision_with_doors(
            &self.collision_index,
            (Vec2::new(origin.x, origin.z), Vec2::new(origin.x, origin.z)),
            PLAYER_RADIUS,
            origin.y,
            self.body_height(),
            &self.walls,
            self.world.door_colliders(),
        );
        candidates.push(Vec3::new(horizontal.x, origin.y, horizontal.y));
        for door in self.world.door_colliders() {
            if door.overlaps_body_y(origin.y, self.body_height())
                && let Some((x, z)) =
                    door.depenetrate(origin.x, origin.z, PLAYER_RADIUS + CONTACT_EPS)
            {
                candidates.push(Vec3::new(x, origin.y, z));
            }
        }
        for wall in &self.walls {
            if wall.max_y <= origin.y + CONTACT_EPS
                || wall.min_y >= origin.y + self.body_height() - CONTACT_EPS
                || !wall.overlaps_disc(origin.x, origin.z, PLAYER_RADIUS - CONTACT_EPS)
            {
                continue;
            }
            for x in [
                wall.min_x - PLAYER_RADIUS - CONTACT_EPS,
                wall.max_x + PLAYER_RADIUS + CONTACT_EPS,
            ] {
                candidates.push(Vec3::new(x, origin.y, origin.z));
            }
            for z in [
                wall.min_z - PLAYER_RADIUS - CONTACT_EPS,
                wall.max_z + PLAYER_RADIUS + CONTACT_EPS,
            ] {
                candidates.push(Vec3::new(origin.x, origin.y, z));
            }
            candidates.push(Vec3::new(origin.x, wall.max_y + CONTACT_EPS, origin.z));
            candidates.push(Vec3::new(
                origin.x,
                wall.min_y - self.body_height() - CONTACT_EPS,
                origin.z,
            ));
        }
        self.spawn_surface_candidates(origin, &mut candidates);
        let limit = PLAYER_HEIGHT * 2.0;
        let recovered = candidates
            .into_iter()
            .filter(|candidate| {
                candidate.distance_squared(origin) <= limit * limit && self.body_fits_at(*candidate)
            })
            .min_by(|a, b| {
                a.distance_squared(origin)
                    .total_cmp(&b.distance_squared(origin))
            });
        if let Some(position) = recovered {
            if self.movement_debug {
                crate::engine::logging::warn(format_args!(
                    "[movement] spawn_recovery initial={origin:?} final={position:?} maximum={limit}"
                ));
            }
            self.player_position.x = position.x;
            self.player_position.z = position.z;
            self.set_feet_y(position.y);
        } else {
            crate::engine::logging::warn_once(
                "movement_invalid_spawn",
                "[movement] no clear spawn recovery within 3.6 m; correct the level spawn",
            );
        }
    }

    /// Vertical escape candidates belong only to explicit spawn recovery.
    fn spawn_surface_candidates(&self, origin: Vec3, candidates: &mut Vec<Vec3>) {
        if let Some(floor) =
            self.floor
                .height_below(origin.x, origin.z, origin.y + self.body_height())
        {
            candidates.push(Vec3::new(origin.x, floor, origin.z));
        }
        if let Some(ceiling) = self.head_limit(origin.y) {
            candidates.push(Vec3::new(
                origin.x,
                ceiling - self.body_height() - CONTACT_EPS,
                origin.z,
            ));
        }
        self.ceiling.for_each_body_contact(
            Vec2::new(origin.x, origin.z),
            PLAYER_RADIUS - CONTACT_EPS,
            origin.y,
            self.body_height(),
            |[low, high]| {
                candidates.push(Vec3::new(origin.x, high + CONTACT_EPS, origin.z));
                candidates.push(Vec3::new(
                    origin.x,
                    low - self.body_height() - CONTACT_EPS,
                    origin.z,
                ));
            },
        );
    }

    fn body_fits_at(&self, feet: Vec3) -> bool {
        self.body_fits_with_height(feet, self.body_height())
    }

    fn body_fits_with_height(&self, feet: Vec3, height: f32) -> bool {
        let mut clear = true;
        self.ceiling.for_each_body_contact(
            Vec2::new(feet.x, feet.z),
            PLAYER_RADIUS - CONTACT_EPS,
            feet.y,
            height,
            |_| clear = false,
        );
        self.collision_index.for_each_disc(
            feet.x,
            feet.z,
            PLAYER_RADIUS - CONTACT_EPS,
            &self.walls,
            |wall| {
                if wall.max_y > feet.y + CONTACT_EPS
                    && wall.min_y < feet.y + height - CONTACT_EPS
                    && wall.overlaps_disc(feet.x, feet.z, PLAYER_RADIUS - CONTACT_EPS)
                {
                    clear = false;
                }
            },
        );
        clear
            && !self.world.door_colliders().iter().any(|door| {
                door.overlaps_body_y(feet.y, height)
                    && door.overlaps_disc(feet.x, feet.z, PLAYER_RADIUS - CONTACT_EPS)
            })
            && self
                .head_limit_at(feet.x, feet.z, feet.y)
                .is_none_or(|ceiling| ceiling >= feet.y + height - CONTACT_EPS)
            && self
                .floor
                .height_below(feet.x, feet.z, feet.y + height - CONTACT_EPS)
                .is_none_or(|floor| floor <= feet.y + CONTACT_EPS)
    }

    /// A grounded step is an up/forward clearance test, never airborne
    /// depenetration. The upward path and destination must both fit, and all
    /// intersected solids must remain within the documented maximum height.
    fn solid_step_height(&self, from: Vec2, to: Vec2, feet: f32) -> Option<f32> {
        let mut top: Option<f32> = None;
        let mut too_tall = false;
        self.collision_index
            .for_each_disc(to.x, to.y, PLAYER_RADIUS, &self.walls, |wall| {
                if !wall.overlaps_disc(to.x, to.y, PLAYER_RADIUS)
                    || wall.max_y <= feet + CONTACT_EPS
                    || wall.min_y >= feet + self.body_height() - CONTACT_EPS
                {
                    return;
                }
                too_tall |= wall.max_y > feet + PLAYER_STEP_HEIGHT + STEP_EPS;
                top = Some(top.map_or(wall.max_y, |height| height.max(wall.max_y)));
            });
        let accepted_top = top.filter(|_| !too_tall);
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] step_attempt from={from:?} to={to:?} feet={feet} maximum={PLAYER_STEP_HEIGHT} top={accepted_top:?} rejected_tall={too_tall}"
            ));
        }
        let step_top = accepted_top?;
        if [from, to].into_iter().any(|point| {
            self.head_limit_at(point.x, point.y, feet)
                .is_some_and(|limit| limit < step_top + self.body_height() - CONTACT_EPS)
        }) {
            if self.movement_debug {
                crate::engine::logging::warn("[movement] step_rejected reason=head_clearance");
            }
            return None;
        }
        let resolved = crate::engine::collision::sweep_horizontal(
            &self.horizontal_world(),
            (from, to),
            (step_top, self.body_height(), PLAYER_RADIUS),
            false,
        );
        let accepted = resolved.distance_squared(to) <= 1e-10;
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] step_result accepted={accepted} top={step_top} resolved={resolved:?}"
            ));
        }
        accepted.then_some(step_top)
    }

    /// Resolves one walking sub-step through [`Self::walking_support_at`]:
    /// rises stay bounded by the step rule (a floor, a prop top or a wall
    /// within one walkable step is stepped onto), a drop of any size is
    /// accepted and reported as lost support. Unsupported space is crossable
    /// and starts a fall from the last supported height.
    fn walk_step(&self, current_surface: f32, previous: Vec2, candidate: Vec2) -> StepOutcome {
        match self.walking_support_at(candidate.x, candidate.y, current_surface) {
            Some(support)
                if !support.dropped
                    && self
                        .head_limit_at(candidate.x, candidate.y, support.surface_y)
                        .is_some_and(|limit| {
                            limit < support.surface_y + self.body_height() - CONTACT_EPS
                        }) =>
            {
                StepOutcome::Refused
            }
            Some(support) => {
                // A stair's pitch line can sit above its real tread. At the
                // foot, compare the drop against that tread, otherwise a legal
                // maximum-height riser plus one pitch sample starts a fall.
                // Only use it when the feet follow the walking surface: a prop
                // above the floor must still lose support at its own edge.
                let dropped = support.dropped
                    && !self
                        .floor
                        .walk_height_at(previous.x, previous.y)
                        .filter(|height| (*height - current_surface).abs() <= STEP_EPS)
                        .and_then(|_| self.floor.height_at(previous.x, previous.y))
                        .is_some_and(|height| {
                            support.surface_y >= height - PLAYER_STEP_HEIGHT - STEP_EPS
                        });
                StepOutcome::Accepted {
                    floor: support.surface_y,
                    dropped,
                }
            }
            None if self.floor.height_at(candidate.x, candidate.y).is_some() => {
                StepOutcome::Refused
            }
            None => StepOutcome::Void,
        }
    }

    /// Resolves one airborne sub-step: a floor above the live feet refuses the
    /// step, the void and any lower floor are crossed.
    fn airborne_step(&self, candidate: Vec2, live_feet: f32) -> StepOutcome {
        match self
            .floor
            .height_below(candidate.x, candidate.y, live_feet + CONTACT_EPS)
        {
            Some(y) if y <= live_feet + STEP_EPS => StepOutcome::Accepted {
                floor: self
                    .floor
                    .walk_height_below(candidate.x, candidate.y, live_feet + CONTACT_EPS)
                    .unwrap_or(y),
                dropped: false,
            },
            Some(_) => StepOutcome::Refused,
            None => StepOutcome::Void,
        }
    }

    /// Resolves one swimming sub-step: any real walkable floor at or below the
    /// surface is reachable, and while the eye is near the surface a floor up
    /// to [`WATER_EXIT_STEP_M`] above it is too — that is the bounded step-up
    /// onto a pool deck. A higher floor is a ledge the swimmer cannot cross,
    /// the allowance never applies deep underwater, and the void is open water.
    ///
    /// Only the walkable floor is considered: a solid prop or wall is never a
    /// water exit, and the wall pass (which real solids always block) is what
    /// keeps a barrier at the water's edge solid.
    fn swim_step(&self, candidate: Vec2, surface_y: f32) -> StepOutcome {
        let ceiling = if self.player_position.y >= surface_y - EXIT_EYE_MARGIN {
            surface_y + WATER_EXIT_STEP_M
        } else {
            surface_y
        };
        match self.floor.walk_height_at(candidate.x, candidate.y) {
            Some(y) if y <= ceiling + STEP_EPS && y <= self.feet_y + CONTACT_EPS => {
                StepOutcome::Accepted {
                    floor: y,
                    dropped: false,
                }
            }
            Some(_) => StepOutcome::Refused,
            None => StepOutcome::Void,
        }
    }

    /// Records a raised water exit the swimmer is pressing into this step.
    ///
    /// The body is what touches a rim before the centre can cross it: the
    /// probe follows the leading edge and also each rim the disc is actually
    /// against (a diagonal approach into a corner touches two rims without the
    /// leading edge reaching either). A candidate floor is only an exit when
    /// the point itself is free space — a wall standing on a floor is a
    /// barrier, never a climbable pool edge. The record is only set while the
    /// movement points into that edge. Parallel travel and side-current
    /// pressure cannot claim a ledge in the player's forward direction.
    /// The physical band retains diagonal corner contacts.
    fn note_pressed_water_exit(&mut self, position: Vec2, step_dir: Vec2, surface_y: f32) {
        // The indexed disc query collects the rim contact points; the exit
        // floor validation runs afterwards, because validating a floor asks
        // the index for the stance's head clearance and an index query is not
        // re-entrant (its scratch borrow is held across the visitor). The
        // candidate list is one leading probe plus the rims the disc touches,
        // so the buffer stays tiny.
        let mut probes = std::mem::take(&mut self.water_exit_probes);
        probes.clear();
        if step_dir.length_squared() > 0.0 {
            let probe_dir = step_dir.normalize();
            probes.push(Vec2::new(
                probe_dir.x.mul_add(EXIT_PRESS_PROBE_M, position.x),
                probe_dir.y.mul_add(EXIT_PRESS_PROBE_M, position.y),
            ));
        }
        let feet = self.feet_y;
        let head = feet + self.body_height();
        self.collision_index.for_each_disc(
            position.x,
            position.y,
            EXIT_PRESS_PROBE_M,
            &self.walls,
            |wall| {
                if wall.min_y >= head + EXIT_PRESS_MARGIN_M - CONTACT_EPS
                    || wall.max_y <= feet - EXIT_PRESS_MARGIN_M + CONTACT_EPS
                {
                    return;
                }
                if !wall.overlaps_disc(position.x, position.y, EXIT_PRESS_PROBE_M) {
                    return;
                }
                let closest_x = position.x.clamp(wall.min_x, wall.max_x);
                let closest_z = position.y.clamp(wall.min_z, wall.max_z);
                let dx = closest_x - position.x;
                let dz = closest_z - position.y;
                let length = dx.hypot(dz);
                // A near-tangent corner of a segmented bank is side contact.
                // The approach cone still admits ordinary diagonal exits.
                if length <= CONTACT_EPS
                    || dx.mul_add(step_dir.x, dz * step_dir.y) <= 0.6 * length * step_dir.length()
                {
                    return;
                }
                probes.push(Vec2::new(
                    (dx / length).mul_add(EXIT_PRESS_PROBE_M, position.x),
                    (dz / length).mul_add(EXIT_PRESS_PROBE_M, position.y),
                ));
            },
        );
        let mut best: Option<f32> = None;
        for probe in &probes {
            if let Some(floor) = self.exit_probe_floor(*probe, surface_y) {
                best = Some(best.map_or(floor, |current| current.max(floor)));
            }
        }
        if let Some(floor) = best {
            self.pressed_exit_support = Some(floor);
        }
        self.water_exit_probes = probes;
    }

    /// The standable exit floor at `probe`, if the point is free space.
    ///
    /// The walkable floor under a wall is not an exit: the point must not be
    /// inside a wall or solid prop that would block a body standing on the
    /// candidate floor. This is what keeps a pool wall from ever being pulled
    /// up as if it were a deck edge.
    fn exit_probe_floor(&self, probe: Vec2, surface_y: f32) -> Option<f32> {
        let floor = self.floor.walk_height_at(probe.x, probe.y)?;
        if !floor.is_finite() || !self.water_support_reachable(floor, surface_y) {
            return None;
        }
        let body_height = self.body_height();
        let solid = self.walls.iter().any(|wall| {
            wall.overlaps_disc(probe.x, probe.y, CONTACT_EPS)
                && (wall.blocks_body(floor, body_height)
                    || (wall.step_up <= 0.0
                        && wall.max_y >= floor - CONTACT_EPS
                        && wall.min_y < floor + body_height))
        });
        if solid {
            return None;
        }
        let door = self.world.door_colliders().iter().any(|door| {
            door.overlaps_disc(probe.x, probe.y, CONTACT_EPS)
                && door.hinge_y + CONTACT_EPS < floor + body_height
                && door.hinge_y + door.height > floor + STEP_EPS
        });
        if door {
            return None;
        }
        Some(floor)
    }

    /// Pushes the swimmer's disc out of any solid whose vertical span is
    /// within the projection near plane of the rendered eye.
    ///
    /// A raised rim is a step the swimmer may cross, but only once the camera
    /// has cleared its top by [`WATER_EXIT_EYE_CLEARANCE_M`]: while the eye is
    /// within that clearance of the rim top (below it, or less than one near
    /// plane above it) the disc keeps a body radius from the rim, so the climb
    /// can lift the eye at the rim instead of carrying the rendered camera
    /// inside the floor's own collision volume. The push is the ordinary disc
    /// depenetration; the caller still refuses a correction deeper than the
    /// body radius, like every other step.
    fn clear_swim_camera(&self, mut position: Vec2) -> Vec2 {
        let eye = self.player_position.y;
        // The comparison carries the contact tolerance: the pull-up's hold
        // line and the clearance are computed through an eye-offset round trip,
        // so an exact equality must count as cleared, not as a hair inside.
        let vertical_clear = |min_y: f32, max_y: f32| {
            min_y >= eye + WATER_EXIT_EYE_CLEARANCE_M - CONTACT_EPS
                || max_y <= eye - WATER_EXIT_EYE_CLEARANCE_M + CONTACT_EPS
        };
        for _ in 0_i32..4_i32 {
            let mut collided = false;
            self.collision_index.for_each_disc(
                position.x,
                position.y,
                PLAYER_RADIUS,
                &self.walls,
                |wall| {
                    if vertical_clear(wall.min_y, wall.max_y) {
                        return;
                    }
                    if let Some(next) = push_disc_out(position, PLAYER_RADIUS, wall) {
                        position = next;
                        collided = true;
                    }
                },
            );
            for door in self.world.door_colliders() {
                if vertical_clear(door.hinge_y, door.hinge_y + door.height) {
                    continue;
                }
                if let Some((x, z)) = door.depenetrate(position.x, position.y, PLAYER_RADIUS) {
                    position = Vec2::new(x, z);
                    collided = true;
                }
            }
            if !collided {
                break;
            }
        }
        position
    }

    /// Vertical step for a grounded or airborne player: the grounded snap, a
    /// launch on a fresh Jump press, and bounded-interval gravity integration
    /// while airborne.
    fn land_vertical(&mut self, jump_pressed: bool) {
        if self.grounded {
            self.refresh_ground_support();
        }
        if self.grounded {
            self.set_feet_y(self.player_floor_y);
            self.vertical_velocity = 0.0;
            self.vertical_accumulator = 0.0;
            if !jump_pressed {
                return;
            }
            self.vertical_velocity = JUMP_VELOCITY;
            self.grounded = false;
        }

        self.vertical_accumulator = 0.0;
        let _vertical_step = self.integrate_vertical_substep(self.sim_delta_seconds);
    }

    /// Advances the airborne player through one bounded simulation interval.
    ///
    /// The feet are the integrated coordinate; the rendered eye is always
    /// `feet + eye_offset_current`. The position uses the average of the
    /// substep's start and end velocities (the trapezoidal form), which
    /// reproduces the exact ballistic parabola at every substep boundary: the
    /// apex is therefore frame-rate independent rather than depending on where
    /// a frame boundary lands.
    ///
    /// Ceiling impact time is solved against room planes and overhead solids,
    /// consuming the incoming upward velocity and applying gravity for the
    /// remaining interval. Landing checks the highest support beneath the
    /// body footprint, including a prop edge touched by the disc.
    ///
    /// Returns what the substep resolved to: still airborne, landed, or a
    /// ceiling bump.
    fn integrate_vertical_substep(&mut self, delta: f32) -> VerticalStep {
        let height = self.body_height();
        let start_feet = self.feet_y;
        let initial_velocity = self.vertical_velocity;
        let next_velocity = GRAVITY.mul_add(-delta, initial_velocity);
        let rise = f32::midpoint(initial_velocity, next_velocity) * delta;
        let peak_rise = if initial_velocity > 0.0 && next_velocity < 0.0 {
            initial_velocity * initial_velocity / (2.0 * GRAVITY)
        } else {
            rise.max(0.0)
        };
        self.vertical_velocity = next_velocity;
        let mut feet = self.feet_y + rise;
        let mut bumped = false;

        // Ceiling and overhead boxes: the top of the head (`feet + height`) is
        // what bumps, and the upward velocity is consumed by the impact instead
        // of being applied again. Clamping a hair under the limit keeps the
        // horizontal pass from re-reading the same box as a wall at the
        // contact plane.
        if initial_velocity > 0.0
            && let Some(limit) = self.head_limit(start_feet)
            && start_feet + height <= limit + CONTACT_EPS
            && start_feet + peak_rise + height > limit - CONTACT_EPS
        {
            let contact_feet = limit - height - CONTACT_EPS;
            let clearance = (contact_feet - start_feet).max(0.0);
            let impact_velocity = (initial_velocity
                .mul_add(initial_velocity, -2.0 * GRAVITY * clearance))
            .max(0.0)
            .sqrt();
            let impact_time =
                (2.0 * clearance / (initial_velocity + impact_velocity)).clamp(0.0, delta);
            let remaining = delta - impact_time;
            self.vertical_velocity = -GRAVITY * remaining;
            feet = (-0.5 * GRAVITY * remaining).mul_add(remaining, contact_feet);
            bumped = true;
            self.ceiling_contact_this_frame = true;
            if self.movement_debug {
                crate::engine::logging::warn(format_args!(
                    "[movement] ceiling_hit limit={limit} initial_feet={start_feet} impact_time={impact_time} final_feet={feet} normal=(0,-1,0) velocity_y={}",
                    self.vertical_velocity
                ));
            }
        }
        self.set_feet_y(feet);

        // Landing: only while descending, on the highest support under the
        // body that is at or below the feet at the substep's start. Only an
        // empty-floor world has a fallback Y=0 plane; an authored room's open
        // edge never acquires invisible support.
        if self.vertical_velocity <= 0.0
            && let Some(support) =
                self.support_at(self.player_position.x, self.player_position.z, start_feet)
            && feet <= support + CONTACT_EPS
        {
            self.set_feet_y(support);
            self.player_floor_y = support;
            self.vertical_velocity = 0.0;
            self.grounded = true;
            if self.movement_debug {
                crate::engine::logging::warn(format_args!(
                    "[movement] support_hit height={support} initial_feet={start_feet} final_feet={feet} normal=(0,1,0) classification=floor"
                ));
            }
            return VerticalStep::Landed;
        }
        if bumped {
            VerticalStep::Bumped
        } else {
            VerticalStep::Airborne
        }
    }

    /// Ground is a current supporting surface, never a cached collision flag.
    /// Only numerical contact tolerance may be corrected here; steps and spawn
    /// recovery have separate, explicitly bounded paths.
    fn refresh_ground_support(&mut self) {
        let was_grounded = self.grounded;
        let x = self.player_position.x;
        let z = self.player_position.z;
        let support = self
            .walking_support_at(x, z, self.feet_y)
            .map(|support| support.surface_y)
            .filter(|height| (height - self.feet_y).abs() <= CONTACT_EPS)
            .or_else(|| self.support_at(x, z, self.feet_y));
        self.grounded = self.vertical_velocity <= 0.0
            && support.is_some_and(|height| (height - self.feet_y).abs() <= CONTACT_EPS)
            && self.head_clear_for(self.feet_y, self.body_height());
        if self.movement_debug && was_grounded != self.grounded {
            crate::engine::logging::warn(format_args!(
                "[movement] grounded_transition {was_grounded}->{} support={support:?} feet={} velocity_y={}",
                self.grounded, self.feet_y, self.vertical_velocity
            ));
        }
        if self.grounded {
            if let Some(height) = support {
                self.player_floor_y = height;
                self.set_feet_y(height);
            }
            self.vertical_velocity = 0.0;
        }
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
                // one, and let normal physics own the frame. Any water climb
                // this attachment superseded is stale.
                self.climbing = None;
                self.grounded = false;
                self.swimming = false;
                self.water_exit = None;
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
        self.water_exit = None;
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
        let height = self.body_height();
        let feet = self.feet_y;
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
        let clamped_target = target.min(ladder.top_y).max(feet);
        self.set_feet_y(clamped_target);
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;

        // Safe top landing: once the feet reach the authored top, a real
        // walkable surface within a step of them takes over. This is a normal
        // grounded transition (the deck under the player), never a snap
        // through the rim: the feet are already at the top.
        if clamped_target >= ladder.top_y - STEP_EPS
            && let Some(support) = self
                .floor
                .walk_height_at(self.player_position.x, self.player_position.z)
            && (support - clamped_target).abs() <= PLAYER_STEP_HEIGHT + STEP_EPS
            && self.head_clear_for(support, height)
        {
            self.player_floor_y = support;
            self.set_feet_y(support);
            self.grounded = true;
            self.swimming = false;
            self.water_exit = None;
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
/// Collision subdivision count and its exact floating point divisor.
///
/// The million-step ceiling preserves the existing cap, stays exactly
/// representable in f32 and fits every supported usize target. Non-finite
/// NaN displacements retain the float cast's zero-step fallback.
fn horizontal_step_counts(distance: f32, max_step: f32) -> (usize, f32) {
    let divisor = (distance / max_step).ceil().clamp(1.0, 1_048_576.0);
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Non-NaN counts are integral and clamped to 1..=1048576; Rust's float cast maps NaN to zero steps."
    )]
    let count = divisor as usize;
    (count, divisor)
}

/// input so a released key never counts as intent.
fn normalized_climb_intent(move_dir: Vec3, ladder: &Ladder) -> f32 {
    if move_dir.length_squared() <= 0.0 {
        return 0.0;
    }
    let normalized = move_dir.normalize();
    ladder.climb_intent(normalized.x, normalized.z)
}

/// Pushes a body disc out of one wall box, or `None` when it does not touch.
///
/// The same circle-versus-box contact rule the collision resolver applies;
/// kept beside the swim camera clearance so that pass needs no second public
/// collision entry point. The resolver's own helper is private to its module
/// and its step allowance is exactly what the camera clearance must ignore.
fn push_disc_out(position: Vec2, radius: f32, wall: &WallAabb) -> Option<Vec2> {
    let closest_x = position.x.clamp(wall.min_x, wall.max_x);
    let closest_z = position.y.clamp(wall.min_z, wall.max_z);
    let dx = position.x - closest_x;
    let dz = position.y - closest_z;
    let dist_sq = dx.mul_add(dx, dz * dz);
    if dist_sq >= radius * radius {
        return None;
    }
    if dist_sq > 1e-6 {
        let dist = dist_sq.sqrt();
        let penetration = radius - dist;
        return Some(Vec2::new(
            dx.mul_add(penetration / dist, position.x),
            dz.mul_add(penetration / dist, position.y),
        ));
    }
    // Centre inside or exactly on the box boundary: push out of the nearest
    // face, the resolver's own fallback.
    let d_left = (position.x - wall.min_x).abs();
    let d_right = (wall.max_x - position.x).abs();
    let d_near = (position.y - wall.min_z).abs();
    let d_far = (wall.max_z - position.y).abs();
    let min_d = d_left.min(d_right).min(d_near).min(d_far);
    if (min_d - d_left).abs() < 1e-5 {
        Some(Vec2::new(wall.min_x - radius, position.y))
    } else if (min_d - d_right).abs() < 1e-5 {
        Some(Vec2::new(wall.max_x + radius, position.y))
    } else if (min_d - d_near).abs() < 1e-5 {
        Some(Vec2::new(position.x, wall.min_z - radius))
    } else {
        Some(Vec2::new(position.x, wall.max_z + radius))
    }
}

#[cfg(test)]
mod tests;

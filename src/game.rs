use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::collision::{
    PLAYER_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, STEP_EPS, WallAabb, resolve_player_collision,
};
use crate::input::{Control, InputState};
use crate::level::{
    LevelDef, LevelSurfaces, WalkableCeiling, WalkableFloor, WaterSample, WaterVolumes,
};
use crate::settings::Settings;

pub const TWO_PI: f32 = std::f32::consts::TAU;
pub const EYE_HEIGHT: f32 = 1.6;
pub const MAX_PITCH: f32 = 1.4835; // ~85 degrees in radians

/// Downward acceleration applied to the vertical velocity while airborne, in
/// m/s^2.
pub const GRAVITY: f32 = 18.0;

/// Height of a full jump's apex above the take-off floor, in metres.
///
/// The office desk's top surface at scale 1 in the demo, which a standing jump
/// is meant to clear.
pub const JUMP_APEX_M: f32 = 0.75;

/// Take-off speed of a jump, in m/s: the f32 value of
/// `sqrt(2.0 * GRAVITY * JUMP_APEX_M)` (27.0, whose square root is not
/// available in a `const` initializer). A test re-derives it from the formula.
pub const JUMP_VELOCITY: f32 = 5.196_152;

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
/// floor.
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

/// Water depth below the surface within which the player can stand up out of
/// the water, in metres.
///
/// Equal to [`WADE_DEPTH`], so the thresholds are disjoint: exiting puts the
/// feet in water shallow enough that the swim state cannot re-trigger.
pub const EXIT_DEPTH: f32 = 0.55;

/// How far below the water surface the eye may be and still stand up, in
/// metres.
const EXIT_EYE_MARGIN: f32 = 0.6;

/// Eye height above which the swimming pose is the surface pose, in metres.
const SURFACE_SWIM_EYE_MARGIN: f32 = 0.25;

/// Distance from the eye to the top of the head, in metres.
const HEAD_OFFSET: f32 = PLAYER_HEIGHT - EYE_HEIGHT;

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

/// What a character animation system needs to know about the player.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LocomotionSnapshot {
    pub state: LocomotionState,
    /// Actual horizontal speed for the frame, in m/s.
    pub speed: f32,
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
    /// World Y of the eye. `player_floor_y + EYE_HEIGHT` while grounded and
    /// while walking; off that line while airborne or swimming.
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
    /// Vertical speed in m/s, positive upward. Zero while grounded.
    pub vertical_velocity: f32,
    /// True while the player stands on the walkable floor.
    pub grounded: bool,
    /// The locomotion pose and speed reported to animation, refreshed by every
    /// Playing update.
    locomotion: LocomotionSnapshot,
    /// True while the water under the player is deep enough to swim.
    swimming: bool,
    /// Set on the first frame Jump is held and cleared on release: one press is
    /// one jump, and landing (or standing up in water) while holding the key
    /// never bounces.
    jump_latched: bool,
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
    /// Feet on the walkable floor: the historical step rule applies.
    Walk,
    /// Off the floor: a floor above the live feet, and the void, refuse the
    /// step; wall collision uses the live foot height.
    Airborne,
    /// Floating: wall collision uses the band one step below the surface and
    /// any floor at or below the surface is reachable.
    Swim { surface_y: f32 },
}

impl Game {
    #[must_use]
    pub fn new(
        spawn_pos: Vec3,
        spawn_yaw: f32,
        walls: Vec<WallAabb>,
        floor: WalkableFloor,
        water: WaterVolumes,
        ceiling: WalkableCeiling,
    ) -> Self {
        let grounded = floor.walk_height_at(spawn_pos.x, spawn_pos.z).is_some();
        Self {
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
            walls,
            floor,
            ceiling,
            water,
            vertical_velocity: 0.0,
            grounded,
            locomotion: LocomotionSnapshot::default(),
            swimming: false,
            jump_latched: false,
            vertical_accumulator: 0.0,
            bob_phase: 0.0,
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
    /// ceilings and water when loading a level.
    ///
    /// The player spawns grounded when a walkable floor exists under the spawn
    /// point; outside every room a legacy spawn settles on the historical floor
    /// at its own height on the first update. Vertical velocity starts at zero
    /// and the jump latch is released.
    pub fn reset_level(
        &mut self,
        spawn_pos: Vec3,
        spawn_yaw: f32,
        walls: Vec<WallAabb>,
        floor: WalkableFloor,
        water: WaterVolumes,
        ceiling: WalkableCeiling,
    ) {
        self.player_floor_y = spawn_pos.y - EYE_HEIGHT;
        self.player_position = spawn_pos;
        self.player_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.player_pitch = 0.0;
        self.walls = walls;
        self.floor = floor;
        self.water = water;
        self.ceiling = ceiling;
        self.vertical_velocity = 0.0;
        self.vertical_accumulator = 0.0;
        self.grounded = self
            .floor
            .walk_height_at(spawn_pos.x, spawn_pos.z)
            .is_some();
        self.swimming = false;
        self.jump_latched = false;
        self.bob_phase = 0.0;
        self.locomotion = LocomotionSnapshot::default();
        self.last_frame_time = Instant::now();
        self.delta_seconds = 0.0;
        self.sim_delta_seconds = 0.0;
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

    /// Updates first-person movement, look, jumping and swimming for one frame.
    ///
    /// Keyboard look stays delta-time scaled; relative mouse motion is applied
    /// in pixels (no delta-time factor) at [`Settings::mouse_sensitivity`].
    /// Vertical motion integrates in fixed [`VERTICAL_SUBSTEP`] steps, and
    /// horizontal collision switches between the historical walking step rule,
    /// an airborne rule (no floors above the feet, no moving over the void) and
    /// the swimming band.
    pub fn update_player_movement(&mut self, input: &mut InputState, settings: &Settings) {
        // The frame's relative motion is always consumed, even while paused or
        // in a menu, so motion collected around a pause is never applied later.
        let motion = input.take_mouse_motion();

        // While paused or in menus, do not update player movement or looking
        if self.app_state != AppState::Playing {
            return;
        }

        let delta = self.sim_delta_seconds;
        self.update_look(input, settings, motion, delta);

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

        // Deep water at the feet decides this frame's horizontal mode and
        // speed; the post-move sample decides the vertical behaviour.
        let feet = self.player_position.y - EYE_HEIGHT;
        let sample = self
            .water
            .sample(self.player_position.x, self.player_position.z, feet);
        let wet = sample.is_some_and(|s| s.swimming && s.surface_y - feet > WADE_DEPTH);

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

        // The water at the post-move position decides the vertical behaviour.
        let feet = self.player_position.y - EYE_HEIGHT;
        let sample = self
            .water
            .sample(self.player_position.x, self.player_position.z, feet);
        let swimming_now = sample.is_some_and(|s| s.swimming && s.surface_y - feet > WADE_DEPTH);

        if self.swimming && swimming_now {
            if let Some(sample) = sample {
                self.swim_vertical(sample, jump_held);
            }
        } else if self.swimming {
            // The water ended or became shallow under the player.
            self.leave_water();
        } else if swimming_now {
            // Entering deep water: the swimmer keeps no carry-over velocity and
            // rises to the float line on the next hold.
            self.swimming = true;
            self.grounded = false;
            self.vertical_velocity = 0.0;
            self.vertical_accumulator = 0.0;
            if let Some(sample) = sample {
                self.swim_vertical(sample, jump_held);
            }
        } else {
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

    /// Moves the player horizontally with the sub-stepped collision rule of the
    /// given mode, updating `player_floor_y` and (while walking) the eye line.
    ///
    /// Walking keeps the historical step rule, where *reachability* is decided
    /// on the rendered floor -- the geometry the player's feet can actually
    /// step over -- exactly as it always was: a rise or drop larger than
    /// `PLAYER_STEP_HEIGHT` is refused, which keeps a cliff edge, a wall and a
    /// tall obstacle impassable. The height *applied* is the walking surface:
    /// identical to the rendered floor on ramps, regions and room floors, but
    /// the line through a staircase's nosings rather than the individual
    /// treads. The player therefore rises and falls continuously from one tread
    /// to the next instead of the eye jumping a whole riser at every boundary,
    /// while the rendered treads stay stepped. The walking surface never leaves
    /// the tread underfoot (it meets the render at every nosing) and never
    /// rises above the next tread, so the feet can be neither inside a step nor
    /// floating over the one ahead.
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
        // The airborne wall band follows the live foot height, which does not
        // change during a horizontal sweep: vertical motion runs after it.
        let live_feet = self.player_position.y - EYE_HEIGHT;

        for _ in 0..steps {
            let foot_y = match mode {
                HorizontalMode::Walk => current_floor,
                HorizontalMode::Airborne => live_feet,
                HorizontalMode::Swim { surface_y } => surface_y - PLAYER_STEP_HEIGHT,
            };
            let candidate = resolve_player_collision(
                Vec2::new(current_pos.x + step_delta.x, current_pos.y + step_delta.z),
                PLAYER_RADIUS,
                foot_y,
                &self.walls,
            );
            match mode {
                HorizontalMode::Walk => {
                    let current_rendered = self
                        .floor
                        .height_at(current_pos.x, current_pos.y)
                        .unwrap_or(current_floor);
                    match self.floor.height_at(candidate.x, candidate.y) {
                        Some(y)
                            if (y - current_rendered).abs() <= PLAYER_STEP_HEIGHT + STEP_EPS =>
                        {
                            current_pos = candidate;
                            // Stand on the walking surface. It equals the
                            // rendered floor everywhere except on a staircase,
                            // where it is within one riser of it by
                            // construction.
                            current_floor = self
                                .floor
                                .walk_height_at(candidate.x, candidate.y)
                                .unwrap_or(y);
                            on_a_floor = true;
                        }
                        Some(_) => break,
                        None => {
                            // Outside every room: keep the historical freedom
                            // to walk over the void, but never step off a real
                            // floor into it.
                            if on_a_floor {
                                break;
                            }
                            current_pos = candidate;
                        }
                    }
                }
                HorizontalMode::Airborne => {
                    // A jump may not clip into a platform ahead or leave the
                    // world: any floor above the live feet, and the void
                    // itself, refuse the step.
                    match self.floor.height_at(candidate.x, candidate.y) {
                        Some(y) if y <= live_feet + STEP_EPS => {
                            current_pos = candidate;
                            current_floor = self
                                .floor
                                .walk_height_at(candidate.x, candidate.y)
                                .unwrap_or(y);
                        }
                        Some(_) | None => break,
                    }
                }
                HorizontalMode::Swim { surface_y } => {
                    // Any floor at or below the surface is reachable; a floor
                    // above it is a ledge the swimmer cannot cross. The wall
                    // band one step below the surface makes a rink at surface
                    // level climbable while taller rims stay solid.
                    match self.floor.height_at(candidate.x, candidate.y) {
                        Some(y) if y <= surface_y + STEP_EPS => {
                            current_pos = candidate;
                            current_floor = self
                                .floor
                                .walk_height_at(candidate.x, candidate.y)
                                .unwrap_or(y);
                        }
                        Some(_) => break,
                        None => current_pos = candidate,
                    }
                }
            }
        }
        self.player_floor_y = current_floor;
        self.player_position.x = current_pos.x;
        self.player_position.z = current_pos.y;
        if mode == HorizontalMode::Walk {
            // Maintain grounded eye height regardless of pitch
            self.player_position.y = self.player_floor_y + EYE_HEIGHT;
        }
    }

    /// Vertical step for a grounded or airborne player: the historical floor
    /// snap while grounded, a launch on a fresh Jump press, and fixed-substep
    /// gravity integration while airborne.
    fn land_vertical(&mut self, jump_pressed: bool) {
        if self.grounded {
            self.player_position.y = self.player_floor_y + EYE_HEIGHT;
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
    /// Returns what the substep resolved to: still airborne, landed, or a
    /// ceiling bump.
    fn integrate_vertical_substep(&mut self) -> VerticalStep {
        let next_velocity = GRAVITY.mul_add(-VERTICAL_SUBSTEP, self.vertical_velocity);
        let rise = f32::midpoint(self.vertical_velocity, next_velocity) * VERTICAL_SUBSTEP;
        self.player_position.y += rise;
        self.vertical_velocity = next_velocity;

        // Ceiling: the top of the head is what bumps, and the upward velocity
        // is consumed by the impact instead of being applied again.
        let mut bumped = false;
        if let Some(ceiling) = self
            .ceiling
            .ceiling_y_at(self.player_position.x, self.player_position.z)
        {
            let max_eye = ceiling - HEAD_OFFSET;
            if self.player_position.y > max_eye {
                self.player_position.y = max_eye;
                if self.vertical_velocity > 0.0 {
                    self.vertical_velocity = 0.0;
                    bumped = true;
                }
            }
        }

        // Landing: only while descending, on the rendered walkable surface.
        // Outside every room (a legacy spawn or the void the historical
        // controller let the player walk over) the last known floor stands in,
        // so an off-room spawn never falls forever.
        if self.vertical_velocity <= 0.0 {
            let support = self
                .floor
                .walk_height_at(self.player_position.x, self.player_position.z)
                .unwrap_or(self.player_floor_y);
            let feet = self.player_position.y - EYE_HEIGHT;
            if feet <= support + STEP_EPS {
                self.player_position.y = support + EYE_HEIGHT;
                self.player_floor_y = support;
                self.vertical_velocity = 0.0;
                self.grounded = true;
                return VerticalStep::Landed;
            }
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

        // The head may still bump a ceiling while rising in water.
        if let Some(ceiling) = self
            .ceiling
            .ceiling_y_at(self.player_position.x, self.player_position.z)
        {
            let max_eye = ceiling - HEAD_OFFSET;
            if self.player_position.y > max_eye {
                self.player_position.y = max_eye;
                if self.vertical_velocity > 0.0 {
                    self.vertical_velocity = 0.0;
                }
            }
        }

        // Exit: the walkable floor is within standing depth of the surface and
        // the eye is near the top of the water. The exited feet are shallow
        // enough that [`WADE_DEPTH`] cannot immediately re-enter swimming, so a
        // pool edge never oscillates.
        let can_stand = floor.is_some_and(|support| {
            support <= surface_y + STEP_EPS
                && surface_y - support <= EXIT_DEPTH
                && self.player_position.y >= surface_y - EXIT_EYE_MARGIN
        });
        if let Some(support) = floor.filter(|_| can_stand) {
            self.player_floor_y = support;
            self.player_position.y = support + EYE_HEIGHT;
            self.vertical_velocity = 0.0;
            self.grounded = true;
            self.swimming = false;
            self.bob_phase = 0.0;
        } else {
            self.player_floor_y = floor.unwrap_or(self.player_floor_y);
        }
    }

    /// Leaves the swimming state when the water under the player ends or
    /// becomes shallow: stand on the floor when it is within a step of the
    /// feet, otherwise fall.
    fn leave_water(&mut self) {
        self.swimming = false;
        self.bob_phase = 0.0;
        let feet = self.player_position.y - EYE_HEIGHT;
        let support = self
            .floor
            .walk_height_at(self.player_position.x, self.player_position.z)
            .filter(|support| feet <= support + PLAYER_STEP_HEIGHT + STEP_EPS);
        if let Some(support) = support {
            self.player_floor_y = support;
            self.player_position.y = support + EYE_HEIGHT;
            self.vertical_velocity = 0.0;
            self.grounded = true;
        } else {
            self.grounded = false;
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
                    eye - EYE_HEIGHT,
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

#[cfg(test)]
mod tests;

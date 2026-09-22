use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::collision::{PLAYER_RADIUS, PLAYER_STEP_HEIGHT, WallAabb, resolve_player_collision};
use crate::input::{Control, InputState};
use crate::level::{LevelDef, LevelSurfaces, WalkableFloor};
use crate::settings::Settings;

pub const TWO_PI: f32 = std::f32::consts::TAU;
pub const EYE_HEIGHT: f32 = 1.6;
pub const MAX_PITCH: f32 = 1.4835; // ~85 degrees in radians

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
pub struct Game {
    running: bool,
    app_state: AppState,
    last_frame_time: Instant,
    /// Real elapsed time since the previous frame (used for FPS measurement).
    delta_seconds: f32,
    /// Clamped delta used for gameplay simulation (see [`MAX_SIM_DELTA`]).
    sim_delta_seconds: f32,
    frame_count: u64,
    /// World Y of the eye. Always `player_floor_y + EYE_HEIGHT`.
    pub player_position: Vec3,
    /// World Y of the walkable floor the player is standing on. This is the
    /// value collision filters against and the value the step rule updates, so
    /// the camera and the collision band always agree about the local floor.
    pub player_floor_y: f32,
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub walls: Vec<WallAabb>,
    /// The level's walkable floor surfaces (rooms + local floor regions).
    pub floor: WalkableFloor,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            0.0,
            Vec::new(),
            WalkableFloor::default(),
        )
    }
}

impl Game {
    #[must_use]
    pub fn new(
        spawn_pos: Vec3,
        spawn_yaw: f32,
        walls: Vec<WallAabb>,
        floor: WalkableFloor,
    ) -> Self {
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

    /// Resets player position, orientation, collision walls and walkable floor
    /// when loading a level.
    pub fn reset_level(
        &mut self,
        spawn_pos: Vec3,
        spawn_yaw: f32,
        walls: Vec<WallAabb>,
        floor: WalkableFloor,
    ) {
        self.player_floor_y = spawn_pos.y - EYE_HEIGHT;
        self.player_position = spawn_pos;
        self.player_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.player_pitch = 0.0;
        self.walls = walls;
        self.floor = floor;
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
        self.delta_seconds = (now - self.last_frame_time).as_secs_f32();
        self.sim_delta_seconds = clamp_sim_delta(self.delta_seconds);
        self.last_frame_time = now;
        self.frame_count = self.frame_count.saturating_add(1);
    }

    #[must_use]
    pub const fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    /// Gameplay delta after clamping (see [`MAX_SIM_DELTA`]).
    #[must_use]
    pub const fn sim_delta_seconds(&self) -> f32 {
        self.sim_delta_seconds
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

    /// Sets the collision walls for the current level.
    pub fn set_walls(&mut self, walls: Vec<WallAabb>) {
        self.walls = walls;
    }

    /// Updates first-person WASD movement, pitch/yaw looking, and wall collision with smooth sliding.
    pub fn update_player_movement(&mut self, input: &InputState, settings: &Settings) {
        // While paused or in menus, do not update player movement or looking
        if self.app_state != AppState::Playing {
            return;
        }

        let delta = self.sim_delta_seconds;
        let look_speed_h = settings.look_speed_h.to_radians();
        let look_speed_v = settings.look_speed_v.to_radians();

        // Horizontal camera turn (yaw)
        if input.is_held(Control::LookLeft) {
            self.player_yaw = look_speed_h.mul_add(-delta, self.player_yaw);
        }
        if input.is_held(Control::LookRight) {
            self.player_yaw = look_speed_h.mul_add(delta, self.player_yaw);
        }
        self.player_yaw = self.player_yaw.rem_euclid(TWO_PI);

        // Vertical camera look (pitch) with clamping to prevent camera flipping
        if input.is_held(Control::LookUp) {
            self.player_pitch = look_speed_v.mul_add(delta, self.player_pitch);
        }
        if input.is_held(Control::LookDown) {
            self.player_pitch = look_speed_v.mul_add(-delta, self.player_pitch);
        }
        self.player_pitch = self.player_pitch.clamp(-MAX_PITCH, MAX_PITCH);

        // Planar horizontal movement (grounded, independent of pitch)
        let forward = Vec3::new(self.player_yaw.sin(), 0.0, -self.player_yaw.cos());
        let right = Vec3::new(self.player_yaw.cos(), 0.0, self.player_yaw.sin());

        let mut move_dir = Vec3::ZERO;
        if input.is_held(Control::MoveForward) {
            move_dir += forward;
        }
        if input.is_held(Control::MoveBackward) {
            move_dir -= forward;
        }
        if input.is_held(Control::StrafeLeft) {
            move_dir -= right;
        }
        if input.is_held(Control::StrafeRight) {
            move_dir += right;
        }

        if move_dir.length_squared() > 0.0 {
            let total_delta = move_dir.normalize() * settings.walk_speed * delta;
            let total_dist = total_delta.length();
            let max_step = PLAYER_RADIUS * 0.5;
            let steps = ((total_dist / max_step).ceil() as usize).max(1);
            let step_delta = total_delta / (steps as f32);

            let previous = Vec2::new(self.player_position.x, self.player_position.z);
            let mut current_pos = previous;
            for _ in 0..steps {
                current_pos += Vec2::new(step_delta.x, step_delta.z);
                current_pos = resolve_player_collision(
                    current_pos,
                    PLAYER_RADIUS,
                    self.player_floor_y,
                    &self.walls,
                );
            }
            // The floor sampler is the same model the mesh was built from, so
            // the player stands exactly where the geometry says. A rise or drop
            // within `PLAYER_STEP_HEIGHT` is walked through instantly; anything
            // larger is refused, which is the conservative stand-in for falling
            // physics (there is none) and keeps the player off cliff edges.
            match self.floor.height_at(current_pos.x, current_pos.y) {
                Some(y) if (y - self.player_floor_y).abs() <= PLAYER_STEP_HEIGHT => {
                    self.player_floor_y = y;
                    self.player_position.x = current_pos.x;
                    self.player_position.z = current_pos.y;
                }
                Some(_) => {}
                None => {
                    // Outside every room: keep the historical freedom to walk
                    // over the void, but never step off a real floor into it.
                    if self.floor.height_at(previous.x, previous.y).is_none() {
                        self.player_position.x = current_pos.x;
                        self.player_position.z = current_pos.y;
                    }
                }
            }
            // Maintain grounded eye height regardless of pitch
            self.player_position.y = self.player_floor_y + EYE_HEIGHT;
        }
    }
}

#[cfg(test)]
mod tests;

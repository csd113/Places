use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::collision::{
    PLAYER_RADIUS, PLAYER_STEP_HEIGHT, STEP_EPS, WallAabb, resolve_player_collision,
};
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
        self.delta_seconds = now.duration_since(self.last_frame_time).as_secs_f32();
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

        // Vertical camera look (pitch) with clamping to prevent camera flipping.
        // `invert_look` flips only the vertical direction; the horizontal turn
        // and every movement key are unaffected.
        let pitch_sign = if settings.invert_look { -1.0 } else { 1.0 };
        if input.is_held(Control::LookUp) {
            self.player_pitch = (look_speed_v * pitch_sign).mul_add(delta, self.player_pitch);
        }
        if input.is_held(Control::LookDown) {
            self.player_pitch = (look_speed_v * -pitch_sign).mul_add(delta, self.player_pitch);
        }
        self.player_pitch = self.player_pitch.clamp(-MAX_PITCH, MAX_PITCH);

        // Planar horizontal movement (grounded, independent of pitch)
        let sin_yaw = self.player_yaw.sin();
        let cos_yaw = self.player_yaw.cos();
        // Summing the held directions keeps the accumulation order the moving
        // keys have always had (forward, back, left, right).
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

        if move_dir.length_squared() > 0.0 {
            // Component-wise, with the original `(n * walk_speed) * delta`
            // association so the movement stays bit-for-bit identical.
            let total_delta = Vec3::from_array(
                move_dir
                    .normalize()
                    .to_array()
                    .map(|component| component * settings.walk_speed * delta),
            );
            let total_dist = total_delta.length();
            let max_step = PLAYER_RADIUS * 0.5;
            // Clamped up to at least one sub-step (as the historical
            // `max(1)` did) and down to a million: the ceiling is far above
            // anything `MAX_SIM_DELTA` can produce, and it keeps the count
            // exactly representable so the cast below cannot truncate.
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
            let mut on_a_floor = self.floor.height_at(previous.x, previous.y).is_some();
            // The floor sampler is the same model the mesh was built from, so
            // the player stands exactly where the geometry says. The step rule
            // runs *per sub-step* (each at most half a player radius, 0.15 m):
            // at the loader's maximum ramp slope a sub-step rises at most
            // 0.3 m, so every legal slope and flight is climbable at any frame
            // rate. Applying the rule only to the frame's end point made a
            // maximum-slope ramp unwalkable whenever `walk_speed * delta`
            // exceeded `PLAYER_STEP_HEIGHT / slope`. A rise or drop within
            // `PLAYER_STEP_HEIGHT` plus the surface tolerance is walked through
            // instantly; anything larger is refused, which is the conservative
            // stand-in for falling physics (there is none) and keeps the player
            // off cliff edges.
            for _ in 0..steps {
                let candidate = resolve_player_collision(
                    Vec2::new(current_pos.x + step_delta.x, current_pos.y + step_delta.z),
                    PLAYER_RADIUS,
                    current_floor,
                    &self.walls,
                );
                match self.floor.height_at(candidate.x, candidate.y) {
                    Some(y) if (y - current_floor).abs() <= PLAYER_STEP_HEIGHT + STEP_EPS => {
                        current_floor = y;
                        current_pos = candidate;
                        on_a_floor = true;
                    }
                    Some(_) => break,
                    None => {
                        // Outside every room: keep the historical freedom to
                        // walk over the void, but never step off a real floor
                        // into it.
                        if on_a_floor {
                            break;
                        }
                        current_pos = candidate;
                    }
                }
            }
            self.player_floor_y = current_floor;
            self.player_position.x = current_pos.x;
            self.player_position.z = current_pos.y;
            // Maintain grounded eye height regardless of pitch
            self.player_position.y = self.player_floor_y + EYE_HEIGHT;
        }
    }
}

#[cfg(test)]
mod tests;

use std::time::Instant;

use glam::{Vec2, Vec3};

use crate::collision::{PLAYER_RADIUS, WallAabb, resolve_player_collision};
use crate::input::{Control, InputState};
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
    pub player_position: Vec3,
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub walls: Vec<WallAabb>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, Vec::new())
    }
}

impl Game {
    #[must_use]
    pub fn new(spawn_pos: Vec3, spawn_yaw: f32, walls: Vec<WallAabb>) -> Self {
        Self {
            running: true,
            app_state: AppState::MainMenu,
            last_frame_time: Instant::now(),
            delta_seconds: 0.0,
            sim_delta_seconds: 0.0,
            frame_count: 0,
            player_position: spawn_pos,
            player_yaw: spawn_yaw.rem_euclid(TWO_PI),
            player_pitch: 0.0,
            walls,
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

    /// Resets player position, orientation, and level collision walls when loading a level.
    pub fn reset_level(&mut self, spawn_pos: Vec3, spawn_yaw: f32, walls: Vec<WallAabb>) {
        self.player_position = spawn_pos;
        self.player_yaw = spawn_yaw.rem_euclid(TWO_PI);
        self.player_pitch = 0.0;
        self.walls = walls;
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

            let mut current_pos = Vec2::new(self.player_position.x, self.player_position.z);
            for _ in 0..steps {
                current_pos += Vec2::new(step_delta.x, step_delta.z);
                current_pos = resolve_player_collision(current_pos, PLAYER_RADIUS, &self.walls);
            }
            self.player_position.x = current_pos.x;
            self.player_position.z = current_pos.y;
            // Maintain grounded eye height regardless of pitch
            self.player_position.y = EYE_HEIGHT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_exact;

    #[test]
    fn test_pitch_movement_and_clamping() {
        let mut game = Game::new(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, Vec::new());
        game.set_app_state(AppState::Playing);
        game.sim_delta_seconds = 10.0; // Large step to test pitch clamp
        let settings = Settings::default();

        let input_up = InputState::holding(&[Control::LookUp]);
        game.update_player_movement(&input_up, &settings);
        assert!((game.player_pitch - MAX_PITCH).abs() < 1e-4);

        let input_down = InputState::holding(&[Control::LookDown]);
        game.update_player_movement(&input_down, &settings);
        assert!((game.player_pitch - (-MAX_PITCH)).abs() < 1e-4);
    }

    #[test]
    fn test_sim_delta_is_clamped() {
        assert_exact(clamp_sim_delta(0.016), 0.016);
        assert_exact(clamp_sim_delta(5.0), MAX_SIM_DELTA);
        assert_exact(clamp_sim_delta(-1.0), 0.0);
        assert_exact(clamp_sim_delta(f32::NAN), 0.0);
        assert_exact(clamp_sim_delta(f32::INFINITY), MAX_SIM_DELTA);
    }

    #[test]
    fn test_escape_pause_toggle() {
        let mut game = Game::new(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, Vec::new());
        game.set_app_state(AppState::Playing);
        assert_eq!(game.app_state(), AppState::Playing);

        // Escape opens pause
        game.handle_escape();
        assert_eq!(game.app_state(), AppState::Paused);

        // Escape resumes playing
        game.handle_escape();
        assert_eq!(game.app_state(), AppState::Playing);
    }

    #[test]
    fn test_paused_gameplay_does_not_move_or_turn() {
        let mut game = Game::new(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, Vec::new());
        game.set_app_state(AppState::Paused);
        game.delta_seconds = 1.0;
        let settings = Settings::default();

        let input =
            InputState::holding(&[Control::MoveForward, Control::LookLeft, Control::LookUp]);
        game.update_player_movement(&input, &settings);

        assert_eq!(game.player_position, Vec3::new(0.0, EYE_HEIGHT, 0.0));
        assert_exact(game.player_yaw, 0.0);
        assert_exact(game.player_pitch, 0.0);
    }

    #[test]
    fn test_menu_state_transitions() {
        let mut game = Game::new(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, Vec::new());
        assert_eq!(game.app_state(), AppState::MainMenu);

        game.set_app_state(AppState::LevelSelect);
        assert_eq!(game.app_state(), AppState::LevelSelect);

        game.handle_escape();
        assert_eq!(game.app_state(), AppState::MainMenu);

        game.set_app_state(AppState::Settings);
        assert_eq!(game.app_state(), AppState::Settings);

        game.handle_escape();
        assert_eq!(game.app_state(), AppState::MainMenu);
    }
}

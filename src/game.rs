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
mod tests {
    use super::*;
    use crate::test_support::assert_exact;

    #[test]
    fn test_pitch_movement_and_clamping() {
        let mut game = Game::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            0.0,
            Vec::new(),
            WalkableFloor::default(),
        );
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
        let mut game = Game::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            0.0,
            Vec::new(),
            WalkableFloor::default(),
        );
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
        let mut game = Game::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            0.0,
            Vec::new(),
            WalkableFloor::default(),
        );
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

    /// One 16 m room with a shallow recess (a walkable step), a deep recess (a
    /// cliff), and a two-step staircase, all on the +X side of the spawn.
    fn step_rule_level() -> LevelDef {
        LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "steps",
                "name": "Steps",
                "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 8.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 3.0, "z": 2.0, "width": 2.0, "depth": 4.0, "offset_y": -0.3 },
                    { "x": 8.0, "z": 2.0, "width": 2.0, "depth": 4.0, "offset_y": -1.5 },
                    { "x": 13.0, "z": 3.0, "width": 1.0, "depth": 2.0, "offset_y": 0.35 },
                    { "x": 14.0, "z": 3.0, "width": 1.0, "depth": 2.0, "offset_y": 0.7 }
                ]
            }"#,
        )
        .expect("valid step json")
    }

    fn game_for(level: &LevelDef) -> Game {
        Game::new(
            spawn_position(level),
            level.spawn.yaw_degrees.to_radians(),
            level.collision_aabbs(),
            WalkableFloor::from_level(level),
        )
    }

    fn walk_forward(game: &mut Game, steps: usize) {
        let settings = Settings::default();
        let input = InputState::holding(&[Control::MoveForward]);
        game.set_app_state(AppState::Playing);
        game.sim_delta_seconds = MAX_SIM_DELTA;
        for _ in 0..steps {
            game.update_player_movement(&input, &settings);
        }
    }

    #[test]
    fn test_spawn_position_resolves_the_local_floor() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "elevated_spawn",
                "name": "Elevated Spawn",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                          "height": 3.0, "floor_y": 2.0 },
                "floor_regions": [
                    { "x": 3.0, "z": 3.0, "width": 4.0, "depth": 4.0, "offset_y": -0.5 }
                ]
            }"#,
        )
        .expect("elevated json");
        // The spawn sits over the recess, so the eye follows the recess floor.
        let spawn = spawn_position(&level);
        assert!(
            (spawn.y - (2.0 - 0.5 + EYE_HEIGHT)).abs() < 1e-4,
            "{spawn:?}"
        );
        let game = game_for(&level);
        assert!((game.player_floor_y - 1.5).abs() < 1e-4);
    }

    #[test]
    fn test_controller_steps_down_into_a_shallow_recess_and_back_out() {
        let level = step_rule_level();
        let mut game = game_for(&level);
        walk_forward(&mut game, 12);
        assert!(
            (game.player_floor_y - (-0.3)).abs() < 1e-4,
            "a walkable recess is stepped into: floor {}",
            game.player_floor_y
        );
        assert!(
            game.player_position.x > 3.0 && game.player_position.x < 6.0,
            "{:?}",
            game.player_position
        );

        // Turn around and walk back out; the step is climbed again.
        game.player_yaw = (-90.0f32).to_radians();
        walk_forward(&mut game, 20);
        assert!((game.player_floor_y - 0.0).abs() < 1e-4);
        assert!(game.player_position.x < 3.0);
    }

    #[test]
    fn test_controller_climbs_a_staircase_of_floor_regions() {
        let level = step_rule_level();
        let mut game = game_for(&level);
        // Skip over the recesses by spawning near the staircase.
        game.player_position = Vec3::new(11.5, EYE_HEIGHT, 4.0);
        game.player_floor_y = 0.0;
        walk_forward(&mut game, 25);
        assert!(
            (game.player_floor_y - 0.7).abs() < 1e-4,
            "two 0.35 m steps are climbable: floor {}",
            game.player_floor_y
        );
        assert!(game.player_position.x > 14.0);
    }

    #[test]
    fn test_controller_refuses_a_drop_larger_than_a_step() {
        let level = step_rule_level();
        let mut game = game_for(&level);
        // Walk from the room floor straight at the 1.5 m deep recess.
        game.player_position = Vec3::new(6.5, EYE_HEIGHT, 4.0);
        game.player_floor_y = 0.0;
        walk_forward(&mut game, 40);
        assert!(
            game.player_position.x < 8.0 + 1e-3,
            "the player stops at the cliff edge, not inside the pit: {}",
            game.player_position.x
        );
        assert!((game.player_floor_y - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_controller_cannot_walk_off_the_last_floor_into_the_void() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "open_edge",
                "name": "Open Edge",
                "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 }
            }"#,
        )
        .expect("open edge json");
        let mut game = game_for(&level);
        walk_forward(&mut game, 60);
        assert!(
            game.player_position.x <= 10.0 + 1e-3,
            "walking out of the room is refused: {}",
            game.player_position.x
        );
    }

    #[test]
    fn test_menu_state_transitions() {
        let mut game = Game::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            0.0,
            Vec::new(),
            WalkableFloor::default(),
        );
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

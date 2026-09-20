use std::time::Instant;

use glam::Vec3;

use crate::input::InputState;

const WALK_SPEED: f32 = 1.4;

/// Manages game loop timing and foundational lifecycle state.
pub struct Game {
    running: bool,
    last_frame_time: Instant,
    delta_seconds: f32,
    frame_count: u64,
    pub player_position: Vec3,
    pub player_yaw: f32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Self {
            running: true,
            last_frame_time: Instant::now(),
            delta_seconds: 0.0,
            frame_count: 0,
            player_position: Vec3::new(0.0, 1.6, 0.0),
            player_yaw: 0.0,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Updates loop timing and calculates delta time between frames.
    pub fn update_timing(&mut self) {
        let now = Instant::now();
        self.delta_seconds = (now - self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        self.frame_count = self.frame_count.saturating_add(1);
    }

    pub fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Updates first-person WASD movement without physics or collision.
    pub fn update_player_movement(&mut self, input: &InputState) {
        let forward = Vec3::new(self.player_yaw.sin(), 0.0, -self.player_yaw.cos());
        let right = Vec3::new(self.player_yaw.cos(), 0.0, self.player_yaw.sin());

        let mut move_dir = Vec3::ZERO;
        if input.move_forward {
            move_dir += forward;
        }
        if input.move_backward {
            move_dir -= forward;
        }
        if input.strafe_left {
            move_dir -= right;
        }
        if input.strafe_right {
            move_dir += right;
        }

        if move_dir.length_squared() > 0.0 {
            self.player_position += move_dir.normalize() * WALK_SPEED * self.delta_seconds;
        }
    }
}

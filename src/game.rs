use std::time::Instant;

/// Manages game loop timing and foundational lifecycle state.
pub struct Game {
    running: bool,
    last_frame_time: Instant,
    delta_seconds: f32,
    frame_count: u64,
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
}

//! Measured, rotation-safe prop bodies shared by every transformation caller.
use super::{EYE_HEIGHT, Game, Stance};
use crate::engine::collision::{PLAYER_HEIGHT, PLAYER_RADIUS};
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct PlayerShape {
    pub half: f32,
    pub height: f32,
    pub eye: f32,
}
impl PlayerShape {
    /// A square containing the model at every yaw, bottom anchored at the feet.
    /// # Errors
    /// Rejects nonfinite, degenerate or excessively large model envelopes.
    pub fn from_bounds(bounds: ([f32; 3], [f32; 3]), scale: f32) -> Result<Self, String> {
        let size = Vec3::new(
            (bounds.1[0] - bounds.0[0]) * scale,
            (bounds.1[1] - bounds.0[1]) * scale,
            (bounds.1[2] - bounds.0[2]) * scale,
        );
        if !size.is_finite() || size.min_element() <= 0.0 || size.max_element() > 4.0 {
            return Err("Form bounds must be finite, positive and at most four metres.".into());
        }
        let height = size.y.max(0.06);
        Ok(Self {
            half: (size.x.hypot(size.z) * 0.5).max(0.03),
            height,
            eye: (height * 0.9).clamp(0.04, height - 0.01),
        })
    }
    #[must_use]
    pub const fn human() -> Self {
        Self {
            half: PLAYER_RADIUS,
            height: PLAYER_HEIGHT,
            eye: EYE_HEIGHT,
        }
    }
}
impl Game {
    #[must_use]
    pub const fn body_half(&self) -> f32 {
        match self.player_shape {
            Some(shape) => shape.half,
            None => PLAYER_RADIUS,
        }
    }
    #[must_use]
    pub fn form_model(&self) -> Option<&str> {
        self.form_model.as_deref()
    }

    /// Tests a proposed complete body without changing pose, stance or momentum.
    #[must_use]
    pub fn form_fits(&self, shape: PlayerShape) -> bool {
        self.body_fits_for(
            Vec3::new(self.player_position.x, self.feet_y, self.player_position.z),
            shape.height,
            shape.half,
        )
    }
    /// Commit only after both static and live-body clearance have succeeded.
    pub fn commit_form(&mut self, model: Option<String>, shape: PlayerShape) {
        self.player_shape = model.as_ref().map(|_| shape);
        self.form_model = model;
        self.stance = Stance::Standing;
        self.duck_fraction = 0.0;
        self.eye_offset_current = shape.eye;
        self.climbing = None;
        self.sync_eye();
    }
}

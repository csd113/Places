//! Source-only water controls shared by presentation and fluid response.
use serde::{Deserialize, Serialize};

/// Mean-plane ripples and a uniform authored horizontal current.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WaterMotionDef {
    /// Ripple clock multiplier, 0..=8; zero holds the ripple pattern.
    pub speed: f32,
    /// Ripple displacement/normal blend, 0..=1; zero restores the flat artwork.
    pub strength: f32,
    /// Fluid speed in metres per second, 0..=4; zero is still water.
    pub flow_speed: f32,
    /// Places yaw: zero is -Z, 90 is +X, in 0..=360 degrees.
    pub direction_degrees: f32,
}

impl Default for WaterMotionDef {
    fn default() -> Self {
        Self {
            speed: 1.0,
            strength: 0.35,
            flow_speed: 0.0,
            direction_degrees: 0.0,
        }
    }
}

impl WaterMotionDef {
    /// Whether source serialization may omit the default presentation controls.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Validated source range; malformed values never reach physics or GPU.
    #[must_use]
    pub fn is_valid(self) -> bool {
        [
            (self.speed, 8.0),
            (self.strength, 1.0),
            (self.flow_speed, 4.0),
            (self.direction_degrees, 360.0),
        ]
        .into_iter()
        .all(|(value, maximum)| value.is_finite() && (0.0..=maximum).contains(&value))
    }

    /// Horizontal world velocity shared by player and rigid-body water drag.
    #[must_use]
    pub fn flow(self) -> [f32; 2] {
        let yaw = self.direction_degrees.to_radians();
        [yaw.sin() * self.flow_speed, -yaw.cos() * self.flow_speed]
    }
}

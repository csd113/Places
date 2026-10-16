//! Bounded authoring contract for an anchored latex balloon.

use serde::{Deserialize, Serialize};

/// The seven reusable latex colours. Values become linear albedo, never emission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalloonColor {
    /// Bright party red.
    #[default]
    Red,
    /// Royal blue.
    RoyalBlue,
    /// Golden yellow.
    GoldenYellow,
    /// Grass green.
    GrassGreen,
    /// Deep purple.
    DeepPurple,
    /// Orange.
    Orange,
    /// Pink.
    Pink,
}

impl BalloonColor {
    /// Authored linear latex albedo, applied only to the balloon-body node.
    #[must_use]
    pub const fn linear_albedo(self) -> [f32; 3] {
        match self {
            Self::Red => [0.82, 0.027, 0.045],
            Self::RoyalBlue => [0.028, 0.12, 0.82],
            Self::GoldenYellow => [0.95, 0.64, 0.023],
            Self::GrassGreen => [0.045, 0.52, 0.09],
            Self::DeepPurple => [0.28, 0.045, 0.52],
            Self::Orange => [0.95, 0.235, 0.025],
            Self::Pink => [0.92, 0.20, 0.45],
        }
    }
}

/// One independently placeable balloon, tied to its placement's fixed anchor.
///
/// The shared rigid model supplies a `balloon_body` node and twelve sibling
/// `balloon_string_00` through `balloon_string_11` nodes. The body knot is at
/// the node origin; every string cylinder runs from local Y=0 to Y=1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BalloonDef {
    /// Latex colour; string material is unaffected.
    pub color: BalloonColor,
    /// Uniform latex-body scale, `0.25..=2`; never scales the string thickness.
    pub scale: f32,
    /// Resting knot height above the anchor, `0.15..=4` metres.
    pub string_length: f32,
    /// Fixed entity-local tie point; every axis within +/-8 metres.
    pub anchor: [f32; 3],
    /// Resting knot offset from the tie point, every axis within +/-2 metres.
    /// Bouquet copies fan out using this offset while sharing one anchor.
    pub offset: [f32; 3],
    /// False holds the rest pose; `enable`/`disable` controls motion.
    pub enabled: bool,
    /// Peak individual horizontal displacement, `0..=0.15` metres.
    pub sway_amplitude: f32,
    /// Peak vertical displacement, `0..=0.15` metres.
    pub bob_amplitude: f32,
    /// Peak body tilt, `0..=12` degrees.
    pub rotation_degrees: f32,
    /// Individual motion frequency, `0.01..=1` Hz.
    pub speed_hz: f32,
    /// Phase in cycles, `0..=1`; omitted uses a deterministic instance-id hash.
    pub phase: Option<f32>,
    /// Common bouquet phase in cycles, `0..=1`; omitted uses the individual phase.
    pub shared_phase: Option<f32>,
    /// Peak shared horizontal sway at 0.13 Hz, `0..=0.15` metres.
    pub shared_sway: f32,
}

impl Default for BalloonDef {
    fn default() -> Self {
        Self {
            color: BalloonColor::Red,
            scale: 1.0,
            string_length: 1.1,
            anchor: [0.0; 3],
            offset: [0.0; 3],
            enabled: true,
            sway_amplitude: 0.025,
            bob_amplitude: 0.012,
            rotation_degrees: 3.0,
            speed_hz: 0.19,
            phase: None,
            shared_phase: None,
            shared_sway: 0.007,
        }
    }
}

impl BalloonDef {
    /// The invalid field and its documented bound, if any.
    #[must_use]
    pub fn invalid_field(&self) -> Option<(&'static str, &'static str)> {
        for (name, value, low, high, bound) in [
            ("scale", self.scale, 0.25, 2.0, "0.25..=2"),
            (
                "string_length",
                self.string_length,
                0.15,
                4.0,
                "0.15..=4 metres",
            ),
            (
                "sway_amplitude",
                self.sway_amplitude,
                0.0,
                0.15,
                "0..=0.15 metres",
            ),
            (
                "bob_amplitude",
                self.bob_amplitude,
                0.0,
                0.15,
                "0..=0.15 metres",
            ),
            (
                "shared_sway",
                self.shared_sway,
                0.0,
                0.15,
                "0..=0.15 metres",
            ),
            (
                "rotation_degrees",
                self.rotation_degrees,
                0.0,
                12.0,
                "0..=12 degrees",
            ),
            ("speed_hz", self.speed_hz, 0.01, 1.0, "0.01..=1 Hz"),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Some((name, bound));
            }
        }
        for (name, value) in [("phase", self.phase), ("shared_phase", self.shared_phase)] {
            if value.is_some_and(|phase| !phase.is_finite() || !(0.0..=1.0).contains(&phase)) {
                return Some((name, "0..=1 cycles"));
            }
        }
        for (name, value, limit, bound) in [
            ("anchor", self.anchor, 8.0, "+/-8 metres per axis"),
            ("offset", self.offset, 2.0, "+/-2 metres per axis"),
        ] {
            if !value
                .iter()
                .all(|axis| axis.is_finite() && axis.abs() <= limit)
            {
                return Some((name, bound));
            }
        }
        if self.string_length + self.offset.get(1).copied().unwrap_or(0.0)
            <= self.bob_amplitude + 0.05
        {
            return Some((
                "offset",
                "resting knot height greater than bob_amplitude + 0.05 metres",
            ));
        }
        None
    }
}

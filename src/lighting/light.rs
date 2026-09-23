//! Engine-level light sources.
//!
//! A [`LightSource`] is what the lighting system consumes: a shape, a position,
//! a colour, an intensity and a falloff. It owns **no geometry**. A visible
//! object — a ceiling panel, a vending machine, an EXIT sign, a future arcade
//! cabinet — is a separate concern that may *own* zero or more light sources:
//!
//! ```text
//!     visible fixture / prop  ──owns──▶  0..n LightSource
//!            │
//!            └─ material emission: how bright the object itself reads
//! ```
//!
//! That separation is the whole point of this module:
//!
//! * Adding a new glowing object never means adding a new hardcoded light
//!   family. A new fixture *look* is geometry and art; the light it casts is a
//!   shape plus numbers.
//! * Emission and illumination are independent. A surface can emit strongly and
//!   cast nothing, or cast light while emitting little or nothing.
//! * The bake ([`super::bake`]) and everything after it — including a future
//!   lightmap pass — consume this representation rather than fixture ids.
//!
//! Shapes
//! ------
//! The compact set the current static bake can use well: a point, a rectangle
//! (a panel, a sign face, a screen), and a line (a tube, a neon strip, a
//! fluorescent batten). A cone/spot light needs a directional response model
//! that belongs with later surface-response work, so it is deliberately not
//! part of this batch.
//!
//! Falloff
//! -------
//! Every light has a `range` and a `falloff` curve. The default — a 6 m smooth
//! cushion — is exactly what Places has always baked, so a light that does not
//! author either keeps the historical look bit-for-bit.

use super::color::LightColor;
use super::math::{sanitize_intensity, smooth_falloff};
use super::tuning::MAX_LIGHT_INTENSITY;

/// Distance at which a light that does not author a `range` stops contributing.
///
/// The historical Places pool radius; `tuning::LOCAL_LIGHT_RADIUS_M` is defined
/// from it so the two can never drift.
pub const DEFAULT_LIGHT_RANGE_M: f32 = 6.0;

/// Largest accepted light range, in metres.
///
/// A pool is bounded work per sample; a value past this is a units mistake, not
/// a lighting choice, and is clamped rather than trusted.
pub const MAX_LIGHT_RANGE_M: f32 = 64.0;

/// Smallest accepted light range, in metres.
pub const MIN_LIGHT_RANGE_M: f32 = 0.05;

/// Largest accepted half-extent of a rectangular light, in metres.
pub const MAX_LIGHT_HALF_EXTENT_M: f32 = 8.0;

/// Largest accepted length of a line light, in metres.
pub const MAX_LIGHT_LENGTH_M: f32 = 32.0;

/// Half-thickness the bake gives a line light's tube, in metres.
///
/// A tube is a thin rectangle: its length is authored, its thickness is the
/// physical size of the fixture (roughly a T5/T8 batten or a neon strip). It is
/// not a second dimension for authors to tune.
pub const LINE_LIGHT_HALF_THICKNESS_M: f32 = 0.02;

/// The shape of a light's emitting surface, in the light's own local frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LightShape {
    /// A single point: a bare lamp, a small indicator, an LED.
    #[default]
    Point,
    /// A flat rectangle in the local X/Z plane, rotated by the light's yaw.
    Rect {
        /// Half-extent along the local X axis, in metres.
        half_width: f32,
        /// Half-extent along the local Z axis, in metres.
        half_depth: f32,
    },
    /// A straight tube running along the local X axis.
    Line {
        /// Total length along the local X axis, in metres.
        length: f32,
    },
}

impl LightShape {
    /// Stable name of the shape kind, for diagnostics and validation messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Rect { .. } => "rect",
            Self::Line { .. } => "line",
        }
    }

    /// The shape's half-extents in its own X/Z plane.
    ///
    /// A line is treated as a thin rectangle whose length spans the local X
    /// axis, which is what lets one bake path serve panels and tubes.
    #[must_use]
    pub const fn half_extents(self) -> (f32, f32) {
        match self {
            Self::Point => (0.0, 0.0),
            Self::Rect {
                half_width,
                half_depth,
            } => (half_width, half_depth),
            Self::Line { length } => (length * 0.5, LINE_LIGHT_HALF_THICKNESS_M),
        }
    }

    /// Half-extents in world X/Z after the light's yaw is applied.
    ///
    /// The bake's historical rule: rotations within a whole degree of an odd
    /// multiple of 90 degrees swap the two axes, and every other angle keeps
    /// them. Fractional rotations therefore behave identically in the bake and
    /// in the drawn fixture, which is what the audit tests pin.
    #[must_use]
    pub const fn half_extents_rotated(self, rotation_degrees: f32) -> (f32, f32) {
        let (half_width, half_depth) = self.half_extents();
        if super::math::fixture_is_turned(rotation_degrees) {
            (half_depth, half_width)
        } else {
            (half_width, half_depth)
        }
    }

    /// Clamps every authored dimension into range, mapping a non-finite extent
    /// to a point (no size) rather than to a NaN the bake would spread.
    #[must_use]
    pub const fn sanitized(self) -> Self {
        match self {
            Self::Point => Self::Point,
            Self::Rect {
                half_width,
                half_depth,
            } => Self::Rect {
                half_width: sanitize_extent(half_width, MAX_LIGHT_HALF_EXTENT_M),
                half_depth: sanitize_extent(half_depth, MAX_LIGHT_HALF_EXTENT_M),
            },
            Self::Line { length } => Self::Line {
                length: sanitize_extent(length, MAX_LIGHT_LENGTH_M),
            },
        }
    }

    /// The same shape with every authored dimension multiplied by `factor`.
    ///
    /// Used to place a prop-attached light: scaling the object scales its
    /// emitter, exactly as it scales the object's geometry. The result is
    /// clamped like any other authored extent, so an extreme scale saturates
    /// instead of overflowing to infinity or collapsing to zero.
    #[must_use]
    pub fn scaled(self, factor: f32) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self;
        }
        match self {
            Self::Point => Self::Point,
            Self::Rect {
                half_width,
                half_depth,
            } => Self::Rect {
                half_width: clamp_scaled(half_width, factor, MAX_LIGHT_HALF_EXTENT_M),
                half_depth: clamp_scaled(half_depth, factor, MAX_LIGHT_HALF_EXTENT_M),
            },
            Self::Line { length } => Self::Line {
                length: clamp_scaled(length, factor, MAX_LIGHT_LENGTH_M),
            },
        }
    }

    /// True when the shape's authored dimensions are finite and positive.
    ///
    /// Used by the validator: a rectangle with no usable size is an authoring
    /// error, not a shape that silently becomes a point.
    #[must_use]
    pub fn is_valid(self) -> bool {
        match self {
            Self::Point => true,
            Self::Rect {
                half_width,
                half_depth,
            } => is_positive_extent(half_width) && is_positive_extent(half_depth),
            Self::Line { length } => is_positive_extent(length) && length <= MAX_LIGHT_LENGTH_M,
        }
    }

    /// The shape's longer in-plane axis, in metres.
    #[must_use]
    pub const fn longest_extent(self) -> f32 {
        let (half_width, half_depth) = self.half_extents();
        if half_width > half_depth {
            half_width * 2.0
        } else {
            half_depth * 2.0
        }
    }
}

/// Clamps one authored extent, treating non-finite input as zero.
const fn sanitize_extent(value: f32, max: f32) -> f32 {
    if !value.is_finite() || value < 0.0 {
        0.0
    } else if value > max {
        max
    } else {
        value
    }
}

/// Multiplies an authored extent by a scale, saturating at the cap.
///
/// A non-positive or non-finite extent becomes zero and an overflowing product
/// saturates at `max`, so scaling can never produce a NaN or an infinity.
fn clamp_scaled(value: f32, factor: f32, max: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }
    let scaled = value * factor;
    if scaled.is_nan() {
        0.0
    } else {
        scaled.clamp(0.0, max)
    }
}

/// True when an authored extent is finite, positive and within its cap.
const fn is_positive_extent(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// How a light's contribution decays with distance.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LightFalloff {
    /// The historical Places cushion: `(1 - t)^2 * (1 + 2t)`.
    #[default]
    Smooth,
    /// A straight ramp to zero at the range.
    Linear,
    /// Flat to the range, then zero: a deliberately stylised hard pool.
    Constant,
}

impl LightFalloff {
    /// Every curve, in report order.
    pub const ALL: [Self; 3] = [Self::Smooth, Self::Linear, Self::Constant];

    /// Stable lowercase name, as written in a level and shown in diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Linear => "linear",
            Self::Constant => "constant",
        }
    }

    /// Parses a falloff name, case-insensitively. Unknown names are `None`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|falloff| falloff.name().eq_ignore_ascii_case(trimmed))
    }

    /// The curve's value at `t = distance / range`.
    ///
    /// The curve is defined on `0..1`: negative input is treated as the centre
    /// and any input at or past the range contributes nothing, so every curve
    /// ends at zero and a caller can render the distance test optional.
    #[must_use]
    pub fn factor(self, t: f32) -> f32 {
        if !t.is_finite() || t >= 1.0 {
            return 0.0;
        }
        let t = if t < 0.0 { 0.0 } else { t };
        match self {
            // Delegated so the historical curve cannot drift by one rounding
            // step: it is the same function the bake always used.
            Self::Smooth => smooth_falloff(t),
            Self::Linear => 1.0 - t,
            Self::Constant => 1.0,
        }
    }
}

/// One engine-level light: everything the bake needs, and nothing about what
/// object (if any) owns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSource {
    /// Emitting shape in local space.
    pub shape: LightShape,
    /// World-space centre of the light's emitting surface.
    pub position: [f32; 3],
    /// Yaw of the shape's local axes about Y, in degrees.
    pub rotation_degrees: f32,
    /// Emitted colour, each channel in `0..=1`.
    pub color: LightColor,
    /// Authored strength before the bake's ceiling-height correction.
    pub intensity: f32,
    /// Distance at which the light's contribution reaches zero, in metres.
    pub range: f32,
    /// The curve between centre and range.
    pub falloff: LightFalloff,
    /// Whether the light illuminates at all.
    ///
    /// An object can keep glowing while contributing nothing to the room: set
    /// `enabled: false` and its emission still draws while its light is
    /// skipped. That is the authored half of "emission is not illumination".
    pub enabled: bool,
}

impl LightSource {
    /// A point light at `position` with the documented defaults.
    #[must_use]
    pub const fn point(position: [f32; 3], color: LightColor, intensity: f32) -> Self {
        Self {
            shape: LightShape::Point,
            position,
            rotation_degrees: 0.0,
            color,
            intensity,
            range: DEFAULT_LIGHT_RANGE_M,
            falloff: LightFalloff::Smooth,
            enabled: true,
        }
    }

    /// Clamps every value into its documented range and makes the result
    /// finite, so malformed authored data bakes to a valid light instead of
    /// spreading a NaN through the level.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let color = self.color.sanitized();
        let position = self
            .position
            .map(|value| if value.is_finite() { value } else { 0.0 });
        let rotation_degrees = if self.rotation_degrees.is_finite() {
            self.rotation_degrees
        } else {
            0.0
        };
        let range = if self.range.is_finite() {
            self.range.clamp(MIN_LIGHT_RANGE_M, MAX_LIGHT_RANGE_M)
        } else {
            DEFAULT_LIGHT_RANGE_M
        };
        Self {
            shape: self.shape.sanitized(),
            position,
            rotation_degrees,
            color,
            intensity: sanitize_intensity(self.intensity),
            range,
            falloff: self.falloff,
            enabled: self.enabled,
        }
    }

    /// True when this light actually illuminates: enabled and non-zero.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && self.intensity > 0.0 && self.color.max_channel() > 0.0
    }

    /// The light's centre in world space.
    #[must_use]
    pub const fn x(&self) -> f32 {
        self.position[0]
    }

    /// The light plane's world height.
    #[must_use]
    pub const fn y(&self) -> f32 {
        self.position[1]
    }

    /// The light's centre depth in world space.
    #[must_use]
    pub const fn z(&self) -> f32 {
        self.position[2]
    }

    /// Half-extents of the emitting surface in world X/Z, after rotation.
    #[must_use]
    pub const fn half_extents(&self) -> (f32, f32) {
        self.shape.half_extents_rotated(self.rotation_degrees)
    }

    /// The largest authored intensity the bake will honour.
    pub const MAX_INTENSITY: f32 = MAX_LIGHT_INTENSITY;
}

#[cfg(test)]
mod tests;

//! Emitted light colour.
//!
//! Every value in the baked model is a three-channel [`LightColor`], accumulated
//! per channel; a fixture emits the colour it authors rather than one global
//! tint. The type is serialised as a plain three-element JSON array so level
//! files stay terse and editable.

use serde::{Deserialize, Serialize};

/// Highest legal value of one authored light-colour channel. Channels are
/// fractions of full output, so 1.0 is the natural ceiling.
pub const MAX_LIGHT_COLOR: f32 = 1.0;

/// Emitted colour of one ceiling fixture, or of the ambient fill.
///
/// Channels are linear fractions in `[0, 1]`: `[1, 0, 0]` is pure red,
/// `[1, 1, 1]` is neutral white and `[0, 0, 0]` emits nothing. The type is
/// serialised as a plain three-element JSON array so level files stay terse and
/// editable:
///
/// ```json
/// { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0, "color": [1.0, 0.55, 0.2] }
/// ```
///
/// Values are sanitised, never trusted: see [`LightColor::sanitized`] and
/// [`crate::loader::validate_level`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f32; 3]", into = "[f32; 3]")]
pub struct LightColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl LightColor {
    /// No output.
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);

    /// Neutral channel maximum, used for greyscale helpers in tests.
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    /// Builds a colour from three channel values (no sanitising).
    #[must_use]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// The same colour on all three channels.
    #[must_use]
    pub const fn grey(value: f32) -> Self {
        Self::rgb(value, value, value)
    }

    /// The three channels as an array, in RGB order.
    #[must_use]
    pub const fn to_array(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }

    /// One channel by index (`0 = r`, `1 = g`, `2 = b`).
    #[must_use]
    pub fn channel(self, index: usize) -> f32 {
        match index {
            0 => self.r,
            1 => self.g,
            _ => self.b,
        }
    }

    /// Perceptual luminance (Rec. 709 weights) used for logging and for tests
    /// that reason about overall brightness rather than colour.
    #[must_use]
    pub fn luminance(self) -> f32 {
        0.2126f32.mul_add(self.r, 0.7152f32.mul_add(self.g, 0.0722 * self.b))
    }

    /// Brightest channel, for diagnostics and bounded-accumulation reasoning.
    #[must_use]
    pub fn max_channel(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    /// Dimmest channel.
    #[must_use]
    pub fn min_channel(self) -> f32 {
        self.r.min(self.g).min(self.b)
    }

    /// True when every channel is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.r.is_finite() && self.g.is_finite() && self.b.is_finite()
    }

    /// True when every channel is finite and inside `[0, MAX_LIGHT_COLOR]`.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.is_finite()
            && (0.0..=MAX_LIGHT_COLOR).contains(&self.r)
            && (0.0..=MAX_LIGHT_COLOR).contains(&self.g)
            && (0.0..=MAX_LIGHT_COLOR).contains(&self.b)
    }

    /// Clamps every channel into `[0, MAX_LIGHT_COLOR]`.
    ///
    /// `NaN` becomes zero (an undefined colour must not become a bright one),
    /// `+inf` saturates at the legal maximum and `-inf` emits nothing; finite
    /// out-of-range channels clamp at the legal bound rather than wrapping,
    /// exactly like [`sanitize_intensity`].
    #[must_use]
    pub fn sanitized(self) -> Self {
        Self {
            r: sanitize_channel(self.r),
            g: sanitize_channel(self.g),
            b: sanitize_channel(self.b),
        }
    }

    /// Component-wise sum. The caller is responsible for clamping the result;
    /// this deliberately does not saturate, so callers can decide the bound.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            r: self.r + other.r,
            g: self.g + other.g,
            b: self.b + other.b,
        }
    }

    /// Linear mix towards `other`; `t = 0` keeps `self`, `t = 1` returns
    /// `other`. Non-finite `t` keeps `self`.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        if !t.is_finite() {
            return self;
        }
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (other.r - self.r).mul_add(t, self.r),
            g: (other.g - self.g).mul_add(t, self.g),
            b: (other.b - self.b).mul_add(t, self.b),
        }
    }

    /// Component-wise clamp to `[low, high]`.
    #[must_use]
    pub fn clamped(self, low: f32, high: f32) -> Self {
        Self {
            r: self.r.clamp(low, high),
            g: self.g.clamp(low, high),
            b: self.b.clamp(low, high),
        }
    }
}

/// One sanitised light-colour channel: finite values clamp into
/// `[0, MAX_LIGHT_COLOR]`, non-finite values become zero.
fn sanitize_channel(value: f32) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    if value.is_infinite() {
        return if value > 0.0 { MAX_LIGHT_COLOR } else { 0.0 };
    }
    value.clamp(0.0, MAX_LIGHT_COLOR)
}

impl From<[f32; 3]> for LightColor {
    fn from(value: [f32; 3]) -> Self {
        Self {
            r: value[0],
            g: value[1],
            b: value[2],
        }
    }
}

impl From<LightColor> for [f32; 3] {
    fn from(value: LightColor) -> Self {
        value.to_array()
    }
}

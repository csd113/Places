//! A small, controlled set of animated surface emissions.
//!
//! The dynamic path moves objects; this module moves *brightness*. It is
//! deliberately not a general animation framework: two shapes, both of which
//! multiply the emissive term a surface already has, and nothing else.
//!
//! ```text
//! pulse    a slow sinusoid: an illuminated sign that breathes
//! flicker  an occasional, bounded stutter: a failing tube or ballast
//! ```
//!
//! Three properties keep it in the Places register:
//!
//! * **Emission only.** An animation scales the additive emissive term. It
//!   never touches the baked light, the albedo, the alpha or the geometry, so a
//!   flickering panel keeps lighting the room exactly as it was baked to: the
//!   bake is static by design, and this is a surface effect.
//! * **Deterministic.** Both shapes are pure functions of elapsed seconds, so
//!   the same clock reading always produces the same image and the first frame
//!   is always the authored brightness. The renderer advances that clock from
//!   the simulation's own delta, so a frame *number* does not pin the phase of
//!   a running animation — `PLACES_CAPTURE_FRAME=1` does.
//! * **Quiet by default.** [`EmissionAnimation::NONE`] is the identity: a level
//!   that declares no animation renders exactly the frame it did before this
//!   existed, and the whole module costs one uniform per draw.

use serde::{Deserialize, Serialize};

/// Longest flicker the level format accepts, in hertz.
///
/// Above this a flicker reads as strobe lighting rather than as a fault.
pub const MAX_FLICKER_HZ: f32 = 24.0;
/// Fastest pulse the level format accepts, in hertz.
pub const MAX_PULSE_HZ: f32 = 2.0;
/// Deepest modulation the level format accepts: the emission never drops below
/// `1 - depth` of its authored value.
pub const MAX_ANIMATION_DEPTH: f32 = 0.85;

/// How one surface's emission changes over time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationEffect {
    /// A slow sinusoid, `1 - depth/2` to `1`. For an illuminated sign.
    Pulse,
    /// A bounded, irregular stutter that is at rest most of the time. For a
    /// fluorescent tube or a failing ballast.
    Flicker,
}

impl AnimationEffect {
    /// Every effect, in report order.
    pub const ALL: [Self; 2] = [Self::Pulse, Self::Flicker];

    /// Stable lowercase name, as written in the level and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pulse => "pulse",
            Self::Flicker => "flicker",
        }
    }

    /// Parses an effect name, case-insensitively.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|effect| effect.name().eq_ignore_ascii_case(trimmed))
    }

    /// The default rate of this effect, in hertz.
    #[must_use]
    pub const fn default_hz(self) -> f32 {
        match self {
            Self::Pulse => 0.12,
            Self::Flicker => 9.5,
        }
    }

    /// The default depth of this effect.
    #[must_use]
    pub const fn default_depth(self) -> f32 {
        match self {
            // A sign should breathe, not blink.
            Self::Pulse => 0.22,
            // A flicker has to be visible to read as a fault at all.
            Self::Flicker => 0.55,
        }
    }
}

/// One animated emission, resolved into the multiplier it applies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmissionAnimation {
    /// The effect's shape.
    pub effect: AnimationEffect,
    /// Cycles per second.
    pub hz: f32,
    /// How far the emission may fall below its authored value, `0.0..=0.85`.
    pub depth: f32,
    /// Phase offset in cycles, so two surfaces with the same effect do not
    /// breathe in lockstep.
    pub phase: f32,
}

impl EmissionAnimation {
    /// No animation: the identity multiplier.
    pub const NONE: Self = Self {
        effect: AnimationEffect::Pulse,
        hz: 0.0,
        depth: 0.0,
        phase: 0.0,
    };

    /// An animation of the given effect, with the effect's default rate.
    #[must_use]
    pub const fn new(effect: AnimationEffect) -> Self {
        Self {
            effect,
            hz: effect.default_hz(),
            depth: effect.default_depth(),
            phase: 0.0,
        }
    }

    /// True when this animation can change a pixel.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.depth > 0.0 && self.hz > 0.0
    }

    /// Clamps every field into its documented range.
    #[must_use]
    pub const fn sanitized(self) -> Self {
        let hz = if self.hz.is_finite() {
            self.hz.clamp(0.0, MAX_FLICKER_HZ)
        } else {
            0.0
        };
        let hz = match self.effect {
            AnimationEffect::Pulse => hz.min(MAX_PULSE_HZ),
            AnimationEffect::Flicker => hz,
        };
        let depth = if self.depth.is_finite() {
            self.depth.clamp(0.0, MAX_ANIMATION_DEPTH)
        } else {
            0.0
        };
        let phase = if self.phase.is_finite() {
            self.phase.fract().abs()
        } else {
            0.0
        };
        Self {
            effect: self.effect,
            hz,
            depth,
            phase,
        }
    }

    /// The emission multiplier at `seconds` since the level loaded.
    ///
    /// `1.0` at `seconds == 0` for both effects, so a first-frame capture shows
    /// every surface at its authored brightness.
    #[must_use]
    // Float-only arithmetic on finite inputs: no overflow, no panic path, and
    // the result is clamped to `[0, 1]`.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn factor(self, seconds: f32) -> f32 {
        if !seconds.is_finite() {
            return 1.0;
        }
        let cycle = seconds.mul_add(self.hz, self.phase);
        match self.effect {
            // A cosine that starts at its peak: the sign is brightest at load
            // and settles into a slow breath.
            AnimationEffect::Pulse => {
                let wave = (cycle * std::f32::consts::TAU).cos();
                (self.depth * 0.5).mul_add(-(1.0 - wave), 1.0)
            }
            // Two square waves at incommensurate rates gate a fast ripple, so
            // the stutter never repeats exactly and stays at full brightness
            // most of the time. `step`-like comparisons keep it deterministic.
            AnimationEffect::Flicker => {
                // The gates are offset so both are shut at `seconds == 0`: a
                // level must load at full brightness, not mid-stutter.
                let gate_a = square(cycle + FLICKER_GATE_A_OFFSET, 0.17);
                let gate_b = square(cycle.mul_add(0.37, FLICKER_GATE_B_OFFSET), 0.61);
                let ripple = 0.5f32.mul_add((cycle * std::f32::consts::TAU * 3.0).cos(), 0.5);
                let dip = if gate_a && gate_b { ripple } else { 0.0 };
                self.depth.mul_add(-dip, 1.0)
            }
        }
        .clamp(0.0, 1.0)
    }
}

/// Phase offset that keeps the first flicker gate shut at `t = 0`.
const FLICKER_GATE_A_OFFSET: f32 = 0.5;
/// Phase offset that keeps the second flicker gate shut at `t = 0`.
const FLICKER_GATE_B_OFFSET: f32 = 0.2;

/// A square wave of unit period that is high for `duty` of each cycle.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // float-only
fn square(cycle: f32, duty: f32) -> bool {
    cycle.fract().abs() < duty
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // the shapes are pinned by exact values

    use super::*;

    #[test]
    fn no_animation_is_exactly_the_identity() {
        assert!(!EmissionAnimation::NONE.is_active());
        for seconds in [0.0, 0.5, 3.7, 100.0] {
            assert_eq!(EmissionAnimation::NONE.factor(seconds), 1.0);
        }
    }

    #[test]
    fn both_effects_start_at_full_brightness() {
        for effect in AnimationEffect::ALL {
            let animation = EmissionAnimation::new(effect).sanitized();
            assert!(
                (animation.factor(0.0) - 1.0).abs() < 1.0e-5,
                "{} must not be caught mid-dip on the first frame",
                effect.name()
            );
        }
    }

    #[test]
    fn an_animation_stays_within_its_bounds() {
        for effect in AnimationEffect::ALL {
            let animation = EmissionAnimation::new(effect).sanitized();
            let floor = 1.0 - animation.depth;
            for step in 0..600 {
                let seconds = f64::from(step) / 60.0;
                #[allow(clippy::cast_possible_truncation)] // a handful of seconds
                let seconds = seconds as f32;
                let value = animation.factor(seconds);
                assert!(
                    (floor - 1.0e-5..=1.0 + 1.0e-5).contains(&value),
                    "{} left [{floor}, 1]: {value} at {seconds}",
                    effect.name()
                );
            }
        }
    }

    #[test]
    fn a_pulse_is_a_slow_breath_and_is_not_always_dim() {
        let animation = EmissionAnimation::new(AnimationEffect::Pulse);
        let mut bright = 0;
        let mut dim = 0;
        for step in 0..480 {
            #[allow(clippy::cast_possible_truncation)]
            let seconds = (f64::from(step) / 60.0) as f32;
            if animation.factor(seconds) > 0.97 {
                bright += 1;
            }
            if animation.factor(seconds) < 0.93 {
                dim += 1;
            }
        }
        assert!(bright > 30, "a pulse must spend time at full brightness");
        assert!(dim > 30, "a pulse must actually move");
    }

    #[test]
    fn a_flicker_is_at_rest_most_of_the_time() {
        let animation = EmissionAnimation::new(AnimationEffect::Flicker);
        let mut at_rest = 0;
        let mut dipping = 0;
        let mut samples = 0;
        for step in 0..1200 {
            #[allow(clippy::cast_possible_truncation)]
            let seconds = (f64::from(step) / 60.0) as f32;
            let value = animation.factor(seconds);
            if value > 0.999 {
                at_rest += 1;
            }
            if value < 0.8 {
                dipping += 1;
            }
            samples += 1;
        }
        let resting_share = f64::from(at_rest) / f64::from(samples);
        assert!(
            resting_share > 0.6,
            "a flicker is an occasional stutter, not a strobe: {resting_share}"
        );
        assert!(dipping > 0, "a flicker that never dips is invisible");
    }

    #[test]
    fn sanitize_clamps_every_field() {
        let animation = EmissionAnimation {
            effect: AnimationEffect::Flicker,
            hz: 900.0,
            depth: 4.0,
            phase: f32::NAN,
        }
        .sanitized();
        assert_eq!(animation.hz, MAX_FLICKER_HZ);
        assert_eq!(animation.depth, MAX_ANIMATION_DEPTH);
        assert_eq!(animation.phase, 0.0);
        let pulse = EmissionAnimation {
            effect: AnimationEffect::Pulse,
            hz: 900.0,
            ..animation
        }
        .sanitized();
        assert_eq!(pulse.hz, MAX_PULSE_HZ);
    }

    #[test]
    fn effect_names_round_trip() {
        for effect in AnimationEffect::ALL {
            assert_eq!(AnimationEffect::parse(effect.name()), Some(effect));
            assert_eq!(
                AnimationEffect::parse(&effect.name().to_uppercase()),
                Some(effect)
            );
        }
        assert_eq!(AnimationEffect::parse("blink"), None);
    }

    #[test]
    fn a_non_finite_time_is_the_identity() {
        let animation = EmissionAnimation::new(AnimationEffect::Flicker);
        assert_eq!(animation.factor(f32::NAN), 1.0);
        assert_eq!(animation.factor(f32::INFINITY), 1.0);
    }
}

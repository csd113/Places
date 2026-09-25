//! Lightweight atmospheric distance fog.
//!
//! Fog is the one part of the post-processing brief that lives in the world
//! fragment shader rather than in a separate pass: it has to be applied per
//! fragment anyway, it costs no extra draw and no extra target, and keeping it
//! in the world shader means the historical direct (no-offscreen) path still
//! gets it. Bloom, exposure and grading live in [`super::postprocess`] instead,
//! because they need the finished image.
//!
//! The model is exponential-squared distance fog with a mild height term:
//!
//! ```text
//! density(x) = density * (1 + height_gain * max(0, reference_y - y))
//! amount     = 1 - exp(-(density(x) * distance)^2)
//! ```
//!
//! Both constants are deliberately small. The shipped interiors are at most
//! about 30 m across, so the fog only becomes visible where the building is
//! genuinely deep — a long corridor, the far end of the pool hall, the unmade
//! world through the last doorway — and never turns a room smoky. The height
//! term gives the air a touch more body near the floor without a volumetric
//! pass: it is a gradient on a scalar, not a light shaft.

/// How the air is tinted and how thick it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogState {
    /// Colour the fog mixes towards.
    pub color: [f32; 3],
    /// Extinction per metre at the reference height.
    pub density: f32,
    /// World `y` above which the height term stops thinning the fog.
    pub reference_y: f32,
    /// How much denser the fog gets per metre below the reference height.
    pub height_gain: f32,
}

impl FogState {
    /// The shipped atmosphere: neutral-cool, thin, and a little heavier low
    /// down.
    ///
    /// `density = 0.0095` gives about 4 % at 20 m, 15 % at 40 m and 63 % at the
    /// 100 m far plane before the height term, which is depth without haze. The
    /// colour is the concrete-and-glass grey the interiors already use, so the
    /// fog reads as the building rather than as weather.
    pub const SHIPPED: Self = Self {
        color: [0.60, 0.63, 0.68],
        density: 0.0095,
        reference_y: 2.0,
        height_gain: 0.045,
    };

    /// The fraction of fog at `distance` metres and `height` metres.
    ///
    /// Mirrors the shader's arithmetic exactly, so a test can pin the numbers a
    /// level's depth actually produces.
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow, no panic, and the
    // result is clamped.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn amount(self, distance: f32, height: f32) -> f32 {
        let below = (self.reference_y - height).clamp(0.0, 12.0);
        let density = self.height_gain.mul_add(below, 1.0) * self.density;
        let scaled = density * distance;
        (1.0 - (-scaled * scaled).exp()).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // the helper is mirrored by these exact values

    use super::*;

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() < 1.0e-6
    }

    #[test]
    fn the_shipped_fog_is_invisible_up_close_and_subtle_far_away() {
        let fog = FogState::SHIPPED;
        assert!(
            fog.amount(5.0, 1.0) < 0.01,
            "a room's near wall must not haze"
        );
        assert!(
            (0.05..0.20).contains(&fog.amount(40.0, 1.0)),
            "40 m must read as depth, not as weather: {}",
            fog.amount(40.0, 1.0)
        );
        assert!(
            fog.amount(100.0, 1.0) < 0.65,
            "the far plane must stay readable: {}",
            fog.amount(100.0, 1.0)
        );
    }

    #[test]
    fn the_height_term_only_ever_thickens_the_air() {
        let fog = FogState::SHIPPED;
        let high = fog.amount(30.0, 4.0);
        let low = fog.amount(30.0, -1.5);
        assert!(low > high, "air near the floor must be at least as thick");
        assert!(close(high, fog.amount(30.0, fog.reference_y)));
        assert!(
            close(high, fog.amount(30.0, fog.reference_y + 10.0)),
            "above the reference height the term must be flat"
        );
    }

    #[test]
    fn a_zero_density_fog_is_exactly_off() {
        let fog = FogState {
            color: [0.0; 3],
            density: 0.0,
            reference_y: 0.0,
            height_gain: 0.0,
        };
        assert!(close(fog.amount(1000.0, -20.0), 0.0));
    }

    #[test]
    fn the_amount_is_bounded_for_extreme_inputs() {
        let fog = FogState::SHIPPED;
        assert!(close(fog.amount(f32::INFINITY, 0.0), 1.0));
        assert!(fog.amount(1.0e6, -1.0e6) <= 1.0);
        assert!(fog.amount(1.0e6, -1.0e6) >= 0.0);
    }
}

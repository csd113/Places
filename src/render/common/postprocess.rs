//! Post-processing policy: bloom, exposure, tone and grade settings.
//!
//! This is renderer-neutral: the settings are authored values and the target
//! size is arithmetic. The OpenGL backend (`opengl::postprocess`) owns the
//! programs and targets that execute this policy.

use super::view::{BLOOM_SCALE_DIVISOR, DrawableSize};
use crate::quality::QualityProfile;

/// Resolve-stage settings for one frame.
///
/// Built from the quality profile ([`PostSettings::for_profile`]) plus the
/// player's independent bloom choice ([`PostSettings::with_bloom`]) rather than
/// from level data: post-processing is a presentation choice, not content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostSettings {
    /// Linear exposure multiplier applied before the tone curve.
    pub exposure: f32,
    /// Value at which the tone curve starts rolling off; below it the image is
    /// untouched.
    pub tone_knee: f32,
    /// How much of the blurred emissive image is added back. Zero disables the
    /// bloom passes entirely for the frame.
    pub bloom_strength: f32,
    /// Saturation multiplier of the colour grade (1.0 leaves colour alone).
    pub grade_saturation: f32,
    /// Contrast multiplier around mid grey (1.0 leaves contrast alone).
    pub grade_contrast: f32,
}

/// Bloom strength a blooming frame adds.
///
/// Under one, so a fixture glows instead of blooming across the room. Applied
/// by [`PostSettings::with_bloom`] to every profile: bloom is an independent
/// player preference, so `Low + Bloom On` is as valid as `Full + Bloom On`.
pub const BLOOM_STRENGTH: f32 = 0.42;

impl PostSettings {
    /// The profile-owned settings one quality profile runs with.
    ///
    /// `Full` gets the complete restrained stack: a shoulder at 0.75 and a
    /// grade that is barely a tint. `Low` keeps the tone shoulder — it is part
    /// of the resolve pass the offscreen path already pays for — and drops the
    /// two effects that need extra per-pixel work. Bloom is *not* part of this:
    /// the player decides it separately, in Settings.
    #[must_use]
    pub const fn for_profile(profile: QualityProfile) -> Self {
        match profile {
            QualityProfile::Full => Self {
                exposure: 1.0,
                tone_knee: 0.75,
                bloom_strength: 0.0,
                grade_saturation: 1.03,
                grade_contrast: 1.02,
            },
            // Low presents the scene unfiltered by profile effects: no exposure,
            // no shoulder and no grade. With bloom off, the resolve stage is the
            // identity and the renderer uses the plain copy quad instead of it,
            // which keeps Low exactly as cheap as the plain copy presentation
            // while the world shader keeps the fog (part of the image, not an
            // extra pass). With bloom on, the resolve pass runs to add it.
            QualityProfile::Low => Self {
                exposure: 1.0,
                tone_knee: 1.0,
                bloom_strength: 0.0,
                grade_saturation: 1.0,
                grade_contrast: 1.0,
            },
        }
    }

    /// Returns these settings with the player's bloom preference applied.
    #[must_use]
    pub const fn with_bloom(mut self, enabled: bool) -> Self {
        self.bloom_strength = if enabled { BLOOM_STRENGTH } else { 0.0 };
        self
    }

    /// Whether the resolve stage would change the image at all.
    ///
    /// When it would not — no bloom, unit exposure, no shoulder, no grade — the
    /// renderer presents the scene with the plain copy quad instead, which is
    /// both cheaper and exactly what that resolve would have produced.
    #[must_use]
    #[allow(clippy::float_cmp)] // these are authored constants, not measurements
    pub fn is_identity(self) -> bool {
        self.bloom_strength == 0.0
            && self.exposure == 1.0
            && self.tone_knee >= 1.0
            && self.grade_saturation == 1.0
            && self.grade_contrast == 1.0
    }

    /// Whether this frame draws the extra bloom passes.
    #[must_use]
    #[allow(clippy::float_cmp)] // an exact zero is how "no bloom" is spelled
    pub fn blooms(self) -> bool {
        self.bloom_strength > 0.0
    }
}

/// Size of the emissive and blur targets for a scene target of `scene_size`.
#[must_use]
pub fn bloom_target_size(scene_size: DrawableSize) -> DrawableSize {
    if scene_size.is_empty() {
        return scene_size;
    }
    DrawableSize::new(
        (scene_size.width / BLOOM_SCALE_DIVISOR).max(1),
        (scene_size.height / BLOOM_SCALE_DIVISOR).max(1),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // the profile contract is exact

    use super::*;

    #[test]
    fn bloom_targets_are_a_quarter_of_the_scene() {
        let size = bloom_target_size(DrawableSize::new(960, 544));
        assert_eq!(size, DrawableSize::new(240, 136));
    }

    #[test]
    fn a_tiny_scene_still_gets_a_one_texel_target() {
        assert_eq!(
            bloom_target_size(DrawableSize::new(2, 3)),
            DrawableSize::new(1, 1)
        );
        assert_eq!(
            bloom_target_size(DrawableSize::new(0, 0)),
            DrawableSize::new(0, 0)
        );
    }

    #[test]
    fn bloom_is_an_independent_choice_on_every_profile() {
        let full = PostSettings::for_profile(QualityProfile::Full);
        let low = PostSettings::for_profile(QualityProfile::Low);

        // The profile alone decides exposure/tone/grade; bloom starts off and
        // is the player's separate switch.
        assert!(!full.blooms(), "bloom is not implied by the profile");
        assert!(!low.blooms());
        assert!(!full.is_identity(), "Full still exposes and grades");
        assert!(
            low.is_identity(),
            "Low without bloom is the plain copy presentation"
        );

        // Bloom on is valid on both profiles and changes only the bloom term.
        let full_bloom = full.with_bloom(true);
        let low_bloom = low.with_bloom(true);
        assert!(full_bloom.blooms(), "Full + Bloom On must bloom");
        assert!(
            low_bloom.blooms(),
            "Low + Bloom On is a valid combination and must bloom"
        );
        assert!(!low_bloom.is_identity(), "a blooming Low needs the resolve");
        assert_eq!(full_bloom.exposure, full.exposure);
        assert_eq!(full_bloom.grade_contrast, full.grade_contrast);
        assert_eq!(
            low_bloom.grade_saturation, 1.0,
            "Low still leaves colour alone"
        );

        // Bloom off never pays for the stage.
        assert!(!full.with_bloom(false).blooms());
        assert!(!low.with_bloom(false).blooms());
    }

    #[test]
    fn the_tone_knee_leaves_the_common_range_alone() {
        // The shoulder is only applied above the knee; the resolve shader's
        // arithmetic is pinned here so a change to the constant is deliberate.
        let settings = PostSettings::for_profile(QualityProfile::Full).with_bloom(true);
        assert!((0.5..1.0).contains(&settings.tone_knee));
        assert!(settings.bloom_strength < 1.0, "bloom must stay restrained");
    }
}

//! Post-processing policy: bloom, exposure, tone and grade settings.
//!
//! This is renderer-neutral: the settings are authored values and the target
//! size is arithmetic. The renderer owns the programs and targets that execute
//! this policy.

use super::view::{BLOOM_SCALE_DIVISOR, DrawableSize};
use crate::quality::{QualityLevel, QualityProfile};

/// Resolve-stage settings for one frame.
///
/// Built from the quality level ([`PostSettings::for_level`]) plus the
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
/// by [`PostSettings::with_bloom`] to every level: bloom is an independent
/// player preference, so `Low + Bloom On` is as valid as `High + Bloom On`.
pub const BLOOM_STRENGTH: f32 = 0.42;

impl PostSettings {
    /// The settings one quality level runs with.
    ///
    /// High gets the complete restrained stack: a shoulder at 0.75 and a grade
    /// that is barely a tint. Medium keeps the shoulder — it is part of the
    /// resolve pass the offscreen path already pays for — and drops the two
    /// effects that need extra per-pixel work. Low presents the scene
    /// unfiltered by level effects: no shoulder and no grade. Bloom is *not*
    /// part of this: the player decides it separately, in Settings.
    #[must_use]
    pub const fn for_level(level: QualityLevel) -> Self {
        match level {
            QualityLevel::High => Self::for_profile(QualityProfile::Full),
            QualityLevel::Medium => Self {
                exposure: 1.0,
                tone_knee: 0.75,
                bloom_strength: 0.0,
                grade_saturation: 1.0,
                grade_contrast: 1.0,
            },
            QualityLevel::Low => Self::for_profile(QualityProfile::Low),
        }
    }

    /// The profile-owned settings one validated profile runs with.
    ///
    /// This is the delegation boundary [`Self::for_level`] uses for Low and
    /// High; Medium has no profile arm because its only difference from Full is
    /// the grade, which is not part of the profile budget.
    const fn for_profile(profile: QualityProfile) -> Self {
        match profile {
            QualityProfile::Full => Self {
                exposure: 1.0,
                tone_knee: 0.75,
                bloom_strength: 0.0,
                grade_saturation: 1.03,
                grade_contrast: 1.02,
            },
            // Low presents the scene unfiltered by level effects: no exposure,
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

    /// The three levels resolve the documented tone/grade stack, and Low and
    /// High match the validated profile values exactly.
    #[test]
    fn the_tone_and_grade_stack_is_the_documented_low_medium_high_mapping() {
        let low = PostSettings::for_level(QualityLevel::Low);
        let medium = PostSettings::for_level(QualityLevel::Medium);
        let high = PostSettings::for_level(QualityLevel::High);

        assert_eq!(low.tone_knee, 1.0);
        assert_eq!(medium.tone_knee, 0.75);
        assert_eq!(high.tone_knee, 0.75);

        assert_eq!(low.grade_saturation, 1.0);
        assert_eq!(low.grade_contrast, 1.0);
        assert_eq!(medium.grade_saturation, 1.0);
        assert_eq!(medium.grade_contrast, 1.0);
        assert_eq!(high.grade_saturation, 1.03);
        assert_eq!(high.grade_contrast, 1.02);

        // Exposure never varies; the levels differ in shoulder and grade only.
        assert_eq!(low.exposure, 1.0);
        assert_eq!(medium.exposure, 1.0);
        assert_eq!(high.exposure, 1.0);

        // Low and High delegate to the validated profile values.
        assert_eq!(
            low,
            PostSettings::for_profile(QualityProfile::Low),
            "Low delegates to the profile"
        );
        assert_eq!(
            high,
            PostSettings::for_profile(QualityProfile::Full),
            "High delegates to the profile"
        );

        // Low without bloom is the plain copy presentation; Medium keeps the
        // shoulder, so it always resolves.
        assert!(low.is_identity());
        assert!(!medium.is_identity());
        assert!(!high.is_identity());
    }

    #[test]
    fn bloom_is_an_independent_choice_on_every_level() {
        let high = PostSettings::for_level(QualityLevel::High);
        let medium = PostSettings::for_level(QualityLevel::Medium);
        let low = PostSettings::for_level(QualityLevel::Low);

        // The level alone decides exposure/tone/grade; bloom starts off and is
        // the player's separate switch.
        assert!(!high.blooms(), "bloom is not implied by the level");
        assert!(!medium.blooms());
        assert!(!low.blooms());
        assert!(!high.is_identity(), "High still exposes and grades");
        assert!(
            low.is_identity(),
            "Low without bloom is the plain copy presentation"
        );

        // Bloom on is valid on every level and changes only the bloom term.
        let high_bloom = high.with_bloom(true);
        let medium_bloom = medium.with_bloom(true);
        let low_bloom = low.with_bloom(true);
        assert!(high_bloom.blooms(), "High + Bloom On must bloom");
        assert!(medium_bloom.blooms(), "Medium + Bloom On must bloom");
        assert!(
            low_bloom.blooms(),
            "Low + Bloom On is a valid combination and must bloom"
        );
        assert!(!low_bloom.is_identity(), "a blooming Low needs the resolve");
        assert_eq!(high_bloom.exposure, high.exposure);
        assert_eq!(high_bloom.grade_contrast, high.grade_contrast);
        assert_eq!(
            low_bloom.grade_saturation, 1.0,
            "Low still leaves colour alone"
        );

        // Bloom off never pays for the stage.
        assert!(!high.with_bloom(false).blooms());
        assert!(!medium.with_bloom(false).blooms());
        assert!(!low.with_bloom(false).blooms());
    }

    #[test]
    fn the_tone_knee_leaves_the_common_range_alone() {
        // The shoulder is only applied above the knee; the resolve shader's
        // arithmetic is pinned here so a change to the constant is deliberate.
        let settings = PostSettings::for_level(QualityLevel::High).with_bloom(true);
        assert!((0.5..1.0).contains(&settings.tone_knee));
        assert!(settings.bloom_strength < 1.0, "bloom must stay restrained");
    }
}

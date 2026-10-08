//! Post-processing policy: bloom, exposure, tone and grade settings.
//!
//! This is renderer-neutral: the settings are authored values and the target
//! size is arithmetic. The renderer owns the programs and targets that execute
//! this policy.

use super::view::{BLOOM_SCALE_DIVISOR, DrawableSize};
use crate::quality::QualityLevel;

/// Resolve-stage settings for one frame.
///
/// Authored by the level ([`PostSettings::from_level`]) plus the player's
/// independent bloom choice ([`PostSettings::with_bloom`]). Quality presets
/// preserve the same display transform.
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
pub const BLOOM_STRENGTH: f32 = 0.22;

impl PostSettings {
    /// Every quality shares the same authored display transform. Presets change
    /// scene resolution and lighting resources, never display exposure or grade.
    #[must_use]
    pub const fn for_level(_level: QualityLevel) -> Self {
        Self {
            exposure: 1.0,
            tone_knee: 0.75,
            bloom_strength: 0.0,
            grade_saturation: 1.03,
            grade_contrast: 1.02,
        }
    }

    /// Diagnostic display encoding without exposure, tone, grade or bloom.
    pub const DIAGNOSTIC: Self = Self {
        exposure: 1.0,
        tone_knee: 1.0,
        bloom_strength: 0.0,
        grade_saturation: 1.0,
        grade_contrast: 1.0,
    };

    /// Validated level controls reach all static and entity draw families once.
    #[must_use]
    pub fn from_level(level: Option<&crate::level::LevelDef>) -> Self {
        let authored = level
            .and_then(|value| value.environment)
            .unwrap_or_default()
            .presentation;
        Self {
            exposure: authored.exposure,
            tone_knee: authored.tone_knee,
            bloom_strength: 0.0,
            grade_saturation: authored.saturation,
            grade_contrast: authored.contrast,
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
    #[expect(
        clippy::float_cmp,
        reason = "these are authored constants, not measurements"
    )] // these are authored constants, not measurements
    pub fn is_identity(self) -> bool {
        self.bloom_strength == 0.0
            && self.exposure == 1.0
            && self.tone_knee >= 1.0
            && self.grade_saturation == 1.0
            && self.grade_contrast == 1.0
    }

    /// Whether this frame draws the extra bloom passes.
    #[must_use]
    // an exact zero is how "no bloom" is spelled
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
    #![allow(clippy::float_cmp, reason = "the profile contract is exact")] // the profile contract is exact

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
    fn quality_never_changes_exposure_curve_or_grade() {
        let expected = PostSettings::for_level(QualityLevel::High);
        for quality in [QualityLevel::Low, QualityLevel::Medium, QualityLevel::High] {
            assert_eq!(PostSettings::for_level(quality), expected);
            assert!(!PostSettings::for_level(quality).is_identity());
        }
        assert!(PostSettings::DIAGNOSTIC.is_identity());
    }

    #[test]
    fn authored_presentation_reaches_resolve_once() -> Result<(), serde_json::Error> {
        let mut level: crate::level::LevelDef = serde_json::from_str(include_str!(
            "../../../tests/fixtures/levels/art_style_hero.json"
        ))?;
        let mut controls = crate::environment::EnvironmentDef::default();
        controls.presentation.exposure = 1.25;
        controls.presentation.saturation = 0.97;
        level.environment = Some(controls);
        let settings = PostSettings::from_level(Some(&level));
        assert_eq!(settings.exposure, 1.25);
        assert_eq!(settings.grade_saturation, 0.97);
        assert_eq!(settings.tone_knee, controls.presentation.tone_knee);
        assert_eq!(settings.bloom_strength, 0.0);
        Ok(())
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
            !low.is_identity(),
            "Low retains the shared display transform"
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
            low_bloom.grade_saturation, high.grade_saturation,
            "Low shares the same grade"
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

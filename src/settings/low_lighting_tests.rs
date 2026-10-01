//! Persistence and graphics-transition contract for the Low lighting override.
#![allow(clippy::expect_used)]

use super::*;
use crate::quality::{GraphicsAction, graphics_action};

fn assert_low_lighting(settings: &Settings, texture_quality: QualityLevel, filtering: &str) {
    let actual = settings.graphics_spec();
    let mut low = Settings::default();
    low.set_quality(QualityLevel::Low);
    let expected = low.graphics_spec();
    assert_eq!(actual.quality, texture_quality);
    assert_eq!(actual.filtering, filtering);
    assert_eq!(actual.lighting, expected.lighting);
    assert_eq!(actual.lightmaps, expected.lightmaps);
    assert_eq!(actual.reflections, expected.reflections);
    assert_eq!(actual.bloom, expected.bloom);
    assert!(!actual.lighting.draws_surface_response());
    assert!(actual.lightmaps.lightmap_config().is_none());
    assert!(!actual.reflections.draws_probes());
    assert_eq!(
        crate::render::LightmapBuildOptions::for_lightmaps(actual.lightmaps),
        crate::render::LightmapBuildOptions::for_lightmaps(expected.lightmaps),
    );
}

#[test]
fn old_settings_default_off_and_saved_opt_in_survives_launch() {
    let legacy = serde_json::to_value(Settings::default()).expect("legacy settings");
    let mut legacy = legacy.as_object().expect("settings object").clone();
    legacy.remove("use_low_quality_lighting");
    let settings: Settings = serde_json::from_value(legacy.into()).expect("old file loads");
    assert!(!settings.use_low_quality_lighting);
    assert_eq!(settings.graphics_spec().lighting, QualityLevel::High);

    let path = std::env::temp_dir().join(format!(
        "places-low-lighting-persistence-{}.json",
        std::process::id()
    ));
    let mut settings = Settings::default();
    settings.set_texture_filtering("medium");
    settings.set_use_low_quality_lighting(true);
    settings.save_to_path(&path).expect("save opt-in");
    let mut launched = Settings::load_or_default_from_path(&path);
    assert!(launched.use_low_quality_lighting);
    assert!(
        !launched.take_pending_apply().any(),
        "launch has no stale apply"
    );
    assert_low_lighting(&launched, QualityLevel::High, "medium");
    launched.set_use_low_quality_lighting(false);
    assert_eq!(launched.graphics_spec().lighting, QualityLevel::High);
    assert_eq!(launched.lightmap_quality(), LightmapQuality::Full);
    assert_eq!(launched.reflection_quality(), ReflectionQuality::Full);
    launched.save_to_path(&path).expect("save disabled");
    assert!(!Settings::load_or_default_from_path(&path).use_low_quality_lighting);
    std::fs::remove_file(path).expect("remove fixture");
}

#[test]
fn live_high_on_medium_off_high_preserves_selected_quality() {
    let mut settings = Settings::default();
    let high = settings.graphics_spec();
    settings.set_use_low_quality_lighting(true);
    assert!(settings.take_pending_apply().graphics);
    assert_low_lighting(&settings, QualityLevel::High, "high");
    let low_high = settings.graphics_spec();
    assert_eq!(
        graphics_action(low_high, high, None, None, false),
        GraphicsAction::Schedule
    );

    settings.set_quality(QualityLevel::Medium);
    assert!(settings.take_pending_apply().graphics);
    assert_low_lighting(&settings, QualityLevel::Medium, "medium");
    let low_medium = settings.graphics_spec();
    assert_eq!(
        graphics_action(low_medium, low_high, Some(low_high), None, true),
        GraphicsAction::Schedule
    );
    settings.set_use_low_quality_lighting(false);
    assert!(settings.take_pending_apply().graphics);
    let medium = settings.graphics_spec();
    assert_eq!(medium.quality, QualityLevel::Medium);
    assert_eq!(medium.lighting, QualityLevel::Medium);
    assert_eq!(medium.lightmaps, LightmapQuality::Medium);
    assert_eq!(medium.reflections, ReflectionQuality::Medium);
    assert_eq!(medium.filtering, "medium");
    settings.set_quality(QualityLevel::High);
    assert_eq!(settings.graphics_spec(), high);
}

#[test]
fn repeated_toggles_restore_advanced_preferences_and_cancel_stale_installs() {
    let mut settings = Settings::default();
    settings.set_texture_filtering("medium");
    settings.set_lightmap_quality(LightmapQuality::Medium);
    settings.set_reflection_quality(ReflectionQuality::Off);
    settings.set_bloom(false);
    let _ = settings.take_pending_apply();
    let saved = settings.graphics_spec();
    for _ in 0..8 {
        assert!(settings.set_use_low_quality_lighting(true));
        let low = settings.graphics_spec();
        assert_eq!(low.quality, saved.quality);
        assert_eq!(low.filtering, saved.filtering);
        assert_eq!(low.bloom, saved.bloom);
        assert_eq!(low.lighting, QualityLevel::Low);
        assert!(settings.take_pending_apply().graphics);
        assert!(!settings.set_use_low_quality_lighting(true));
        assert!(!settings.take_pending_apply().any());
        settings.set_use_low_quality_lighting(false);
        assert_eq!(settings.graphics_spec(), saved);
        assert_eq!(
            graphics_action(saved, saved, Some(low), None, true),
            GraphicsAction::CancelStale
        );
        assert!(settings.take_pending_apply().graphics);
    }
}

#[test]
fn low_to_medium_and_high_keeps_override_until_explicitly_disabled() {
    for quality in [QualityLevel::Medium, QualityLevel::High] {
        let mut settings = Settings::default();
        settings.set_quality(QualityLevel::Low);
        settings.set_use_low_quality_lighting(true);
        settings.set_quality(quality);
        assert_low_lighting(&settings, quality, texture_filtering_for(quality));
        settings.set_use_low_quality_lighting(false);
        assert_eq!(settings.lighting_quality(), quality);
        assert_eq!(
            settings.lightmap_quality(),
            LightmapQuality::default_for(quality)
        );
        assert_eq!(
            settings.reflection_quality(),
            ReflectionQuality::default_for(quality)
        );
    }
}

#[test]
fn low_override_dominates_startup_lighting_overrides_without_erasing_them() {
    let mut settings = Settings::default();
    settings.overrides.quality = Some(QualityLevel::Medium);
    settings.overrides.lightmaps = Some(LightmapQuality::Full);
    settings.overrides.reflections = Some(ReflectionQuality::Full);
    settings.set_use_low_quality_lighting(true);
    assert_low_lighting(&settings, QualityLevel::Medium, "high");
    assert!(settings.lightmap_quality_overridden());
    settings.set_use_low_quality_lighting(false);
    assert_eq!(settings.lighting_quality(), QualityLevel::Medium);
    assert_eq!(settings.lightmap_quality(), LightmapQuality::Full);
    assert_eq!(settings.reflection_quality(), ReflectionQuality::Full);
}

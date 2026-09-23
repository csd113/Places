// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic in
// tests; the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use super::*;

#[test]
fn the_default_profile_is_full() {
    assert_eq!(QualityProfile::default(), QualityProfile::Full);
    assert_eq!(QualityProfile::DEFAULT, QualityProfile::Full);
}

#[test]
fn names_round_trip_and_parse_is_forgiving_about_case() {
    for profile in QualityProfile::ALL {
        assert_eq!(QualityProfile::parse(profile.name()), Some(profile));
        assert_eq!(
            QualityProfile::parse(&profile.name().to_uppercase()),
            Some(profile)
        );
        assert_eq!(
            QualityProfile::parse(&format!("  {}  ", profile.name())),
            Some(profile)
        );
    }
    assert_eq!(QualityProfile::parse("medium"), None);
    assert_eq!(QualityProfile::parse(""), None);
    assert_eq!(QualityProfile::parse("high"), None);
}

#[test]
fn full_keeps_the_historical_runtime_sizes() {
    assert_eq!(QualityProfile::Full.budget(TextureClass::Surface), 1_024);
    assert_eq!(
        QualityProfile::Full.budget(TextureClass::FixtureFace),
        1_024
    );
    assert_eq!(QualityProfile::Full.budget(TextureClass::DecalSheet), 1_024);
    assert_eq!(QualityProfile::Full.budget(TextureClass::Prop), 256);
    assert_eq!(QualityProfile::Full.budget(TextureClass::EmissionMask), 512);
}

#[test]
fn low_is_strictly_smaller_for_every_class() {
    for class in [
        TextureClass::Surface,
        TextureClass::FixtureFace,
        TextureClass::DecalSheet,
        TextureClass::Prop,
        TextureClass::EmissionMask,
    ] {
        assert!(
            QualityProfile::Low.budget(class) < QualityProfile::Full.budget(class),
            "low must downscale {class:?} further than full"
        );
    }
    assert_eq!(QualityProfile::Low.budget(TextureClass::Surface), 256);
    assert_eq!(QualityProfile::Low.budget(TextureClass::Prop), 128);
    assert_eq!(QualityProfile::Low.budget(TextureClass::EmissionMask), 128);
}

#[test]
fn budgets_are_power_of_two_so_downscaling_is_a_clean_halving_chain() {
    for profile in QualityProfile::ALL {
        for class in [
            TextureClass::Surface,
            TextureClass::FixtureFace,
            TextureClass::DecalSheet,
            TextureClass::Prop,
            TextureClass::EmissionMask,
        ] {
            assert!(profile.budget(class).is_power_of_two());
        }
    }
}

#[test]
fn only_low_reserves_the_optional_feature_hook() {
    assert!(!QualityProfile::Full.reduces_optional_features());
    assert!(QualityProfile::Low.reduces_optional_features());
}

/// A `size` x `size` image whose texels all carry the same colour.
fn solid(size: u32, color: [u8; 4]) -> crate::materials::RawImage {
    let mut rgba = Vec::new();
    for _ in 0..size.saturating_mul(size) {
        rgba.extend_from_slice(&color);
    }
    crate::materials::RawImage::new(size, size, rgba)
}

#[test]
fn full_uploads_a_shipped_surface_sheet_exactly_as_decoded() {
    let image = solid(1_024, [10, 20, 30, 255]);
    let fitted = fit_image(&image, QualityProfile::Full, TextureClass::Surface);
    assert!(matches!(fitted, std::borrow::Cow::Borrowed(_)));
    assert_eq!((fitted.width, fitted.height), (1_024, 1_024));
}

#[test]
fn low_downscales_the_same_sheet_to_the_documented_budget() {
    let image = solid(1_024, [10, 20, 30, 255]);
    let fitted = fit_image(&image, QualityProfile::Low, TextureClass::Surface);
    assert!(matches!(fitted, std::borrow::Cow::Owned(_)));
    assert_eq!((fitted.width, fitted.height), (256, 256));
    assert!(
        fitted
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|texel| *texel == [10, 20, 30, 255]),
        "downscaling must preserve a solid colour exactly"
    );
}

#[test]
fn a_prop_sheet_is_smaller_than_a_surface_sheet_under_both_profiles() {
    let image = solid(512, [1, 2, 3, 255]);
    let full = fit_image(&image, QualityProfile::Full, TextureClass::Prop);
    let low = fit_image(&image, QualityProfile::Low, TextureClass::Prop);
    assert_eq!((full.width, full.height), (256, 256));
    assert_eq!((low.width, low.height), (128, 128));
}

#[test]
fn an_image_already_within_budget_is_never_copied() {
    let image = solid(64, [1, 2, 3, 255]);
    for profile in QualityProfile::ALL {
        for class in [
            TextureClass::Surface,
            TextureClass::Prop,
            TextureClass::EmissionMask,
        ] {
            let fitted = fit_image(&image, profile, class);
            assert!(
                matches!(fitted, std::borrow::Cow::Borrowed(_)),
                "{profile:?}/{class:?} copied an image that already fits"
            );
        }
    }
}

#[test]
fn fitting_is_deterministic_for_the_same_input() {
    let image = solid(1_024, [200, 100, 50, 255]);
    let first = fit_image(&image, QualityProfile::Low, TextureClass::Surface).into_owned();
    let second = fit_image(&image, QualityProfile::Low, TextureClass::Surface).into_owned();
    assert_eq!(first, second);
}

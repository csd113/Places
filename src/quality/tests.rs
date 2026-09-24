// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic in
// tests; the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used, clippy::float_cmp, clippy::indexing_slicing)]

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
fn full_keeps_the_native_runtime_sizes() {
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
fn full_keeps_a_native_256_prop_texture_exactly() {
    let image = solid(256, [12, 34, 56, 255]);
    let full = fit_image(&image, QualityProfile::Full, TextureClass::Prop);
    assert!(
        matches!(full, std::borrow::Cow::Borrowed(_)),
        "Full must neither copy nor resample a native 256px prop sheet"
    );
    assert_eq!((full.width, full.height), (256, 256));
    assert_eq!(full.rgba, image.rgba);
}

#[test]
fn low_halves_a_native_prop_texture_to_the_documented_budget() {
    let image = solid(256, [12, 34, 56, 255]);
    let low = fit_image(&image, QualityProfile::Low, TextureClass::Prop);
    assert!(matches!(low, std::borrow::Cow::Owned(_)));
    assert_eq!((low.width, low.height), (128, 128));
    assert!(
        low.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|texel| *texel == [12, 34, 56, 255]),
        "the halved sheet must keep the source colour"
    );
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

#[test]
fn lightmap_budgets_scale_with_the_profile() {
    use crate::lighting::lightmap::LightmapConfig;

    let full = LightmapConfig::for_profile(QualityProfile::Full);
    let low = LightmapConfig::for_profile(QualityProfile::Low);

    assert_eq!(full.texels_per_metre, 16.0);
    assert_eq!(full.page_edge, 1024);
    assert_eq!(full.padding, 2);
    assert_eq!(full.usable_edge(), 1020);

    assert_eq!(low.texels_per_metre, 9.0);
    assert_eq!(low.page_edge, 512);
    assert_eq!(low.padding, 1);
    assert_eq!(low.usable_edge(), 510);

    // The profile carries the config itself, so the bake and the cache key can
    // never read two different descriptions of one profile.
    assert_eq!(QualityProfile::Full.lightmap_config(), full);
    assert_eq!(QualityProfile::Low.lightmap_config(), low);

    // Both profiles split the world in the same places: one shared chart-span
    // cap, and the same page budget.
    assert_eq!(full.max_chart_span_m(), low.max_chart_span_m());
    assert_eq!(full.max_pages, 2);
    assert_eq!(low.max_pages, 2);
    assert_eq!(full.bytes_per_texel, 3);
    assert_eq!(low.bytes_per_texel, 3);
    // Full must resolve the patch's full span at its density without using the
    // last-resort clamp: the shared cap and the usable edge agree.
    let cap_texels = (full.max_chart_span_m() * full.texels_per_metre).ceil();
    assert!(
        cap_texels <= f32::from(u16::try_from(full.usable_edge()).unwrap_or(u16::MAX)),
        "the shared chart-span cap must fit Full's usable edge: {cap_texels} vs {}",
        full.usable_edge()
    );
}

/// The baked-shadow half of a profile is a real quality decision, not a
/// label: Full resolves a finer prop-occlusion grid and a denser penumbra than
/// Low, and Low's single tap is the cheapest bake that still grounds a shadow.
#[test]
fn shadow_quality_scales_with_the_profile() {
    use crate::lighting::ShadowSampling;

    assert_eq!(QualityProfile::Full.shadow_taps_per_axis(), 2);
    assert_eq!(QualityProfile::Low.shadow_taps_per_axis(), 1);
    assert_eq!(
        ShadowSampling::HARD.taps_per_axis,
        QualityProfile::Low.shadow_taps_per_axis(),
        "Low is the historical one-tap bake"
    );
    assert!(
        QualityProfile::Full.shadow_taps_per_axis() > QualityProfile::Low.shadow_taps_per_axis(),
        "Full must sample the emitter more finely than Low"
    );
    let full_cell = QualityProfile::Full.prop_occlusion_cell_m();
    let low_cell = QualityProfile::Low.prop_occlusion_cell_m();
    assert!(
        full_cell < low_cell && full_cell > 0.0,
        "Full must resolve prop occlusion more finely: {full_cell} vs {low_cell}"
    );
    // Low keeps the historical 0.15 m default, which is also `BakeConfig::HARD`.
    assert_eq!(low_cell, 0.15);
}

/// A live profile switch must change *every* decision the renderer derives from
/// the profile, and switching back must restore all of them.
///
/// This is the pure half of the runtime Full ↔ Low transition: the GPU half is
/// `Renderer::release_profile_textures` followed by a level rebuild, which the
/// settings layer requests through `SettingsApply::graphics_rebuild` and which
/// needs a GL context to execute. If any derived decision were profile-blind,
/// the switch would be cosmetic and this test would fail.
#[test]
fn switching_full_low_full_changes_every_derived_decision() {
    use crate::lighting::lightmap::LightmapConfig;

    let prop = solid(256, [12, 34, 56, 255]);
    let sheet = solid(1_024, [10, 20, 30, 255]);

    // Full -> Low -> Full, checking the same decisions at every stop.
    let stops = [
        (QualityProfile::Full, 256u32, 1_024u32, true, true, 1_024u32, 2u8, 0.075f32),
        (QualityProfile::Low, 128, 256, false, false, 512, 1, 0.15),
        (QualityProfile::Full, 256, 1_024, true, true, 1_024, 2, 0.075),
    ];
    for (profile, prop_edge, sheet_edge, response, scene_at_drawable, lightmap_page, taps, cell) in
        stops
    {
        assert_eq!(
            fit_image(&prop, profile, TextureClass::Prop).width,
            prop_edge,
            "{profile:?}: prop texture budget"
        );
        assert_eq!(
            fit_image(&sheet, profile, TextureClass::Surface).width,
            sheet_edge,
            "{profile:?}: surface sheet budget"
        );
        assert_eq!(
            profile.draws_surface_response(),
            response,
            "{profile:?}: surface-response gate"
        );
        assert_eq!(
            profile.draws_scene_at_drawable_resolution(),
            scene_at_drawable,
            "{profile:?}: scene-resolution policy"
        );
        assert_eq!(
            LightmapConfig::for_profile(profile).page_edge,
            lightmap_page,
            "{profile:?}: lightmap density"
        );
        assert_eq!(
            profile.shadow_taps_per_axis(),
            taps,
            "{profile:?}: penumbra taps"
        );
        assert_eq!(
            profile.prop_occlusion_cell_m(),
            cell,
            "{profile:?}: prop occlusion cell"
        );
    }
}

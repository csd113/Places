// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic in
// tests; the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use super::*;

/// Every texture class, for loops that assert a property across all of them.
const CLASSES: [TextureClass; 6] = [
    TextureClass::Surface,
    TextureClass::FixtureFace,
    TextureClass::DecalSheet,
    TextureClass::Prop,
    TextureClass::EmissionMask,
    TextureClass::Sky,
];

#[test]
fn the_default_level_is_high() {
    assert_eq!(QualityLevel::default(), QualityLevel::High);
    assert_eq!(QualityLevel::DEFAULT, QualityLevel::High);
}

#[test]
fn names_round_trip_and_parse_is_forgiving_about_case() {
    for level in QualityLevel::ALL {
        assert_eq!(QualityLevel::parse(level.name()), Some(level));
        assert_eq!(
            QualityLevel::parse(&level.name().to_uppercase()),
            Some(level)
        );
        assert_eq!(
            QualityLevel::parse(&format!("  {}  ", level.name())),
            Some(level)
        );
    }
    assert_eq!(QualityLevel::parse(""), None);
    assert_eq!(QualityLevel::parse("ultra"), None);
}

/// Low and High are not new numbers: they answer exactly what the validated
/// profiles answer, so the level can never drift from the profile the lightmap
/// planner and content key use.
#[test]
fn low_and_high_delegate_to_the_validated_profiles() {
    for class in CLASSES {
        assert_eq!(
            QualityLevel::Low.budget(class),
            QualityProfile::Low.budget(class),
            "Low/{class:?} budget"
        );
        assert_eq!(
            QualityLevel::High.budget(class),
            QualityProfile::Full.budget(class),
            "High/{class:?} budget"
        );
    }
    assert_eq!(
        QualityLevel::Low.lightmap_config(),
        QualityProfile::Low.lightmap_config()
    );
    assert_eq!(
        QualityLevel::High.lightmap_config(),
        QualityProfile::Full.lightmap_config()
    );
    assert_eq!(
        QualityLevel::Low.bake_config(),
        QualityProfile::Low.bake_config()
    );
    assert_eq!(
        QualityLevel::High.bake_config(),
        QualityProfile::Full.bake_config()
    );
    assert_eq!(
        QualityLevel::Low.draws_surface_response(),
        QualityProfile::Low.draws_surface_response()
    );
    assert_eq!(
        QualityLevel::High.draws_surface_response(),
        QualityProfile::Full.draws_surface_response()
    );
    assert_eq!(
        QualityLevel::Low.reduces_optional_features(),
        QualityProfile::Low.reduces_optional_features()
    );
    assert_eq!(
        QualityLevel::High.reduces_optional_features(),
        QualityProfile::Full.reduces_optional_features()
    );
}

/// The profile mapping is the protected content-key boundary: only Low maps to
/// the Low profile; Medium and High share Full because the content key already
/// folds the lightmap configuration and bake settings into the hash.
#[test]
fn the_profile_mapping_is_the_content_key_boundary() {
    assert_eq!(QualityLevel::Low.profile(), QualityProfile::Low);
    assert_eq!(QualityLevel::Medium.profile(), QualityProfile::Full);
    assert_eq!(QualityLevel::High.profile(), QualityProfile::Full);
}

#[test]
fn medium_budgets_are_the_documented_intermediate() {
    assert_eq!(QualityLevel::Medium.budget(TextureClass::Surface), 512);
    assert_eq!(QualityLevel::Medium.budget(TextureClass::FixtureFace), 512);
    assert_eq!(QualityLevel::Medium.budget(TextureClass::DecalSheet), 512);
    assert_eq!(QualityLevel::Medium.budget(TextureClass::Prop), 256);
    assert_eq!(QualityLevel::Medium.budget(TextureClass::EmissionMask), 256);
    assert_eq!(QualityLevel::High.budget(TextureClass::Sky), 2048);
    assert_eq!(QualityLevel::Medium.budget(TextureClass::Sky), 1024);
    assert_eq!(QualityLevel::Low.budget(TextureClass::Sky), 512);

    for class in CLASSES {
        let low = QualityLevel::Low.budget(class);
        let medium = QualityLevel::Medium.budget(class);
        let high = QualityLevel::High.budget(class);
        assert!(
            low <= medium && medium <= high,
            "{class:?}: budgets must not invert: {low}/{medium}/{high}"
        );
    }
    assert!(QualityLevel::Low.budget(TextureClass::Surface) < 512);
    assert!(512 < QualityLevel::High.budget(TextureClass::Surface));
    // A native prop sheet already fits the Medium budget, so Medium uploads it
    // unchanged while Low halves it.
    assert_eq!(QualityLevel::Medium.budget(TextureClass::Prop), 256);
    assert!(QualityLevel::Medium.budget(TextureClass::EmissionMask) < 512);
}

#[test]
fn budgets_are_power_of_two_so_downscaling_is_a_clean_halving_chain() {
    for level in QualityLevel::ALL {
        for class in CLASSES {
            assert!(level.budget(class).is_power_of_two());
        }
    }
}

#[test]
fn only_low_reserves_the_optional_feature_hook() {
    assert!(!QualityLevel::High.reduces_optional_features());
    assert!(!QualityLevel::Medium.reduces_optional_features());
    assert!(QualityLevel::Low.reduces_optional_features());
}

#[test]
fn surface_response_is_drawn_except_on_low() {
    assert!(QualityLevel::High.draws_surface_response());
    assert!(QualityLevel::Medium.draws_surface_response());
    assert!(!QualityLevel::Low.draws_surface_response());
    assert!(QualityLevel::High.draws_scene_at_drawable_resolution());
    assert!(!QualityLevel::Medium.draws_scene_at_drawable_resolution());
    assert!(!QualityLevel::Low.draws_scene_at_drawable_resolution());
}

/// The lightmap half of a level is a real quality decision, not a label: the
/// three levels resolve the documented density, page size, padding and tap
/// count, and all three bake the same patch set with a shared chart-span cap
/// and their documented page budgets.
#[test]
fn lightmap_configs_are_the_documented_low_medium_high_values() {
    use crate::lighting::lightmap::LightmapConfig;

    let low = QualityLevel::Low.lightmap_config();
    let medium = QualityLevel::Medium.lightmap_config();
    let high = QualityLevel::High.lightmap_config();

    assert_eq!(low.texels_per_metre, 10.0);
    assert_eq!(low.page_edge, 512);
    assert_eq!(low.padding, 1);
    assert_eq!(low.usable_edge(), 510);

    assert_eq!(medium.texels_per_metre, 12.0);
    assert_eq!(medium.page_edge, 1_024);
    assert_eq!(medium.padding, 2);
    assert_eq!(medium.usable_edge(), 1_020);

    assert_eq!(high.texels_per_metre, 16.0);
    assert_eq!(high.page_edge, 1_024);
    assert_eq!(high.padding, 2);
    assert_eq!(high.usable_edge(), 1_020);

    for (config, pages) in [(low, 8), (medium, 8), (high, 11)] {
        // Full permits eleven base pages; preparation applies the actual
        // illumination-group byte bound. Lower profiles retain eight pages.
        assert_eq!(config.max_pages, pages);
        assert_eq!(config.bytes_per_texel, 16);
        assert_eq!(config.max_chart_span_m(), low.max_chart_span_m());
    }
    assert_eq!(
        high.max_pages,
        crate::lighting::lightmap::LIGHTMAP_ATLAS_MAX_PAGES,
        "Full owns the shared renderer capacity"
    );

    // Medium and High must resolve the shared chart-span cap at their density
    // without using the last-resort clamp, exactly as Full did; Low's smaller
    // page deliberately clamps the longest charts instead (the patch set is
    // still the same).
    for config in [medium, high] {
        let cap_texels = (config.max_chart_span_m() * config.texels_per_metre).ceil();
        assert!(
            cap_texels <= f32::from(u16::try_from(config.usable_edge()).unwrap_or(u16::MAX)),
            "the shared chart-span cap must fit a {} usable edge: {cap_texels} vs {}",
            config.page_edge,
            config.usable_edge()
        );
    }

    // Medium and High share the page shape but differ in density and budget, so the
    // content key (which hashes the config) keeps them apart.
    assert_ne!(medium, high);
    assert_eq!(
        medium,
        LightmapConfig {
            texels_per_metre: 12.0,
            max_pages: 8,
            ..high
        }
    );
}

/// Low keeps the historical one-tap bake; Medium and High take the measured
/// quincunx, and the prop-occlusion grid refines from 0.15 m to 0.11 m to
/// 0.075 m.
#[test]
fn shadow_quality_scales_with_the_level() {
    use crate::lighting::{BakeConfig, ShadowSampling};

    assert_eq!(QualityLevel::Low.shadow_taps_per_axis(), 1);
    assert_eq!(QualityLevel::Medium.shadow_taps_per_axis(), 2);
    assert_eq!(QualityLevel::High.shadow_taps_per_axis(), 2);
    assert_eq!(
        ShadowSampling::HARD.taps_per_axis,
        QualityLevel::Low.shadow_taps_per_axis(),
        "Low is the historical one-tap bake"
    );

    let low_cell = QualityLevel::Low.prop_occlusion_cell_m();
    let medium_cell = QualityLevel::Medium.prop_occlusion_cell_m();
    let high_cell = QualityLevel::High.prop_occlusion_cell_m();
    assert_eq!(low_cell, 0.15);
    assert_eq!(medium_cell, 0.11);
    assert_eq!(high_cell, 0.075);
    assert!(high_cell < medium_cell && medium_cell < low_cell);

    assert_eq!(
        QualityLevel::Medium.bake_config(),
        BakeConfig {
            sampling: ShadowSampling { taps_per_axis: 2 },
            prop_occlusion_cell_m: 0.11,
        }
    );
}

/// One row of the level contract: the level plus every derived decision.
///
/// `(level, prop edge, sheet edge, mask edge, response, roomy scene, lightmap
/// page edge, density, taps, cell, optional-feature hook)`.
type LevelStop = (
    QualityLevel,
    u32,
    u32,
    u32,
    bool,
    bool,
    u32,
    f32,
    u8,
    f32,
    bool,
);

/// Asserts every decision of one [`LevelStop`] row.
fn assert_level_decisions(
    (
        level,
        prop_edge,
        sheet_edge,
        mask_edge,
        response,
        scene_at_drawable,
        lightmap_page,
        density,
        taps,
        cell,
        optional_hook,
    ): LevelStop,
) {
    use crate::lighting::lightmap::LightmapConfig;

    let prop = solid(256, [12, 34, 56, 255]);
    let sheet = solid(1_024, [10, 20, 30, 255]);
    assert_eq!(
        fit_image(&prop, level, TextureClass::Prop).width,
        prop_edge,
        "{level:?}: prop texture budget"
    );
    assert_eq!(
        fit_image(&sheet, level, TextureClass::Surface).width,
        sheet_edge,
        "{level:?}: surface sheet budget"
    );
    assert_eq!(
        fit_image(
            &solid(mask_edge.saturating_mul(2_u32), [1, 2, 3, 255]),
            level,
            TextureClass::EmissionMask
        )
        .width,
        mask_edge,
        "{level:?}: emission-mask budget"
    );
    assert_eq!(
        level.draws_surface_response(),
        response,
        "{level:?}: surface-response gate"
    );
    assert_eq!(
        level.draws_scene_at_drawable_resolution(),
        scene_at_drawable,
        "{level:?}: scene-resolution policy"
    );
    let config = LightmapConfig::for_profile(level.profile());
    assert_eq!(
        level.lightmap_config().page_edge,
        lightmap_page,
        "{level:?}: lightmap page edge"
    );
    assert_eq!(
        level.lightmap_config().texels_per_metre,
        density,
        "{level:?}: lightmap density (profile config {})",
        config.texels_per_metre
    );
    assert_eq!(
        level.shadow_taps_per_axis(),
        taps,
        "{level:?}: penumbra taps"
    );
    assert_eq!(
        level.prop_occlusion_cell_m(),
        cell,
        "{level:?}: prop occlusion cell"
    );
    assert_eq!(
        level.reduces_optional_features(),
        optional_hook,
        "{level:?}: optional-feature hook"
    );
}

/// A live level switch must change *every* decision the renderer derives from
/// the level, and switching back must restore all of them.
///
/// This is the pure half of the runtime transition: the GPU half is
/// `Renderer::release_profile_textures` followed by a level rebuild, which the
/// settings layer requests through `SettingsApply::graphics` and which
/// needs a GPU context to execute. If any derived decision were level-blind,
/// the switch would be cosmetic and this test would fail.
#[test]
fn switching_levels_changes_every_derived_decision() {
    let stops: [LevelStop; 3] = [
        (
            QualityLevel::Low,
            128,
            256,
            128,
            false,
            false,
            512,
            10.0,
            1,
            0.15,
            true,
        ),
        (
            QualityLevel::Medium,
            256,
            512,
            256,
            true,
            false,
            1_024,
            12.0,
            2,
            0.11,
            false,
        ),
        (
            QualityLevel::High,
            256,
            1_024,
            512,
            true,
            true,
            1_024,
            16.0,
            2,
            0.075,
            false,
        ),
    ];
    for stop in stops {
        assert_level_decisions(stop);
    }
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
fn high_uploads_a_shipped_surface_sheet_exactly_as_decoded() {
    let image = solid(1_024, [10, 20, 30, 255]);
    let fitted = fit_image(&image, QualityLevel::High, TextureClass::Surface);
    assert!(matches!(fitted, std::borrow::Cow::Borrowed(_)));
    assert_eq!((fitted.width, fitted.height), (1_024, 1_024));
}

#[test]
fn medium_halves_the_same_sheet_to_the_documented_budget() {
    let image = solid(1_024, [10, 20, 30, 255]);
    let fitted = fit_image(&image, QualityLevel::Medium, TextureClass::Surface);
    assert!(matches!(fitted, std::borrow::Cow::Owned(_)));
    assert_eq!((fitted.width, fitted.height), (512, 512));
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
fn low_quarters_the_same_sheet_to_the_documented_budget() {
    let image = solid(1_024, [10, 20, 30, 255]);
    let fitted = fit_image(&image, QualityLevel::Low, TextureClass::Surface);
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
fn a_prop_sheet_is_smaller_than_a_surface_sheet_under_every_level() {
    let image = solid(512, [1, 2, 3, 255]);
    let high = fit_image(&image, QualityLevel::High, TextureClass::Prop);
    let medium = fit_image(&image, QualityLevel::Medium, TextureClass::Prop);
    let low = fit_image(&image, QualityLevel::Low, TextureClass::Prop);
    assert_eq!((high.width, high.height), (256, 256));
    assert_eq!((medium.width, medium.height), (256, 256));
    assert_eq!((low.width, low.height), (128, 128));
}

#[test]
fn high_and_medium_keep_a_native_256_prop_texture_exactly() {
    let image = solid(256, [12, 34, 56, 255]);
    for level in [QualityLevel::High, QualityLevel::Medium] {
        let fitted = fit_image(&image, level, TextureClass::Prop);
        assert!(
            matches!(fitted, std::borrow::Cow::Borrowed(_)),
            "{level:?} must neither copy nor resample a native 256px prop sheet"
        );
        assert_eq!((fitted.width, fitted.height), (256, 256));
        assert_eq!(fitted.rgba, image.rgba);
    }
}

#[test]
fn low_halves_a_native_prop_texture_to_the_documented_budget() {
    let image = solid(256, [12, 34, 56, 255]);
    let low = fit_image(&image, QualityLevel::Low, TextureClass::Prop);
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
    for level in QualityLevel::ALL {
        for class in [
            TextureClass::Surface,
            TextureClass::Prop,
            TextureClass::EmissionMask,
        ] {
            let fitted = fit_image(&image, level, class);
            assert!(
                matches!(fitted, std::borrow::Cow::Borrowed(_)),
                "{level:?}/{class:?} copied an image that already fits"
            );
        }
    }
}

#[test]
fn fitting_is_deterministic_for_the_same_input() {
    let image = solid(1_024, [200, 100, 50, 255]);
    let first = fit_image(&image, QualityLevel::Medium, TextureClass::Surface).into_owned();
    let second = fit_image(&image, QualityLevel::Medium, TextureClass::Surface).into_owned();
    assert_eq!(first, second);
}

/// The Advanced enums round-trip their persisted names and show the
/// player-facing labels, with no numeric internal on the screen.
#[test]
fn advanced_quality_names_labels_and_parse_round_trip() {
    for level in LightmapQuality::ALL {
        assert_eq!(LightmapQuality::parse(level.name()), Some(level));
        assert_eq!(
            LightmapQuality::parse(&level.name().to_uppercase()),
            Some(level)
        );
        assert_eq!(
            LightmapQuality::parse(&format!("  {}  ", level.name())),
            Some(level)
        );
        assert!(!level.label().chars().any(|ch| ch.is_ascii_digit()));
    }
    assert_eq!(LightmapQuality::parse("on"), None);
    assert_eq!(LightmapQuality::parse("ultra"), None);

    for level in ReflectionQuality::ALL {
        assert_eq!(ReflectionQuality::parse(level.name()), Some(level));
        assert_eq!(
            ReflectionQuality::parse(&format!(" {} ", level.name().to_uppercase())),
            Some(level)
        );
        assert!(!level.label().chars().any(|ch| ch.is_ascii_digit()));
    }
    assert_eq!(ReflectionQuality::parse("1"), None);
    assert_eq!(ReflectionQuality::parse(""), None);
}

/// The preset table is exact: each overall level selects the documented
/// Advanced defaults, and the defaults are themselves valid overrides.
#[test]
fn the_advanced_preset_table_is_exact() {
    for (level, lightmaps, reflections) in [
        (
            QualityLevel::Low,
            LightmapQuality::Off,
            ReflectionQuality::Off,
        ),
        (
            QualityLevel::Medium,
            LightmapQuality::Medium,
            ReflectionQuality::Medium,
        ),
        (
            QualityLevel::High,
            LightmapQuality::Full,
            ReflectionQuality::Full,
        ),
    ] {
        assert_eq!(LightmapQuality::default_for(level), lightmaps, "{level:?}");
        assert_eq!(
            ReflectionQuality::default_for(level),
            reflections,
            "{level:?}"
        );
    }
    assert_eq!(LightmapQuality::DEFAULT, LightmapQuality::Full);
    assert_eq!(ReflectionQuality::DEFAULT, ReflectionQuality::Full);
}

/// Lightmaps Off has no bake at all; Medium and Full reuse the validated level
/// configurations, so a saved choice maps onto the same atlas the level used.
#[test]
fn lightmap_quality_bakes_match_the_validated_configurations() {
    assert!(LightmapQuality::Off.is_off());
    assert!(LightmapQuality::Off.lightmap_config().is_none());
    assert!(LightmapQuality::Off.bake_config().is_none());

    assert_eq!(
        LightmapQuality::Medium.lightmap_config(),
        Some(QualityLevel::Medium.lightmap_config())
    );
    assert_eq!(
        LightmapQuality::Medium.bake_config(),
        Some(QualityLevel::Medium.bake_config())
    );
    assert_eq!(
        LightmapQuality::Full.lightmap_config(),
        Some(QualityLevel::High.lightmap_config())
    );
    assert_eq!(
        LightmapQuality::Full.bake_config(),
        Some(QualityLevel::High.bake_config())
    );
    for level in [LightmapQuality::Medium, LightmapQuality::Full] {
        assert_eq!(level.profile(), QualityProfile::Full);
        assert!(!level.is_off());
    }
    // Medium and Full never share a cache entry: the concrete configs differ.
    assert_ne!(
        LightmapQuality::Medium.lightmap_config(),
        LightmapQuality::Full.lightmap_config()
    );
}

/// Reflections Off draws nothing; Medium and Full both draw the probes and the
/// planar mirror.
#[test]
fn reflection_quality_gates_are_exact() {
    assert!(!ReflectionQuality::Off.draws_probes());
    assert!(!ReflectionQuality::Off.draws_planar());
    for level in [ReflectionQuality::Medium, ReflectionQuality::Full] {
        assert!(level.draws_probes());
        assert!(level.draws_planar());
    }
}

// ------------------------------------------------- graphics transition matrix

/// The [`GraphicsSpec`] a quality preset cascades to, without going through
/// `Settings`: the documented preset table.
fn preset(level: QualityLevel) -> GraphicsSpec {
    GraphicsSpec {
        quality: level,
        lighting: level,
        filtering: match level {
            QualityLevel::Low => "low",
            QualityLevel::Medium => "medium",
            QualityLevel::High => "high",
        },
        bloom: true,
        lightmaps: LightmapQuality::default_for(level),
        reflections: ReflectionQuality::default_for(level),
    }
}

#[test]
fn every_graphics_spec_names_its_expected_configuration() {
    assert_eq!(GraphicsSpec::DEFAULT, preset(QualityLevel::High));
    for level in QualityLevel::ALL {
        let spec = preset(level);
        assert_eq!(spec.quality, level);
        // A different quality is never equal to another one's preset: the six
        // directed transitions below rely on every pair being distinguishable.
        for other in QualityLevel::ALL {
            if other != level {
                assert_ne!(spec, preset(other), "{level:?} vs {other:?}");
            }
        }
    }
}

/// The six directed quality transitions all schedule work from the
/// configuration they start at.
#[test]
fn every_quality_transition_is_scheduled() {
    for from in QualityLevel::ALL {
        for to in QualityLevel::ALL {
            if from == to {
                assert_eq!(
                    graphics_action(preset(from), preset(from), None, None, false),
                    GraphicsAction::Settled
                );
                continue;
            }
            assert_eq!(
                graphics_action(preset(to), preset(from), None, None, false),
                GraphicsAction::Schedule,
                "{from:?} -> {to:?} must be scheduled"
            );
        }
    }
}

/// A late install never overwrites a newer selection, and a commit that
/// delivered an older configuration leaves the remainder to be scheduled.
#[test]
fn a_stale_install_never_wins_over_a_newer_selection() {
    // Low is resident and an install that snapshotted Low is staging while the
    // player selects High: the High request must supersede the staged install.
    assert_eq!(
        graphics_action(
            preset(QualityLevel::High),
            preset(QualityLevel::Low),
            Some(preset(QualityLevel::Low)),
            None,
            true
        ),
        GraphicsAction::Schedule
    );
    // The High install is staging; the player selects Medium instead.
    assert_eq!(
        graphics_action(
            preset(QualityLevel::Medium),
            preset(QualityLevel::Low),
            Some(preset(QualityLevel::High)),
            None,
            true
        ),
        GraphicsAction::Schedule
    );
    // The install that matches the request is awaited, in both the staged and
    // the still-on-CPU shapes.
    assert_eq!(
        graphics_action(
            preset(QualityLevel::High),
            preset(QualityLevel::Low),
            Some(preset(QualityLevel::High)),
            None,
            true
        ),
        GraphicsAction::AwaitCommit
    );
    assert_eq!(
        graphics_action(
            preset(QualityLevel::High),
            preset(QualityLevel::Low),
            None,
            None,
            true
        ),
        GraphicsAction::AwaitCommit
    );
    // The stale install committed the older configuration anyway: the request
    // is still outstanding and is scheduled again from the truth.
    assert_eq!(
        graphics_action(
            preset(QualityLevel::High),
            preset(QualityLevel::Low),
            None,
            None,
            false
        ),
        GraphicsAction::Schedule
    );
}

/// Choosing the resident configuration again cancels an install of the
/// superseded one instead of letting it commit.
#[test]
fn returning_to_the_resident_configuration_cancels_a_stale_install() {
    assert_eq!(
        graphics_action(
            preset(QualityLevel::Low),
            preset(QualityLevel::Low),
            Some(preset(QualityLevel::High)),
            None,
            true
        ),
        GraphicsAction::CancelStale
    );
    assert_eq!(
        graphics_action(
            preset(QualityLevel::Low),
            preset(QualityLevel::Low),
            Some(preset(QualityLevel::Low)),
            None,
            true
        ),
        GraphicsAction::AwaitCommit,
        "an install of the configuration already resident is harmless"
    );
}

/// A request that already failed is held until it changes; an explicit new
/// selection is scheduled again.
#[test]
fn a_failed_request_is_held_until_the_request_changes() {
    assert_eq!(
        graphics_action(
            preset(QualityLevel::High),
            preset(QualityLevel::Low),
            None,
            Some(preset(QualityLevel::High)),
            false
        ),
        GraphicsAction::Held
    );
    // The player answers the actionable message by choosing a configuration
    // the package can serve: the held request is gone and the new one runs.
    let mut overridden = preset(QualityLevel::High);
    overridden.lightmaps = LightmapQuality::Off;
    assert_eq!(
        graphics_action(
            overridden,
            preset(QualityLevel::Low),
            None,
            Some(preset(QualityLevel::High)),
            false
        ),
        GraphicsAction::Schedule
    );
    // A settled state clears the record of the failure.
    assert_eq!(
        graphics_action(
            preset(QualityLevel::Low),
            preset(QualityLevel::Low),
            None,
            Some(preset(QualityLevel::High)),
            false
        ),
        GraphicsAction::Settled
    );
}

/// A repeated cycle of selections ends settled at the last one, with every
/// step scheduled in the direction asked for.
#[test]
fn repeated_quality_cycles_end_settled_at_the_last_selection() {
    let cycle = [
        QualityLevel::Low,
        QualityLevel::Medium,
        QualityLevel::High,
        QualityLevel::Medium,
        QualityLevel::Low,
        QualityLevel::High,
        QualityLevel::Medium,
        QualityLevel::High,
    ];
    let mut applied = preset(QualityLevel::High);
    for requested_level in cycle {
        let requested = preset(requested_level);
        assert_eq!(
            graphics_action(requested, applied, None, None, false),
            GraphicsAction::Schedule,
            "{requested_level:?} after {applied:?}"
        );
        // The commit of that request installs exactly it.
        applied = requested;
    }
    assert_eq!(applied, preset(QualityLevel::High));
    assert_eq!(
        graphics_action(applied, applied, None, None, false),
        GraphicsAction::Settled
    );
}

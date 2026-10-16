//! Unit tests for the baked lighting model.
//!
//! They exercise the module through its public surface plus the pure helpers,
//! which is exactly what the geometry emitter uses.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::suboptimal_flops,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use super::bake::PoolTerm;
use super::tuning::BASELINE_MAX;
use super::*;
use crate::engine::level::LevelDef;
use crate::test_support::{assert_exact, scan, scan_below};

/// One rectangular room with `lights` fixtures evenly spread across it.
fn level_with_room(width: f32, depth: f32, height: f32, intensities: &[f32]) -> LevelDef {
    let lights: Vec<String> = intensities
        .iter()
        .enumerate()
        .map(|(index, intensity)| {
            let x = width * (crate::test_support::exact_f32(index) + 1.0) / (crate::test_support::exact_f32(intensities.len()) + 1.0);
            let z = depth * (crate::test_support::exact_f32(index) + 1.0) / (crate::test_support::exact_f32(intensities.len()) + 1.0);
            format!(
                r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "brightness": {intensity} }}"#
            )
        })
        .collect();
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "lighting_test",
            "name": "Lighting Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": {height} }}],
            "ceiling_lights": [{}]
        }}"#,
        lights.join(",")
    );
    LevelDef::from_json(&json).expect("test level parses")
}

/// One rectangular room with explicit per-fixture emitted colours, spread
/// evenly over the floor like [`level_with_room`].
fn level_with_colored_room(
    width: f32,
    depth: f32,
    height: f32,
    lights: &[(f32, [f32; 3])],
) -> LevelDef {
    let entries: Vec<String> = lights
        .iter()
        .enumerate()
        .map(|(index, (intensity, color))| {
            let x = width * (crate::test_support::exact_f32(index) + 1.0) / (crate::test_support::exact_f32(lights.len()) + 1.0);
            let z = depth * (crate::test_support::exact_f32(index) + 1.0) / (crate::test_support::exact_f32(lights.len()) + 1.0);
            format!(
                r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "brightness": {intensity}, "color": [{}, {}, {}] }}"#,
                color[0], color[1], color[2]
            )
        })
        .collect();
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "lighting_color_test",
            "name": "Lighting Colour Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": {height} }}],
            "ceiling_lights": [{}]
        }}"#,
        entries.join(",")
    );
    LevelDef::from_json(&json).expect("test level parses")
}

/// Shorthand: the luminance of a baked sample, for tests that reason about
/// overall brightness rather than colour.
fn lum(lighting: &LevelLighting, x: f32, y: f32, z: f32) -> f32 {
    lighting.sample(x, y, z).luminance()
}

// ------------------------------------------------------- vertical geometry

#[test]
fn fixture_panel_follows_a_gable_eave_and_ridge() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "gable_light",
            "name": "Gable Light",
            "spawn": { "x": 12.0, "z": 12.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 24.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } } ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 12.0, "brightness": 0.2 },
                { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 1.0, "brightness": 0.2 }
            ]
        }"#,
    )
    .expect("gable light json");
    let lighting = LevelLighting::bake(&level);

    // The ridge fixture hangs just under the 5.0 m ridge, the eave fixture
    // under the 3.0 m eave: they must not share one flat plane.
    let ridge_y = lighting.fixture_y(12.0, 12.0);
    let eave_y = lighting.fixture_y(12.0, 1.0);
    assert!((ridge_y - (5.0 - FIXTURE_DROP_M)).abs() < 1e-3, "{ridge_y}");
    // The eave fixture uses the *lowest* ceiling its panel covers, so it is
    // a little above the eave plane but nowhere near the ridge.
    assert!(
        (3.0..3.4).contains(&eave_y),
        "eave fixture hangs under the slope: {eave_y}"
    );
    assert!(ridge_y - eave_y > 1.5);

    // Mount clearance uses the full body, while source taps sit on the smaller
    // recessed diffuser. A gable must still clear the outer housing footprint.
    let (half_w, half_d) = fixture_half_extents(0.0);
    let profile = fixture_profile("core:fluorescent_panel_01");
    let (source_w, source_d) = profile.emission_shape().half_extents();
    for light in lighting.lights() {
        let mount_y = lighting.fixture_panel_y(light.x(), light.z(), half_w, half_d);
        let expected = profile.emission_y(mount_y);
        assert!((light.y() - expected).abs() < 1e-4);
        assert!(light.half_w() == source_w && light.half_d() == source_d);
        // The panel stays below the ceiling everywhere it hangs.
        assert!(light.y() < 5.0 - FIXTURE_DROP_M + 1e-3);
    }

    // The ridge fixture's pool is centred on the high ceiling: directly
    // under the ridge, the air just below the panel is brighter than the
    // air at eave height in the same column.
    let under_ridge_high = lighting.sample_in_room_luminance(0, 12.0, 4.8, 12.0);
    let under_ridge_low = lighting.sample_in_room_luminance(0, 12.0, 3.0, 12.0);
    assert!(
        under_ridge_high > under_ridge_low,
        "{under_ridge_high} vs {under_ridge_low}"
    );
    // The mirror case at the eave: the fixture there lights its own height.
    let at_eave = lighting.sample_in_room_luminance(0, 12.0, 3.0, 1.0);
    let above_eave = lighting.sample_in_room_luminance(0, 12.0, 4.8, 1.0);
    assert!(at_eave > above_eave, "{at_eave} vs {above_eave}");
}

#[test]
fn elevated_room_fixtures_hang_from_their_own_ceiling() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "elevated_light",
            "name": "Elevated Light",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 2.0 } ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("elevated light json");
    let lighting = LevelLighting::bake(&level);
    assert!((lighting.fixture_y(4.0, 4.0) - (5.0 - FIXTURE_DROP_M)).abs() < 1e-3);
    assert_eq!(lighting.rooms()[0].floor_y, 2.0);

    // The pool reaches the raised floor, not the world floor beneath it.
    let on_elevated_floor = lum(&lighting, 4.0, 2.0, 4.0);
    let below_the_room = lum(&lighting, 4.0, 0.0, 4.0);
    assert!(
        on_elevated_floor > below_the_room,
        "the elevated floor must be the lit one: {on_elevated_floor} vs {below_the_room}"
    );
}

#[test]
fn more_lights_raise_the_room_baseline() {
    let dim = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0]));
    let brighter = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0, 1.0, 1.0, 1.0]));
    assert!(
        brighter.rooms()[0].baseline.luminance() > dim.rooms()[0].baseline.luminance(),
        "4 lights ({:?}) must beat 1 light ({:?})",
        brighter.rooms()[0].baseline,
        dim.rooms()[0].baseline
    );
    assert!(brighter.rooms()[0].baseline.max_channel() <= MAX_BRIGHTNESS);
}

#[test]
fn larger_rooms_are_dimmer_for_the_same_lights() {
    let small = LevelLighting::bake(&level_with_room(10.0, 10.0, 3.5, &[1.0, 1.0]));
    let large = LevelLighting::bake(&level_with_room(30.0, 30.0, 3.5, &[1.0, 1.0]));
    assert!(
        small.rooms()[0].baseline.luminance() > large.rooms()[0].baseline.luminance(),
        "12 m2-class room ({:?}) must beat 900 m2 one ({:?})",
        small.rooms()[0].baseline,
        large.rooms()[0].baseline
    );
    // Both rooms' fixtures are owned; area is the only difference.
    assert_eq!(small.rooms()[0].fixture_count, 2);
    assert_eq!(large.rooms()[0].fixture_count, 2);
}

#[test]
fn brighter_fixtures_raise_the_room_baseline() {
    let weak = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[0.5]));
    let standard = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0]));
    let strong = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[2.0]));
    assert!(weak.rooms()[0].baseline.luminance() < standard.rooms()[0].baseline.luminance());
    assert!(standard.rooms()[0].baseline.luminance() < strong.rooms()[0].baseline.luminance());
}

#[test]
fn a_missing_intensity_behaves_as_a_standard_fixture() {
    let json = r#"{
        "format_version": 3,
        "id": "defaults",
        "name": "Defaults",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 16.0, "depth": 16.0, "height": 3.5 }],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    assert_exact(level.ceiling_lights[0].intensity(), 1.0);
    // An omitted colour is exactly as bright as the documented default.
    assert_eq!(level.ceiling_lights[0].emitted_color(), DEFAULT_LIGHT_COLOR);
    let omitted = LevelLighting::bake(&level);
    let explicit = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0]));
    assert_eq!(omitted.rooms()[0].baseline, explicit.rooms()[0].baseline);
    assert_exact(omitted.lights()[0].intensity(), 1.0);
}

#[test]
fn negative_fixture_brightness_is_sanitized_to_no_light() {
    let json = r#"{
        "format_version": 3,
        "id": "brightness",
        "name": "Brightness",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.4 },
            { "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0, "brightness": -4.0 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    assert_exact(level.ceiling_lights[0].intensity(), 1.4);
    assert_exact(level.ceiling_lights[1].intensity(), 0.0);

    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights().len(), 2);
    // The negative fixture adds no baseline power and no local light.
    let under_negative = lighting.sample(7.0, 0.0, 7.0);
    assert!(under_negative.is_finite());
    assert!(under_negative.is_valid());
}

#[test]
fn higher_ceilings_lower_the_effective_illumination() {
    let low = LevelLighting::bake(&level_with_room(16.0, 16.0, 2.6, &[1.0, 1.0]));
    let normal = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0, 1.0]));
    let tall = LevelLighting::bake(&level_with_room(16.0, 16.0, 5.0, &[1.0, 1.0]));
    assert!(
        low.rooms()[0].baseline.luminance() > normal.rooms()[0].baseline.luminance(),
        "2.6 m ({:?}) should beat 3.5 m ({:?})",
        low.rooms()[0].baseline,
        normal.rooms()[0].baseline
    );
    assert!(
        normal.rooms()[0].baseline.luminance() > tall.rooms()[0].baseline.luminance(),
        "3.5 m ({:?}) should beat 5 m ({:?})",
        normal.rooms()[0].baseline,
        tall.rooms()[0].baseline
    );
    // The correction is gentle, not an inverse square: a 5 m room keeps most
    // of the reference output.
    assert!(tall.lights()[0].height_factor > 0.7);
    assert!(low.lights()[0].height_factor < 1.3);
    // And it must not be a fixed height assumption: taller rooms really do
    // bake a lower fixture panel.
    assert!(tall.lights()[0].y() > normal.lights()[0].y());
    assert!(normal.lights()[0].y() > low.lights()[0].y());
}

#[test]
fn brightness_saturates_instead_of_growing_without_bound() {
    // 200 high-output fixtures in a small room: extreme but finite input.
    let intensities = vec![2.0_f32; 200];
    let lighting = LevelLighting::bake(&level_with_room(4.0, 4.0, 3.5, &intensities));
    let baseline = lighting.rooms()[0].baseline;
    let sparse = LevelLighting::bake(&level_with_room(4.0, 4.0, 3.5, &[2.0]));
    // The dense grid must beat the single fixture by the same relative margin
    // it always did; the fill's absolute span is calibrated separately (see
    // `BASELINE_MAX`), so the comparison is a ratio rather than a fixed
    // brightness step.
    assert!(
        baseline.luminance() > sparse.rooms()[0].baseline.luminance() * 1.05,
        "a dense grid ({baseline:?}) must beat a single fixture ({:?})",
        sparse.rooms()[0].baseline
    );
    // The fill saturates towards its own ceiling rather than towards white:
    // 200 fixtures reach the 0.40 fill ceiling (`BASELINE_MAX`), where the
    // default warm colour's luminance is about 0.37, i.e. the curve has
    // stopped responding to fixture count.
    assert!(
        baseline.max_channel() <= MAX_BRIGHTNESS && baseline.luminance() > 0.35,
        "an absurd fixture count must saturate near the fill ceiling, got {baseline:?}"
    );
    assert!(baseline.is_finite());
    for sample in [
        lighting.sample(0.1, 0.0, 0.1),
        lighting.sample(2.0, 1.6, 2.0),
        lighting.sample(3.9, 2.9, 3.9),
    ] {
        assert!(sample.is_finite());
        assert!(sample.is_valid(), "{sample:?}");
        assert!(sample.max_channel() <= MAX_BRIGHTNESS);
    }

    // Even overflow-sized inputs stay inside the allowed range.
    let extreme = vec![f32::MAX; 4];
    let extreme_lighting = LevelLighting::bake(&level_with_room(2.0, 2.0, 3.5, &extreme));
    let extreme_baseline = extreme_lighting.rooms()[0].baseline;
    assert!(extreme_baseline.is_finite());
    assert!(extreme_baseline.max_channel() <= MAX_BRIGHTNESS);
    assert!(extreme_baseline.luminance() >= AMBIENT_LEVEL);
}

#[test]
fn a_room_without_fixtures_is_dim_but_never_black() {
    let lighting = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[]));
    assert_eq!(lighting.rooms()[0].baseline, ambient_color());
    // Regression guard: the historical 0.55 "ambient" floor must never come
    // back. Ambient may provide visibility, never room illumination.
    const { assert!(AMBIENT_LEVEL < 0.2) };
    let sample = lighting.sample(10.0, 0.0, 10.0);
    assert_eq!(sample, ambient_color());
    // A lit room of the same size is meaningfully brighter than the ambient
    // floor, so the floor is not doing the illumination work. The rebalanced
    // fill spans `[0.10, 0.40]`, so the ratio follows that span rather than
    // the historical 3x.
    let lit = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0]));
    assert!(lit.rooms()[0].baseline.luminance() > sample.luminance() * 2.0);
}

#[test]
fn samples_under_a_fixture_are_brighter_than_distant_samples() {
    let json = r#"{
        "format_version": 3,
        "id": "pool",
        "name": "Pool",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 8.0, "height": 3.0 }],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);

    let beneath = lum(&lighting, 4.0, 0.0, 4.0);
    let near = lum(&lighting, 6.0, 0.0, 4.0);
    let far = lum(&lighting, 20.0, 0.0, 4.0);
    assert!(
        beneath > near,
        "directly beneath ({beneath}) must beat near ({near})"
    );
    assert!(near > far, "near ({near}) must beat far ({far})");
    assert!(
        (far - lighting.rooms()[0].baseline.luminance()).abs() < 1e-4,
        "far from every fixture must sit at the room baseline"
    );
    // Pools are broad, not spotlights: 2 m away still benefits.
    assert!(
        near - far > 0.03,
        "expected a broad pool, got {}",
        near - far
    );
}

#[test]
fn a_fixture_pool_reaches_its_whole_ceiling_without_a_ring() {
    // The pool has to be visible on the ceiling all the way around a fixture,
    // not only straight below it: every ceiling sample sits exactly on the
    // plane of the room's own ceiling body, and the visibility clip must treat
    // that as "touching, not crossing". A rounding error in that clip used to
    // delete the fixture's contribution on ~10% of ceiling samples, which a
    // 12-texels/metre lightmap draws as hard dark rings around every fixture.
    let level = level_with_room(16.0, 16.0, 2.7, &[0.5]);
    let lighting = LevelLighting::bake(&level);
    let ceiling_y = 2.7;
    let baseline = lighting.baseline_in_room(0, 8.0, 8.0).luminance();
    // The pool is present at every radius out to 6 m, not only straight below.
    let ceiling = |radius: f32| {
        lighting
            .sample_in_room(0, 8.0 + radius, ceiling_y, 8.0)
            .luminance()
    };
    for radius in [0.0_f32, 0.6, 1.0, 1.3, 2.0, 2.7, 3.2, 4.0, 4.5] {
        let value = ceiling(radius);
        assert!(
            value > baseline + 0.05,
            "the ceiling pool is missing at r = {radius} m: {value} against a baseline of {baseline}"
        );
    }
    // And it varies smoothly: the artifact was a ~0.24 step between two
    // neighbouring samples 2 cm apart.
    let mut previous = ceiling(0.0);
    let mut worst: f32 = 0.0;
    for step in 1..=300_u16 {
        let value = ceiling(f32::from(step) * 0.02);
        worst = worst.max((value - previous).abs());
        previous = value;
    }
    assert!(
        worst < 0.01,
        "the ceiling pool has a ring: largest 2 cm step {worst}"
    );
}

#[test]
fn local_pools_scale_with_fixture_intensity() {
    // One fixture at a known position in a large room, sampled directly
    // beneath it: a 2.0 fixture must out-light a 0.5 fixture clearly.
    let level = |intensity: f32| {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "pool_intensity",
                "name": "Pool Intensity",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 3.0 }}],
                "ceiling_lights": [{{
                    "fixture": "core:fluorescent_panel_01",
                    "x": 10.0, "z": 10.0, "brightness": {intensity}
                }}]
            }}"#
        ))
        .expect("test level parses")
    };
    let weak = LevelLighting::bake(&level(0.5));
    let strong = LevelLighting::bake(&level(2.0));
    let weak_under = lum(&weak, 10.0, 0.0, 10.0);
    let strong_under = lum(&strong, 10.0, 0.0, 10.0);
    assert!(
        strong_under > weak_under + 0.1,
        "2.0 fixture ({strong_under}) must clearly beat 0.5 ({weak_under})"
    );
    // Both stay inside the legal range.
    for value in [weak_under, strong_under] {
        assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value));
    }
}

/// Two differently lit rooms sharing a wall. `opening` adds a walk-through
/// doorway; without it the wall is solid.
fn two_room_level(opening: bool) -> LevelDef {
    let openings = if opening {
        r#"[{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#
    } else {
        "[]"
    };
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "blend",
            "name": "Blend",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [
                {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }},
                {{ "x": 10.4, "z": 0.0, "width": 40.0, "depth": 20.0, "height": 3.0 }}
            ],
            "walls": [{{
                "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                "openings": {openings}
            }}],
            "ceiling_lights": [
                {{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 2.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 8.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 8.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0 }},
                {{ "fixture": "core:fluorescent_panel_01", "x": 30.0, "z": 2.0 }}
            ]
        }}"#
    );
    LevelDef::from_json(&json).expect("test level parses")
}

#[test]
fn openings_blend_between_differently_lit_rooms() {
    let level = two_room_level(true);
    let solid = LevelLighting::bake(&two_room_level(false));
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.rooms().len(), 2);

    let baseline_bright = lighting.rooms()[0].baseline.luminance();
    let baseline_dim = lighting.rooms()[1].baseline.luminance();
    assert!(
        baseline_bright > baseline_dim + 0.1,
        "test setup needs contrasting rooms: {baseline_bright} vs {baseline_dim}"
    );

    // Sampling below y = 3 m near the doorway, on both sides of the wall.
    let bright_near_door = lum(&lighting, 9.9, 0.0, 5.0);
    let dim_near_door = lum(&lighting, 10.5, 0.0, 5.0);
    let bright_without_opening = solid.sample_in_room(0, 9.9, 0.0, 5.0).luminance();
    let dim_without_opening = solid.sample_in_room(1, 10.5, 0.0, 5.0).luminance();

    // The doorway pulls each side towards the other room...
    assert!(
        bright_near_door < bright_without_opening - 1e-3,
        "the bright side must lose light to the dim room: {bright_near_door} vs {bright_without_opening}"
    );
    assert!(
        dim_near_door > dim_without_opening + 1e-3,
        "the dim side must gain light from the bright room: {dim_near_door} vs {dim_without_opening}"
    );
    // ...through an exchange that cancels exactly across the threshold: each
    // face of the wall is the same distance from the opening, so the blend
    // is equal and opposite. Local fixture pools are *not* part of the
    // exchange any more — a fixture behind the wall no longer lights the
    // other side — so the blend is measured on its own.
    let left_blend = lighting.opening_blend(0, 9.95, 0.0, 5.0);
    let right_blend = lighting.opening_blend(1, 10.45, 0.0, 5.0);
    assert!(
        left_blend.luminance() < -0.001 && right_blend.luminance() > 0.001,
        "the doorway must exchange light: {left_blend:?} vs {right_blend:?}"
    );
    assert!(
        (left_blend.luminance() + right_blend.luminance()).abs() < 0.01,
        "the doorway seam must cancel: {left_blend:?} vs {right_blend:?}"
    );
    // The exchange is bounded by half the baseline difference at the
    // opening (OPENING_BLEND_STRENGTH), so a very bright neighbour cannot
    // turn the threshold into a hole in the wall.
    for blend in [left_blend, right_blend] {
        assert!(
            blend.luminance().abs() <= 0.5 * (baseline_bright - baseline_dim) + 0.02,
            "the doorway exchange must stay bounded: {blend:?}"
        );
    }
    for value in [bright_near_door, dim_near_door] {
        assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value), "{value}");
    }

    // The influence is bounded: far from the opening the rooms keep their
    // own baselines (local pools are identical in both bakes).
    for (room, x, z) in [(0usize, 1.0f32, 5.0f32), (1, 30.0, 15.0)] {
        let open = lighting.sample_in_room(room, x, 0.0, z);
        let closed = solid.sample_in_room(room, x, 0.0, z);
        assert!(
            (open.luminance() - closed.luminance()).abs() < 1e-4,
            "room {room} at ({x}, {z}) must not be blended from {OPENING_BLEND_RADIUS_M} m away: {open:?} vs {closed:?}"
        );
    }
}

#[test]
fn openings_do_not_blend_through_solid_walls() {
    let json = r#"{
        "format_version": 3,
        "id": "solid",
        "name": "Solid",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
            { "x": 10.4, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }
        ],
        "walls": [{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0 }],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    let a = lighting.rooms()[0].baseline;
    let b = lighting.rooms()[1].baseline;
    assert!(a.luminance() > b.luminance(), "only room A has fixtures");
    // Deep inside room B, including just past the solid wall, nothing leaks.
    for x in [12.0, 18.0] {
        let sample = lighting.sample_in_room(1, x, 0.0, 5.0);
        assert!(
            (sample.luminance() - b.luminance()).abs() < 1e-4,
            "solid wall leaked light at x = {x}"
        );
    }
}

#[test]
fn overlapping_rooms_own_lights_deterministically_and_only_once() {
    // A big room overlapped by a small one. The fixture inside both belongs
    // to the smaller room only, so the small room is bright and the big one
    // receives nothing.
    let json = r#"{
        "format_version": 3,
        "id": "overlap",
        "name": "Overlap",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 40.0, "depth": 40.0, "height": 3.0 },
            { "x": 4.0, "z": 4.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
        ],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.rooms()[0].fixture_count, 0);
    assert_eq!(lighting.rooms()[1].fixture_count, 1);
    assert_eq!(lighting.room_index_at(7.0, 7.0), Some(1));
    assert_eq!(lighting.room_index_at(30.0, 30.0), Some(0));
    assert_eq!(lighting.room_index_at(-1.0, -1.0), None);
    // The light is counted once, in the small room, per channel.
    let light = lighting.lights()[0];
    let power = light.intensity() * light.height_factor;
    let summed = LightColor {
        r: lighting.rooms()[0].effective_power.r + lighting.rooms()[1].effective_power.r,
        g: lighting.rooms()[0].effective_power.g + lighting.rooms()[1].effective_power.g,
        b: lighting.rooms()[0].effective_power.b + lighting.rooms()[1].effective_power.b,
    };
    for channel in 0..3 {
        assert_exact(
            summed.channel(channel),
            power * light.color().channel(channel),
        );
    }
    assert_eq!(lighting.rooms()[0].baseline, ambient_color());
}

#[test]
fn malformed_inputs_stay_finite_and_never_panic() {
    let mut level = level_with_room(10.0, 10.0, 3.0, &[1.0]);
    level.ceiling_lights[0].x = f32::NAN;
    level.ceiling_lights[0].brightness = Some(f32::NAN);
    level.ceiling_lights[0].color = Some(LightColor::rgb(f32::NAN, f32::INFINITY, -3.0));
    level.rooms[0].height = -2.0;
    let lighting = LevelLighting::bake(&level);
    assert!(
        lighting.lights().is_empty(),
        "non-finite fixtures are skipped"
    );
    assert!(lighting.sample(0.0, 0.0, 0.0).is_finite());
    assert!(lighting.sample(f32::NAN, 0.0, 3.0).is_finite());

    // Extreme-but-finite levels must not overflow or produce NaN.
    let mut extreme = level_with_room(1.0e30, 1.0e30, 3.5, &[1.0, 1.0]);
    extreme.ceiling_lights[0].brightness = Some(f32::MAX);
    extreme.ceiling_lights[1].brightness = Some(f32::INFINITY);
    extreme.ceiling_lights[0].color = Some(LightColor::rgb(f32::MAX, f32::NAN, -0.5));
    extreme.ceiling_lights[1].x = f32::NAN; // dropped entirely
    let extreme_lighting = LevelLighting::bake(&extreme);
    let baseline = extreme_lighting.rooms()[0].baseline;
    assert!(baseline.is_finite() && baseline.max_channel() <= MAX_BRIGHTNESS);
    assert!(extreme_lighting.sample(1.0, 0.0, 1.0).is_finite());
}

#[test]
fn helper_curves_are_monotonic_and_numerically_safe() {
    assert_exact(saturating_brightness(0.0), 0.0);
    assert_exact(saturating_brightness(-1.0), 0.0);
    assert_exact(saturating_brightness(f32::NAN), 0.0);
    assert_exact(saturating_brightness(f32::INFINITY), 1.0);
    assert_exact(saturating_brightness(f32::NEG_INFINITY), 0.0);
    let mut previous = 0.0;
    for step in 0_i32..40_i32 {
        let value = saturating_brightness(crate::test_support::exact_f32(step) * 0.25);
        assert!(value > previous || step == 0_i32 || previous > 0.99);
        assert!((0.0..=1.0).contains(&value));
        previous = value;
    }
    // Adding lights keeps increasing brightness, but ever more slowly.
    let first = saturating_brightness(0.5) - saturating_brightness(0.0);
    let later = saturating_brightness(4.5) - saturating_brightness(4.0);
    assert!(first > later && later > 0.0);

    assert_exact(smooth_falloff(0.0), 1.0);
    assert_exact(smooth_falloff(1.0), 0.0);
    assert_exact(smooth_falloff(f32::NAN), 0.0);
    assert!(smooth_falloff(0.5) > 0.0 && smooth_falloff(0.5) < 1.0);

    assert_exact(sanitize_intensity(f32::NAN), 1.0);
    assert_exact(sanitize_intensity(-3.0), 0.0);
    assert_exact(sanitize_intensity(1.0e30), MAX_LIGHT_INTENSITY);
    assert_exact(sanitize_intensity(1.4), 1.4);

    assert_exact(ceiling_height_factor(f32::NAN), 1.0);
    assert_exact(ceiling_height_factor(0.0), 1.0);
    assert_exact(ceiling_height_factor(REFERENCE_CEILING_HEIGHT_M), 1.0);
    assert!(ceiling_height_factor(2.6) > 1.0 && ceiling_height_factor(2.6) < 1.3);
    assert!(ceiling_height_factor(6.0) > 0.6 && ceiling_height_factor(6.0) < 1.0);

    assert_eq!(light_grid_cells(0.0), 1);
    assert_eq!(light_grid_cells(2.0), 1);
    assert_eq!(light_grid_cells(6.0), 3);
    assert_eq!(light_grid_cells(1.0e30), MAX_LIGHT_GRID_CELLS);
    assert_eq!(wall_light_segments(1.0), 1);
    assert_eq!(wall_light_segments(1000.0), MAX_WALL_LIGHT_SEGMENTS);

    // Density compression: zero stays zero, the curve is monotonic and it
    // never exceeds the saturating curve's own input.
    assert_exact(compressed_density(0.0), 0.0);
    assert_exact(compressed_density(-1.0), 0.0);
    assert_exact(compressed_density(f32::NAN), 0.0);
    assert_exact(compressed_density(f32::INFINITY), f32::INFINITY);
    let mut previous_density = 0.0;
    for step in 1_i32..40_i32 {
        let value = compressed_density(crate::test_support::exact_f32(step) * 0.5);
        assert!(value > previous_density, "compression must be monotonic");
        assert!(value < crate::test_support::exact_f32(step) * 0.5 || step == 1_i32);
        previous_density = value;
    }

    // Colour helpers stay finite and bounded.
    assert_eq!(
        LightColor::rgb(f32::NAN, f32::INFINITY, -1.0).sanitized(),
        LightColor::rgb(0.0, MAX_LIGHT_COLOR, 0.0)
    );
    assert_eq!(
        LightColor::rgb(0.25, 0.5, 0.75).clamped(0.4, 0.6),
        LightColor::rgb(0.4, 0.5, 0.6)
    );
    assert_eq!(
        LightColor::BLACK.mix(LightColor::WHITE, 0.5),
        LightColor::grey(0.5)
    );
    assert_eq!(
        LightColor::WHITE.mix(LightColor::BLACK, f32::NAN),
        LightColor::WHITE
    );
    assert!(DEFAULT_LIGHT_COLOR.is_valid());
    const { assert!(DEFAULT_LIGHT_COLOR.r >= DEFAULT_LIGHT_COLOR.g) };
    const { assert!(DEFAULT_LIGHT_COLOR.g >= DEFAULT_LIGHT_COLOR.b) };
    const { assert!(DEFAULT_LIGHT_COLOR.b > 0.7) };
    assert!(ambient_color().is_valid());
}

#[test]
fn brightness_changes_smoothly_instead_of_in_bands() {
    // One fixture in a 20x20 room: scanning the floor in 5 cm steps must
    // never produce a visible jump, so rooms have gradients rather than
    // discrete brightness tiers.
    let level = level_with_room(20.0, 20.0, 3.0, &[1.0]);
    let lighting = LevelLighting::bake(&level);
    let mut previous = lum(&lighting, 0.0, 0.0, 10.0);
    for x in scan(0.0, 0.05, 20.0) {
        let current = lum(&lighting, x, 0.0, 10.0);
        assert!(
            (current - previous).abs() < 0.02,
            "brightness jumped at x = {x}: {previous} -> {current}"
        );
        previous = current;
    }
    // And it genuinely varies across the room.
    assert!(lum(&lighting, 10.0, 0.0, 10.0) - lum(&lighting, 0.0, 0.0, 10.0) > 0.05);
}

#[test]
fn summaries_describe_the_bake() {
    let lighting = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0, 1.0, 1.0, 1.0]));
    let summary = lighting.summary();
    assert_eq!(summary.rooms, 1);
    assert_eq!(summary.lights, 4);
    assert_exact(summary.min_baseline, summary.max_baseline);
    assert!(summary.average_baseline >= AMBIENT_LEVEL);
    assert!(summary.average_baseline <= MAX_BRIGHTNESS);
}

#[test]
fn fixture_plane_follows_the_room_ceiling() {
    let json = r#"{
        "format_version": 3,
        "id": "heights",
        "name": "Heights",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
            { "x": 12.0, "z": 0.0, "width": 10.0, "depth": 4.0, "height": 2.6 }
        ],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0 },
            { "fixture": "core:fluorescent_panel_01", "x": 17.0, "z": 2.0 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    assert!((lighting.fixture_y(5.0, 5.0) - 2.99).abs() < 1e-4);
    assert!((lighting.fixture_y(17.0, 2.0) - 2.59).abs() < 1e-4);
    assert_eq!(lighting.lights()[0].room, Some(0));
    assert_eq!(lighting.lights()[1].room, Some(1));
}

#[test]
fn vertically_offset_samples_follow_their_true_position() {
    // Samples at the same (x, z) but different heights follow the fixture's
    // real position: directly under the emitter a horizontal surface sees the
    // same pool at any height (the area light's result), a point one metre to
    // the side at the emitter's own height is darker than the floor beneath
    // (the directional lobe), and a distant point is darker still.
    let json = r#"{
        "format_version": 3,
        "id": "height_sample",
        "name": "Height Sample",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
        "ceiling_lights": [{
            "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.4
        }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    let floor = lum(&lighting, 5.0, 0.0, 5.0);
    let just_below_panel = lum(&lighting, 5.0, 2.8, 5.0);
    let beside_at_panel_height = lum(&lighting, 6.0, 2.8, 5.0);
    let distant = lum(&lighting, 8.0, 0.0, 5.0);
    assert!(
        (just_below_panel - floor).abs() < 0.04,
        "under the emitter the direct pool is height-independent (the bounce fill still \
         falls with distance): {just_below_panel} vs {floor}"
    );
    assert!(
        beside_at_panel_height < floor - 0.1,
        "a sample at the emitter's height, off to the side, is weaker: \
         {beside_at_panel_height} vs {floor}"
    );
    assert!(
        distant < floor - 0.1,
        "a distant sample is weaker than the floor beneath: {distant} vs {floor}"
    );
}

#[test]
fn sample_points_use_the_smaller_overlapping_room() {
    // The documented ownership rule is shared by light ownership and point
    // sampling, so a sample inside the overlap resolves to the small room.
    let json = r#"{
        "format_version": 3,
        "id": "sample_overlap",
        "name": "Sample Overlap",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 40.0, "depth": 40.0, "height": 3.0 },
            { "x": 4.0, "z": 4.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
        ],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.room_index_at(7.0, 7.0), Some(1));
    let inside = lum(&lighting, 7.0, 0.0, 7.0);
    let outside = lum(&lighting, 30.0, 0.0, 30.0);
    assert!(inside > outside, "the small room owns the light");
    assert!(outside >= AMBIENT_LEVEL);
}

// -----------------------------------------------------------------------
// RGB lighting
// -----------------------------------------------------------------------

// A few scalar queries protect the coloured recovery target from desaturation
// without baking any fixture; white-light calibration must remain exact.
#[test]
fn baseline_compression_preserves_rgb_power_ratios_and_white_calibration() {
    for area in [9.0, 432.0] {
        for power in [0.01, 0.5, 8.0] {
            let white = room_baseline(area, LightColor::grey(power));
            let component =
                saturating_brightness(compressed_density(power / area * REFERENCE_LIGHT_AREA_M2));
            let expected = (BASELINE_MAX - AMBIENT_LEVEL).mul_add(component, AMBIENT_LEVEL);
            assert_eq!(white, LightColor::grey(expected));
            for colour in [[1.0, 0.1, 0.45], [0.0, 0.2, 1.0]] {
                let baseline = room_baseline(area, LightColor::from(colour.map(|c| c * power)));
                let amplitude = white.r - AMBIENT_LEVEL;
                for (actual, ratio) in baseline.to_array().into_iter().zip(colour) {
                    assert!((actual - AMBIENT_LEVEL - amplitude * ratio).abs() < 1.0e-7);
                }
            }
        }
    }
    for power in [
        LightColor::BLACK,
        LightColor::rgb(f32::NAN, f32::NEG_INFINITY, -1.0),
        LightColor::rgb(f32::INFINITY, 1.0, 0.0),
    ] {
        let baseline = room_baseline(f32::NAN, power);
        assert!(baseline.is_finite());
        assert!(baseline.min_channel() >= AMBIENT_LEVEL);
        assert!(baseline.max_channel() <= BASELINE_MAX);
    }
}

// Four 35-cell baseline grids and 25 receiver targets protect real partition
// isolation and a continuous ceiling at sub-cell wall boundaries, without GI.
#[test]
fn corridor_baseline_uses_its_actual_wall_side_and_storey() {
    use super::lightmap::{Chart, LightmapPatch, PatchKind};

    for opening_height in [2.35_f32, 2.8_f32] {
        let mut original = None;
        for lower_wall in [false, true] {
            let extra_wall = if lower_wall {
                r#",{"x":3.9,"z":0,"width":0.2,"depth":6,"y":0,"height":2.8}"#
            } else {
                ""
            };
            let level = LevelDef::from_json(&format!(
                r#"{{"format_version":3,"id":"corridor_baseline","name":"Corridor baseline",
                "spawn":{{"x":4,"z":3}},
                "rooms":[{{"x":0,"z":0,"width":8,"depth":6,"floor_y":3.6,"height":2.8}}],
                "walls":[
                  {{"x":0,"z":1.88,"width":8,"depth":0.24,"y":3.6,"height":2.8,
                    "openings":[{{"kind":"passage","offset":3.4,"width":1.2,"height":{opening_height}}}]}},
                  {{"x":0,"z":3.88,"width":8,"depth":0.24,"y":3.6,"height":2.8}}
                  {extra_wall}],
                "ceiling_lights":[{{"fixture":"core:fluorescent_panel_01","x":4,"z":3,
                    "y":6.4,"align":"none","brightness":0.5,"color":[1,1,1]}}]}}"#
            ))
            .expect("small partition fixture");
            let lighting = LevelLighting::bake(&level);
            let corridor = lighting.baseline_in_room(0, 4.0, 3.0);
            assert!(corridor.min_channel() > AMBIENT_LEVEL);
            if let Some(baseline) = original {
                assert_eq!(
                    corridor, baseline,
                    "another storey's wall changed the baseline"
                );
            } else {
                original = Some(corridor);
            }
            for z in [2.13, 2.3, 2.41, 3.0, 3.87] {
                assert_eq!(lighting.baseline_in_room(0, 4.0, z), corridor);
            }
            assert_eq!(lighting.baseline_in_room(0, 4.0, 5.4), ambient_color());
            let north = lighting.baseline_in_room(0, 4.0, 1.0);
            if opening_height < 2.65_f32 {
                assert_eq!(
                    north,
                    ambient_color(),
                    "the solid header must isolate the north area"
                );
            } else {
                assert_eq!(
                    north, corridor,
                    "a full-height aperture must retain its connected area"
                );
            }
            let patch = LightmapPatch::from_quad(
                PatchKind::Ceiling,
                [
                    [0.0, 6.4, 2.12],
                    [8.0, 6.4, 2.12],
                    [8.0, 6.4, 3.88],
                    [0.0, 6.4, 3.88],
                ],
                Some(0),
            )
            .expect("small ceiling");
            let chart = Chart {
                page: 0,
                x: 2,
                y: 2,
                width: 5,
                height: 5,
            };
            for target in super::transport::receiver_targets(&lighting, &[(patch, chart)]) {
                for (actual, baseline) in target.into_iter().zip(corridor.to_array()) {
                    assert!((actual - (baseline - AMBIENT_LEVEL)).abs() < 1.0e-7);
                }
            }
        }
    }
}

#[test]
fn a_red_fixture_lights_geometry_red() {
    let level = level_with_colored_room(20.0, 20.0, 3.5, &[(1.0, [1.0, 0.0, 0.0])]);
    let lighting = LevelLighting::bake(&level);
    let baseline = lighting.rooms()[0].baseline;
    assert!(
        baseline.r > baseline.g + 0.1 && baseline.r > baseline.b + 0.1,
        "the red channel must dominate the baseline, got {baseline:?}"
    );
    let beneath = lighting.sample(10.0, 0.0, 10.0);
    assert!(
        beneath.r > beneath.g + 0.2 && beneath.r > beneath.b + 0.2,
        "the floor beneath the fixture must be clearly red, got {beneath:?}"
    );
    // The unlit channels stay at the ambient floor: this is colour, not a
    // uniform desaturation.
    assert_exact(beneath.g, AMBIENT_LEVEL);
    assert_exact(beneath.b, AMBIENT_LEVEL);
}

#[test]
fn a_blue_fixture_lights_geometry_blue() {
    let level = level_with_colored_room(20.0, 20.0, 3.5, &[(1.0, [0.0, 0.1, 1.0])]);
    let lighting = LevelLighting::bake(&level);
    let beneath = lighting.sample(10.0, 0.0, 10.0);
    assert!(
        beneath.b > beneath.r + 0.3 && beneath.b > beneath.g + 0.2,
        "the floor beneath the fixture must be clearly blue, got {beneath:?}"
    );
    assert!(
        beneath.r < beneath.g,
        "the near-zero red channel must stay dim"
    );
}

#[test]
fn warm_and_cool_fixtures_mix_without_losing_either_colour() {
    // Two fixtures on one axis. Each light is baked alone first, then both
    // together: the blend in the middle must gain from both additions
    // instead of one colour replacing the other.
    let scenario = |lights: &str| {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "mix_two",
                "name": "Mix Two",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 16.0, "depth": 16.0, "height": 3.0 }}],
                "ceiling_lights": [{lights}]
            }}"#
        ))
        .expect("valid json")
    };
    let warm = r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 8.0,
                    "color": [1.0, 0.55, 0.1] }"#;
    let cool = r#"{ "fixture": "core:fluorescent_panel_01", "x": 11.0, "z": 8.0,
                    "color": [0.2, 0.4, 1.0] }"#;
    let both = LevelLighting::bake(&scenario(&format!("{warm},{cool}")));
    let only_warm = LevelLighting::bake(&scenario(warm));
    let only_cool = LevelLighting::bake(&scenario(cool));

    // Near the warm fixture the warm channels dominate...
    let near_warm = both.sample(5.0, 1.6, 8.0);
    assert!(
        near_warm.r > near_warm.b,
        "warm fixture must dominate nearby, got {near_warm:?}"
    );
    // ...near the cool one the blue channel does...
    let near_cool = both.sample(11.0, 1.6, 8.0);
    assert!(
        near_cool.b > near_cool.r,
        "cool fixture must dominate nearby, got {near_cool:?}"
    );
    // ...and the transition region keeps contributions from both: adding
    // the cool fixture must raise blue without removing the warm fixture's
    // red, and vice versa.
    let middle = both.sample(8.0, 1.6, 8.0);
    let middle_warm_only = only_warm.sample(8.0, 1.6, 8.0);
    let middle_cool_only = only_cool.sample(8.0, 1.6, 8.0);
    assert!(
        middle.b > middle_warm_only.b + 0.05,
        "the cool fixture must add blue in the middle: {middle:?} vs {middle_warm_only:?}"
    );
    assert!(
        middle.r > middle_cool_only.r + 0.05,
        "the warm fixture must add red in the middle: {middle:?} vs {middle_cool_only:?}"
    );
    assert!(
        middle.r > AMBIENT_LEVEL + 0.2 && middle.b > AMBIENT_LEVEL + 0.2,
        "both channels must survive in the transition region: {middle:?}"
    );
}

#[test]
fn three_arbitrary_colours_accumulate_independently() {
    // Red, green and blue fixtures across one room: every channel must be
    // able to accumulate from an arbitrary fixture, which rules out two
    // hardcoded fixture modes.
    let json = r#"{
        "format_version": 3,
        "id": "mix_three",
        "name": "Mix Three",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 18.0, "depth": 6.0, "height": 3.0 }],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0,
              "color": [1.0, 0.0, 0.0] },
            { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 3.0,
              "color": [0.0, 1.0, 0.0] },
            { "fixture": "core:fluorescent_panel_01", "x": 15.0, "z": 3.0,
              "color": [0.0, 0.0, 1.0] }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights().len(), 3);
    for (x, channel, name) in [
        (3.0_f32, 0_usize, "red"),
        (9.0, 1, "green"),
        (15.0, 2, "blue"),
    ] {
        let sample = lighting.sample(x, 1.6, 3.0);
        assert!(
            sample.channel(channel) > sample.channel((channel + 1) % 3) + 0.2,
            "the {name} fixture at x = {x} must dominate its own channel, got {sample:?}"
        );
    }
    // A point between all three keeps every channel above ambient: the
    // system accumulates arbitrary RGB fixtures rather than two modes.
    let middle = lighting.sample(9.0, 0.0, 3.0);
    for channel in 0..3 {
        assert!(
            middle.channel(channel) > AMBIENT_LEVEL + 0.02,
            "channel {channel} vanished in the three-colour blend: {middle:?}"
        );
    }
}

#[test]
fn coloured_accumulation_stays_finite_and_bounded() {
    // A dense grid of saturated colours: every channel must stay finite and
    // inside the render range even where pools overlap.
    let lights: Vec<(f32, [f32; 3])> = (0_i32..36_i32)
        .map(|index| {
            let color = match index % 3_i32 {
                0_i32 => [1.0, 0.0, 0.0],
                1_i32 => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            };
            (2.0, color)
        })
        .collect();
    let level = level_with_colored_room(4.0, 4.0, 2.6, &lights);
    let lighting = LevelLighting::bake(&level);
    let mut samples = Vec::new();
    for x in scan(0.0, 0.25, 4.0) {
        samples.push(lighting.sample(x, 0.0, 2.0));
    }
    for sample in &samples {
        assert!(sample.is_finite());
        assert!(
            sample.is_valid(),
            "channel escaped the legal range: {sample:?}"
        );
        assert!(sample.max_channel() <= MAX_BRIGHTNESS);
    }
    // The three colours really are all present in the room.
    let baseline = lighting.rooms()[0].baseline;
    for channel in 0..3 {
        assert!(
            baseline.channel(channel) > AMBIENT_LEVEL + 0.05,
            "channel {channel} missing from the mixed room: {baseline:?}"
        );
    }
}

#[test]
fn a_colour_that_emits_nothing_is_dark_but_valid() {
    let level = level_with_colored_room(20.0, 20.0, 3.5, &[(1.0, [0.0, 0.0, 0.0])]);
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_eq!(lighting.rooms()[0].baseline, ambient_color());
    assert_eq!(lighting.sample(10.0, 0.0, 10.0), ambient_color());
}

#[test]
fn intensities_multiply_the_authored_colour_per_channel() {
    // The same colour at half intensity must be dimmer in every channel it
    // emits, and a coloured fixture must not brighten channels it does not
    // emit.
    let level =
        |intensity: f32| level_with_colored_room(16.0, 16.0, 3.0, &[(intensity, [0.4, 0.6, 0.9])]);
    let dim = LevelLighting::bake(&level(0.5));
    let bright = LevelLighting::bake(&level(2.0));
    let dim_baseline = dim.rooms()[0].baseline;
    let bright_baseline = bright.rooms()[0].baseline;
    assert!(bright_baseline.r > dim_baseline.r);
    assert!(bright_baseline.g > dim_baseline.g);
    assert!(bright_baseline.b > dim_baseline.b);
    // Hue order is preserved: blue > green > red at both intensities.
    assert!(bright_baseline.b > bright_baseline.g);
    assert!(bright_baseline.g > bright_baseline.r);
    assert!(dim_baseline.b > dim_baseline.g);
    assert!(dim_baseline.g > dim_baseline.r);
}

#[test]
fn levels_without_a_colour_use_the_documented_default() {
    // Every shipped level omits `color`; they must bake the restrained warm
    // default rather than turning white or black.
    let omitted = r#"{
        "format_version": 3,
        "id": "default_colour",
        "name": "Default Colour",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
            { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 9.0, "brightness": 0.75 }
        ]
    }"#;
    let level = LevelDef::from_json(omitted).expect("default-colour level parses");
    assert_eq!(level.ceiling_lights[0].color, None);
    assert_eq!(level.ceiling_lights[0].emitted_color(), DEFAULT_LIGHT_COLOR);
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights()[0].color(), DEFAULT_LIGHT_COLOR);
    let baseline = lighting.rooms()[0].baseline;
    assert!(
        baseline.r > baseline.b && baseline.b >= AMBIENT_LEVEL,
        "a colourless fixture must bake the warm default, got {baseline:?}"
    );
    let sample = lighting.sample(3.0, 0.0, 3.0);
    assert!(sample.r > sample.b);
}

/// One room carrying the three shipped fixture families.
fn level_with_fixture_families() -> LevelDef {
    let json = r#"{
        "format_version": 3,
        "id": "fixtures",
        "name": "Fixture Families",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 14.0, "depth": 10.0, "height": 4.0 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_round", "x": 3.0, "z": 3.0,
              "color": [0.7, 0.85, 1.0], "brightness": 1.0 },
            { "fixture": "core:pool_light_wall", "x": 10.0, "z": 0.2,
              "mount": "wall", "y": 2.2, "color": [0.7, 0.85, 1.0], "brightness": 0.8 },
            { "fixture": "core:does_not_exist", "x": 7.0, "z": 8.0 },
            { "fixture": "home:ceiling_light_round", "x": 11.0, "z": 8.0 }
        ]
    }"#;
    LevelDef::from_json(json).expect("fixture level parses")
}

#[test]
fn fixture_families_own_their_footprint_and_mount() {
    // The catalog id selects the fixture family: the office panel keeps its
    // 1.2 x 0.6 m footprint, the round downlight is a 0.5 m body, and the
    // wall luminaire is a thin strip on the wall.
    let panel = fixture_profile("core:fluorescent_panel_01");
    assert_eq!(panel.kind, FixtureKind::FluorescentPanel);
    assert_eq!((panel.half_width, panel.half_depth), (0.6, 0.3));
    let round = fixture_profile("core:pool_light_round");
    assert_eq!(round.kind, FixtureKind::RoundRecessed);
    assert_eq!((round.half_width, round.half_depth), (0.25, 0.25));
    let wall = fixture_profile("core:pool_light_wall");
    assert_eq!(wall.kind, FixtureKind::WallSconce);
    assert!(wall.half_width > wall.half_depth);
    // An unknown or future fixture id falls back to the office panel rather
    // than failing to light the room.
    assert_eq!(
        fixture_profile("core:does_not_exist").kind,
        FixtureKind::FluorescentPanel
    );
    assert_eq!(
        fixture_half_extents_for(FixtureKind::RoundRecessed, 90.0),
        (0.22, 0.22),
        "a round fixture is rotation-invariant"
    );

    let flush = fixture_profile("home:ceiling_light_round");
    assert_eq!(flush.kind, FixtureKind::FlushMount);
    let flush_extents = (
        crate::engine::lighting::FLUSH_MOUNT_RADIUS_M,
        crate::engine::lighting::FLUSH_MOUNT_RADIUS_M,
    );
    assert_eq!((flush.half_width, flush.half_depth), flush_extents,);
    assert_eq!(
        fixture_half_extents_for(FixtureKind::FlushMount, 90.0),
        flush_extents,
        "a round fixture is rotation-invariant"
    );

    let level = level_with_fixture_families();
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights().len(), 5);

    // A ceiling fixture hangs just below the room ceiling; a wall fixture
    // stays at its authored height.
    let round_light = lighting.lights()[lighting.fixture_light_index(0).expect("round source")];
    assert!(
        (round_light.y() - round.emission_y(4.0 - FIXTURE_DROP_M)).abs() < 1e-4,
        "{round_light:?}"
    );
    let wall_sources = lighting.fixture_light_indices(1);
    assert_eq!(wall_sources.len(), 2, "separated wall apertures");
    for (index, offset) in wall_sources.into_iter().zip([-0.045, 0.045]) {
        let wall_light = lighting.lights()[index];
        assert!(
            (wall_light.y() - (2.2 + offset)).abs() < 1e-4,
            "{wall_light:?}"
        );
        assert!((wall_light.half_w() - 0.176).abs() < 1e-4);
    }
    // The residential flush mount hangs just below the ceiling like the round
    // pool downlight, with its own disc footprint.
    let flush_light = lighting.lights()[lighting.fixture_light_index(3).expect("flush source")];
    assert!(
        (flush_light.y() - (4.0 - FIXTURE_DROP_M)).abs() < 1e-4,
        "{flush_light:?}"
    );
    assert!((flush_light.half_w() - crate::engine::lighting::FLUSH_MOUNT_RADIUS_M).abs() < 1e-4);
    // The unknown id uses the panel's emitting aperture, not its opaque body.
    let unknown = lighting.lights()[lighting.fixture_light_index(2).expect("fallback source")];
    assert!((unknown.half_w() - (0.6 - PANEL_BORDER_WIDTH_M)).abs() < 1e-4);
    assert!(
        (unknown.y() - fixture_profile("core:does_not_exist").emission_y(4.0 - FIXTURE_DROP_M))
            .abs()
            < 1e-4
    );

    // A hand-edited wall fixture without a height stays finite.
    let json = r#"{
        "format_version": 3,
        "id": "wallless",
        "name": "Wallless",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_wall", "x": 3.0, "z": 0.2, "mount": "wall" }
        ]
    }"#;
    let wall_level = LevelDef::from_json(json).expect("wall fixture parses");
    let wall_lighting = LevelLighting::bake(&wall_level);
    let mount = wall_lighting.fixture_y_for(&wall_level.ceiling_lights[0]);
    assert!(mount.is_finite());
    assert!(
        (mount - WALL_LIGHT_DEFAULT_HEIGHT_M).abs() < 1e-4,
        "{mount}"
    );

    // A room lit only by the cool round fixture must read cool; mixing a
    // warm fixture in stays a blend, which the RGB lighting tests cover.
    let cool_only = r#"{
        "format_version": 3,
        "id": "cool_only",
        "name": "Cool Only",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_round", "x": 4.0, "z": 4.0,
              "color": [0.7, 0.85, 1.0], "brightness": 1.0 }
        ]
    }"#;
    let cool_level = LevelDef::from_json(cool_only).expect("cool level parses");
    let cool_lighting = LevelLighting::bake(&cool_level);
    let sample = cool_lighting.sample(4.0, 0.0, 4.0);
    assert!(sample.b > sample.r, "cool light must stay cool: {sample:?}");
    assert!(sample.b >= AMBIENT_LEVEL && sample.b <= MAX_BRIGHTNESS);
    for light in cool_lighting.lights() {
        assert!(light.intensity().is_finite() && light.intensity() >= 0.0);
    }
}

#[test]
fn a_wall_fixture_needs_a_height_and_validation_says_so() {
    // The loader rejects a wall fixture without a `y` (it cannot derive one)
    // and accepts a valid one. A ceiling fixture may also author a world `y`,
    // which mounts its panel at that height (the stacked-building form).
    let level = |lights: &str| {
        format!(
            r#"{{
                "format_version": 3,
                "id": "wall_validation",
                "name": "Wall Validation",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.5 }}],
                "ceiling_lights": [{lights}]
            }}"#
        )
    };
    let missing = LevelDef::from_json(&level(
        r#"{ "fixture": "core:pool_light_wall", "x": 1.0, "z": 0.2, "mount": "wall" }"#,
    ))
    .expect("parses");
    let error =
        crate::engine::loader::validate_level(&missing).expect_err("a wall fixture needs y");
    assert!(error.contains("Wall light 0"), "{error}");

    let valid = LevelDef::from_json(&level(
        r#"{ "fixture": "core:pool_light_wall", "x": 1.0, "z": 0.2, "mount": "wall", "y": 2.1 }"#,
    ))
    .expect("parses");
    crate::engine::loader::validate_level(&valid)
        .expect("a wall fixture with its height validates");

    let ceiling_with_y = LevelDef::from_json(&level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0, "y": 2.4 }"#,
    ))
    .expect("parses");
    crate::engine::loader::validate_level(&ceiling_with_y)
        .expect("an authored y on a ceiling fixture mounts it there, not rejected");
    let lighting = LevelLighting::bake(&ceiling_with_y);
    let panel = lighting.lights().first().expect("the fixture bakes");
    assert!(
        (panel.y() - fixture_profile("core:fluorescent_panel_01").emission_y(2.4)).abs() < 1e-6,
        "the source occupies the diffuser below its authored mounting height: {}",
        panel.y()
    );
    assert!((lighting.fixture_y_for(&ceiling_with_y.ceiling_lights[0]) - 2.4).abs() < 1e-6);
    assert_eq!(panel.room, Some(0), "and it still owns its room");

    let non_finite = LevelDef::from_json(&level(
        r#"{ "fixture": "core:pool_light_wall", "x": 1.0, "z": 0.2, "mount": "wall",
             "y": 1e40 }"#,
    ))
    .expect("parses");
    // 1e40 overflows f32 to infinity, which must be rejected, not blessed.
    drop(
        crate::engine::loader::validate_level(&non_finite)
            .expect_err("non-finite heights are rejected"),
    );
}

// ----------------------------------------------- opening seams and corners

/// Two 4 x 4 m rooms side by side, separated by a 0.4 m wall at x = 4 on which
/// the caller authors the openings. The wall runs along Z, so `offset` is a Z
/// coordinate and the fixture sits flush on its west face.
fn two_rooms_with_wall(openings_json: &str, fixture_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "opening_seam",
            "name": "Opening Seam",
            "spawn": {{ "x": 2.0, "z": 2.0 }},
            "rooms": [
                {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }},
                {{ "x": 4.4, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }}
            ],
            "walls": [
                {{ "x": 4.0, "z": 0.0, "width": 0.4, "depth": 4.0, "height": 3.0,
                   "openings": {openings_json} }}
            ],
            "ceiling_lights": [{fixture_json}]
        }}"#
    );
    LevelDef::from_json(&json).expect("opening seam level parses")
}

#[test]
fn a_window_jamb_does_not_transmit_beside_itself() {
    // The window occupies z = 1.0..2.0, y = 1.0..2.0. The fixture sits flush on
    // the wall's west face at the jamb's own Z, and the sample is three
    // millimetres on the *solid* side of that jamb in the far room: the joining
    // segment crosses the wall through solid material, so it must stay at the
    // unlit room's ambient fill.
    let level = two_rooms_with_wall(
        r#"[{ "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0, "sill": 1.0 }]"#,
        r#"{ "fixture": "core:pool_light_wall", "x": 4.0, "z": 0.997,
             "mount": "wall", "y": 1.5, "rotation_degrees": -90, "brightness": 1.0 }"#,
    );
    let lighting = LevelLighting::bake(&level);
    let ambient = ambient_color();

    // Straight through the solid column beside the window: blocked.
    let beside = lighting.sample_in_room(1, 5.0, 1.5, 0.997);
    assert!(
        (beside.luminance() - ambient.luminance()).abs() < 1e-3,
        "light leaked through the wall beside the window jamb: {beside:?}"
    );

    // Through the window's own aperture: transmitted.
    let through = lighting.sample_in_room(1, 5.0, 1.5, 1.5);
    assert!(
        through.luminance() > ambient.luminance() + 0.05,
        "the window must still transmit: {through:?}"
    );

    // A fixture below the sill, on the window's own span: its light has to
    // cross the solid wall under the sill, so the far room stays ambient.
    let below_level = two_rooms_with_wall(
        r#"[{ "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0, "sill": 1.0 }]"#,
        r#"{ "fixture": "core:pool_light_wall", "x": 4.0, "z": 1.5,
             "mount": "wall", "y": 0.5, "rotation_degrees": -90, "brightness": 1.0 }"#,
    );
    let below_lighting = LevelLighting::bake(&below_level);
    let below = below_lighting.sample_in_room(1, 5.0, 0.5, 1.5);
    assert!(
        (below.luminance() - ambient.luminance()).abs() < 1e-3,
        "light leaked under the window sill: {below:?}"
    );
}

#[test]
fn a_lit_corner_does_not_transmit_diagonally() {
    // Two rooms meeting at the corner of two solid walls, so the only straight
    // line between them is through one of the walls. Approaching the corner
    // diagonally must stay blocked at every offset from the exact corner line.
    let json = r#"{
        "format_version": 3,
        "id": "corner_seam",
        "name": "Corner Seam",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 },
            { "x": 4.4, "z": 4.4, "width": 4.0, "depth": 4.0, "height": 3.0 }
        ],
        "walls": [
            { "x": 4.0, "z": 0.0, "width": 0.4, "depth": 4.4, "height": 3.0 },
            { "x": 0.0, "z": 4.0, "width": 4.4, "depth": 0.4, "height": 3.0 }
        ],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 1.0 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("corner seam level parses");
    let lighting = LevelLighting::bake(&level);
    let ambient = ambient_color();
    for offset in [0.0, 0.002, 0.01, 0.1, 0.5] {
        let sample = lighting.sample_in_room(1, 5.0 + offset, 1.5, 5.0 + offset);
        assert!(
            (sample.luminance() - ambient.luminance()).abs() < 1e-3,
            "light leaked around the corner at offset {offset}: {sample:?}"
        );
    }
    // The same corner rooms stay isolated in colour, not just brightness.
    let sample = lighting.sample_in_room(1, 6.0, 1.5, 6.0);
    assert!(
        (sample.r - ambient.r).abs() < 1e-3 && (sample.b - ambient.b).abs() < 1e-3,
        "the sealed room must keep the ambient colour exactly: {sample:?}"
    );
}

// ------------------------------------------------------- generic light sources

/// One room with one light entry, written verbatim so a test can exercise any
/// authored field of the fixture/light schema.
fn level_with_one_light(light_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "single_light_test",
            "name": "Single Light Test",
            "spawn": {{ "x": 5.0, "z": 5.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
            "ceiling_lights": [{light_json}]
        }}"#
    );
    LevelDef::from_json(&json).expect("test level parses")
}

#[test]
fn a_disabled_fixture_keeps_its_emission_but_casts_no_light() {
    let level = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0,
             "brightness": 1.0, "enabled": false }"#,
    );
    let lighting = LevelLighting::bake(&level);

    // The light exists, keeps its authored values, and is inert.
    let light = lighting.lights().first().expect("the light still bakes");
    assert!(!light.enabled());
    assert!((light.intensity() - 1.0).abs() < 1e-6);
    assert!(!light.is_active());

    // Nothing it could have illuminated changed: the room stays at ambient.
    let ambient = ambient_color();
    for (x, z) in [(5.0, 5.0), (2.0, 2.0), (5.0, 2.0)] {
        let sample = lighting.sample(x, 0.0, z);
        assert!(
            (sample.luminance() - ambient.luminance()).abs() < 1e-6,
            "a disabled fixture lit ({x}, {z}): {sample:?}"
        );
    }
    // It is still owned (the count describes the room's fixtures) and its
    // emissive value is untouched, so the visible face can still glow.
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_eq!(lighting.rooms()[0].effective_power.luminance(), 0.0);
    assert!((level.ceiling_lights[0].emission_intensity() - 1.0).abs() < 1e-6);
}

#[test]
fn a_fixture_can_author_an_independent_emissive_strength() {
    let dim_but_bright = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0,
             "brightness": 0.2, "emission": 1.0 }"#,
    );
    let plain = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0,
             "brightness": 0.2 }"#,
    );
    // The light is identical: the bake cannot see the emissive strength.
    assert_eq!(
        LevelLighting::bake(&dim_but_bright).sample(5.0, 0.0, 5.0),
        LevelLighting::bake(&plain).sample(5.0, 0.0, 5.0)
    );
    // The authored face strength is not.
    let mut bright = dim_but_bright.clone();
    bright.ceiling_lights[0].emission = None;
    assert!((dim_but_bright.ceiling_lights[0].emission_intensity() - 1.0).abs() < 1e-6);
    assert!((bright.ceiling_lights[0].emission_intensity() - 0.2).abs() < 1e-6);
}

#[test]
fn a_fixture_can_author_its_own_pool_shape() {
    let default_range = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.1 }"#,
    );
    let short_range = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.1,
             "range": 3.0 }"#,
    );
    let constant = level_with_one_light(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.1,
             "falloff": "constant" }"#,
    );
    let default = LevelLighting::bake(&default_range);
    let narrow = LevelLighting::bake(&short_range);
    let flat = LevelLighting::bake(&constant);

    // Every pool reaches the floor under the panel (the pool is a 3D distance
    // from the panel plane, so a range that cannot reach the floor at all would
    // light nothing), and a tighter pool is never brighter than the default:
    // the falloff curve is evaluated over the light's own range.
    assert!(narrow.sample(5.0, 0.0, 5.0).r > 0.0);
    assert!(narrow.sample(5.0, 0.0, 5.0).r < default.sample(5.0, 0.0, 5.0).r);
    // Away from the panel the shorter range has already ended while the
    // default pool still contributes.
    assert!(narrow.sample(7.5, 0.0, 5.0).r < default.sample(7.5, 0.0, 5.0).r);
    // A constant curve holds full strength out to its range, so it wins at a
    // distance where the smooth curve has decayed.
    assert!(flat.sample(7.5, 0.0, 5.0).r > default.sample(7.5, 0.0, 5.0).r);
    assert!(flat.sample(7.5, 0.0, 5.0).r <= LOCAL_LIGHT_MAX);

    // The authored range is visible on the baked source, and reflects the
    // documented default when it is not authored.
    assert!((default.lights()[0].range() - LOCAL_LIGHT_RADIUS_M).abs() < 1e-6);
    assert!((narrow.lights()[0].range() - 3.0).abs() < 1e-6);
    assert_eq!(flat.lights()[0].falloff(), LightFalloff::Constant);
}

/// A room whose only illumination is a light attached to a placed prop.
fn level_with_prop_light(prop_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "prop_light_test",
            "name": "Prop Light Test",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "props": [{prop_json}]
        }}"#
    );
    LevelDef::from_json(&json).expect("test level parses")
}

#[test]
fn the_occlusion_fingerprint_tracks_the_solids_the_bake_uses() {
    // The lightmap cache key folds this value in, so it must be deterministic
    // for one level and must change whenever a solid the bake tests against
    // changes: a moved prop, an added prop, a re-aimed wall.
    let level = level_with_prop_light(r#"{ "model": "core:desk", "x": 6.0, "z": 6.0 }"#);
    let first = LevelLighting::bake(&level);
    let again = LevelLighting::bake(&level);
    assert_eq!(first.occlusion_fingerprint(), again.occlusion_fingerprint());

    // Moving the prop moves its derived occluders.
    let mut moved = level.clone();
    if let Some(prop) = moved.props.first_mut() {
        prop.x += 1.0;
    }
    let moved_lighting = LevelLighting::bake(&moved);
    assert_ne!(
        first.occlusion_fingerprint(),
        moved_lighting.occlusion_fingerprint()
    );

    // Removing it removes them.
    let mut removed = level.clone();
    removed.props.clear();
    let removed_lighting = LevelLighting::bake(&removed);
    assert_ne!(
        first.occlusion_fingerprint(),
        removed_lighting.occlusion_fingerprint()
    );

    // Turning it changes the oriented boxes' sin/cos, not just their centre.
    // (The last use of `level`, so it can be consumed rather than cloned.)
    let mut turned = level;
    if let Some(prop) = turned.props.first_mut() {
        prop.rotation_degrees += 45.0;
    }
    let turned_lighting = LevelLighting::bake(&turned);
    assert_ne!(
        first.occlusion_fingerprint(),
        turned_lighting.occlusion_fingerprint()
    );
}

/// The same level twice through the renderer's key path must produce one key,
/// and a prop edit that changes occlusion must produce another.
#[test]
fn the_lightmap_content_key_covers_the_occluder_set() {
    use crate::engine::lighting::lightmap::{LightmapConfig, content_key_with_extra};
    use crate::engine::quality::QualityProfile;

    let config = LightmapConfig::for_profile(QualityProfile::Full);
    let level = level_with_prop_light(r#"{ "model": "core:desk", "x": 6.0, "z": 6.0 }"#);
    let key_of = |source_level: &LevelDef| {
        let lighting = LevelLighting::bake(source_level);
        content_key_with_extra(
            source_level,
            &config,
            QualityProfile::Full,
            &lighting.occlusion_fingerprint().to_le_bytes(),
        )
    };
    assert_eq!(key_of(&level), key_of(&level));

    let mut moved = level.clone();
    if let Some(prop) = moved.props.first_mut() {
        prop.z += 0.75;
    }
    assert_ne!(key_of(&level), key_of(&moved));
}

#[test]
fn a_prop_light_is_placed_by_the_props_own_transform() {
    // A prop at (6, 0, 6) turned a quarter turn and scaled 2x, with a rect
    // light offset one metre along its local +Z (which its yaw sends along +X).
    let level = level_with_prop_light(
        r#"{ "model": "core:desk", "x": 6.0, "z": 6.0, "rotation_degrees": 90.0, "scale": 2.0,
             "lights": [ { "shape": "rect", "half_width": 0.3, "half_depth": 0.1,
                           "offset": [0.0, 1.5, 1.0],
                           "color": [1.0, 0.0, 0.0], "intensity": 1.0 } ] }"#,
    );
    let lighting = LevelLighting::bake(&level);
    let light = lighting.lights().first().expect("the prop light bakes");

    // Yaw 90 degrees sends local +Z to world +X and local +X to world -Z.
    assert!((light.x() - 8.0).abs() < 1e-5, "x was {}", light.x());
    assert!((light.z() - 6.0).abs() < 1e-5, "z was {}", light.z());
    // The offset's height is scaled with the object; the floor is at 0.
    assert!((light.y() - 3.0).abs() < 1e-5, "y was {}", light.y());
    // The emitter's own half-extents scale with the object and then rotate as a
    // rectangle: 2 x (0.3, 0.1) turned 90 degrees is (0.2, 0.6).
    assert!(
        (light.half_w() - 0.2).abs() < 1e-5,
        "half_w {}",
        light.half_w()
    );
    assert!(
        (light.half_d() - 0.6).abs() < 1e-5,
        "half_d {}",
        light.half_d()
    );
    assert!((light.color().r - 1.0).abs() < 1e-6);
    assert_eq!(light.room, Some(0));
}

#[test]
fn a_prop_light_lights_its_room_and_a_disabled_one_does_not() {
    let lit = level_with_prop_light(
        r#"{ "model": "core:desk", "x": 6.0, "z": 6.0,
             "lights": [ { "shape": "point", "offset": [0.0, 1.0, 0.0], "intensity": 2.0 } ] }"#,
    );
    let off = level_with_prop_light(
        r#"{ "model": "core:desk", "x": 6.0, "z": 6.0,
             "lights": [ { "shape": "point", "offset": [0.0, 1.0, 0.0], "intensity": 2.0,
                           "enabled": false } ] }"#,
    );
    let no_lights = level_with_prop_light(r#"{ "model": "core:desk", "x": 6.0, "z": 6.0 }"#);
    let on_lighting = LevelLighting::bake(&lit);
    let off_lighting = LevelLighting::bake(&off);
    let none = LevelLighting::bake(&no_lights);

    // A prop light raises the room baseline and its local pool.
    assert!(on_lighting.rooms()[0].baseline.luminance() > none.rooms()[0].baseline.luminance());
    assert!(on_lighting.sample(6.0, 0.5, 6.0).luminance() > none.sample(6.0, 0.5, 6.0).luminance());
    // Disabled, it changes nothing at all.
    assert!(
        (off_lighting.rooms()[0].baseline.luminance() - none.rooms()[0].baseline.luminance()).abs()
            < 1e-6
    );
    assert!(
        (off_lighting.sample(6.0, 0.5, 6.0).luminance() - none.sample(6.0, 0.5, 6.0).luminance())
            .abs()
            < 1e-6
    );
    assert!(!off_lighting.lights()[0].enabled());
    assert_eq!(none.lights().len(), 0);
}

#[test]
fn malformed_prop_lights_are_rejected_or_skipped_without_panicking() {
    // A light with a finite offset but a malformed shape is a level error.
    let bad_shape = level_with_prop_light(
        r#"{ "model": "core:desk", "x": 6.0, "z": 6.0,
             "lights": [ { "shape": "rect", "half_width": 0.0, "half_depth": 0.1 } ] }"#,
    );
    assert!(crate::engine::loader::validate_level(&bad_shape).is_err());

    // A non-finite offset is a level error too.
    let bad_offset = LevelDef::from_json(
        r#"{ "format_version": 3, "id": "bad_offset", "name": "Bad Offset",
             "spawn": { "x": 0.0, "z": 0.0 },
             "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }],
             "props": [{ "model": "core:desk", "x": 1.0, "z": 1.0,
                         "lights": [{ "shape": "point" }] }] }"#,
    )
    .expect("parses");
    assert!(crate::engine::loader::validate_level(&bad_offset).is_ok());
    let mut hand_edited = bad_offset;
    hand_edited.props[0].lights[0].offset = [f32::NAN, 0.0, 0.0];
    assert!(crate::engine::loader::validate_level(&hand_edited).is_err());

    // The bake itself stays finite for hand-edited data that skipped the
    // loader: a non-finite offset is dropped, not propagated.
    let baked = LevelLighting::bake(&hand_edited);
    assert_eq!(baked.lights().len(), 0);
    assert!(baked.sample(1.0, 0.5, 1.0).luminance().is_finite());
}

// ===========================================================================
// Prop occlusion
// ===========================================================================

/// One rectangular room with the given `ceiling_lights` and `props` entries.
fn prop_scene(width: f32, depth: f32, lights: &[String], props: &[String]) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "prop_occlusion",
            "name": "Prop Occlusion",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": 3.0 }}],
            "ceiling_lights": [{}],
            "props": [{}]
        }}"#,
        lights.join(","),
        props.join(",")
    );
    LevelDef::from_json(&json).expect("prop scene parses")
}

/// One office-panel fixture entry.
fn fixture_at(x: f32, z: f32, intensity: f32) -> String {
    format!(
        r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "brightness": {intensity} }}"#
    )
}

/// One placed prop entry with no lights.
fn prop_at(model: &str, x: f32, z: f32) -> String {
    format!(r#"{{ "model": "{model}", "x": {x}, "z": {z} }}"#)
}

/// Bakes a scene and returns it plus the sample grid the occlusion tests use.
fn bake_scene(width: f32, depth: f32, lights: &[String], props: &[String]) -> LevelLighting {
    LevelLighting::bake(&prop_scene(width, depth, lights, props))
}

#[test]
fn lightmap_texels_match_the_vertex_path_exactly() {
    // A wall plus two props make every query path interesting: pools, prop
    // bodies and the wall solid all participate. The grid stays clear of the
    // wall's own boxes, which is the one condition the fast path is defined
    // for (a texel centre is generated on a surface, never inside a wall).
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "prop_occlusion",
            "name": "Prop Occlusion",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "walls": [{{ "x": 5.9, "z": 0.0, "width": 0.2, "depth": 12.0, "height": 3.0 }}],
            "ceiling_lights": [{}, {}],
            "props": [{}, {}]
        }}"#,
        fixture_at(3.0, 3.0, 0.5),
        fixture_at(9.0, 5.0, 0.3),
        prop_at("core:washing_machine", 3.0, 6.0),
        prop_at("core:desk", 9.0, 9.0),
    );
    let level = LevelDef::from_json(&json).expect("scene parses");
    let lighting = LevelLighting::bake(&level);
    let mut checked = 0usize;
    for ix in 0_i32..13_i32 {
        let x = 0.25 + crate::test_support::exact_f32(ix) * 0.95;
        if (x - 6.0).abs() < 0.5 {
            continue;
        }
        for iz in 0_i32..13_i32 {
            let z = 0.25 + crate::test_support::exact_f32(iz) * 0.95;
            for y in [0.0_f32, 0.6, 1.5, 2.5] {
                let fast = lighting.lightmap_texel(Some(0), x, y, z);
                let slow = lighting.sample_in_room(0, x, y, z);
                assert_eq!(fast, slow, "texel ({x}, {y}, {z})");
                checked += 1;
            }
        }
    }
    assert!(checked > 500, "the grid must actually cover the room");
}

#[test]
fn lightmap_texels_evaluate_point_rect_and_line_sources_like_the_vertex_path() {
    let shapes = [
        ("point", r#""shape": "point""#),
        (
            "rect",
            r#""shape": "rect", "half_width": 0.4, "half_depth": 0.2"#,
        ),
        ("line", r#""shape": "line", "length": 2.0"#),
    ];
    for (label, shape) in shapes {
        let prop = format!(
            r#"{{ "model": "core:desk", "x": 6.0, "z": 6.0,
                 "lights": [ {{ {shape}, "offset": [0.0, 1.0, 0.0], "intensity": 1.0,
                                "color": [0.8, 0.4, 0.2], "range": 5.0 }} ] }}"#
        );
        let lighting = bake_scene(12.0, 12.0, &[], &[prop]);
        assert_eq!(lighting.lights().len(), 1, "{label}: one prop light");
        for x in [4.0_f32, 5.0, 6.0, 7.5] {
            for y in [0.5_f32, 1.2, 2.0] {
                for z in [4.5_f32, 6.0, 7.5] {
                    assert_eq!(
                        lighting.lightmap_texel(Some(0), x, y, z),
                        lighting.sample_in_room(0, x, y, z),
                        "{label} texel ({x}, {y}, {z})"
                    );
                }
            }
        }
    }
}

#[test]
fn lightmap_texels_respect_range_falloff_colour_and_enabled() {
    let with_light = |light: &str| {
        let prop = format!(
            r#"{{ "model": "core:desk", "x": 6.0, "z": 6.0, "lights": [ {{ {light} }} ] }}"#
        );
        bake_scene(12.0, 12.0, &[], &[prop])
    };
    let base = r#""shape": "point", "offset": [0.0, 1.0, 0.0], "intensity": 1.0"#;
    let smooth = with_light(&format!(r#"{base}, "range": 4.0"#));
    let constant = with_light(&format!(r#"{base}, "range": 4.0, "falloff": "constant""#));
    let short = with_light(&format!(r#"{base}, "range": 1.5"#));
    let off = with_light(&format!(r#"{base}, "enabled": false"#));
    let none = bake_scene(12.0, 12.0, &[], &[prop_at("core:desk", 6.0, 6.0)]);

    // The pool honours the range: a shorter range ends earlier and is never
    // brighter inside its own reach.
    let near = (6.0_f32, 1.0, 5.0);
    assert!(smooth.lightmap_texel(Some(0), near.0, near.1, near.2).r > 0.0);
    assert!(
        short.lightmap_texel(Some(0), 6.0, 0.0, 3.5).r
            < smooth.lightmap_texel(Some(0), 6.0, 0.0, 3.5).r
    );
    // A constant falloff holds its strength where the smooth curve has decayed.
    assert!(
        constant.lightmap_texel(Some(0), 6.0, 0.0, 3.5).r
            > smooth.lightmap_texel(Some(0), 6.0, 0.0, 3.5).r
    );
    // Colour is carried per channel, not as a luminance wash: a green-only
    // light leaves red and blue on the ambient floor.
    let coloured = with_light(&format!(r#"{base}, "color": [0.0, 1.0, 0.0]"#));
    let sample = coloured.lightmap_texel(Some(0), 6.0, 1.0, 5.0);
    assert!(sample.g > sample.r);
    assert_eq!(sample.r, AMBIENT_LEVEL);
    assert_eq!(sample.b, AMBIENT_LEVEL);

    // `enabled: false` is not a dim light: it is exactly the unlit scene.
    for point in [(6.0_f32, 1.0, 5.0), (7.0, 0.0, 6.0), (4.0, 2.0, 4.0)] {
        assert_eq!(
            off.lightmap_texel(Some(0), point.0, point.1, point.2),
            none.lightmap_texel(Some(0), point.0, point.1, point.2)
        );
    }
    assert_eq!(off.rooms()[0].baseline, none.rooms()[0].baseline);
}

#[test]
fn a_static_prop_blocks_light_and_leaves_the_covered_side_darker() {
    // A tall vending machine between the fixture and the floor sample: the
    // sample is inside the pool's range but on the far side of the body.
    let with_prop = bake_scene(
        14.0,
        10.0,
        &[fixture_at(4.0, 5.0, 0.6)],
        &[prop_at("core:vending_machine", 7.0, 5.0)],
    );
    let without = bake_scene(14.0, 10.0, &[fixture_at(4.0, 5.0, 0.6)], &[]);
    let blocked = with_prop.sample_in_room(0, 8.0, 0.0, 5.0).luminance();
    let open = without.sample_in_room(0, 8.0, 0.0, 5.0).luminance();
    assert!(
        open > blocked,
        "the vending machine must shadow the sample: {open} vs {blocked}"
    );
    assert!(
        open > without.rooms()[0].baseline.luminance() + 1e-4,
        "without the prop the pool must actually reach the sample: {open}"
    );
}

#[test]
fn a_floor_texel_under_a_prop_is_darker_than_open_floor_beside_it() {
    // The fixture sits directly between the two samples, one metre either way:
    // the under-prop texel is blocked by the machine body, the open texel is
    // at exactly the same distance from the fixture.
    let lighting = bake_scene(
        12.0,
        12.0,
        &[fixture_at(5.5, 5.0, 0.6)],
        &[prop_at("core:washing_machine", 4.5, 5.0)],
    );
    let without = bake_scene(12.0, 12.0, &[fixture_at(5.5, 5.0, 0.6)], &[]);
    let under = lighting.sample_in_room(0, 4.5, 0.0, 5.0);
    let open = lighting.sample_in_room(0, 6.5, 0.0, 5.0);
    let open_without = without.sample_in_room(0, 6.5, 0.0, 5.0);
    assert!(
        under.luminance() < open.luminance(),
        "contact darkening must shade the prop's own footprint: {} vs {}",
        under.luminance(),
        open.luminance()
    );
    assert!(
        (open.luminance() - open_without.luminance()).abs() < 1e-6,
        "the open side is not shadowed by the prop"
    );
}

#[test]
fn a_rotated_prop_occludes_its_rotated_footprint_not_its_bounding_box() {
    // The desk is 1.6 x 0.7 m. At 90 degrees its footprint is 0.7 x 1.6, so
    // the sample at x = 5.65 is under the upright desk top but clear of the
    // rotated one. Using the axis-aligned bounds of the rotation would block
    // both.
    let upright = bake_scene(
        12.0,
        12.0,
        &[fixture_at(5.65, 5.0, 0.6)],
        &[prop_at("core:desk", 5.0, 5.0)],
    );
    let rotated = bake_scene(
        12.0,
        12.0,
        &[fixture_at(5.65, 5.0, 0.6)],
        &[r#"{ "model": "core:desk", "x": 5.0, "z": 5.0, "rotation_degrees": 90.0 }"#.to_string()],
    );
    let blocked = upright.sample_in_room(0, 5.65, 0.0, 5.0);
    let lit = rotated.sample_in_room(0, 5.65, 0.0, 5.0);
    assert!(
        lit.luminance() > blocked.luminance(),
        "the rotated footprint must clear the sample: {} vs {}",
        lit.luminance(),
        blocked.luminance()
    );
    assert!(
        (blocked.luminance() - upright.rooms()[0].baseline.luminance()).abs() < 1e-6,
        "the upright desk top blocks the pool completely: {blocked:?}"
    );
}

#[test]
fn a_scaled_prop_occludes_proportionally() {
    // Scale 2 grows the 1.6 m desk to 3.2 m: x = 6.4 is under the scaled top
    // and outside the unscaled one, with the fixture directly above.
    let plain = bake_scene(
        14.0,
        12.0,
        &[fixture_at(6.4, 5.0, 0.6)],
        &[prop_at("core:desk", 5.0, 5.0)],
    );
    let scaled = bake_scene(
        14.0,
        12.0,
        &[fixture_at(6.4, 5.0, 0.6)],
        &[r#"{ "model": "core:desk", "x": 5.0, "z": 5.0, "scale": 2.0 }"#.to_string()],
    );
    let clear = plain.sample_in_room(0, 6.4, 0.0, 5.0);
    let blocked = scaled.sample_in_room(0, 6.4, 0.0, 5.0);
    assert!(
        clear.luminance() > blocked.luminance(),
        "the scaled desk must reach x = 6.4: {} vs {}",
        clear.luminance(),
        blocked.luminance()
    );
}

#[test]
fn multiple_props_stack_their_shadows() {
    // Two fixtures light the sample from opposite sides; one vending machine
    // blocks each fixture's line. Removing a machine lets that side's pool
    // through, so brightness falls in the order none > one > both.
    let lights = [fixture_at(1.5, 6.0, 0.6), fixture_at(10.5, 6.0, 0.6)];
    let both = bake_scene(
        12.0,
        12.0,
        &lights,
        &[
            prop_at("core:vending_machine", 4.0, 6.0),
            prop_at("core:vending_machine", 8.0, 6.0),
        ],
    );
    let one = bake_scene(
        12.0,
        12.0,
        &lights,
        &[prop_at("core:vending_machine", 4.0, 6.0)],
    );
    let none = bake_scene(12.0, 12.0, &lights, &[]);
    let sample = |lighting: &LevelLighting| lighting.sample_in_room(0, 6.0, 0.0, 6.0).luminance();
    assert!(
        sample(&none) > sample(&one),
        "removing one prop must let one pool through: {} vs {}",
        sample(&none),
        sample(&one)
    );
    assert!(
        sample(&one) > sample(&both),
        "with both props neither pool reaches the sample: {} vs {}",
        sample(&one),
        sample(&both)
    );
}

#[test]
fn a_prop_over_a_raised_floor_region_occludes_from_its_own_base() {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "raised_prop",
            "name": "Raised Prop",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "floor_regions": [
                {{ "x": 2.0, "z": 2.0, "width": 8.0, "depth": 8.0, "offset_y": 0.8 }}
            ],
            "ceiling_lights": [{}],
            "props": [{}]
        }}"#,
        fixture_at(6.0, 6.0, 0.6),
        prop_at("core:washing_machine", 6.0, 6.0),
    );
    let with_prop = LevelLighting::bake(&LevelDef::from_json(&json).expect("scene parses"));
    let json_without = format!(
        r#"{{
            "format_version": 3,
            "id": "raised_prop",
            "name": "Raised Prop",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "floor_regions": [
                {{ "x": 2.0, "z": 2.0, "width": 8.0, "depth": 8.0, "offset_y": 0.8 }}
            ],
            "ceiling_lights": [{}]
        }}"#,
        fixture_at(6.0, 6.0, 0.6),
    );
    let without = LevelLighting::bake(&LevelDef::from_json(&json_without).expect("scene parses"));

    // The prop stands on the platform (base 0.8): a platform sample under its
    // footprint is shaded, while the low floor outside the region is not
    // touched at all.
    let on_platform = with_prop.sample_in_room(0, 6.0, 0.8, 6.0);
    let platform_open = without.sample_in_room(0, 6.0, 0.8, 6.0);
    let low_with = with_prop.sample_in_room(0, 1.5, 0.0, 6.0);
    let low_without = without.sample_in_room(0, 1.5, 0.0, 6.0);
    assert!(
        on_platform.luminance() < platform_open.luminance(),
        "the prop must darken the platform it stands on: {} vs {}",
        on_platform.luminance(),
        platform_open.luminance()
    );
    assert!(
        (low_with.luminance() - low_without.luminance()).abs() < 1e-6,
        "the raised prop must not shadow the low floor: {} vs {}",
        low_with.luminance(),
        low_without.luminance()
    );
}

#[test]
fn a_prop_against_a_wall_still_occludes() {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "prop_wall",
            "name": "Prop Wall",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "walls": [{{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 12.0, "height": 3.0 }}],
            "ceiling_lights": [{}],
            "props": [{}]
        }}"#,
        fixture_at(1.5, 9.7, 0.6),
        prop_at("core:vending_machine", 4.0, 9.7),
    );
    let with_prop = LevelLighting::bake(&LevelDef::from_json(&json).expect("scene parses"));
    let json_without = format!(
        r#"{{
            "format_version": 3,
            "id": "prop_wall",
            "name": "Prop Wall",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }}],
            "walls": [{{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 12.0, "height": 3.0 }}],
            "ceiling_lights": [{}]
        }}"#,
        fixture_at(1.5, 9.7, 0.6),
    );
    let without = LevelLighting::bake(&LevelDef::from_json(&json_without).expect("scene parses"));

    // The machine stands just off the wall the light grazes along; it still
    // shadows the floor beyond it.
    let blocked = with_prop.sample_in_room(0, 5.5, 0.0, 9.7).luminance();
    let open = without.sample_in_room(0, 5.5, 0.0, 9.7).luminance();
    assert!(
        open > blocked,
        "the wall-side prop must still block: {open} vs {blocked}"
    );
}

#[test]
fn a_props_own_light_lights_around_its_body() {
    // A vending machine with a front-mounted rect light: the emitter is the
    // source, so the machine's own housing must not occlude it — exactly as a
    // ceiling fixture is never occluded by its own housing. The floor in front
    // of the panel and the floor behind the machine are both inside the pool;
    // only *other* geometry may cast a shadow.
    let with_light = r#"{ "model": "core:vending_machine", "x": 5.0, "z": 5.0,
                "lights": [ { "shape": "rect", "half_width": 0.4, "half_depth": 0.05,
                               "offset": [0.0, 1.2, 0.5], "intensity": 1.0, "range": 5.0,
                               "color": [0.6, 0.8, 1.0] } ] }"#
        .to_string();
    let lighting = bake_scene(12.0, 12.0, &[], std::slice::from_ref(&with_light));
    assert_eq!(lighting.lights().len(), 1, "one prop light bakes");
    let front = lighting.sample_in_room(0, 5.0, 0.6, 7.0);
    let back = lighting.sample_in_room(0, 5.0, 0.6, 3.0);
    assert!(
        front.luminance() > lighting.rooms()[0].baseline.luminance() + 1e-4,
        "the pool must reach the floor in front of the panel"
    );
    // The machine's own boxes must not stand between the emitter and the floor
    // behind it: the pool term there is fully visible, not a body shadow.
    let behind_terms = lighting.pool_terms_in_room(Some(0), 5.0, 0.6, 3.0);
    assert!(
        behind_terms.iter().any(|term| term.visibility > 0.9),
        "the prop's own light must not be blocked by its own body: {behind_terms:?}"
    );
    let unlit = bake_scene(
        12.0,
        12.0,
        &[],
        &[prop_at("core:vending_machine", 5.0, 5.0)],
    );
    let back_open = unlit.sample_in_room(0, 5.0, 0.6, 3.0);
    assert!(
        back.luminance() > back_open.luminance() + 0.02,
        "the pool must actually brighten the floor behind the machine: {} vs {}",
        back.luminance(),
        back_open.luminance()
    );
}

/// A light attached to one prop still treats every *other* prop body as an
/// occluder: only the owning prop's own boxes are exempt.
#[test]
fn a_light_attached_to_another_prop_still_sees_this_prop_as_an_occluder() {
    // The emitter belongs to the desk at (4, 6); the vending machine at (6, 6)
    // stands between it and the floor sample at (8, 6).
    let emitter = r#"{ "model": "core:desk", "x": 4.0, "z": 6.0,
            "lights": [ { "shape": "point", "offset": [0.0, 1.2, 0.0],
                           "intensity": 1.0, "range": 6.0 } ] }"#
        .to_string();
    let with_body = bake_scene(
        12.0,
        12.0,
        &[],
        &[emitter.clone(), prop_at("core:vending_machine", 6.0, 6.0)],
    );
    let without_body = bake_scene(12.0, 12.0, &[], &[emitter]);
    let blocked = with_body.sample_in_room(0, 8.0, 0.0, 6.0);
    let open = without_body.sample_in_room(0, 8.0, 0.0, 6.0);
    assert!(
        open.luminance() > blocked.luminance() + 0.02,
        "a different prop must still occlude the attached light: {open:?} vs {blocked:?}"
    );
}

/// A hanging globe's own light reaches the ceiling above it.
///
/// The emitter sits just under the orb, inside the lamp's own occlusion volume;
/// with the self-exemption the own housing no longer blocks the upward rays, so
/// the ceiling above the lamp gains light compared with the same room without
/// the authored light. The room's ceiling slab is never exempt: an emitter
/// placed above the ceiling plane still lights nothing below it.
#[test]
fn a_props_own_light_reaches_the_ceiling_above_it() {
    let lamp = |y: f32| {
        format!(
            r#"{{ "model": "home:ball_light", "x": 6.0, "z": 6.0, "y": {y},
                 "lights": [ {{ "shape": "point", "offset": [0.0, -0.02, 0.0],
                                "intensity": 1.0, "range": 6.0 }} ] }}"#
        )
    };
    // The orb hangs 1.4 m up, its emitter just under it, well below the 3 m
    // ceiling.
    let lit = bake_scene(12.0, 12.0, &[], &[lamp(1.4)]);
    let unlit = bake_scene(12.0, 12.0, &[], &[prop_at("home:ball_light", 6.0, 6.0)]);
    let above = lit.sample_in_room(0, 6.0, 3.0, 6.0);
    let above_dark = unlit.sample_in_room(0, 6.0, 3.0, 6.0);
    assert!(
        above.luminance() > above_dark.luminance() + 0.02,
        "the lamp must raise the ceiling luminance above it: {} vs {}",
        above.luminance(),
        above_dark.luminance()
    );
    // The ray to the ceiling is genuinely unoccluded, not merely brighter from
    // the room baseline: the light's pool term is fully visible there.
    let terms = lit.pool_terms_in_room(Some(0), 6.0, 3.0, 6.0);
    assert!(
        terms.iter().any(|term| term.visibility > 0.9),
        "the lamp's own orb must not block the pool from reaching the ceiling: {terms:?}"
    );

    // A lamp above the ceiling plane still lights nothing below it: the
    // ceiling slab is not part of the self-exemption.
    let above_ceiling = bake_scene(12.0, 12.0, &[], &[lamp(3.6)]);
    let floor_terms = above_ceiling.pool_terms_in_room(Some(0), 6.0, 0.05, 6.0);
    assert!(
        floor_terms.iter().all(|term| term.visibility <= 1e-4),
        "a light above the ceiling plane must stay blocked: {floor_terms:?}"
    );
}

#[test]
fn prop_emission_is_never_illumination() {
    // The vending machine's GLB carries a bright emissive panel. With no
    // author lights it must contribute exactly nothing: the room baseline is
    // the ambient floor and every sample equals the same scene without props.
    let with_prop = bake_scene(
        12.0,
        12.0,
        &[],
        &[prop_at("core:vending_machine", 5.0, 5.0)],
    );
    let without = bake_scene(12.0, 12.0, &[], &[]);
    assert!(with_prop.lights().is_empty(), "emission is not a light");
    assert_eq!(with_prop.rooms()[0].baseline, ambient_color());
    for point in [(5.0_f32, 1.0, 5.5), (4.0, 0.0, 4.0), (6.0, 2.0, 6.0)] {
        assert_eq!(
            with_prop.lightmap_texel(Some(0), point.0, point.1, point.2),
            without.lightmap_texel(Some(0), point.0, point.1, point.2)
        );
    }

    // A bright emissive panel with `enabled: false` casts nothing either;
    // the authored weak light is the only illumination the scene gains.
    let off = bake_scene(
        12.0,
        12.0,
        &[],
        &[r#"{ "model": "core:vending_machine", "x": 5.0, "z": 5.0,
                "lights": [ { "shape": "rect", "half_width": 0.4, "half_depth": 0.05,
                               "offset": [0.0, 1.2, 0.5], "intensity": 1.0,
                               "enabled": false } ] }"#
            .to_string()],
    );
    assert_eq!(off.rooms()[0].baseline, ambient_color());
    assert_eq!(
        off.sample_in_room(0, 5.0, 0.6, 7.0),
        without.sample_in_room(0, 5.0, 0.6, 7.0)
    );
    assert_eq!(off.lights().len(), 1);
    assert!(!off.lights()[0].enabled());
    let none = bake_scene(
        12.0,
        12.0,
        &[],
        &[prop_at("core:vending_machine", 5.0, 5.0)],
    );

    // A weak prop light must cast exactly the power it authors: the same
    // intensity and colour on a ceiling fixture produces the same baseline.
    let weak_prop = bake_scene(
        12.0,
        12.0,
        &[],
        &[r#"{ "model": "core:vending_machine", "x": 5.0, "z": 5.0,
                "lights": [ { "shape": "point", "offset": [0.0, 1.2, 0.0],
                               "intensity": 0.2, "color": [1.0, 0.0, 0.0] } ] }"#
            .to_string()],
    );
    let weak_fixture = bake_scene(
        12.0,
        12.0,
        &[r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.2, "color": [1.0, 0.0, 0.0] }"#.to_string()],
        &[],
    );
    assert_eq!(
        weak_prop.rooms()[0].baseline,
        weak_fixture.rooms()[0].baseline
    );
    assert_eq!(
        weak_prop.rooms()[0].effective_power,
        weak_fixture.rooms()[0].effective_power
    );
    assert!(weak_prop.rooms()[0].baseline.r > none.rooms()[0].baseline.r);
}

#[test]
fn prop_occluders_do_not_split_a_room_into_baseline_zones() {
    // Occlusion must not answer the partition query: a couch in the middle of
    // a room is furniture, not a wall, so the room keeps one baseline.
    let lighting = bake_scene(
        10.0,
        10.0,
        &[fixture_at(2.5, 5.0, 0.5), fixture_at(7.5, 5.0, 0.5)],
        &[prop_at("core:couch", 5.0, 5.0)],
    );
    assert_eq!(lighting.zone_count(), 1);
    assert!(!lighting.is_partitioned(0));
    assert!(
        lighting.summary().props > 0,
        "the couch must contribute occluders"
    );
}

#[test]
fn prop_occluders_do_not_create_a_dark_hole_outside_their_footprint() {
    // Fixture directly above x = 5.5; the machine footprint ends at x = 4.8.
    // A floor point just outside it is not merely "less dark" — it is exactly
    // the unoccluded value.
    let with_prop = bake_scene(
        12.0,
        12.0,
        &[fixture_at(5.5, 5.0, 0.6)],
        &[prop_at("core:washing_machine", 4.5, 5.0)],
    );
    let without = bake_scene(12.0, 12.0, &[fixture_at(5.5, 5.0, 0.6)], &[]);
    let outside_with = with_prop.sample_in_room(0, 4.95, 0.0, 5.0);
    let outside_without = without.sample_in_room(0, 4.95, 0.0, 5.0);
    assert!(
        (outside_with.luminance() - outside_without.luminance()).abs() < 1e-6,
        "no shadow outside the footprint: {} vs {}",
        outside_with.luminance(),
        outside_without.luminance()
    );
    // And the sample under the footprint is still visibly darker, so the
    // comparison above is not passing because nothing occludes at all.
    let under = with_prop.sample_in_room(0, 4.5, 0.0, 5.0);
    let under_without = without.sample_in_room(0, 4.5, 0.0, 5.0);
    assert!(under.luminance() < under_without.luminance());
}

#[test]
fn lightmap_texel_with_no_room_resolves_by_containment() {
    let lighting = bake_scene(8.0, 8.0, &[fixture_at(4.0, 4.0, 0.5)], &[]);
    // Inside a room, the None hint must still resolve the room (via `sample`
    // rather than `sample_in_room`), and outside every room it returns the
    // local-light + ambient fill.
    let inside = lighting.lightmap_texel(None, 4.0, 0.0, 4.0);
    assert_eq!(inside, lighting.sample(4.0, 0.0, 4.0));
    assert_eq!(inside, lighting.sample_in_room(0, 4.0, 0.0, 4.0));
    let outside = lighting.lightmap_texel(None, 20.0, 0.0, 20.0);
    assert_eq!(outside, lighting.sample(20.0, 0.0, 20.0));
    assert!(outside.luminance() <= AMBIENT_LEVEL + f32::EPSILON);
}

// ------------------------------------------------ lighting decomposition

/// Guards that the developer decomposition is the production model.
#[test]
fn bake_terms_reproduce_the_sample_paths() {
    let level = two_room_level(true);
    let lighting = LevelLighting::bake(&level);
    for room in 0..lighting.rooms().len() {
        let info = lighting.rooms()[room];
        for x in scan_below(info.x0 + 0.25, 1.0, info.x1) {
            for z in scan_below(info.z0 + 0.25, 1.0, info.z1) {
                for y in [info.floor_y, info.floor_y + 0.8, info.floor_y + 1.6] {
                    let terms = lighting.bake_terms_in_room(room, x, y, z);
                    assert_eq!(
                        terms.value,
                        lighting.sample_in_room(room, x, y, z),
                        "terms must reproduce sample_in_room at ({room}, {x}, {y}, {z})"
                    );
                    let global = lighting.bake_terms(x, y, z);
                    assert_eq!(
                        global.value,
                        lighting.sample(x, y, z),
                        "terms must reproduce sample at ({x}, {y}, {z})"
                    );
                    assert!(terms.value.is_valid());
                }
            }
        }
    }
    // Outside every room the decomposition is the ambient fill plus pools.
    let outside = lighting.bake_terms(200.0, 0.0, 200.0);
    assert_eq!(outside.room, None);
    assert_eq!(outside.baseline, ambient_color());
    assert_eq!(outside.blend, LightColor::BLACK);
    assert_eq!(outside.value, lighting.sample(200.0, 0.0, 200.0));
}

/// Guards that the per-fixture pool terms sum to the production pool term.
#[test]
fn pool_terms_reproduce_the_pool_cap() {
    let level = two_room_level(true);
    let lighting = LevelLighting::bake(&level);
    for (x, y, z) in [
        (2.0, 0.0, 2.0),
        (5.0, 0.0, 5.0),
        (8.0, 0.0, 8.0),
        (9.9, 0.0, 5.0),
        (10.5, 0.0, 5.0),
        (30.0, 0.0, 2.0),
        (2.0, 2.5, 2.0),
        (30.0, 2.5, 2.0),
    ] {
        let terms = lighting.pool_terms_in_room(Some(0), x, y, z);
        let expected = screen_terms_for_test(&terms);
        let produced = lighting.bake_terms_in_room(0, x, y, z);
        assert_eq!(
            produced.pool, expected.0,
            "direct terms must screen to the production direct pool at ({x}, {y}, {z})"
        );
        assert_eq!(
            produced.fill, expected.1,
            "fill terms must screen to the production fill at ({x}, {y}, {z})"
        );
    }
}

/// The screen composition, re-implemented from the public terms the way the
/// production path composes them, so the guard above compares two independent
/// expressions of the same equation.
fn screen_terms_for_test(terms: &[PoolTerm]) -> (LightColor, LightColor) {
    let mut direct = [0.0_f32; 3];
    let mut fill = [0.0_f32; 3];
    for term in terms {
        let direct_factor = LOCAL_LIGHT_STRENGTH
            * term.intensity
            * term.height_factor
            * term.direct_shape
            * term.visibility;
        let fill_factor =
            FILL_STRENGTH * term.intensity * term.height_factor * term.fill_shape * term.visibility;
        for (pool, factor, cap) in [
            (&mut direct, direct_factor, LOCAL_LIGHT_MAX),
            (&mut fill, fill_factor, FILL_MAX),
        ] {
            if !factor.is_finite() || factor <= 0.0 {
                continue;
            }
            let peak = term.color.max_channel();
            if !peak.is_finite() || peak <= 0.0 {
                continue;
            }
            for (channel, value) in [term.color.r, term.color.g, term.color.b]
                .into_iter()
                .enumerate()
            {
                let channel_cap = cap * value / peak;
                if !channel_cap.is_finite() || channel_cap <= 0.0 {
                    continue;
                }
                let Some(slot) = pool.get_mut(channel) else {
                    continue;
                };
                let claim = (factor * value).min(channel_cap) / channel_cap;
                *slot = ((channel_cap - *slot).max(0.0)).mul_add(claim, *slot);
            }
        }
    }
    (
        LightColor::rgb(direct[0], direct[1], direct[2]),
        LightColor::rgb(fill[0], fill[1], fill[2]),
    )
}

/// Guards against a zone seam: a 1.4 m corridor between two
/// full-depth partition walls is narrower than a light-grid cell, so with the
/// coarse grid both neighbouring cell centres landed inside the walls and the
/// corridor's samples were split between unrelated zones (measured 0.322 vs
/// 0.295, an 8% step across open floor). The finer, air-aware zone grid must
/// keep the corridor one area.
#[test]
fn a_narrow_corridor_keeps_one_baseline_across_the_zone_grid() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "narrow_corridor",
            "name": "Narrow Corridor",
            "spawn": { "x": 1.0, "z": 2.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 4.0, "height": 3.0 }],
            "walls": [
                { "x": 4.0, "z": 0.0, "width": 0.3, "depth": 4.0, "height": 3.0 },
                { "x": 5.7, "z": 0.0, "width": 0.3, "depth": 4.0, "height": 3.0 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 0.6 },
                { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 2.0, "brightness": 0.5 },
                { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0, "brightness": 0.6 }
            ]
        }"#,
    )
    .expect("narrow corridor parses");
    let lighting = LevelLighting::bake(&level);
    assert!(
        lighting.is_partitioned(0),
        "two full-depth walls must split the room"
    );
    assert_eq!(lighting.zone_count(), 3, "left, corridor and right areas");
    // Two samples 25 cm apart inside the corridor share one baseline; the
    // historical coarse grid split them across its cell cut line.
    let before = lighting.baseline_in_room(0, 4.75, 2.0);
    let after = lighting.baseline_in_room(0, 5.0, 2.0);
    assert_eq!(
        before, after,
        "the corridor must not step at the zone grid cut line"
    );
    // The corridor owns its own fixture and the side rooms keep theirs: the
    // corridor's baseline must differ from at least one side (it is its own
    // area), and all three are inside the fill span.
    let left = lighting.baseline_in_room(0, 2.0, 2.0);
    assert!(before != left, "the corridor is its own area");
    for value in [before, after, left] {
        assert!(
            value.luminance() >= AMBIENT_LEVEL && value.luminance() <= super::tuning::BASELINE_MAX
        );
    }
}

/// Guards the developer zone view against a uniform and a partitioned room.
#[test]
fn probe_visibility_checks_short_walls_inside_one_air_region() {
    let definition = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"probe_short_partition","name":"Probe partition",
        "spawn":{"x":1.0,"z":1.0},
        "rooms":[{"x":0.0,"z":0.0,"width":6.0,"depth":6.0,"height":3.0}],
        "walls":[{"x":2.9,"z":1.0,"width":0.2,"depth":4.0,"height":1.8}]
    }"#,
    )
    .expect("partition parses");
    let lighting = LevelLighting::bake(&definition);
    let left = [2.0, 1.0, 3.0];
    let right = [4.0, 1.0, 3.0];
    assert!(
        lighting.same_probe_region(0, left, right),
        "air connects over the short wall"
    );
    assert!(
        !lighting.probe_visible_from(left, right),
        "connected air does not allow a probe ray through an opaque wall"
    );
    assert!(
        lighting.probe_visible_from([2.0, 2.0, 3.0], [4.0, 2.0, 3.0]),
        "clear rays over the wall remain valid"
    );
}

/// Guards the developer zone view against a uniform and a partitioned room.
#[test]
fn zones_in_room_describes_uniform_and_partitioned_rooms() {
    let uniform = LevelLighting::bake(&level_with_room(10.0, 10.0, 3.0, &[0.5]));
    let zones = uniform.zones_in_room(0);
    assert_eq!(zones.len(), 1, "an open room is one area");
    assert!((zones[0].area_m2 - 100.0).abs() < 1e-3);
    assert_eq!(zones[0].fixture_count, 1);
    assert_eq!(zones[0].baseline, uniform.rooms()[0].baseline);
    assert!(
        uniform.zones_in_room(7).is_empty(),
        "uniform.zones_in_room(7) must be empty"
    );

    // One 10 x 10 room split by a full-height wall at x = 4.9, lit only on
    // the west side: the two areas must report their own areas, powers and
    // baselines, and their areas must tile the room.
    let partitioned = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "zone_report",
            "name": "Zone Report",
            "spawn": { "x": 2.0, "z": 5.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
            "walls": [{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.0 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 5.0 }]
        }"#,
    )
    .expect("partition level parses");
    let lighting = LevelLighting::bake(&partitioned);
    assert!(lighting.is_partitioned(0));
    assert!(lighting.same_probe_region(0, [2.0, 1.5, 5.0], [3.0, 1.5, 5.0]));
    assert!(!lighting.same_probe_region(0, [2.0, 1.5, 5.0], [2.0, -1.0, 5.0]));
    assert!(!lighting.same_probe_region(0, [2.0, 1.5, 5.0], [2.0, 10.0, 5.0]));
    assert!(!lighting.same_probe_region(0, [4.5, 1.5, 5.0], [5.5, 1.5, 5.0]));
    let split_zones = lighting.zones_in_room(0);
    assert_eq!(split_zones.len(), 2, "the wall must split the room");
    let total: f32 = split_zones.iter().map(|zone| zone.area_m2).sum();
    assert!(
        (total - 100.0).abs() < 1.0,
        "areas must tile the room: {total}"
    );
    let lit = split_zones
        .iter()
        .find(|zone| zone.fixture_count == 1)
        .expect("one side owns the fixture");
    let dark = split_zones
        .iter()
        .find(|zone| zone.fixture_count == 0)
        .expect("the other side owns none");
    assert!(
        lit.baseline.luminance() > dark.baseline.luminance() + 0.2,
        "a fixture's own area must beat the isolated area: {} vs {}",
        lit.baseline.luminance(),
        dark.baseline.luminance()
    );
    assert_eq!(dark.baseline, ambient_color());
    // The zone view must agree with the per-position baseline lookup.
    let position = if lit.area_m2 > 0.0 { 2.0 } else { 7.0 };
    assert_eq!(
        lighting.baseline_in_room(0, position, 5.0),
        lit.baseline,
        "the reported area baseline must be the sampled one"
    );
}

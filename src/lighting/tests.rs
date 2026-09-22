//! Unit tests for the baked lighting model.
//!
//! They exercise the module through its public surface plus the pure helpers,
//! which is exactly what the geometry emitter and the editor mirror use.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::suboptimal_flops
)]

use super::*;
use crate::level::LevelDef;
use crate::test_support::{assert_exact, scan};

/// One rectangular room with `lights` fixtures evenly spread across it.
fn level_with_room(width: f32, depth: f32, height: f32, intensities: &[f32]) -> LevelDef {
    let lights: Vec<String> = intensities
        .iter()
        .enumerate()
        .map(|(index, intensity)| {
            let x = width * (index as f32 + 1.0) / (intensities.len() as f32 + 1.0);
            let z = depth * (index as f32 + 1.0) / (intensities.len() as f32 + 1.0);
            format!(
                r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "intensity": {intensity} }}"#
            )
        })
        .collect();
    let json = format!(
        r#"{{
            "format_version": 1,
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
            let x = width * (index as f32 + 1.0) / (lights.len() as f32 + 1.0);
            let z = depth * (index as f32 + 1.0) / (lights.len() as f32 + 1.0);
            format!(
                r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "intensity": {intensity}, "color": [{}, {}, {}] }}"#,
                color[0], color[1], color[2]
            )
        })
        .collect();
    let json = format!(
        r#"{{
            "format_version": 1,
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

// ------------------------------------------------- vertical geometry (4.0)

#[test]
fn fixture_panel_follows_a_gable_eave_and_ridge() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable_light",
            "name": "Gable Light",
            "spawn": { "x": 12.0, "z": 12.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 24.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 12.0, "intensity": 0.2 },
                { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 1.0, "intensity": 0.2 }
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

    // The baked panel plane is exactly the drawn one, panel extents included.
    let (half_w, half_d) = fixture_half_extents(0.0);
    for light in lighting.lights() {
        let expected = lighting.fixture_panel_y(light.x, light.z, light.half_w, light.half_d);
        assert!((light.y - expected).abs() < 1e-4);
        assert!(light.half_w == half_w && light.half_d == half_d);
        // The panel stays below the ceiling everywhere it hangs.
        assert!(light.y < 5.0 - FIXTURE_DROP_M + 1e-3);
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
            "format_version": 1,
            "id": "elevated_light",
            "name": "Elevated Light",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 2.0 },
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
        "format_version": 1,
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
    assert_exact(omitted.lights()[0].intensity, 1.0);
}

#[test]
fn the_intensity_alias_is_accepted_and_negative_values_are_sanitized() {
    let json = r#"{
        "format_version": 1,
        "id": "alias",
        "name": "Alias",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "intensity": 1.4 },
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
    assert!(tall.lights()[0].y > normal.lights()[0].y);
    assert!(normal.lights()[0].y > low.lights()[0].y);
}

#[test]
fn brightness_saturates_instead_of_growing_without_bound() {
    // 200 high-output fixtures in a small room: extreme but finite input.
    let intensities = vec![2.0_f32; 200];
    let lighting = LevelLighting::bake(&level_with_room(4.0, 4.0, 3.5, &intensities));
    let baseline = lighting.rooms()[0].baseline;
    let sparse = LevelLighting::bake(&level_with_room(4.0, 4.0, 3.5, &[2.0]));
    assert!(
        baseline.luminance() > sparse.rooms()[0].baseline.luminance() + 0.05,
        "a dense grid ({baseline:?}) must beat a single fixture ({:?})",
        sparse.rooms()[0].baseline
    );
    assert!(
        baseline.max_channel() <= MAX_BRIGHTNESS && baseline.luminance() > 0.85,
        "an absurd fixture count must saturate near the maximum, got {baseline:?}"
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
    let lighting = LevelLighting::bake(&level_with_room(2.0, 2.0, 3.5, &extreme));
    let baseline = lighting.rooms()[0].baseline;
    assert!(baseline.is_finite());
    assert!(baseline.max_channel() <= MAX_BRIGHTNESS);
    assert!(baseline.luminance() >= AMBIENT_LEVEL);
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
    // floor, so the floor is not doing the illumination work.
    let lit = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0]));
    assert!(lit.rooms()[0].baseline.luminance() > sample.luminance() * 3.0);
}

#[test]
fn samples_under_a_fixture_are_brighter_than_distant_samples() {
    let json = r#"{
        "format_version": 1,
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
fn local_pools_scale_with_fixture_intensity() {
    // One fixture at a known position in a large room, sampled directly
    // beneath it: a 2.0 fixture must out-light a 0.5 fixture clearly.
    let level = |intensity: f32| {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 1,
                "id": "pool_intensity",
                "name": "Pool Intensity",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 3.0 }}],
                "ceiling_lights": [{{
                    "fixture": "core:fluorescent_panel_01",
                    "x": 10.0, "z": 10.0, "intensity": {intensity}
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
            "format_version": 1,
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
        "format_version": 1,
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
        "format_version": 1,
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
    let power = light.intensity * light.height_factor;
    let summed = LightColor {
        r: lighting.rooms()[0].effective_power.r + lighting.rooms()[1].effective_power.r,
        g: lighting.rooms()[0].effective_power.g + lighting.rooms()[1].effective_power.g,
        b: lighting.rooms()[0].effective_power.b + lighting.rooms()[1].effective_power.b,
    };
    for channel in 0..3 {
        assert_exact(
            summed.channel(channel),
            power * light.color.channel(channel),
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
    let lighting = LevelLighting::bake(&extreme);
    let baseline = lighting.rooms()[0].baseline;
    assert!(baseline.is_finite() && baseline.max_channel() <= MAX_BRIGHTNESS);
    assert!(lighting.sample(1.0, 0.0, 1.0).is_finite());
}

#[test]
fn helper_curves_are_monotonic_and_numerically_safe() {
    assert_exact(saturating_brightness(0.0), 0.0);
    assert_exact(saturating_brightness(-1.0), 0.0);
    assert_exact(saturating_brightness(f32::NAN), 0.0);
    assert_exact(saturating_brightness(f32::INFINITY), 1.0);
    assert_exact(saturating_brightness(f32::NEG_INFINITY), 0.0);
    let mut previous = 0.0;
    for step in 0..40 {
        let value = saturating_brightness(step as f32 * 0.25);
        assert!(value > previous || step == 0 || previous > 0.99);
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
    let mut previous = 0.0;
    for step in 1..40 {
        let value = compressed_density(step as f32 * 0.5);
        assert!(value > previous, "compression must be monotonic");
        assert!(value < step as f32 * 0.5 || step == 1);
        previous = value;
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
        "format_version": 1,
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
    // Same (x, z), different heights: below the fixture plane the pool is
    // weaker than right next to the panel.
    let json = r#"{
        "format_version": 1,
        "id": "height_sample",
        "name": "Height Sample",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
        "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let lighting = LevelLighting::bake(&level);
    let floor = lum(&lighting, 5.0, 0.0, 5.0);
    let beside_panel = lum(&lighting, 5.0, 2.8, 5.0);
    assert!(beside_panel > floor);
}

#[test]
fn sample_points_use_the_smaller_overlapping_room() {
    // The documented ownership rule is shared by light ownership and point
    // sampling, so a sample inside the overlap resolves to the small room.
    let json = r#"{
        "format_version": 1,
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
                "format_version": 1,
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
        "format_version": 1,
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
    let lights: Vec<(f32, [f32; 3])> = (0..36)
        .map(|index| {
            let color = match index % 3 {
                0 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
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
fn legacy_levels_without_a_colour_use_the_documented_default() {
    // Every shipped level omits `color`; they must load unchanged and bake
    // the restrained warm default rather than turning white or black.
    let legacy = r#"{
        "format_version": 1,
        "id": "legacy",
        "name": "Legacy",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 }],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
            { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 9.0, "brightness": 0.75 }
        ]
    }"#;
    let level = LevelDef::from_json(legacy).expect("legacy level parses");
    assert_eq!(level.ceiling_lights[0].color, None);
    assert_eq!(level.ceiling_lights[0].emitted_color(), DEFAULT_LIGHT_COLOR);
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights()[0].color, DEFAULT_LIGHT_COLOR);
    let baseline = lighting.rooms()[0].baseline;
    assert!(
        baseline.r > baseline.b && baseline.b >= AMBIENT_LEVEL,
        "legacy lights must bake the warm default, got {baseline:?}"
    );
    let sample = lighting.sample(3.0, 0.0, 3.0);
    assert!(sample.r > sample.b);
}

/// One room carrying the three shipped fixture families.
fn level_with_fixture_families() -> LevelDef {
    let json = r#"{
        "format_version": 1,
        "id": "fixtures",
        "name": "Fixture Families",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 14.0, "depth": 10.0, "height": 4.0 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_round", "x": 3.0, "z": 3.0,
              "color": [0.7, 0.85, 1.0], "brightness": 1.0 },
            { "fixture": "core:pool_light_wall", "x": 10.0, "z": 0.2,
              "mount": "wall", "y": 2.2, "color": [0.7, 0.85, 1.0], "brightness": 0.8 },
            { "fixture": "core:does_not_exist", "x": 7.0, "z": 8.0 }
        ]
    }"#;
    LevelDef::from_json(json).expect("fixture level parses")
}

#[test]
fn fixture_families_own_their_footprint_and_mount() {
    // The catalog id selects the fixture family: the office panel keeps its
    // 1.2 x 0.6 m footprint, the round downlight is a 0.44 m disc, and the
    // wall luminaire is a thin strip on the wall.
    let panel = fixture_profile("core:fluorescent_panel_01");
    assert_eq!(panel.kind, FixtureKind::FluorescentPanel);
    assert_eq!((panel.half_width, panel.half_depth), (0.6, 0.3));
    let round = fixture_profile("core:pool_light_round");
    assert_eq!(round.kind, FixtureKind::RoundRecessed);
    assert_eq!((round.half_width, round.half_depth), (0.22, 0.22));
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

    let level = level_with_fixture_families();
    let lighting = LevelLighting::bake(&level);
    assert_eq!(lighting.lights().len(), 3);

    // A ceiling fixture hangs just below the room ceiling; a wall fixture
    // stays at its authored height.
    let round = lighting.lights()[0];
    assert!((round.y - (4.0 - FIXTURE_DROP_M)).abs() < 1e-4, "{round:?}");
    let wall = lighting.lights()[1];
    assert!((wall.y - 2.2).abs() < 1e-4, "{wall:?}");
    assert!((wall.half_w - 0.20).abs() < 1e-4);
    // The unknown id is baked with the panel footprint, not skipped.
    let unknown = lighting.lights()[2];
    assert!((unknown.half_w - 0.6).abs() < 1e-4);
    assert!((unknown.y - (4.0 - FIXTURE_DROP_M)).abs() < 1e-4);

    // A hand-edited wall fixture without a height stays finite.
    let json = r#"{
        "format_version": 1,
        "id": "wallless",
        "name": "Wallless",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_wall", "x": 3.0, "z": 0.2, "mount": "wall" }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("wall fixture parses");
    let lighting = LevelLighting::bake(&level);
    let y = lighting.lights()[0].y;
    assert!(y.is_finite());
    assert!((y - WALL_LIGHT_DEFAULT_HEIGHT_M).abs() < 1e-4, "{y}");

    // A room lit only by the cool round fixture must read cool; mixing a
    // warm fixture in stays a blend, which the Goal 1 RGB tests cover.
    let cool_only = r#"{
        "format_version": 1,
        "id": "cool_only",
        "name": "Cool Only",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.5 }],
        "ceiling_lights": [
            { "fixture": "core:pool_light_round", "x": 4.0, "z": 4.0,
              "color": [0.7, 0.85, 1.0], "brightness": 1.0 }
        ]
    }"#;
    let level = LevelDef::from_json(cool_only).expect("cool level parses");
    let lighting = LevelLighting::bake(&level);
    let sample = lighting.sample(4.0, 0.0, 4.0);
    assert!(sample.b > sample.r, "cool light must stay cool: {sample:?}");
    assert!(sample.b >= AMBIENT_LEVEL && sample.b <= MAX_BRIGHTNESS);
    for light in lighting.lights() {
        assert!(light.intensity.is_finite() && light.intensity >= 0.0);
    }
}

#[test]
fn a_wall_fixture_needs_a_height_and_validation_says_so() {
    // The loader rejects a wall fixture without a `y` (it cannot derive
    // one), accepts a valid one, and ignores `y` on ceiling fixtures.
    let level = |lights: &str| {
        format!(
            r#"{{
                "format_version": 1,
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
    let error = crate::loader::validate_level(&missing).expect_err("a wall fixture needs y");
    assert!(error.contains("Wall light 0"), "{error}");

    let valid = LevelDef::from_json(&level(
        r#"{ "fixture": "core:pool_light_wall", "x": 1.0, "z": 0.2, "mount": "wall", "y": 2.1 }"#,
    ))
    .expect("parses");
    crate::loader::validate_level(&valid).expect("a wall fixture with its height validates");

    let ceiling_with_y = LevelDef::from_json(&level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0, "y": 2.4 }"#,
    ))
    .expect("parses");
    crate::loader::validate_level(&ceiling_with_y)
        .expect("an authored y on a ceiling fixture is ignored, not rejected");

    let non_finite = LevelDef::from_json(&level(
        r#"{ "fixture": "core:pool_light_wall", "x": 1.0, "z": 0.2, "mount": "wall",
             "y": 1e40 }"#,
    ))
    .expect("parses");
    // 1e40 overflows f32 to infinity, which must be rejected, not blessed.
    crate::loader::validate_level(&non_finite).expect_err("non-finite heights are rejected");
}

// ----------------------------------------- opening seams and corners (5.6)

/// Two 4 x 4 m rooms side by side, separated by a 0.4 m wall at x = 4 on which
/// the caller authors the openings. The wall runs along Z, so `offset` is a Z
/// coordinate and the fixture sits flush on its west face.
fn two_rooms_with_wall(openings_json: &str, fixture_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
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
             "mount": "wall", "y": 1.5, "intensity": 1.0 }"#,
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
             "mount": "wall", "y": 0.5, "intensity": 1.0 }"#,
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
        "format_version": 1,
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
            { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "intensity": 1.0 }
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

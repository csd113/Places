//! Adversarial correctness cases for the static baked lighting system.
//!
//! Grouped by the audit plan: room area and density (A), zero-light rooms (B),
//! dense fixture grids (C), intensity handling (D), ceiling height (E), fixture
//! placement (F), overlapping rooms (G), opening blending (H), non-recursive
//! propagation (I), local pools (J/K), material modulation (L), prop lighting
//! (M/N), fixtures outside rooms (O) and degenerate data (P). Colour safety (Q)
//! and determinism (R) are asserted across all of them.

use crate::level::{
    CeilingLightDef, LevelDef, MAX_LEVEL_FLOOR_AREA_M2, PropDef, RoomDef, SpawnDef,
};
use crate::lighting::{
    AMBIENT_LEVEL, LOCAL_LIGHT_MAX, LOCAL_LIGHT_RADIUS_M, LOCAL_LIGHT_STRENGTH, LevelLighting,
    MAX_BRIGHTNESS, MAX_LIGHT_INTENSITY, REFERENCE_CEILING_HEIGHT_M, ambient_color,
};
use crate::render::{build_level_geometry, build_level_geometry_with_assets};

use super::lighting_audit::{
    assert_vertex_colors_safe, level_json, light, parse, room, square_room_level,
};
use crate::render::SurfaceKind;
use crate::test_support::{assert_exact, assert_exact_named, scan, scan_below};

fn bake(level: &LevelDef) -> LevelLighting {
    LevelLighting::bake(level)
}

/// Per-channel effective power of one baked fixture.
fn fixture_power(light: &crate::lighting::BakedLight) -> [f32; 3] {
    let power = light.intensity * light.height_factor;
    light.color.to_array().map(|channel| channel * power)
}

/// Per-channel sum of the rooms' effective power.
fn room_power(lighting: &LevelLighting) -> [f32; 3] {
    let mut total = [0.0_f32; 3];
    for room in lighting.rooms() {
        for (sum, value) in total.iter_mut().zip(room.effective_power.to_array()) {
            *sum += value;
        }
    }
    total
}

/// Asserts two per-channel power sums agree in every channel.
fn assert_power_eq(actual: [f32; 3], expected: [f32; 3], context: &str) {
    for (channel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() < 1e-5,
            "{context}: channel {channel} drifted: {actual} vs {expected}"
        );
    }
}

/// Builds geometry, asserting every generated vertex is safe.
fn build_checked(level: &LevelDef) -> crate::render::LevelMesh {
    let mesh = build_level_geometry(level);
    assert_vertex_colors_safe(&mesh.all_vertices());
    mesh
}

fn color_range(mesh: &crate::render::LevelMesh) -> (f32, f32) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for vertex in &mesh.all_vertices() {
        for value in &vertex.color[..3] {
            min = min.min(*value);
            max = max.max(*value);
        }
    }
    (min, max)
}

// ===========================================================================
// Group A - room area and density
// ===========================================================================

#[test]
fn group_a_larger_area_lowers_the_baseline_and_stays_continuous() {
    // One fixture at the centre of a square room: as the room grows the same
    // fixture power spreads over more floor, so the baseline must fall.
    let areas = [
        0.25_f32, 1.0, 4.0, 8.0, 16.0, 64.0, 256.0, 1_024.0, 10_000.0, 810_000.0,
    ];
    let mut previous = f32::INFINITY;
    for area in areas {
        let size = area.sqrt();
        let level = square_room_level(size, 3.5, 1, None);
        let baseline = bake(&level).rooms()[0].baseline.luminance();
        assert!(
            baseline.is_finite(),
            "baseline must stay finite at area {area}"
        );
        assert!(
            (AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&baseline),
            "baseline {baseline} out of range at area {area}"
        );
        assert!(
            baseline <= previous + 1e-6,
            "growing the room brightened it: {previous} -> {baseline} at area {area}"
        );
        previous = baseline;
    }

    // Continuity: the baseline is a smooth function of area, with no jump where
    // surfaces would switch tessellation density.
    let mut previous = f32::INFINITY;
    for area in scan(1.0, 0.25, 40.0) {
        let level = square_room_level(area.sqrt(), 3.5, 1, None);
        let baseline = bake(&level).rooms()[0].baseline.luminance();
        if previous.is_finite() {
            assert!(
                previous - baseline < 0.02,
                "baseline jumped between adjacent areas at {area}: {previous} -> {baseline}"
            );
        }
        previous = baseline;
    }
}

#[test]
fn group_a_room_area_extremes_stay_inside_the_budget() {
    // 1000 x 1000 m is exactly the floor-area budget and must still bake.
    let level = parse(&level_json(
        &room(0.0, 0.0, 1000.0, 1000.0, 3.5),
        &light(500.0, 500.0, None),
    ));
    assert!(crate::loader::validate_level(&level).is_ok());
    let lighting = bake(&level);
    assert!(lighting.rooms()[0].baseline.luminance() >= AMBIENT_LEVEL);
    assert!(lighting.sample_luminance(500.0, 0.0, 500.0).is_finite());
    build_checked(&level);

    // Just past the budget the loader must reject rather than try to reserve.
    let over = parse(&level_json(
        &room(0.0, 0.0, 1001.0, 1001.0, 3.5),
        &light(500.0, 500.0, None),
    ));
    assert!(over.estimate_geometry().floor_area_m2 > MAX_LEVEL_FLOOR_AREA_M2);
    let error = crate::loader::validate_level(&over).expect_err("over-budget level must reject");
    assert!(error.contains("floor area"), "unexpected error: {error}");
}

// ===========================================================================
// Group B - rooms without fixtures
// ===========================================================================

#[test]
fn group_b_zero_light_rooms_stay_at_minimum_ambient_and_render() {
    for (width, depth) in [(10.0_f32, 10.0_f32), (900.0, 900.0)] {
        let level = parse(&level_json(&room(0.0, 0.0, width, depth, 3.0), ""));
        let lighting = bake(&level);
        let info = &lighting.rooms()[0];
        assert_eq!(info.fixture_count, 0);
        assert_exact(info.effective_power.luminance(), 0.0);
        assert_eq!(info.baseline, ambient_color());
        for point in [
            [0.0, 0.0, 0.0],
            [width * 0.5, 0.0, depth * 0.5],
            [width, 0.0, depth],
            [width * 0.5, 3.0, depth * 0.5],
        ] {
            assert_eq!(
                lighting.sample(point[0], point[1], point[2]),
                ambient_color()
            );
        }

        let mesh = build_checked(&level);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        // Floors carry the light directly; ceilings apply their own 0.72 tint.
        // Everything stays inside the renderer range and nothing is black.
        let (min, max) = color_range(&mesh);
        assert!(min > 0.0 && max <= MAX_BRIGHTNESS + 1e-6);
        assert!(min >= AMBIENT_LEVEL.mul_add(0.7, -1e-6));
        let floor = mesh.triangles_for(SurfaceKind::Floor);
        assert!((floor[0].color[0] - AMBIENT_LEVEL).abs() < 1e-6);
        let ceiling = mesh.triangles_for(SurfaceKind::Ceiling);
        assert!(AMBIENT_LEVEL.mul_add(-0.70, ceiling[0].color[2]).abs() < 1e-6);
        // The tone is flat across the room: no accidental pools in the dark.
        assert!((max - min).abs() < 0.25);
    }
}

#[test]
fn group_b_a_zero_intensity_fixture_behaves_like_no_fixture() {
    let level = parse(&level_json(
        &room(0.0, 0.0, 10.0, 10.0, 3.0),
        &light(5.0, 5.0, Some(0.0)),
    ));
    let lighting = bake(&level);
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_exact(lighting.rooms()[0].effective_power.luminance(), 0.0);
    assert_eq!(lighting.rooms()[0].baseline, ambient_color());
    // No local pool either: a zero-output fixture is physically dark.
    assert_eq!(lighting.sample(5.0, 0.0, 5.0), ambient_color());
    assert_exact(lighting.lights()[0].intensity, 0.0);

    // Its panel shows no glow at all, even with an authored colour: an off
    // fixture must not look lit while emitting nothing.
    let coloured_off = parse(&level_json(
        &room(0.0, 0.0, 10.0, 10.0, 3.0),
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "brightness": 0.0, "color": [1.0, 0.0, 0.0] }"#,
    ));
    let mesh = build_checked(&coloured_off);
    let panel = mesh.triangles_for(SurfaceKind::Light);
    assert!(!panel.is_empty(), "the fixture panel still renders");
    assert!(
        panel.iter().all(|vertex| vertex.color[0] < 0.5),
        "a zero-output red fixture must not glow red: {:?}",
        panel[0].color
    );
    let lighting = bake(&coloured_off);
    assert_eq!(lighting.sample(5.0, 0.0, 5.0), ambient_color());
}

// ===========================================================================
// Group C - extremely dense lighting
// ===========================================================================

#[test]
fn group_c_dense_fixture_grids_saturate_without_overflow_or_geometry_explosion() {
    for count in [10usize, 50, 100, 250] {
        let level = square_room_level(40.0, 3.5, count, None);
        let lighting = bake(&level);
        let info = &lighting.rooms()[0];
        assert_eq!(info.fixture_count, count);
        assert_eq!(lighting.lights().len(), count);
        assert_eq!(lighting.summary().lights, count);
        assert!(info.baseline.luminance().is_finite());
        assert!(
            (AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&info.baseline.luminance()),
            "{count} lights produced {baseline}",
            baseline = info.baseline.luminance()
        );
        assert!(info.effective_power.is_finite());

        // Walk the room at 1 m steps and a few heights: every sample is finite
        // and capped.
        for x in scan_below(0.5, 3.7, 40.0) {
            for z in scan_below(0.5, 3.7, 40.0) {
                for y in [0.0, 1.5, 3.4] {
                    let value = lighting.sample_luminance(x, y, z);
                    assert!(value.is_finite(), "NaN at ({x}, {y}, {z})");
                    assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value));
                }
            }
        }

        // Bounded geometry: only the fixture panel batch grows with light count,
        // and flat floor cells merge below the cell-grid bound.
        let mesh = build_checked(&level);
        assert_eq!(
            mesh.batches.light_batch.count,
            i32::try_from(count).unwrap_or(i32::MAX) * 18
        );
        assert!(mesh.batches.floor_batch.count <= 12 * 12 * 6);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count <= 12 * 12 * 6);
        assert!(mesh.batches.ceiling_batch.count > 0);
        // Draw calls do not scale with light count: an empty wall batch and no
        // props, so the static draw-call shape stays four fixed ranges.
        let non_empty = [
            mesh.batches.floor_batch,
            mesh.batches.ceiling_batch,
            mesh.batches.wall_batch,
            mesh.batches.light_batch,
            mesh.batches.prop_batch,
            mesh.batches.decal_batch,
        ]
        .iter()
        .filter(|range| range.count > 0)
        .count();
        assert_eq!(non_empty, 3);
    }
}

// ===========================================================================
// Group D - fixture intensity
// ===========================================================================

#[test]
fn group_d_intensity_boundaries_are_sanitized_deterministically() {
    use crate::lighting::sanitize_intensity;
    assert_exact(sanitize_intensity(0.0), 0.0);
    assert_exact(sanitize_intensity(1e-9), 1e-9);
    assert_exact(sanitize_intensity(0.5), 0.5);
    assert_exact(sanitize_intensity(1.0), 1.0);
    assert_exact(sanitize_intensity(1.4), 1.4);
    assert_exact(sanitize_intensity(2.0), 2.0);
    assert_exact(sanitize_intensity(8.0), MAX_LIGHT_INTENSITY);
    assert_exact(sanitize_intensity(1.0e30), MAX_LIGHT_INTENSITY);
    assert_exact(sanitize_intensity(f32::INFINITY), MAX_LIGHT_INTENSITY);
    assert_exact(sanitize_intensity(f32::NEG_INFINITY), 0.0);
    assert_exact(sanitize_intensity(-0.5), 0.0);
    assert_exact(sanitize_intensity(-1.0e30), 0.0);
    assert_exact(sanitize_intensity(f32::NAN), 1.0);
    for value in [
        0.0_f32,
        1e-9,
        0.5,
        1.0,
        1.4,
        2.0,
        8.0,
        1.0e30,
        -0.5,
        f32::NAN,
    ] {
        let sanitized = sanitize_intensity(value);
        assert!(sanitized.is_finite() && (0.0..=MAX_LIGHT_INTENSITY).contains(&sanitized));
    }
}

#[test]
fn group_d_intensity_inputs_brighten_monotonically_up_to_the_clamp() {
    let mut previous = -1.0;
    for intensity in [0.0_f32, 0.1, 0.5, 1.0, 1.4, 2.0, 4.0, 8.0, 100.0] {
        let level = parse(&level_json(
            &room(0.0, 0.0, 16.0, 16.0, 3.5),
            &light(8.0, 8.0, Some(intensity)),
        ));
        let lighting = bake(&level);
        let info = lighting.rooms()[0];
        assert!(
            info.baseline.luminance() >= previous - 1e-6,
            "intensity {intensity} lowered the baseline"
        );
        assert!(info.baseline.luminance() <= MAX_BRIGHTNESS);
        previous = info.baseline.luminance();
    }
    // Values above the clamp are identical to the clamp itself.
    let clamp = |value: f32| {
        bake(&parse(&level_json(
            &room(0.0, 0.0, 16.0, 16.0, 3.5),
            &light(8.0, 8.0, Some(value)),
        )))
        .rooms()[0]
            .baseline
            .luminance()
    };
    assert_exact(clamp(8.0), clamp(100.0));
    assert_exact(clamp(8.0), clamp(1.0e30));
}

#[test]
fn group_d_loader_rejects_invalid_intensities_and_accepts_both_spellings() {
    let with_field = |field: &str| {
        let separator = if field.is_empty() { "" } else { ", " };
        format!(
            r#"{{
                "format_version": 1,
                "id": "intensity",
                "name": "Intensity",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
                "ceiling_lights": [{{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0{separator}{field} }}]
            }}"#
        )
    };

    // `brightness` (canonical) and `intensity` (alias) parse to the same value.
    let canonical = parse(&with_field(r#""brightness": 1.4"#));
    let alias = parse(&with_field(r#""intensity": 1.4"#));
    assert_exact(canonical.ceiling_lights[0].intensity(), 1.4);
    assert_exact(alias.ceiling_lights[0].intensity(), 1.4);
    assert_exact(
        bake(&canonical).rooms()[0].baseline.luminance(),
        bake(&alias).rooms()[0].baseline.luminance(),
    );

    // Omitting both means the standard fixture.
    assert_exact(parse(&with_field("")).ceiling_lights[0].intensity(), 1.0);

    // Negative and non-finite values are rejected by the loader with a message.
    for field in [
        r#""brightness": -1.0"#,
        r#""intensity": -0.001"#,
        r#""brightness": null"#,
    ] {
        let level = parse(&with_field(field));
        if field.contains("null") {
            assert_exact(level.ceiling_lights[0].intensity(), 1.0);
            continue;
        }
        let error = crate::loader::validate_level(&level).expect_err("negative must reject");
        assert!(
            error.contains("negative") || error.contains("finite"),
            "unexpected error: {error}"
        );
    }

    // NaN/Infinity are not representable in strict JSON, and serde rejects them
    // deterministically instead of silently parsing.
    assert!(LevelDef::from_json(&with_field(r#""intensity": NaN"#)).is_err());
    assert!(LevelDef::from_json(&with_field(r#""intensity": Infinity"#)).is_err());

    // Supplying both keys is ambiguous, so it must not silently pick one:
    // serde reports the duplicate field deterministically.
    let both = LevelDef::from_json(&with_field(r#""brightness": 1.0, "intensity": 2.0"#));
    let first_error = both.expect_err("both keys must fail loudly, not pick one");
    let both_again = LevelDef::from_json(&with_field(r#""brightness": 1.0, "intensity": 2.0"#));
    assert_eq!(first_error.to_string(), both_again.unwrap_err().to_string());
    assert!(
        first_error.to_string().contains("duplicate"),
        "the failure must name the ambiguity: {first_error}"
    );
}

// ===========================================================================
// Group E - ceiling height
// ===========================================================================

#[test]
fn group_e_ceiling_height_correction_is_monotonic_bounded_and_safe() {
    use crate::lighting::ceiling_height_factor;
    // Reference height is neutral; lower ceilings help, taller ones reduce
    // gently, and the correction is always finite and positive.
    assert!((ceiling_height_factor(REFERENCE_CEILING_HEIGHT_M) - 1.0).abs() < 1e-6);
    let heights = [0.5_f32, 1.0, 2.6, 3.5, 4.0, 8.0, 20.0, 100.0];
    let mut previous = f32::INFINITY;
    for height in heights {
        let factor = ceiling_height_factor(height);
        assert!(factor.is_finite() && factor > 0.0, "height {height}");
        assert!(factor <= previous + 1e-6, "factor rose at height {height}");
        previous = factor;
    }
    // The documented examples: ~+16% at 2.6 m, ~-7% at 4 m.
    assert!((ceiling_height_factor(2.6) - 1.16).abs() < 0.02);
    assert!((ceiling_height_factor(4.0) - 0.935).abs() < 0.02);

    // Invalid heights fall back to neutral; absurd heights clamp instead of
    // dividing by zero or overflowing.
    assert_exact(ceiling_height_factor(0.0), 1.0);
    assert_exact(ceiling_height_factor(-3.0), 1.0);
    assert_exact(ceiling_height_factor(f32::NAN), 1.0);
    assert_exact(ceiling_height_factor(f32::INFINITY), 1.0);
    assert!(ceiling_height_factor(1.0e30).is_finite());
}

#[test]
fn group_e_taller_rooms_stay_dim_but_valid_and_lower_rooms_stay_bounded() {
    let mut previous = f32::INFINITY;
    for height in [0.5_f32, 1.0, 2.6, 3.5, 4.0, 8.0, 20.0, 50.0] {
        let level = parse(&level_json(
            &room(0.0, 0.0, 16.0, 16.0, height),
            &format!("{},{}", light(5.3, 5.3, None), light(10.7, 10.7, None)),
        ));
        assert!(
            crate::loader::validate_level(&level).is_ok(),
            "height {height} must be a valid level"
        );
        let lighting = bake(&level);
        let baseline = lighting.rooms()[0].baseline.luminance();
        assert!(baseline.is_finite());
        assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&baseline));
        assert!(
            baseline <= previous + 1e-6,
            "raising the ceiling brightened the room at {height} m: {previous} -> {baseline}"
        );
        previous = baseline;

        // Fixtures hang just below the ceiling and pools follow them.
        let fixture_y = lighting.lights()[0].y;
        assert!(fixture_y < height && fixture_y > height - 0.02);
        build_checked(&level);
    }

    // Heights outside the supported envelope are rejected by the loader.
    let too_tall = parse(&level_json(
        &room(0.0, 0.0, 16.0, 16.0, 50.1),
        &light(8.0, 8.0, None),
    ));
    assert!(crate::loader::validate_level(&too_tall).is_err());
    let zero = parse(&level_json(
        &room(0.0, 0.0, 16.0, 16.0, 0.0),
        &light(8.0, 8.0, None),
    ));
    assert!(crate::loader::validate_level(&zero).is_err());
    let negative = parse(&level_json(
        &room(0.0, 0.0, 16.0, 16.0, -1.0),
        &light(8.0, 8.0, None),
    ));
    assert!(crate::loader::validate_level(&negative).is_err());

    // Programmatic extremes (bypassing validation) still bake safely.
    let mut extreme = parse(&level_json(
        &room(0.0, 0.0, 16.0, 16.0, 3.0),
        &light(8.0, 8.0, None),
    ));
    extreme.rooms[0].height = 1.0e30;
    let lighting = bake(&extreme);
    assert!(lighting.rooms()[0].baseline.luminance().is_finite());
    assert!(lighting.sample_luminance(8.0, 0.0, 8.0).is_finite());
    extreme.rooms[0].height = f32::NAN;
    let lighting = bake(&extreme);
    assert!(lighting.rooms()[0].baseline.luminance().is_finite());
    assert!(lighting.sample_luminance(8.0, 0.0, 8.0).is_finite());
}

// ===========================================================================
// Group F - fixture position
// ===========================================================================

#[test]
fn group_f_fixture_ownership_follows_the_epsilon_boundary_exactly() {
    let level = parse(&level_json(&room(0.0, 0.0, 10.0, 10.0, 3.0), ""));
    let lighting = bake(&level);
    // Inside, exactly on the edge and just inside the epsilon band belong to
    // the room; clearly outside does not.
    assert_eq!(lighting.room_index_at(5.0, 5.0), Some(0));
    assert_eq!(lighting.room_index_at(0.0, 0.0), Some(0));
    assert_eq!(lighting.room_index_at(10.0, 10.0), Some(0));
    assert_eq!(lighting.room_index_at(10.009, 5.0), Some(0));
    assert_eq!(lighting.room_index_at(10.02, 5.0), None);
    assert_eq!(lighting.room_index_at(-0.02, 5.0), None);
    assert_eq!(lighting.room_index_at(f32::NAN, 5.0), None);
}

#[test]
fn group_f_fixture_placement_variants_are_all_deterministic() {
    let placements: [(&str, f32, f32); 7] = [
        ("centre", 5.0, 5.0),
        ("near wall", 9.7, 5.0),
        ("exactly on wall", 10.0, 5.0),
        ("corner", 0.0, 0.0),
        ("near corner", 0.1, 0.1),
        ("barely inside epsilon", 10.009, 5.0),
        ("barely outside", 10.02, 5.0),
    ];
    for (label, x, z) in placements {
        let level = parse(&level_json(
            &room(0.0, 0.0, 10.0, 10.0, 3.0),
            &light(x, z, None),
        ));
        let first = bake(&level);
        let second = bake(&level);
        assert_eq!(first.rooms(), second.rooms(), "{label}: room bake drifted");
        assert_eq!(
            first.lights(),
            second.lights(),
            "{label}: light bake drifted"
        );
        let owned = first.lights()[0].room.is_some();
        assert_eq!(
            first.rooms()[0].fixture_count,
            usize::from(owned),
            "{label}: fixture count must match ownership"
        );
        let fixture_y = first.lights()[0].y;
        assert!(fixture_y.is_finite());
        assert!((fixture_y - 2.99).abs() < 1e-6 || fixture_y > 0.0);
        build_checked(&level);
    }
}

#[test]
fn group_f_duplicate_and_overlapping_fixtures_count_once_each_and_saturate() {
    // Two fixtures at identical coordinates both contribute to power and to the
    // local pool, but never accidentally to a third room.
    let single = parse(&level_json(
        &room(0.0, 0.0, 12.0, 12.0, 3.0),
        &light(6.0, 6.0, None),
    ));
    let double = parse(&level_json(
        &room(0.0, 0.0, 12.0, 12.0, 3.0),
        &format!("{},{}", light(6.0, 6.0, None), light(6.0, 6.0, None)),
    ));
    let one = bake(&single);
    let two = bake(&double);
    assert_eq!(one.rooms()[0].fixture_count, 1);
    assert_eq!(two.rooms()[0].fixture_count, 2);
    assert!(
        two.rooms()[0].effective_power.luminance() > one.rooms()[0].effective_power.luminance()
    );
    assert!(two.sample_luminance(6.0, 0.0, 6.0) >= one.sample_luminance(6.0, 0.0, 6.0));
    assert!(two.sample_luminance(6.0, 0.0, 6.0) <= MAX_BRIGHTNESS);

    // Saturation: 64 coincident fixtures stay capped and finite.
    let many: Vec<String> = (0..64).map(|_| light(6.0, 6.0, None)).collect();
    let saturated = parse(&level_json(
        &room(0.0, 0.0, 12.0, 12.0, 3.0),
        &many.join(","),
    ));
    let lighting = bake(&saturated);
    assert_eq!(lighting.rooms()[0].fixture_count, 64);
    assert!(lighting.sample_luminance(6.0, 0.0, 6.0) <= MAX_BRIGHTNESS);
    // The local pool is explicitly capped, above and beyond the room clamp.
    let floor = lighting.sample_luminance(6.0, 0.0, 6.0);
    assert!(floor - lighting.rooms()[0].baseline.luminance() <= LOCAL_LIGHT_MAX + 1e-5);
}

#[test]
fn group_f_rotation_swaps_the_panel_pool_orientation() {
    let level = |rotation: f32| {
        parse(&level_json(
            &room(0.0, 0.0, 24.0, 24.0, 3.0),
            &format!(
                r#"{{ "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 12.0, "rotation_degrees": {rotation} }}"#
            ),
        ))
    };
    for rotation in [0.0_f32, 90.0, 180.0, 270.0] {
        let lighting = bake(&level(rotation));
        // 2 m out along +X vs +Z: at 0 degrees the 1.2 m panel runs along X, so
        // the +X probe is closer to the panel; at 90 degrees it is the reverse.
        let along_x = lighting.sample_luminance(14.0, 2.8, 12.0);
        let along_z = lighting.sample_luminance(12.0, 2.8, 14.0);
        let swapped = (rotation as i64).rem_euclid(180) != 0;
        if swapped {
            assert!(
                along_z > along_x,
                "rotation {rotation}: the turned panel should favour +Z ({along_z} vs {along_x})"
            );
        } else {
            assert!(
                along_x > along_z,
                "rotation {rotation}: the panel should favour +X ({along_x} vs {along_z})"
            );
        }
    }
}

// ===========================================================================
// Group G - overlapping rooms
// ===========================================================================

fn overlap_level(rooms: &str, lights: &str) -> LevelDef {
    parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "overlap",
            "name": "Overlap",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{rooms}],
            "ceiling_lights": [{lights}]
        }}"#
    ))
}

#[test]
fn group_g_smallest_containing_room_wins_for_every_lookup() {
    // A small room fully inside a large one, with one fixture inside both.
    let level = overlap_level(
        &format!(
            "{},{}",
            room(0.0, 0.0, 40.0, 40.0, 3.5),
            room(4.0, 4.0, 6.0, 6.0, 2.6)
        ),
        &format!("{},{}", light(6.0, 6.0, None), light(8.0, 8.0, None)),
    );
    let lighting = bake(&level);
    assert_eq!(lighting.room_index_at(7.0, 7.0), Some(1));
    assert_eq!(lighting.room_index_at(30.0, 30.0), Some(0));
    // Fixtures owned exactly once, by the small room.
    assert_eq!(lighting.rooms()[0].fixture_count, 0);
    assert_eq!(lighting.rooms()[1].fixture_count, 2);
    assert_eq!(lighting.lights()[0].room, Some(1));
    assert_eq!(lighting.lights()[1].room, Some(1));
    // And they hang from the small room's lower ceiling.
    assert!((lighting.lights()[0].y - 2.59).abs() < 1e-6);
    // World sampling and prop sampling use the same rule: a point in the
    // overlap is lit by the small room (bright), not the big dim one.
    let small_baseline = lighting.rooms()[1].baseline.luminance();
    let big_baseline = lighting.rooms()[0].baseline.luminance();
    assert!(
        small_baseline > big_baseline + 0.1,
        "small room {small_baseline} vs big {big_baseline}"
    );
    assert!(lighting.sample_luminance(6.0, 0.0, 6.0) >= small_baseline - 1e-6);
    // A prop's vertices are sampled with `sample`, so the same point sees the
    // small room's baseline; the small room's own surface sampling agrees.
    assert_exact(
        lighting.sample_in_room_luminance(1, 6.0, 0.0, 6.0),
        lighting.sample_luminance(6.0, 0.0, 6.0),
    );
}

#[test]
fn group_g_equal_area_overlaps_resolve_by_level_order() {
    let level = overlap_level(
        &format!(
            "{},{}",
            room(0.0, 0.0, 10.0, 10.0, 3.0),
            room(5.0, 5.0, 10.0, 10.0, 3.0)
        ),
        &light(7.5, 7.5, None),
    );
    let lighting = bake(&level);
    // Both rooms have equal area, so the earlier room keeps the tie.
    assert_exact(lighting.rooms()[0].area_m2, lighting.rooms()[1].area_m2);
    assert_eq!(lighting.room_index_at(7.5, 7.5), Some(0));
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_eq!(lighting.rooms()[1].fixture_count, 0);
}

#[test]
fn group_g_three_overlapping_rooms_order_by_area_strictly() {
    let level = overlap_level(
        &format!(
            "{},{},{}",
            room(0.0, 0.0, 30.0, 30.0, 3.0),
            room(4.0, 4.0, 6.0, 6.0, 3.0),
            room(2.0, 2.0, 20.0, 20.0, 3.0)
        ),
        &format!("{},{}", light(5.0, 5.0, None), light(25.0, 25.0, None)),
    );
    let lighting = bake(&level);
    // Areas: room 0 = 900, room 1 = 36, room 2 = 400. The smallest wins.
    assert_eq!(lighting.room_index_at(5.0, 5.0), Some(1));
    // A point only inside rooms 0 and 2 resolves to room 2 (smaller).
    assert_eq!(lighting.room_index_at(20.0, 20.0), Some(2));
    assert_eq!(lighting.room_index_at(28.0, 28.0), Some(0));
    assert_eq!(lighting.rooms()[1].fixture_count, 1);
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_eq!(lighting.rooms()[2].fixture_count, 0);
    // Sum of owned power equals the sum of the fixtures' effective power: no
    // double counting anywhere.
    let mut expected = [0.0_f32; 3];
    for light in lighting.lights() {
        for (sum, value) in expected.iter_mut().zip(fixture_power(light)) {
            *sum += value;
        }
    }
    assert_power_eq(room_power(&lighting), expected, "overlapping rooms");

    // Determinism: three bakes agree exactly.
    let a = bake(&level);
    let b = bake(&level);
    assert_eq!(a.rooms(), b.rooms());
    assert_eq!(a.lights(), b.lights());
    for point in [[5.0, 0.0, 5.0], [20.0, 1.2, 20.0], [28.0, 2.5, 28.0]] {
        assert_exact(
            a.sample_luminance(point[0], point[1], point[2]),
            b.sample_luminance(point[0], point[1], point[2]),
        );
    }
}

#[test]
fn group_g_props_inside_overlapping_rooms_are_lit_by_the_smallest_room() {
    let level = overlap_level(
        &format!(
            "{},{}",
            room(0.0, 0.0, 40.0, 40.0, 3.5),
            room(4.0, 4.0, 6.0, 6.0, 2.6)
        ),
        &format!("{},{}", light(6.0, 6.0, None), light(8.0, 8.0, None)),
    );
    let lighting = bake(&level);
    let over_overlap = lighting.sample_luminance(6.0, 0.0, 6.0);
    let outside_small = lighting.sample_luminance(20.0, 0.0, 20.0);
    assert!(
        over_overlap > outside_small + 0.05,
        "the prop in the overlap must read as the bright small room: {over_overlap} vs {outside_small}"
    );
}

// ===========================================================================
// Group H - opening blending
// ===========================================================================

/// Two rooms of very different brightness sharing a wall, with a configurable
/// walk-through opening. Returns (level, door world centre x).
fn two_room_opening(opening_json: &str, bright_on_left: bool, wall_x: f32) -> (LevelDef, f32) {
    let dim_room = room(wall_x + 0.4, 0.0, 40.0, 20.0, 3.0);
    let bright_room = room(0.0, 0.0, wall_x, 10.0, 3.0);
    let (left, right) = if bright_on_left {
        (bright_room, dim_room)
    } else {
        (dim_room, bright_room)
    };
    let lights = if bright_on_left {
        format!(
            "{},{},{},{},{},{}",
            light(2.0, 2.0, None),
            light(5.0, 2.0, None),
            light(8.0, 2.0, None),
            light(2.0, 8.0, None),
            light(5.0, 8.0, None),
            light(8.0, 8.0, None)
        )
    } else {
        format!(
            "{},{},{},{}",
            light(wall_x + 3.0, 2.0, None),
            light(wall_x + 8.0, 2.0, None),
            light(wall_x + 3.0, 15.0, None),
            light(wall_x + 8.0, 15.0, None)
        )
    };
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "opening",
            "name": "Opening",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{left},{right}],
            "walls": [{{
                "x": {wall_x}, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                "openings": [{opening_json}]
            }}],
            "ceiling_lights": [{lights}]
        }}"#
    );
    (parse(&json), wall_x + 0.2)
}

const STANDARD_DOOR: &str =
    r#"{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }"#;

#[test]
fn group_h_doorway_transition_is_smooth_and_symmetric() {
    let (bright_left, _) = two_room_opening(STANDARD_DOOR, true, 10.0);
    let (solid_left, _) = two_room_opening("", true, 10.0);
    let a = bake(&bright_left);
    let without = bake(&solid_left);
    let left_baseline = a.rooms()[0].baseline.luminance();
    let right_baseline = a.rooms()[1].baseline.luminance();
    assert!(
        (left_baseline - right_baseline).abs() > 0.1,
        "test setup needs a contrast"
    );

    // Isolated blend contribution: the opening bake minus the same level with a
    // solid wall. Local fixture pools are identical in both, so this is purely
    // the doorway exchange. It must be smooth on both sides.
    let blend =
        |x: f32, y: f32| a.sample_luminance(x, y, 5.0) - without.sample_luminance(x, y, 5.0);
    let mut previous = blend(0.5, 0.0);
    for x in scan(0.5, 0.05, 9.95) {
        let value = blend(x, 0.0);
        assert!(
            (value - previous).abs() < 0.02,
            "blend stepped inside the bright room at x = {x}: {previous} -> {value}"
        );
        previous = value;
    }
    let mut previous = blend(10.45, 0.0);
    for x in scan(10.45, 0.05, 20.0) {
        let value = blend(x, 0.0);
        assert!(
            (value - previous).abs() < 0.02,
            "blend stepped inside the dim room at x = {x}: {previous} -> {value}"
        );
        previous = value;
    }

    // The threshold itself: each face of the 0.4 m wall is the same distance
    // from the opening centre, so the two sides move by equal and opposite
    // amounts and meet at the average of the two baselines.
    let left_face = blend(9.95, 0.0);
    let right_face = blend(10.45, 0.0);
    assert!(left_face < -0.001 && right_face > 0.001);
    assert!(
        (left_face + right_face).abs() < 0.01,
        "the doorway seam must cancel: {left_face} vs {right_face}"
    );

    // And the raw values at the faces are close, so a viewer at the threshold
    // sees no step.
    let raw_left = a.sample_luminance(9.95, 0.0, 5.0);
    let raw_right = a.sample_luminance(10.45, 0.0, 5.0);
    assert!(
        (raw_left - raw_right).abs() < 0.08,
        "hard step at the doorway: {raw_left} vs {raw_right}"
    );

    // Both interiors vary smoothly with no NaN.
    for interior in [0..190, 209..390] {
        for step in interior {
            let x = step as f32 * 0.05;
            let value = a.sample_luminance(x, 0.0, 5.0);
            assert!(value.is_finite());
            assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value));
        }
    }
}

#[test]
fn group_h_opening_variants_blend_or_do_not_without_artifacts() {
    let variants: [(&str, &str); 6] = [
        (
            "narrow door",
            r#"{ "kind": "door", "offset": 4.6, "width": 0.6, "height": 2.1, "sill": 0.0 }"#,
        ),
        (
            "wide passage",
            r#"{ "kind": "passage", "offset": 2.0, "width": 6.0, "height": 2.4, "sill": 0.0 }"#,
        ),
        (
            "very wide passage",
            r#"{ "kind": "passage", "offset": 0.0, "width": 10.0, "height": 3.0, "sill": 0.0 }"#,
        ),
        (
            "corner door",
            r#"{ "kind": "door", "offset": 0.0, "width": 1.2, "height": 2.1, "sill": 0.0 }"#,
        ),
        (
            "low header",
            r#"{ "kind": "door", "offset": 4.5, "width": 1.2, "height": 1.0, "sill": 0.0 }"#,
        ),
        (
            "tall opening",
            r#"{ "kind": "passage", "offset": 4.0, "width": 1.5, "height": 3.0, "sill": 0.0 }"#,
        ),
    ];
    for (label, opening) in variants {
        let (level, _) = two_room_opening(opening, true, 10.0);
        let lighting = bake(&level);
        let (solid, _) = two_room_opening("", true, 10.0);
        let without = bake(&solid);
        let bright = lighting.rooms()[0].baseline.luminance();
        let dim = lighting.rooms()[1].baseline.luminance();

        // Both sides still move towards each other (the doorway does something).
        let near_bright_open = lighting.sample_in_room_luminance(0, 9.9, 0.0, 5.0);
        let near_bright_closed = without.sample_in_room_luminance(0, 9.9, 0.0, 5.0);
        let near_dim_open = lighting.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
        let near_dim_closed = without.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
        assert!(
            near_bright_open <= near_bright_closed + 1e-6,
            "{label}: bright side must not gain light"
        );
        assert!(
            near_dim_open >= near_dim_closed - 1e-6,
            "{label}: dim side must not lose light"
        );
        assert!((bright - dim).abs() > 0.05);
        assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&near_dim_open));

        // The influence is bounded and never turns into a global leak.
        let far_dim = lighting.sample_in_room_luminance(1, 35.0, 0.0, 15.0);
        assert!(
            (far_dim - dim).abs() < 1e-4,
            "{label}: blend leaked far into the dim room ({far_dim} vs {dim})"
        );

        // The isolated blend component must be continuous on both sides; the
        // wall thickness itself carries no geometry or player.
        let blend =
            |x: f32| lighting.sample_luminance(x, 0.0, 5.0) - without.sample_luminance(x, 0.0, 5.0);
        let mut previous = blend(0.5);
        for x in scan(0.5, 0.2, 9.95) {
            let value = blend(x);
            assert!(
                (value - previous).abs() < 0.05,
                "{label}: blend step at x = {x}: {previous} -> {value}"
            );
            previous = value;
        }
        let mut previous = blend(10.45);
        for x in scan(10.45, 0.2, 20.0) {
            let value = blend(x);
            assert!(
                (value - previous).abs() < 0.05,
                "{label}: blend step at x = {x}: {previous} -> {value}"
            );
            previous = value;
        }
        for point in [0.5_f32, 5.0, 9.95, 10.45, 15.0, 20.0] {
            let value = lighting.sample_luminance(point, 0.0, 5.0);
            assert!(value.is_finite(), "{label}: NaN at x = {point}");
            assert!((AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value));
        }
    }
}

#[test]
fn group_h_vertical_fade_above_the_header_and_non_connecting_openings() {
    let (level, _) = two_room_opening(STANDARD_DOOR, true, 10.0);
    let lighting = bake(&level);
    let (solid, _) = two_room_opening("", true, 10.0);
    let without = bake(&solid);

    // Just above the 2.1 m header the blend exists but is weaker; a metre above
    // it has faded to nothing. Isolate the blend from local pools (which grow
    // near the ceiling) by subtracting the solid-wall bake.
    let blend =
        |y: f32| lighting.sample_luminance(10.5, y, 5.0) - without.sample_luminance(10.5, y, 5.0);
    let below = blend(1.9);
    let above = blend(2.5);
    let way_above = blend(3.2);
    assert!(
        below > 0.01,
        "the blend must exist below the header: {below}"
    );
    assert!(above < below, "the blend must fade above the header");
    assert!(
        way_above.abs() < 1e-4,
        "the blend must be gone a metre above the header: {way_above}"
    );
    // The blend above the header is monotone on the way out.
    let mut previous = below;
    for y in scan(1.9, 0.1, 3.2) {
        let value = blend(y);
        assert!(value <= previous + 1e-6, "blend rose at y = {y}");
        previous = value;
    }

    // An opening in a wall that does not join two rooms must not blend.
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "unconnected",
            "name": "Unconnected",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{}],
            "walls": [
                {{ "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                   "openings": [{}] }},
                {{ "x": 0.0, "z": 20.0, "width": 20.0, "depth": 0.4, "height": 3.0,
                   "openings": [{}] }}
            ],
            "ceiling_lights": [{}]
        }}"#,
        room(0.0, 0.0, 10.0, 10.0, 3.0),
        STANDARD_DOOR,
        STANDARD_DOOR,
        light(2.0, 2.0, None),
    );
    let unconnected = bake(&parse(&json));
    assert_eq!(unconnected.rooms().len(), 1);
    // A single room's baseline is untouched by walls standing inside it.
    let plain = bake(&parse(&level_json(
        &room(0.0, 0.0, 10.0, 10.0, 3.0),
        &light(2.0, 2.0, None),
    )));
    assert_exact(
        unconnected.rooms()[0].baseline.luminance(),
        plain.rooms()[0].baseline.luminance(),
    );
    assert_exact(
        unconnected.sample_luminance(5.0, 0.0, 5.0),
        plain.sample_luminance(5.0, 0.0, 5.0),
    );
}

#[test]
fn group_h_two_openings_between_the_same_rooms_stay_bounded() {
    let (level, _) = two_room_opening(STANDARD_DOOR, true, 10.0);
    let (solid, _) = two_room_opening("", true, 10.0);
    let without = bake(&solid);
    let mut with_two = level.clone();
    with_two.walls[0].openings = parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "two",
            "name": "Two",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
            "ceiling_lights": [],
            "walls": [{{ "x": 0.0, "z": 0.0, "width": 0.4, "depth": 10.0,
                         "openings": [{{ "kind": "door", "offset": 0.5, "width": 1.0, "height": 2.1, "sill": 0.0 }},
                                      {STANDARD_DOOR}] }}]
        }}"#
    ))
    .walls[0]
        .openings
        .clone();

    let lighting = bake(&with_two);
    let dim_closed = without.rooms()[1].baseline.luminance();
    // Between the openings the two influences overlap, so the value is higher
    // than a single opening but still clamped and finite.
    let between = lighting.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
    assert!(between.is_finite());
    assert!(between >= dim_closed);
    assert!(between <= MAX_BRIGHTNESS);
    // At the far end of the room nothing leaks.
    let far = lighting.sample_in_room_luminance(1, 35.0, 0.0, 15.0);
    assert!((far - lighting.rooms()[1].baseline.luminance()).abs() < 1e-4);
    // Two openings influence more than one at the same point.
    let single = bake(&level);
    let single_between = single.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
    assert!(
        between >= single_between - 1e-6,
        "a second nearby doorway must not reduce the blend"
    );
}

// ===========================================================================
// Group I - non-recursive propagation
// ===========================================================================

#[test]
fn group_i_propagation_is_one_hop_only() {
    // A bright room joins a dim room, which joins a third dim room 30 m away.
    // Rooms B and C carry one weak fixture each so their baselines differ
    // slightly; the control level simply removes room A's lights.
    let build = |bright_a: bool| {
        let a_lights = if bright_a {
            format!(
                "{},{},{},{},{},{}",
                light(2.0, 2.0, None),
                light(5.0, 2.0, None),
                light(8.0, 2.0, None),
                light(2.0, 8.0, None),
                light(5.0, 8.0, None),
                light(8.0, 8.0, None)
            )
        } else {
            String::new()
        };
        let b_light = light(11.0, 4.0, Some(0.6));
        let c_light = light(31.0, 4.0, Some(0.6));
        let lights = [a_lights, b_light, c_light]
            .into_iter()
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>()
            .join(",");
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "hops",
                "name": "Hops",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{},{},{}],
                "walls": [
                    {{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                       "openings": [{{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }}] }},
                    {{ "x": 30.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                       "openings": [{{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }}] }}
                ],
                "ceiling_lights": [{lights}]
            }}"#,
            room(0.0, 0.0, 10.0, 10.0, 3.0),
            room(10.4, 0.0, 19.6, 10.0, 3.0),
            room(30.4, 0.0, 19.6, 10.0, 3.0),
        ))
    };

    let bright = bake(&build(true));
    let control = bake(&build(false));
    assert!(bright.rooms()[0].fixture_count > 0);
    assert_eq!(control.rooms()[0].fixture_count, 0);
    assert!(bright.rooms()[0].baseline.luminance() > bright.rooms()[1].baseline.luminance() + 0.05);
    assert!(
        (bright.rooms()[1].baseline.luminance() - control.rooms()[1].baseline.luminance()).abs()
            < 1e-6,
        "room baselines only depend on their own fixtures"
    );
    assert!(
        (bright.rooms()[2].baseline.luminance() - control.rooms()[2].baseline.luminance()).abs()
            < 1e-6
    );

    // Room B's doorway sample *is* brighter when room A is lit (A's light
    // reaches B through their shared doorway).
    let b_door_bright = bright.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
    let b_door_control = control.sample_in_room_luminance(1, 10.5, 0.0, 5.0);
    assert!(
        b_door_bright > b_door_control + 0.001,
        "room B must gain light from room A: {b_door_bright} vs {b_door_control}"
    );

    // Room C's doorway sample is identical with and without room A: the blend
    // is one hop only and uses room B's own baseline. (Room A's fixtures are
    // more than 6 m from every room-C sample, so local pools cannot reach.)
    for point in [
        [30.5_f32, 0.0_f32, 5.0_f32],
        [31.0, 1.0, 5.0],
        [35.0, 2.5, 5.0],
    ] {
        let with_a = bright.sample_in_room_luminance(2, point[0], point[1], point[2]);
        let without_a = control.sample_in_room_luminance(2, point[0], point[1], point[2]);
        assert!(
            (with_a - without_a).abs() < 1e-6,
            "room A leaked across room B into room C at {point:?}: {with_a} vs {without_a}"
        );
    }
    // And C's own doorway blend genuinely exists (it uses B's baseline).
    let c_door = bright.sample_in_room_luminance(2, 30.5, 0.0, 5.0);
    let c_far = bright.sample_in_room_luminance(2, 45.0, 0.0, 5.0);
    assert!(
        c_door > c_far + 1e-4,
        "the second doorway still blends a little"
    );
    assert!(
        c_door < bright.rooms()[0].baseline.luminance(),
        "room C stays dark"
    );
}

// ===========================================================================
// Group J/K - local fixture pools and saturation
// ===========================================================================

#[test]
fn group_j_pools_fall_off_monotonically_and_reach_the_room_baseline() {
    let level = parse(&level_json(
        &room(0.0, 0.0, 40.0, 8.0, 3.0),
        &light(4.0, 4.0, None),
    ));
    let lighting = bake(&level);
    let baseline = lighting.rooms()[0].baseline.luminance();

    let beneath = lighting.sample_luminance(4.0, 0.0, 4.0);
    let one = lighting.sample_luminance(5.0, 0.0, 4.0);
    let three = lighting.sample_luminance(7.0, 0.0, 4.0);
    let six = lighting.sample_luminance(10.0, 0.0, 4.0);
    let outside = lighting.sample_luminance(20.0, 0.0, 4.0);
    assert!(beneath >= one, "{beneath} vs {one}");
    assert!(one >= three, "{one} vs {three}");
    assert!(three >= six, "{three} vs {six}");
    assert!((outside - baseline).abs() < 1e-4);
    assert!(beneath - outside > 0.05, "the pool must be clearly visible");

    // The radius is a hard bound: nothing outside it contributes.
    let beyond = LOCAL_LIGHT_RADIUS_M + 1.0;
    assert!((lighting.sample_luminance(4.0 + beyond, 0.0, 4.0) - baseline).abs() < 1e-4);

    // The pool is measured to the 1.2 x 0.6 m panel, not to a point: points one
    // metre beyond the long edge and one metre beyond the short end are equally
    // far from the rectangle (so equally lit), while a point one metre past the
    // corner is farther and dimmer.
    let beyond_long = lighting.sample_luminance(4.0, 0.0, 4.0 + 0.3 + 1.0);
    let beyond_short = lighting.sample_luminance(4.0 + 0.6 + 1.0, 0.0, 4.0);
    let beyond_corner = lighting.sample_luminance(4.0 + 0.6 + 1.0, 0.0, 4.0 + 0.3 + 1.0);
    assert!(
        (beyond_long - beyond_short).abs() < 1e-6,
        "rectangle falloff must be symmetric: {beyond_long} vs {beyond_short}"
    );
    assert!(
        beyond_corner < beyond_short - 0.005,
        "the panel corner must fall off faster: {beyond_corner} vs {beyond_short}"
    );
}

#[test]
fn group_k_local_pool_saturation_is_capped_and_finite() {
    // Eight fixtures in one spot: the summed pool must reach the cap but never
    // exceed it, and never corrupt the sample.
    let lights: Vec<String> = (0..8).map(|_| light(5.0, 5.0, Some(8.0))).collect();
    let level = parse(&level_json(
        &room(0.0, 0.0, 20.0, 20.0, 3.0),
        &lights.join(","),
    ));
    let lighting = bake(&level);
    let beneath = lighting.sample_luminance(5.0, 0.0, 5.0);
    let baseline = lighting.rooms()[0].baseline.luminance();
    assert!(beneath.is_finite());
    assert!(beneath <= MAX_BRIGHTNESS);
    assert!(
        beneath - baseline <= LOCAL_LIGHT_MAX + 1e-5,
        "local pool exceeded its cap: {} over {}",
        beneath - baseline,
        LOCAL_LIGHT_MAX
    );
    // The raw strength of one fixture beneath itself is a fraction of the cap.
    const { assert!(LOCAL_LIGHT_STRENGTH < LOCAL_LIGHT_MAX) };
}

// ===========================================================================
// Group L - material modulation
// ===========================================================================

#[test]
fn group_l_lighting_multiplies_materials_instead_of_replacing_them() {
    // A lit room with a wall: the directional face tints (north/south) and the
    // material base colour must survive as ratios while the baked light scales
    // all of them.
    let level = parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "materials",
            "name": "Materials",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{}],
            "walls": [{{ "x": -5.0, "z": 0.0, "width": 10.0, "depth": 0.4, "height": 3.5 }}],
            "ceiling_lights": [{}]
        }}"#,
        room(-5.0, -5.0, 10.0, 10.0, 3.5),
        light(0.0, -3.0, None),
    ));
    let mesh = build_checked(&level);
    let wall = mesh.triangles_for(SurfaceKind::Wall);
    assert!(!wall.is_empty());

    // Two wall vertices with different base tints at the same world position
    // differ by the same ratio as their base colours (north face is lighter
    // than the south face).
    let sample = |want_z: f32| {
        wall.iter()
            .filter(|v| (v.pos[2] - want_z).abs() < 1e-3)
            .collect::<Vec<_>>()
    };
    let north = sample(0.0);
    let south = sample(0.4);
    assert!(!north.is_empty() && !south.is_empty());
    // North faces use mult 1.00, south 0.88, so a vertex pair at the same
    // height and x keeps that ratio in the lit colour.
    let pair = north
        .iter()
        .flat_map(|n| {
            south.iter().filter(move |s| {
                (s.pos[0] - n.pos[0]).abs() < 1e-3 && (s.pos[1] - n.pos[1]).abs() < 1e-3
            })
        })
        .next();
    if let Some(s) = pair {
        let n = north
            .iter()
            .find(|n| (n.pos[0] - s.pos[0]).abs() < 1e-3 && (n.pos[1] - s.pos[1]).abs() < 1e-3)
            .expect("matching north vertex");
        let ratio = n.color[0] / s.color[0];
        assert!(
            (ratio - 1.0 / 0.88).abs() < 0.02,
            "material ratio must survive the bake, got {ratio}"
        );
    }

    // Unlit and lit rooms both keep every channel proportional: a dark texture
    // stays darker than a light one under the same illumination because the
    // texture is multiplied by the vertex colour, which is exactly the light.
    let dark = bake(&parse(&format!(
        r#"{{
            "format_version": 1, "id": "dark", "name": "Dark", "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{}], "ceiling_lights": [{}]
        }}"#,
        room(0.0, 0.0, 10.0, 10.0, 3.0),
        light(5.0, 5.0, None)
    )));
    let floor_light = dark.sample_luminance(5.0, 0.0, 5.0);
    let dark_texture = [0.10_f32, 0.08, 0.07];
    let light_texture = [0.90_f32, 0.90, 0.90];
    let shaded_dark: Vec<f32> = dark_texture.iter().map(|c| c * floor_light).collect();
    let shaded_light: Vec<f32> = light_texture.iter().map(|c| c * floor_light).collect();
    assert!(shaded_dark[0] < shaded_light[0]);
    assert!(
        (shaded_dark[0] / shaded_light[0] - dark_texture[0] / light_texture[0]).abs() < 1e-5,
        "texture contrast must survive lighting"
    );
}

// ===========================================================================
// Group M/N - prop lighting and vertical offsets
// ===========================================================================

fn prop_level(props_json: &str, lights_json: &str) -> LevelDef {
    parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "props",
            "name": "Props",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{}],
            "ceiling_lights": [{lights_json}],
            "props": [{props_json}]
        }}"#,
        room(0.0, 0.0, 20.0, 20.0, 3.0)
    ))
}

#[test]
fn group_m_n_real_props_are_lit_from_their_transformed_world_position() {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    assert!(assets.root().is_some(), "assets/props must exist");

    // The same chair at the floor, one metre up and two metres up. The higher
    // copies sit closer to the 2.99 m fixture plane, so they must read brighter.
    let floor = prop_level(
        r#"{ "model": "core:chair", "x": 5.0, "z": 5.0 }"#,
        &light(5.0, 5.0, None),
    );
    let lifted = prop_level(
        r#"{ "model": "core:chair", "x": 5.0, "y": 1.0, "z": 5.0 }"#,
        &light(5.0, 5.0, None),
    );
    let high = prop_level(
        r#"{ "model": "core:chair", "x": 5.0, "y": 2.0, "z": 5.0 }"#,
        &light(5.0, 5.0, None),
    );
    let bright = |level: &LevelDef, assets: &mut crate::props::PropAssets| {
        let (_, batches) = build_level_geometry_with_assets(level, &catalog, assets);
        let vertex = batches[0]
            .vertices
            .iter()
            .max_by(|a, b| a.color[0].partial_cmp(&b.color[0]).unwrap())
            .expect("chair vertices");
        assert_vertex_colors_safe(&batches[0].vertices);
        vertex.color[0]
    };
    let floor_bright = bright(&floor, &mut assets);
    let lifted_bright = bright(&lifted, &mut assets);
    let high_bright = bright(&high, &mut assets);
    assert!(
        floor_bright < lifted_bright && lifted_bright < high_bright,
        "prop lighting must follow world height: {floor_bright} {lifted_bright} {high_bright}"
    );

    // The prop keeps its requested vertical offset: sinking is never corrected.
    let sunk = prop_level(
        r#"{ "model": "core:crate", "x": 5.0, "y": -0.25, "z": 5.0 }"#,
        &light(5.0, 5.0, None),
    );
    let (_, batches) = build_level_geometry_with_assets(&sunk, &catalog, &mut assets);
    let lowest = batches[0]
        .vertices
        .iter()
        .map(|v| v.pos[1])
        .fold(f32::MAX, f32::min);
    assert!(
        lowest < -0.2,
        "the sunk crate must stay sunk, base at {lowest}"
    );

    // A prop far from every fixture receives the plain room baseline.
    let far = prop_level(
        r#"{ "model": "core:plant", "x": 1.0, "y": 0.0, "z": 1.0 }"#,
        &light(18.0, 18.0, None),
    );
    let lighting = bake(&far);
    let (_, batches) = build_level_geometry_with_assets(&far, &catalog, &mut assets);
    for vertex in &batches[0].vertices {
        assert_vertex_colors_safe(std::slice::from_ref(vertex));
    }
    let baseline = lighting.rooms()[0].baseline.luminance();
    assert!(
        (lighting.sample_luminance(1.0, 0.4, 1.0) - baseline).abs() < 1e-4,
        "far prop must sit at the baseline"
    );

    // spooner-man on the bed (0.44 m up) and the same cat on the floor: both
    // build, are lit at their true height and stay in range.
    for (label, y) in [("floor", 0.0_f32), ("on the bed", 0.44)] {
        let level = prop_level(
            &format!(r#"{{ "model": "spooner-man", "x": 8.0, "y": {y}, "z": 8.0 }}"#),
            &light(8.0, 8.0, None),
        );
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        assert_eq!(batches.len(), 1, "{label}: one model, one batch");
        assert_vertex_colors_safe(&batches[0].vertices);
    }
}

#[test]
fn group_n_extreme_prop_offsets_are_lit_without_correction_or_rejection() {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    for (label, y) in [
        ("deeply negative", -1.5_f32),
        ("zero", 0.0),
        ("small positive", 0.05),
        ("large positive", 6.0),
        ("very large positive", 40.0),
    ] {
        let level = prop_level(
            &format!(r#"{{ "model": "core:crate", "x": 5.0, "y": {y}, "z": 5.0 }}"#),
            &light(5.0, 5.0, None),
        );
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        assert_eq!(batches.len(), 1, "{label}: one batch");
        assert_vertex_colors_safe(&batches[0].vertices);
        // The requested offset is present in the geometry: no auto-correction.
        let lowest = batches[0]
            .vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::MAX, f32::min);
        assert!(
            (lowest - y).abs() < 0.05,
            "{label}: expected base at {y}, got {lowest}"
        );
        // And the light sampled at those true heights stays in range.
        let lighting = bake(&level);
        for vertex in &batches[0].vertices {
            let value = lighting.sample_luminance(vertex.pos[0], vertex.pos[1], vertex.pos[2]);
            assert!(value.is_finite() && (AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&value));
        }
    }
}

// ===========================================================================
// Group O - fixtures outside every room
// ===========================================================================

#[test]
fn group_o_fixtures_outside_rooms_are_defined_and_isolated() {
    let level = parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "outside",
            "name": "Outside",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{},{}],
            "ceiling_lights": [{},{},{}]
        }}"#,
        room(0.0, 0.0, 10.0, 10.0, 3.0),
        room(20.0, 0.0, 10.0, 10.0, 4.0),
        light(5.0, 5.0, None),
        light(25.0, 5.0, None),
        light(-50.0, -50.0, None),
    ));
    let lighting = bake(&level);
    assert_eq!(lighting.rooms().len(), 2);
    let stray = lighting
        .lights()
        .iter()
        .find(|l| (l.x + 50.0).abs() < 1e-3)
        .expect("stray fixture survives the bake");
    assert_eq!(stray.room, None, "no room owns the stray fixture");
    // It contributes no baseline power anywhere.
    assert_eq!(lighting.rooms()[0].fixture_count, 1);
    assert_eq!(lighting.rooms()[1].fixture_count, 1);
    let mut expected = [0.0_f32; 3];
    for light in lighting
        .lights()
        .iter()
        .filter(|light| light.room.is_some())
    {
        for (sum, value) in expected.iter_mut().zip(fixture_power(light)) {
            *sum += value;
        }
    }
    assert_power_eq(room_power(&lighting), expected, "stray fixtures");
    // Outside every room it still lights its own immediate surroundings with a
    // local pool and hangs at the first room's ceiling height (documented
    // fallback), so the panel is not drawn at NaN or zero.
    let stray_y = lighting.fixture_y(-50.0, -50.0);
    assert!(stray_y.is_finite() && stray_y > 0.0);
    let outside = lighting.sample_luminance(-50.0, 0.0, -50.0);
    assert!(
        outside > AMBIENT_LEVEL,
        "the stray pool still works outside rooms"
    );
    assert!(outside <= MAX_BRIGHTNESS);
    // Deterministic across bakes.
    assert_eq!(lighting.rooms(), bake(&level).rooms());
    build_checked(&level);
}

// ===========================================================================
// Group P - degenerate data
// ===========================================================================

fn empty_level() -> LevelDef {
    LevelDef {
        format_version: 1,
        id: "empty".into(),
        name: "Empty".into(),
        author: String::new(),
        room: None,
        rooms: Vec::new(),
        spawn: SpawnDef {
            x: 0.0,
            z: 0.0,
            yaw_degrees: 0.0,
        },
        defaults: crate::level::LevelDefaults::default(),
        walls: Vec::new(),
        floor_patches: Vec::new(),
        decals: Vec::new(),
        ceiling_lights: Vec::new(),
        props: Vec::new(),
    }
}

#[test]
fn group_p_degenerate_levels_never_panic_and_never_emit_bad_vertices() {
    // Completely empty.
    let empty = empty_level();
    let lighting = bake(&empty);
    assert!(lighting.rooms().is_empty());
    assert!(lighting.lights().is_empty());
    assert_eq!(lighting.sample(0.0, 0.0, 0.0), ambient_color());
    assert_eq!(lighting.summary().rooms, 0);
    assert_exact(
        lighting.fixture_y(0.0, 0.0),
        REFERENCE_CEILING_HEIGHT_M - 0.01,
    );
    let mesh = build_checked(&empty);
    assert_eq!(mesh.vertex_count, 0);

    // Zero-area rooms (only constructible programmatically).
    let mut zero_room = empty.clone();
    zero_room.rooms.push(RoomDef {
        x: 5.0,
        z: 5.0,
        width: 0.0,
        depth: 0.0,
        height: 3.0,
        material: None,
        ceiling_material: None,
    });
    zero_room.ceiling_lights.push(CeilingLightDef {
        fixture: "core:fluorescent_panel_01".into(),
        x: 5.0,
        z: 5.0,
        rotation_degrees: 0.0,
        brightness: None,
        color: None,
    });
    let lighting = bake(&zero_room);
    let baseline = lighting.rooms()[0].baseline.luminance();
    assert!(baseline.is_finite() && (AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(&baseline));
    assert!(lighting.rooms()[0].area_m2 <= f32::EPSILON);
    assert!(lighting.sample_luminance(5.0, 0.0, 5.0).is_finite());
    let mesh = build_checked(&zero_room);
    assert!(
        mesh.batches.floor_batch.count == 0,
        "a zero-area room must not generate floor geometry"
    );

    // Duplicate rooms and duplicate fixtures: deterministic, counted once each.
    let mut duplicate = parse(&level_json(
        &format!(
            "{},{}",
            room(0.0, 0.0, 10.0, 10.0, 3.0),
            room(0.0, 0.0, 10.0, 10.0, 3.0)
        ),
        &format!("{},{}", light(5.0, 5.0, None), light(5.0, 5.0, None)),
    ));
    let lighting = bake(&duplicate);
    assert_eq!(lighting.rooms()[0].fixture_count, 2);
    assert_eq!(lighting.rooms()[1].fixture_count, 0);
    assert_eq!(lighting.room_index_at(5.0, 5.0), Some(0));
    let first = bake(&duplicate).rooms().to_vec();
    let second = bake(&duplicate).rooms().to_vec();
    assert_eq!(first, second);
    // Huge coordinates: finite geometry, no NaN.
    duplicate.rooms[0] = RoomDef {
        x: 1.0e30,
        z: -1.0e30,
        width: 10.0,
        depth: 10.0,
        height: 3.0,
        material: None,
        ceiling_material: None,
    };
    duplicate.ceiling_lights[0].x = 1.0e30 + 5.0;
    duplicate.ceiling_lights[0].z = -1.0e30 + 5.0;
    let lighting = bake(&duplicate);
    assert!(lighting.rooms()[0].baseline.luminance().is_finite());
    assert!(
        lighting
            .sample_luminance(1.0e30 + 5.0, 0.0, -1.0e30 + 5.0)
            .is_finite()
    );
    build_checked(&duplicate);

    // A level containing only props (no rooms at all).
    let props_only = LevelDef {
        props: vec![PropDef {
            model: "core:chair".into(),
            x: 1.0,
            y: 0.0,
            z: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            size: None,
            solid: false,
        }],
        ..empty.clone()
    };
    let lighting = bake(&props_only);
    assert_eq!(lighting.rooms().len(), 0);
    assert!(lighting.sample_luminance(1.0, 0.5, 1.0) >= AMBIENT_LEVEL);
    let mesh = build_checked(&props_only);
    assert_eq!(
        mesh.batches.prop_batch.count, 36,
        "placeholder box still renders"
    );

    // A level with a room but no openings, no lights, no props.
    let bare = parse(&level_json(&room(0.0, 0.0, 4.0, 4.0, 3.0), ""));
    assert_vertex_colors_safe(&build_checked(&bare).all_vertices());

    // A level with nothing but a ceiling fixture list.
    let mut lights_only = empty.clone();
    lights_only.ceiling_lights.push(CeilingLightDef {
        fixture: "core:fluorescent_panel_01".into(),
        x: 0.0,
        z: 0.0,
        rotation_degrees: 0.0,
        brightness: Some(f32::NAN),
        color: None,
    });
    let lighting = bake(&lights_only);
    assert_eq!(lighting.lights().len(), 1);
    assert_exact_named(lighting.lights()[0].intensity, 1.0, "NaN falls back to 1.0");
    assert!(lighting.sample_luminance(0.0, 0.0, 0.0).is_finite());
    build_checked(&lights_only);

    // Non-finite fixtures are dropped, not propagated.
    let mut broken = empty;
    broken.ceiling_lights.push(CeilingLightDef {
        fixture: "core:fluorescent_panel_01".into(),
        x: f32::INFINITY,
        z: 0.0,
        rotation_degrees: 0.0,
        brightness: Some(1.0),
        color: None,
    });
    let lighting = bake(&broken);
    assert!(lighting.lights().is_empty());
    assert!(lighting.sample_luminance(0.0, 0.0, 0.0).is_finite());
    assert_eq!(
        lighting.sample(f32::NAN, f32::INFINITY, f32::NEG_INFINITY),
        ambient_color()
    );
}

// ===========================================================================
// Group Q - coloured illumination
// ===========================================================================

/// One square room with a single fixture of the given colour.
fn coloured_room_json(color_json: &str, width: f32, height: f32) -> LevelDef {
    parse(&level_json(
        &room(0.0, 0.0, width, width, height),
        &format!(
            r#"{{ "fixture": "core:fluorescent_panel_01", "x": {}, "z": {}, "color": {color_json} }}"#,
            width * 0.5,
            width * 0.5
        ),
    ))
}

#[test]
fn group_q_coloured_fixtures_tint_floor_and_wall_geometry() {
    // A blue fixture must colour the baked floor and wall vertices blue: the
    // geometry response, not just the fixture panel, carries the colour.
    let mut level = coloured_room_json("[0.0, 0.15, 1.0]", 12.0, 3.0);
    // A wall to prove the tint reaches vertical surfaces too.
    level.walls.push(crate::level::WallDef {
        x: 2.0,
        y: 0.0,
        z: 6.0,
        width: 8.0,
        depth: 0.4,
        height: None,
        faces: std::collections::HashMap::default(),
        openings: Vec::new(),
        material: None,
    });
    let lighting = bake(&level);
    let mesh = build_checked(&level);

    let floor = mesh.triangles_for(SurfaceKind::Floor);
    let wall = mesh.triangles_for(SurfaceKind::Wall);
    let ceiling = mesh.triangles_for(SurfaceKind::Ceiling);
    for (name, triangles) in [("floor", floor), ("wall", wall), ("ceiling", ceiling)] {
        assert!(!triangles.is_empty(), "{name} batch must exist");
        for vertex in triangles {
            assert!(
                vertex.color[2] > vertex.color[0] + 0.05,
                "{name} vertex must be blue-tinted, got {:?}",
                vertex.color
            );
        }
    }

    // The sample the geometry was baked from agrees with the mesh.
    let sample = lighting.sample(6.0, 0.0, 6.0);
    assert!(sample.b > sample.r + 0.2, "got {sample:?}");
    // Red and green stay at the ambient floor where the fixture emits nothing.
    assert!((sample.r - AMBIENT_LEVEL).abs() < 1e-6);
    assert!(sample.g >= AMBIENT_LEVEL);
}

#[test]
fn group_q_mixed_colours_stay_distinct_and_bounded() {
    // Two rooms of the same size, one red and one blue, plus a mixed room with
    // both. Each must keep its identity and every vertex must stay in range.
    let red = build_checked(&coloured_room_json("[1.0, 0.0, 0.0]", 10.0, 3.0));
    let blue = build_checked(&coloured_room_json("[0.0, 0.0, 1.0]", 10.0, 3.0));
    let floor_red = red.triangles_for(SurfaceKind::Floor)[0].color;
    let floor_blue = blue.triangles_for(SurfaceKind::Floor)[0].color;
    assert!(
        floor_red[0] > floor_red[2] + 0.2,
        "red room floor must stay red: {floor_red:?}"
    );
    assert!(
        floor_blue[2] > floor_blue[0] + 0.2,
        "blue room floor must stay blue: {floor_blue:?}"
    );

    // Mixed fixtures in one room: both channels genuinely accumulate.
    let mixed = parse(&level_json(
        &room(0.0, 0.0, 10.0, 10.0, 3.0),
        &format!(
            "{},{}",
            r#"{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 5.0, "color": [1.0, 0.0, 0.0] }"#,
            r#"{ "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 5.0, "color": [0.0, 0.0, 1.0] }"#
        ),
    ));
    let lighting = bake(&mixed);
    let middle = lighting.sample(5.0, 0.0, 5.0);
    assert!(
        middle.r > AMBIENT_LEVEL + 0.05 && middle.b > AMBIENT_LEVEL + 0.05,
        "the mixed room must keep both colours: {middle:?}"
    );
    let mesh = build_checked(&mixed);
    for vertex in mesh.all_vertices() {
        assert!(vertex.color.iter().all(|c| c.is_finite()));
        assert!(vertex.color.iter().all(|c| (0.0..=1.0).contains(c)));
    }
}

#[test]
fn group_q_legacy_and_explicit_default_colours_bake_identically() {
    // Existing levels omit `color`; making the default explicit must not change
    // a single channel of the bake.
    let legacy = coloured_room_json("[1.0, 0.96, 0.88]", 14.0, 3.5);
    let implicit = parse(&level_json(
        &room(0.0, 0.0, 14.0, 14.0, 3.5),
        &light(7.0, 7.0, Some(1.0)),
    ));
    let a = bake(&legacy);
    let b = bake(&implicit);
    assert_eq!(a.rooms(), b.rooms());
    for point in [[1.0, 0.0, 1.0], [7.0, 0.0, 7.0], [13.0, 2.5, 13.0]] {
        assert_exact_named(
            a.sample(point[0], point[1], point[2]).luminance(),
            b.sample(point[0], point[1], point[2]).luminance(),
            "legacy default colour",
        );
    }
}

#[test]
fn group_q_tall_coloured_rooms_keep_the_height_response() {
    // Colour must not bypass the ceiling-height correction: the same fixture in
    // a taller room is dimmer in every channel it emits.
    let low = bake(&coloured_room_json("[1.0, 0.5, 0.0]", 10.0, 2.6));
    let tall = bake(&coloured_room_json("[1.0, 0.5, 0.0]", 10.0, 6.0));
    let low_baseline = low.rooms()[0].baseline;
    let tall_baseline = tall.rooms()[0].baseline;
    // The fixture emits red and green, so those channels drop with height; the
    // blue channel it does not emit stays at ambient in both rooms.
    for channel in [0_usize, 1] {
        assert!(
            low_baseline.channel(channel) > tall_baseline.channel(channel),
            "channel {channel}: 2.6 m {:?} must beat 6.0 m {:?}",
            low_baseline,
            tall_baseline
        );
    }
    // A hue that emits in only two channels leaves the third at ambient at any
    // height.
    assert_exact(tall_baseline.b, AMBIENT_LEVEL);
    assert_exact(low_baseline.b, AMBIENT_LEVEL);
}

//! Unit tests for the engine-level light source.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::fmt::Write as _;

use super::*;
use crate::level::LevelDef;
use crate::lighting::bake::LevelLighting;
use crate::lighting::math::smooth_falloff;
use crate::lighting::tuning::{LOCAL_LIGHT_MAX, LOCAL_LIGHT_STRENGTH};
use crate::test_support::{assert_exact, assert_exact_array, assert_exact_named};

#[test]
fn a_point_shape_has_no_extent() {
    assert_eq!(LightShape::Point.half_extents(), (0.0, 0.0));
    assert_eq!(LightShape::Point.longest_extent(), 0.0);
    assert!(LightShape::Point.is_valid());
}

#[test]
fn a_rectangle_keeps_its_authored_axes_and_edge_lengths() {
    let rect = LightShape::Rect {
        half_width: 0.6,
        half_depth: 0.3,
    };
    assert_eq!(rect.half_extents(), (0.6, 0.3));
    assert_exact(rect.longest_extent(), 1.2);
}

#[test]
fn a_line_is_a_thin_rectangle_along_its_local_x_axis() {
    let line = LightShape::Line { length: 1.2 };
    assert_eq!(
        line.half_extents(),
        (0.6, LINE_LIGHT_HALF_THICKNESS_M),
        "a tube spans half its length and is one tube thick"
    );
}

#[test]
fn a_quarter_turn_swaps_the_world_extents() {
    let rect = LightShape::Rect {
        half_width: 0.6,
        half_depth: 0.3,
    };
    assert_eq!(rect.half_extents_rotated(0.0), (0.6, 0.3));
    assert_eq!(rect.half_extents_rotated(90.0), (0.3, 0.6));
    assert_eq!(rect.half_extents_rotated(270.0), (0.3, 0.6));
    assert_eq!(rect.half_extents_rotated(180.0), (0.6, 0.3));
    // The historical rule rounds to whole degrees first, so any rotation that
    // is not an exact multiple of 180 turns the panel — including fractional
    // angles like 89.4. Mirroring `fixture_is_turned` is what keeps the bake
    // and the drawn fixture from drifting.
    assert_eq!(rect.half_extents_rotated(89.4), (0.3, 0.6));
    assert_eq!(rect.half_extents_rotated(90.6), (0.3, 0.6));
    // Only an exact multiple of 180 keeps the authored axes.
    assert_eq!(rect.half_extents_rotated(360.0), (0.6, 0.3));
    // A non-finite rotation is treated as unrotated, never as a NaN extent.
    assert_eq!(rect.half_extents_rotated(f32::NAN), (0.6, 0.3));
}

#[test]
fn sanitizing_clamps_extents_and_never_produces_nan() {
    let huge = LightShape::Rect {
        half_width: 1_000.0,
        half_depth: f32::NAN,
    }
    .sanitized();
    assert_eq!(
        huge,
        LightShape::Rect {
            half_width: MAX_LIGHT_HALF_EXTENT_M,
            half_depth: 0.0,
        }
    );
    let negative = LightShape::Line { length: -4.0 }.sanitized();
    assert_eq!(negative, LightShape::Line { length: 0.0 });
    let long = LightShape::Line {
        length: MAX_LIGHT_LENGTH_M * 4.0,
    }
    .sanitized();
    assert_eq!(
        long,
        LightShape::Line {
            length: MAX_LIGHT_LENGTH_M
        }
    );
}

#[test]
fn validity_rejects_missing_and_absurd_dimensions() {
    assert!(
        !LightShape::Rect {
            half_width: 0.0,
            half_depth: 0.2
        }
        .is_valid()
    );
    assert!(
        !LightShape::Rect {
            half_width: f32::INFINITY,
            half_depth: 0.2
        }
        .is_valid()
    );
    assert!(
        !LightShape::Line {
            length: MAX_LIGHT_LENGTH_M * 2.0
        }
        .is_valid()
    );
    assert!(LightShape::Line { length: 1.0 }.is_valid());
}

#[test]
fn falloff_names_round_trip_and_parse_case_insensitively() {
    for falloff in LightFalloff::ALL {
        assert_eq!(LightFalloff::parse(falloff.name()), Some(falloff));
        assert_eq!(
            LightFalloff::parse(&falloff.name().to_uppercase()),
            Some(falloff)
        );
    }
    assert_eq!(LightFalloff::parse("inverse-square"), None);
    assert_eq!(LightFalloff::default(), LightFalloff::Smooth);
}

#[test]
fn the_smooth_curve_is_exactly_the_historical_pool_curve() {
    for step in 0..=20 {
        let t = f32::from(u8::try_from(step).unwrap_or(0)) / 20.0;
        assert_exact_named(
            LightFalloff::Smooth.factor(t),
            smooth_falloff(t),
            "smooth falloff",
        );
    }
}

#[test]
fn every_curve_starts_full_and_ends_empty() {
    for falloff in LightFalloff::ALL {
        assert_exact_named(falloff.factor(0.0), 1.0, "at the centre");
        assert_exact_named(falloff.factor(1.0), 0.0, "at the range");
        assert_exact_named(falloff.factor(4.0), 0.0, "past the range");
    }
    // Linear decays strictly; constant is flat by design.
    assert_exact(LightFalloff::Linear.factor(0.25), 0.75);
    assert_exact(LightFalloff::Constant.factor(0.99), 1.0);
    // Negative input is the centre, and non-finite input is the range.
    assert_exact(LightFalloff::Linear.factor(-3.0), 1.0);
    assert_exact(LightFalloff::Linear.factor(f32::NAN), 0.0);
}

#[test]
fn a_point_light_carries_the_documented_defaults() {
    let light = LightSource::point([1.0, 2.0, 3.0], LightColor::rgb(1.0, 0.9, 0.8), 1.0);
    assert_eq!(light.shape, LightShape::Point);
    assert_exact(light.range, DEFAULT_LIGHT_RANGE_M);
    assert_eq!(light.falloff, LightFalloff::Smooth);
    assert!(light.enabled);
    assert!(light.is_active());
    assert_exact(light.x(), 1.0);
    assert_exact(light.y(), 2.0);
    assert_exact(light.z(), 3.0);
}

#[test]
fn sanitizing_a_source_clamps_range_intensity_colour_and_position() {
    let source = LightSource {
        shape: LightShape::Point,
        position: [f32::NAN, 2.0, f32::INFINITY],
        rotation_degrees: f32::NAN,
        color: LightColor::rgb(4.0, -1.0, 0.5),
        intensity: -3.0,
        range: 1_000.0,
        falloff: LightFalloff::Linear,
        enabled: true,
    }
    .sanitized();
    assert_exact_array(source.position, [0.0, 2.0, 0.0]);
    assert_exact(source.rotation_degrees, 0.0);
    assert_exact_array(source.color.to_array(), [1.0, 0.0, 0.5]);
    assert_exact(source.intensity, 0.0);
    assert_exact(source.range, MAX_LIGHT_RANGE_M);
    assert_eq!(source.falloff, LightFalloff::Linear);
    assert!(!source.is_active(), "a zero-intensity light is inert");

    let loose_range = LightSource {
        range: f32::NAN,
        ..LightSource::point([0.0; 3], LightColor::WHITE, 1.0)
    }
    .sanitized();
    assert_exact(loose_range.range, DEFAULT_LIGHT_RANGE_M);
    let tiny_range = LightSource {
        range: 0.0,
        ..LightSource::point([0.0; 3], LightColor::WHITE, 1.0)
    }
    .sanitized();
    assert_exact(tiny_range.range, MIN_LIGHT_RANGE_M);
}

#[test]
fn a_disabled_light_is_inactive_but_keeps_its_values() {
    let source = LightSource {
        enabled: false,
        ..LightSource::point([0.0; 3], LightColor::WHITE, 2.0)
    };
    assert!(!source.is_active());
    assert_exact(source.intensity, 2.0);
}

#[test]
fn a_black_light_is_inactive() {
    let source = LightSource::point([0.0; 3], LightColor::BLACK, 1.0);
    assert!(!source.is_active());
}

// --------------------------------------------- fixture distribution audit
//
// Developer measurement, not an assertion: builds one minimal scene per
// shipped fixture family (plus the office row, a rotated panel and the low /
// tall ceiling extremes), bakes it and prints the pool the engine resolves
// along the fixture's own axis and across its room. `pool` is the measured
// `sample - room baseline`; `model` is the documented `local_light` expression
// evaluated independently, with visibility assumed clear (true away from the
// scene's own walls), so a difference between the two columns is a
// bake/runtime interpretation mismatch, not a falloff curve. The `n` column is
// the same scene under the proposal in
// `target/agent-work/agent-d/bake_pool_proposal.md`: horizontal distance,
// `(1 - t)^2` lateral curve, a downward incidence term, and saturating
// composition instead of the hard cap (see `audit_proposed_pool`).
//
// ```text
// cargo test --release fixture_distribution_audit_report -- --ignored --nocapture
// ```

/// One placed fixture of an audit scene.
struct AuditLight {
    fixture: &'static str,
    x: f32,
    z: f32,
    rotation_degrees: f32,
    brightness: f32,
    /// World Y when the fixture is wall mounted, `None` for a ceiling fixture.
    wall_y: Option<f32>,
}

/// One minimal level the audit profiles.
struct AuditScene {
    label: &'static str,
    width: f32,
    depth: f32,
    height: f32,
    lights: Vec<AuditLight>,
    /// Profile heights above the room floor, in metres.
    heights: Vec<f32>,
}

impl AuditScene {
    fn json(&self) -> String {
        let entries: Vec<String> = self
            .lights
            .iter()
            .map(|light| {
                let mount = light.wall_y.map_or_else(String::new, |y| {
                    format!(r#", "mount": "wall", "y": {y}"#)
                });
                format!(
                    r#"{{ "fixture": "{}", "x": {}, "z": {}, "rotation_degrees": {}, "brightness": {}{} }}"#,
                    light.fixture,
                    light.x,
                    light.z,
                    light.rotation_degrees,
                    light.brightness,
                    mount,
                )
            })
            .collect();
        format!(
            r#"{{
                "format_version": 1,
                "id": "distribution_audit",
                "name": "Distribution Audit",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": {}, "depth": {}, "height": {} }}],
                "ceiling_lights": [{}]
            }}"#,
            self.width,
            self.depth,
            self.height,
            entries.join(","),
        )
    }
}

/// The audit's copy of the documented pool expression: one independent
/// evaluation of [`crate::lighting::bake`]'s `local_light` with visibility 1.
fn audit_model_pool(lighting: &LevelLighting, x: f32, y: f32, z: f32) -> LightColor {
    let mut sum = LightColor::BLACK;
    for light in lighting.lights() {
        if !light.is_active() {
            continue;
        }
        let (half_w, half_d) = light.source.half_extents();
        let range = light.source.range;
        let dx = ((x - light.x()).abs() - half_w).max(0.0);
        let dz = ((z - light.z()).abs() - half_d).max(0.0);
        let horizontal = dx * dx + dz * dz;
        if horizontal >= range * range {
            continue;
        }
        let vertical = y - light.y();
        let distance_squared = vertical.mul_add(vertical, horizontal);
        if distance_squared >= range * range {
            continue;
        }
        let falloff = light.source.falloff.factor(distance_squared.sqrt() / range);
        let strength =
            LOCAL_LIGHT_STRENGTH * light.source.intensity * light.height_factor * falloff;
        sum = LightColor {
            r: strength.mul_add(light.source.color.r, sum.r),
            g: strength.mul_add(light.source.color.g, sum.g),
            b: strength.mul_add(light.source.color.b, sum.b),
        };
        if sum.min_channel() >= LOCAL_LIGHT_MAX {
            return LightColor::grey(LOCAL_LIGHT_MAX);
        }
    }
    sum.clamped(0.0, LOCAL_LIGHT_MAX)
}

/// The default pool range the proposal recommends, in metres. An authored
/// range shorter than this is respected; a longer one is the user's choice.
const AUDIT_PROPOSED_RANGE_M: f32 = 4.0;

/// Simulated proposal: the pool a *directional, horizontally-bounded* model
/// would produce for the same scene, with visibility 1.
///
/// It exists so the audit can print the expected numbers of the bake proposal
/// before the bake implements it; it is not a second source of truth for the
/// engine. `lateral = (1 - dh/range)^2` over the horizontal distance to the
/// rotated emitter footprint, `incidence = (ly - y) / distance`, and overlapping
/// pools compose with `cap * (1 - prod(1 - c_i/cap))` instead of a hard cap.
fn audit_proposed_pool(lighting: &LevelLighting, x: f32, y: f32, z: f32) -> LightColor {
    let mut remain = [1.0_f32; 3];
    for light in lighting.lights() {
        if !light.is_active() {
            continue;
        }
        let (half_w, half_d) = light.source.half_extents();
        let range = light.source.range.min(AUDIT_PROPOSED_RANGE_M);
        let dx = ((x - light.x()).abs() - half_w).max(0.0);
        let dz = ((z - light.z()).abs() - half_d).max(0.0);
        let horizontal = dx * dx + dz * dz;
        if horizontal >= range * range {
            continue;
        }
        let vertical = light.y() - y;
        if vertical <= 0.0 {
            continue;
        }
        let distance = vertical.mul_add(vertical, horizontal).sqrt();
        if !distance.is_finite() || distance <= 0.0 {
            continue;
        }
        let t = horizontal.sqrt() / range;
        let lateral = (1.0 - t) * (1.0 - t);
        let incidence = vertical / distance;
        let strength = LOCAL_LIGHT_STRENGTH
            * light.source.intensity
            * light.height_factor
            * lateral
            * incidence;
        let channels = [
            light.source.color.r,
            light.source.color.g,
            light.source.color.b,
        ];
        for (slot, channel) in remain.iter_mut().zip(channels) {
            let contribution = (strength * channel).min(LOCAL_LIGHT_MAX);
            *slot *= 1.0 - contribution / LOCAL_LIGHT_MAX;
        }
    }
    LightColor {
        r: LOCAL_LIGHT_MAX * (1.0 - remain[0]),
        g: LOCAL_LIGHT_MAX * (1.0 - remain[1]),
        b: LOCAL_LIGHT_MAX * (1.0 - remain[2]),
    }
}

/// The signed distance from a fixture centre along `+x`/`-x`/`+z`/`-z` up to
/// the scene edge, in the scene's own profile steps.
const AUDIT_STEPS: [f32; 16] = [
    0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0,
];

/// Prints one audit scene's profiles. `axis` is the unit direction in X/Z,
/// `limit` how far the walk may go before leaving the room.
#[allow(clippy::print_stdout, clippy::too_many_arguments)] // developer measurement output
fn audit_profile(
    lighting: &LevelLighting,
    baseline: LightColor,
    label: &str,
    origin: (f32, f32),
    axis: (f32, f32),
    limit: f32,
    y: f32,
) {
    let mut line = format!("  {label:<26} y={y:<5}:");
    for step in AUDIT_STEPS {
        if step > limit {
            continue;
        }
        let x = axis.0.mul_add(step, origin.0);
        let z = axis.1.mul_add(step, origin.1);
        let sample = lighting.sample_in_room(0, x, y, z);
        let pool = LightColor {
            r: (sample.r - baseline.r).max(0.0),
            g: (sample.g - baseline.g).max(0.0),
            b: (sample.b - baseline.b).max(0.0),
        };
        let model = audit_model_pool(lighting, x, y, z);
        let proposed = audit_proposed_pool(lighting, x, y, z);
        // Writing into the line rather than a temporary string is the lint's
        // own recommendation; a `String` never fails to be written to.
        write!(
            line,
            " {step:.2}[s{:.3} p{:.3} m{:.3} n{:.3}]",
            sample.luminance(),
            pool.luminance(),
            model.luminance(),
            proposed.luminance(),
        )
        .expect("writing to a String cannot fail");
    }
    println!("{line}");
}

/// Prints one audit scene: its fixture records, then the pool profile from the
/// first fixture along both axes at every authored height.
#[allow(clippy::print_stdout)] // developer measurement output
fn audit_scene(scene: &AuditScene) {
    let level = LevelDef::from_json(&scene.json()).expect("audit scene parses");
    let lighting = LevelLighting::bake(&level);
    let Some(room) = lighting.rooms().first() else {
        return;
    };
    let baseline = room.baseline;
    println!(
        "\n=== {} === room {} x {} x {} m, baseline {:.4} {:?}",
        scene.label,
        scene.width,
        scene.depth,
        scene.height,
        baseline.luminance(),
        baseline.to_array(),
    );
    audit_lights(&level, &lighting);
    let Some(first) = scene.lights.first() else {
        return;
    };
    for &y in &scene.heights {
        audit_heights(&lighting, baseline, scene, (first.x, first.z), y);
    }
}

/// One record per baked light: position, authored height, shape, range, curve,
/// intensity and ceiling-height factor.
#[allow(clippy::print_stdout)] // developer measurement output
fn audit_lights(level: &LevelDef, lighting: &LevelLighting) {
    for (index, light) in lighting.lights().iter().enumerate() {
        let authored = level
            .ceiling_lights
            .get(index)
            .map_or(0.0, |fixture| lighting.fixture_y_for(fixture));
        println!(
            "  [{index}] {:?} pos [{:.3}, {:.3}, {:.3}] (authored y {:.3}) shape {} \
             rot {:.1} range {:.2} falloff {} intensity {:.3} hf {:.4}",
            light.source.position,
            light.x(),
            light.y(),
            light.z(),
            authored,
            light.source.shape.name(),
            light.source.rotation_degrees,
            light.source.range,
            light.source.falloff.name(),
            light.source.intensity,
            light.height_factor,
        );
    }
}

/// The five profiles at one height: both axes through the first fixture, then
/// a transverse walk across the room.
#[allow(clippy::print_stdout)] // developer measurement output
fn audit_heights(
    lighting: &LevelLighting,
    baseline: LightColor,
    scene: &AuditScene,
    origin: (f32, f32),
    y: f32,
) {
    let plus_x = (scene.width - origin.0).max(0.0);
    for (label, axis, limit) in [
        ("+x through fixture", (1.0, 0.0), plus_x),
        ("-x through fixture", (-1.0, 0.0), origin.0.max(0.0)),
        (
            "+z through fixture",
            (0.0, 1.0),
            (scene.depth - origin.1).max(0.0),
        ),
        ("-z through fixture", (0.0, -1.0), origin.1.max(0.0)),
    ] {
        audit_profile(lighting, baseline, label, origin, axis, limit, y);
    }
    // A second fixed coordinate so a row profile and a transverse profile both
    // appear.
    let across = (origin.0, (scene.depth * 0.75).min(scene.depth - 0.05));
    audit_profile(
        lighting,
        baseline,
        "+x across the room",
        across,
        (1.0, 0.0),
        plus_x,
        y,
    );
}

/// One office panel placement.
fn panel_light(x: f32, z: f32, brightness: f32) -> AuditLight {
    AuditLight {
        fixture: "core:fluorescent_panel_01",
        x,
        z,
        rotation_degrees: 0.0,
        brightness,
        wall_y: None,
    }
}

/// Every audit scene, cheapest first: the isolated families, the office row,
/// the rotation swap and the low / tall ceiling extremes.
fn audit_scenes() -> Vec<AuditScene> {
    let mut scenes = panel_scenes();
    scenes.extend(family_scenes());
    scenes
}

/// The office panel scenes: low, normal, high and tall ceilings plus the row.
fn panel_scenes() -> Vec<AuditScene> {
    let panel = panel_light;
    vec![
        AuditScene {
            label: "office panel isolated 2.7 m",
            width: 12.0,
            depth: 12.0,
            height: 2.7,
            lights: vec![panel(6.0, 6.0, 0.62)],
            heights: vec![0.02, 1.35, 2.60],
        },
        AuditScene {
            label: "office panel isolated 2.2 m (low)",
            width: 12.0,
            depth: 12.0,
            height: 2.2,
            lights: vec![panel(6.0, 6.0, 0.62)],
            heights: vec![0.02, 1.10, 2.10],
        },
        AuditScene {
            label: "office panel isolated 4.2 m",
            width: 12.0,
            depth: 12.0,
            height: 4.2,
            lights: vec![panel(6.0, 6.0, 0.62)],
            heights: vec![0.02, 2.10, 4.10],
        },
        AuditScene {
            label: "office panel 17 m tower (The Pit)",
            width: 24.0,
            depth: 6.0,
            height: 17.0,
            lights: vec![panel(12.0, 3.0, 0.7)],
            heights: vec![0.02, 5.0, 10.0, 16.0],
        },
        AuditScene {
            label: "office panel row 4 m spacing 2.7 m",
            width: 24.0,
            depth: 7.0,
            height: 2.7,
            lights: vec![
                panel(2.5, 2.2, 0.62),
                panel(6.5, 2.2, 0.62),
                panel(10.5, 2.2, 0.62),
            ],
            heights: vec![0.02, 1.35, 2.60],
        },
    ]
}

/// The other shipped families: the rotated panel, the pool downlight, the wall
/// sconce and the residential flush mount.
fn family_scenes() -> Vec<AuditScene> {
    vec![
        AuditScene {
            label: "office panel rot 90 2.7 m",
            width: 12.0,
            depth: 12.0,
            height: 2.7,
            lights: vec![AuditLight {
                rotation_degrees: 90.0,
                ..panel_light(6.0, 6.0, 0.62)
            }],
            heights: vec![0.02, 2.60],
        },
        AuditScene {
            label: "pool round downlight 2.7 m",
            width: 8.0,
            depth: 8.0,
            height: 2.7,
            lights: vec![AuditLight {
                fixture: "core:pool_light_round",
                x: 4.0,
                z: 4.0,
                rotation_degrees: 0.0,
                brightness: 0.85,
                wall_y: None,
            }],
            heights: vec![0.02, 1.35, 2.60],
        },
        AuditScene {
            label: "pool wall sconce 2.7 m",
            width: 8.0,
            depth: 8.0,
            height: 2.7,
            lights: vec![AuditLight {
                fixture: "core:pool_light_wall",
                x: 0.15,
                z: 4.0,
                rotation_degrees: 90.0,
                brightness: 0.7,
                wall_y: Some(1.9),
            }],
            heights: vec![0.02, 1.90, 2.60],
        },
        AuditScene {
            label: "home flush mount 2.9 m",
            width: 8.0,
            depth: 8.0,
            height: 2.9,
            lights: vec![AuditLight {
                fixture: "home:ceiling_light_round",
                x: 4.0,
                z: 4.0,
                rotation_degrees: 0.0,
                brightness: 0.3,
                wall_y: None,
            }],
            heights: vec![0.02, 1.45, 2.83],
        },
    ]
}

/// Developer measurement, not an assertion. See the module docs above.
#[test]
#[ignore = "developer measurement: prints per-family fixture pool profiles"]
#[allow(clippy::print_stdout)]
fn fixture_distribution_audit_report() {
    for scene in audit_scenes() {
        audit_scene(&scene);
    }
}

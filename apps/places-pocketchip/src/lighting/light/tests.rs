//! Unit tests for the engine-level light source.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic
)]

use super::*;
use crate::lighting::math::smooth_falloff;
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

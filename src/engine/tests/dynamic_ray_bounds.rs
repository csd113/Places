//! Essential broad-phase safety: reject clear diagonal misses but retain
//! boundaries, parallel/tiny segments and every hit of two synthetic triangles
//! at three scales/two translations. Scalar queries cost milliseconds; no
//! assets, map bake or GPU allocation. Existing triangle/alpha order is untouched.

use super::{VisibilitySegment, placement_rounding_scale, segment_bounds_overlap};
use crate::engine::lighting::transport::{TransportScene, TransportTriangle};
use crate::engine::spatial::Aabb;
use glam::{Mat4, Vec3};

#[test]
fn movable_segment_guard_keeps_boundaries_and_rejects_clear_diagonal_misses() {
    let missed = Aabb {
        min: [1.0, 8.0, -1.0],
        max: [2.0, 9.0, 1.0],
    };
    assert!(segment_bounds_overlap([0.0; 3], [10.0, 10.0, 0.0], missed));
    assert!(!VisibilitySegment::new([0.0; 3], [10.0, 10.0, 0.0]).overlaps(missed, 0.0));
    for (from, to, bounds) in [
        (
            [0.0; 3],
            [10.0, 10.0, 0.0],
            Aabb::from_point([5.0, 5.0, 0.0]),
        ),
        ([1.0; 3], [1.0; 3], Aabb::from_point([1.0; 3])),
        (
            [0.0, 1.0, 0.0],
            [2.0, 1.0, 0.0],
            Aabb::from_point([1.0, 1.0, 0.0]),
        ),
        ([1.0e-30; 3], [2.0e-30; 3], Aabb::from_point([1.5e-30; 3])),
        (
            [100_000.0, 0.0, 0.0],
            [100_016.0, 0.0, 0.0],
            Aabb::from_point([100_008.0, 0.0, 0.0]),
        ),
    ] {
        assert!(VisibilitySegment::new(from, to).overlaps(bounds, 0.0));
        assert!(VisibilitySegment::new(to, from).overlaps(bounds, 0.0));
    }
}

#[test]
fn movable_segment_guard_preserves_actual_triangle_answers_across_scales() -> Result<(), String> {
    let mut rejected = 0_usize;
    let mut hits = 0_usize;
    for scale in [0.01_f32, 1.0, 1000.0] {
        for offset in [Vec3::ZERO, Vec3::splat(10_000.0)] {
            let transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                glam::Quat::IDENTITY,
                offset,
            );
            let point = |p| transform.transform_point3(Vec3::from_array(p)).to_array();
            let corners = [
                [0.0, -1.0, -1.0],
                [0.0, 1.0, -1.0],
                [0.0, 1.0, 1.0],
                [0.0, -1.0, 1.0],
            ]
            .map(point);
            let [a, b, c, d] = corners;
            let scene = TransportScene::new(
                vec![
                    TransportTriangle::new(a, b, c, [1.0; 3]).ok_or("invalid triangle")?,
                    TransportTriangle::new(a, c, d, [1.0; 3]).ok_or("invalid triangle")?,
                ],
                Vec::new(),
            )
            .ok_or("invalid tiny scene")?;
            let mut bounds = Aabb::EMPTY;
            for corner in corners {
                bounds.expand(corner);
            }
            for y in -5_i16..=5 {
                for z in -5_i16..=5 {
                    let from = point([-2.0, f32::from(y), f32::from(z)]);
                    let to = point([2.0, 0.5, -0.5]);
                    let expected = scene.transmittance(from, to);
                    if expected < 1.0 {
                        hits = hits.saturating_add(1);
                    }
                    if segment_bounds_overlap(from, to, bounds)
                        && !VisibilitySegment::new(from, to).overlaps(bounds, 0.0)
                    {
                        rejected = rejected.saturating_add(1);
                        assert_eq!(
                            expected.to_bits(),
                            1.0_f32.to_bits(),
                            "a rejected candidate cannot block the original triangle query"
                        );
                    }
                }
            }
        }
    }
    assert!(
        hits > 0 && rejected > 0,
        "the fixture must exercise real hits and useful rejection"
    );
    Ok(())
}

// A canceled imported pivot can hide the magnitude of f32 placement work.
// Two triangles and 25 rays protect that distinct precision boundary in <1ms.
#[test]
fn movable_segment_guard_accounts_for_a_large_canceled_model_pivot() -> Result<(), String> {
    let center = 1_000_000.0_f32;
    let scale = 0.01_f32;
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        glam::Quat::IDENTITY,
        Vec3::new(-center * scale, 0.0, 0.0),
    );
    let local = Aabb {
        min: [center, -1.0, -1.0],
        max: [center, 1.0, 1.0],
    };
    let allowance = placement_rounding_scale(transform, local);
    assert!(
        allowance > 19_999.0_f64,
        "cancellation must not erase the placement magnitude"
    );
    let [a, b, c, d] = [
        [center, -1.0, -1.0],
        [center, 1.0, -1.0],
        [center, 1.0, 1.0],
        [center, -1.0, 1.0],
    ];
    let scene = TransportScene::new(
        vec![
            TransportTriangle::new(a, b, c, [1.0; 3]).ok_or("invalid triangle")?,
            TransportTriangle::new(a, c, d, [1.0; 3]).ok_or("invalid triangle")?,
        ],
        Vec::new(),
    )
    .ok_or("invalid scene")?;
    let mut bounds = Aabb::EMPTY;
    for p in [a, b, c, d] {
        bounds.expand(transform.transform_point3(Vec3::from_array(p)).to_array());
    }
    let inverse = transform.inverse();
    for y in -2_i16..=2 {
        for z in -2_i16..=2 {
            let from = Vec3::new(-0.02, f32::from(y) * scale, f32::from(z) * scale);
            let to = Vec3::new(0.02, 0.0, 0.0);
            let expected = scene.transmittance(
                inverse.transform_point3(from).to_array(),
                inverse.transform_point3(to).to_array(),
            );
            if expected < 1.0 {
                assert!(
                    VisibilitySegment::new(from.to_array(), to.to_array())
                        .overlaps(bounds, allowance),
                    "every model-local hit must reach the unchanged query"
                );
            }
        }
    }
    Ok(())
}

// Protect the water-field broad phase against false clear certificates. This
// compares a few thousand scalar boxes with the existing full SAT predicate;
// no map, model, bake, GPU or random fixture is needed.
#[test]
fn light_region_envelope_keeps_every_full_certificate_overlap() {
    use super::light_region_bounds_overlap;
    use crate::engine::spatial::finite_light_region_overlaps;

    let mut rejected = 0_usize;
    let mut retained = 0_usize;
    for scale in [0.000_01_f32, 1.0, 1000.0] {
        for offset in [0.0_f32, 100_000.0] {
            let bounds = |low: [f32; 3], high: [f32; 3]| Aabb {
                min: low.map(|v| v.mul_add(scale, offset)),
                max: high.map(|v| v.mul_add(scale, offset)),
            };
            let source = bounds([-0.5, 3.0, -0.5], [0.5, 3.0, 0.5]);
            let receivers = bounds([-2.0, -1.0, -2.0], [2.0, 0.0, 2.0]);
            for x in -4_i16..=4 {
                for y in -3_i16..=4 {
                    for z in -4_i16..=4 {
                        let center = [f32::from(x), f32::from(y), f32::from(z)];
                        let blocker = bounds(center.map(|v| v - 0.01), center.map(|v| v + 0.01));
                        let coarse = light_region_bounds_overlap(source.union(&receivers), blocker);
                        if coarse {
                            retained = retained.saturating_add(1);
                        } else {
                            rejected = rejected.saturating_add(1);
                            assert!(!finite_light_region_overlaps(source, receivers, blocker));
                        }
                    }
                }
            }
        }
    }
    assert!(rejected > 0 && retained > 0);
}

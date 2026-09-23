//! Unit tests for the spatial grid, frustum and mesh buckets.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::indexing_slicing
)]

use super::*;
use crate::test_support::{assert_exact, assert_exact_array};
use glam::{Mat4, Vec3};

fn bounds(min: [f32; 3], max: [f32; 3]) -> Aabb {
    Aabb { min, max }
}

fn view_projection(eye: Vec3, look_at: Vec3, fov_degrees: f32) -> Mat4 {
    view_projection_up(eye, look_at, fov_degrees, Vec3::Y)
}

/// `look_at_rh` needs an `up` that is not parallel to the view direction, so
/// the straight-down/straight-up cases pass an explicit axis.
fn view_projection_up(eye: Vec3, look_at: Vec3, fov_degrees: f32, up: Vec3) -> Mat4 {
    let aspect = 480.0 / 272.0;
    let proj = Mat4::perspective_rh(fov_degrees.to_radians(), aspect, 0.1, 100.0);
    let view = Mat4::look_at_rh(eye, look_at, up);
    proj * view
}

#[test]
fn empty_bounds_are_empty_and_never_grow_from_non_finite_input() {
    // An all-non-finite box stays empty rather than becoming infinite.
    let mut empty = Aabb::EMPTY;
    assert!(empty.is_empty());
    empty.expand([f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
    assert!(empty.is_empty(), "no finite component was supplied");

    // Non-finite components are ignored; the finite remainder still counts,
    // because dropping it could only make the bounds too small.
    let mut partial = Aabb::EMPTY;
    partial.expand([f32::NAN, 5.0, f32::INFINITY]);
    partial.expand([2.0, f32::NAN, 3.0]);
    assert!(!partial.is_empty());
    assert_exact(partial.min[0], 2.0);
    assert_exact(partial.min[1], 5.0);
    assert_exact(partial.min[2], 3.0);
    assert_exact(partial.max[1], 5.0);
    assert_exact(partial.max[2], 3.0);

    // An all-NaN box stays useless rather than becoming infinite.
    let mut poisoned = Aabb::EMPTY;
    poisoned.expand([f32::NAN, f32::NAN, f32::NAN]);
    assert!(poisoned.is_empty());
}

#[test]
fn bounds_union_and_centre() {
    let a = Aabb::from_point([0.0, 0.0, 0.0]);
    let b = Aabb::from_point([2.0, 4.0, 6.0]);
    let union = a.union(&b);
    assert_exact_array(union.min, [0.0, 0.0, 0.0]);
    assert_exact_array(union.max, [2.0, 4.0, 6.0]);
    assert_exact_array(union.centre(), [1.0, 2.0, 3.0]);
}

#[test]
fn cells_are_stable_for_negative_and_extreme_coordinates() {
    let cell = 12.0;
    assert_eq!(cell_of([0.0, 0.0, 0.0], cell), CellKey { x: 0, z: 0 });
    assert_eq!(cell_of([11.9, 0.0, -0.1], cell), CellKey { x: 0, z: -1 });
    assert_eq!(cell_of([-0.1, 0.0, 0.0], cell), CellKey { x: -1, z: 0 });
    assert_eq!(cell_of([-12.0, 5.0, -24.0], cell), CellKey { x: -1, z: -2 });
    // Extreme but valid level coordinates must clamp rather than wrap.
    let extreme = cell_of([1.0e30, 0.0, -1.0e30], cell);
    assert_eq!(extreme.x, CELL_LIMIT);
    assert_eq!(extreme.z, -CELL_LIMIT);
    // Degenerate grid parameters fall back to the origin cell.
    assert_eq!(cell_of([5.0, 0.0, 5.0], 0.0), CellKey { x: 0, z: 0 });
    assert_eq!(cell_of([5.0, 0.0, 5.0], -1.0), CellKey { x: 0, z: 0 });
    assert_eq!(cell_of([f32::NAN, 0.0, 0.0], cell), CellKey { x: 0, z: 0 });
}

#[test]
fn a_box_the_camera_is_inside_is_never_culled() {
    let frustum = Frustum::from_view_projection(
        &view_projection(Vec3::ZERO, -Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );
    // The camera sits inside this box; every plane must accept it.
    assert!(frustum.intersects_aabb(&bounds([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])));

    let wrong = Frustum::from_view_projection(
        &view_projection(Vec3::ZERO, -Vec3::Z, 60.0),
        DepthRange::NegativeOneToOne,
    );
    // (The wrong depth convention is still conservative here, which is the
    // property that matters: it must never cull this box either.)
    assert!(wrong.intersects_aabb(&bounds([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])));
}

#[test]
fn a_box_straight_ahead_is_visible_and_the_same_box_behind_is_culled() {
    // Yaw 0 looks along -Z, matching `render_scene`'s forward vector.
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let frustum = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );

    assert!(
        frustum.intersects_aabb(&bounds([-1.0, 0.0, -12.0], [1.0, 2.0, -10.0])),
        "a box ten metres ahead must be visible"
    );
    assert!(
        !frustum.intersects_aabb(&bounds([-1.0, 0.0, 10.0], [1.0, 2.0, 12.0])),
        "the same box behind the camera must be culled"
    );
}

#[test]
fn a_box_far_outside_the_lateral_planes_is_culled() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let frustum = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );
    // Sixty metres to the side while ten metres ahead: outside any 60-degree
    // vertical / ~90-degree horizontal view.
    assert!(!frustum.intersects_aabb(&bounds([59.0, 0.0, -11.0], [61.0, 2.0, -9.0])));
    // Two metres to the side while ten metres ahead: comfortably inside.
    assert!(frustum.intersects_aabb(&bounds([1.9, 0.0, -11.0], [2.1, 2.0, -9.0])));
}

#[test]
fn a_huge_box_crossing_the_frustum_is_never_culled() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let frustum = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );
    // A level-sized batch that spans the camera must survive every plane.
    assert!(frustum.intersects_aabb(&bounds([-500.0, -50.0, -500.0], [500.0, 50.0, 500.0])));
}

#[test]
fn a_box_touching_a_plane_is_kept() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let frustum = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );
    // Exactly on the far plane: `intersects_aabb` is conservative, so
    // "touching" must count as visible rather than being dropped.
    let on_far = bounds([-1.0, 0.0, -100.0], [1.0, 2.0, -100.0]);
    assert!(frustum.intersects_aabb(&on_far));
    // Just past the far plane is definitively outside.
    assert!(!frustum.intersects_aabb(&bounds([-1.0, 0.0, -140.0], [1.0, 2.0, -140.0])));
}

#[test]
fn pitch_and_yaw_rotate_the_visible_set() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    // Looking straight down: a box below the camera is visible, one ahead is not.
    let down = Frustum::from_view_projection(
        &view_projection_up(eye, eye - Vec3::Y, 60.0, Vec3::Z),
        DepthRange::ZeroToOne,
    );
    assert!(down.intersects_aabb(&bounds([-1.0, -6.0, -1.0], [1.0, -4.0, 1.0])));
    assert!(!down.intersects_aabb(&bounds([-1.0, -1.0, -6.0], [1.0, 1.0, -4.0])));

    // Looking straight up: the reverse.
    let up = Frustum::from_view_projection(
        &view_projection_up(eye, eye + Vec3::Y, 60.0, Vec3::Z),
        DepthRange::ZeroToOne,
    );
    assert!(up.intersects_aabb(&bounds([-1.0, 4.0, -1.0], [1.0, 6.0, 1.0])));
    assert!(!up.intersects_aabb(&bounds([-1.0, -6.0, -1.0], [1.0, -4.0, 1.0])));

    // Yawed 90 degrees: -X becomes "ahead".
    let yawed = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::X, 60.0),
        DepthRange::ZeroToOne,
    );
    assert!(yawed.intersects_aabb(&bounds([-12.0, 0.0, -1.0], [-10.0, 2.0, 1.0])));
    assert!(!yawed.intersects_aabb(&bounds([10.0, 0.0, -1.0], [12.0, 2.0, 1.0])));
}

#[test]
fn a_zero_area_box_is_treated_as_invisible() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let frustum = Frustum::from_view_projection(
        &view_projection(eye, eye - Vec3::Z, 60.0),
        DepthRange::ZeroToOne,
    );
    assert!(!frustum.intersects_aabb(&Aabb::EMPTY));
    // A single-point box is non-empty and is judged normally.
    assert!(frustum.intersects_aabb(&Aabb::from_point([0.0, 1.6, -5.0])));
}

/// A quad of four distinct corners, written the way the emitters write it:
/// a triangle list of six vertices with the shared corners repeated.
fn quad(x0: f32, z0: f32, size: f32) -> Vec<crate::render::Vertex> {
    let vertex = |x: f32, z: f32| crate::render::Vertex {
        pos: [x, 0.0, z],
        color: [1.0, 1.0, 1.0, 1.0],
        uv: [x, z],
        ..crate::render::Vertex::UNLIT
    };
    let (x1, z1) = (x0 + size, z0 + size);
    vec![
        vertex(x0, z0),
        vertex(x1, z0),
        vertex(x1, z1),
        vertex(x0, z0),
        vertex(x1, z1),
        vertex(x0, z1),
    ]
}

#[test]
fn quads_are_routed_to_their_own_cells_and_keep_every_vertex() {
    let mut run = quad(1.0, 1.0, 1.0);
    run.extend(quad(3.0, 3.0, 1.0));
    run.extend(quad(13.0, 1.0, 1.0));

    let mut buckets = SpatialBuckets::<()>::new(12.0);
    buckets.add_quads((), &run);
    assert_eq!(buckets.cell_count(), 2);

    let drained = buckets.drain_sorted();
    assert_eq!(drained.len(), 2);
    // Sorted by cell key: (0,0) before (1,0).
    assert_eq!(drained[0].0.1, CellKey { x: 0, z: 0 });
    assert_eq!(drained[0].1.len(), 12, "two quads in the origin cell");
    assert_eq!(drained[1].0.1, CellKey { x: 1, z: 0 });
    assert_eq!(drained[1].1.len(), 6, "one quad in the neighbouring cell");

    let total: usize = drained.iter().map(|(_, vertices)| vertices.len()).sum();
    assert_eq!(total, run.len(), "no vertex may be dropped");
}

#[test]
fn indexing_turns_six_vertices_per_quad_into_four() {
    let mut buckets = SpatialBuckets::<()>::new(12.0);
    buckets.add_quads((), &quad(1.0, 1.0, 1.0));
    // A second quad sharing the first quad's far edge: its near corners are
    // bit-identical, so indexing must reuse them.
    buckets.add_quads((), &quad(2.0, 1.0, 1.0));

    let drained = buckets.drain_indexed();
    assert_eq!(drained.len(), 1);
    let range = &drained[0].1;
    assert_eq!(range.indices.len(), 12, "two quads is twelve indices");
    assert_eq!(
        range.vertices.len(),
        6,
        "a shared edge collapses two quads to six distinct vertices"
    );
    // Every vertex must actually be referenced.
    let mut used = vec![false; range.vertices.len()];
    for index in &range.indices {
        used[*index as usize] = true;
    }
    assert!(used.iter().all(|seen| *seen), "no orphan vertices");
    assert!(!range.bounds.is_empty());
}

#[test]
fn indexing_keeps_triangles_in_the_same_winding() {
    let mut buckets = SpatialBuckets::<()>::new(12.0);
    buckets.add_quads((), &quad(0.0, 0.0, 1.0));
    let drained = buckets.drain_indexed();
    let range = &drained[0].1;
    let pos = |index: u16| range.vertices[index as usize].pos;
    assert_exact_array(pos(range.indices[0]), [0.0, 0.0, 0.0]);
    assert_exact_array(pos(range.indices[1]), [1.0, 0.0, 0.0]);
    assert_exact_array(pos(range.indices[2]), [1.0, 0.0, 1.0]);
    assert_exact_array(pos(range.indices[3]), [0.0, 0.0, 0.0]);
    assert_exact_array(pos(range.indices[4]), [1.0, 0.0, 1.0]);
    assert_exact_array(pos(range.indices[5]), [0.0, 0.0, 1.0]);
}

#[test]
fn indexing_never_merges_vertices_that_differ_in_any_attribute() {
    // Two quads sharing an edge. Indexed submission may only collapse the
    // shared corners when every attribute matches bit for bit; a difference
    // in UV, baked light or position is a different render vertex and must
    // stay separate.
    type Corner = ([f32; 3], [f32; 2]);

    fn emit(
        corners: &[Corner; 4],
        adjust: &dyn Fn(Corner) -> Corner,
        out: &mut Vec<crate::render::Vertex>,
    ) {
        // Written the way the emitters write a quad: (p0,p1,p2, p0,p2,p3).
        for slot in [0usize, 1, 2, 0, 2, 3] {
            let (pos, uv) = adjust(corners[slot]);
            out.push(crate::render::Vertex {
                pos,
                color: [0.5, 1.0, 1.0, 1.0],
                uv,
                ..crate::render::Vertex::UNLIT
            });
        }
    }

    let left: [Corner; 4] = [
        ([0.0, 0.0, 0.0], [0.0, 0.0]),
        ([1.0, 0.0, 0.0], [1.0, 0.0]),
        ([1.0, 0.0, 1.0], [1.0, 1.0]),
        ([0.0, 0.0, 1.0], [0.0, 1.0]),
    ];
    let right: [Corner; 4] = [
        ([1.0, 0.0, 0.0], [1.0, 0.0]),
        ([2.0, 0.0, 0.0], [2.0, 0.0]),
        ([2.0, 0.0, 1.0], [2.0, 1.0]),
        ([1.0, 0.0, 1.0], [1.0, 1.0]),
    ];
    let build = |adjust: &dyn Fn(Corner) -> Corner| {
        let mut out = Vec::new();
        emit(&left, &|corner| corner, &mut out);
        emit(&right, adjust, &mut out);
        out
    };
    let identity = |corner: Corner| corner;

    let identical = index_run(&build(&identity));
    assert_eq!(identical.len(), 1);
    assert_eq!(identical[0].vertices.len(), 6, "shared corners collapse");
    assert_eq!(identical[0].indices.len(), 12);

    // Nudge the right quad's UVs: one extra vertex per differing corner.
    let shifted_uv = index_run(&build(&|(pos, uv): Corner| (pos, [uv[0] + 0.25, uv[1]])));
    assert_eq!(shifted_uv.len(), 1);
    assert_eq!(
        shifted_uv[0].vertices.len(),
        8,
        "a UV difference on a shared edge must not merge"
    );
    assert_eq!(shifted_uv[0].indices.len(), 12);

    // Nudge the right quad's positions, which is how a baked-lighting
    // difference between two rooms shows up.
    let shifted_pos = index_run(&build(&|(pos, uv): Corner| {
        ([pos[0], pos[1] + 0.001, pos[2]], uv)
    }));
    assert_eq!(
        shifted_pos[0].vertices.len(),
        8,
        "a position difference must not merge"
    );
    assert_eq!(shifted_pos[0].indices.len(), 12);
}

#[test]
fn indexing_never_merges_vertices_that_differ_in_baked_lighting() {
    // Two quads sharing an edge where the right quad is lit differently.
    // Baked light lives in the colour channels, so a difference there is a
    // difference in an attribute and the shared corners must stay separate.
    // (This renderer has no per-vertex normal: the baked colour is the only
    // shading attribute, and it is compared exactly like every other one.)
    // Two quads sharing the edge x = 1: four corners each, six distinct.
    let mut run = Vec::new();
    let push = |light: f32, left: bool, out: &mut Vec<crate::render::Vertex>| {
        let corners = if left {
            [(0.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        } else {
            [(1.0f32, 0.0f32), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0)]
        };
        for slot in [0usize, 1, 2, 0, 2, 3] {
            let (x, z) = corners[slot];
            out.push(crate::render::Vertex {
                pos: [x, 0.0, z],
                color: [light, light, light, 1.0],
                uv: [x, z],
                ..crate::render::Vertex::UNLIT
            });
        }
    };
    push(0.8, true, &mut run);
    push(0.8, false, &mut run);
    let same = index_run(&run);
    assert_eq!(
        same[0].vertices.len(),
        6,
        "identical lighting shares corners"
    );

    let mut mixed = Vec::new();
    push(0.8, true, &mut mixed);
    push(0.6, false, &mut mixed);
    let different = index_run(&mixed);
    assert_eq!(
        different[0].vertices.len(),
        8,
        "a baked-lighting difference on a shared edge must not merge"
    );
    assert_eq!(different[0].indices.len(), 12);
}

#[test]
fn a_range_is_split_before_it_overflows_16_bit_indices() {
    let mut run: Vec<crate::render::Vertex> = Vec::new();
    let count = MAX_INDEX_VERTICES / 4 + 8;
    for index in 0..count {
        run.extend(quad(index as f32, 0.0, 0.5));
    }
    let ranges = index_run(&run);
    assert!(
        ranges.len() >= 2,
        "an oversized run must split, got {} range(s)",
        ranges.len()
    );
    let mut total_indices = 0usize;
    for range in &ranges {
        assert!(
            range.vertices.len() <= MAX_INDEX_VERTICES,
            "range has {} vertices",
            range.vertices.len()
        );
        assert!(
            range
                .indices
                .iter()
                .all(|index| (*index as usize) < range.vertices.len()),
            "an index pointed outside its own range"
        );
        total_indices += range.indices.len();
    }
    assert_eq!(total_indices, run.len(), "no triangle may be lost");
}

#[test]
fn indexed_draining_is_deterministic() {
    let build = || {
        let mut buckets = SpatialBuckets::<()>::new(12.0);
        for index in 0..8 {
            buckets.add_quads((), &quad(index as f32 * 0.5, 0.0, 0.5));
        }
        buckets.add_quads((), &quad(20.0, 0.0, 0.5));
        buckets.drain_indexed()
    };
    let first = build();
    let second = build();
    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1.vertices, b.1.vertices);
        assert_eq!(a.1.indices, b.1.indices);
        assert_eq!(a.1.bounds, b.1.bounds);
    }
}

#[test]
fn add_run_keeps_a_prop_box_in_one_piece_across_a_cell_boundary() {
    let mut straddling = Vec::new();
    for offset in [-0.5f32, 0.5] {
        straddling.extend(quad(1.0, offset, 0.25));
    }
    let mut split = SpatialBuckets::<()>::new(12.0);
    split.add_quads((), &straddling);
    assert_eq!(split.drain_indexed().len(), 2, "per-quad routing splits it");

    let mut whole = SpatialBuckets::<()>::new(12.0);
    whole.add_run((), &straddling);
    assert_eq!(whole.drain_indexed().len(), 1, "whole-run routing keeps it");
}

#[test]
fn the_adaptive_grid_keeps_the_cell_count_bounded() {
    // Small levels keep the finest granularity...
    let small = CellGrid::for_extent(20.0, 20.0);
    assert_exact(small.x, SPATIAL_CELL_MIN_METRES);
    assert_exact(small.z, SPATIAL_CELL_MIN_METRES);

    // ...the grid grows with the level so the cell count stays near the
    // target, and never exceeds the maximum cell size...
    let big = CellGrid::for_extent(264.0, 262.0);
    assert!((big.x - 33.0).abs() < 0.01, "got {}", big.x);
    assert!((big.z - 32.75).abs() < 0.01, "got {}", big.z);
    assert!(264.0 / big.x <= SPATIAL_CELL_TARGET_PER_AXIS + 0.5);
    assert!(264.0 / big.x >= 4.0);

    // ...and an absurd extent is capped rather than becoming one cell.
    let huge = CellGrid::for_extent(1.0e9, 1.0e9);
    assert_exact(huge.x, SPATIAL_CELL_MAX_METRES);

    // A long, thin corridor gets per-axis sizing.
    let corridor = CellGrid::for_extent(200.0, 20.0);
    assert!(corridor.x > corridor.z, "{corridor:?}");
    assert_exact(corridor.z, SPATIAL_CELL_MIN_METRES);

    // Degenerate extents are treated as small.
    assert_exact(
        CellGrid::for_extent(0.0, f32::NAN).x,
        SPATIAL_CELL_MIN_METRES,
    );
}

#[test]
fn the_explicit_override_is_not_clamped_to_the_adaptive_range() {
    // The sweep must be able to go below the adaptive minimum and above the
    // adaptive maximum; `uniform` is a guard, not a policy.
    assert_exact(CellGrid::uniform(4.0).x, 4.0);
    assert_exact(CellGrid::uniform(96.0).x, 96.0);
    // Only genuinely unusable values fall back.
    assert_exact(CellGrid::uniform(0.0).x, SPATIAL_CELL_MIN_METRES);
    assert_exact(CellGrid::uniform(-3.0).x, SPATIAL_CELL_MIN_METRES);
    assert_exact(CellGrid::uniform(f32::NAN).x, SPATIAL_CELL_MIN_METRES);
}

#[test]
fn a_per_axis_grid_keys_each_axis_independently() {
    let grid = CellGrid { x: 20.0, z: 5.0 };
    assert_eq!(grid.cell_of([0.0, 0.0, 14.0]), CellKey { x: 0, z: 2 });
    assert_eq!(grid.cell_of([25.0, 0.0, -6.0]), CellKey { x: 1, z: -2 });
    assert_eq!(grid.cell_of([-0.5, 0.0, -0.5]), CellKey { x: -1, z: -1 });
}

#[test]
fn a_trailing_partial_quad_is_kept_rather_than_dropped() {
    let one = crate::render::Vertex {
        pos: [0.0, 0.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
        ..crate::render::Vertex::UNLIT
    };
    let mut buckets = SpatialBuckets::<()>::new(12.0);
    buckets.add_quads((), &[one; 3]);
    let drained = buckets.drain_sorted();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].1.len(), 3);

    // Indexing a run shorter than a quad keeps the data rather than
    // dropping it: one degenerate triangle over the three deduplicated to
    // a single vertex.
    let mut indexed = SpatialBuckets::<()>::new(12.0);
    indexed.add_quads((), &[one; 3]);
    let ranges = indexed.drain_indexed();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].1.vertices.len(), 1);
    assert_eq!(ranges[0].1.indices, vec![0, 0, 0]);
}

#[test]
fn cell_order_is_sorted_rather_than_insertion_order() {
    let vertex = |x: f32| crate::render::Vertex {
        pos: [x, 0.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
        ..crate::render::Vertex::UNLIT
    };
    let flat_quad = |x: f32| vec![vertex(x); 6];

    let mut forward = SpatialBuckets::<()>::new(12.0);
    for x in [0.0f32, 13.0, 26.0] {
        forward.add_quads((), &flat_quad(x));
    }
    let mut reverse = SpatialBuckets::<()>::new(12.0);
    for x in [26.0f32, 13.0, 0.0] {
        reverse.add_quads((), &flat_quad(x));
    }

    let a = forward.drain_indexed();
    let b = reverse.drain_indexed();
    let keys_a: Vec<CellKey> = a.iter().map(|(((), cell), _)| *cell).collect();
    let keys_b: Vec<CellKey> = b.iter().map(|(((), cell), _)| *cell).collect();
    assert_eq!(
        keys_a, keys_b,
        "cell order must be sorted, not insertion order"
    );
    assert_eq!(
        keys_a,
        vec![
            CellKey { x: 0, z: 0 },
            CellKey { x: 1, z: 0 },
            CellKey { x: 2, z: 0 },
        ]
    );
}

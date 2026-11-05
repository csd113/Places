//! Essential culling safety regressions: a false positive removes visible
//! content. These tiny triangle/box fixtures need no assets, bake or GPU.
use super::Occluders;
use crate::engine::spatial::Aabb;
use glam::{Mat4, Vec3};

fn wall() -> Occluders {
    Occluders::new(std::iter::once([
        [-4.0, -4.0, 2.0],
        [4.0, -4.0, 2.0],
        [0.0, 4.0, 2.0],
    ]))
}

#[test]
fn occlusion_requires_whole_box_strictly_behind_solid_triangle() {
    let view = wall().view(Vec3::ZERO, Mat4::IDENTITY);
    assert!(
        view.hides(&Aabb {
            min: [-0.2, -0.2, 4.0],
            max: [0.2, 0.2, 4.5]
        }),
        "fully covered box must be hidden"
    );
    for bounds in [
        Aabb {
            min: [-0.2, -0.2, 1.0],
            max: [0.2, 0.2, 1.5],
        },
        Aabb {
            min: [-0.2, -0.2, 1.9],
            max: [0.2, 0.2, 2.1],
        },
        Aabb {
            min: [-0.2, -0.2, 2.0],
            max: [0.2, 0.2, 2.0],
        },
        Aabb {
            min: [-0.2, -0.2, 4.0],
            max: [20.0, 0.2, 4.5],
        },
        Aabb::EMPTY,
        Aabb {
            min: [f32::NAN, 0.0, 4.0],
            max: [0.2, 0.2, 4.5],
        },
    ] {
        assert!(
            !view.hides(&bounds),
            "front, crossing, coplanar, protruding or invalid bounds must survive: {bounds:?}"
        );
    }
}

#[test]
fn occlusion_keeps_openings_and_tracks_current_eye_and_moving_bounds() {
    let walls = Occluders::new(
        [
            [[-4.0, -4.0, 2.0], [-1.0, -4.0, 2.0], [-1.0, 4.0, 2.0]],
            [[1.0, -4.0, 2.0], [4.0, -4.0, 2.0], [1.0, 4.0, 2.0]],
        ]
        .into_iter(),
    );
    let target = Aabb {
        min: [-0.1, -0.1, 4.0],
        max: [0.1, 0.1, 4.2],
    };
    assert!(
        !walls.view(Vec3::ZERO, Mat4::IDENTITY).hides(&target),
        "a doorway cannot be filled by inferred coverage"
    );
    assert!(
        wall().view(Vec3::ZERO, Mat4::IDENTITY).hides(&target),
        "control is occluded"
    );
    assert!(
        !wall()
            .view(Vec3::new(0.0, 0.0, 10.0), Mat4::IDENTITY)
            .hides(&target),
        "a reflected or moved camera must use its own eye"
    );
    assert!(
        !wall().view(Vec3::ZERO, Mat4::IDENTITY).hides(&Aabb {
            min: [-0.1, -0.1, 1.0],
            max: [0.1, 0.1, 1.2]
        }),
        "moving bounds cannot reuse the previous answer"
    );
}

#[test]
fn near_clipped_and_degenerate_occluders_never_remove_content() {
    let triangles = Occluders::new(
        [
            [[-4.0, -4.0, -0.1], [4.0, -4.0, 2.0], [0.0, 4.0, 2.0]],
            [[0.0, 0.0, 2.0]; 3],
            [[f32::INFINITY, 0.0, 2.0]; 3],
        ]
        .into_iter(),
    );
    assert!(
        !triangles.view(Vec3::ZERO, Mat4::IDENTITY).hides(&Aabb {
            min: [-0.1, -0.1, 4.0],
            max: [0.1, 0.1, 4.2]
        }),
        "clipped or invalid occluders cannot prove coverage"
    );
}

/// One triangle reproduces clipped backing occlusion; no GPU/assets/bake.
#[test]
fn clipped_or_crossing_capture_occluders_cannot_hide_retained_objects() {
    let occluders = wall();
    let target = Aabb {
        min: [-0.1, -0.1, 4.0],
        max: [0.1, 0.1, 4.2],
    };
    assert!(
        occluders.view(Vec3::ZERO, Mat4::IDENTITY).hides(&target),
        "ordinary control must still occlude"
    );
    assert!(
        !occluders
            .clipped_view(
                Vec3::ZERO,
                Mat4::IDENTITY,
                glam::Vec4::new(0.0, 0.0, 1.0, -3.0)
            )
            .hides(&target),
        "discarded backing cannot prove capture coverage"
    );
    assert!(
        !occluders
            .clipped_view(Vec3::ZERO, Mat4::IDENTITY, glam::Vec4::X)
            .hides(&target),
        "partially clipped triangles cannot prove full coverage"
    );
    assert!(
        occluders
            .clipped_view(
                Vec3::ZERO,
                Mat4::IDENTITY,
                glam::Vec4::new(0.0, 0.0, 1.0, -1.0)
            )
            .hides(&target),
        "wholly retained opaque triangles remain useful"
    );
}

/// Reproduces the Pool dry-gap blocker with four wall triangles and one wet
/// triangle. The wall crosses the near plane and its internal mesh seam.
/// A real opening must survive. CPU-only, no assets/bake, microseconds.
#[test]
fn water_coverage_joins_exact_wall_seams_but_keeps_real_openings() -> Result<(), String> {
    use crate::engine::render::{DrawableSize, RenderCamera};
    let camera = RenderCamera::new(
        Vec3::new(9.0, 1.6, 11.0),
        90.0_f32.to_radians(),
        -89.0_f32.to_radians(),
        60.0,
    );
    let frame = crate::engine::render::wgpu::world::prepare_world_frame(
        camera,
        DrawableSize::new(1280, 720),
    );
    let wall_sections = |gap: f32| {
        let mut result = Occluders::new(
            [(6.066, 8.512 - gap), (8.512 + gap, 10.958)]
                .into_iter()
                .flat_map(|(left, right)| {
                    [
                        [[left, 0.0, 10.85], [right, 0.0, 10.85], [right, 4.0, 10.85]],
                        [[left, 0.0, 10.85], [right, 4.0, 10.85], [left, 4.0, 10.85]],
                    ]
                }),
        );
        result.enable_water_quads();
        result
    };
    let wet = [
        Vec3::new(7.5, -0.15, 9.6),
        Vec3::new(7.978_354_5, -0.15, 10.754_849),
        Vec3::new(8.125, -0.15, 10.682_532),
    ];
    let matrix = frame.view_projection.transpose();
    let [x, y, z, w] = matrix.to_cols_array_2d().map(glam::Vec4::from_array);
    let clipped = super::clip_polygon(&wet, &[w + x, w - x, w + y, w - y, z, w - z])
        .ok_or("small finite polygon must fit clipping scratch")?;
    assert!(
        !clipped.points().is_empty(),
        "water actually projects on screen"
    );
    let solid = wall_sections(0.0).view(frame.eye, frame.view_projection);
    assert!(
        solid.hides_polygon(clipped.points()),
        "exact wall union covers the internal seam"
    );
    let bounds = wet
        .iter()
        .fold(Aabb::EMPTY, |value, point| value.expanded(point.to_array()));
    assert!(
        !solid.hides(&bounds),
        "ordinary whole-box near-plane policy stays unchanged"
    );
    let opening = wall_sections(0.08).view(frame.eye, frame.view_projection);
    assert!(
        !opening.hides_polygon(clipped.points()),
        "a real wall opening cannot be merged shut"
    );
    let moved = wall_sections(0.0).view(Vec3::new(9.0, 1.6, 10.0), frame.view_projection);
    assert!(
        !moved.hides_polygon(clipped.points()),
        "moving behind the wall invalidates its coverage"
    );
    Ok(())
}

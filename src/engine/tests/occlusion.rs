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

//! Three tiny triangles reproduce cloth/face BLEND ordering across primitives.
//! This uniquely catches the rear face-through defect, camera/pose invalidation
//! and material-run mistakes without assets, GPU or baking; runtime is sub-ms.
use super::{BlendTriangle, OrderedTriangle, order_triangles};
use glam::{Mat4, Vec3};

fn composite(order: &[OrderedTriangle], alpha: f32) -> f32 {
    order.iter().fold(0.0, |background, triangle| {
        let colour = if triangle.source == 2 { 0.0 } else { 1.0 };
        background.mul_add(1.0 - alpha, colour * alpha)
    })
}

#[test]
fn posed_blend_order_hides_rear_face_and_preserves_partial_layers_and_stable_ties() {
    let triangles = [
        BlendTriangle {
            indices: [0; 3],
            submesh: 0,
        },
        BlendTriangle {
            indices: [1; 3],
            submesh: 0,
        },
        BlendTriangle {
            indices: [2; 3],
            submesh: 1,
        },
    ];
    let mut positions = [Vec3::new(0.0, 0.0, -1.0), Vec3::Z, Vec3::new(0.0, 0.0, 0.9)];
    let mut order = vec![OrderedTriangle::default(); triangles.len()];
    let allocation = order.as_ptr();
    order_triangles(
        &triangles,
        &positions,
        Mat4::IDENTITY,
        Vec3::new(0.0, 0.0, -4.0),
        &mut order,
    );
    assert_eq!(
        order.iter().map(|entry| entry.source).collect::<Vec<_>>(),
        [1, 2, 0],
        "cloth must cover the face from behind regardless of primitive packing"
    );
    assert!(
        (composite(&order, 1.0) - 1.0).abs() < f32::EPSILON,
        "rear cloth fully covers the face at alpha one"
    );
    assert!(
        (composite(&order, 0.5) - 0.625).abs() < f32::EPSILON,
        "partial alpha preserves both cloth layers and the face between them"
    );
    order_triangles(
        &triangles,
        &positions,
        Mat4::IDENTITY,
        Vec3::new(0.0, 0.0, 4.0),
        &mut order,
    );
    assert_eq!(
        order.iter().map(|entry| entry.source).collect::<Vec<_>>(),
        [0, 2, 1],
        "camera reversal changes triangle order across materials"
    );
    positions.swap(0, 1);
    order_triangles(
        &triangles,
        &positions,
        Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
        Vec3::new(2.0, 0.0, -4.0),
        &mut order,
    );
    assert_eq!(
        order.iter().map(|entry| entry.source).collect::<Vec<_>>(),
        [0, 2, 1],
        "order follows deformed positions and placement"
    );
    positions.fill(Vec3::ZERO);
    order_triangles(
        &triangles,
        &positions,
        Mat4::IDENTITY,
        Vec3::ZERO,
        &mut order,
    );
    assert_eq!(
        order.iter().map(|entry| entry.source).collect::<Vec<_>>(),
        [0, 1, 2],
        "ties use source order, independent of the previous camera"
    );
    assert_eq!(
        order.as_ptr(),
        allocation,
        "reordering retains its allocation"
    );
}

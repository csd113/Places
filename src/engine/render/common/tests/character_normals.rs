use super::CharacterAnimator;
use crate::engine::gltf::{PropModel, PropNode};
use glam::{Mat4, Vec3};

/// Protects authored smooth normals at small animation scales and the retained
/// weight policy shared with `skin_position`. One node and two matrix cases need
/// no vertices, textures, asset loading, GPU or map bake.
#[test]
fn skinned_normal_survives_tiny_scales_and_retained_weight_totals() -> Result<(), String> {
    let model = PropModel {
        nodes: vec![PropNode {
            name: "root".into(),
            parent: None,
            children: Vec::new(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }],
        ..PropModel::default()
    };
    let mut animator = CharacterAnimator::new(&model).ok_or("one-node animator")?;
    let normal = Vec3::new(1.0, 1.0, 0.0).normalize();
    let reference = Mat4::from_rotation_y(0.7) * Mat4::from_scale(Vec3::new(2.0, 0.5, 1.5));
    let expected = reference
        .inverse()
        .transpose()
        .transform_vector3(normal)
        .normalize();
    for units in [1.0e-5, 1.0] {
        animator.deltas = vec![reference * Mat4::from_scale(Vec3::splat(units))];
        for (joints, weights) in [
            ([0; 4], [1.0, 0.0, 0.0, 0.0]),
            ([0, 1, 1, 1], [1.0e-4, 0.9999, 0.0, 0.0]),
        ] {
            let actual = Vec3::from_array(animator.skin_normal(joints, weights, normal.to_array()));
            assert!(actual.is_finite(), "skinned normal stays finite");
            assert!(
                actual.dot(expected) > 0.999_999,
                "retained normal at {units}: {actual:?}"
            );
            let position =
                Vec3::from_array(animator.skin_position(joints, weights, normal.to_array()));
            let expected_position = reference.transform_vector3(normal) * units;
            assert!(
                (position / units).distance(expected_position / units) < 1.0e-6,
                "position uses the same retained joints at {units}"
            );
        }
    }
    animator.deltas = vec![Mat4::ZERO];
    assert_eq!(
        animator.skin_normal([0; 4], [1.0, 0.0, 0.0, 0.0], normal.to_array()),
        [0.0; 3],
        "collapsed skinning retains the missing-normal sentinel"
    );
    assert_eq!(
        animator.skin_normal([0; 4], [0.0; 4], normal.to_array()),
        normal.to_array(),
        "unweighted geometry retains the authored normal"
    );
    Ok(())
}

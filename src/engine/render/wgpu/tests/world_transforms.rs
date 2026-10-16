use super::{EnvironmentUniform, Mat4};
use crate::engine::render::common::atmosphere::FogState;
use glam::{Mat3, Vec3};

/// Protects normal orientation for valid small/large model units and retains
/// the ordinary and singular-transform contracts. Four 3x3 fixtures require no
/// GPU, asset loading or map bake.
#[test]
fn normal_transform_preserves_direction_across_model_units() {
    let rotation = Mat4::from_rotation_y(0.7);
    let anisotropic = Mat4::from_scale(Vec3::new(2.0, 0.5, 1.5));
    let reference_model = rotation * anisotropic;
    let source_normal = Vec3::new(1.0, 1.0, 0.0).normalize();
    let source_tangent = Vec3::new(1.0, -1.0, 0.0).normalize();
    let expected = reference_model
        .inverse()
        .transpose()
        .transform_vector3(source_normal)
        .normalize();
    for units in [1.0e-20, 1.0e-5, 1.0, 1.0e15] {
        let model = reference_model * Mat4::from_scale(Vec3::splat(units));
        let uniform = EnvironmentUniform::new([1.0; 3], false, FogState::SHIPPED).with_model(model);
        let normal_matrix =
            Mat3::from_cols_array_2d(&uniform.normal_model.map(|[x, y, z, _]| [x, y, z]));
        let actual = (normal_matrix * source_normal).normalize();
        // Divide before normalizing so the fixture's own squared length cannot
        // underflow at the smallest supported units.
        let tangent = (model.transform_vector3(source_tangent) / units).normalize();
        assert!(actual.is_finite(), "normal must stay finite at {units}");
        assert!(
            actual.dot(expected) > 0.999_999,
            "orientation at {units}: {actual:?}"
        );
        assert!(
            actual.dot(tangent).abs() < 1.0e-6,
            "orthogonality at {units}"
        );
        if units > 0.5 && units < 2.0 {
            let [x, y, z, _] = model.inverse().transpose().to_cols_array_2d();
            assert_eq!(uniform.normal_model, [x, y, z], "ordinary path stays exact");
        }
    }
    for invalid in [
        Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0)),
        Mat4::from_scale(Vec3::splat(f32::NAN)),
        Mat4::from_scale(Vec3::splat(f32::INFINITY)),
    ] {
        let uniform =
            EnvironmentUniform::new([1.0; 3], false, FogState::SHIPPED).with_model(invalid);
        let [x, y, z, _] = Mat4::IDENTITY.to_cols_array_2d();
        assert_eq!(
            uniform.normal_model,
            [x, y, z],
            "invalid inverse uses finite fallback"
        );
    }
}

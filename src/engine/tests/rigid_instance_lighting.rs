//! One pair of fixed uniforms protects independent moving-prop lighting;
//! no GPU, assets, physics step or map bake is required.
use super::{EnvironmentUniform, same_fragment_environment};

#[test]
fn rigid_instancing_ignores_only_pose_and_preserves_every_fragment_lighting_bit() {
    let first = EnvironmentUniform::new([1.0; 3], false, crate::engine::render::FogState::SHIPPED);
    let moved = first.with_model(glam::Mat4::from_scale_rotation_translation(
        glam::Vec3::splat(1.25),
        glam::Quat::from_rotation_y(0.7),
        glam::Vec3::new(3.0, 1.0, -2.0),
    ));
    assert!(
        same_fragment_environment(&first, &moved),
        "pose differences alone can share a submission"
    );
    let mut changed = moved;
    changed.entity_irradiance[0] = f32::from_bits(1);
    assert!(
        !same_fragment_environment(&first, &changed),
        "even a single-bit independent probe sample prevents batching"
    );
    changed = moved;
    changed.opacity = 0.5;
    assert!(
        !same_fragment_environment(&first, &changed),
        "per-object alpha never leaks between rigid instances"
    );
}

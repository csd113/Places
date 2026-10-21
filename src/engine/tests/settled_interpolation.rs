//! A scalar pose fixture protects stationary rotated props from interpolation
//! jitter that repeatedly dirties exact render/visibility/lighting keys. 257
//! alpha samples cost microseconds; no body simulation, assets, GPU or bake.
use super::ItemPose;
use glam::{Quat, Vec3};

#[test]
fn identical_settled_poses_publish_one_exact_transform_at_every_frame_fraction() {
    let pose = ItemPose {
        position: Vec3::new(10.65, 0.178_207_37, 25.095_991),
        rotation: Quat::from_xyzw(0.556_073_3, -0.338_207_42, -0.711_742_6, 0.264_236_06),
    };
    let center = Vec3::new(0.0, 0.108, 0.0);
    for step in 0_u16..=256 {
        let alpha = f32::from(step) / 256.0;
        assert_eq!(
            pose.interpolate(pose, alpha, center),
            pose,
            "a settled body cannot acquire a new transform from a frame fraction"
        );
    }
}

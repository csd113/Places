//! Protect the missing-reflection failure with two tiny CPU checks: mirrored
//! half-space ownership and camera uniform reset. No assets, GPU or bake; sub-ms.
use super::CaptureFrame;
use crate::engine::render::{
    common::reflections::ReflectionPlane,
    wgpu::world::{CameraUniform, camera_uniform_changed},
};
use crate::engine::spatial::Aabb;
use glam::{Mat4, Vec3, Vec4};

#[test]
fn planar_capture_retains_real_camera_side_and_preserves_projection() {
    for (normal, offset, eye) in [
        ([0.0, 1.0, 0.0], 0.0, Vec3::new(6.4, 1.8, 3.4)),
        ([0.0, 1.0, 0.0], -2.0, Vec3::new(0.0, 1.0, 3.0)),
        ([1.0, 0.0, 0.0], -3.0, Vec3::new(5.0, 1.0, 2.0)),
    ] {
        let plane = ReflectionPlane {
            normal,
            offset,
            bounds: Aabb::EMPTY,
        };
        let main = CaptureFrame::new(Mat4::IDENTITY, eye).world_frame();
        let capture = CaptureFrame::planar(&main, &plane);
        let clip = Vec4::from_array(capture.clip_plane);
        assert!(
            clip.dot(eye.extend(1.0)) > 0.0,
            "real side must remain drawable"
        );
        assert!(
            clip.dot(capture.eye.extend(1.0)) < 0.0,
            "backing on mirrored-eye side must be discarded"
        );
        assert_eq!(
            capture.view_projection,
            super::planar_view_projection(main.view_projection, &plane),
            "clipping must not change depth, far plane, XY projection or transparency distances"
        );
        assert_eq!(main.eye, eye, "direct-view state must remain unchanged");
    }
}

#[test]
fn camera_upload_tracks_clip_changes_and_resets_for_probe_and_direct_views() {
    let main = CameraUniform::new(Mat4::IDENTITY, Vec3::ZERO);
    let planar = main.with_clip_plane([0.0, 1.0, 0.0, 0.0]);
    assert!(
        camera_uniform_changed(Some(main), planar),
        "same matrix/eye with changed clip plane must upload"
    );
    assert!(
        !camera_uniform_changed(Some(planar), planar),
        "still capture must reuse its uniform"
    );
    assert!(
        camera_uniform_changed(Some(planar), main),
        "probe/direct reuse must clear planar clipping"
    );
    assert_eq!(
        CaptureFrame::new(Mat4::IDENTITY, Vec3::ZERO).clip_plane,
        [0.0; 4],
        "probe captures must not clip against a stale plane"
    );
}

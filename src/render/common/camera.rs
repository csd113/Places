//! The camera a frame is rendered from, and its view-projection.
//!
//! This is renderer-neutral: position, yaw, pitch and field of view are game
//! state, and the matrix and frustum they produce are pure maths. The renderer
//! and the per-frame preparation both use them.

use super::view::DrawableSize;
use crate::spatial::{DepthRange, Frustum};

/// The camera one frame renders from.
///
/// Yaw 0 looks toward −Z, yaw 90 toward +X; positive pitch looks up. The
/// vertical field of view is the configured baseline; wider drawables expand
/// horizontally (see [`super::view::vertical_fov_for_aspect`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderCamera {
    /// Eye position in world metres.
    pub position: glam::Vec3,
    /// Look yaw in radians.
    pub yaw: f32,
    /// Look pitch in radians.
    pub pitch: f32,
    /// Configured vertical field of view, in degrees.
    pub vertical_fov_degrees: f32,
}

impl RenderCamera {
    /// Builds a camera from the game's position/yaw/pitch/fov state.
    #[must_use]
    pub const fn new(
        position: glam::Vec3,
        yaw: f32,
        pitch: f32,
        vertical_fov_degrees: f32,
    ) -> Self {
        Self {
            position,
            yaw,
            pitch,
            vertical_fov_degrees,
        }
    }

    /// The view-projection and its matching frustum for a render size.
    ///
    /// The projection uses the *OpenGL* clip-depth convention (`z_ndc` in
    /// `[-1, 1]`), not glam's default `[0, 1]` one. OpenGL maps `[-1, 1]` onto
    /// the depth buffer, so a `[0, 1]` matrix would only ever write the
    /// buffer's upper half and halve the usable depth precision for no reason —
    /// precisely the margin coplanar surfaces need. The frustum is extracted
    /// with the matching depth convention so culling and clipping stay in
    /// lockstep.
    #[must_use]
    pub fn view_projection(self, render_size: DrawableSize) -> (glam::Mat4, Frustum) {
        let aspect = render_size.aspect_ratio();
        let effective_fov = super::view::vertical_fov_for_aspect(self.vertical_fov_degrees, aspect);
        let proj = glam::Mat4::perspective_rh_gl(
            effective_fov.to_radians(),
            aspect,
            super::SCENE_NEAR_M,
            super::SCENE_FAR_M,
        );

        // Correctly combine yaw and pitch in the camera forward vector.
        let cos_pitch = self.pitch.cos();
        let forward = glam::Vec3::new(
            self.yaw.sin() * cos_pitch,
            self.pitch.sin(),
            -self.yaw.cos() * cos_pitch,
        );
        // `glam`'s vector and matrix operators are per-component `f32`
        // arithmetic with no overflow or panic path; clippy cannot see that
        // through the operator impls, so the two operations below carry a
        // documented allow.
        #[allow(clippy::arithmetic_side_effects)]
        let view = glam::Mat4::look_at_rh(self.position, self.position + forward, glam::Vec3::Y);
        #[allow(clippy::arithmetic_side_effects)]
        let mvp = proj * view;
        let frustum = Frustum::from_view_projection(&mvp, DepthRange::NegativeOneToOne);
        (mvp, frustum)
    }
}

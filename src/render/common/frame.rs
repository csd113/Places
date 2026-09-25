//! The renderer-neutral description of one frame: what to draw and from where.
//!
//! The OpenGL backend (and, later, any other backend) consumes a
//! [`PreparedFrame`] instead of computing frame policy itself. Everything here
//! is data and maths: the camera, the scene render size, the view-projection
//! and frustum, which mirror plane this frame reflects through and which probe
//! cubemap it samples. No GPU type appears.
//!
//! Preparation is two-phase because the offscreen target is a GPU resource:
//! [`PreparedFrame::plan`] decides everything that does not depend on the
//! target existing, the backend ensures the target, and
//! [`PreparedFrame::begin_scene`] finalises the render size and the matrices
//! that depend on it.

use super::camera::RenderCamera;
use super::framebuffer::scene_target_size;
use super::reflections::{Reflections, nearest_probe, nearest_visible_reflection_plane};
use super::view::DrawableSize;
use crate::quality::QualityProfile;
use crate::spatial::Frustum;

/// The neutral renderer state one frame is prepared from.
pub struct FrameState<'a> {
    /// The camera the frame is drawn from.
    pub camera: RenderCamera,
    /// Physical drawable size the frame targets.
    pub drawable: DrawableSize,
    /// Active quality profile (scene-target size and feature policy).
    pub quality: QualityProfile,
    /// Whether offscreen rendering is enabled at all (settings + benchmark).
    pub offscreen_enabled: bool,
    /// True once a target creation has failed, so the session stays direct.
    pub offscreen_failed: bool,
    /// Whether frustum culling is applied.
    pub culling: bool,
    /// The current level's reflection routing and switches.
    pub reflections: &'a Reflections,
    /// Baked probe positions, in bake order.
    pub probe_positions: &'a [[f32; 3]],
}

/// Everything one frame needs, decided before the GPU is touched.
#[derive(Clone, Copy, Debug)]
pub struct PreparedFrame {
    /// The camera this frame renders from.
    pub camera: RenderCamera,
    /// Physical drawable size.
    pub drawable: DrawableSize,
    /// Size the offscreen scene target should have, or `None` for the direct
    /// default-framebuffer path.
    pub target_size: Option<DrawableSize>,
    /// Whether this frame draws offscreen; finalised by
    /// [`PreparedFrame::begin_scene`] once the target exists.
    pub offscreen: bool,
    /// Size the scene actually renders at.
    pub render_size: DrawableSize,
    /// Whether frustum culling is applied this frame.
    pub culling: bool,
    /// View-projection for `render_size`.
    pub view_projection: glam::Mat4,
    /// Frustum extracted with the same depth convention as the projection.
    pub frustum: Frustum,
    /// The mirror plane this frame reflects through, when one is visible and
    /// the planar pass is enabled.
    pub planar_plane: Option<usize>,
    /// The probe cubemap this frame samples, when probes are resident and
    /// reflections are enabled.
    pub probe: Option<usize>,
    /// Whether the planar pass may run at all this frame (enabled, allowed by
    /// the profile, and the level has a plane). Used by `begin_scene`.
    planar_wanted: bool,
}

/// The offscreen scene target size for one frame, or `None` for the direct
/// path.
///
/// Pure policy, testable without a GL context: `None` is the historical direct
/// path (the switch is off, a target has already failed, or there is nothing to
/// render into), `Some(size)` is the target to have.
/// [`scene_target_size`] owns the profile's size rule.
#[must_use]
pub fn offscreen_plan(
    enabled: bool,
    failed: bool,
    quality: QualityProfile,
    drawable: DrawableSize,
) -> Option<DrawableSize> {
    if !enabled || failed || drawable.is_empty() {
        return None;
    }
    Some(scene_target_size(quality, drawable))
}

impl PreparedFrame {
    /// Prepares a frame, or `None` when the drawable cannot be rendered to.
    ///
    /// The view-projection is provisional at the drawable's size; the backend
    /// finalises it in [`Self::begin_scene`] if the offscreen target renders at
    /// a different size (the Low profile's cap).
    #[must_use]
    pub fn plan(state: &FrameState<'_>) -> Option<Self> {
        if state.drawable.is_empty() {
            return None;
        }
        let target_size = offscreen_plan(
            state.offscreen_enabled,
            state.offscreen_failed,
            state.quality,
            state.drawable,
        );
        let camera = state.camera;
        let position = camera.position_array();
        // The nearest baked probe, when reflections are on. This is the same
        // choice the draw path has always made: nearest by squared distance,
        // first probe wins a tie.
        let probe = if state.reflections.enabled() {
            nearest_probe(state.probe_positions, position)
        } else {
            None
        };
        let planar_wanted = state.reflections.planar_wanted();
        let (view_projection, frustum) = camera.view_projection(state.drawable);
        Some(Self {
            camera,
            drawable: state.drawable,
            target_size,
            offscreen: false,
            render_size: state.drawable,
            culling: state.culling,
            view_projection,
            frustum,
            planar_plane: None,
            probe,
            planar_wanted,
        })
    }

    /// Finalises the frame once the backend knows whether a target exists.
    ///
    /// `offscreen` comes from the target creation: it may be false even when
    /// [`Self::target_size`] is `Some` if the target could not be created, in
    /// which case the frame renders straight into the default framebuffer.
    /// The planar plane is chosen here because it needs the frustum, which
    /// depends on the final render size.
    pub fn begin_scene(
        &mut self,
        offscreen: bool,
        routing: &super::reflections::ReflectionRouting,
    ) {
        self.offscreen = offscreen;
        let render_size = if offscreen {
            self.target_size.unwrap_or(self.drawable)
        } else {
            self.drawable
        };
        if render_size != self.render_size {
            self.render_size = render_size;
            let (view_projection, frustum) = self.camera.view_projection(render_size);
            self.view_projection = view_projection;
            self.frustum = frustum;
        }
        self.planar_plane = if self.planar_wanted {
            nearest_visible_reflection_plane(routing, self.camera.position_array(), &self.frustum)
        } else {
            None
        };
    }
}

//! Reflections: probe cubemaps, the planar mirror target and the capture
//! matrices.
//!
//! Two deliberately limited sources, both opt-in per material (see
//! [`crate::materials::reflection`]):
//!
//! * **Static probes.** One or two cubemaps (64-texel faces at Reflections
//!   Full, 48 at Medium, none at Off), baked once per level load at the
//!   centroid of the reflective geometry that asked for one, from six
//!   90-degree views of the whole scene. The reference renders
//!   the six faces in the GL cube order +X/-X/+Y/-Y/+Z/-Z with the GL face-up
//!   vectors, through `look_at_rh`/`perspective_rh_gl`.
//! * **Planar mirrors.** A real second view of the level, mirrored through a
//!   plane derived from the geometry itself: half the render size, its own
//!   depth, rendered with the mirrored view-projection and a reversed front
//!   face (the reference's `glFrontFace(GL_CW)`, which compensates for the
//!   mirror flipping every triangle's winding). Allowed on Medium and Full.
//!
//! This module owns the GPU resources and the pure capture maths. The renderer
//! owns when a capture runs and which body it draws; the material modes are its
//! decision too, because a capture must suppress reflection sampling exactly
//! like the reference's `reflection_capture` flag.
//!
//! Colour space: the targets are raw `Rgba8Unorm`, the same convention as the
//! scene and presented images, so the world fragment stage's display-space
//! values are captured and sampled back unchanged — exactly what the reference
//! read from its RGBA8 attachments and cubemaps.

use crate::quality::ReflectionQuality;
use crate::render::common::reflections::{ReflectionPlane, mirror_matrix, planar_target_size};
use crate::render::common::view::{DrawableSize, MAX_REFLECTION_PROBES};
use crate::spatial::{DepthRange, Frustum};

/// The format every reflection target uses.
///
/// Raw (non-sRGB), exactly like the reference's RGBA8 attachments and cubemap:
/// the capture pipelines write the shader's display-space output directly, so
/// hardware filtering, mip selection and the sample sites all operate on the
/// reference's own values, and blending inside a capture behaves as GL did.
pub const REFLECTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Cube face edge, in texels, at Reflections Full.
pub const PROBE_FACE_SIZE_FULL: u32 = 64;
/// Cube face edge, in texels, at Reflections Medium.
pub const PROBE_FACE_SIZE_MEDIUM: u32 = 48;

/// The probe face edge a Reflections setting bakes, or `None` for
/// [`ReflectionQuality::Off`] (which has no probe cubemaps at all).
///
/// The probe resolution is deliberately independent of [`crate::quality::QualityLevel`]:
/// `Low + Reflections Full` keeps 64-texel probes, exactly like every other
/// combination that selects Full.
#[must_use]
pub const fn probe_face_size(quality: ReflectionQuality) -> Option<u32> {
    match quality {
        ReflectionQuality::Off => None,
        ReflectionQuality::Medium => Some(PROBE_FACE_SIZE_MEDIUM),
        ReflectionQuality::Full => Some(PROBE_FACE_SIZE_FULL),
    }
}

/// The depth format every reflection target uses.
///
/// The same format as the main depth attachment, because the world pipelines
/// declare it in their depth-stencil state and a capture pass runs those same
/// pipelines. The reference uses a 16-bit renderbuffer for these targets; the
/// difference is depth precision only, and no capture samples its own depth.
pub const REFLECTION_DEPTH_FORMAT: wgpu::TextureFormat = super::surface::DEPTH_FORMAT;

/// The six cube face directions, in the GL cube order the reference used.
///
/// Layer `i` of the wgpu cube array is sampled as face `i`; WebGPU's cube
/// convention is the GL/Vulkan one, so the face order and the up vectors below
/// are the reference's `CUBE_FACES` verbatim.
pub const CUBE_FACE_DIRECTIONS: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],  // +X
    [-1.0, 0.0, 0.0], // -X
    [0.0, 1.0, 0.0],  // +Y
    [0.0, -1.0, 0.0], // -Y
    [0.0, 0.0, 1.0],  // +Z
    [0.0, 0.0, -1.0], // -Z
];

/// The up vector of each cube face, in the reference's order.
pub const CUBE_FACE_UPS: [[f32; 3]; 6] = [
    [0.0, -1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
    [0.0, -1.0, 0.0],
    [0.0, -1.0, 0.0],
];

/// The reference's probe projection: 90 degrees square, near 0.1, far 100,
/// with the one WebGPU capture correction.
///
/// The reference builds this with `perspective_rh_gl` and renders into a GL FBO,
/// whose face image runs bottom-up: a world direction above the face centre
/// lands in a *higher* `t` row. WebGPU render targets store their first row at
/// the top, so a face captured with the unmodified projection would sample
/// vertically flipped (verified by the ignored GPU round-trip test below). The
/// projection therefore negates NDC `y`, which stores the reference's
/// bottom-up image; the capture pipeline uses the reversed front face to
/// compensate for the winding this flip reverses, exactly like the planar
/// mirror's `glFrontFace(GL_CW)`.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // glam matrix products are per-element f32 arithmetic with no overflow or panic path
pub fn probe_projection() -> glam::Mat4 {
    let projection = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
    projection * glam::Mat4::from_scale(glam::Vec3::new(1.0, -1.0, 1.0))
}

/// The wgpu-clip view-projection of one probe face.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // glam vector/matrix arithmetic is per-element f32 with no overflow or panic path
pub fn probe_face_view_projection(eye: [f32; 3], face: usize) -> glam::Mat4 {
    let Some(direction) = CUBE_FACE_DIRECTIONS.get(face) else {
        return glam::Mat4::IDENTITY;
    };
    let Some(up) = CUBE_FACE_UPS.get(face) else {
        return glam::Mat4::IDENTITY;
    };
    let eye = glam::Vec3::from_array(eye);
    let dir = glam::Vec3::from_array(*direction);
    let up = glam::Vec3::from_array(*up);
    let view = glam::Mat4::look_at_rh(eye, eye + dir, up);
    probe_projection() * view
}

/// The mirrored wgpu-clip view-projection of the active planar plane.
///
/// The reference's `mvp * mirror(normal, offset)` for a world-space mirror
/// plane `dot(normal, p) + offset == 0`. The matrix is the corrected one the
/// main pass uses: the clip correction commutes with the mirror (it only
/// touches `z` and `w`), so multiplying the corrected matrix by the mirror is
/// the corrected mirrored matrix.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // one glam matrix product: per-element f32 with no overflow or panic path
pub fn planar_view_projection(view_projection: glam::Mat4, plane: &ReflectionPlane) -> glam::Mat4 {
    view_projection * mirror_matrix(plane.normal, plane.offset)
}

/// One capture's render frame: the wgpu clip-space matrix plus the frustum and
/// eye the culling and sheen need.
#[derive(Clone, Copy, Debug)]
pub struct CaptureFrame {
    /// Clip-space view-projection (wgpu depth range).
    pub view_projection: glam::Mat4,
    /// Frustum extracted in the wgpu depth convention.
    pub frustum: Frustum,
    /// World-space eye; the probe position or the mirrored camera.
    pub eye: glam::Vec3,
}

impl CaptureFrame {
    /// Builds a capture frame from a wgpu-clip view-projection and eye.
    #[must_use]
    pub fn new(view_projection: glam::Mat4, eye: glam::Vec3) -> Self {
        let frustum = Frustum::from_view_projection(&view_projection, DepthRange::ZeroToOne);
        Self {
            view_projection,
            frustum,
            eye,
        }
    }

    /// The world frame this capture's draws run under.
    #[must_use]
    pub const fn world_frame(self) -> super::world::WorldFrame {
        super::world::WorldFrame {
            view_projection: self.view_projection,
            frustum: self.frustum,
            eye: self.eye,
        }
    }
}

/// How far above its reflective geometry a probe is baked, in metres.
///
/// The reference's own lift: a floor's centroid is *on* the floor, and a
/// cubemap baked in a surface sees nothing.
pub const PROBE_LIFT_M: f32 = 1.2;

/// Where one probe bakes: the routing's centroid lifted off the surface.
#[must_use]
pub fn probe_bake_position(point: [f32; 3]) -> [f32; 3] {
    [point[0], point[1] + PROBE_LIFT_M, point[2]]
}

/// One probe cubemap with its own depth attachment.
pub struct ProbeCube {
    /// Kept for ownership; the cube view references it.
    _texture: wgpu::Texture,
    /// The sampling view, dimensioned `Cube`.
    view: wgpu::TextureView,
    /// One render view per face, created once.
    face_views: Vec<wgpu::TextureView>,
    /// Kept for ownership; the face views reference it.
    _depth_texture: wgpu::Texture,
    /// One depth view per face, in cube face order.
    depth_face_views: Vec<wgpu::TextureView>,
    /// World position the probe was baked from.
    pub position: [f32; 3],
    /// Face edge, in texels.
    pub face_size: u32,
}

impl ProbeCube {
    /// Creates one probe's cubemap and per-face depth attachments.
    #[must_use]
    pub fn create(device: &wgpu::Device, position: [f32; 3], face_size: u32) -> Self {
        let face_size = face_size.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-probe"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: REFLECTION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("places-wgpu-probe-view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..wgpu::TextureViewDescriptor::default()
        });
        let face_views = (0..6)
            .map(|face| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("places-wgpu-probe-face"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: face,
                    array_layer_count: Some(1),
                    ..wgpu::TextureViewDescriptor::default()
                })
            })
            .collect();
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-probe-depth"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: REFLECTION_DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_face_views = (0..6)
            .map(|face| {
                depth_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("places-wgpu-probe-depth-face"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: face,
                    array_layer_count: Some(1),
                    ..wgpu::TextureViewDescriptor::default()
                })
            })
            .collect();
        Self {
            _texture: texture,
            view,
            face_views,
            _depth_texture: depth_texture,
            depth_face_views,
            position,
            face_size,
        }
    }

    /// The cube sampling view.
    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The render view of one face (one array layer, 2D view).
    #[must_use]
    pub fn face_view(&self, face: usize) -> Option<&wgpu::TextureView> {
        self.face_views.get(face)
    }

    /// The depth view of one face.
    #[must_use]
    pub fn depth_face_view(&self, face: usize) -> Option<&wgpu::TextureView> {
        self.depth_face_views.get(face)
    }
}

/// The planar mirror target: half the render size, its own depth.
pub struct PlanarTarget {
    /// Kept for ownership; the view references it.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Kept for ownership; the view references it.
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// Target size in pixels.
    pub size: DrawableSize,
}

impl PlanarTarget {
    /// Creates the planar target at `size` (each axis at least one).
    #[must_use]
    pub fn create(device: &wgpu::Device, size: DrawableSize) -> Self {
        let width = size.width.max(1);
        let height = size.height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-planar"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: REFLECTION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-planar-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: REFLECTION_DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            _depth_texture: depth_texture,
            depth_view,
            size: DrawableSize::new(width, height),
        }
    }

    /// The sampling view.
    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The depth view for the capture pass.
    #[must_use]
    pub const fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }
}

/// Every reflection resource of one level.
///
/// Probes are created at level load and live until the next load; the planar
/// target is created lazily on the first frame that needs it and recreated on a
/// size change. Nothing here is created per frame.
#[derive(Default)]
pub struct ReflectionTargets {
    probes: Vec<ProbeCube>,
    planar: Option<PlanarTarget>,
}

impl ReflectionTargets {
    /// Creates one cubemap per wanted probe point, up to the two-probe budget.
    ///
    /// The capture position is the reference's `point + 1.2 m Y`; the face edge
    /// follows the Reflections setting, and [`ReflectionQuality::Off`] creates
    /// no cubemaps at all (nothing may sample a stale capture).
    #[must_use]
    pub fn for_quality(
        device: &wgpu::Device,
        quality: ReflectionQuality,
        probe_points: &[[f32; 3]],
    ) -> Self {
        let probes = probe_face_size(quality).map_or_else(Vec::new, |face_size| {
            probe_points
                .iter()
                .take(MAX_REFLECTION_PROBES)
                .map(|point| ProbeCube::create(device, probe_bake_position(*point), face_size))
                .collect()
        });
        Self {
            probes,
            planar: None,
        }
    }

    /// The baked probe cubemaps.
    #[must_use]
    pub fn probes(&self) -> &[ProbeCube] {
        &self.probes
    }

    /// The planar target.
    #[must_use]
    pub const fn planar(&self) -> Option<&PlanarTarget> {
        self.planar.as_ref()
    }

    /// Drops the planar target, retiring the mirror's GPU image and depth.
    ///
    /// Called when the Reflections setting stops allowing the planar pass, so
    /// a later frame cannot sample (or capture into) a stale mirror image. The
    /// target is recreated lazily by [`Self::ensure_planar`] when the pass is
    /// allowed again.
    pub fn drop_planar(&mut self) {
        self.planar = None;
    }

    /// Ensures the planar target exists at `size`, recreating it when the size
    /// changed. Returns true when the target was created or recreated.
    pub fn ensure_planar(&mut self, device: &wgpu::Device, size: DrawableSize) -> bool {
        if self
            .planar
            .as_ref()
            .is_some_and(|planar| planar.size == size)
        {
            return false;
        }
        self.planar = Some(PlanarTarget::create(device, size));
        true
    }
}

/// The planar target size for the current render size.
///
/// A Reflections setting that does not draw the planar pass never runs it; the
/// caller checks [`crate::render::common::reflections::Reflections::planar_wanted`]
/// first.
#[must_use]
pub fn planar_size_for(render_size: DrawableSize) -> DrawableSize {
    planar_target_size(render_size)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        clippy::expect_used
    )]

    use super::*;

    #[test]
    fn the_probe_face_set_is_the_reference_cube() {
        assert_eq!(CUBE_FACE_DIRECTIONS.len(), 6);
        assert_eq!(CUBE_FACE_UPS.len(), 6);
        for (direction, up) in CUBE_FACE_DIRECTIONS.iter().zip(CUBE_FACE_UPS) {
            let d = glam::Vec3::from_array(*direction);
            let u = glam::Vec3::from_array(up);
            assert!((d.length() - 1.0).abs() < 1e-6);
            assert!((u.length() - 1.0).abs() < 1e-6);
            assert!(d.dot(u).abs() < 1e-6, "the up vector must be perpendicular");
        }
        // The reference's order and ups, verbatim.
        assert_eq!(CUBE_FACE_DIRECTIONS[0], [1.0, 0.0, 0.0]);
        assert_eq!(CUBE_FACE_DIRECTIONS[1], [-1.0, 0.0, 0.0]);
        assert_eq!(CUBE_FACE_DIRECTIONS[2], [0.0, 1.0, 0.0]);
        assert_eq!(CUBE_FACE_DIRECTIONS[3], [0.0, -1.0, 0.0]);
        assert_eq!(CUBE_FACE_DIRECTIONS[4], [0.0, 0.0, 1.0]);
        assert_eq!(CUBE_FACE_DIRECTIONS[5], [0.0, 0.0, -1.0]);
        assert_eq!(CUBE_FACE_UPS[2], [0.0, 0.0, 1.0]);
        assert_eq!(CUBE_FACE_UPS[3], [0.0, 0.0, -1.0]);
    }

    #[test]
    fn a_probe_face_looks_along_its_direction() {
        let eye = [3.0, 4.0, 5.0];
        for face in 0..6 {
            let vp = probe_face_view_projection(eye, face);
            let direction = glam::Vec3::from_array(CUBE_FACE_DIRECTIONS[face]);
            // A point one metre along the face direction projects to the face
            // centre (x=y=0 in NDC) and in front of the near plane.
            let point = glam::Vec3::from_array(eye) + direction;
            let clip = vp * point.extend(1.0);
            assert!(clip.w > 0.0, "face {face} looks forward");
            assert!(
                (clip.x / clip.w).abs() < 1e-6 && (clip.y / clip.w).abs() < 1e-6,
                "face {face} centre"
            );
            // A point one metre along the face's up must stay on the positive
            // side of the frustum; the Y-flipped projection stores it in the
            // lower half of the WebGPU image, which is the reference's
            // bottom-up row order (verified by the GPU round-trip test).
            let up = glam::Vec3::from_array(CUBE_FACE_UPS[face]);
            let above = glam::Vec3::from_array(eye) + direction + up;
            let clip = vp * above.extend(1.0);
            assert!(
                clip.y / clip.w < 0.0,
                "face {face}: the capture projection must store up in a lower row"
            );
        }
    }

    #[test]
    fn the_probe_projection_is_ninety_degrees_square() {
        let projection = probe_projection();
        // A point 1 m ahead and 1 m to the side is exactly on the edge of the
        // 90-degree frustum.
        let edge = projection * glam::Vec4::new(1.0, 0.0, -1.0, 1.0);
        assert!((edge.x / edge.w - 1.0).abs() < 1e-6);
        let centre = projection * glam::Vec4::new(0.0, 0.0, -1.0, 1.0);
        assert!(centre.x.abs() < 1e-6 && centre.y.abs() < 1e-6);
        assert!(centre.z / centre.w > -1.0 && centre.z / centre.w < 1.0);
        // The WebGPU capture correction: NDC y is negated, so a point above the
        // centre lands in a *lower* row, storing the reference's bottom-up
        // face image. The ignored GPU round-trip test verifies the whole
        // capture+sample path against it.
        let above = projection * glam::Vec4::new(0.0, 1.0, -1.0, 1.0);
        assert!(
            above.y / above.w < 0.0,
            "the probe projection must negate NDC y for WebGPU's top-down rows"
        );
        let below = projection * glam::Vec4::new(0.0, -1.0, -1.0, 1.0);
        assert!(below.y / below.w > 0.0);
    }

    #[test]
    fn the_probe_face_size_follows_the_reflections_setting() {
        assert_eq!(probe_face_size(ReflectionQuality::Full), Some(64));
        assert_eq!(probe_face_size(ReflectionQuality::Medium), Some(48));
        assert_eq!(probe_face_size(ReflectionQuality::Off), None);
        // The resolution must not follow the overall quality level any more:
        // the function does not even see it.
        assert_eq!(PROBE_FACE_SIZE_FULL, 64);
        assert_eq!(PROBE_FACE_SIZE_MEDIUM, 48);
    }

    #[test]
    fn a_probe_bakes_above_the_surface_that_asked_for_it() {
        assert_eq!(PROBE_LIFT_M, 1.2);
        assert_eq!(probe_bake_position([3.0, 4.0, 5.0]), [3.0, 5.2, 5.0]);
    }

    #[test]
    fn the_planar_target_is_half_the_render_size() {
        assert_eq!(
            planar_size_for(DrawableSize::new(1280, 720)),
            DrawableSize::new(640, 360)
        );
        assert_eq!(
            planar_size_for(DrawableSize::new(1, 1)),
            DrawableSize::new(1, 1)
        );
        assert_eq!(
            planar_size_for(DrawableSize::new(0, 0)),
            DrawableSize::new(0, 0)
        );
    }

    #[test]
    fn the_planar_matrix_is_the_reference_mirror_composition() {
        let plane = ReflectionPlane {
            normal: [0.0, 1.0, 0.0],
            offset: -1.5,
            bounds: crate::spatial::Aabb::EMPTY,
        };
        let view_projection = glam::Mat4::perspective_rh_gl(1.0, 1.0, 0.1, 100.0);
        let mirrored = planar_view_projection(view_projection, &plane);
        let expected = view_projection * mirror_matrix(plane.normal, plane.offset);
        assert_eq!(mirrored, expected);

        // A point on the plane maps to itself in world space under the mirror.
        let point = glam::Vec4::new(0.3, 1.5, -2.0, 1.0);
        let mirrored_point = mirror_matrix(plane.normal, plane.offset) * point;
        assert!((mirrored_point.x - point.x).abs() < 1e-6);
        assert!((mirrored_point.y - point.y).abs() < 1e-6);
        assert!((mirrored_point.z - point.z).abs() < 1e-6);
        // And a point one metre above the plane lands one metre below it.
        let above = glam::Vec4::new(0.0, 2.5, 0.0, 1.0);
        let below = mirror_matrix(plane.normal, plane.offset) * above;
        assert!((below.y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_capture_frame_extracts_a_matching_frustum() {
        let wgpu_vp = probe_face_view_projection([0.0, 0.0, 0.0], 4);
        let frame = CaptureFrame::new(wgpu_vp, glam::Vec3::ZERO);
        assert_eq!(frame.view_projection, wgpu_vp);
        // The frustum is extracted from the wgpu [0, 1] depth matrix. A point
        // 1000 m away is outside the probe's 100 m far plane; a point one metre
        // ahead is inside.
        assert!(!frame.frustum.intersects_aabb(&crate::spatial::Aabb {
            min: [999.0, -0.1, -0.1],
            max: [1001.0, 0.1, 0.1],
        }));
        assert!(frame.frustum.intersects_aabb(&crate::spatial::Aabb {
            min: [-0.1, -0.1, -0.1],
            max: [0.1, 0.1, 0.1],
        }));
    }

    #[test]
    fn the_reflection_format_is_raw_display_space() {
        assert_eq!(REFLECTION_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert!(!REFLECTION_FORMAT.is_srgb());
        assert_eq!(REFLECTION_DEPTH_FORMAT, super::super::surface::DEPTH_FORMAT);
    }

    /// A GPU round trip that pins the cube convention end to end.
    ///
    /// Each face is captured with the *actual* [`probe_face_view_projection`]
    /// matrix from a world-space quad that carries its in-plane axes as vertex
    /// colours (`red = right`, `green = up`). Sampling known directions then
    /// checks that:
    ///
    /// * every face direction selects its own layer (a face swap fails);
    /// * the face's `right` raises the image's `s` channel and its `up` raises
    ///   the `t` channel — the reference's GL FBO convention.
    ///
    /// Ignored by default: it needs a real adapter. Run with:
    ///
    /// ```text
    /// cargo test --all-features --bin places -- --ignored the_cube_round_trip
    /// ```
    #[test]
    #[ignore = "requires a GPU adapter"]
    #[allow(clippy::too_many_lines)]
    fn the_cube_round_trip_matches_the_reference_face_convention() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("a GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("places-cube-convention-test"),
            ..Default::default()
        }))
        .expect("a device");

        let face_size = 16u32;
        let eye = [4.0_f32, 5.0, 6.0];
        let cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cube-convention"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let cube_view = cube.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..wgpu::TextureViewDescriptor::default()
        });

        // Six clip-space vertices per face: a quad one metre in front of the
        // probe eye, spanning the 90-degree frustum, coloured by its in-plane
        // offset (`red = a`, `green = b`).
        let mut vertex_bytes: Vec<u8> = Vec::with_capacity(36 * 32);
        for face in 0..6usize {
            let direction = glam::Vec3::from_array(CUBE_FACE_DIRECTIONS[face]);
            let up = glam::Vec3::from_array(CUBE_FACE_UPS[face]);
            let right = direction.cross(up).normalize();
            let projection = probe_face_view_projection(eye, face);
            let centre = glam::Vec3::from_array(eye) + direction;
            for (a, b) in [
                (-1.0_f32, -1.0_f32),
                (1.0, -1.0),
                (1.0, 1.0),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
            ] {
                let world = centre + right * (a * 1.2) + up * (b * 1.2);
                let clip = projection * world.extend(1.0);
                for value in [clip.x, clip.y, clip.z, clip.w] {
                    vertex_bytes.extend_from_slice(&value.to_le_bytes());
                }
                for value in [a.mul_add(0.5, 0.5), b.mul_add(0.5, 0.5), 0.0, 1.0] {
                    vertex_bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube-vertices"),
            size: u64::try_from(vertex_bytes.len()).unwrap_or(1152),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, &vertex_bytes);

        let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube-fill"),
            source: wgpu::ShaderSource::Wgsl(
                r"
struct VertexIn {
    @location(0) clip: vec4<f32>,
    @location(1) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};
@vertex
fn vs(vertex: VertexIn) -> VOut {
    var out: VOut;
    out.pos = vertex.clip;
    out.color = vertex.color;
    return out;
}
@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    return in.color;
}
"
                .into(),
            ),
        });
        let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube-fill"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &fill_shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fill_shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cube-fill"),
        });
        for face in 0..6usize {
            let view = cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: u32::try_from(face).unwrap_or(0),
                array_layer_count: Some(1),
                ..wgpu::TextureViewDescriptor::default()
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube-fill-face"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&fill_pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            let start = u32::try_from(face).unwrap_or(0) * 6;
            pass.draw(start..start + 6, 0..1);
        }
        queue.submit([encoder.finish()]);

        // Sample 18 directions: each face centre, +up * 0.5 and +right * 0.5.
        let mut directions = [[0.0_f32; 4]; 18];
        for face in 0..6usize {
            let d = glam::Vec3::from_array(CUBE_FACE_DIRECTIONS[face]);
            let up = glam::Vec3::from_array(CUBE_FACE_UPS[face]);
            let right = d.cross(up).normalize();
            directions[face * 3] = d.extend(0.0).to_array();
            directions[face * 3 + 1] = (d + up * 0.5).normalize().extend(0.0).to_array();
            directions[face * 3 + 2] = (d + right * 0.5).normalize().extend(0.0).to_array();
        }
        let mut direction_bytes = Vec::with_capacity(18 * 16);
        for direction in directions {
            for value in direction {
                direction_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let direction_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube-directions"),
            size: u64::try_from(direction_bytes.len()).unwrap_or(288),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&direction_buffer, 0, &direction_bytes);
        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube-results"),
            size: 18 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube-readback"),
            size: 18 * 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cube-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let sample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube-sample"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@group(0) @binding(0) var cube_texture: texture_cube<f32>;
@group(0) @binding(1) var cube_sampler: sampler;
@group(0) @binding(2) var<storage, read> directions: array<vec4<f32>, 18>;
@group(0) @binding(3) var<storage, read_write> results: array<vec4<f32>, 18>;
@compute @workgroup_size(1)
fn sample_all() {
    for (var i = 0u; i < 18u; i = i + 1u) {
        results[i] = textureSampleLevel(cube_texture, cube_sampler, directions[i].xyz, 0.0);
    }
}
"
                .into(),
            ),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube-sample-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube-sample-pipeline"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cube-sample"),
            layout: Some(&pipeline_layout),
            module: &sample_shader,
            entry_point: Some("sample_all"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cube-sample"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: direction_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: result_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cube-sample"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cube-sample"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&result_buffer, 0, &readback, 0, 18 * 16);
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        receiver.recv().expect("map callback").expect("buffer maps");
        let data = slice.get_mapped_range().expect("mapped range");
        let mut readings = [[0.0_f32; 4]; 18];
        for (index, pixels) in data.as_chunks::<16>().0.iter().enumerate() {
            for (component, value) in pixels.as_chunks::<4>().0.iter().enumerate() {
                readings[index][component] = f32::from_le_bytes(*value);
            }
        }
        drop(data);
        readback.unmap();

        for face in 0..6usize {
            let centre = readings[face * 3];
            assert!(
                (centre[0] - 0.5).abs() < 0.1 && (centre[1] - 0.5).abs() < 0.1,
                "face {face} centre sampled {centre:?}; the direction must select \
                 the face's layer and its centre"
            );
            let up_sample = readings[face * 3 + 1];
            assert!(
                up_sample[1] > centre[1] + 0.15,
                "face {face}: sampling towards its up must raise the image t axis \
                 (got {up_sample:?} vs centre {centre:?})"
            );
            let right_sample = readings[face * 3 + 2];
            assert!(
                right_sample[0] > centre[0] + 0.15,
                "face {face}: sampling towards its right must raise the image s axis \
                 (got {right_sample:?} vs centre {centre:?})"
            );
        }
    }
}

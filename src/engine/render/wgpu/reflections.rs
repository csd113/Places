//! Reflections: probe cubemaps, the planar mirror target and the capture
//! matrices.
//!
//! Two deliberately limited sources, both opt-in per material (see
//! [`crate::engine::materials::reflection`]):
//!
//! * **Static probes.** One or two cubemaps (64-texel faces at Reflections
//!   Full, 48 at Medium, none at Off), baked once per level load at the
//!   centroid of the reflective geometry that asked for one, from six
//!   90-degree views of the whole scene. The reference renders
//!   the six faces in the GL cube order +X/-X/+Y/-Y/+Z/-Z with the GL face-up
//!   vectors, through [`glam::camera::rh::view::look_at_mat4`] and
//!   [`glam::camera::rh::proj::opengl::perspective`]. A compiled package
//!   stores each capture with an offline-prefiltered roughness mip chain
//!   ([`crate::engine::render::common::probe_filter`]); a developer install that bakes
//!   its own probes fills level 0 only and the shader keeps its two-tap rough
//!   fallback.
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
//! Captures and prefiltered mip chains use linear RGBA16F radiance. They have
//! no exposure, display curve or sRGB encode, and sampling returns linear HDR.

use crate::engine::quality::ReflectionQuality;
use crate::engine::render::common::reflections::{
    ReflectionPlane, mirror_matrix, planar_target_size,
};
use crate::engine::render::common::view::{DrawableSize, MAX_REFLECTION_PROBES};
use crate::engine::render::common::{MAX_PROBE_MIPS, mip_levels_for, prefilter_hdr_cube};
use crate::engine::spatial::{DepthRange, Frustum};

/// The format every reflection target uses.
///
/// Linear HDR capture/storage, shared by cubemaps and planar images.
pub const REFLECTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Cube face edge, in texels, at Reflections Full.
pub const PROBE_FACE_SIZE_FULL: u32 = 64;
/// Cube face edge, in texels, at Reflections Medium.
pub const PROBE_FACE_SIZE_MEDIUM: u32 = 48;

/// The probe face edge a Reflections setting bakes, or `None` for
/// [`ReflectionQuality::Off`] (which has no probe cubemaps at all).
///
/// The probe resolution is deliberately independent of [`crate::engine::quality::QualityLevel`]:
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
/// The reference builds this with [`glam::camera::rh::proj::opengl::perspective`]
/// and renders into a GL FBO,
/// whose face image runs bottom-up: a world direction above the face centre
/// lands in a *higher* `t` row. WebGPU render targets store their first row at
/// the top, so a face captured with the unmodified projection would sample
/// vertically flipped (verified by the ignored GPU round-trip test below). The
/// projection therefore negates NDC `y`, which stores the reference's
/// bottom-up image; the capture pipeline uses the reversed front face to
/// compensate for the winding this flip reverses, exactly like the planar
/// mirror's `glFrontFace(GL_CW)`.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam matrix products are per-element f32 arithmetic with no integer overflow or panic path"
)] // glam matrix products are per-element f32 arithmetic with no overflow or panic path
pub fn probe_projection() -> glam::Mat4 {
    let projection =
        glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
    projection * glam::Mat4::from_scale(glam::Vec3::new(1.0, -1.0, 1.0))
}

/// The wgpu-clip view-projection of one probe face.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam vector/matrix arithmetic is per-element f32 with no integer overflow or panic path"
)] // glam vector/matrix arithmetic is per-element f32 with no overflow or panic path
pub fn probe_face_view_projection(eye: [f32; 3], face: usize) -> glam::Mat4 {
    let Some(direction) = CUBE_FACE_DIRECTIONS.get(face) else {
        return glam::Mat4::IDENTITY;
    };
    let Some(up) = CUBE_FACE_UPS.get(face) else {
        return glam::Mat4::IDENTITY;
    };
    let eye_point = glam::Vec3::from_array(eye);
    let dir = glam::Vec3::from_array(*direction);
    let up_vector = glam::Vec3::from_array(*up);
    let view = glam::camera::rh::view::look_at_mat4(eye_point, eye_point + dir, up_vector);
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
#[expect(
    clippy::arithmetic_side_effects,
    reason = "one glam matrix product: per-element f32 with no integer overflow or panic path"
)] // one glam matrix product: per-element f32 with no overflow or panic path
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
    /// Keeps geometry on the real camera's side of the mirror; zero for probes.
    pub clip_plane: [f32; 4],
}

impl CaptureFrame {
    /// Reflect both camera spaces together before preparing visibility.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Negating a validated finite floating-point plane has no overflow or panic path"
    )]
    pub fn planar(frame: &super::world::WorldFrame, plane: &ReflectionPlane) -> Self {
        let mut capture = Self::new(
            planar_view_projection(frame.view_projection, plane),
            glam::Vec3::from_array(crate::engine::render::common::reflections::mirror_point(
                plane.normal,
                plane.offset,
                frame.eye.to_array(),
            )),
        );
        let half_space = glam::Vec4::new(
            plane.normal[0],
            plane.normal[1],
            plane.normal[2],
            plane.offset,
        );
        capture.clip_plane = if plane.distance(frame.eye.to_array()) < 0.0 {
            -half_space
        } else {
            half_space
        }
        .to_array();
        capture
    }

    /// Builds a capture frame from a wgpu-clip view-projection and eye.
    #[must_use]
    pub fn new(view_projection: glam::Mat4, eye: glam::Vec3) -> Self {
        let frustum = Frustum::from_view_projection(&view_projection, DepthRange::ZeroToOne);
        Self {
            view_projection,
            frustum,
            eye,
            clip_plane: [0.0; 4],
        }
    }

    /// The world frame this capture's draws run under.
    #[must_use]
    pub const fn world_frame(self) -> super::world::WorldFrame {
        super::world::WorldFrame {
            view_projection: self.view_projection,
            frustum: self.frustum,
            eye: self.eye,
            occlusion: crate::engine::render::common::occlusion::OcclusionView::EMPTY,
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
///
/// The colour texture always carries the full packaging chain for its face
/// size, so a packaged payload's levels always fit; a developer install writes
/// level 0 and leaves the small coarse levels unread (`probe_max_mip` is zero,
/// which selects the shader's two-tap fallback).
pub struct ProbeCube {
    /// Kept for ownership; the cube view references it, and the compiler
    /// reads faces back from it.
    texture: wgpu::Texture,
    /// The sampling view, dimensioned `Cube`, over every mip level.
    view: wgpu::TextureView,
    /// One render view per face at level 0, created once.
    face_views: Vec<wgpu::TextureView>,
    /// Kept for ownership; the face views reference it.
    _depth_texture: wgpu::Texture,
    /// One depth view per face, in cube face order.
    depth_face_views: Vec<wgpu::TextureView>,
    /// World position the probe was baked from.
    pub position: [f32; 3],
    /// Base face edge, in texels.
    pub face_size: u32,
    /// Mip levels the colour texture carries.
    pub mip_levels: u32,
}

/// The edge of mip `level` of a probe captured at `face_edge`, or `None` past
/// the chain.
///
/// Level 0 is the base edge; each level halves, and a level below one texel
/// does not exist.
#[must_use]
pub const fn probe_mip_edge(face_edge: u32, level: u32) -> Option<u32> {
    if face_edge == 0 || level >= u32::BITS {
        return None;
    }
    let edge = face_edge >> level;
    if edge == 0 { None } else { Some(edge) }
}

impl ProbeCube {
    /// Creates one probe's cubemap and per-face depth attachments.
    ///
    /// `mip_levels` is clamped to the chain the face size supports
    /// (`ilog2(face_size) + 1`), so an oversized request cannot make wgpu
    /// reject the texture. The depth attachment stays one level: a capture
    /// renders level 0 only.
    #[must_use]
    pub fn create(
        device: &wgpu::Device,
        position: [f32; 3],
        face_size: u32,
        mip_levels: u32,
    ) -> Self {
        let resident_face_size = face_size.max(1);
        let max_levels = resident_face_size.ilog2().saturating_add(1);
        let resident_mip_levels = mip_levels.clamp(1, max_levels);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-probe"),
            size: wgpu::Extent3d {
                width: resident_face_size,
                height: resident_face_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: resident_mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: REFLECTION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
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
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    ..wgpu::TextureViewDescriptor::default()
                })
            })
            .collect();
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-probe-depth"),
            size: wgpu::Extent3d {
                width: resident_face_size,
                height: resident_face_size,
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
            texture,
            view,
            face_views,
            _depth_texture: depth_texture,
            depth_face_views,
            position,
            face_size: resident_face_size,
            mip_levels: resident_mip_levels,
        }
    }

    /// The cube sampling view.
    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The owning texture, for the offline compiler's face read-back.
    #[must_use]
    pub const fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Writes one linear RGBA16F face into one resident cubemap mip.
    ///
    /// `rgba` must hold exactly `edge * edge * 8` bytes for `edge` the level's
    /// edge (`face_size >> level`); a shorter buffer, a level past the resident
    /// chain, or a face index past six is ignored rather than letting wgpu
    /// validate the write.
    pub fn write_face(&self, queue: &wgpu::Queue, level: u32, face: usize, rgba: &[u8]) {
        if level >= self.mip_levels {
            return;
        }
        let Some(level_edge) = probe_mip_edge(self.face_size, level) else {
            return;
        };
        let expected = usize::try_from(level_edge)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::try_from(level_edge).unwrap_or(usize::MAX))
            .saturating_mul(8);
        if face >= 6 || rgba.len() != expected {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: u32::try_from(face).unwrap_or(0),
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(level_edge.saturating_mul(8)),
                rows_per_image: Some(level_edge),
            },
            wgpu::Extent3d {
                width: level_edge,
                height: level_edge,
                depth_or_array_layers: 1,
            },
        );
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

/// One probe's captured faces, as the offline compiler packages them.
///
/// Linear RGBA16F, one `edge * edge * 8` buffer per face in the renderer's cube face
/// order; [`Self::packaged_mips`] turns the capture into the roughness chain
/// the compiler writes into one KTX2 cube and the player uploads back verbatim.
#[derive(Clone, Debug)]
pub struct ProbeFaceReadback {
    /// World position the probe was captured from.
    pub position: [f32; 3],
    /// Base face edge in texels.
    pub face_size: u32,
    /// Six linear RGBA16F base faces in cube face order.
    pub faces: [Vec<u8>; 6],
}

impl ProbeFaceReadback {
    /// Maximum mip levels a packaged probe chain may carry.
    ///
    /// The package record validates against this; the value is the neutral
    /// prefilter's [`MAX_PROBE_MIPS`].
    pub const MAX_MIP_LEVELS: u32 = MAX_PROBE_MIPS;

    /// The mip level count a packaged chain at `face_edge` carries.
    #[must_use]
    pub const fn packaged_mip_levels(face_edge: u32) -> u32 {
        mip_levels_for(face_edge)
    }

    /// Builds the offline-prefiltered mip chain this capture packages.
    ///
    /// Level 0 is a copy of [`Self::faces`]; each later level is a cone average
    /// with roughness growing to one, computed by
    /// [`crate::engine::render::common::probe_filter`]. The filter is deterministic and
    /// thread-free, so two builds of the same capture package identical bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the face buffers do not match `face_size`.
    pub fn packaged_mips(&self) -> Result<Vec<[Vec<u8>; 6]>, String> {
        prefilter_hdr_cube(
            self.face_size,
            &self.faces,
            Self::packaged_mip_levels(self.face_size),
        )
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
    /// no cubemaps at all (nothing may sample a stale capture). Every cubemap
    /// carries the packaging chain for its face size, so a compiled payload's
    /// levels always fit; a live bake fills level 0 only.
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
                .map(|point| {
                    ProbeCube::create(
                        device,
                        probe_bake_position(*point),
                        face_size,
                        mip_levels_for(face_size),
                    )
                })
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
/// caller checks [`crate::engine::render::common::reflections::Reflections::planar_wanted`]
/// first.
#[must_use]
pub fn planar_size_for(render_size: DrawableSize) -> DrawableSize {
    planar_target_size(render_size)
}

#[cfg(test)]
#[path = "../../tests/planar_capture.rs"]
mod tests;

//! Selective reflections: the planes, the probe targets and the mirror maths.
//!
//! Two deliberately limited sources, both opt-in per material (see
//! [`crate::materials::reflection`]):
//!
//! * **Static probes.** One or two 64-texel cubemaps baked once per level load
//!   at the centroid of the reflective geometry that asked for one. Sampling a
//!   cubemap costs one texture read, so probes stay on both quality profiles
//!   (Low bakes 32-texel faces).
//! * **Planar mirrors.** A real second view of the level, mirrored through a
//!   plane derived from the geometry itself. Only ever one plane per frame, only
//!   when that plane's reflective batches survive the frustum test, and only on
//!   the Full profile: it is the one reflection cost that scales with scene
//!   complexity.
//!
//! This module holds the GL objects and the mathematics; the renderer owns the
//! draw calls, because a reflection is a second run of the ordinary scene body
//! with a different view-projection.
//!
//! Nothing here culls back faces — the renderer never does — so the mirrored
//! pass keeps `gl_FrontFacing` meaningful by flipping the front-face winding
//! instead of the geometry.

use glow::HasContext;

use super::DrawableSize;
use super::mesh::LevelMesh;
use super::postprocess::ColorTarget;
use super::view::{
    MAX_REFLECTION_PROBES, PLANAR_REFLECTION_SCALE_DIVISOR, PROBE_FACE_TEXELS_FULL,
    PROBE_FACE_TEXELS_LOW,
};
use crate::materials::{MaterialReflection, ReflectionMode};
use crate::quality::QualityProfile;
use crate::spatial::Aabb;

/// How far apart two probe-reflective surfaces may be and still share a probe,
/// in metres.
///
/// A probe is baked at a point and reads convincingly within roughly a room of
/// it, so this is the size of a large room: two polished floors in one hall
/// share a probe, and the same material in the next hall gets its own.
const PROBE_CLUSTER_RADIUS_M: f32 = 12.0;

/// How close two planes must be to count as the same surface, in metres.
///
/// A floor patch and the deck beside it are the same mirror plane and should
/// share one reflection pass; a tile that sits two millimetres proud of another
/// is not a different mirror.
const PLANE_MERGE_EPS: f32 = 1.0e-3;

/// One mirror plane the level's materials marked.
#[derive(Clone, Copy, Debug)]
pub(super) struct ReflectionPlane {
    /// Unit normal, pointing out of the surface.
    pub(super) normal: [f32; 3],
    /// Offset such that a point on the plane satisfies `dot(normal, p) + offset == 0`.
    pub(super) offset: f32,
    /// Bounding box of every reflective batch on this plane, for the frustum test.
    pub(super) bounds: Aabb,
}

impl ReflectionPlane {
    /// Signed distance from the plane to `point`.
    #[must_use]
    pub(super) fn distance(&self, point: [f32; 3]) -> f32 {
        self.normal[0].mul_add(
            point[0],
            self.normal[1].mul_add(point[1], self.normal[2].mul_add(point[2], self.offset)),
        )
    }
}

/// Where every reflective material gets its image from.
///
/// Built once per level load from the emitted geometry, not from the level
/// file: a material can be used on two different planes, and the plane is a
/// property of the surface, not of the material.
#[derive(Clone, Debug, Default)]
pub(super) struct ReflectionRouting {
    /// The distinct mirror planes the level's geometry produced.
    pub(super) planes: Vec<ReflectionPlane>,
    /// Per material index: the plane index its planar reflection reads.
    pub(super) plane_for_material: Vec<Option<u16>>,
    /// Per material index: whether its reflection is probe-based.
    pub(super) probe_for_material: Vec<bool>,
    /// Centroid of every probe-reflective surface, in world metres.
    pub(super) probe_points: Vec<[f32; 3]>,
}

impl ReflectionRouting {
    /// The plane a material reflects on, if it is a planar material.
    #[must_use]
    pub(super) fn plane_of(&self, material: usize) -> Option<usize> {
        self.plane_for_material
            .get(material)
            .copied()
            .flatten()
            .map(usize::from)
    }

    /// True when the level has anything to reflect at all.
    #[must_use]
    pub(super) const fn is_empty(&self) -> bool {
        self.planes.is_empty() && self.probe_points.is_empty()
    }
}

/// Derives the reflection routing from the emitted static geometry.
///
/// Only ranges whose material authors an active reflection contribute, which is
/// what makes this "selective": a level that marks nothing produces an empty
/// routing, no reflection pass ever runs, and nothing about the frame changes.
///
/// A planar range whose vertices do not actually lie on one plane (a material
/// reused on a curved or stepped surface) is reported and skipped rather than
/// reflected wrongly.
#[allow(clippy::print_stderr)] // one line per malformed marking, at level load only
#[must_use]
pub(super) fn routing_from_mesh(
    mesh: &LevelMesh,
    reflections: &[MaterialReflection],
    material_count: usize,
) -> ReflectionRouting {
    let mut routing = ReflectionRouting {
        plane_for_material: vec![None; material_count],
        probe_for_material: vec![false; material_count],
        ..ReflectionRouting::default()
    };
    // Probe-reflective geometry is clustered by distance, so a level with
    // polished floors in two rooms gets two probes instead of one point in the
    // wall between them. Each cluster's representative is its area-weighted
    // centroid.
    let mut clusters: Vec<ProbeCluster> = Vec::new();
    for range in &mesh.ranges {
        let material = usize::from(range.key.material);
        let Some(reflection) = reflections.get(material).copied() else {
            continue;
        };
        if !reflection.is_active() {
            continue;
        }
        if reflection.mode == ReflectionMode::Probe {
            if let Some(centre) = range_centre(range) {
                let size = range.bounds.max;
                let span = [
                    (size[0] - range.bounds.min[0]).max(0.0),
                    (size[1] - range.bounds.min[1]).max(0.0),
                    (size[2] - range.bounds.min[2]).max(0.0),
                ];
                // The reflective area of a thin surface: a floor patch is wide
                // and flat, a panel is wide and tall. Taking the largest pair of
                // extents keeps both from vanishing.
                let area = (span[0] * span[2]).max(span[0] * span[1]);
                add_probe_sample(&mut clusters, centre, area);
                if let Some(entry) = routing.probe_for_material.get_mut(material) {
                    *entry = true;
                }
            }
            continue;
        }
        let Some((normal, offset)) = range_plane(range) else {
            eprintln!(
                "[reflections] material {material} marks a planar reflection but its \
                 geometry is not planar; skipping that range"
            );
            continue;
        };
        let index = merge_plane(&mut routing.planes, normal, offset, range.bounds);
        if let Some(entry) = routing.plane_for_material.get_mut(material) {
            *entry = Some(index);
        }
    }
    clusters.sort_by(|left, right| {
        right
            .area
            .partial_cmp(&left.area)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for cluster in clusters.into_iter().take(MAX_REFLECTION_PROBES) {
        if let Some(point) = cluster.centroid() {
            routing.probe_points.push(point);
        }
    }
    routing
}

/// One cluster of probe-reflective geometry, accumulated by distance.
struct ProbeCluster {
    /// Area-weighted sum of the sample centres.
    sum: [f32; 3],
    /// Total reflective area sampled.
    area: f32,
    /// A representative point, so a sample can be tested against the cluster.
    representative: [f32; 3],
}

impl ProbeCluster {
    /// The cluster's area-weighted centre, or `None` for an empty cluster.
    fn centroid(&self) -> Option<[f32; 3]> {
        if self.area <= 0.0 {
            return None;
        }
        let point = [
            self.sum[0] / self.area,
            self.sum[1] / self.area,
            self.sum[2] / self.area,
        ];
        point.iter().all(|value| value.is_finite()).then_some(point)
    }
}

/// Adds one reflective sample to the nearest cluster, or starts a new one.
///
/// The radius is deliberately room-sized: a room's worth of polished floor is
/// one probe, and the next room is another.
#[allow(clippy::arithmetic_side_effects)] // float-only, finite inputs
fn add_probe_sample(clusters: &mut Vec<ProbeCluster>, centre: [f32; 3], area: f32) {
    if !area.is_finite() || area <= 0.0 {
        return;
    }
    let mut nearest: Option<(f32, usize)> = None;
    for (index, cluster) in clusters.iter().enumerate() {
        let delta = glam::Vec3::new(
            cluster.representative[0] - centre[0],
            cluster.representative[1] - centre[1],
            cluster.representative[2] - centre[2],
        );
        let distance = delta.length();
        if !distance.is_finite() || distance > PROBE_CLUSTER_RADIUS_M {
            continue;
        }
        if nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, index));
        }
    }
    let cluster = if let Some((_, index)) = nearest {
        match clusters.get_mut(index) {
            Some(cluster) => cluster,
            None => return,
        }
    } else {
        {
            clusters.push(ProbeCluster {
                sum: [0.0; 3],
                area: 0.0,
                representative: centre,
            });
            match clusters.last_mut() {
                Some(cluster) => cluster,
                None => return,
            }
        }
    };
    for (slot, value) in cluster.sum.iter_mut().zip(centre) {
        *slot = value.mul_add(area, *slot);
    }
    cluster.area += area;
}

/// Centre of one mesh range's bounding box.
fn range_centre(range: &super::LevelMeshRange) -> Option<[f32; 3]> {
    let min = range.bounds.min;
    let max = range.bounds.max;
    let centre = [
        f32::midpoint(min[0], max[0]),
        f32::midpoint(min[1], max[1]),
        f32::midpoint(min[2], max[2]),
    ];
    centre
        .iter()
        .all(|value| value.is_finite())
        .then_some(centre)
}

/// The plane one range lies on, or `None` when its vertices disagree.
///
/// The normal comes from the emitted surface frame (`compute_surface_frames`),
/// so it is the winding's normal rather than a guess, and the offset is taken
/// from the first vertex once every other vertex is confirmed to be on that
/// plane to within [`PLANE_MERGE_EPS`].
fn range_plane(range: &super::LevelMeshRange) -> Option<([f32; 3], f32)> {
    let first = range.vertices.first()?;
    let normal = first.normal;
    let length = normal[0].mul_add(
        normal[0],
        normal[1].mul_add(normal[1], normal[2] * normal[2]),
    );
    if !length.is_finite() || length <= 1.0e-12 {
        return None;
    }
    let inverse = 1.0 / length.sqrt();
    let unit = [
        normal[0] * inverse,
        normal[1] * inverse,
        normal[2] * inverse,
    ];
    let offset = -(unit[0].mul_add(
        first.pos[0],
        unit[1].mul_add(first.pos[1], unit[2] * first.pos[2]),
    ));
    for vertex in &range.vertices {
        let distance = unit[0].mul_add(
            vertex.pos[0],
            unit[1].mul_add(vertex.pos[1], unit[2].mul_add(vertex.pos[2], offset)),
        );
        if !distance.is_finite() || distance.abs() > PLANE_MERGE_EPS {
            return None;
        }
    }
    Some((unit, offset))
}

/// Finds or adds the plane, returning its index.
fn merge_plane(
    planes: &mut Vec<ReflectionPlane>,
    normal: [f32; 3],
    offset: f32,
    bounds: Aabb,
) -> u16 {
    for (index, plane) in planes.iter_mut().enumerate() {
        let same_normal = plane
            .normal
            .iter()
            .zip(normal)
            .all(|(left, right)| (left - right).abs() <= PLANE_MERGE_EPS);
        if same_normal && (plane.offset - offset).abs() <= PLANE_MERGE_EPS {
            plane.bounds = union_bounds(plane.bounds, bounds);
            #[allow(clippy::cast_possible_truncation)] // one level has far fewer planes
            return index as u16;
        }
    }
    planes.push(ReflectionPlane {
        normal,
        offset,
        bounds,
    });
    // `planes.len()` is at most one more than the number of planes, and a level
    // cannot produce 65 536 distinct mirror planes: the value fits a `u16`.
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    let index = (planes.len() - 1) as u16;
    index
}

/// Smallest box containing both.
#[allow(clippy::arithmetic_side_effects)] // float min/max of finite inputs
const fn union_bounds(left: Aabb, right: Aabb) -> Aabb {
    Aabb {
        min: [
            left.min[0].min(right.min[0]),
            left.min[1].min(right.min[1]),
            left.min[2].min(right.min[2]),
        ],
        max: [
            left.max[0].max(right.max[0]),
            left.max[1].max(right.max[1]),
            left.max[2].max(right.max[2]),
        ],
    }
}

/// The matrix that mirrors a world point through a plane.
///
/// `M = I - 2 * n nᵀ` with the translation `-2 d n`, so `M * p` is the
/// reflection of `p`. Applying it to a view-projection mirrors the camera
/// through the same plane: for a point *on* the plane `M * p == p`, which is
/// exactly what makes the reflected image line up with the surface.
#[must_use]
// Float-only arithmetic on finite inputs: no overflow and no panic path.
#[allow(clippy::arithmetic_side_effects)]
pub(super) fn mirror_matrix(normal: [f32; 3], offset: f32) -> glam::Mat4 {
    let [nx, ny, nz] = normal;
    let scale = -2.0_f32;
    glam::Mat4::from_cols_array_2d(&[
        [
            (scale * nx).mul_add(nx, 1.0),
            scale * nx * ny,
            scale * nx * nz,
            0.0,
        ],
        [
            scale * ny * nx,
            (scale * ny).mul_add(ny, 1.0),
            scale * ny * nz,
            0.0,
        ],
        [
            scale * nz * nx,
            scale * nz * ny,
            (scale * nz).mul_add(nz, 1.0),
            0.0,
        ],
        [
            scale * nx * offset,
            scale * ny * offset,
            scale * nz * offset,
            1.0,
        ],
    ])
}

/// Mirrors one point through a plane.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // glam float math
pub(super) fn mirror_point(normal: [f32; 3], offset: f32, point: [f32; 3]) -> [f32; 3] {
    let n = glam::Vec3::new(normal[0], normal[1], normal[2]);
    let p = glam::Vec3::new(point[0], point[1], point[2]);
    let reflected = p - n * (2.0 * (n.dot(p) + offset));
    [reflected.x, reflected.y, reflected.z]
}

/// A cubemap the renderer bakes once per level load.
pub(super) struct ProbeTarget {
    framebuffer: glow::Framebuffer,
    depth: glow::Renderbuffer,
    cube: glow::Texture,
    face_texels: u32,
    position: [f32; 3],
}

/// The six cubemap faces, in the order the renderer bakes them.
///
/// The look directions and up vectors follow the standard OpenGL cubemap face
/// layout, which is what makes a `textureCube` lookup of a reflected vector
/// fetch the part of the room that vector points at. `up` is deliberately not
/// `+Y` for the four vertical faces: each face's image must be oriented the way
/// the cube lookup expects, not the way a normal camera would shoot it.
pub(super) const CUBE_FACES: [(u32, [f32; 3], [f32; 3]); 6] = [
    // (face, look direction, up)
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_X,
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_X,
        [-1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_Y,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Y,
        [0.0, -1.0, 0.0],
        [0.0, 0.0, -1.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_Z,
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Z,
        [0.0, 0.0, -1.0],
        [0.0, -1.0, 0.0],
    ),
];

impl ProbeTarget {
    /// Creates a probe cubemap and its framebuffer at the profile's face size.
    ///
    /// # Errors
    ///
    /// Returns a message when the texture, framebuffer or depth renderbuffer
    /// cannot be created or the framebuffer is not complete.
    pub(super) unsafe fn create(
        gl: &glow::Context,
        profile: QualityProfile,
        position: [f32; 3],
    ) -> Result<Self, String> {
        let face_texels = match profile {
            QualityProfile::Full => PROBE_FACE_TEXELS_FULL,
            QualityProfile::Low => PROBE_FACE_TEXELS_LOW,
        };
        let side = i32::try_from(face_texels).unwrap_or(i32::MAX);
        let cube = unsafe { gl.create_texture()? };
        let framebuffer = match unsafe { gl.create_framebuffer() } {
            Ok(framebuffer) => framebuffer,
            Err(error) => {
                unsafe { gl.delete_texture(cube) };
                return Err(error);
            }
        };
        let depth = match unsafe { gl.create_renderbuffer() } {
            Ok(depth) => depth,
            Err(error) => {
                unsafe {
                    gl.delete_framebuffer(framebuffer);
                    gl.delete_texture(cube);
                }
                return Err(error);
            }
        };
        let target = Self {
            framebuffer,
            depth,
            cube,
            face_texels,
            position,
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(cube));
            for (face, ..) in CUBE_FACES {
                gl.tex_image_2d(
                    face,
                    0,
                    glow::RGBA8.cast_signed(),
                    side,
                    side,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR.cast_signed(),
            );
            gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, side, side);
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            // Attach one face and check completeness once: every face has the
            // same format and size, so one test covers all six.
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_CUBE_MAP_POSITIVE_X,
                Some(cube),
                0,
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if !complete {
                target.destroy(gl);
                return Err("reflection probe framebuffer is not complete".to_string());
            }
        }
        Ok(target)
    }

    /// Binds one face as the draw target.
    pub(super) unsafe fn bind_face(&self, gl: &glow::Context, face: u32) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                face,
                Some(self.cube),
                0,
            );
        }
    }

    /// The cubemap texture for the world pass to sample.
    pub(super) const fn cube(&self) -> glow::Texture {
        self.cube
    }

    /// World position the probe was baked from.
    pub(super) const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Side of one face, in texels.
    pub(super) const fn face_texels(&self) -> u32 {
        self.face_texels
    }

    /// Deletes every GL object this probe owns.
    pub(super) unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_renderbuffer(self.depth);
            gl.delete_texture(self.cube);
        }
    }
}

/// The planar reflection target: a half-resolution colour+depth view of the
/// level seen through the active mirror plane.
pub(super) struct PlanarTarget {
    target: ColorTarget,
    framebuffer: glow::Framebuffer,
    depth: glow::Renderbuffer,
    size: DrawableSize,
}

impl PlanarTarget {
    /// Creates a half-resolution colour+depth target for `scene_size`.
    ///
    /// # Errors
    ///
    /// Returns a message when any GL object cannot be created or the framebuffer
    /// is not complete.
    pub(super) unsafe fn create(
        gl: &glow::Context,
        scene_size: DrawableSize,
    ) -> Result<Self, String> {
        let size = planar_target_size(scene_size);
        if size.is_empty() {
            return Err("refusing to create a zero-sized reflection target".to_string());
        }
        let target = unsafe { ColorTarget::create(gl, size, true)? };
        let framebuffer = match unsafe { gl.create_framebuffer() } {
            Ok(framebuffer) => framebuffer,
            Err(error) => {
                unsafe { target.destroy(gl) };
                return Err(error);
            }
        };
        let depth = match unsafe { gl.create_renderbuffer() } {
            Ok(depth) => depth,
            Err(error) => {
                unsafe {
                    gl.delete_framebuffer(framebuffer);
                    target.destroy(gl);
                }
                return Err(error);
            }
        };
        let planar = Self {
            target,
            framebuffer,
            depth,
            size,
        };
        let width = i32::try_from(size.width).unwrap_or(i32::MAX);
        let height = i32::try_from(size.height).unwrap_or(i32::MAX);
        let complete = unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(planar.color()),
                0,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, width, height);
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            complete
        };
        if !complete {
            unsafe { planar.destroy(gl) };
            return Err("planar reflection framebuffer is not complete".to_string());
        }
        Ok(planar)
    }

    /// Binds this target as the draw target at its own viewport.
    pub(super) unsafe fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.viewport(
                0,
                0,
                i32::try_from(self.size.width).unwrap_or(i32::MAX),
                i32::try_from(self.size.height).unwrap_or(i32::MAX),
            );
        }
    }

    /// The colour attachment for the world pass to sample.
    pub(super) const fn color(&self) -> glow::Texture {
        self.target.color()
    }

    /// Deletes every GL object this target owns.
    pub(super) unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.target.destroy(gl);
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_renderbuffer(self.depth);
        }
    }
}

/// Size of the planar reflection target for a scene target of `scene_size`.
#[must_use]
#[allow(clippy::arithmetic_side_effects)] // integer division by a small constant
pub(super) fn planar_target_size(scene_size: DrawableSize) -> DrawableSize {
    if scene_size.is_empty() {
        return scene_size;
    }
    let divisor = PLANAR_REFLECTION_SCALE_DIVISOR.max(1);
    // Integer division by a small constant: no overflow, no panic.
    #[allow(clippy::arithmetic_side_effects)]
    DrawableSize::new(
        (scene_size.width / divisor).max(1),
        (scene_size.height / divisor).max(1),
    )
}

/// Everything the renderer keeps between frames for reflections.
pub(super) struct Reflections {
    /// Where each reflective material reads from.
    pub(super) routing: ReflectionRouting,
    /// Baked probe cubemaps, at most [`MAX_REFLECTION_PROBES`].
    probes: Vec<ProbeTarget>,
    /// The planar reflection target, created lazily for the current scene size.
    planar: Option<PlanarTarget>,
    /// Whether the profile allows the planar pass at all.
    planar_enabled: bool,
    /// Whether any reflection may run this session (`LIMINAL_NO_REFLECTIONS`).
    enabled: bool,
}

impl Default for Reflections {
    fn default() -> Self {
        Self {
            routing: ReflectionRouting::default(),
            probes: Vec::new(),
            planar: None,
            planar_enabled: true,
            enabled: true,
        }
    }
}

impl Reflections {
    /// Applies the quality profile's reflection budget.
    ///
    /// Low keeps the probes (they cost one texture read) and drops the planar
    /// pass (it costs a whole extra view of the level).
    pub(super) const fn set_profile(&mut self, profile: QualityProfile) {
        self.planar_enabled = match profile {
            QualityProfile::Full => true,
            QualityProfile::Low => false,
        };
    }

    /// Turns every reflection source off for the session.
    pub(super) const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the planar pass may run.
    pub(super) const fn planar_enabled(&self) -> bool {
        self.planar_enabled
    }

    /// The probe cubemaps, in bake order.
    pub(super) fn probes(&self) -> &[ProbeTarget] {
        &self.probes
    }

    /// How many probes the routing wants, capped by the renderer's budget.
    #[must_use]
    pub(super) fn wanted_probe_count(&self) -> usize {
        if !self.enabled {
            return 0;
        }
        self.routing.probe_points.len().min(MAX_REFLECTION_PROBES)
    }

    /// Replaces the routing and drops every baked probe: the geometry it was
    /// baked for no longer exists.
    pub(super) unsafe fn set_routing(&mut self, gl: &glow::Context, routing: ReflectionRouting) {
        unsafe { self.clear_probes(gl) };
        self.routing = routing;
    }

    /// Deletes every baked probe.
    pub(super) unsafe fn clear_probes(&mut self, gl: &glow::Context) {
        for probe in self.probes.drain(..) {
            unsafe { probe.destroy(gl) };
        }
    }

    /// Adds one baked probe.
    pub(super) fn push_probe(&mut self, probe: ProbeTarget) {
        if self.probes.len() < MAX_REFLECTION_PROBES {
            self.probes.push(probe);
        }
    }

    /// The probe a material draws with, chosen by the camera's distance.
    pub(super) fn probe_texture(&self, camera: [f32; 3]) -> Option<glow::Texture> {
        if !self.enabled {
            return None;
        }
        let mut best: Option<(f32, glow::Texture)> = None;
        for probe in &self.probes {
            let position = probe.position();
            let delta = glam::Vec3::new(
                position[0] - camera[0],
                position[1] - camera[1],
                position[2] - camera[2],
            );
            let distance = delta.length_squared();
            if best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, probe.cube()));
            }
        }
        best.map(|(_, cube)| cube)
    }

    /// Ensures the planar target matches `scene_size`, creating it on demand.
    ///
    /// Returns whether the planar pass can run. A context that refuses the
    /// target disables the pass for the session rather than retrying every
    /// frame.
    pub(super) unsafe fn ensure_planar(
        &mut self,
        gl: &glow::Context,
        scene_size: DrawableSize,
    ) -> bool {
        if !self.enabled || !self.planar_enabled || self.routing.planes.is_empty() {
            return false;
        }
        let wanted = planar_target_size(scene_size);
        if self
            .planar
            .as_ref()
            .is_some_and(|planar| planar.size == wanted)
        {
            return true;
        }
        if let Some(planar) = self.planar.take() {
            unsafe { planar.destroy(gl) };
        }
        match unsafe { PlanarTarget::create(gl, scene_size) } {
            Ok(planar) => {
                self.planar = Some(planar);
                true
            }
            Err(_error) => {
                // Reported by the one-line diagnostic the caller prints; a
                // context that cannot give us a reflection target simply loses
                // the planar pass for the session.
                self.planar_enabled = false;
                false
            }
        }
    }

    /// The planar target, if one is resident.
    pub(super) const fn planar(&self) -> Option<&PlanarTarget> {
        self.planar.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mirror_matrix_reflects_points_and_fixes_the_plane() {
        // The pool deck: y = -1.5, normal up.
        let matrix = mirror_matrix([0.0, 1.0, 0.0], 1.5);
        let point = glam::Vec4::new(2.0, 0.5, -3.0, 1.0);
        let mirrored = matrix * point;
        assert!((mirrored.x - 2.0).abs() < 1.0e-5);
        assert!((mirrored.y + 3.5).abs() < 1.0e-5);
        assert!((mirrored.z + 3.0).abs() < 1.0e-5);
        // A point on the plane is unchanged, which is what makes the reflected
        // image line up with the surface it is sampled on.
        let on_plane = glam::Vec4::new(7.0, -1.5, 4.0, 1.0);
        let fixed = matrix * on_plane;
        assert!((fixed.x - on_plane.x).abs() < 1.0e-5);
        assert!((fixed.y - on_plane.y).abs() < 1.0e-5);
        assert!((fixed.z - on_plane.z).abs() < 1.0e-5);
    }

    #[test]
    fn mirroring_twice_is_the_identity() {
        let normal = [0.0, 0.6, -0.8];
        let offset = -2.25;
        let point = [3.0, 1.0, 5.0];
        let once = mirror_point(normal, offset, point);
        let twice = mirror_point(normal, offset, once);
        for (left, right) in point.iter().zip(twice) {
            assert!(
                (left - right).abs() < 1.0e-4,
                "{twice:?} is not the identity"
            );
        }
    }

    #[test]
    fn a_planar_distance_is_signed() {
        let plane = ReflectionPlane {
            normal: [0.0, 1.0, 0.0],
            offset: 1.5,
            bounds: Aabb::default(),
        };
        assert!(plane.distance([0.0, -1.5, 0.0]).abs() < 1.0e-6);
        assert!(plane.distance([0.0, 0.0, 0.0]) > 0.0);
        assert!(plane.distance([0.0, -3.0, 0.0]) < 0.0);
    }

    #[test]
    fn the_planar_target_is_half_the_scene() {
        assert_eq!(
            planar_target_size(DrawableSize::new(960, 544)),
            DrawableSize::new(480, 272)
        );
        assert_eq!(
            planar_target_size(DrawableSize::new(1, 1)),
            DrawableSize::new(1, 1)
        );
        assert_eq!(
            planar_target_size(DrawableSize::new(0, 0)),
            DrawableSize::new(0, 0)
        );
    }

    #[test]
    fn low_disables_the_planar_pass_and_full_enables_it() {
        let mut reflections = Reflections::default();
        reflections.set_profile(QualityProfile::Low);
        assert!(!reflections.planar_enabled());
        reflections.set_profile(QualityProfile::Full);
        assert!(reflections.planar_enabled());
    }
}

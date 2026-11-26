//! Geometry-derived reflection routing and capture integration.

use super::mesh::LevelMesh;
use super::view::{DrawableSize, PLANAR_REFLECTION_SCALE_DIVISOR};
use crate::engine::materials::MaterialReflection;
use crate::engine::quality::ReflectionQuality;
use crate::engine::spatial::Frustum;

pub use crate::engine::lighting::reflections::{ReflectionPlane, ReflectionRouting, nearest_probe};
use crate::engine::lighting::reflections::{ReflectionSurface, route_surfaces};

/// Convert existing draw ranges to lighting inputs without copying any vertices.
#[must_use]
pub fn routing_from_mesh(
    mesh: &LevelMesh,
    reflections: &[MaterialReflection],
    material_count: usize,
) -> ReflectionRouting {
    route_surfaces(
        mesh.ranges.iter().map(|range| ReflectionSurface {
            material: range.key.material,
            bounds: range.bounds,
            normal: range.vertices.first().map(|vertex| vertex.normal),
            positions: range.vertices.iter().map(|vertex| vertex.pos),
        }),
        reflections,
        material_count,
    )
}

/// The renderer supplies view-dependent culling for the lighting selection.
#[must_use]
pub fn nearest_visible_reflection_plane(
    routing: &ReflectionRouting,
    camera: [f32; 3],
    frustum: &Frustum,
    occlusion: &super::occlusion::OcclusionView,
) -> Option<usize> {
    crate::engine::lighting::reflections::nearest_visible_plane(routing, camera, |bounds| {
        frustum.intersects_aabb(bounds) && !occlusion.hides(bounds)
    })
}

/// The matrix that mirrors a world point through a plane.
///
/// `M = I - 2 * n nᵀ` with the translation `-2 d n`, so `M * p` is the
/// reflection of `p`. Applying it to a view-projection mirrors the camera
/// through the same plane: for a point *on* the plane `M * p == p`, which is
/// exactly what makes the reflected image line up with the surface.
#[must_use]
// Float-only arithmetic on finite inputs: no overflow and no panic path.
pub fn mirror_matrix(normal: [f32; 3], offset: f32) -> glam::Mat4 {
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
#[expect(clippy::arithmetic_side_effects, reason = "glam float math")] // glam float math
pub fn mirror_point(normal: [f32; 3], offset: f32, point: [f32; 3]) -> [f32; 3] {
    let n = glam::Vec3::new(normal[0], normal[1], normal[2]);
    let p = glam::Vec3::new(point[0], point[1], point[2]);
    let reflected = p - n * (2.0 * (n.dot(p) + offset));
    [reflected.x, reflected.y, reflected.z]
}

/// Size of the planar reflection target for a scene target of `scene_size`.
#[must_use]
// integer division by a small constant
pub fn planar_target_size(scene_size: DrawableSize) -> DrawableSize {
    if scene_size.is_empty() {
        return scene_size;
    }
    let divisor = PLANAR_REFLECTION_SCALE_DIVISOR.max(1);
    // Integer division by a small constant: no overflow, no panic.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Integer division by a small constant: no overflow, no panic."
    )]
    DrawableSize::new(
        (scene_size.width / divisor).max(1),
        (scene_size.height / divisor).max(1),
    )
}

/// Everything the renderer keeps between frames for reflections that is not a
/// GPU resource: where each reflective material reads from and whether the
/// reflection passes may run at all.
#[derive(Clone, Debug)]
pub struct Reflections {
    /// Where each reflective material reads from.
    pub routing: ReflectionRouting,
    /// Whether the level allows the planar pass at all.
    planar_enabled: bool,
    /// Whether any reflection may run this session (`PLACES_NO_REFLECTIONS`).
    enabled: bool,
}

impl Default for Reflections {
    /// Reflections start fully enabled: the settings and the quality level
    /// apply their own switches before the first frame, and starting disabled
    /// would silently drop a pass that nothing re-enabled
    /// (`apply_graphics` early-returns on an unchanged configuration).
    fn default() -> Self {
        Self {
            routing: ReflectionRouting::default(),
            planar_enabled: true,
            enabled: true,
        }
    }
}

impl Reflections {
    /// Applies the Reflections setting's gates.
    ///
    /// This records only the allow/gate decisions: the GPU probe cubemaps and
    /// the planar target are the renderer's (`ReflectionTargets`), retired and
    /// created there. Off disables both sources so no capture work can run;
    /// Medium and Full allow the planar pass.
    pub const fn set_quality(&mut self, quality: ReflectionQuality) {
        self.planar_enabled = quality.draws_planar();
        self.enabled = quality.draws_probes();
    }

    /// Whether the planar pass should run: enabled, profile-permitted and the
    /// level actually produced a mirror plane.
    #[must_use]
    pub const fn planar_wanted(&self) -> bool {
        self.enabled && self.planar_enabled && !self.routing.planes.is_empty()
    }
}

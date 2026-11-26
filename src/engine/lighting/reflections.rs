//! Reflected-radiance source eligibility, ordering and geometry-derived routing.

use crate::engine::materials::{MaterialReflection, ReflectionMode};
use crate::engine::spatial::Aabb;

/// Bound the number of distinct probe capture locations in a level.
pub const MAX_REFLECTION_PROBES: usize = 2;

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
pub struct ReflectionPlane {
    /// Unit normal, pointing out of the surface.
    pub normal: [f32; 3],
    /// Offset such that a point on the plane satisfies `dot(normal, p) + offset == 0`.
    pub offset: f32,
    /// Bounding box of every reflective batch on this plane, for the frustum test.
    pub bounds: Aabb,
}

impl ReflectionPlane {
    /// Signed distance from the plane to `point`.
    #[must_use]
    pub const fn distance(&self, point: [f32; 3]) -> f32 {
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
pub struct ReflectionRouting {
    /// The distinct mirror planes the level's geometry produced.
    pub planes: Vec<ReflectionPlane>,
    /// Per mesh range: the plane index its planar reflection reads, if any.
    ///
    /// Parallel to the original mesh ranges. The plane is a property of the
    /// surface, so a material used on two different planes keeps both routes
    /// instead of the last one overwriting the first.
    pub plane_for_range: Vec<Option<u16>>,
    /// Per material index: whether its reflection is probe-based.
    pub probe_for_material: Vec<bool>,
    /// Centroid of every probe-reflective surface, in world metres.
    pub probe_points: Vec<[f32; 3]>,
}

impl ReflectionRouting {
    /// The plane one mesh range reflects on, if it is a planar material.
    #[must_use]
    pub fn plane_of_range(&self, range: usize) -> Option<usize> {
        self.plane_for_range
            .get(range)
            .copied()
            .flatten()
            .map(usize::from)
    }
}

pub struct ReflectionSurface<I> {
    pub material: u32,
    pub bounds: Aabb,
    pub normal: Option<[f32; 3]>,
    pub positions: I,
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
// one line per malformed marking, at level load only
#[must_use]
pub fn route_surfaces<I: Iterator<Item = [f32; 3]>>(
    surfaces: impl ExactSizeIterator<Item = ReflectionSurface<I>>,
    reflections: &[MaterialReflection],
    material_count: usize,
) -> ReflectionRouting {
    let mut routing = ReflectionRouting {
        plane_for_range: vec![None; surfaces.len()],
        probe_for_material: vec![false; material_count],
        ..ReflectionRouting::default()
    };
    // Probe-reflective geometry is clustered by distance, so a level with
    // polished floors in two rooms gets two probes instead of one point in the
    // wall between them. Each cluster's representative is its area-weighted
    // centroid.
    let mut clusters: Vec<ProbeCluster> = Vec::new();
    for (range_index, range) in surfaces.enumerate() {
        let material = usize::try_from(range.material).unwrap_or(usize::MAX);
        let Some(reflection) = reflections.get(material).copied() else {
            continue;
        };
        if !reflection.is_active() {
            continue;
        }
        if reflection.mode == ReflectionMode::Probe {
            if let Some(centre) = range_centre(range.bounds) {
                let size = range.bounds.max;
                let span = [
                    (size[0] - range.bounds.min[0]).max(0.0),
                    (size[1] - range.bounds.min[1]).max(0.0),
                    (size[2] - range.bounds.min[2]).max(0.0),
                ];
                // The reflective area of a thin surface: a floor patch is wide
                // and flat, a panel is wide and tall. Taking the largest pair of
                // extents keeps both from vanishing — including a wall whose
                // thin axis is X or Y, which is why all three pairs are needed.
                let area = (span[0] * span[2])
                    .max(span[0] * span[1])
                    .max(span[1] * span[2]);
                add_probe_sample(&mut clusters, centre, area);
                if let Some(entry) = routing.probe_for_material.get_mut(material) {
                    *entry = true;
                }
            }
            continue;
        }
        let Some((normal, offset)) = range_plane(range.normal, range.positions) else {
            crate::engine::logging::warn_once(
                format!("planar-not-planar:{material}"),
                format!(
                    "[reflections] material {material} marks a planar reflection but its \
                     geometry is not planar; skipping that range"
                ),
            );
            continue;
        };
        let Some(index) = merge_plane(&mut routing.planes, normal, offset, range.bounds) else {
            crate::engine::logging::warn_once(
                "planar-route-capacity",
                "[reflections] too many distinct mirror planes for u16 routes; skipping that range"
                    .to_string(),
            );
            continue;
        };
        if let Some(entry) = routing.plane_for_range.get_mut(range_index) {
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
// float-only, finite inputs
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
fn range_centre(bounds: Aabb) -> Option<[f32; 3]> {
    let min = bounds.min;
    let max = bounds.max;
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
fn range_plane(
    first_normal: Option<[f32; 3]>,
    mut positions: impl Iterator<Item = [f32; 3]>,
) -> Option<([f32; 3], f32)> {
    let first = positions.next()?;
    let normal = first_normal?;
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
    let offset = -(unit[0].mul_add(first[0], unit[1].mul_add(first[1], unit[2] * first[2])));
    for position in std::iter::once(first).chain(positions) {
        let distance = unit[0].mul_add(
            position[0],
            unit[1].mul_add(position[1], unit[2].mul_add(position[2], offset)),
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
) -> Option<u16> {
    for (index, plane) in planes.iter_mut().enumerate() {
        let same_normal = plane
            .normal
            .iter()
            .zip(normal)
            .all(|(left, right)| (left - right).abs() <= PLANE_MERGE_EPS);
        if same_normal && (plane.offset - offset).abs() <= PLANE_MERGE_EPS {
            let route = u16::try_from(index).ok()?;
            plane.bounds = union_bounds(plane.bounds, bounds);
            return Some(route);
        }
    }
    let route = u16::try_from(planes.len()).ok()?;
    planes.push(ReflectionPlane {
        normal,
        offset,
        bounds,
    });
    Some(route)
}

/// Smallest box containing both.
// float min/max of finite inputs
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

/// The nearest mirror plane whose reflective geometry is on screen.
///
/// At most one plane is reflected per frame: the nearest one whose reflective
/// geometry survived the frustum test. Distance is measured to the geometry's
/// finite bounds: a distant floor must not displace a nearby wall mirror just
/// because the floor's infinite plane extends beneath the camera.
#[must_use]
pub fn nearest_visible_plane(
    routing: &ReflectionRouting,
    camera: [f32; 3],
    is_visible: impl Fn(&Aabb) -> bool,
) -> Option<usize> {
    let eye = glam::Vec3::from_array(camera);
    let mut best: Option<(f32, usize)> = None;
    for (index, plane) in routing.planes.iter().enumerate() {
        if !is_visible(&plane.bounds) {
            continue;
        }
        let closest = eye.clamp(
            glam::Vec3::from_array(plane.bounds.min),
            glam::Vec3::from_array(plane.bounds.max),
        );
        let distance = eye.distance_squared(closest);
        if !distance.is_finite() {
            continue;
        }
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, index));
        }
    }
    best.map(|(_, index)| index)
}

/// The nearest baked probe to the camera, by squared distance.
///
/// Ties keep the earlier probe in bake order, which is the deterministic
/// tie-break the draw path has always used.
#[must_use]
pub fn nearest_probe(positions: &[[f32; 3]], camera: [f32; 3]) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (index, position) in positions.iter().enumerate() {
        let delta = glam::Vec3::new(
            position[0] - camera[0],
            position[1] - camera[1],
            position[2] - camera[2],
        );
        let distance = delta.length_squared();
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, index));
        }
    }
    best.map(|(_, index)| index)
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

/// The material's authored specular factor weights each channel separately.
pub fn weighted_strength(specular: [f32; 3], reflection: MaterialReflection) -> [f32; 3] {
    specular.map(|channel| channel * reflection.strength)
}

pub fn material_eligible(specular: [f32; 3], reflection: MaterialReflection) -> bool {
    reflection.is_active()
        && weighted_strength(specular, reflection)
            .iter()
            .any(|c| *c > 0.0)
}

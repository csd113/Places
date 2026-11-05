//! Conservative, current-view occlusion by actual opaque architecture.
//!
//! No room inference, previous-frame answers or dynamic occluders: a box is
//! hidden only if every point lies strictly behind one solid triangle and
//! inside its eye-to-edge cone. Holes between triangles stay holes. Bounded
//! candidate limits can only miss opportunities, never hide extra content.

use glam::{Mat4, Vec3, Vec4};

use crate::engine::spatial::Aabb;

const MAX_OCCLUDERS: usize = 512;
const MAX_VIEW_OCCLUDERS: usize = 24;
const MAX_WATER_OCCLUDERS: usize = 8;
const MARGIN: f32 = 0.01;

#[derive(Clone, Copy, Debug)]
struct Triangle {
    points: [Vec3; 3],
    area: f32,
}

/// Immutable, level-owned subset of solid architectural triangles.
#[derive(Default)]
pub struct Occluders {
    triangles: Vec<Triangle>,
    water_quads: Vec<[Vec3; 4]>,
    water_enabled: bool,
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite vector geometry uses floating-point arithmetic only"
)]
impl Occluders {
    /// The caller supplies only triangles that always write opaque depth.
    pub fn new(triangles: impl Iterator<Item = [[f32; 3]; 3]>) -> Self {
        let mut selected = Vec::new();
        let mut water_quads = Vec::new();
        let mut previous: Option<[Vec3; 3]> = None;
        for vertices in triangles {
            let points = vertices.map(Vec3::from_array);
            let [a, b, c] = points;
            let area = (b - a).cross(c - a).length();
            if points
                .iter()
                .any(|point| !point.is_finite() || point.abs().max_element() > 100_000.0)
                || !area.is_finite()
                || area < 0.5
            {
                continue;
            }
            if let Some(before) = previous
                && let Some(quad) = joined_quad(before, points)
            {
                store_water_quad(&mut water_quads, quad);
                if water_quads.len() == MAX_OCCLUDERS.saturating_mul(2) {
                    water_quads.truncate(MAX_OCCLUDERS);
                }
            }
            previous = Some(points);
            selected.push(Triangle { points, area });
            // Bound temporary storage too. Retain the largest useful surfaces;
            // stable ties preserve the mesh's deterministic order.
            if selected.len() == MAX_OCCLUDERS.saturating_mul(2) {
                selected.sort_by(|left, right| right.area.total_cmp(&left.area));
                selected.truncate(MAX_OCCLUDERS);
            }
        }
        selected.sort_by(|left, right| right.area.total_cmp(&left.area));
        selected.truncate(MAX_OCCLUDERS);
        Self {
            triangles: selected,
            water_quads,
            water_enabled: false,
        }
    }

    /// Only installed water geometry enables the extra coverage proof. Normal
    /// scene boxes continue to use exactly the original triangle policy.
    pub const fn enable_water_quads(&mut self) {
        self.water_enabled = true;
    }

    /// Build fresh cones for this camera, including a mirrored camera. Exclude
    /// triangles touching/crossing the near plane: clipped geometry cannot
    /// prove coverage. No occluder may hide its own plane or coplanar decals.
    pub fn view(&self, eye: Vec3, matrix: Mat4) -> OcclusionView {
        self.clipped_view(eye, matrix, Vec4::ZERO)
    }

    /// A planar half-space can discard architectural backing or cut a triangle.
    /// Such triangles cannot prove coverage. Keep ordinary-view selection intact.
    pub fn clipped_view(&self, eye: Vec3, matrix: Mat4, clip_plane: Vec4) -> OcclusionView {
        if !clip_plane.is_finite() {
            return OcclusionView::EMPTY;
        }
        let mut result = OcclusionView::EMPTY;
        let mut scores = [0.0_f32; MAX_VIEW_OCCLUDERS];
        for triangle in &self.triangles {
            if clip_plane != Vec4::ZERO
                && triangle
                    .points
                    .iter()
                    .any(|point| clip_plane.dot(point.extend(1.0)) <= MARGIN)
            {
                continue;
            }
            if triangle.points.iter().any(|point| {
                let clip = matrix * point.extend(1.0);
                !clip.is_finite() || clip.w <= 0.0 || clip.z <= MARGIN
            }) {
                continue;
            }
            let [a, b, c] = triangle.points;
            let Some(mut normal) = (b - a).cross(c - a).try_normalize() else {
                continue;
            };
            if normal.dot(eye - a) > 0.0 {
                normal = -normal;
            }
            if normal.dot(a - eye) <= MARGIN {
                continue;
            }
            let center = (a + b + c) / 3.0;
            let mut planes = [Vec4::ZERO; 4];
            let mut valid = true;
            for (slot, (first, last)) in planes.iter_mut().zip([(a, b), (b, c), (c, a)]) {
                let Some(mut side) = (first - eye).cross(last - eye).try_normalize() else {
                    valid = false;
                    break;
                };
                if side.dot(center - eye) < 0.0 {
                    side = -side;
                }
                *slot = side.extend(-side.dot(eye));
            }
            if !valid {
                continue;
            }
            if let Some(depth) = planes.last_mut() {
                *depth = normal.extend(-normal.dot(a));
            }
            let score = triangle.area / center.distance_squared(eye).max(0.01);
            let target = if result.count < MAX_VIEW_OCCLUDERS {
                let next = result.count;
                result.count = result.count.saturating_add(1);
                next
            } else {
                let Some((worst, value)) = scores
                    .iter()
                    .enumerate()
                    .min_by(|left, right| left.1.total_cmp(right.1))
                else {
                    continue;
                };
                if score <= *value {
                    continue;
                }
                worst
            };
            if let (Some(slot), Some(weight)) =
                (result.cones.get_mut(target), scores.get_mut(target))
            {
                *slot = planes;
                *weight = score;
            }
        }
        if self.water_enabled {
            self.add_water_cones(&mut result, eye, matrix, clip_plane);
        }
        result
    }
    fn add_water_cones(
        &self,
        result: &mut OcclusionView,
        eye: Vec3,
        matrix: Mat4,
        clip_plane: Vec4,
    ) {
        let mut weights = [0.0_f32; MAX_WATER_OCCLUDERS];
        for points in &self.water_quads {
            if clip_plane != Vec4::ZERO
                && points
                    .iter()
                    .any(|point| clip_plane.dot(point.extend(1.0)) <= MARGIN)
            {
                continue;
            }
            let Some(planes) = quad_cone(*points, eye, matrix) else {
                continue;
            };
            let center = points.iter().copied().sum::<Vec3>() * 0.25;
            let area = (points[1] - points[0])
                .cross(points[3] - points[0])
                .length();
            let score = area / center.distance_squared(eye).max(0.01);
            let target = if result.water_count < MAX_WATER_OCCLUDERS {
                let next = result.water_count;
                result.water_count = result.water_count.saturating_add(1);
                next
            } else {
                let Some((worst, value)) = weights
                    .iter()
                    .enumerate()
                    .min_by(|left, right| left.1.total_cmp(right.1))
                else {
                    continue;
                };
                if score <= *value {
                    continue;
                }
                worst
            };
            if let (Some(slot), Some(weight)) =
                (result.water_cones.get_mut(target), weights.get_mut(target))
            {
                *slot = planes;
                *weight = score;
            }
        }
    }
}

/// Fixed scratch for clipping a triangle/quad against convex view planes.
/// Invalid or exhausted scratch cannot prove invisibility.
pub struct ClippedPolygon {
    points: [Vec3; 12],
    count: usize,
}

impl ClippedPolygon {
    pub fn points(&self) -> &[Vec3] {
        self.points.get(..self.count).unwrap_or_default()
    }
}

pub fn clip_polygon(points: &[Vec3], planes: &[Vec4]) -> Option<ClippedPolygon> {
    if points.len() > 12
        || points.iter().any(|point| !point.is_finite())
        || planes.iter().any(|plane| !plane.is_finite())
    {
        return None;
    }
    let mut polygon = ClippedPolygon {
        points: [Vec3::ZERO; 12],
        count: points.len(),
    };
    polygon
        .points
        .get_mut(..points.len())?
        .copy_from_slice(points);
    for plane in planes {
        let mut clipped = [Vec3::ZERO; 12];
        let mut count = 0_usize;
        let Some(mut previous) = polygon.points().last().copied() else {
            break;
        };
        let mut before = plane.dot(previous.extend(1.0)) + 0.00001;
        if !before.is_finite() {
            return None;
        }
        for point in polygon.points() {
            let distance = plane.dot(point.extend(1.0)) + 0.00001;
            if !distance.is_finite() {
                return None;
            }
            if (before >= 0.0) != (distance >= 0.0) {
                *clipped.get_mut(count)? = previous.lerp(*point, before / (before - distance));
                count = count.saturating_add(1);
            }
            if distance >= 0.0 {
                *clipped.get_mut(count)? = *point;
                count = count.saturating_add(1);
            }
            previous = *point;
            before = distance;
        }
        polygon = ClippedPolygon {
            points: clipped,
            count,
        };
    }
    Some(polygon)
}

/// Two adjacent opaque triangles may prove a convex quad's coverage without
/// inventing a seam hole or filling a real opening. Exact shared vertices,
/// coplanarity and consistent convex turns are all required at installation.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite vector geometry uses floating-point arithmetic only"
)]
fn joined_quad(first: [Vec3; 3], second: [Vec3; 3]) -> Option<[Vec3; 4]> {
    let shared: Vec<_> = first
        .iter()
        .filter(|point| second.contains(point))
        .copied()
        .collect();
    let [shared_first, shared_last] = shared.as_slice() else {
        return None;
    };
    let outer_first = *first.iter().find(|point| !second.contains(point))?;
    let outer_last = *second.iter().find(|point| !first.contains(point))?;
    let quad = [outer_first, *shared_first, outer_last, *shared_last];
    let normal = (first[1] - first[0])
        .cross(first[2] - first[0])
        .try_normalize()?;
    if normal.dot(outer_last - first[0]).abs() > 0.00001 {
        return None;
    }
    let [q0, q1, q2, q3] = quad;
    let turns = [
        (q1 - q0).cross(q2 - q1).dot(normal),
        (q2 - q1).cross(q3 - q2).dot(normal),
        (q3 - q2).cross(q0 - q3).dot(normal),
        (q0 - q3).cross(q1 - q0).dot(normal),
    ];
    (turns.iter().all(|turn| *turn > 0.00001) || turns.iter().all(|turn| *turn < -0.00001))
        .then_some(quad)
}

/// Remove artificial seams only where the exact union remains a rectangle.
/// Exact corners and matching spans are required: even a tiny real gap, an
/// L-shaped union or a different plane cannot be filled by this preparation.
fn store_water_quad(quads: &mut Vec<[Vec3; 4]>, mut quad: [Vec3; 4]) {
    let mut index = 0;
    while let Some(other) = quads.get(index) {
        if let Some(joined) = joined_rectangles(quad, *other) {
            quad = joined;
            let _removed = quads.remove(index);
            index = 0;
        } else {
            index = index.saturating_add(1);
        }
    }
    quads.push(quad);
}

fn rectangle_bounds(points: [Vec3; 4]) -> Option<(Vec3, Vec3)> {
    let min = points
        .iter()
        .fold(Vec3::INFINITY, |value, point| value.min(*point));
    let max = points
        .iter()
        .fold(Vec3::NEG_INFINITY, |value, point| value.max(*point));
    if min.cmpeq(max).bitmask().count_ones() != 1
        || points
            .iter()
            .any(|point| !(point.cmpeq(min) | point.cmpeq(max)).all())
        || points
            .iter()
            .enumerate()
            .any(|(index, point)| points.get(..index).unwrap_or_default().contains(point))
    {
        return None;
    }
    Some((min, max))
}

fn joined_rectangles(first: [Vec3; 4], second: [Vec3; 4]) -> Option<[Vec3; 4]> {
    let (first_min, first_max) = rectangle_bounds(first)?;
    let (second_min, second_max) = rectangle_bounds(second)?;
    let same_plane = first_min.cmpeq(first_max).bitmask();
    if same_plane != second_min.cmpeq(second_max).bitmask() {
        return None;
    }
    let joins = [Vec3::X, Vec3::Y, Vec3::Z].into_iter().any(|axis| {
        let mask = axis.cmpeq(Vec3::ZERO);
        let retain = |point| Vec3::select(mask, point, Vec3::ZERO);
        retain(first_min) == retain(second_min)
            && retain(first_max) == retain(second_max)
            && first_min.dot(axis) <= second_max.dot(axis)
            && second_min.dot(axis) <= first_max.dot(axis)
    });
    if !joins {
        return None;
    }
    let min = first_min.min(second_min);
    let max = first_max.max(second_max);
    match same_plane {
        1 => Some([
            min,
            Vec3::new(min.x, max.y, min.z),
            max,
            Vec3::new(min.x, min.y, max.z),
        ]),
        2 => Some([
            min,
            Vec3::new(max.x, min.y, min.z),
            max,
            Vec3::new(min.x, min.y, max.z),
        ]),
        4 => Some([
            min,
            Vec3::new(max.x, min.y, min.z),
            max,
            Vec3::new(min.x, max.y, min.z),
        ]),
        _ => None,
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite vector cone planes use floating-point arithmetic only"
)]
fn quad_cone(points: [Vec3; 4], eye: Vec3, matrix: Mat4) -> Option<[Vec4; 6]> {
    // A near-crossing wall cannot prove an ordinary box hidden. For water,
    // clip the opaque quad to the retained near half-space instead; only its
    // genuinely drawn convex portion contributes coverage.
    let near = matrix.transpose().z_axis - Vec4::W * MARGIN;
    let clipped = clip_polygon(&points, &[near])?;
    let polygon = clipped.points();
    if polygon.len() < 3
        || polygon.iter().any(|point| {
            let clip = matrix * point.extend(1.0);
            !clip.is_finite() || clip.w <= 0.0 || clip.z <= 0.0
        })
    {
        return None;
    }
    let mut normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .try_normalize()?;
    if normal.dot(eye - points[0]) > 0.0 {
        normal = -normal;
    }
    if normal.dot(points[0] - eye) <= MARGIN {
        return None;
    }
    let center = polygon.iter().copied().sum::<Vec3>()
        / crate::engine::render::common::count_to_f32(polygon.len());
    let mut planes = [Vec4::W * 1_000_000.0; 6];
    let next = polygon.iter().cycle().skip(1);
    for (plane, (first, last)) in planes
        .iter_mut()
        .take(5)
        .zip(polygon.iter().zip(next).take(polygon.len()))
    {
        let mut side = (*first - eye).cross(*last - eye).try_normalize()?;
        if side.dot(center - eye) < 0.0 {
            side = -side;
        }
        *plane = side.extend(-side.dot(eye));
    }
    *planes.last_mut()? = normal.extend(-normal.dot(points[0]));
    Some(planes)
}

/// Small stack-owned camera state, reused by scene and emission submissions.
#[derive(Clone, Copy, Debug)]
pub struct OcclusionView {
    cones: [[Vec4; 4]; MAX_VIEW_OCCLUDERS],
    count: usize,
    water_cones: [[Vec4; 6]; MAX_WATER_OCCLUDERS],
    water_count: usize,
}

impl OcclusionView {
    pub const EMPTY: Self = Self {
        cones: [[Vec4::ZERO; 4]; MAX_VIEW_OCCLUDERS],
        count: 0,
        water_cones: [[Vec4::ZERO; 6]; MAX_WATER_OCCLUDERS],
        water_count: 0,
    };

    /// A clipped water polygon can fit a blocker cone even when its original
    /// triangle/box extends off screen. The same strict plane margin proves
    /// coverage; ordinary scene culling keeps its existing whole-box policy.
    pub fn hides_polygon(&self, points: &[Vec3]) -> bool {
        if points.is_empty() || points.iter().any(|point| !point.is_finite()) {
            return false;
        }
        let margin = points.iter().fold(MARGIN, |value, point| {
            value.max(point.abs().max_element() * 0.00001)
        });
        let covered = |planes: &[Vec4]| {
            planes.iter().all(|plane| {
                points
                    .iter()
                    .all(|point| plane.dot(point.extend(1.0)) > margin)
            })
        };
        self.cones
            .get(..self.count)
            .unwrap_or_default()
            .iter()
            .any(|planes| covered(planes))
            || self
                .water_cones
                .get(..self.water_count)
                .unwrap_or_default()
                .iter()
                .any(|planes| covered(planes))
    }

    /// Whole-box containment in a strict convex cone proves every enclosed
    /// triangle hidden. Invalid, intersecting and edge-touching boxes survive.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Conservative plane support uses finite floating-point vector arithmetic"
    )]
    pub fn hides(&self, bounds: &Aabb) -> bool {
        let min = Vec3::from_array(bounds.min);
        let max = Vec3::from_array(bounds.max);
        if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
            return false;
        }
        let center = (min + max) * 0.5;
        let extent = (max - min) * 0.5;
        // Scale the safety margin with world coordinates to cover roundoff.
        let margin = MARGIN.max(center.abs().max_element() * 0.00001);
        self.cones
            .get(..self.count)
            .unwrap_or_default()
            .iter()
            .any(|planes| {
                planes.iter().all(|plane| {
                    plane.truncate().dot(center) + plane.w - plane.truncate().abs().dot(extent)
                        > margin
                })
            })
    }
}

#[cfg(test)]
#[path = "../../tests/occlusion.rs"]
mod tests;

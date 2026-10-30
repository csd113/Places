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
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite vector geometry uses floating-point arithmetic only"
)]
impl Occluders {
    /// The caller supplies only triangles that always write opaque depth.
    pub fn new(triangles: impl Iterator<Item = [[f32; 3]; 3]>) -> Self {
        let mut selected = Vec::new();
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
        }
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
        result
    }
}

/// Small stack-owned camera state, reused by scene and emission submissions.
#[derive(Clone, Copy, Debug)]
pub struct OcclusionView {
    cones: [[Vec4; 4]; MAX_VIEW_OCCLUDERS],
    count: usize,
}

impl OcclusionView {
    pub const EMPTY: Self = Self {
        cones: [[Vec4::ZERO; 4]; MAX_VIEW_OCCLUDERS],
        count: 0,
    };

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

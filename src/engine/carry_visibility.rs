//! Conservative retirement proof.
//!
//! A single real opaque wall
//! face must cover the projection of the entire current posed model AND prop
//! bound. Eight corners prove coverage of the convex bound, unlike torso rays
//! or a finite surface-sampling grid which can miss an exposed finger/prop.

use glam::Vec3;

use crate::engine::{collision::WallAabb, spatial::Aabb};

/// True only when one rectangular opaque wall face hides the entire bound.
///
/// `walls` must contain actual uncut opaque architecture, never collision-only
/// blockers, approximated prop boxes, transparent walls or doorway envelopes.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite validated metre bounds; all projections reject nonfinite results"
)]
pub fn assembly_hidden(eye: Vec3, assembly: Aabb, walls: &[WallAabb]) -> bool {
    if !eye.is_finite()
        || assembly.is_empty()
        || assembly
            .min
            .into_iter()
            .chain(assembly.max)
            .any(|v| !v.is_finite())
    {
        return false;
    }
    // Account for floating point coverage and the next submitted edge raster.
    let minimum = Vec3::from_array(assembly.min) - Vec3::splat(0.035);
    let maximum = Vec3::from_array(assembly.max) + Vec3::splat(0.035);
    walls.iter().any(|wall| {
        let low = Vec3::new(wall.min_x, wall.min_y, wall.min_z);
        let high = Vec3::new(wall.max_x, wall.max_y, wall.max_z);
        if !low.is_finite() || !high.is_finite() || (high.cmple(low)).any() {
            return false;
        }
        [(0usize, 1usize, 2usize), (2, 0, 1)]
            .into_iter()
            .any(|(axis, u, v)| {
                let face = if eye[axis] < low[axis] - 0.01 && minimum[axis] > high[axis] + 0.01 {
                    low[axis]
                } else if eye[axis] > high[axis] + 0.01 && maximum[axis] < low[axis] - 0.01 {
                    high[axis]
                } else {
                    return false;
                };
                (0u8..8).all(|corner| {
                    let point = Vec3::new(
                        if corner & 1 == 0 {
                            minimum.x
                        } else {
                            maximum.x
                        },
                        if corner & 2 == 0 {
                            minimum.y
                        } else {
                            maximum.y
                        },
                        if corner & 4 == 0 {
                            minimum.z
                        } else {
                            maximum.z
                        },
                    );
                    let fraction = (face - eye[axis]) / (point[axis] - eye[axis]);
                    let crossing = eye + (point - eye) * fraction;
                    crossing.is_finite()
                        && fraction > 0.0
                        && fraction < 1.0
                        && crossing[u] > low[u] + 0.015
                        && crossing[u] < high[u] - 0.015
                        && crossing[v] > low[v] + 0.015
                        && crossing[v] < high[v] - 0.015
                })
            })
    })
}

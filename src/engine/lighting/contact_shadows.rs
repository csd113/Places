//! Movable contact-shadow admission and floor-footprint ranking.

use glam::Vec3;

use super::lightmap::LightmapTexel;
use super::transport::TransportScene;
use crate::engine::level::LevelSurfaces;
use crate::engine::materials::AlphaMode;
use crate::engine::spatial::{Aabb, Frustum};

/// A soft projected bounds shadow for a movable opaque subject. This is a
/// floor receiver approximation, not a mesh silhouette or shadow map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactShadow {
    pub position_floor: [f32; 4],
    pub half_height: [f32; 4],
    pub incoming: [f32; 4],
    pub receiver_plane: [f32; 4],
}

pub(in crate::engine) fn eligible_caster(
    visible: bool,
    mut alpha_modes: impl Iterator<Item = AlphaMode>,
) -> bool {
    visible && alpha_modes.any(|alpha| alpha != AlphaMode::Blend)
}

/// Evaluates contacts in caller order within the caller's fixed slot budget.
#[must_use]
pub(in crate::engine) fn rank_contact_shadows<const SLOTS: usize>(
    subjects: impl Iterator<Item = (Aabb, Option<LightmapTexel>)>,
    surfaces: &LevelSurfaces<'_>,
    eye: Vec3,
    frustum: &Frustum,
    scene: Option<&TransportScene>,
) -> ([Option<ContactShadow>; SLOTS], usize) {
    let candidates = subjects
        .filter_map(|(bounds, sample)| {
            contact_shadow(
                bounds,
                surfaces,
                sample.map_or([0.0, 1.0, 0.0], |texel| texel.direction),
            )
        })
        .filter_map(|shadow| {
            contact_relevance(shadow, eye, frustum, scene).map(|distance| (shadow, distance))
        });
    select_contact_shadows(candidates)
}

/// Resolve transformed bounds against the highest authored walkable floor
/// below their top. A wholly buried subject cannot shade a floor above it;
/// stacked storeys retain their own support. Suspended contacts still fade.
#[must_use]
fn contact_shadow(
    bounds: crate::engine::spatial::Aabb,
    surfaces: &crate::engine::level::LevelSurfaces<'_>,
    incoming: [f32; 3],
) -> Option<ContactShadow> {
    let minimum = Vec3::from_array(bounds.min);
    let maximum = Vec3::from_array(bounds.max);
    if !minimum.is_finite() || !maximum.is_finite() {
        return None;
    }
    let centre = minimum.lerp(maximum, 0.5);
    let offset = surfaces.walkable_offset_at(centre.x, centre.z);
    let floor_y = surfaces
        .rooms()
        .iter()
        .filter(|room| room.contains(centre.x, centre.z))
        .map(|room| room.floor_y + offset)
        .filter(|floor| floor.is_finite() && *floor < maximum.y)
        .max_by(f32::total_cmp)?;
    let half = Vec3::new(
        (maximum.x - minimum.x) * 0.5,
        (maximum.y - minimum.y) * 0.5,
        (maximum.z - minimum.z) * 0.5,
    );
    if !half.is_finite() || half.min_element() <= 0.0 {
        return None;
    }
    let incoming_direction = Vec3::from_array(incoming).normalize_or(Vec3::Y);
    // Residual probes can be dominated by light bouncing up from the floor.
    // That below-receiver lobe cannot project a cast shadow down onto the
    // same floor. Keep its ambient grounding footprint beneath the subject.
    let direction = if incoming_direction.y > 0.0 {
        incoming_direction
    } else {
        Vec3::Y
    };
    let grounded = (1.0 - (minimum.y - floor_y).max(0.0) / 0.5).clamp(0.0, 1.0);
    if grounded <= 0.0 {
        return None;
    }
    let normal = surfaces
        .ramp_at(centre.x, centre.z)
        .map_or(Vec3::Y, |ramp| {
            let gradient = ramp.rise() / ramp.length().max(0.001);
            match ramp.axis() {
                crate::engine::level::WallAxis::X => Vec3::new(-gradient, 1.0, 0.0),
                crate::engine::level::WallAxis::Z => Vec3::new(0.0, 1.0, -gradient),
            }
            .normalize_or(Vec3::Y)
        });
    let support = Vec3::new(centre.x, floor_y, centre.z);
    Some(ContactShadow {
        position_floor: [centre.x, centre.y, centre.z, floor_y],
        half_height: [half.x, half.z, minimum.y, maximum.y],
        incoming: [direction.x, direction.y, direction.z, grounded],
        receiver_plane: [normal.x, normal.y, normal.z, -normal.dot(support)],
    })
}

/// Rank actual floor footprints, rather than object insertion order. Five
/// receiver samples retain a shadow whose subject is offscreen or occluded
/// while its floor footprint is visible. World clipping also keeps a hidden
/// subject's footprint from claiming relevance through a wall. The finite
/// scene/object limits bound this CPU work; no lighting solve is performed.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Validated finite contact uniforms use floating-point vector arithmetic."
)]
fn contact_relevance(
    shadow: ContactShadow,
    eye: Vec3,
    frustum: &crate::engine::spatial::Frustum,
    scene: Option<&crate::engine::lighting::transport::TransportScene>,
) -> Option<f32> {
    let transport = scene?;
    let centre = Vec3::from_array(shadow.position_floor[..3].try_into().ok()?);
    let support = Vec3::new(centre.x, shadow.position_floor[3], centre.z);
    let normal = Vec3::from_array(shadow.receiver_plane[..3].try_into().ok()?);
    let height = (centre.y - support.y).max(0.0);
    let direction = Vec3::from_array(shadow.incoming[..3].try_into().ok()?);
    let projection = -direction * height / direction.y.max(0.35);
    let radius = glam::Vec2::new(shadow.half_height[0], shadow.half_height[1])
        .max(glam::Vec2::splat(0.05))
        + glam::Vec2::splat(height.mul_add(0.15, 0.08));
    let on_plane = |x: f32, z: f32| {
        let y = -(normal
            .x
            .mul_add(x, normal.z.mul_add(z, shadow.receiver_plane[3])))
            / normal.y;
        Vec3::new(x, y, z)
    };
    let projected = on_plane(support.x + projection.x, support.z + projection.z);
    let extent = Vec3::new(
        radius.x * 1.8,
        (normal.x.abs().mul_add(radius.x, normal.z.abs() * radius.y) * 1.8 / normal.y) + 0.03,
        radius.y * 1.8,
    );
    if !frustum.intersects_aabb(&crate::engine::spatial::Aabb {
        min: (projected - extent).to_array(),
        max: (projected + extent).to_array(),
    }) {
        return None;
    }
    [
        glam::Vec2::ZERO,
        glam::Vec2::new(radius.x, 0.0),
        glam::Vec2::new(-radius.x, 0.0),
        glam::Vec2::new(0.0, radius.y),
        glam::Vec2::new(0.0, -radius.y),
    ]
    .into_iter()
    .filter_map(|offset| {
        let point = on_plane(projected.x + offset.x, projected.z + offset.y);
        let lifted = point + normal * 0.01;
        if transport.occluded((support + normal * 0.01).to_array(), lifted.to_array())
            || transport.occluded(eye.to_array(), lifted.to_array())
        {
            return None;
        }
        Some(eye.distance_squared(point))
    })
    .min_by(f32::total_cmp)
}

/// Distance first, then geometry as a stable tie break independent of spawn
/// order. Identical footprints are interchangeable and retain identical bytes.
#[must_use]
fn select_contact_shadows<const SLOTS: usize>(
    candidates: impl Iterator<Item = (ContactShadow, f32)>,
) -> ([Option<ContactShadow>; SLOTS], usize) {
    let mut ranked: [Option<(ContactShadow, f32)>; SLOTS] = [None; SLOTS];
    for candidate in candidates {
        let insertion = ranked.iter().position(|entry| {
            entry
                .as_ref()
                .is_none_or(|current| contact_order(&candidate, current).is_lt())
        });
        if let Some(tail) = insertion.and_then(|index| ranked.get_mut(index..)) {
            tail.rotate_right(1);
            if let Some(first) = tail.first_mut() {
                *first = Some(candidate);
            }
        }
    }
    let mut shadows = [None; SLOTS];
    let count = ranked.iter().flatten().count();
    for (slot, (shadow, _distance)) in shadows.iter_mut().zip(ranked.into_iter().flatten()) {
        *slot = Some(shadow);
    }
    (shadows, count)
}

fn contact_order(a: &(ContactShadow, f32), b: &(ContactShadow, f32)) -> std::cmp::Ordering {
    a.1.total_cmp(&b.1).then_with(|| {
        a.0.position_floor
            .iter()
            .chain(&a.0.half_height)
            .chain(&a.0.incoming)
            .zip(
                b.0.position_floor
                    .iter()
                    .chain(&b.0.half_height)
                    .chain(&b.0.incoming),
            )
            .map(|(left, right)| left.total_cmp(right))
            .find(|ordering| !ordering.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

#[cfg(test)]
#[path = "../tests/contact_shadow.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/lighting_receivers.rs"]
mod receiver_tests;

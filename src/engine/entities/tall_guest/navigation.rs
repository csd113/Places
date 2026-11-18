//! The native controller plans against the same bounded live item solids that
//! its physical movement checks. Baked topology and door policy stay unchanged.
use crate::engine::collision::{DoorCollider, STEP_EPS, WallAabb, dynamic_disc_segment_blocked};
use crate::engine::nav::NavDoorState;
use glam::{Vec2, Vec3};

pub(super) struct LiveSolids<'a>(pub &'a [WallAabb]);

impl<'a> LiveSolids<'a> {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The finite navigation disc is conservatively expanded in metre space."
    )]
    fn candidates(&self, from: Vec2, to: Vec2, radius: f32) -> impl Iterator<Item = &'a WallAabb> {
        // Keep boundary contacts in the broad phase and leave the actual
        // overlap, vertical clearance and sweep to the collision predicates.
        let padding = Vec2::splat(radius + STEP_EPS);
        let low = from.min(to) - padding;
        let high = from.max(to) + padding;
        let min = Vec2::new(low.x.next_down(), low.y.next_down());
        let max = Vec2::new(high.x.next_up(), high.y.next_up());
        self.0.iter().filter(move |solid| {
            !(solid.max_x < min.x
                || solid.min_x > max.x
                || solid.max_z < min.y
                || solid.min_z > max.y)
        })
    }
}

impl NavDoorState for LiveSolids<'_> {
    fn obstacle_blocked(&self, position: Vec3, radius: f32, height: f32) -> bool {
        let point = Vec2::new(position.x, position.z);
        self.candidates(point, point, radius).any(|solid| {
            solid.blocks_body(position.y, height)
                && solid.overlaps_disc(position.x, position.z, radius)
        })
    }

    fn obstacle_segment_blocked(&self, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
        let start = Vec2::new(from.x, from.z);
        let end = Vec2::new(to.x, to.z);
        let feet = from.y.min(to.y);
        let swept_height = height + (to.y - from.y).abs();
        self.candidates(start, end, radius).any(|solid| {
            dynamic_disc_segment_blocked(
                start,
                end,
                radius,
                feet,
                swept_height,
                std::slice::from_ref(solid),
            )
        })
    }

    fn door_open(&self, _id: &str) -> bool {
        true
    }

    fn door_locked(&self, _id: &str) -> Option<bool> {
        Some(false)
    }

    fn door_leaf(&self, _id: &str) -> Option<DoorCollider> {
        None
    }
}

#[cfg(test)]
#[path = "../../tests/tall_guest_navigation.rs"]
mod tests;

//! The native controller plans against the same bounded live item solids that
//! its physical movement checks. Baked topology and door policy stay unchanged.
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::nav::NavDoorState;
use glam::{Vec2, Vec3};

pub(super) struct LiveSolids<'a>(pub &'a [WallAabb]);

impl NavDoorState for LiveSolids<'_> {
    fn obstacle_blocked(&self, position: Vec3, radius: f32, height: f32) -> bool {
        self.0.iter().any(|solid| {
            solid.blocks_body(position.y, height)
                && solid.overlaps_disc(position.x, position.z, radius)
        })
    }

    fn obstacle_segment_blocked(&self, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
        crate::engine::collision::dynamic_disc_segment_blocked(
            Vec2::new(from.x, from.z),
            Vec2::new(to.x, to.z),
            radius,
            from.y.min(to.y),
            height + (to.y - from.y).abs(),
            self.0,
        )
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

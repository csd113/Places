//! Fixed-capacity trigger paths retain reversals between solver intervals.

use glam::{Quat, Vec3};

use super::collision::{add, point, pose, subtract};
use super::world::ItemPose;

/// Frame start, twelve fixed solver intervals and a possible final carry pose.
pub(super) const MAX_TRIGGER_PATH_POINTS: usize = 14;

/// Frame-swept collision bounds for item trigger checks, without diagnostics or
/// allocation. Fixed scenery and suspended recovery bodies are excluded.
#[derive(Debug, Clone, Copy)]
pub struct ItemTriggerBody {
    pub key: u64,
    pub previous: Vec3,
    pub current: Vec3,
    /// Fixed-orientation bodies retain their actual world AABB half extents.
    /// Turning bodies use the shape's enclosing sphere around its rigid origin,
    /// covering every intermediate orientation, including off-centre compounds.
    pub half_extents: Vec3,
    pub held: bool,
    path: [Vec3; MAX_TRIGGER_PATH_POINTS],
    path_len: u8,
}

impl ItemTriggerBody {
    /// Ordered frame start, solved substep positions and final carried position.
    /// Every segment must be sampled: endpoints alone can hide a bounce.
    #[must_use]
    pub fn path(&self) -> &[Vec3] {
        self.path
            .get(..usize::from(self.path_len))
            .unwrap_or_default()
    }
}

pub(super) struct TriggerPath {
    origins: [Vec3; MAX_TRIGGER_PATH_POINTS],
    len: u8,
    rotation: Quat,
    rotated: bool,
}

impl TriggerPath {
    pub fn new(initial: ItemPose, center: Vec3) -> Self {
        let origin = add(initial.position, initial.rotation.mul_vec3(center));
        Self {
            origins: [origin; MAX_TRIGGER_PATH_POINTS],
            len: 1,
            rotation: initial.rotation,
            rotated: false,
        }
    }

    pub fn record(&mut self, current: ItemPose, center: Vec3) {
        self.rotated |= current.rotation != self.rotation;
        if let Some(destination) = self.origins.get_mut(usize::from(self.len)) {
            *destination = add(current.position, current.rotation.mul_vec3(center));
            self.len = self.len.saturating_add(1);
        }
    }

    pub fn snapshot(
        &self,
        key: u64,
        current: ItemPose,
        center: Vec3,
        shape: &rapier3d::prelude::SharedShape,
        radius: f32,
        held: bool,
    ) -> ItemTriggerBody {
        let origin = add(current.position, current.rotation.mul_vec3(center));
        let mut path = self.origins;
        let mut path_len = self.len;
        if path
            .get(usize::from(path_len.saturating_sub(1)))
            .is_some_and(|last| *last != origin)
            && let Some(destination) = path.get_mut(usize::from(path_len))
        {
            *destination = origin;
            path_len = path_len.saturating_add(1);
        }
        let rotated = self.rotated || current.rotation != self.rotation;
        let half_extents = if rotated {
            Vec3::splat(radius)
        } else {
            let bounds = shape.compute_aabb(&pose(origin, current.rotation));
            let offset = subtract(point(bounds.center()), origin);
            for path_point in path.get_mut(..usize::from(path_len)).unwrap_or_default() {
                *path_point = add(*path_point, offset);
            }
            point(bounds.half_extents())
        };
        ItemTriggerBody {
            key,
            previous: path.first().copied().unwrap_or(origin),
            current: path
                .get(usize::from(path_len.saturating_sub(1)))
                .copied()
                .unwrap_or(origin),
            half_extents,
            held,
            path,
            path_len,
        }
    }
}

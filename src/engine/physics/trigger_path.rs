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
    /// True only for a sleeping dynamic solver body, never a dormant item.
    pub sleeping: bool,
    path: [Vec3; MAX_TRIGGER_PATH_POINTS],
    centre_path: [Vec3; MAX_TRIGGER_PATH_POINTS],
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

    /// Actual shape AABB centres, without the rotation-safe overlap envelope.
    #[must_use]
    pub fn centre_path(&self) -> &[Vec3] {
        self.centre_path
            .get(..usize::from(self.path_len))
            .unwrap_or_default()
    }
}

pub(super) struct TriggerPath {
    origins: [Vec3; MAX_TRIGGER_PATH_POINTS],
    centres: [Vec3; MAX_TRIGGER_PATH_POINTS],
    len: u8,
    rotation: Quat,
    rotated: bool,
}

impl TriggerPath {
    pub fn new(initial: ItemPose, center: Vec3, shape: &rapier3d::prelude::SharedShape) -> Self {
        let origin = add(initial.position, initial.rotation.mul_vec3(center));
        let centre = point(shape.compute_aabb(&pose(origin, initial.rotation)).center());
        Self {
            origins: [origin; MAX_TRIGGER_PATH_POINTS],
            centres: [centre; MAX_TRIGGER_PATH_POINTS],
            len: 1,
            rotation: initial.rotation,
            rotated: false,
        }
    }

    pub fn record(
        &mut self,
        current: ItemPose,
        center: Vec3,
        shape: &rapier3d::prelude::SharedShape,
    ) {
        self.rotated |= current.rotation != self.rotation;
        if let Some(destination) = self.origins.get_mut(usize::from(self.len)) {
            let origin = add(current.position, current.rotation.mul_vec3(center));
            *destination = origin;
            if let Some(centre) = self.centres.get_mut(usize::from(self.len)) {
                *centre = point(shape.compute_aabb(&pose(origin, current.rotation)).center());
            }
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
    ) -> ItemTriggerBody {
        let origin = add(current.position, current.rotation.mul_vec3(center));
        let centre = point(shape.compute_aabb(&pose(origin, current.rotation)).center());
        let mut path = self.origins;
        let mut centre_path = self.centres;
        let mut path_len = self.len;
        let last_index = usize::from(path_len.saturating_sub(1));
        if (path
            .get(last_index)
            .is_some_and(|previous| *previous != origin)
            || centre_path
                .get(last_index)
                .is_some_and(|previous| *previous != centre))
            && let Some(origin_destination) = path.get_mut(usize::from(path_len))
        {
            *origin_destination = origin;
            if let Some(centre_destination) = centre_path.get_mut(usize::from(path_len)) {
                *centre_destination = centre;
            }
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
            held: false,
            sleeping: false,
            path,
            centre_path,
            path_len,
        }
    }
}

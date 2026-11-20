//! CPU eligibility and invalidation rules for current-pose lighting queries.
use super::entity_receivers::EntityDirectLight;
use crate::engine::{entity::JointPose, spatial::Aabb};
use glam::Vec3;

#[derive(PartialEq)]
pub(in crate::engine) struct GeometryKey {
    asset: usize,
    pose: Vec<JointPose>,
    opacity: u32,
}

impl GeometryKey {
    pub const fn new(asset: usize, pose: Vec<JointPose>, opacity: f32) -> Self {
        Self {
            asset,
            pose,
            opacity: opacity.to_bits(),
        }
    }

    pub const fn can_refit(&self, other: &Self) -> bool {
        self.asset == other.asset
    }
}

#[derive(PartialEq)]
pub(in crate::engine) struct PoseKey {
    pub identity: usize,
    pub geometry: GeometryKey,
    pub transform: [f32; 16],
    pub direct: Vec<EntityDirectLight>,
}

pub(in crate::engine) fn ambient_changed(previous: &[PoseKey], current: &[PoseKey]) -> bool {
    previous.len() != current.len()
        || previous.iter().zip(current).any(|(old, new)| {
            old.identity != new.identity
                || old.geometry != new.geometry
                || old.transform[..12] != new.transform[..12]
        })
}

pub(in crate::engine) const fn ambient_needs_reset(
    pending: bool,
    changed: bool,
    previous_count: u32,
    count: u32,
) -> bool {
    pending || changed || previous_count != count
}

pub(in crate::engine) fn ambient_reach(bounds: &Aabb) -> f32 {
    Vec3::from_array(bounds.max)
        .distance(Vec3::from_array(bounds.min))
        .max(0.01)
        * 1.01
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite geometric range comparison; no integer arithmetic"
)]
pub(in crate::engine) fn depth_source_eligible(source: &EntityDirectLight, bounds: &Aabb) -> bool {
    if source.color_strength[3] <= 0.0 {
        return false;
    }
    let point = Vec3::from_slice(&source.position_range[..3]);
    let closest = point.clamp(Vec3::from_array(bounds.min), Vec3::from_array(bounds.max));
    let delta = point - closest;
    let distance = if source.extent_falloff[3] > 0.5 {
        (delta.x.abs() - source.extent_falloff[0])
            .max(0.0)
            .hypot((delta.z.abs() - source.extent_falloff[1]).max(0.0))
    } else {
        delta.length()
    };
    distance < source.position_range[3]
}

pub(in crate::engine) fn depth_taps(
    source: &EntityDirectLight,
) -> impl Iterator<Item = (usize, &[f32; 4])> {
    source.taps.iter().enumerate().filter_map(|(index, tap)| {
        if tap[3] <= 0.0 {
            None
        } else {
            Some((index, tap))
        }
    })
}

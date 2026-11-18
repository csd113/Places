//! Read-only solver snapshots and interpolated model-origin poses.

use glam::Vec3;

use super::super::collision::{add, point};
use super::super::{BodyMotion, FIXED_STEP};
use super::{BodyEntry, BodySnapshot, BodyState, ItemPose, ItemWorld};

impl BodyEntry {
    fn render_pose(&self, alpha: f32, held: bool) -> ItemPose {
        if self.dormant {
            self.dormant_pose
        } else if held {
            self.current
        } else {
            self.previous
                .interpolate(self.current, alpha, self.interpolation_center)
        }
    }
}

impl ItemWorld {
    /// One physical model pose and actual solver velocity, without allocating snapshots.
    #[must_use]
    pub fn body_pose_velocity(&self, key: u64) -> Option<(ItemPose, Vec3)> {
        let runtime = self.runtime.as_ref()?;
        let entry = runtime.entries.get(&key)?;
        let body = runtime.world.bodies.get(entry.body)?;
        let alpha = (self.accumulator / FIXED_STEP).clamp(0.0, 1.0);
        Some((
            entry.render_pose(alpha, self.held() == Some(key)),
            point(body.linvel()),
        ))
    }

    /// Emits interpolated model-origin poses in stable key order.
    pub fn for_each_pose(&self, mut visit: impl FnMut(u64, ItemPose)) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let alpha = (self.accumulator / FIXED_STEP).clamp(0.0, 1.0);
        for (&key, entry) in &runtime.entries {
            if entry.recovery.suspended {
                continue;
            }
            let interpolated = entry.render_pose(
                alpha,
                runtime.held.as_ref().is_some_and(|held| held.key == key),
            );
            visit(key, interpolated);
        }
    }

    /// Current solver poses and velocities; these do not include render interpolation.
    #[must_use]
    pub fn snapshots(&self) -> Vec<BodySnapshot> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let alpha = (self.accumulator / FIXED_STEP).clamp(0.0, 1.0);
        runtime
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                let body = runtime.world.bodies.get(entry.body)?;
                let collider = runtime.world.colliders.get(entry.collider)?;
                Some(BodySnapshot {
                    key,
                    pose: entry.current,
                    render_pose: entry.render_pose(
                        alpha,
                        runtime.held.as_ref().is_some_and(|held| held.key == key),
                    ),
                    linear_velocity: point(body.linvel()),
                    angular_velocity: point(body.angvel()),
                    sleeping: !entry.dormant && body.is_sleeping(),
                    dormant: entry.dormant,
                    state: if entry.recovery.suspended {
                        BodyState::Suspended
                    } else if entry.dormant {
                        BodyState::Dormant
                    } else if runtime.held.as_ref().is_some_and(|held| held.key == key) {
                        BodyState::Held
                    } else if entry.motion == BodyMotion::Fixed {
                        BodyState::Fixed
                    } else if entry.motion == BodyMotion::Kinematic {
                        BodyState::Kinematic
                    } else if body.is_sleeping() {
                        BodyState::Sleeping
                    } else {
                        BodyState::Active
                    },
                    held: runtime.held.as_ref().is_some_and(|held| held.key == key),
                    motion: entry.motion,
                    mass_kg: collider.mass(),
                    solver_mass_kg: body.mass(),
                    effective_mass_kg: point(body.mass_properties().effective_mass()),
                    local_center_of_mass: add(entry.center, point(body.local_center_of_mass())),
                    world_center_of_mass: point(body.center_of_mass()),
                    principal_inertia: point(
                        body.mass_properties().local_mprops.principal_inertia(),
                    ),
                    friction: collider.friction(),
                    restitution: collider.restitution(),
                    displaced_volume_m3: entry.water.as_ref().map_or(0.0, |water| water.volume_m3),
                    submerged_fraction: entry
                        .water
                        .as_ref()
                        .map_or(0.0, |water| water.submerged_fraction),
                })
            })
            .collect()
    }
}

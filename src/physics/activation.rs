//! Exact authored dormancy and bounded activation before rigid-body solving.

use glam::Vec3;
use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
use rapier3d::parry::query::{NonlinearRigidMotion, cast_shapes_nonlinear, contact};
use rapier3d::prelude::{Collider, Pose, RigidBodyType};
use rapier3d::utils::AngularInertiaOps;
use serde::Serialize;

use super::collision::{add, point, query_filter, scaled, subtract, vector};
use super::config::{BodyMotion, validate_reset_group};
use super::world::{ItemWorld, Runtime};
use super::{FIXED_STEP, MAX_BODIES};

/// Minimum predicted inward contact speed that counts as a physical activation.
pub const ACTIVATION_SPEED: f32 = 0.15;
const CONTACT_MARGIN: f32 = 0.006;

/// Number of previously dormant members activated by an explicit group action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActivationReport {
    pub activated_count: usize,
}

#[derive(Clone, Copy)]
pub(super) struct WakeSweep {
    key: u64,
    linear: Vec3,
    angular: Vec3,
    remaining: f32,
}

#[derive(Clone, Copy)]
pub(super) struct WakeTarget {
    key: u64,
    bounds: Aabb,
    pose: Pose,
}

#[derive(Clone, Copy)]
struct WakeImpact {
    target: u64,
    normal: Vec3,
    point: Vec3,
    speed: f32,
    source_mass: f32,
    remaining: f32,
}

impl ItemWorld {
    /// Explicitly enables a movable body's configured physics. Ordinary
    /// sleeping bodies are woken, while fixed definitions remain fixed.
    pub fn activate(&mut self, key: u64) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let activated = runtime.activate_key(key);
        runtime.populate_counts(&mut self.stats);
        activated
    }

    /// Activates existing members of a reusable authored group without moving
    /// their poses or supplying synthetic motion. Already active sleepers wake.
    ///
    /// # Errors
    /// Returns an error for an invalid group name or a group without bodies.
    pub fn activate_group(&mut self, group: &str) -> Result<ActivationReport, String> {
        validate_reset_group(group)?;
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("physics activation group has no bodies")?;
        let keys: Vec<u64> = runtime
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                (entry.reset_group.as_deref() == Some(group)).then_some(key)
            })
            .collect();
        if keys.is_empty() {
            return Err(format!("physics activation group '{group}' has no bodies"));
        }
        let mut activated_count = 0_usize;
        for key in keys {
            let dormant = runtime.entries.get(&key).is_some_and(|entry| entry.dormant);
            if runtime.activate_key(key) && dormant {
                activated_count = activated_count.saturating_add(1);
            }
        }
        runtime.populate_counts(&mut self.stats);
        Ok(ActivationReport { activated_count })
    }
}

impl Runtime {
    pub(super) fn activate_key(&mut self, key: u64) -> bool {
        let Some(entry) = self.entries.get_mut(&key) else {
            return false;
        };
        if !entry.motion.is_movable() || entry.recovery.suspended {
            return false;
        }
        let Some(body) = self.world.bodies.get_mut(entry.body) else {
            return false;
        };
        if entry.dormant {
            let motion = match entry.motion {
                BodyMotion::Dynamic => RigidBodyType::Dynamic,
                BodyMotion::Kinematic => RigidBodyType::KinematicPositionBased,
                BodyMotion::Fixed => return false,
            };
            body.set_body_type(motion, true);
            body.set_next_kinematic_position(*body.position());
            entry.dormant = false;
            entry.previous = entry.current;
        }
        self.world.wake_up(entry.body, true);
        true
    }

    /// All wake predictions are bounded by one fixed step and 128 existing
    /// bodies. Queue motion predicts possible impulse propagation; only Rapier
    /// applies the actual collision impulse, after every target is dynamic.
    pub(super) fn activate_impacts(&mut self) {
        if !self
            .entries
            .values()
            .any(|entry| entry.dormant && entry.motion == BodyMotion::Dynamic)
        {
            return;
        }
        let mut sweeps = std::mem::take(&mut self.wake_sweeps);
        let mut targets = std::mem::take(&mut self.wake_targets);
        sweeps.clear();
        targets.clear();
        self.activation_candidates(&mut sweeps, &mut targets);
        let mut cursor = 0_usize;
        while let Some(sweep) = sweeps.get(cursor).copied() {
            cursor = cursor.saturating_add(1);
            for target in &targets {
                let dormant = self
                    .entries
                    .get(&target.key)
                    .is_some_and(|entry| entry.dormant);
                if dormant
                    && let Some(impact) = self.predicted_impact(sweep, *target)
                    && self.activate_key(target.key)
                    && sweeps.len() < MAX_BODIES
                {
                    sweeps.push(self.propagated_sweep(impact));
                }
            }
        }
        self.wake_sweeps = sweeps;
        self.wake_targets = targets;
    }

    fn activation_candidates(&self, sweeps: &mut Vec<WakeSweep>, targets: &mut Vec<WakeTarget>) {
        for (&key, entry) in &self.entries {
            if entry.motion != BodyMotion::Dynamic {
                continue;
            }
            let Some(body) = self.world.bodies.get(entry.body) else {
                continue;
            };
            if entry.dormant {
                if let Some(collider) = self.world.colliders.get(entry.collider) {
                    targets.push(WakeTarget {
                        key,
                        bounds: collider.compute_aabb(),
                        pose: *body.position(),
                    });
                }
            } else if body.is_dynamic() && !body.is_sleeping() {
                let acceleration = add(
                    scaled(point(self.world.gravity), body.gravity_scale()),
                    scaled(point(body.user_force()), 1.0 / body.mass()),
                );
                let linear = add(point(body.linvel()), scaled(acceleration, FIXED_STEP));
                let angular_acceleration = point(
                    body.mass_properties()
                        .effective_world_inv_inertia
                        .transform_vector(body.user_torque()),
                );
                let angular = add(
                    point(body.angvel()),
                    scaled(angular_acceleration, FIXED_STEP),
                );
                if linear.is_finite()
                    && angular.is_finite()
                    && angular.length().mul_add(entry.radius, linear.length()) >= ACTIVATION_SPEED
                {
                    sweeps.push(WakeSweep {
                        key,
                        linear,
                        angular,
                        remaining: FIXED_STEP,
                    });
                }
            }
        }
    }

    fn predicted_impact(&self, sweep: WakeSweep, target: WakeTarget) -> Option<WakeImpact> {
        let source = self.entries.get(&sweep.key)?;
        let destination = self.entries.get(&target.key)?;
        let body = self.world.bodies.get(source.body)?;
        // The solver can apply a speculative impulse before shapes overlap.
        // Activate inside the same prediction distance so a dormant fixed
        // receiver cannot rebound a slow incoming body before it wakes.
        let contact_margin = self
            .world
            .integration_parameters
            .prediction_distance()
            .max(CONTACT_MARGIN);
        let motion = NonlinearRigidMotion::new(
            *body.position(),
            body.local_center_of_mass(),
            vector(sweep.linear),
            vector(sweep.angular),
        );
        let end_pose = motion.position_at_time(sweep.remaining);
        let angular_margin = sweep.angular.length()
            * (source.radius + body.local_center_of_mass().length())
            * sweep.remaining;
        let bounds = source
            .shape
            .compute_aabb(body.position())
            .merged(&source.shape.compute_aabb(&end_pose))
            .loosened(contact_margin + angular_margin);
        if !bounds.intersects(&target.bounds) {
            return None;
        }
        let existing = contact(
            body.position(),
            source.shape.as_ref(),
            &target.pose,
            destination.shape.as_ref(),
            contact_margin,
        )
        .ok()
        .flatten();
        let (time, candidate) = if let Some(existing_contact) = existing {
            (0.0, existing_contact)
        } else {
            // The swept AABB already selected this exact existing target.
            // Query the pair directly, including before the first world step,
            // instead of traversing the world's composite broad-phase again.
            let cast = cast_shapes_nonlinear(
                &motion,
                source.shape.as_ref(),
                &NonlinearRigidMotion::constant_position(target.pose),
                destination.shape.as_ref(),
                0.0,
                sweep.remaining,
                false,
            );
            let hit = cast.ok().flatten()?;
            let at_impact = motion.position_at_time(hit.time_of_impact);
            let geometry = contact(
                &at_impact,
                source.shape.as_ref(),
                &target.pose,
                destination.shape.as_ref(),
                contact_margin,
            )
            .ok()
            .flatten()?;
            (hit.time_of_impact, geometry)
        };
        let impact_pose = motion.position_at_time(time);
        let center = point(impact_pose.transform_point(body.local_center_of_mass()));
        let contact_point = point(candidate.point1);
        let velocity = add(
            sweep.linear,
            sweep.angular.cross(subtract(contact_point, center)),
        );
        let normal = point(candidate.normal1);
        let speed = velocity.dot(normal);
        if speed < ACTIVATION_SPEED
            || !speed.is_finite()
            || !self.impact_path_clear(sweep, &motion, source.collider, destination.collider)
        {
            return None;
        }
        Some(WakeImpact {
            target: target.key,
            normal,
            point: point(candidate.point2),
            speed,
            source_mass: body.mass(),
            remaining: (sweep.remaining - time).max(0.0),
        })
    }

    fn impact_path_clear(
        &self,
        sweep: WakeSweep,
        motion: &NonlinearRigidMotion,
        source: rapier3d::prelude::ColliderHandle,
        target: rapier3d::prelude::ColliderHandle,
    ) -> bool {
        let Some(incoming) = self.world.colliders.get(source) else {
            return false;
        };
        let Some(receiver) = self.world.colliders.get(target) else {
            return false;
        };
        let receiver_time = cast_shapes_nonlinear(
            motion,
            incoming.shape(),
            &NonlinearRigidMotion::constant_position(*receiver.position()),
            receiver.shape(),
            0.0,
            sweep.remaining,
            false,
        )
        .ok()
        .flatten()
        .map_or(sweep.remaining, |hit| hit.time_of_impact);
        let outside_pair = |handle, collider: &Collider| {
            collider.is_enabled()
                && handle != source
                && handle != target
                && !self.is_quiet_support(sweep, motion, incoming, collider)
        };
        let filter = query_filter().predicate(&outside_pair);
        self.world
            .cast_shape_nonlinear(motion, incoming.shape(), 0.0, receiver_time, false, filter)
            .is_none_or(|(_, hit)| hit.time_of_impact + 1.0e-5 >= receiver_time)
    }

    fn is_quiet_support(
        &self,
        sweep: WakeSweep,
        motion: &NonlinearRigidMotion,
        incoming: &Collider,
        support: &Collider,
    ) -> bool {
        let margin = self.world.integration_parameters.prediction_distance();
        contact(
            &motion.start,
            incoming.shape(),
            support.position(),
            support.shape(),
            margin,
        )
        .is_ok_and(|geometry| {
            geometry.is_some_and(|surface| {
                let center = point(motion.start.transform_point(motion.local_center));
                let velocity = add(
                    sweep.linear,
                    sweep.angular.cross(subtract(point(surface.point1), center)),
                );
                velocity.dot(point(surface.normal1)) < ACTIVATION_SPEED
            })
        })
    }

    fn propagated_sweep(&self, impact: WakeImpact) -> WakeSweep {
        let mut result = WakeSweep {
            key: impact.target,
            linear: Vec3::ZERO,
            angular: Vec3::ZERO,
            remaining: impact.remaining,
        };
        if let Some(entry) = self.entries.get(&impact.target)
            && let Some(body) = self.world.bodies.get(entry.body)
        {
            // Elastic reduced-mass impulse is a conservative wake prediction.
            // It never supplies velocity or energy to the actual rigid body.
            let impulse = 2.0 * impact.source_mass * body.mass()
                / (impact.source_mass + body.mass())
                * impact.speed;
            let momentum = scaled(impact.normal, impulse);
            result.linear = scaled(momentum, 1.0 / body.mass());
            let torque = subtract(impact.point, point(body.center_of_mass())).cross(momentum);
            result.angular = point(
                body.mass_properties()
                    .effective_world_inv_inertia
                    .transform_vector(vector(torque)),
            );
        }
        result
    }
}

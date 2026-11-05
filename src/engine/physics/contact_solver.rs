//! Bounded convergence for nearby heavy/light props, without changing mass.

use std::collections::BTreeMap;

use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
use rapier3d::parry::shape::{Ball, Shape};
use rapier3d::prelude::{ActiveHooks, ColliderHandle, PhysicsWorld, SpringCoefficients};

use super::FIXED_STEP;
use super::config::BodyMotion;
use super::world::BodyEntry;

#[derive(Default)]
pub(super) struct ContactSolver {
    quiet_seconds: f32,
    high_ratio_seconds: f32,
    bodies: Vec<SolverBody>,
    fresh_contacts: Vec<(ColliderHandle, ActiveHooks)>,
}

struct SolverBody {
    key: u64,
    collider: ColliderHandle,
    mass: f32,
    dynamic: bool,
    sleeping: bool,
    bounds: Aabb,
}

impl ContactSolver {
    pub(super) fn configure(
        &mut self,
        world: &mut PhysicsWorld,
        entries: &BTreeMap<u64, BodyEntry>,
        held: Option<u64>,
    ) {
        self.restore_contact_hooks(world);
        self.bodies.clear();
        for (&key, entry) in entries {
            if entry.motion != BodyMotion::Dynamic || entry.recovery.suspended {
                continue;
            }
            let Some(body) = world
                .bodies
                .get(entry.body)
                .filter(|body| body.is_enabled())
            else {
                continue;
            };
            let mass = body.mass();
            if mass <= 0.0 {
                continue;
            }
            // Catch the first approaching contact, not just last tick's
            // manifolds. Angular travel conservatively expands the bound.
            let margin = body
                .angvel()
                .length()
                .mul_add(entry.radius, body.linvel().length())
                .mul_add(FIXED_STEP, 0.02);
            let current_bounds = entry.shape.compute_aabb(body.position());
            let swept_bounds = if body.is_kinematic() {
                // Endpoint boxes miss intermediate orientations during a
                // slow-world control interval. The enclosing ball covers the
                // whole angular sweep, including a half-turn of a long prop.
                let ball = Ball::new(entry.radius);
                ball.compute_aabb(body.position())
                    .merged(&ball.compute_aabb(body.next_position()))
            } else {
                current_bounds
            };
            self.bodies.push(SolverBody {
                key,
                collider: entry.collider,
                mass,
                dynamic: body.is_dynamic(),
                sleeping: body.is_sleeping(),
                bounds: swept_bounds.loosened(margin),
            });
        }
        let ratio = self.maximum_nearby_ratio();
        // Retain one profile as a pair separates so manifold churn cannot
        // toggle stiffness every tick. At most 128 props enter this query.
        self.quiet_seconds = if ratio >= 32.0 {
            0.25
        } else {
            (self.quiet_seconds - FIXED_STEP).max(0.0)
        };
        self.high_ratio_seconds = if ratio >= 256.0 {
            0.25
        } else {
            (self.high_ratio_seconds - FIXED_STEP).max(0.0)
        };
        let robust = self.quiet_seconds > 0.0;
        let iterations = if self.high_ratio_seconds > 0.0 {
            64
        } else {
            24
        };
        let parameters = &mut world.integration_parameters;
        parameters.contact_softness = if self.high_ratio_seconds > 0.0 {
            SpringCoefficients::new(1_000.0, 1.0)
        } else if robust {
            SpringCoefficients::new(240.0, 20.0)
        } else {
            SpringCoefficients::contact_defaults()
        };
        parameters.static_contact_softness =
            SpringCoefficients::new(if robust { 10_000.0 } else { 240.0 }, 1.0);
        // Damped medium-ratio contacts need normal convergence but only four
        // relaxation passes. Repeating both passes equally wastes work and
        // prolongs rocking in dense stacks. Higher ratios retain the stronger
        // convergence budget required by heavy drops onto light supports.
        let passes = if robust { iterations } else { 1 };
        parameters.num_internal_pgs_iterations = passes;
        parameters.num_internal_stabilization_iterations = if self.high_ratio_seconds > 0.0 {
            64
        } else if robust {
            4
        } else {
            1
        };
        // Stiff wedged contacts must not turn positional correction into a
        // launch. This bounds penetration bias, not actual impact velocity or
        // restitution. Retain this bound while held too: the larger bias
        // can leave a dense light-prop pile compressed after the hand stops.
        parameters.normalized_max_corrective_velocity =
            if robust || held.is_some() { 0.2 } else { 3.0 };
        // One-millimetre recycling retains resting anchors at bounded cost.
        parameters.contact_recycling = robust;
        parameters.normalized_contact_recycle_distance = 0.001;
        if robust {
            self.fresh_hand_neighborhood(world, held);
        }
    }

    fn maximum_nearby_ratio(&self) -> f32 {
        let mut ratio = 1.0_f32;
        for (index, first) in self.bodies.iter().enumerate() {
            for second in self.bodies.iter().skip(index.saturating_add(1)) {
                // A held prop is kinematic: its authored mass does not enter
                // the contact equations. It must not enable the heavy/light
                // dynamic profile for every nearby equal-mass prop.
                if !first.dynamic || !second.dynamic || (first.sleeping && second.sleeping) {
                    continue;
                }
                if first.bounds.intersects(&second.bounds) {
                    ratio = ratio.max(first.mass.max(second.mass) / first.mass.min(second.mass));
                }
            }
        }
        ratio
    }

    fn fresh_hand_neighborhood(&mut self, world: &mut PhysicsWorld, held: Option<u64>) {
        let Some(hand) = self.bodies.iter().find(|body| Some(body.key) == held) else {
            return;
        };
        let bounds = hand.bounds;
        for body in self
            .bodies
            .iter()
            .filter(|body| body.bounds.intersects(&bounds))
        {
            if let Some(collider) = world.colliders.get_mut(body.collider) {
                let previous = collider.active_hooks();
                self.fresh_contacts.push((body.collider, previous));
                // Rapier excludes hooked pairs from recycling. The existing
                // floor hook leaves every non-floor material unchanged. Mark
                // neighbors too: their support manifolds must remain fresh
                // when a prescribed hand presses them against scenery.
                collider.set_active_hooks(previous.union(ActiveHooks::MODIFY_SOLVER_CONTACTS));
            }
        }
    }

    pub(super) fn restore_contact_hooks(&mut self, world: &mut PhysicsWorld) {
        for (handle, previous) in self.fresh_contacts.drain(..) {
            if let Some(collider) = world.colliders.get_mut(handle) {
                collider.set_active_hooks(previous);
            }
        }
    }
}

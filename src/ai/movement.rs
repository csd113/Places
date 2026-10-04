//! Collision-respecting locomotion for AI agents.
//!
//! An agent moves through the **same** shared rules the player and authored
//! routes use: resolve the body disc against the static boxes and the live
//! door leaves, snap to the walkable floor, and refuse a step the body cannot
//! take. There is no teleporting between path nodes: the path only supplies a
//! waypoint, and this mover decides whether the next substep is physically
//! possible.
//!
//! The substep is [`crate::nav::NAV_MOVE_SUBSTEP_M`], short enough that
//! `substep * NAV_MAX_SLOPE <= step_height`, so a continuous stair or ramp
//! that the bake accepts is climbable here too; a test pins that relationship.

// Preserve exact sentinel comparisons, floating-point operation order and
// cohesive geometry/query stages. Numeric conversions and integer arithmetic
// are audited at their local expressions instead of exempting the module.
#![allow(
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "Preserve exact sentinel comparisons and established floating-point operation order; named geometry stages and cohesive query parameters keep these numeric kernels readable. Numeric conversions and integer arithmetic exceptions are documented locally."
)]

use glam::Vec3;

use crate::collision::{DoorCollider, WallAabb, resolve_player_collision_with_doors};
use crate::collision_index::CollisionIndex;
use crate::door::Doors;
use crate::entity::{ENTITY_TURN_RATE_DEGREES_PER_SECOND, turn_toward};
use crate::level::WalkableFloor;
use crate::nav::NAV_MOVE_SUBSTEP_M;

use super::AiState;

/// How close an agent must come to a waypoint for it to count as reached.
pub const ARRIVE_RADIUS_M: f32 = 0.22;

/// Largest simulation step one move call integrates, in seconds.
pub const MAX_MOVE_STEP_S: f32 = 0.10;

/// The pose cue for one locomotion state and speed.
#[must_use]
pub fn cue_for(state: AiState, speed_mps: f32) -> crate::entity::PoseCue {
    if state.is_moving() {
        crate::entity::PoseCue::Walk {
            speed_mps: speed_mps.max(0.01),
        }
    } else {
        crate::entity::PoseCue::Idle
    }
}

/// What one move call did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveStep {
    /// The body advanced (or is still moving); carries the new pose.
    Moved {
        /// New feet position.
        position: Vec3,
        /// New yaw in degrees.
        yaw_degrees: f32,
        /// Distance covered this call, in metres per second.
        speed_mps: f32,
    },
    /// Something physical refuses the next substep.
    Blocked,
    /// The waypoint is within [`ARRIVE_RADIUS_M`].
    Arrived,
}

/// One agent's moving body.
#[derive(Debug, Clone, Copy)]
pub struct AgentMove {
    /// Current feet position.
    pub position: Vec3,
    /// Current yaw in degrees.
    pub yaw_degrees: f32,
    /// Body disc radius.
    pub radius: f32,
    /// Body height.
    pub height: f32,
    /// Largest surface rise the body walks up.
    pub step_height: f32,
    /// Largest rise per metre of run the body walks.
    pub max_slope: f32,
    /// Desired speed in m/s.
    pub speed_mps: f32,
}

impl AgentMove {
    /// Advances toward `waypoint`, resolving collision and floor support.
    ///
    /// `leaves` are the world's live door colliders, so a moving leaf blocks
    /// exactly as it does for the player.
    pub fn step(
        &mut self,
        waypoint: Vec3,
        delta: f32,
        walls: &[WallAabb],
        index: &CollisionIndex,
        floor: &WalkableFloor,
        doors: &Doors,
    ) -> MoveStep {
        let leaves = doors.colliders();
        self.step_with_leaves(waypoint, delta, walls, index, floor, &leaves)
    }

    /// [`Self::step`] against a caller-held leaf list.
    pub fn step_with_leaves(
        &mut self,
        waypoint: Vec3,
        delta: f32,
        walls: &[WallAabb],
        index: &CollisionIndex,
        floor: &WalkableFloor,
        leaves: &[DoorCollider],
    ) -> MoveStep {
        if !waypoint.is_finite() || !self.position.is_finite() {
            return MoveStep::Blocked;
        }
        let step_delta = delta.clamp(0.0, MAX_MOVE_STEP_S);
        let flat = Vec3::new(
            waypoint.x - self.position.x,
            0.0,
            waypoint.z - self.position.z,
        );
        let distance = flat.length();
        if distance <= ARRIVE_RADIUS_M {
            return MoveStep::Arrived;
        }
        let Some(direction) = flat.try_normalize() else {
            return MoveStep::Arrived;
        };
        let target_yaw = direction.x.atan2(direction.z).to_degrees();
        self.yaw_degrees = turn_toward(
            self.yaw_degrees.to_radians(),
            target_yaw.to_radians(),
            ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * step_delta.max(1.0e-4),
        )
        .to_degrees();
        let travel = (self.speed_mps * step_delta).min(distance);
        if travel <= 0.0 {
            return MoveStep::Arrived;
        }
        let substep_count = (travel / NAV_MOVE_SUBSTEP_M).ceil().clamp(1.0, 64.0);
        let substep = travel / substep_count;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "The substep count is clamped to 1..=64 before casting; a NaN count retains the existing zero-step saturation behavior."
        )]
        let count = substep_count as u32;
        let mut moved = 0.0_f32;
        for _ in 0..count {
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "glam vector addition, subtraction and scaling intentionally use ordinary f32 arithmetic; no integer sizing or indexing is performed here."
            )]
            let candidate = self.position + direction * substep;
            let resolved = resolve_player_collision_with_doors(
                index,
                glam::Vec2::new(candidate.x, candidate.z),
                self.radius,
                self.position.y,
                self.height,
                walls,
                leaves,
            );
            let depenetration =
                glam::Vec2::new(resolved.x - candidate.x, resolved.y - candidate.z).length();
            if depenetration > self.radius + 0.01 {
                return if moved > 0.0 {
                    MoveStep::Moved {
                        position: self.position,
                        yaw_degrees: self.yaw_degrees,
                        speed_mps: moved / step_delta.max(1.0e-4),
                    }
                } else {
                    MoveStep::Blocked
                };
            }
            let Some(floor_y) = floor.walk_height_at(resolved.x, resolved.y) else {
                return MoveStep::Blocked;
            };
            let allowance =
                self.step_height + self.max_slope * substep + crate::collision::STEP_EPS;
            if (floor_y - self.position.y).abs() > allowance {
                return MoveStep::Blocked;
            }
            self.position = Vec3::new(resolved.x, floor_y, resolved.y);
            moved += substep;
        }
        MoveStep::Moved {
            position: self.position,
            yaw_degrees: self.yaw_degrees,
            speed_mps: moved / step_delta.max(1.0e-4),
        }
    }
}

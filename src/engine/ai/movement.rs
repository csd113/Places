//! Collision-respecting locomotion for AI agents.
//!
//! Movement uses exact horizontal disc sweeps against static solids, live
//! door leaves and current physical item bounds. Each short substep retains
//! the nearest supported storey and checks the body's actual headroom and
//! shared step/slope rule. A path supplies direction; it never teleports a body.
//!
//! The bake and mover use [`crate::engine::nav::NAV_MOVE_SUBSTEP_M`] and the same
//! surface identity and collision rules. Reported speed measures distance
//! actually travelled, including partial progress before a blocked substep.

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

use glam::{Vec2, Vec3};

use crate::engine::collision::{
    DoorCollider, WallAabb, disc_segment_blocked, door_segment_blocked,
    dynamic_disc_segment_blocked,
};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::entity::{ENTITY_TURN_RATE_DEGREES_PER_SECOND, turn_toward};
use crate::engine::level::{WalkableCeiling, WalkableFloor};
use crate::engine::nav::{CELL_SLOPED, NAV_MOVE_SUBSTEP_M, neighbour_rule};

use super::AiState;

/// How close an agent must come to a waypoint for it to count as reached.
pub const ARRIVE_RADIUS_M: f32 = 0.05;

/// Keep arrivals well inside a default 20 cm navigation cell so an actor
/// reaches a corridor corner before turning toward its next segment.
#[must_use]
pub fn arrival_radius(radius: f32) -> f32 {
    radius.clamp(0.025, ARRIVE_RADIUS_M)
}

/// Largest simulation step one move call integrates, in seconds.
pub const MAX_MOVE_STEP_S: f32 = 0.10;

/// The pose cue for one locomotion state and speed.
#[must_use]
pub fn cue_for(state: AiState, speed_mps: f32) -> crate::engine::entity::PoseCue {
    if state.is_moving() && speed_mps > 0.01 {
        crate::engine::entity::PoseCue::Walk {
            speed_mps: speed_mps.max(0.01),
        }
    } else {
        crate::engine::entity::PoseCue::Idle
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
        self.step_with_solids(
            waypoint,
            delta,
            walls,
            index,
            floor,
            &WalkableCeiling::default(),
            leaves,
            &[],
        )
    }

    /// Movement against the same live physical boxes the route overlay reads.
    pub fn step_with_solids(
        &mut self,
        waypoint: Vec3,
        delta: f32,
        walls: &[WallAabb],
        index: &CollisionIndex,
        floor: &WalkableFloor,
        ceiling: &WalkableCeiling,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> MoveStep {
        if !waypoint.is_finite() || !self.position.is_finite() {
            return MoveStep::Blocked;
        }
        if !delta.is_finite() || delta <= 0.0 {
            return MoveStep::Moved {
                position: self.position,
                yaw_degrees: self.yaw_degrees,
                speed_mps: 0.0,
            };
        }
        if !headroom_clear(
            self,
            self.position.x,
            self.position.z,
            self.position.y,
            floor,
            ceiling,
            index,
            walls,
        ) {
            return MoveStep::Blocked;
        }
        let step_delta = delta.clamp(0.0, MAX_MOVE_STEP_S);
        let flat = Vec3::new(
            waypoint.x - self.position.x,
            0.0,
            waypoint.z - self.position.z,
        );
        let distance = flat.length();
        if waypoint.distance(self.position) <= arrival_radius(self.radius) {
            return MoveStep::Arrived;
        }
        let Some(direction) = flat.try_normalize() else {
            return MoveStep::Blocked;
        };
        let target_yaw = direction.x.atan2(direction.z).to_degrees();
        self.yaw_degrees = turn_toward(
            self.yaw_degrees.to_radians(),
            target_yaw.to_radians(),
            ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * step_delta,
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
            let Some(floor_y) =
                support_step(self, self.position, candidate, floor, ceiling, index, walls)
            else {
                return self.progress(moved, step_delta);
            };
            let from = Vec2::new(self.position.x, self.position.z);
            let to = Vec2::new(candidate.x, candidate.z);
            let feet = self.position.y.min(floor_y);
            let sweep_height = self.height + (floor_y - self.position.y).abs();
            if [from, to].into_iter().any(|point| {
                ceiling
                    .ceiling_above_disc(point.x, point.y, self.radius, feet)
                    .is_some_and(|underside| {
                        feet + sweep_height > underside + crate::engine::collision::CONTACT_EPS
                    })
            }) {
                return self.progress(moved, step_delta);
            }
            if disc_segment_blocked(
                from,
                to,
                self.radius,
                feet,
                sweep_height,
                self.step_height,
                index,
                walls,
            ) || dynamic_disc_segment_blocked(from, to, self.radius, feet, sweep_height, solids)
                || door_segment_blocked(from, to, self.radius, feet, sweep_height, leaves)
            {
                return self.progress(moved, step_delta);
            }
            let next = Vec3::new(candidate.x, floor_y, candidate.z);
            moved += self.position.distance(next);
            self.position = next;
        }
        self.progress(moved, step_delta)
    }

    fn progress(&self, moved: f32, delta: f32) -> MoveStep {
        if moved <= 1.0e-6 {
            MoveStep::Blocked
        } else {
            MoveStep::Moved {
                position: self.position,
                yaw_degrees: self.yaw_degrees,
                speed_mps: moved / delta.max(1.0e-4),
            }
        }
    }
}

/// Support reached by walking short samples from the current storey.
/// Choosing the closest emitted surface prevents a higher room from winning a
/// long steering probe merely because it lies within the slope search bound.
pub(super) fn surface_along(
    body: &AgentMove,
    target: Vec2,
    floor: &WalkableFloor,
    ceiling: &WalkableCeiling,
    index: &CollisionIndex,
    walls: &[WallAabb],
) -> Option<f32> {
    let distance = Vec2::new(body.position.x, body.position.z).distance(target);
    let mut point = body.position;
    let mut travelled = 0.0_f32;
    for _ in 0_u8..32 {
        if travelled >= distance {
            return Some(point.y);
        }
        travelled = (travelled + NAV_MOVE_SUBSTEP_M).min(distance);
        let t = travelled / distance.max(1.0e-6);
        let next = Vec3::new(
            (target.x - body.position.x).mul_add(t, body.position.x),
            point.y,
            (target.y - body.position.z).mul_add(t, body.position.z),
        );
        let y = support_step(body, point, next, floor, ceiling, index, walls)?;
        point = Vec3::new(next.x, y, next.z);
    }
    None
}

fn support_step(
    body: &AgentMove,
    from: Vec3,
    to: Vec3,
    floor: &WalkableFloor,
    ceiling: &WalkableCeiling,
    index: &CollisionIndex,
    walls: &[WallAabb],
) -> Option<f32> {
    let distance = (to.x - from.x).hypot(to.z - from.z);
    let reach = body.step_height + body.max_slope * distance + crate::engine::collision::STEP_EPS;
    let flags = |x, z, height| {
        if floor.walk_surface_sloped(x, z, height) {
            CELL_SLOPED
        } else {
            0
        }
    };
    let profile = crate::engine::package::navigation::NavClass {
        radius: body.radius,
        height: body.height,
        step_height: body.step_height,
        max_slope: body.max_slope,
    };
    let from_flags = flags(from.x, from.z, from.y);
    let mut selected: Option<f32> = None;
    floor.for_each_walk_surface(to.x, to.z, |y| {
        if (y - from.y).abs() <= reach
            && selected.is_none_or(|previous| (y - from.y).abs() < (previous - from.y).abs())
            && neighbour_rule(
                from.y,
                y,
                distance,
                from_flags,
                flags(to.x, to.z, y),
                &profile,
            )
            && headroom_clear(body, to.x, to.z, y, floor, ceiling, index, walls)
        {
            selected = Some(y);
        }
    });
    selected
}

fn headroom_clear(
    body: &AgentMove,
    x: f32,
    z: f32,
    y: f32,
    floor: &WalkableFloor,
    ceiling: &WalkableCeiling,
    index: &CollisionIndex,
    walls: &[WallAabb],
) -> bool {
    let point = Vec2::new(x, z);
    let underside = ceiling
        .ceiling_above_disc(x, z, body.radius, y)
        .into_iter()
        // A leading disc may step onto an adjacent landing, while a slab
        // directly above the centre remains real overhead clearance.
        .chain(floor.lowest_room_underside(point, body.radius, y + body.step_height))
        .chain(floor.lowest_room_underside(point, 1.0e-4, y))
        .chain(crate::engine::collision::lowest_underside_indexed(
            index,
            x,
            z,
            body.radius,
            y,
            walls,
        ))
        .min_by(f32::total_cmp);
    underside.is_none_or(|height| y + body.height <= height + crate::engine::collision::CONTACT_EPS)
}

use super::{Feltling, Stage, navigation_class};
use crate::engine::ai::movement::{AgentMove, MoveStep};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::door::Doors;
use crate::engine::entities::WorldContext;
use crate::engine::nav::{NavDoorState, PathQuery};
use glam::{Vec2, Vec3};

/// Both navigation queries and accepted physical substeps consult the same
/// live door/obstacle snapshot. A failed route stays blocked and retries.
struct Overlay<'a> {
    doors: &'a Doors,
    solids: &'a [WallAabb],
    leaves: &'a [DoorCollider],
}
impl NavDoorState for Overlay<'_> {
    fn door_open(&self, id: &str) -> bool {
        self.doors.door_open(id)
    }
    fn door_locked(&self, id: &str) -> Option<bool> {
        self.doors.door_locked(id)
    }
    fn door_leaf(&self, id: &str) -> Option<DoorCollider> {
        self.doors.door_leaf(id)
    }
    fn door_obstructed(&self, id: &str) -> bool {
        self.doors.door_obstructed(id)
    }
    fn obstacle_segment_blocked(&self, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
        crate::engine::collision::dynamic_disc_segment_blocked(
            Vec2::new(from.x, from.z),
            Vec2::new(to.x, to.z),
            radius,
            from.y.min(to.y),
            height + (to.y - from.y).abs(),
            self.solids,
        ) || self
            .leaves
            .iter()
            .any(|leaf| leaf_path_blocked(leaf, from, to, radius, height))
    }
}

// Grid centres can sit just inside a moving actor even when the exact body
// starts clear. The shared separating sweep lets the plan exit that overlap;
// accepted movement still uses exact live leaves and strict admission.
fn leaf_path_blocked(leaf: &DoorCollider, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
    let local = |point: Vec3| {
        let dx = point.x - leaf.hinge_x;
        let dz = point.z - leaf.hinge_z;
        Vec2::new(
            dx.mul_add(leaf.dir_x, dz * leaf.dir_z),
            dx.mul_add(-leaf.dir_z, dz * leaf.dir_x),
        )
    };
    let solid = WallAabb::with_y(
        0.0,
        leaf.hinge_y,
        -leaf.thickness * 0.5,
        leaf.width,
        leaf.height,
        leaf.thickness,
    );
    crate::engine::collision::dynamic_disc_segment_blocked(
        local(from),
        local(to),
        radius,
        from.y.min(to.y),
        height + (to.y - from.y).abs(),
        &[solid],
    )
}

#[must_use]
pub fn base_speed(player_walk_speed: f32) -> f32 {
    let walk = if player_walk_speed.is_finite() && (1.0..=10.0).contains(&player_walk_speed) {
        player_walk_speed
    } else {
        3.75
    };
    walk * 1.3
}

impl Feltling {
    #[must_use]
    pub fn collider(&self) -> Option<DoorCollider> {
        if self.attachment.is_some() && self.enabled() {
            let vertices = self.physical_vertices();
            let lo = vertices
                .iter()
                .copied()
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let hi = vertices
                .iter()
                .copied()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            return Some(DoorCollider::from_pose(
                [lo.x, lo.y, lo.z.midpoint(hi.z)],
                [1.0, 0.0],
                hi.x - lo.x,
                hi.y - lo.y,
                hi.z - lo.z,
            ));
        }
        self.enabled().then(|| {
            DoorCollider::from_pose(
                [
                    self.state.position.x - 0.40,
                    self.state.position.y,
                    self.state.position.z,
                ],
                [1.0, 0.0],
                0.80,
                0.80,
                0.80,
            )
        })
    }

    #[must_use]
    pub fn clear(
        &self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        let point = Vec2::new(self.state.position.x, self.state.position.z);
        let class = navigation_class();
        !crate::engine::collision::disc_segment_blocked(
            point,
            point,
            class.radius,
            self.state.position.y,
            class.height,
            class.step_height,
            ctx.index,
            ctx.walls,
        ) && !solids.iter().any(|solid| {
            // A zero-motion separating sweep deliberately tolerates an old
            // overlap. Admission requires actual unoccupied standing space.
            solid.blocks_body(self.state.position.y, class.height)
                && solid.overlaps_disc(point.x, point.y, class.radius)
        }) && !crate::engine::collision::door_segment_blocked(
            point,
            point,
            class.radius,
            self.state.position.y,
            class.height,
            leaves,
        ) && self
            .ceiling
            .ceiling_above_disc(point.x, point.y, class.radius, self.state.position.y)
            .is_none_or(|ceiling| ceiling >= class.height + self.state.position.y - 0.001)
            && ctx
                .floor
                .walk_height_near(point.x, point.y, self.state.position.y, 0.02)
                .is_some_and(|floor| (floor - self.state.position.y).abs() <= 0.02)
    }

    pub fn tick(
        &mut self,
        ctx: &WorldContext<'_>,
        doors: &Doors,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) {
        if self.attachment.is_some() {
            return;
        }
        if !ctx.delta_seconds.is_finite() || ctx.delta_seconds <= 0.0 {
            return;
        }
        if self.state.enable_pending && self.clear(ctx, leaves, solids) {
            self.state.enabled = true;
            self.state.enable_pending = false;
        }
        if !self.enabled() {
            return;
        }
        let previous_position = self.state.position;
        let previous_yaw = self.state.yaw;
        let dt = ctx.delta_seconds.min(0.1);
        let old = self.state.policy.stage;
        let arrived = self
            .definition
            .routes
            .get(self.state.route)
            .is_some_and(|route| self.state.waypoint >= route.points.len());
        let distance_squared = self.state.position.distance_squared(ctx.feet);
        self.state.policy.advance(dt, distance_squared, arrived);
        self.next_plan = (self.next_plan - dt).max(0.0);
        self.state.next_voice = (self.state.next_voice - dt).max(0.0);
        self.speed = 0.0;
        self.blocked_by = None;
        if self.state.policy.stage == Stage::Leading && self.state.gesture.is_none() {
            self.move_route(ctx, doors, leaves, solids, dt);
        } else if self.state.policy.mode != super::Mode::Flee {
            self.face_player(ctx, dt);
        }
        if !self.clear(ctx, leaves, solids) {
            self.blocked_by = Some("post_step_clearance");
            self.state.position = previous_position;
            self.state.yaw = previous_yaw;
            self.state.policy.enter(Stage::Blocked);
            self.speed = 0.0;
        }
        let changed = old != self.state.policy.stage;
        self.status_changed |= changed;
        self.state.pose_restart |= changed;
        self.choose_pose(dt);
        self.sample_pose();
        self.sample_terrain(ctx);
        self.apply_terrain();
        self.choose_voice(changed);
        if self.state.policy.mode == super::Mode::Flee {
            self.settle_blocked_pose(dt);
        }
    }

    fn route_goal(&mut self) -> Option<Vec3> {
        let Some(route) = self.definition.routes.get(self.state.route) else {
            self.blocked_by = Some("route_missing");
            return None;
        };
        while route
            .points
            .get(self.state.waypoint)
            .is_some_and(|point| self.state.position.distance(Vec3::from(*point)) <= 0.08)
        {
            self.state.waypoint = self.state.waypoint.saturating_add(1);
            self.path.clear();
            self.next_plan = 0.0;
        }
        let Some(goal) = route
            .points
            .get(self.state.waypoint)
            .copied()
            .map(Vec3::from)
        else {
            self.state.policy.enter(Stage::Arrived);
            return None;
        };
        Some(goal)
    }

    fn move_route(
        &mut self,
        ctx: &WorldContext<'_>,
        doors: &Doors,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        dt: f32,
    ) {
        let Some(goal) = self.route_goal() else {
            return;
        };
        let Some(nav) = ctx.nav else {
            self.blocked_by = Some("navigation_missing");
            self.state.policy.enter(Stage::Blocked);
            return;
        };
        let class = navigation_class();
        let Some(class_index) = nav.compatible_class_index(&class) else {
            self.blocked_by = Some("navigation_profile_missing");
            self.state.policy.enter(Stage::Blocked);
            return;
        };
        if self.next_plan <= 0.0 {
            let query = PathQuery {
                class: class_index,
                start: self.state.position,
                goal,
                can_open_doors: false,
                max_expansions: 4096,
                doors: &Overlay {
                    doors,
                    solids,
                    leaves,
                },
            };
            if nav
                .path_into(&query, &mut self.scratch, &mut self.path)
                .is_err()
            {
                self.path.clear();
            }
            self.path_cursor = 1;
            self.next_plan = 0.4;
        }
        while self
            .path
            .waypoints
            .get(self.path_cursor)
            .is_some_and(|point| point.distance(self.state.position) <= 0.05)
        {
            self.path_cursor = self.path_cursor.saturating_add(1);
        }
        let Some(point) = self.path.waypoints.get(self.path_cursor).copied() else {
            self.blocked_by = Some("path_blocked");
            self.state.policy.enter(Stage::Blocked);
            return;
        };
        let target_yaw = (point.x - self.state.position.x).atan2(point.z - self.state.position.z);
        self.state.yaw = crate::engine::entity::turn_toward(self.state.yaw, target_yaw, 3.0 * dt);
        if crate::engine::entity::angle_difference(self.state.yaw, target_yaw).abs() > 0.12 {
            return;
        }
        let before = self.state.position;
        let mut body = AgentMove {
            position: before,
            yaw_degrees: self.state.yaw.to_degrees(),
            radius: class.radius,
            height: class.height,
            step_height: class.step_height,
            max_slope: class.max_slope,
            speed_mps: base_speed(ctx.player_walk_speed),
        };
        for _ in 0_u8..6 {
            match body.step_with_solids(
                point,
                dt / 6.0,
                ctx.walls,
                ctx.index,
                ctx.floor,
                &self.ceiling,
                leaves,
                solids,
            ) {
                MoveStep::Moved {
                    position,
                    yaw_degrees,
                    speed_mps: _,
                } => {
                    self.state.position = position;
                    self.state.yaw = yaw_degrees.to_radians();
                }
                MoveStep::Blocked => {
                    self.blocked_by = Some("physical_movement");
                    self.state.policy.enter(Stage::Blocked);
                    break;
                }
                MoveStep::Arrived => break,
            }
        }
        self.speed = before.distance(self.state.position) / dt;
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Bounded finite body yaw rotates toward the player's visible location without changing position."
    )]
    fn face_player(&mut self, ctx: &WorldContext<'_>, dt: f32) {
        let pointing = self.state.policy.stage == Stage::Pointing;
        let target = if pointing {
            self.definition
                .routes
                .get(self.state.route)
                .and_then(|route| route.points.get(self.state.waypoint))
                .copied()
                .map_or(ctx.feet, Vec3::from)
        } else if self.state.policy.mode == super::Mode::Talk {
            self.definition.look_at.map_or(ctx.feet, Vec3::from)
        } else {
            ctx.feet
        };
        let delta = target - self.state.position;
        if delta.x.hypot(delta.z) > 0.01 {
            let yaw = delta.x.atan2(delta.z)
                - if pointing {
                    std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                };
            self.state.yaw = crate::engine::entity::turn_toward(self.state.yaw, yaw, 3.0 * dt);
        }
    }

    fn choose_voice(&mut self, changed: bool) {
        if self.voice.is_some() {
            return;
        }
        if changed && self.state.policy.stage == Stage::AttentionJump {
            self.voice = Some("feltling_attention");
            self.state.next_voice = 5.0;
            return;
        }
        if self.state.next_voice > 0.0 {
            return;
        }
        self.voice = match self.state.policy.stage {
            Stage::AttentionJump if changed => Some("feltling_attention"),
            Stage::Greeting | Stage::Talking => Some("feltling_babble"),
            Stage::Cheering => Some("feltling_cheer"),
            Stage::Pointing
            | Stage::Leading
            | Stage::Waiting
            | Stage::AttentionJump
            | Stage::AttentionWave
            | Stage::Arrived
            | Stage::Blocked => None,
        };
        if self.voice.is_some() {
            self.state.next_voice = if self.state.policy.stage == Stage::Cheering {
                3.6
            } else {
                5.0
            };
        }
    }
}

#[cfg(test)]
#[path = "../tests/feltling_motion.rs"]
mod tests;

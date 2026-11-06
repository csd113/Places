//! Terrain support follows rendered treads, while authored foot lift controls
//! stance. Raising the floor must never suppress a real swing phase.
use super::{Guest, smooth};
use crate::engine::entities::WorldContext;
use glam::{Quat, Vec3};

fn height(ctx: &WorldContext<'_>, point: Vec3, root: f32) -> f32 {
    ctx.floor
        .height_below(point.x, point.z, 0.45_f32.mul_add(1.0, root))
        .unwrap_or(root)
}
impl Guest {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite authored foot vectors sample nearby terrain within a bounded half-metre support region."
    )]
    pub(super) fn sample_support(&mut self, ctx: &WorldContext<'_>) {
        let root = self.state.position.y;
        let forward = Quat::from_rotation_y(self.state.yaw).mul_vec3(Vec3::Z);
        for (index, name) in ["foot_front", "foot_left", "foot_right"]
            .into_iter()
            .enumerate()
        {
            let Some(socket) = self.socket(name) else {
                continue;
            };
            let position = socket.transform_point3(Vec3::ZERO);
            let floor = height(ctx, position, root);
            if let Some(value) = self.support_heights.get_mut(index) {
                *value = floor;
            }
            if let Some(value) = self.swing_heights.get_mut(index) {
                *value = height(ctx, forward.mul_add(Vec3::splat(0.35), position), root);
            }
            let dx = height(ctx, position + Vec3::X * 0.15, root)
                - height(ctx, position - Vec3::X * 0.15, root);
            let dz = height(ctx, position + Vec3::Z * 0.15, root)
                - height(ctx, position - Vec3::Z * 0.15, root);
            // A riser is a flat tread, not a sharply tilted support plane.
            let normal = if dx.abs() < 0.18 && dz.abs() < 0.18 {
                Vec3::new(-dx, 0.3, -dz).normalize_or_zero()
            } else {
                Vec3::Y
            };
            if let Some(value) = self.support_normals.get_mut(index) {
                *value = normal;
            }
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Unit quaternion composition rotates the foot into a finite normalized support plane."
    )]
    pub(super) fn terrain_rotation(&self, index: usize, rotation: Quat) -> Quat {
        let normal = self.support_normals.get(index).copied().unwrap_or(Vec3::Y);
        Quat::from_rotation_arc(Vec3::Y, normal) * rotation
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite inverse root transform and unit quaternion composition preserve the authored limb frame."
    )]
    pub(super) fn swing_terrain(
        &mut self,
        index: usize,
        suffix: &str,
        position: Vec3,
        rotation: Quat,
        lift: f32,
    ) {
        let floor = self
            .support_heights
            .get(index)
            .copied()
            .unwrap_or(self.state.position.y);
        let future = self.swing_heights.get(index).copied().unwrap_or(floor);
        if (floor - self.state.position.y).abs() < 0.001 && (future - floor).abs() < 0.001 {
            return;
        }
        let raised = (future - floor)
            .max(0.0)
            .mul_add(smooth(lift / 0.06), floor);
        let target = Vec3::new(position.x, raised + 0.055 + lift, position.z);
        let local = self.transform().inverse().transform_point3(target);
        let turn = Quat::from_rotation_y(-self.state.yaw) * self.terrain_rotation(index, rotation);
        self.solve_leg(suffix, local, turn);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite inverse root transform and unit quaternion composition preserve the planted foot frame."
    )]
    pub(super) fn seat_terrain_soles(&mut self, ctx: &WorldContext<'_>) {
        for (index, suffix) in ["front", "left", "right"].into_iter().enumerate() {
            let floor = self
                .support_heights
                .get(index)
                .copied()
                .unwrap_or(self.state.position.y);
            let future = self.swing_heights.get(index).copied().unwrap_or(floor);
            if (floor - self.state.position.y).abs() < 0.001 && (future - floor).abs() < 0.001 {
                continue;
            }
            let planted = self.state.plants.get(index).and_then(Option::as_ref);
            let supported = planted.is_some();
            let Some(foot) = self.socket(&format!("foot_{suffix}")) else {
                continue;
            };
            let (_, current_rotation, current_position) = foot.to_scale_rotation_translation();
            let (position, rotation) = planted
                .map_or((current_position, current_rotation), |plant| {
                    (plant.position, plant.rotation)
                });
            let mut target = self.transform().inverse().transform_point3(position);
            let turn = Quat::from_rotation_y(-self.state.yaw) * rotation;
            for _ in 0_u8..4 {
                let Some(minimum) = self.sole_clearance(index, ctx) else {
                    break;
                };
                let change = (0.001 - minimum).clamp(if supported { -0.04 } else { 0.0 }, 0.04);
                if change.abs() < 0.0001 {
                    break;
                }
                target.y += change;
                self.solve_leg(suffix, target, turn);
            }
            let requested = self.transform().transform_point3(target);
            if let Some(actual) = self
                .socket(&format!("foot_{suffix}"))
                .map(|matrix| matrix.transform_point3(Vec3::ZERO))
                && actual.distance(requested) <= 0.002
                && let Some(Some(plant)) = self.state.plants.get_mut(index)
            {
                plant.position = actual;
            }
        }
    }

    fn sole_clearance(&self, index: usize, ctx: &WorldContext<'_>) -> Option<f32> {
        self.sole_vertices
            .get(index)?
            .iter()
            .filter_map(|slot| {
                let vertex = self.asset.model.vertices.get(*slot)?;
                let joints = self.asset.model.joints.get(*slot)?;
                let weights = self.asset.model.weights.get(*slot)?;
                let point = self.transform().transform_point3(Vec3::from(
                    self.animator.skin_position(*joints, *weights, vertex.pos),
                ));
                Some(point.y - height(ctx, point, self.state.position.y))
            })
            .min_by(f32::total_cmp)
    }
}

impl Guest {
    pub(super) fn step_in_place(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[crate::engine::collision::DoorCollider],
        solids: &[crate::engine::collision::WallAabb],
        sub: f32,
        scene: &dyn super::prop_track::Scene,
    ) -> bool {
        if self.state.blocked_seconds > 0.8
            || self.state.transit.is_some()
            || self.passage_plan.is_some()
            || self.state.goal.is_none()
            || !matches!(
                self.state.intent,
                super::Intent::Patrol
                    | super::Intent::Chase
                    | super::Intent::InvestigateNoise
                    | super::Intent::Search
                    | super::Intent::Listen
            )
        {
            return false;
        }
        let saved = self.state.clone();
        let physical = self.animator.joint_pose();
        if !matches!(self.state.clip.as_str(), "walk_slow" | "run_pursuit") {
            self.set_clip("run_pursuit");
            self.state.motion_rate = (self.state.base_speed / super::RUN_REFERENCE).min(7.0);
        }
        if self.state.motion_rate <= 0.01 {
            self.state.motion_rate = if self.state.clip == "walk_slow" {
                super::TURN_RATE / 20.0
            } else {
                self.state.base_speed / super::RUN_REFERENCE
            };
        }
        let before = self.state.clip_time;
        self.advance_pose_clocks(sub);
        if self.clear(ctx, leaves, solids) {
            let after = if self.state.clip_time < before {
                self.state.clip_time + super::duration(&self.state.clip)
            } else {
                self.state.clip_time
            };
            self.update_markers(before, after, ctx, scene);
            self.state.blocked_seconds += sub;
            return true;
        }
        self.state = saved;
        let _restored = self.restore_physical(&physical);
        false
    }
}

impl Guest {
    pub(super) fn contacts_fit(&self) -> bool {
        ["foot_front", "foot_left", "foot_right"]
            .into_iter()
            .zip(&self.state.plants)
            .all(|(name, contact)| {
                contact.as_ref().is_none_or(|plant| {
                    self.socket(name).is_some_and(|matrix| {
                        matrix.transform_point3(Vec3::ZERO).distance(plant.position) <= 0.02
                    })
                })
            })
    }
}

impl Guest {
    pub(super) fn log_rejected_pose(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[crate::engine::collision::DoorCollider],
        solids: &[crate::engine::collision::WallAabb],
    ) {
        if std::env::var_os("PLACES_BENCH").is_none() || self.state.clock < self.next_clear_log {
            return;
        }
        self.next_clear_log = self.state.clock + 0.5;
        let feet = self.state.position.y;
        let ground = self.vertices.iter().find_map(|point| {
            ctx.floor
                .height_below(point.x, point.z, feet + 0.45)
                .filter(|floor| point.y + 0.005 < *floor)
                .map(|floor| (*point, floor))
        });
        let wall = ctx
            .walls
            .iter()
            .chain(solids)
            .find(|wall| wall.max_y > feet + 0.005 && self.intersects(**wall, super::MARGIN));
        let leaf = leaves.iter().any(|leaf| self.door_intersects(leaf));
        let contacts:Vec<_>=["foot_front","foot_left","foot_right"].into_iter().zip(&self.state.plants).filter_map(|(name,plant)| {
            let target=plant.as_ref()?.position;
            let actual=self.socket(name)?.transform_point3(Vec3::ZERO);
            Some(serde_json::json!({"name":name,"target":target,"actual":actual,"error":actual.distance(target)}))
        }).collect();
        self.store_event(serde_json::json!({"actor":self.state.id,"episode":self.state.episode,"time":self.state.clock,"type":"pose_rejected","detail":{"position":self.state.position,"clip":self.state.clip,"clip_time":self.state.clip_time,"ground":ground,"wall":wall.map(|w|[w.min_x,w.min_y,w.min_z,w.max_x,w.max_y,w.max_z]),"leaf":leaf,"contacts":contacts}}));
    }
}
impl Guest {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite authored limb lengths and world planted positions define a bounded vertical support correction; no scale changes."
    )]
    fn required_support_lowering(&self) -> f32 {
        let world = self.transform();
        let mut required = 0.0_f32;
        for (suffix, contact) in ["front", "left", "right"]
            .into_iter()
            .zip(&self.state.plants)
        {
            let Some(plant) = contact.as_ref() else {
                continue;
            };
            let Some(upper) = self.animator.node_global(&format!("leg_{suffix}_upper")) else {
                continue;
            };
            let Some(lower) = self.animator.node_global(&format!("leg_{suffix}_lower")) else {
                continue;
            };
            let Some(foot) = self.animator.node_global(&format!("foot_{suffix}")) else {
                continue;
            };
            let hip = world.transform_point3(upper.transform_point3(Vec3::ZERO));
            let knee = world.transform_point3(lower.transform_point3(Vec3::ZERO));
            let ankle = world.transform_point3(foot.transform_point3(Vec3::ZERO));
            let reach = (hip.distance(knee) + knee.distance(ankle) - 0.003).max(0.01);
            let offset = hip - plant.position;
            let horizontal = offset.x.mul_add(offset.x, offset.z * offset.z);
            let vertical = reach.mul_add(reach, -horizontal).max(0.0).sqrt();
            required = required.max(offset.y - vertical);
        }
        required.clamp(0.0, 0.3)
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The unit-scale root moves at most0.3m down and each original foot is re-solved with the same authored segment lengths."
    )]
    pub(super) fn support_posture(&mut self) {
        self.state.support_lowering = self
            .state
            .support_lowering
            .max(self.required_support_lowering());
        let amount = self.state.support_lowering;
        if amount <= 0.0001 {
            return;
        }
        let targets: [Option<(Vec3, Quat)>; 3] = ["front", "left", "right"].map(|suffix| {
            self.animator
                .node_global(&format!("foot_{suffix}"))
                .map(|matrix| {
                    let (_, rotation, position) = matrix.to_scale_rotation_translation();
                    (position, rotation)
                })
        });
        if let Some(root) = self.animator.node_global("root") {
            let (scale, rotation, position) = root.to_scale_rotation_translation();
            let _root = self.animator.set_node_global(
                "root",
                glam::Mat4::from_scale_rotation_translation(
                    scale,
                    rotation,
                    position - Vec3::Y * amount,
                ),
            );
        }
        for (suffix, target) in ["front", "left", "right"].into_iter().zip(targets) {
            if let Some((position, rotation)) = target {
                self.solve_leg(suffix, position, rotation);
            }
        }
    }
}
impl Guest {
    /// A blocked translation can still need a legal lift before the next step.
    /// Preserve that phase for at most0.8s, then wait/replan rather than resetting
    /// the gait to frame zero on every contact with a riser or turn.
    pub(super) fn stationary_step(&mut self, previous: &super::Record, sub: f32) -> bool {
        if self.state.transit.is_some()
            || self.state.position.distance(previous.position) >= 0.05 * sub
            || crate::engine::entity::angle_difference(self.state.yaw, previous.yaw).abs()
                >= 0.000_001
            || !matches!(self.state.clip.as_str(), "walk_slow" | "run_pursuit")
        {
            return false;
        }
        self.state.speed = 0.0;
        if self.passage_plan.is_some() {
            self.state.motion_rate = 0.0;
            return false;
        }
        let wanted = self
            .state
            .goal
            .is_some_and(|goal| goal.distance(self.state.position) > 0.35);
        if wanted && self.state.blocked_seconds < 0.8 {
            self.state.motion_rate = self.state.motion_rate.max(1.0);
        } else {
            self.set_clip("idle_listen");
        }
        wanted
    }
    pub(super) fn accepted_contact_step(&mut self, stationary: bool, sub: f32) {
        if !stationary {
            self.state.blocked_seconds = 0.0;
            return;
        }
        self.state.blocked_seconds += sub;
        if self.state.blocked_seconds >= 2.0 {
            self.recover_blockage();
            self.state.blocked_seconds = 0.0;
        }
    }
    pub(super) fn capture_authored_lifts(&mut self) {
        for (index, name) in ["foot_front", "foot_left", "foot_right"]
            .into_iter()
            .enumerate()
        {
            let lift = self.socket(name).map_or(0.0, |matrix| {
                (matrix.transform_point3(Vec3::ZERO).y - self.state.position.y - 0.055).max(0.0)
            });
            if let Some(value) = self.authored_lifts.get_mut(index) {
                *value = lift;
            }
        }
    }
}

//! One prey root/pose owner; normal escape motion remains live until lip contact.
use super::Feltling;
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use crate::engine::entity::JointPose;
use glam::{Mat4, Quat, Vec3};

impl Feltling {
    #[must_use]
    pub fn capture_profile(
        &self,
    ) -> Option<&'static crate::engine::tall_guest::prey::CaptureProfile> {
        let profile = &crate::engine::tall_guest::prey::FELTLING;
        crate::engine::props::reviewed_source_matches(&self.asset.model_path, profile.prey_sha256)
            .then_some(profile)
    }

    #[must_use]
    pub fn root_pose(&self) -> (Vec3, Quat) {
        self.attachment
            .unwrap_or_else(|| (self.state.position, Quat::from_rotation_y(self.state.yaw)))
    }

    #[must_use]
    pub fn capture_ready(&self) -> bool {
        self.enabled()
            && self.attachment.is_none()
            && self.speed <= 0.001
            && self.settle_seconds >= super::BLEND_SECONDS
            && self.blocked_by.is_some()
    }

    pub(super) fn settle_blocked_pose(&mut self, dt: f32) {
        if self.speed > 0.001 || self.blocked_by.is_none() {
            self.settle_seconds = 0.0;
            self.settle_from.clear();
            return;
        }
        if self.settle_from.is_empty() {
            self.settle_from = self.animator.joint_pose();
        }
        self.settle_seconds = (self.settle_seconds + dt).min(super::BLEND_SECONDS);
        let weight =
            crate::engine::tall_guest::prey::smooth(self.settle_seconds / super::BLEND_SECONDS);
        let pose: Vec<JointPose> = self
            .settle_from
            .iter()
            .zip(&self.bind_pose)
            .map(|(from, to)| JointPose {
                translation: from.translation.lerp(to.translation, weight),
                rotation: from.rotation.slerp(to.rotation, weight),
                scale: to.scale,
            })
            .collect();
        let _accepted =
            self.animator
                .restore_physical_pose(&self.state.clip, self.state.progress, &pose);
        self.base_pose = pose;
    }

    /// Called only after the physical lip/crown gate has passed.
    pub fn attach(&mut self, pose: (Vec3, Quat)) {
        self.attachment = Some(pose);
        self.state.position = pose.0;
        self.voice = None;
        self.animation_event = None;
        self.speed = 0.0;
        if self.animator.joint_pose() != self.bind_pose {
            let _restored = self
                .animator
                .restore_physical_pose("Run", 0.0, &self.bind_pose);
        }
    }

    #[must_use]
    pub fn physical_vertices(&self) -> Vec<Vec3> {
        let (position, rotation) = self.root_pose();
        let matrix = Mat4::from_rotation_translation(rotation, position);
        self.asset
            .model
            .vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                matrix.transform_point3(Vec3::from_array(
                    self.animator.skin_position(
                        self.asset
                            .model
                            .joints
                            .get(index)
                            .copied()
                            .unwrap_or([0; 4]),
                        self.asset
                            .model
                            .weights
                            .get(index)
                            .copied()
                            .unwrap_or([0.0; 4]),
                        vertex.pos,
                    ),
                ))
            })
            .collect()
    }

    /// Detach only at the already swept, grounded contact pose. No position or
    /// scale teleport: the current full root must coincide with that pose.
    pub(crate) fn release_contact(&mut self, position: Vec3, yaw: f32) -> bool {
        let current = self.root_pose();
        if current.0.distance(position) > 0.000_02
            || current.1.dot(Quat::from_rotation_y(yaw)).abs() < 0.999_999
        {
            return false;
        }
        self.state.position = current.0;
        self.state.yaw = yaw;
        self.attachment = None;
        self.reserved_by = None;
        self.release_pose = None;
        self.settle_seconds = 0.0;
        self.settle_from.clear();
        self.state.pose_restart = true;
        true
    }

    /// An interrupted attachment returns to its retained grounded pose only if
    /// that exact space is currently clear. Otherwise keep the accepted pose.
    pub fn release_at(
        &mut self,
        position: Vec3,
        yaw: f32,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        let old_position = self.state.position;
        let old_yaw = self.state.yaw;
        self.state.position = position;
        self.state.yaw = yaw;
        let mut occupied = solids.to_vec();
        occupied.push(WallAabb::with_y(
            ctx.feet.x - ctx.body_half,
            ctx.feet.y,
            ctx.feet.z - ctx.body_half,
            ctx.body_half * 2.0,
            ctx.body_height,
            ctx.body_half * 2.0,
        ));
        if !self.clear(ctx, leaves, &occupied) {
            self.state.position = old_position;
            self.state.yaw = old_yaw;
            return false;
        }
        self.attachment = None;
        self.reserved_by = None;
        self.settle_seconds = 0.0;
        self.settle_from.clear();
        self.state.pose_restart = true;
        true
    }
}

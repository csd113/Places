//! Replays only previously accepted physical poses when a committed passage
//! becomes obstructed. Every reverse step is checked against the live world.
use super::{Guest, Intent, Plant, Record, Transit};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use glam::Vec3;

#[derive(PartialEq, Eq)]
pub(super) enum Mode {
    Forward,
    Retreat,
}

#[derive(Clone)]
pub(super) struct Pose {
    joints: Vec<crate::engine::entity::JointPose>,
    position: Vec3,
    yaw: f32,
    clip: String,
    time: f32,
    previous: String,
    previous_time: f32,
    blend: f32,
    plants: [Option<Plant>; 3],
    transit: Option<Transit>,
    aim: f32,
    posture: f32,
    support_lowering: f32,
    rate: f32,
}
impl Pose {
    pub fn capture(state: &Record, joints: Vec<crate::engine::entity::JointPose>) -> Self {
        Self {
            joints,
            position: state.position,
            yaw: state.yaw,
            clip: state.clip.clone(),
            time: state.clip_time,
            previous: state.previous.clone(),
            previous_time: state.previous_time,
            blend: state.blend_time,
            plants: state.plants.clone(),
            transit: state.transit.clone(),
            aim: state.aim_weight,
            posture: state.base_posture,
            support_lowering: state.support_lowering,
            rate: state.motion_rate,
        }
    }
    fn apply(&self, state: &mut Record) {
        state.position = self.position;
        state.yaw = self.yaw;
        state.clip.clone_from(&self.clip);
        state.clip_time = self.time;
        state.previous.clone_from(&self.previous);
        state.previous_time = self.previous_time;
        state.blend_time = self.blend;
        state.plants.clone_from(&self.plants);
        state.transit.clone_from(&self.transit);
        state.aim_weight = self.aim;
        state.base_posture = self.posture;
        state.support_lowering = self.support_lowering;
        state.motion_rate = self.rate;
        state.speed = 0.0;
    }
}
impl Guest {
    pub(super) fn retain_passage_pose(
        &mut self,
        state: &Record,
        joints: Vec<crate::engine::entity::JointPose>,
    ) {
        if self.retreat_poses.len() == 8192 {
            let _old = self.retreat_poses.pop_front();
        }
        self.retreat_poses.push_back(Pose::capture(state, joints));
    }

    pub(super) fn recover_blockage(&mut self) {
        self.next_plan = 0.0;
        if self.state.transit.is_some() && !self.retreat_poses.is_empty() {
            self.recovery = Mode::Retreat;
            self.log("traversal_changed", &serde_json::json!({"phase":"Retreat","reason":"forward trajectory obstructed","verified_history":self.retreat_poses.len()}));
        } else if matches!(
            self.state.intent,
            Intent::Inspect | Intent::EscapeFrustration
        ) {
            self.set_intent(Intent::Search, "obstructed action; bounded safe recovery");
            self.set_clip("idle_listen");
            self.state.search_until = self.search_end();
            self.state.goal = None;
            self.state.prop_track = None;
            self.stop_pursuit();
        }
    }

    fn forward_available(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        if self.state.clock < self.next_forward_probe {
            return false;
        }
        self.next_forward_probe = self.state.clock + 0.25;
        let live = self.state.clone();
        let physical = self.animator.joint_pose();
        let sounds = self.sounds.len();
        self.posing_candidate = true;
        self.candidate_events.clear();
        let mut legal = true;
        for _ in 0_u8..48 {
            self.advance_motion(ctx, leaves, solids, 1.0 / 240.0);
            self.advance_pose_clocks(1.0 / 240.0);
            if !self.clear(ctx, leaves, solids) {
                legal = false;
                break;
            }
            if self.state.transit.is_none() {
                break;
            }
        }
        self.state = live;
        self.sounds.truncate(sounds);
        self.posing_candidate = false;
        self.candidate_events.clear();
        let _restored = self.restore_physical(&physical);
        legal
    }

    pub(super) fn retreat_step(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        if self.recovery != Mode::Retreat {
            return false;
        }
        let Some(target) = self.retreat_poses.back().cloned() else {
            self.recovery = Mode::Forward;
            return false;
        };
        let live = self.state.clone();
        let physical = self.animator.joint_pose();
        target.apply(&mut self.state);
        let _historical_restored = self.restore_physical(&target.joints);
        if !self.pose_clear(ctx, leaves, solids) {
            self.state = live;
            let _restored = self.restore_physical(&physical);
            if self.forward_available(ctx, leaves, solids) {
                self.recovery = Mode::Forward;
                self.log("traversal_changed", &serde_json::json!({"phase":"ForwardRecovery","reason":"retreat occupied; forward trajectory reopened"}));
                return false;
            }
            return true;
        }
        let _accepted = self.retreat_poses.pop_back();
        if self.state.transit.is_none() {
            self.retreat_poses.clear();
            self.recovery = Mode::Forward;
            self.next_passage_check = self.state.clock + 2.0;
            self.log("traversal_changed", &serde_json::json!({"phase":"Retreated","position":self.state.position,"world_scale":1_u8}));
            if self.state.pending_loss && !self.fresh() {
                self.state.pending_loss = false;
                self.request_loss();
            }
        }
        true
    }
}

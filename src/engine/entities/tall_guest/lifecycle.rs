//! Behavior lifetime is separate from the retained last safe physical pose.
use super::{Guest, Intent};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Life {
    #[default]
    Active,
    Disabled,
    EnablePending,
}

impl Guest {
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.state.life == Life::Active
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        if enabled {
            if self.state.life != Life::Disabled {
                return false;
            }
            self.state.life = Life::EnablePending;
            self.log(
                "enable_pending",
                &serde_json::json!({"reason":"current full pose must fit before activation"}),
            );
            return true;
        }
        if self.state.life == Life::Disabled {
            return false;
        }
        self.sounds.clear();
        self.voice(None, 0.0);
        self.set_intent(Intent::Disabled, "entity disabled");
        self.state.life = Life::Disabled;
        self.state.observation = None;
        self.state.pending.clear();
        self.state.impacts.clear();
        self.state.heard_impacts.clear();
        self.state.prop_track = None;
        self.state.goal = None;
        self.state.pending_loss = false;
        self.state.speed = 0.0;
        self.state.frustration_probe_until = 0.0;
        self.path.clear();
        self.passage_plan = None;
        self.retreat_poses.clear();
        self.attack_request = false;
        true
    }

    /// Re-enable in the retained pose only after it fits the live world. A
    /// compressed actor resumes that posture; an occupied actor stays hidden.
    pub(super) fn activate_if_clear(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        if self.state.life == Life::Active {
            return true;
        }
        if self.state.life != Life::EnablePending || self.state.clock < self.next_activation_check {
            return false;
        }
        self.next_activation_check = self.state.clock + 0.25;
        let saved = self.state.clone();
        self.state.life = Life::Active;
        if !self.pose_clear(ctx, leaves, solids) {
            self.state = saved;
            return false;
        }
        self.state.life = Life::Active;
        self.set_intent(
            Intent::Patrol,
            "retained physical pose validated for activation",
        );
        self.state.next_pulse = self.state.clock + 0.7;
        self.next_plan = 0.0;
        if self.state.transit.is_none() {
            self.set_clip("idle_listen");
        }
        true
    }
}

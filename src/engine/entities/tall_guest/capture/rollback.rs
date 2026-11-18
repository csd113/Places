//! Collision-checked reverse playback cancels a blocked action without consuming
//! or teleporting prey. A blocked return remains an explicit, resumable state.
use super::{Capture, Direction, Guest};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use crate::engine::feltling::Feltling;

impl Guest {
    pub(super) fn begin_capture_rollback(&mut self, action: &mut Capture, reason: &str) {
        if action.direction == Direction::Reverse {
            return;
        }
        action.direction = Direction::Reverse;
        action.blocked_seconds = 0.0;
        if !action.consumed {
            self.capture_failure = Some((action.key, action.start.0));
        }
        self.log("capture_aborted", &serde_json::json!({"key":action.key,"reason":reason,"clock":action.clock,"latched":action.latched}));
    }

    pub(super) fn tick_capture_rollback(
        &mut self,
        action: &mut Capture,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        mut target: Option<&mut Feltling>,
    ) -> bool {
        let mut left = ctx.delta_seconds.clamp(0.0, 0.1) * 1.5;
        for _ in 0_u8..32 {
            if action.clock <= action.entry_phase + 0.000_001 {
                if !action.released && !action.consumed {
                    if !target
                        .as_mut()
                        .is_some_and(|prey| prey.release_contact(action.start.0, action.start.1))
                    {
                        return false;
                    }
                    action.released = true;
                    self.log("prey_released", &serde_json::json!({"key":action.key,"position":action.start.0,"scale":1_u8,"swept_return":true}));
                }
                if let Some(prey) = target.as_mut() {
                    prey.reserved_by = None;
                    prey.release_pose = None;
                }
                self.log("capture_abort_recovered", &serde_json::json!({"key":action.key,"prey_released":action.released,"consumed":action.consumed}));
                self.finish_capture();
                return true;
            }
            if left <= 0.000_001 {
                break;
            }
            let contact = action.profile.contact_seconds;
            let boundary = if action.clock > contact + 0.000_001 {
                contact
            } else if action.clock > action.entry_phase + action.entry_blend_seconds + 0.000_001 {
                action.entry_phase + action.entry_blend_seconds
            } else {
                action.entry_phase
            };
            let sub = left.min(1.0 / 120.0).min(action.clock - boundary);
            left -= sub;
            let Some(step) = self.capture_candidate(
                action,
                action.clock - sub,
                target.as_deref_mut(),
                ctx,
                leaves,
                solids,
            ) else {
                action.blocked_seconds += ctx.delta_seconds.clamp(0.0, 0.1);
                if action.released && action.blocked_seconds >= 0.5 {
                    self.log("capture_abort_recovered", &serde_json::json!({"key":action.key,"prey_released":true,"ordinary_pose_recovery":true}));
                    self.finish_capture();
                    return true;
                }
                if action.blocked_seconds >= 0.5 {
                    self.log("capture_recovery_waiting", &serde_json::json!({"key":action.key,"clock":action.clock,"released":action.released,"reason":"live return path obstructed; retrying without penetration"}));
                    action.blocked_seconds = 0.0;
                }
                break;
            };
            action.blocked_seconds = 0.0;
            action.clock = step.clock;
            self.accept_capture_pose(&step);
        }
        false
    }
}

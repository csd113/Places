use super::{BLEND_SECONDS, CLIPS, Feltling, Stage};

impl Feltling {
    pub(super) fn choose_pose(&mut self, dt: f32) {
        let (name, progress) = if let Some((name, looped)) = self.state.gesture.clone() {
            self.state.gesture_time = (self.state.gesture_time + dt).min(10_000_000.0);
            let duration = CLIPS
                .iter()
                .find(|(clip, _)| *clip == name)
                .map_or(1.0, |(_, duration)| *duration);
            let progress = if looped {
                (self.state.gesture_time / duration).fract()
            } else {
                (self.state.gesture_time / duration).min(1.0)
            };
            if !looped && self.state.gesture_time >= duration {
                self.state.gesture = None;
                self.animation_event = Some(name.clone());
            }
            (name, progress)
        } else {
            match self.state.policy.stage {
                Stage::Leading if self.speed > 0.01 => {
                    // Measured travel owns the phase. Native contact/gait review
                    // is required before accepting this very small fast actor.
                    (
                        "Run".into(),
                        (self.state.progress
                            + dt * self.speed / (self.run_reference * 0.666_666_7))
                            .fract(),
                    )
                }
                Stage::Pointing => (
                    "Pointing".into(),
                    (self.state.policy.elapsed / 2.0).min(1.0),
                ),
                Stage::AttentionJump => (
                    "Excited_Jumping".into(),
                    (self.state.policy.elapsed / 1.8).min(1.0),
                ),
                Stage::AttentionWave => (
                    "Waving_Both_Arms".into(),
                    (self.state.policy.elapsed / 2.4).min(1.0),
                ),
                Stage::Cheering => (
                    "Excited_Jumping".into(),
                    (self.state.policy.elapsed / 1.8).fract(),
                ),
                Stage::Greeting => (
                    "Waving_Both_Arms".into(),
                    ((self.state.policy.elapsed % 6.0) / 2.4).min(1.0),
                ),
                Stage::Leading
                | Stage::Waiting
                | Stage::Arrived
                | Stage::Talking
                | Stage::Blocked => ("Waving_Both_Arms".into(), 0.0),
            }
        };
        let restart = std::mem::take(&mut self.state.pose_restart);
        if self.state.clip != name || restart {
            // Freeze the accepted animation blend, including an interruption.
            // Terrain is reapplied after sampling rather than blended twice.
            self.state.blend_from.clone_from(&self.base_pose);
            self.state.clip = name;
            self.state.blend = 0.0;
        }
        self.state.progress = progress;
        self.state.blend = (self.state.blend + dt).min(BLEND_SECONDS);
    }

    pub(super) fn sample_pose(&mut self) {
        let _sampled = self
            .sampler
            .sample_fraction(&self.state.clip, self.state.progress);
        let mut pose = self.sampler.joint_pose();
        if self.state.blend < BLEND_SECONDS {
            let weight = self.state.blend / BLEND_SECONDS;
            for (target, previous) in pose.iter_mut().zip(&self.state.blend_from) {
                target.translation = previous.translation.lerp(target.translation, weight);
                target.rotation = previous.rotation.slerp(target.rotation, weight);
                target.scale = previous.scale.lerp(target.scale, weight);
            }
        }
        self.base_pose.clone_from(&pose);
        if pose != self.animator.joint_pose() {
            let _restored =
                self.animator
                    .restore_physical_pose(&self.state.clip, self.state.progress, &pose);
        }
    }
}

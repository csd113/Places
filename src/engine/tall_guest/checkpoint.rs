//! Strict semantic persistence. Cached routes, solver handles and retreat history
//! are rebuilt; original evidence timestamps and clip markers are retained.
use super::{CLIPS, Guest, Record};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use glam::Vec3;
use std::collections::BTreeSet;

pub(super) fn between(value: f32, min: f32, max: f32) -> Result<(), String> {
    if value.is_finite() && value >= min && value <= max {
        Ok(())
    } else {
        Err("Tall Guest checkpoint number outside its supported range".into())
    }
}
pub(super) fn point(value: Vec3) -> Result<(), String> {
    for coordinate in value.to_array() {
        between(coordinate, -100_000.0, 100_000.0)?;
    }
    Ok(())
}
pub(super) fn sample_time(time: f32, now: f32) -> Result<(), String> {
    between(time, 0.0, now)
}
fn observation(value: &super::Observation, record: &Record) -> Result<(), String> {
    point(value.position)?;
    point(value.aim_point)?;
    point(value.velocity)?;
    sample_time(value.sampled, record.clock)?;
    between(value.quality, 0.0, 1.0)?;
    between(value.confidence, 0.0, 1.0)?;
    between(value.uncertainty, 0.0, 100_000.0)?;
    if value.track != 1 || value.pulse == 0 || value.pulse > record.pulse {
        return Err("Invalid saved body-echo correlation".into());
    }
    Ok(())
}
pub(super) fn impact(value: &crate::engine::physics::ImpactEvent) -> Result<(), String> {
    point(value.position)?;
    between(value.impulse, 0.0, 1_000_000.0)?;
    between(value.speed, 0.0, 1000.0)?;
    between(value.strength, 0.0, 1.0)?;
    if value.key == 0
        || value.causal_source == 0
        || value.incident == 0
        || value.other_key == Some(0)
    {
        return Err("Invalid saved physical release evidence".into());
    }
    Ok(())
}
impl Record {
    fn validate_pose(&self) -> Result<(), String> {
        point(self.position)?;
        point(self.noise_progress)?;
        between(self.yaw, -100_000.0, 100_000.0)?;
        for (name, time) in [
            (&self.clip, self.clip_time),
            (&self.previous, self.previous_time),
        ] {
            let seconds = CLIPS
                .iter()
                .find(|(clip, _)| *clip == name)
                .ok_or("Unknown saved Guest clip")?
                .1;
            between(time, 0.0, seconds)?;
        }
        between(self.blend_time, 0.0, super::BLEND)?;
        between(self.aim_weight, 0.0, 0.8)?;
        between(self.base_posture, 0.0, 1.0)?;
        between(self.support_lowering, 0.0, 0.3)?;
        between(self.motion_rate, 0.0, 7.0)?;
        between(self.speed, 0.0, 12.0)?;
        between(self.base_speed, 1.0, 11.1)?;
        between(self.pursuit_speed, 1.0, 12.1)?;
        for plant in self.plants.iter().flatten() {
            point(plant.position)?;
            if !plant.rotation.is_finite()
                || !(0.999..=1.001).contains(&plant.rotation.length_squared())
            {
                return Err("Invalid saved foot rotation".into());
            }
        }
        if let Some(transit) = &self.transit {
            point(transit.entry)?;
            point(transit.exit)?;
            between(transit.yaw, -100_000.0, 100_000.0)?;
            between(transit.elapsed, 0.0, 30.0)?;
            between(transit.progress, 0.0, 1.0)?;
            if transit.phase > 2 || !(0.1..=12.01).contains(&transit.entry.distance(transit.exit)) {
                return Err("Invalid saved passage trajectory".into());
            }
        }
        Ok(())
    }

    fn validate_evidence(&self) -> Result<(), String> {
        if self.pending.len() > 4
            || self.impacts.len() > 32
            || self.heard_impacts.len() > 32
            || self.incidents.len() > 128
            || self.source_cooldowns.len() > 128
        {
            return Err("Saved Tall Guest evidence exceeds bounded capacity".into());
        }
        if let Some(value) = &self.observation {
            observation(value, self)?;
        }
        let mut previous = 0.0;
        let mut pulses = BTreeSet::new();
        for value in &self.pending {
            observation(&value.observation, self)?;
            point(value.origin)?;
            point(value.hit)?;
            between(
                value.deliver,
                value.observation.sampled,
                0.36_f32.mul_add(1.0, value.observation.sampled),
            )?;
            if value.deliver < previous || !pulses.insert(value.observation.pulse) {
                return Err("Unordered or duplicate saved body echoes".into());
            }
            previous = value.deliver;
        }
        let mut sources = BTreeSet::new();
        for (key, generation) in &self.incidents {
            if *key == 0 || *generation == 0 || !sources.insert(*key) {
                return Err("Invalid saved incident identity".into());
            }
        }
        sources.clear();
        for (key, until) in &self.source_cooldowns {
            between(*until, 0.0, 3.0_f32.mul_add(1.0, self.clock))?;
            if *key == 0 || !sources.insert(*key) {
                return Err("Invalid saved source cooldown".into());
            }
        }
        for (event, sampled) in &self.impacts {
            impact(event)?;
            sample_time(*sampled, self.clock)?;
        }
        for landing in &self.heard_impacts {
            landing.validate(self.clock)?;
        }
        if let Some(track) = &self.prop_track {
            track.validate(self.clock)?;
        }
        Ok(())
    }

    fn validate(&self, guest: &Guest) -> Result<(), String> {
        if self.id != guest.state.id
            || self.patrol_index >= guest.definition.patrol.len().max(1)
            || self.frustrated > self.episode
            || self.attacked_episode > self.episode
            || self.blocked_refuge.as_ref().is_some_and(|id| {
                !guest
                    .definition
                    .passages
                    .iter()
                    .any(|p| p.id == *id && p.crouch_only)
            })
        {
            return Err("Saved Tall Guest identity, route or episode mismatch".into());
        }
        between(self.clock, 0.0, 10_000_000.0)?;
        for time in [self.entered, self.goal_time, self.voice_started] {
            sample_time(time, self.clock)?;
        }
        for deadline in [
            self.next_pulse,
            self.frustration_until,
            self.frustration_probe_until,
            self.search_until,
            self.search_deadline,
            self.noise_deadline,
        ] {
            between(deadline, 0.0, 60.0_f32.mul_add(1.0, self.clock))?;
        }
        between(self.blocked_seconds, 0.0, 10_000_000.0)?;
        if let Some(goal) = self.goal {
            point(goal)?;
        }
        if let Some(cue) = self.voice.as_deref()
            && !matches!(
                cue,
                "tg_pursuit_shriek_loop"
                    | "tg_echolocation_howl_01"
                    | "tg_echolocation_howl_02"
                    | "tg_lost_prey_voice"
            )
        {
            return Err("Invalid saved Tall Guest voice".into());
        }
        if self.voice_until.to_bits() != f32::MAX.to_bits() {
            between(self.voice_until, 0.0, 10.0_f32.mul_add(1.0, self.clock))?;
        }
        if (self.life == super::lifecycle::Life::Disabled)
            != (self.intent == super::Intent::Disabled)
            && self.life != super::lifecycle::Life::EnablePending
        {
            return Err("Invalid saved Guest lifecycle".into());
        }
        self.validate_physical_pose(guest)?;
        self.validate_pose()?;
        self.validate_evidence()
    }
}
impl Guest {
    /// # Errors
    /// Refuses malformed semantic state before changing the private candidate.
    pub fn restore(&mut self, record: &Record) -> Result<(), String> {
        record.validate(self)?;
        self.audio_epoch = super::audio_fence::allocate_epoch()?;
        self.state = record.clone();
        self.retime_restored_posture();
        self.path.clear();
        self.path_cursor = 1;
        self.next_plan = 0.0;
        self.passage_plan = None;
        self.retreat_poses.clear();
        self.recovery = super::recovery::Mode::Forward;
        self.attack_request = false;
        self.sounds.clear();
        self.diagnostics.clear();
        if let Some(pose) = self.state.physical_pose.clone() {
            if !self.restore_physical(&pose) {
                return Err("Saved Tall Guest physical pose could not be restored".into());
            }
        } else {
            self.sample_base();
            self.fill_vertices();
        }
        Ok(())
    }

    /// # Errors
    /// Validate the actual pose after player, items and leaves have been restored.
    pub fn validate_restored_pose(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> Result<(), String> {
        if !self.enabled() {
            self.state.physical_pose = None;
            return Ok(());
        }
        let accepted = if self.state.physical_pose.is_some() {
            self.pose_clear(ctx, leaves, solids)
        } else {
            self.clear(ctx, leaves, solids)
        };
        self.state.physical_pose = None;
        if accepted {
            Ok(())
        } else {
            Err(format!(
                "Restored Tall Guest '{}' intersects the live world",
                self.state.id
            ))
        }
    }
}

impl Record {
    fn validate_physical_pose(&self, guest: &Guest) -> Result<(), String> {
        let Some(pose) = &self.physical_pose else {
            return Ok(());
        };
        let skin = guest
            .asset
            .model
            .skin
            .as_ref()
            .ok_or("Saved Tall Guest rig is missing")?;
        if pose.len() != skin.nodes.len() {
            return Err("Saved Tall Guest physical pose count mismatch".into());
        }
        for (joint, node) in pose.iter().zip(&skin.nodes) {
            if !joint.is_valid()
                || joint.translation.abs().max_element() > 10.0
                || joint
                    .scale
                    .to_array()
                    .into_iter()
                    .zip(node.scale)
                    .any(|(value, authored)| (value - authored).abs() > 0.00001)
            {
                return Err("Saved Tall Guest physical pose is invalid or changes scale".into());
            }
        }
        Ok(())
    }
}
impl Guest {
    #[must_use]
    pub fn checkpoint(&self) -> Record {
        let mut record = self.state.clone();
        record.physical_pose = Some(self.animator.joint_pose());
        record
    }
    pub(super) fn restore_physical(&mut self, joints: &[crate::engine::entity::JointPose]) -> bool {
        let restored = self.animator.restore_physical_pose(
            &self.state.clip,
            self.state.clip_time / super::duration(&self.state.clip),
            joints,
        );
        if restored {
            self.fill_vertices();
        }
        restored
    }
}

//! Feltling adapter. Generic actions, generation-safe audio and checkpoints keep
//! their existing owners; the creature never enters the predator sensing path.
use super::{EntityHandle, EntityWorld, StateValue, WorldCommand};
use crate::engine::feltling::{Feltling, Record};

impl EntityWorld {
    pub(super) fn tick_feltlings(&mut self, ctx: &super::WorldContext<'_>) {
        if self.feltlings.is_empty() {
            return;
        }
        let mut bodies: Vec<_> = self.feltlings.iter().map(Feltling::collider).collect();
        let mut output = Vec::new();
        for (index, feltling) in self.feltlings.iter_mut().enumerate() {
            let mut leaves = self
                .door_colliders
                .get(..self.guest_collider_start)
                .unwrap_or(&[])
                .to_vec();
            leaves.extend(
                self.guests
                    .iter()
                    .flat_map(crate::engine::tall_guest::Guest::body_colliders),
            );
            leaves.extend(
                bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(other, body)| (other != index).then_some(*body).flatten()),
            );
            let mut solids = self.navigation_obstacles.clone();
            solids.push(player_bounds(ctx));
            feltling.tick(ctx, &self.doors, &leaves, &solids);
            if let Some(body) = bodies.get_mut(index) {
                *body = feltling.collider();
            }
            output.push((
                feltling.state.id.clone(),
                feltling.state.position,
                feltling.state.yaw,
                feltling.take_status(),
                feltling.take_voice(),
                feltling.take_animation_event(),
            ));
        }
        for (id, position, yaw, status_update, voice, animation) in output {
            let Some(handle) = self.handle_of(&id) else {
                continue;
            };
            if let Some(transform) = self.components.transforms.get_mut(handle) {
                transform.position = position;
                transform.yaw_degrees = yaw.to_degrees();
            }
            if let Some(index) = self.interactables.index_of(&id) {
                let _updated = self
                    .interactables
                    .set_live_pose(index, position, yaw.to_degrees());
            }
            if let Some(status) = status_update {
                let _changed = self.emit_state(
                    &id,
                    "feltling_status",
                    StateValue::Text(status.into()),
                    None,
                    0,
                );
            }
            if let Some(clip) = animation {
                let _queued =
                    self.emit_depth(super::EventKind::AnimationComplete, handle, &clip, None, 0);
            }
            if let Some(cue) = voice
                && let Some(entity) = self.stable_key(handle)
            {
                self.commands.push(WorldCommand::PlaySound {
                    entity,
                    sound: cue.into(),
                    gain: 1.0,
                    looped: false,
                    time_domain: crate::engine::audio::TimeDomain::World,
                });
            }
        }
        self.refresh_guest_colliders();
        self.sync_feltling_interactables();
    }

    pub(super) fn sync_feltling_interactables(&mut self) {
        for feltling in &self.feltlings {
            let requested = self
                .names
                .get(&feltling.state.id)
                .and_then(|handle| self.components.interactables.get(handle))
                .is_some_and(|entry| entry.enabled);
            self.interactables
                .set_controller_enabled(&feltling.state.id, requested && feltling.enabled());
        }
    }

    pub(super) fn set_feltling_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let key = self
            .handle_of(id)
            .and_then(|handle| self.stable_key(handle));
        let Some(feltling) = self.feltlings.iter_mut().find(|actor| actor.state.id == id) else {
            return false;
        };
        let changed = feltling.set_enabled(enabled);
        if !enabled && let Some(entity) = key {
            self.commands.push(WorldCommand::StopSound { entity });
        }
        self.refresh_guest_colliders();
        self.rebuild_entity_frames();
        changed
    }

    pub(super) fn retire_feltling(&mut self, handle: EntityHandle) -> bool {
        let Some(id) = self.id_of(handle).map(str::to_string) else {
            return false;
        };
        let Some(index) = self.feltlings.iter().position(|actor| actor.state.id == id) else {
            return false;
        };
        if self.retired_feltling_frames.len() >= 16 {
            return false;
        }
        if let Some(entity) = self.stable_key(handle) {
            self.commands.push(WorldCommand::StopSound { entity });
        }
        let mut frame = self.feltlings.remove(index).frame();
        frame.opacity = 0.0;
        self.retired_feltling_frames.push(frame);
        self.refresh_guest_colliders();
        true
    }

    pub(super) fn restore_checkpoint_feltlings(
        &mut self,
        records: &[Record],
    ) -> Result<(), String> {
        if records.len() != self.feltlings.len() {
            return Err("Checkpoint Feltling membership mismatch".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for record in records {
            if !seen.insert(&record.id) {
                return Err("Duplicate checkpoint Feltling".into());
            }
            let transform = self
                .names
                .get(&record.id)
                .and_then(|handle| self.components.transforms.get(handle))
                .ok_or("Missing Feltling checkpoint transform")?;
            if transform.position.distance(record.position) > 0.0001
                || crate::engine::entity::angle_difference(
                    transform.yaw_degrees.to_radians(),
                    record.yaw,
                )
                .abs()
                    > 0.0001
            {
                return Err("Feltling checkpoint transform disagrees with entity ownership".into());
            }
            self.feltlings
                .iter_mut()
                .find(|actor| actor.state.id == record.id)
                .ok_or("Unknown checkpoint Feltling")?
                .restore(record)?;
        }
        self.refresh_guest_colliders();
        Ok(())
    }

    pub(super) fn validate_restored_feltlings(
        &self,
        ctx: &super::WorldContext<'_>,
    ) -> Result<(), String> {
        for actor in &self.feltlings {
            if !actor.enabled() {
                continue;
            }
            let mut leaves = self
                .door_colliders
                .get(..self.guest_collider_start)
                .unwrap_or(&[])
                .to_vec();
            leaves.extend(
                self.guests
                    .iter()
                    .flat_map(crate::engine::tall_guest::Guest::body_colliders),
            );
            leaves.extend(
                self.feltlings
                    .iter()
                    .filter(|other| other.state.id != actor.state.id)
                    .filter_map(Feltling::collider),
            );
            let mut solids = self.navigation_obstacles.clone();
            solids.push(player_bounds(ctx));
            if !actor.clear(ctx, &leaves, &solids) {
                return Err(format!(
                    "{}: restored Feltling body is obstructed or unsupported",
                    actor.state.id
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn feltling_diagnostics(&self) -> serde_json::Value {
        serde_json::Value::Array(self.feltlings.iter().map(Feltling::diagnostics).collect())
    }
}

fn player_bounds(ctx: &super::WorldContext<'_>) -> crate::engine::collision::WallAabb {
    crate::engine::collision::WallAabb::with_y(
        ctx.feet.x - ctx.body_half,
        ctx.feet.y,
        ctx.feet.z - ctx.body_half,
        ctx.body_half * 2.0,
        ctx.body_height,
        ctx.body_half * 2.0,
    )
}

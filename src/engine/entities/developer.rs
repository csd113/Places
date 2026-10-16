//! Bounded developer views and manual entry events for real trigger volumes.

use glam::Vec3;

use super::{DispatchReport, EntityWorld, EventKind, ResolvedBinding, WorldTick};
use crate::engine::collision::WallAabb;
use crate::engine::collision_index::CollisionIndex;
use crate::engine::interact::clear_line_of_sight_indexed;

/// Maximum number of trigger labels returned for one frame.
pub const MAX_TRIGGER_LABELS: usize = 16;
/// Developer trigger labels are local, even in a very large authored volume.
pub const TRIGGER_LABEL_RANGE_M: f32 = 12.0;
/// A 60-degree half-angle keeps labels in front of the player's gaze.
const TRIGGER_LABEL_VIEW_COS: f32 = 0.5;

/// One trigger volume's authored id and visible world-space label anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct TriggerLabel {
    /// The volume id accepted by the manual trigger command.
    pub id: String,
    /// World-space anchor projected by the presentation layer.
    pub position: Vec3,
}

fn is_entry_binding(binding: &ResolvedBinding) -> bool {
    binding.on == EventKind::EnterVolume && binding.key.as_deref().is_none_or(str::is_empty)
}

impl EntityWorld {
    /// Removes only a movable authored physics prop's entity representation.
    /// The native loop separately removes its body and runtime render model.
    /// Removed handles remain invalid across runtime resets; loading a fresh
    /// level is what restores the authored prop.
    #[must_use]
    pub fn remove_authored_physics_entity(&mut self, id: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        if !self.authored_physics.contains(&handle) {
            return false;
        }
        let removed = self.remove_entity_handle(handle);
        self.compact_bindings();
        self.rebuild_entity_frames();
        removed
    }

    /// Emits the same entry event as crossing a trigger volume. Occupancy is
    /// unchanged, so a developer command cannot fabricate a later exit edge.
    /// # Errors
    /// Refuses unknown/non-volume ids, absent entry actions, exhausted one-shot
    /// bindings, cooldowns, unmet conditions and a full event queue.
    pub fn developer_trigger(&mut self, id: &str) -> Result<DispatchReport, String> {
        let trigger_id = id.trim();
        let handle = self
            .handle_of(trigger_id)
            .ok_or_else(|| format!("Unknown trigger `{trigger_id}`."))?;
        if !self.components.volumes.contains(handle) {
            return Err(format!(
                "`{trigger_id}` is an entity, not a trigger volume."
            ));
        }
        let bindings: Vec<&ResolvedBinding> = self
            .bindings
            .iter()
            .filter(|binding| binding.owner == handle && is_entry_binding(binding))
            .collect();
        if bindings.is_empty() {
            return Err(format!(
                "Trigger `{trigger_id}` has no entry action binding."
            ));
        }
        let repeatable: Vec<&ResolvedBinding> = bindings
            .into_iter()
            .filter(|binding| !(binding.once && binding.fired))
            .collect();
        if repeatable.is_empty() {
            return Err(format!(
                "Trigger `{trigger_id}` has already fired its one-shot bindings."
            ));
        }
        let ready: Vec<&ResolvedBinding> = repeatable
            .iter()
            .copied()
            .filter(|binding| binding.cooldown_remaining <= 0.0)
            .collect();
        if ready.is_empty() {
            let remaining = repeatable
                .iter()
                .map(|binding| binding.cooldown_remaining)
                .min_by(f32::total_cmp)
                .unwrap_or(0.0);
            return Err(format!(
                "Trigger `{trigger_id}` is cooling down ({remaining:.2} s)."
            ));
        }
        if !ready.iter().any(|binding| {
            binding
                .when
                .iter()
                .all(|condition| self.condition_holds(condition))
        }) {
            return Err(format!(
                "Trigger `{trigger_id}` has unmet binding conditions."
            ));
        }
        if !self.emit(EventKind::EnterVolume, handle, "", None) {
            return Err(format!(
                "Cannot fire trigger `{trigger_id}`: the event queue is full."
            ));
        }
        Ok(self.dispatch_queued_events())
    }

    /// At most sixteen nearest trigger ids in a twelve-metre view cone, with
    /// the same static-wall and live-door occlusion query as ordinary labels.
    #[must_use]
    pub fn developer_trigger_labels(
        &self,
        eye: Vec3,
        forward: Vec3,
        index: &CollisionIndex,
        walls: &[WallAabb],
    ) -> Vec<TriggerLabel> {
        if !eye.is_finite() {
            return Vec::new();
        }
        let Some(direction) = forward.try_normalize() else {
            return Vec::new();
        };
        let mut nearest: Vec<(f32, &str, Vec3)> = Vec::with_capacity(MAX_TRIGGER_LABELS);
        for (handle, volume) in self.components.volumes.iter() {
            let position = Vec3::new(
                volume.bounds[0].midpoint(volume.bounds[1]),
                volume.bottom_y.midpoint(volume.top_y),
                volume.bounds[2].midpoint(volume.bounds[3]),
            );
            let offset = Vec3::new(position.x - eye.x, position.y - eye.y, position.z - eye.z);
            let distance = offset.length();
            if !distance.is_finite() || distance > TRIGGER_LABEL_RANGE_M {
                continue;
            }
            if let Some(to_label) = offset.try_normalize()
                && to_label.dot(direction) < TRIGGER_LABEL_VIEW_COS
            {
                continue;
            }
            let slot = nearest
                .iter()
                .position(|(other_distance, _, _)| distance < *other_distance)
                .unwrap_or(nearest.len());
            if slot >= MAX_TRIGGER_LABELS {
                continue;
            }
            let Some(id) = self.id_of(handle) else {
                continue;
            };
            if !clear_line_of_sight_indexed(
                eye,
                position,
                None,
                None,
                index,
                walls,
                self.door_colliders(),
            ) {
                continue;
            }
            nearest.insert(slot, (distance, id, position));
            nearest.truncate(MAX_TRIGGER_LABELS);
        }
        nearest
            .into_iter()
            .map(|(_, id, position)| TriggerLabel {
                id: id.to_string(),
                position,
            })
            .collect()
    }

    pub(super) fn dispatch_queued_events(&mut self) -> DispatchReport {
        let mut tick = WorldTick::default();
        self.pump_events(&mut tick);
        self.report_queue_refusals(&mut tick);
        if tick.frames_dirty {
            self.sync_transformed_interactables();
            self.sync_routed_interactables();
            self.rebuild_entity_frames();
        }
        DispatchReport {
            actions_run: tick.actions_run,
            player_reset: tick.player_reset,
            spawned: tick.spawned,
            despawned: tick.despawned,
            missing_targets: tick.missing_targets,
            unsupported: tick.unsupported,
            labels_shown: tick.labels_shown,
            labels_hidden: tick.labels_hidden,
            ..DispatchReport::default()
        }
    }
}

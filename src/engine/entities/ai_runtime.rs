//! Adapter from shared AI intent into the existing entity action/physics seams.
use super::{EntityWorld, EventKind, WorldCommand, WorldTick};
use crate::engine::ai::{
    authoring::UseKind,
    controller::OperationToken,
    tasks::{AiObject, MAX_OBJECTS, TaskEffect},
};
use crate::engine::level::ActionDef;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub(super) struct PhysicalObservation {
    pub center: Vec3,
    pub half: Vec3,
    pub held: bool,
}

impl EntityWorld {
    pub(super) fn clear_ai_observations(&mut self) {
        self.ai_objects.clear();
        self.ai_physical.clear();
        self.ai_use_acks.clear();
        self.next_ai_objects = 0.0;
        self.ai_observation_version = 0;
        self.ai_object_cursor = 0;
    }

    pub(super) fn refresh_ai_objects(&mut self) {
        if self.sim_time < self.next_ai_objects
            && self.ai_observation_version == self.ai.observation_version()
        {
            return;
        }
        self.ai_observation_version = self.ai.observation_version();
        self.next_ai_objects = self.sim_time + crate::engine::ai::PERCEPTION_INTERVAL_S;
        self.ai_objects.clear();
        if !self.ai.requires_objects() {
            return;
        }
        // The component iterator is visited only at perception cadence. Rotate
        // the bounded window so large scenes cannot starve late object slots.
        let count = self.components.transforms.len();
        let first = self.ai_object_cursor.checked_rem(count).unwrap_or(0);
        let (reserved, requested) = self.ai.observation_pins();
        let mut pins: Vec<_> = reserved
            .into_iter()
            .chain(requested.into_iter().filter_map(|id| self.handle_of(id)))
            .collect();
        pins.sort_unstable();
        pins.dedup();
        let pinned = pins.iter().filter_map(|handle| {
            self.components
                .transforms
                .get(*handle)
                .map(|pose| (*handle, pose))
        });
        let scanned = self
            .components
            .transforms
            .iter()
            .skip(first)
            .chain(self.components.transforms.iter().take(first))
            .take(MAX_OBJECTS)
            .filter(|(handle, _)| !pins.contains(handle));
        for (handle, pose) in pinned.chain(scanned) {
            let Some(id) = self.id_of(handle) else {
                continue;
            };
            if self.components.ais.contains(handle) {
                continue;
            }
            let physical = self.ai_physical.get(&handle);
            let carried_by =
                self.components.carries.iter().find_map(|(owner, carry)| {
                    (carry.item.as_deref() == Some(id)).then_some(owner)
                });
            self.ai_objects.push(AiObject {
                handle,
                id: id.to_string(),
                object_type: self.components.renderables.get(handle).map_or_else(
                    || {
                        if self.doors.index_of(id).is_some() {
                            "door".to_string()
                        } else {
                            "entity".to_string()
                        }
                    },
                    |renderable| renderable.model.clone(),
                ),
                position: physical.map_or(pose.position, |observation| observation.center),
                loose: physical.is_some_and(|observation| !observation.held),
                radius: physical.map_or(0.0, |observation| {
                    observation.half.x.max(observation.half.z)
                }),
                carried_by,
            });
        }
        self.ai_object_cursor = first
            .saturating_add(MAX_OBJECTS)
            .checked_rem(count)
            .unwrap_or(0);
    }
    pub(super) fn apply_ai_effects(&mut self, effects: Vec<TaskEffect>, tick: &mut WorldTick) {
        for effect in effects {
            let token = match &effect {
                TaskEffect::Pickup { token, item: _ }
                | TaskEffect::Putdown { token, contact: _ }
                | TaskEffect::Use { token, point: _ }
                | TaskEffect::Animate { token, clip: _ } => *token,
            };
            if !self.ai.operation_valid(token) {
                continue;
            }
            let Some(actor) = self.id_of(token.owner).map(str::to_string) else {
                continue;
            };
            match effect {
                TaskEffect::Pickup { item, token: _ } => {
                    let Some(item_handle) = self.handle_of(&item) else {
                        continue;
                    };
                    if !self.ai.item_reserved(item_handle, token) {
                        continue;
                    }
                    if let Some(carry) = self.components.carries.get_mut(token.owner) {
                        if carry.item.is_some() || carry.retired {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    let high = self
                        .components
                        .transforms
                        .get(item_handle)
                        .zip(self.components.transforms.get(token.owner))
                        .is_some_and(|(item_pose, actor_pose)| {
                            item_pose.position.y - actor_pose.position.y > 0.5
                        });
                    self.play_ai_clip(
                        &actor,
                        token,
                        if high {
                            "pick_up_tabletop"
                        } else {
                            "pick_up_floor"
                        },
                        tick,
                    );
                    self.commands.push(WorldCommand::AiPickup { token, item });
                }
                TaskEffect::Putdown { token: _, contact } => {
                    self.play_ai_clip(&actor, token, "set_down_floor", tick);
                    self.commands
                        .push(WorldCommand::AiPutdown { token, contact });
                }
                TaskEffect::Animate { clip, token: _ } => {
                    self.play_ai_clip(&actor, token, &clip, tick);
                }
                TaskEffect::Use { point, token: _ } => {
                    self.commands.push(WorldCommand::AiUse { token, point });
                }
            }
        }
    }
    fn play_ai_clip(
        &mut self,
        actor: &str,
        token: OperationToken,
        clip: &str,
        tick: &mut WorldTick,
    ) {
        let report = self.dispatch_actions(
            &[ActionDef::PlayAnimation {
                target: Some(actor.to_string()),
                clip: Some(clip.to_string()),
                looped: false,
            }],
            Some(token.owner),
        );
        tick.record_dispatch(&report);
        let _released = self.scripted_poses.remove(actor);
        self.ai.set_task_clip(token, clip);
    }
    pub fn confirm_ai_contact(
        &mut self,
        token: OperationToken,
        point: &crate::engine::ai::authoring::UsePoint,
    ) -> bool {
        if !self.ai.operation_running(token) {
            return false;
        }
        if self.handle_of(&point.target).is_none() && self.doors.index_of(&point.target).is_none() {
            return false;
        }
        if point.kind == UseKind::Shelf
            && let Some(item) = self.ai.reserved_item(token)
            && (self
                .components
                .carries
                .get(token.owner)
                .is_some_and(|carry| carry.item.is_some())
                || self.ai_physical.get(&item).is_none_or(|observation| {
                    observation.held
                        || observation.center.distance(Vec3::from_array(point.contact))
                            > point.reach.min(0.35)
                }))
        {
            return false;
        }
        if point.kind == UseKind::Door {
            let opened = self
                .doors
                .request_open_from(&point.target, Some(Vec3::from_array(point.approach)));
            let opening = self
                .doors
                .index_of(&point.target)
                .and_then(|index| self.doors.get(index))
                .is_some_and(|door| {
                    matches!(
                        door.phase(),
                        crate::engine::door::DoorPhase::Opening
                            | crate::engine::door::DoorPhase::Open
                    )
                });
            if !opened && !opening {
                return false;
            }
        } else if let Some(target) = self.handle_of(&point.target) {
            if point.kind != UseKind::Chair
                && !self
                    .components
                    .interactables
                    .get(target)
                    .is_some_and(|interaction| interaction.enabled)
            {
                return false;
            }
            let _queued = self.emit(EventKind::Interact, target, "", Some(token.owner));
        }
        self.ai_use_acks
            .retain(|ack| ack.token != token || ack.target != point.target);
        if self.ai_use_acks.len() < 64 {
            self.ai_use_acks.push(crate::engine::ai::tasks::UseAck {
                token,
                target: point.target.clone(),
                completed_at: self.sim_time,
            });
        }
        true
    }
    pub(super) fn ai_frame_cue(
        &self,
        agent: &crate::engine::ai::AiAgent,
    ) -> crate::engine::entity::PoseCue {
        let cue = if agent.authored_pose() {
            self.animation_override(&agent.instance_id)
                .cloned()
                .unwrap_or_else(|| agent.cue())
        } else {
            agent.cue()
        };

        let mut carrying = self
            .handle_of(&agent.instance_id)
            .and_then(|h| self.components.carries.get(h))
            .filter(|carry| carry.item.is_some())
            .map_or_else(
                || cue.clone(),
                |carry| {
                    if self.scripted_poses.contains(&agent.instance_id) || agent.authored_pose() {
                        cue.clone()
                    } else {
                        crate::engine::entity::PoseCue::Clip {
                            name: if matches!(
                                cue,
                                crate::engine::entity::PoseCue::Walk { speed_mps: _ }
                            ) {
                                carry.def.walk_clip.clone()
                            } else {
                                carry.def.idle_clip.clone()
                            },
                            once: false,
                            paused: false,
                        }
                    }
                },
            );
        if let crate::engine::entity::PoseCue::Clip {
            name: _,
            once: _,
            paused,
        } = &mut carrying
        {
            *paused = agent.light_look_target.is_some();
        }
        carrying
    }
    pub fn observe_carried_pose(&mut self, item: &str, position: Vec3) {
        if let Some(handle) = self.handle_of(item)
            && let Some(pose) = self.components.transforms.get_mut(handle)
        {
            pose.position = position;
        }
    }
    pub fn observe_audio_start(&mut self, event: crate::engine::audio::SoundEvent) {
        if event.looping || event.gain <= 0.0 {
            return;
        }
        let position = Vec3::from_array(event.position);
        if self
            .stimuli
            .iter()
            .any(|noise| noise.age < 0.05 && noise.position.distance_squared(position) < 0.04)
        {
            return;
        }
        let source = event
            .source
            .and_then(|key| self.audio_source_anchor(key))
            .and_then(|(id, _)| self.handle_of(id));
        self.note_stimulus(
            position,
            event.max_distance.min(30.0) * event.gain.clamp(0.0, 1.0),
            "sound",
            source,
        );
    }
    /// Native physics performs this second check immediately before removing a loose body.
    #[must_use]
    pub fn ai_pickup_valid(&self, token: OperationToken, item: &str) -> bool {
        self.ai.operation_running(token)
            && self
                .handle_of(item)
                .is_some_and(|handle| self.ai.item_reserved(handle, token))
            && self.script_held_item.as_deref() != Some(item)
    }
}

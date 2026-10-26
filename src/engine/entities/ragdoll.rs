//! Ownership transfer shared by console, bindings and sandboxed scripts.
use super::{EntityWorld, WorldCommand};

pub(super) struct ActorController {
    ai: Option<crate::engine::ai::AiAgent>,
    aim_size: Option<[f32; 3]>,
}

impl EntityWorld {
    /// Physics pelvis and live collider envelope replace stale upright interaction.
    pub fn publish_ragdoll(
        &mut self,
        id: &str,
        position: glam::Vec3,
        bounds: crate::engine::spatial::Aabb,
    ) {
        if !self.physics_actors.contains_key(id) {
            return;
        }
        if let Some(handle) = self.handle_of(id)
            && let Some(transform) = self.components.transforms.get_mut(handle)
        {
            transform.position = position;
        }
        if let Some(index) = self.interactables.index_of(id) {
            self.interactables.set_articulated_bounds(index, bounds);
        }
    }

    pub(super) fn queue_ragdoll_resets(&mut self) {
        let ids: Vec<_> = self.physics_actors.keys().cloned().collect();
        for target in ids {
            self.commands.push(WorldCommand::Ragdoll {
                target,
                enabled: false,
                velocity: [0.0; 3],
            });
        }
    }

    pub(super) fn resolve_ragdoll_ownership(&mut self) {
        self.physics_actors.clear();
        self.authored_transforms = self
            .components
            .transforms
            .iter()
            .map(|(handle, transform)| (handle, *transform))
            .collect();
    }

    pub(super) fn ragdoll_blocks_action(
        &self,
        action: &crate::engine::level::ActionDef,
        actor: Option<super::EntityHandle>,
    ) -> bool {
        use crate::engine::level::ActionDef;
        matches!(
            action,
            ActionDef::PlayAnimation {
                target: _,
                clip: _,
                looped: _
            } | ActionDef::ToggleAnimation { target: _, clip: _ }
                | ActionDef::MoveObject {
                    target: _,
                    x: _,
                    y: _,
                    z: _,
                    speed: _
                }
                | ActionDef::StartSequence {
                    sequence: _,
                    target: _
                }
                | ActionDef::AiSenses {
                    target: _,
                    rules: _
                }
                | ActionDef::AiPause {
                    target: _,
                    key: _,
                    paused: _
                }
                | ActionDef::AiTask {
                    target: _,
                    key: _,
                    request: _
                }
                | ActionDef::Navigate {
                    target: _,
                    position: _,
                    speed_mps: _
                }
                | ActionDef::HandoffItem { target: _ }
        ) && self
            .resolve_target(action.target(), actor)
            .is_some_and(|id| self.physics_actors.contains_key(&id))
    }

    /// Queues a validated request; native rig/resource validation precedes ownership.
    ///
    /// # Errors
    /// Unknown target or nonfinite/excessive launch velocity.
    pub fn request_ragdoll(
        &mut self,
        target: &str,
        enabled: bool,
        velocity: [f32; 3],
    ) -> Result<(), String> {
        let speed = glam::Vec3::from_array(velocity);
        if self.handle_of(target).is_none() || !speed.is_finite() || speed.length() > 15.0 {
            return Err("ragdoll requires a live entity and finite velocity at most 15 m/s".into());
        }
        self.commands.push(WorldCommand::Ragdoll {
            target: target.to_string(),
            enabled,
            velocity,
        });
        Ok(())
    }

    /// Cancels competing movement/sequence ownership after physical creation succeeds.
    pub fn set_ragdoll_owned(&mut self, id: &str, owned: bool) {
        let Some(handle) = self.handle_of(id) else {
            return;
        };
        if owned {
            if self.physics_actors.contains_key(id) {
                return;
            }
            let ai = self.ai.take_for_physics(id);
            let aim_size = self
                .interactables
                .index_of(id)
                .and_then(|index| self.interactables.get(index))
                .map(|item| item.size);
            let _old = self
                .physics_actors
                .insert(id.to_string(), ActorController { ai, aim_size });
            self.moves.retain(|goal| goal.entity != handle);
            let _stopped = self.stop_sequence(id);
            self.animation_overrides.retain(|(target, _)| target != id);
            let _released = self.scripted_poses.remove(id);
        } else if let Some(controller) = self.physics_actors.remove(id) {
            if let Some(agent) = controller.ai {
                self.ai.restore_from_physics(agent);
            }
            if let Some((_, authored)) = self
                .authored_transforms
                .iter()
                .find(|(entity, _)| *entity == handle)
                && let Some(transform) = self.components.transforms.get_mut(handle)
            {
                *transform = *authored;
            }
            for (index, route) in self.routes.routes().iter().enumerate() {
                if route.instance_id == id
                    && let Some(state) = self.route_states.get_mut(index)
                {
                    *state = route.new_state();
                }
            }
            self.animation_overrides.retain(|(target, _)| target != id);
            if let Some(size) = controller.aim_size
                && let Some(index) = self.interactables.index_of(id)
                && let Some(transform) = self.components.transforms.get(handle)
            {
                let _restored = self.interactables.restore_actor_pose(
                    index,
                    size,
                    transform.position,
                    transform.yaw_degrees,
                );
            }
        }
        if !owned {
            self.rebuild_interactables_from_tables();
        }
        self.rebuild_entity_frames();
    }
}

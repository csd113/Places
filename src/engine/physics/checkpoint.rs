//! Authored item state keyed by source identity, reconstructed in a private world.
use super::{BodyMotion, HeldItem, ItemPose, ItemWorld, Quat, Vec3, add, pose, vector};
use crate::engine::entities::checkpoint::number;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    position: [f32; 3],
    rotation: [f32; 4],
    linear: [f32; 3],
    angular: [f32; 3],
    sleeping: bool,
    dormant: bool,
    held_rotation: Option<[f32; 4]>,
}

impl Item {
    /// Carry a validated rigid state between cabin frames. Held rotation is
    /// camera-relative, so it remains unchanged when rider and item rotate.
    #[must_use]
    pub fn transported(&self, from: Vec3, to: Vec3, turn: Quat) -> Self {
        let mut next = self.clone();
        next.position = crate::engine::elevator::map_cabin_point(
            from,
            to,
            turn,
            Vec3::from_array(self.position),
        )
        .to_array();
        next.rotation = turn
            .mul_quat(Quat::from_array(self.rotation))
            .normalize()
            .to_array();
        next.linear = turn.mul_vec3(Vec3::from_array(self.linear)).to_array();
        next.angular = turn.mul_vec3(Vec3::from_array(self.angular)).to_array();
        next
    }
}

fn rotation(value: [f32; 4]) -> Result<Quat, String> {
    let result = Quat::from_array(value);
    if !result.is_finite() || (result.length_squared() - 1.0).abs() > 0.001 {
        return Err("Invalid checkpoint item orientation".into());
    }
    Ok(result.normalize())
}

fn validate_records(
    authored: &[(u64, String, String)],
    records: &[(String, Item)],
) -> Result<(), String> {
    if records.len() != authored.len() || records.len() > crate::engine::physics::MAX_BODIES {
        return Err("Checkpoint item membership mismatch".into());
    }
    let mut seen = BTreeSet::new();
    let mut held_count = 0_usize;
    for (id, item) in records {
        if !seen.insert(id) || !authored.iter().any(|(_, name, _)| name == id) {
            return Err("Unknown or duplicate checkpoint item".into());
        }
        for value in item.position {
            number(value, 100_000.0)?;
        }
        for value in item.linear.into_iter().chain(item.angular) {
            number(value, 1000.0)?;
        }
        let _checked = rotation(item.rotation)?;
        if let Some(relative) = item.held_rotation {
            let _checked_relative = rotation(relative)?;
            held_count = held_count.saturating_add(1);
            if item.dormant {
                return Err("A held checkpoint item cannot be dormant".into());
            }
        }
    }
    if held_count > 1 {
        return Err("Checkpoint has multiple held items".into());
    }
    Ok(())
}

impl ItemWorld {
    /// Check a resident NPC against the *candidate* batch before any body moves.
    /// NPC controllers are separate from the rigid-body query world.
    pub(crate) fn transport_actor_clear(
        &self,
        selected: &[(u64, String, String)],
        records: &[(String, Item)],
        feet: Vec3,
        height: f32,
        radius: f32,
    ) -> Result<bool, String> {
        validate_records(selected, records)?;
        if records.is_empty() {
            return Ok(true);
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or("Transport physics world is missing")?;
        let actor = rapier3d::prelude::SharedShape::cylinder(height * 0.5, radius);
        let actor_pose = pose(
            Vec3::new(feet.x, height.mul_add(0.5, feet.y), feet.z),
            Quat::IDENTITY,
        );
        for (id, record) in records {
            let key = selected
                .iter()
                .find(|(_, name, _)| name == id)
                .map(|(key, _, _)| key)
                .ok_or("Transport body identity is missing")?;
            let entry = runtime
                .entries
                .get(key)
                .ok_or("Transport body is missing")?;
            let turn = rotation(record.rotation)?;
            let target = pose(
                add(
                    Vec3::from_array(record.position),
                    turn.mul_vec3(entry.center),
                ),
                turn,
            );
            if super::shape_contact(
                &target,
                entry.shape.as_ref(),
                &actor_pose,
                actor.as_ref(),
                0.0,
            )
            .map_err(|error| error.to_string())?
            .is_some_and(|contact| contact.dist < -0.003)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Proves every target against unrelated live bodies, scenery and doors
    /// before publishing a local cabin transfer. Selected bodies retain their
    /// mutual contact layout; the player is admitted separately by Game.
    pub(crate) fn restore_transport(
        &mut self,
        selected: &[(u64, String, String)],
        records: &[(String, Item)],
    ) -> Result<(), String> {
        validate_records(selected, records)?;
        if selected.is_empty() && records.is_empty() {
            return Ok(());
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("Transport physics world is missing")?;
        runtime.refresh_queries();
        let unrelated = |handle, _: &rapier3d::prelude::Collider| {
            runtime
                .key_for_collider(handle)
                .is_none_or(|key| !selected.iter().any(|(k, _, _)| *k == key))
        };
        let filter = super::query_filter()
            .exclude_rigid_body(runtime.player)
            .predicate(&unrelated);
        for (id, record) in records {
            let key = selected
                .iter()
                .find(|(_, name, _)| name == id)
                .map(|(key, _, _)| *key)
                .ok_or("Transport body identity is missing")?;
            let entry = runtime
                .entries
                .get(&key)
                .ok_or("Transport body is missing")?;
            let turn = rotation(record.rotation)?;
            let target = pose(
                add(
                    Vec3::from_array(record.position),
                    turn.mul_vec3(entry.center),
                ),
                turn,
            );
            for (_, other) in runtime
                .world
                .intersect_shape(target, entry.shape.as_ref(), filter)
            {
                let contact = super::shape_contact(
                    &target,
                    entry.shape.as_ref(),
                    other.position(),
                    other.shape(),
                    0.0,
                )
                .map_err(|e| e.to_string())?;
                if contact.is_some_and(|c| c.dist < -0.003) {
                    return Err(format!(
                        "Elevator destination for item '{id}' is obstructed"
                    ));
                }
            }
        }
        let accumulator = self.accumulator;
        let result = self.restore_checkpoint(selected, records);
        self.accumulator = accumulator;
        result
    }

    /// # Errors
    /// Session-created props, articulated bodies and suspended items are unsupported.
    pub fn checkpoint(
        &self,
        authored: &[(u64, String, String)],
    ) -> Result<Vec<(String, Item)>, String> {
        self.checkpoint_records(authored, true)
    }

    /// Bounded transport seam: exports only named rigid bodies. It never
    /// serializes solver handles or articulated actors outside that selection.
    pub(crate) fn transport_checkpoint(
        &self,
        selected: &[(u64, String, String)],
    ) -> Result<Vec<(String, Item)>, String> {
        self.checkpoint_records(selected, false)
    }

    fn checkpoint_records(
        &self,
        authored: &[(u64, String, String)],
        strict: bool,
    ) -> Result<Vec<(String, Item)>, String> {
        if self.simulation.paused || (self.simulation.time_scale - 1.0).abs() > f32::EPSILON {
            return Err("Checkpoint needs the ordinary running physics clock".into());
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return Ok(Vec::new());
        };
        if strict && (!runtime.ragdolls.is_empty() || runtime.entries.len() != authored.len()) {
            return Err("Checkpoint supports authored rigid items only; remove session props or finish articulated interactions".into());
        }
        let mut result = Vec::new();
        for (key, id, _) in authored {
            let entry = runtime
                .entries
                .get(key)
                .ok_or("Checkpoint item identity is missing")?;
            let body = runtime
                .world
                .bodies
                .get(entry.body)
                .ok_or("Checkpoint body is missing")?;
            if !body.is_enabled() {
                return Err("Checkpoint cannot retain a suspended item".into());
            }
            let held_rotation = runtime
                .held
                .as_ref()
                .filter(|held| held.key == *key)
                .map(HeldItem::checkpoint_rotation);
            for coordinate in entry.current.position.to_array() {
                number(coordinate, 100_000.0)?;
            }
            for velocity in super::point(body.linvel())
                .to_array()
                .into_iter()
                .chain(super::point(body.angvel()).to_array())
            {
                number(velocity, 1000.0)?;
            }
            let _checked = rotation(entry.current.rotation.to_array())?;
            result.push((
                id.clone(),
                Item {
                    position: entry.current.position.to_array(),
                    rotation: entry.current.rotation.to_array(),
                    linear: super::point(body.linvel()).to_array(),
                    angular: super::point(body.angvel()).to_array(),
                    sleeping: body.is_sleeping(),
                    dormant: entry.dormant,
                    held_rotation,
                },
            ));
        }
        Ok(result)
    }

    /// # Errors
    /// Validates all identities and values before modifying the private candidate.
    pub fn restore_checkpoint(
        &mut self,
        authored: &[(u64, String, String)],
        records: &[(String, Item)],
    ) -> Result<(), String> {
        validate_records(authored, records)?;
        if records.is_empty() {
            return Ok(());
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("Checkpoint physics world is missing")?;
        if runtime
            .held
            .as_ref()
            .is_some_and(|held| authored.iter().any(|(key, _, _)| *key == held.key))
        {
            runtime.restore_carry_contacts();
            runtime.held = None;
        }
        for (id, item) in records {
            let key = authored
                .iter()
                .find(|(_, name, _)| name == id)
                .map(|(key, _, _)| *key)
                .ok_or("Unknown checkpoint item")?;
            let entry = runtime
                .entries
                .get_mut(&key)
                .ok_or("Checkpoint body is missing")?;
            let target = ItemPose {
                position: Vec3::from_array(item.position),
                rotation: rotation(item.rotation)?,
            };
            let body = runtime
                .world
                .bodies
                .get_mut(entry.body)
                .ok_or("Checkpoint solver body is missing")?;
            let motion = if item.dormant {
                rapier3d::prelude::RigidBodyType::Fixed
            } else if item.held_rotation.is_some() {
                rapier3d::prelude::RigidBodyType::KinematicPositionBased
            } else {
                match entry.motion {
                    BodyMotion::Fixed => rapier3d::prelude::RigidBodyType::Fixed,
                    BodyMotion::Dynamic => rapier3d::prelude::RigidBodyType::Dynamic,
                    BodyMotion::Kinematic => {
                        rapier3d::prelude::RigidBodyType::KinematicPositionBased
                    }
                }
            };
            body.set_body_type(motion, true);
            let solver_pose = pose(
                add(target.position, target.rotation.mul_vec3(entry.center)),
                target.rotation,
            );
            body.set_position(solver_pose, true);
            body.set_next_kinematic_position(solver_pose);
            body.set_linvel(vector(Vec3::from_array(item.linear)), true);
            body.set_angvel(vector(Vec3::from_array(item.angular)), true);
            if item.sleeping {
                body.sleep();
            }
            entry.dormant = item.dormant;
            entry.dormant_pose = target;
            entry.current = target;
            entry.previous = target;
            entry.solver_pose = None;
            entry.trigger_path = super::TriggerPath::new(target, entry.center, &entry.shape);
            entry.impact_motion = super::ImpactMotion::default();
            entry.impact_cooldown = 0.0;
            entry.recovery.reset(target);
            if let Some(relative) = item.held_rotation {
                runtime.held = Some(HeldItem::from_checkpoint(key, relative));
            }
        }
        self.accumulator = 0.0;
        runtime.control_player = None;
        runtime.control_remainder = 0.0;
        for (key, _, _) in authored {
            runtime.clear_item_contacts(*key);
        }
        runtime.refresh_queries();
        runtime.update_poses();
        Ok(())
    }
}

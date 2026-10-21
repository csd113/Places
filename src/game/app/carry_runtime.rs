//! Atomic physical ownership and posed-node presentation for authored NPCs.
//! The item remains the original rendered resource: no clone or replacement.

use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Quat, Vec3};

use super::FrameLoop;
use crate::engine::{
    collision::WallAabb, level::LevelDef, materials::MaterialTable, spatial::Aabb,
};

struct Attachment {
    key: u64,
    item: String,
    socket: String,
    local: Mat4,
    retirement_requested: bool,
    retired: bool,
    bounds: Option<Aabb>,
    character_bounds: Option<Aabb>,
    item_bounds: Option<Aabb>,
    hidden: bool,
    reset: OwnedItemReset,
}

/// Original authored body contract captured before physical ownership leaves.
struct OwnedItemReset {
    definition: crate::engine::physics::PhysicsDef,
    bounds: (Vec3, Vec3),
    pose: crate::engine::physics::ItemPose,
    scale: f32,
}

struct RestoredCarry {
    actor: String,
    key: u64,
    pose: crate::engine::physics::ItemPose,
}

#[derive(Default)]
pub(super) struct CarryRuntime {
    attachments: BTreeMap<String, Attachment>,
    occluders: Vec<WallAabb>,
    item_groups: BTreeMap<String, String>,
}

fn wall_is_opaque(
    wall: &crate::engine::level::WallDef,
    default: &str,
    materials: &MaterialTable,
) -> bool {
    std::iter::once(wall.material.as_deref().unwrap_or(default))
        .chain(wall.faces.values().map(String::as_str))
        .all(|id| {
            materials.entry_of(id).is_some_and(|material| {
                material.alpha.mode == crate::engine::materials::AlphaMode::Opaque
            })
        })
}

fn wall_footprints_overlap(
    a: &crate::engine::level::WallDef,
    b: &crate::engine::level::WallDef,
) -> bool {
    let (ax, az) = a.min_corner();
    let (bx, bz) = b.min_corner();
    ax <= b.width.abs().mul_add(1.0, bx)
        && bx <= a.width.abs().mul_add(1.0, ax)
        && az <= b.depth.abs().mul_add(1.0, bz)
        && bz <= a.depth.abs().mul_add(1.0, az)
}

impl CarryRuntime {
    pub(super) fn from_level(level: &LevelDef, materials: &MaterialTable) -> Self {
        let uncertain: Vec<_> = level
            .walls
            .iter()
            .filter(|wall| !wall_is_opaque(wall, &level.defaults.wall, materials))
            .collect();
        let occluders = level
            .walls
            .iter()
            .filter_map(|wall| {
                // Explicit solid walls only. Excluding openings/profiles is safely
                // conservative; collision and lighting approximation boxes are not
                // visual occluders and must never justify disappearing an actor.
                let height = wall.height?;
                if !wall.openings.is_empty() || height <= 0.0 {
                    return None;
                }
                // Coalesced walls use the last covering member's material.
                // A nonopaque overlapping definition can therefore replace
                // this opaque face. Ambiguous footprints never justify hiding.
                if !wall_is_opaque(wall, &level.defaults.wall, materials)
                    || uncertain
                        .iter()
                        .any(|other| wall_footprints_overlap(wall, other))
                {
                    return None;
                }
                Some(WallAabb::with_y(
                    wall.x, wall.y, wall.z, wall.width, height, wall.depth,
                ))
            })
            .collect();
        let item_groups = level
            .props
            .iter()
            .zip(level.prop_instance_ids())
            .filter_map(|(prop, id)| Some((id, prop.physics.as_ref()?.reset_group.clone()?)))
            .collect();
        Self {
            attachments: BTreeMap::new(),
            occluders,
            item_groups,
        }
    }

    pub(super) fn owns_group(&self, group: &str) -> bool {
        self.attachments.values().any(|attachment| {
            self.item_groups
                .get(&attachment.item)
                .is_some_and(|owned| owned == group)
        })
    }

    /// Restores only transferred bodies; the caller reuses their render keys.
    /// On failure, newly restored bodies are rolled back and custody remains.
    fn restore_items(
        &mut self,
        physics: &mut crate::engine::physics::ItemWorld,
    ) -> Result<Vec<RestoredCarry>, String> {
        let mut restored: Vec<RestoredCarry> = Vec::with_capacity(self.attachments.len());
        for (actor, attachment) in &self.attachments {
            let original = &attachment.reset;
            if let Err(problem) = physics.spawn_authored_oriented(
                attachment.key,
                &original.definition,
                original.bounds,
                original.pose.position,
                original.pose.rotation,
                original.scale,
            ) {
                for previous in &restored {
                    let _removed = physics.remove(previous.key);
                }
                return Err(format!(
                    "Cannot restore NPC-owned item '{}': {problem}",
                    attachment.item
                ));
            }
            restored.push(RestoredCarry {
                actor: actor.clone(),
                key: attachment.key,
                pose: original.pose,
            });
        }
        self.attachments.clear();
        Ok(restored)
    }

    pub(super) fn diagnostics(
        &self,
        renderer: &crate::engine::render::Renderer,
    ) -> serde_json::Value {
        serde_json::json!({
            "opaque_uncut_walls":self.occluders.len(),
            "attachments":self.attachments.iter().map(|(actor,attachment)| serde_json::json!({
                "actor":actor,"item":attachment.item,"key":attachment.key,"socket":attachment.socket,
                "retirement_requested":attachment.retirement_requested,"retired":attachment.retired,
                "whole_assembly_hidden":attachment.hidden,
                "item_mesh_visible":renderer.runtime_model_lighting_snapshot(attachment.key)
                    .and_then(|snapshot| snapshot.get("live").and_then(|live| live.get("visible")).and_then(serde_json::Value::as_bool)),
                "character_bounds":attachment.character_bounds.map(|bounds| serde_json::json!({"min":bounds.min,"max":bounds.max})),
                "item_bounds":attachment.item_bounds.map(|bounds| serde_json::json!({"min":bounds.min,"max":bounds.max})),
                "assembly_bounds":attachment.bounds.map(|bounds| serde_json::json!({"min":bounds.min,"max":bounds.max})),
            })).collect::<Vec<_>>()
        })
    }
}

impl FrameLoop<'_> {
    pub(super) fn restore_carry_visibility(&mut self, level: &LevelDef) {
        for (prop, id) in level.props.iter().zip(level.prop_instance_ids()) {
            if prop
                .components
                .iter()
                .any(|component| matches!(component, crate::engine::level::ComponentDef::Carry(_)))
            {
                let _restored = self.renderer.set_character_visible(&id, true);
            }
        }
    }

    pub(super) fn carry_group_command_blocked(&mut self, group: &str) -> bool {
        if !self.items.carry.owns_group(group) {
            return false;
        }
        crate::engine::logging::event(format!(
            "[physics] group '{group}' refused: an NPC owns a member; use a full map reset"
        ));
        self.trace.record(
            "physics_group_owned_refusal",
            self.load_generation,
            &serde_json::json!({"group":group,"reason":"npc_owned_item","atomic":true}).to_string(),
        );
        true
    }

    pub(super) fn reset_carry_assemblies(&mut self) {
        match self.items.carry.restore_items(&mut self.items.world) {
            Ok(restored) => {
                for item in restored {
                    let _actor_restored = self.renderer.set_character_visible(&item.actor, true);
                    let _prop_restored = self.renderer.set_runtime_model_visible(item.key, true);
                    let _pose_restored = self.renderer.set_runtime_model_pose(
                        item.key,
                        item.pose.position.to_array(),
                        item.pose.rotation.to_array(),
                    );
                }
                self.items.input.invalidate();
                self.items.target = None;
                self.observe_held_item();
            }
            Err(problem) => {
                self.fatal_error = Some(problem);
                self.game.stop();
            }
        }
    }

    fn carry_item_reset(&mut self, item: &str) -> Option<OwnedItemReset> {
        let level = &self.current_level.as_ref()?.level;
        let identities = level.prop_instance_ids();
        let prop = level
            .props
            .iter()
            .zip(&identities)
            .find_map(|(prop, identity)| (identity == item).then_some(prop))?;
        let definition = prop.physics.as_ref()?.clone();
        let (minimum, maximum) = self.renderer.rigid_runtime_model_bounds(&prop.model).ok()?;
        let surfaces = crate::engine::level::LevelSurfaces::new(level);
        Some(OwnedItemReset {
            definition,
            bounds: (Vec3::from_array(minimum), Vec3::from_array(maximum)),
            pose: crate::engine::physics::ItemPose {
                position: Vec3::new(
                    prop.x,
                    surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                    prop.z,
                ),
                rotation: prop.orientation(),
            },
            scale: prop.scale,
        })
    }

    fn carry_assets_ready(
        &mut self,
        target: &str,
        definition: &crate::engine::level::CarryDef,
        key: u64,
    ) -> bool {
        if self
            .renderer
            .character_socket_transform(target, &definition.socket)
            .is_some()
            && self
                .renderer
                .character_has_clip(target, &definition.idle_clip)
            && self
                .renderer
                .character_has_clip(target, &definition.walk_clip)
            && self.renderer.runtime_model_world_bounds(key).is_some()
        {
            return true;
        }
        crate::engine::logging::warn_once(
            format!("carry:{target}"),
            format!(
                "[carry] '{target}' handoff refused: socket '{}', clips '{}'/'{}', or actual item render resource could not resolve",
                definition.socket, definition.idle_clip, definition.walk_clip
            ),
        );
        self.trace_world("npc_handoff_asset_refusal");
        false
    }

    pub(super) fn observe_held_item(&mut self) {
        let identity = self.items.world.held().and_then(|key| {
            self.items
                .authored
                .iter()
                .find(|(body, _, _)| *body == key)
                .map(|(_, id, _)| id.clone())
        });
        self.game.entities_mut().observe_held_item(identity);
    }

    fn handoff_reachable(&mut self, target: &str, key: u64, menu_pickup: bool) -> bool {
        if menu_pickup {
            let Some(actor) = self
                .game
                .entity_frames()
                .iter()
                .find(|frame| frame.instance_id == target)
                .and_then(|frame| frame.transform)
                .map(|(position, _)| position)
            else {
                return false;
            };
            let mut nearby = false;
            self.items.world.for_each_pose(|body, pose| {
                if body == key {
                    nearby = pose.position.distance(actor) < 1.7;
                }
            });
            if !nearby {
                return false;
            }
        } else {
            if self.items.world.held() != Some(key) {
                return false;
            }
            self.synchronize_physical_player();
            let blocker = self.items.world.first_unheld_body_hit(
                self.game.player_position,
                self.game.view_direction(),
                crate::engine::interact::MAX_INTERACTION_REACH_M,
            );
            if !self
                .game
                .carried_door_target(blocker.map(|(_, distance)| distance))
                .is_some_and(|(index, _)| {
                    self.game
                        .interactables()
                        .get(index)
                        .is_some_and(|item| item.id == target)
                })
            {
                return false;
            }
        }
        true
    }

    pub(super) fn handoff_item(&mut self, target: &str) {
        if self.items.carry.attachments.contains_key(target) {
            return;
        }
        let Some(handle) = self.game.entities().handle_of(target) else {
            return;
        };
        let Some(carry) = self.game.entities().components().carries.get(handle) else {
            return;
        };
        if carry.item.is_some() || carry.retired {
            return;
        }
        let def = carry.def.clone();
        let Some(key) = self
            .items
            .authored
            .iter()
            .find(|(_, id, _)| id == &def.item)
            .map(|(key, _, _)| *key)
        else {
            return;
        };
        let menu_pickup = self.menu_world() && self.menu_active();
        if !self.handoff_reachable(target, key, menu_pickup) {
            return;
        }
        if !self.carry_assets_ready(target, &def, key) {
            return;
        }
        let Some(reset) = self.carry_item_reset(&def.item) else {
            return;
        };
        let [x, y, z] = def.rotation_degrees.map(f32::to_radians);
        let local = Mat4::from_rotation_translation(
            Quat::from_euler(EulerRot::XYZ, x, y, z),
            Vec3::from_array(def.offset),
        );
        let transferred = if menu_pickup {
            self.items.world.remove_movable(key)
        } else {
            self.items.world.transfer_held(key)
        };
        if !transferred {
            return;
        }
        let item = def.item;
        let socket = def.socket;
        drop(self.items.carry.attachments.insert(
            target.to_string(),
            Attachment {
                key,
                item: item.clone(),
                socket,
                local,
                retirement_requested: false,
                retired: false,
                bounds: None,
                character_bounds: None,
                item_bounds: None,
                hidden: false,
                reset,
            },
        ));
        self.items.input.invalidate();
        self.items.target = None;
        self.game.entities_mut().confirm_handoff(target, &item);
        self.trace_world("npc_item_handoff");
    }

    pub(super) fn release_carried_item(&mut self, target: &str) {
        let Some(attachment) = self.items.carry.attachments.get(target) else {
            return;
        };
        if attachment.retired {
            return;
        }
        let Some(socket) = self
            .renderer
            .character_socket_transform(target, &attachment.socket)
        else {
            return;
        };
        let (_, rotation, position) = socket
            .mul_mat4(&attachment.local)
            .to_scale_rotation_translation();
        let original = &attachment.reset;
        if let Err(problem) = self.items.world.restore_transferred(
            attachment.key,
            &original.definition,
            original.bounds,
            original.pose,
            crate::engine::physics::ItemPose { position, rotation },
            original.scale,
        ) {
            self.ui_state.set_status(
                format!("Could not put down the carried item: {problem}"),
                true,
            );
            return;
        }
        drop(self.items.carry.attachments.remove(target));
        self.game.entities_mut().confirm_item_release(target);
        self.trace_world("npc_item_released");
    }

    pub(super) fn request_carry_retirement(&mut self, target: &str) {
        if let Some(attachment) = self.items.carry.attachments.get_mut(target) {
            attachment.retirement_requested = true;
        }
    }

    pub(super) fn update_carry_attachments(&mut self) {
        if self.items.carry.attachments.is_empty() {
            return;
        }
        let mut retired = Vec::new();
        let eye = self.frame_camera().position;
        let can_retire = self.gameplay_active();
        for (actor, attachment) in &mut self.items.carry.attachments {
            if attachment.retired {
                continue;
            }
            let Some(socket) = self
                .renderer
                .character_socket_transform(actor, &attachment.socket)
            else {
                continue;
            };
            let (_, rotation, position) = socket
                .mul_mat4(&attachment.local)
                .to_scale_rotation_translation();
            let _moved = self.renderer.set_runtime_model_pose(
                attachment.key,
                position.to_array(),
                rotation.to_array(),
            );
            let Some(mut bounds) = self.renderer.character_posed_bounds(actor) else {
                continue;
            };
            let Some(prop_bounds) = self.renderer.runtime_model_world_bounds(attachment.key) else {
                continue;
            };
            attachment.character_bounds = Some(bounds);
            attachment.item_bounds = Some(prop_bounds);
            bounds.expand(prop_bounds.min);
            bounds.expand(prop_bounds.max);
            attachment.bounds = Some(bounds);
            attachment.hidden = crate::engine::carry_visibility::assembly_hidden(
                eye,
                bounds,
                &self.items.carry.occluders,
            );
            if can_retire && attachment.retirement_requested && attachment.hidden {
                // Both resources were preflighted at handoff and remain owned
                // here until map reset. A failed hide never marks retirement.
                if self.renderer.set_character_visible(actor, false)
                    && self
                        .renderer
                        .set_runtime_model_visible(attachment.key, false)
                {
                    attachment.retired = true;
                    retired.push(actor.clone());
                } else {
                    let _restored_actor = self.renderer.set_character_visible(actor, true);
                    let _restored_prop = self
                        .renderer
                        .set_runtime_model_visible(attachment.key, true);
                }
            }
        }
        for actor in retired {
            self.game.entities_mut().confirm_carry_retired(&actor);
            self.trace_world("npc_retired_hidden");
        }
        let _updated = self.renderer.update_dynamic(0.0);
    }
}

#[cfg(test)]
#[path = "../tests/carry_reset.rs"]
mod tests;

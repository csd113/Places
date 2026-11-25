//! Native aiming and action handoff for the fixed electrical cabinet.

use glam::Vec3;

use super::{EntityWorld, WorldContext, door_pose_hits_box, door_pose_hits_player};
use crate::engine::{
    electrical_panel::{Panel, SIZE},
    interact::{Interactable, InteractableSync},
    spatial::Aabb,
};

pub(super) enum PanelUse {
    Other,
    Unavailable,
    Door,
    Lever(bool),
}

impl EntityWorld {
    #[must_use]
    pub fn electrical_panels(&self) -> &[Panel] {
        &self.fixed_mechanisms.electrical_panels
    }

    pub(super) fn begin_panel_use(&mut self, id: &str) -> PanelUse {
        let Some(index) = self
            .electrical_panels()
            .iter()
            .position(|panel| panel.id == id || panel.door_id == id)
        else {
            return PanelUse::Other;
        };
        let Some(cabinet) = self.electrical_panels().get(index) else {
            return PanelUse::Unavailable;
        };
        let enabled = self
            .handle_of(&cabinet.id)
            .and_then(|handle| self.components.interactables.get(handle))
            .is_some_and(|component| component.enabled);
        if !enabled {
            return PanelUse::Unavailable;
        }
        let Some(panel) = self.fixed_mechanisms.electrical_panels.get_mut(index) else {
            return PanelUse::Unavailable;
        };
        let result = if panel.door_id == id {
            panel.toggle_door();
            PanelUse::Door
        } else if panel.lever_accessible() {
            PanelUse::Lever(panel.toggle_lever())
        } else {
            return PanelUse::Unavailable;
        };
        self.sync_panel_controls();
        result
    }

    pub(super) fn sync_panel_controls(&mut self) {
        let start = self
            .guest_collider_start
            .saturating_sub(self.fixed_mechanisms.electrical_panels.len());
        for (index, panel) in self.fixed_mechanisms.electrical_panels.iter().enumerate() {
            let Some(component) = self
                .names
                .get(&panel.id)
                .and_then(|handle| self.components.interactables.get(handle))
            else {
                continue;
            };
            let leaf = panel.leaf();
            let sync = InteractableSync::from_door_collider(&leaf);
            let door_slot = start.saturating_add(index);
            let owned_box = Some(panel.body());
            self.interactables.publish_control(Interactable {
                id: panel.door_id.clone(),
                display_name: "Electrical panel".into(),
                prompt: if panel.opening_requested() {
                    "Close panel"
                } else {
                    "Open panel"
                }
                .into(),
                reach: component.reach,
                anchor: sync.anchor,
                bounds: sync.bounds,
                yaw_degrees: panel.yaw,
                size: SIZE,
                own_box: owned_box,
                door_index: Some(door_slot),
                leaf_range: None,
                body_range: None,
                enabled: component.enabled,
            });
            let [_, lever] = panel.parts();
            let forward = glam::Quat::from_rotation_y(panel.yaw.to_radians()).mul_vec3(Vec3::Z);
            let centre = forward.mul_add(Vec3::splat(0.022), lever.origin);
            let [width, height, depth] = [0.12_f32, 0.19, 0.14];
            let (x, z) = super::rotated_half_extents(width * 0.5, depth * 0.5, panel.yaw);
            let [cx, cy, cz] = centre.to_array();
            let half_height = height * 0.5;
            self.interactables.publish_control(Interactable {
                id: panel.id.clone(),
                display_name: "Electrical panel".into(),
                prompt: if panel.powered() {
                    "Switch power off"
                } else {
                    "Switch power on"
                }
                .into(),
                reach: component.reach,
                anchor: centre,
                bounds: Aabb {
                    min: [cx - x, cy - half_height, cz - z],
                    max: [cx + x, cy + half_height, cz + z],
                },
                yaw_degrees: panel.yaw,
                size: [width, height, depth],
                own_box: owned_box,
                door_index: None,
                leaf_range: None,
                body_range: None,
                enabled: component.enabled && panel.lever_accessible(),
            });
        }
    }

    pub(super) fn advance_panels(&mut self, ctx: &WorldContext<'_>) -> usize {
        let mut moved = 0_usize;
        let panels = &mut self.fixed_mechanisms.electrical_panels;
        let start = self.guest_collider_start.saturating_sub(panels.len());
        for (slot, panel) in panels.iter_mut().enumerate() {
            let own = panel.body();
            if panel.advance(ctx.delta_seconds, |candidate| {
                door_pose_hits_player(candidate, ctx.feet, ctx.body_height, ctx.body_half)
                    || ctx
                        .walls
                        .iter()
                        .filter(|wall| **wall != own)
                        .any(|wall| door_pose_hits_box(candidate, wall))
                    || self
                        .item_obstacles
                        .all
                        .iter()
                        .any(|obstacle| door_pose_hits_box(candidate, obstacle))
                    || self
                        .door_colliders
                        .iter()
                        .enumerate()
                        .any(|(index, collider)| {
                            index != start.saturating_add(slot) && candidate.overlaps_leaf(collider)
                        })
                    || self.ai.agents().iter().any(|agent| {
                        !matches!(agent.state, crate::engine::ai::AiState::Hidden)
                            && candidate.overlaps_body_y(agent.position.y, agent.profile.height)
                            && candidate.overlaps_disc(
                                agent.position.x,
                                agent.position.z,
                                agent.profile.radius,
                            )
                    })
            }) {
                moved = moved.saturating_add(1);
            }
        }
        if moved > 0 {
            self.doors.notify_clearance_changed();
        }
        moved
    }
}

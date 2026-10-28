//! Explicit, reversible joins for the six delivered decorative duct models.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use glam::{EulerRot, Mat3, Quat, Vec3};
use serde::Deserialize;

use super::App;
use super::actions::Popup;
use super::document::{ObjectId, Transform};
use super::ui::{self, Action, Hit};
use crate::engine::level::{LevelDef, LevelSurfaces, PropDef};
use crate::engine::render::Vertex;

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Port {
    position: [f32; 3],
    outward_normal: [f32; 3],
    height_axis: [f32; 3],
}

type Kit = BTreeMap<String, [Port; 2]>;

fn kit() -> Result<&'static Kit, String> {
    static KIT: OnceLock<Result<Kit, String>> = OnceLock::new();
    KIT.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../tools/props/backrooms_phase6/duct_join_metadata.json"
        ))
        .map_err(|error| format!("Cannot read duct connector metadata: {error}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(super) fn supported(model: &str) -> bool {
    kit().is_ok_and(|data| data.contains_key(model))
}

#[derive(Clone, Copy)]
pub(super) struct WorldPort {
    pub position: Vec3,
    pub normal: Vec3,
    pub height: Vec3,
}

pub(super) struct Guide {
    pub id: ObjectId,
    pub ports: [WorldPort; 2],
}

pub(super) fn guides(level: &LevelDef) -> Vec<Guide> {
    level
        .props
        .iter()
        .enumerate()
        .filter_map(|(index, prop)| {
            world_ports(level, prop).ok().map(|ports| Guide {
                id: ObjectId::Prop(index),
                ports,
            })
        })
        .collect()
}

fn local_ports(prop: &PropDef) -> Result<&'static [Port; 2], String> {
    if prop.span.is_some() || prop.float.is_some() || prop.is_physics_movable() {
        return Err(
            "Duct joins need ordinary fixed props without fitted spans or floating poses".into(),
        );
    }
    kit()?
        .get(&prop.model)
        .ok_or_else(|| "Select a utility HVAC duct first".into())
}

fn world_ports(level: &LevelDef, prop: &PropDef) -> Result<[WorldPort; 2], String> {
    let local = local_ports(prop)?;
    let floor = LevelSurfaces::new(level)
        .floor_y_at(prop.x, prop.z)
        .unwrap_or(0.0);
    let transform = prop.placement_matrix(floor);
    let rotation = prop.orientation();
    Ok(local.map(|port| WorldPort {
        position: transform.transform_point3(Vec3::from_array(port.position)),
        normal: rotation.mul_vec3(Vec3::from_array(port.outward_normal)),
        height: rotation.mul_vec3(Vec3::from_array(port.height_axis)),
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    End,
    Target(bool),
    Face(usize),
    Snap,
    Apply,
    Edit,
}

struct Target {
    index: usize,
    end: usize,
    model: String,
    port: WorldPort,
    scale: f32,
}

pub(super) struct Draft {
    index: usize,
    end: usize,
    target: usize,
    targets: Vec<Target>,
    proposed: Transform,
    expected: serde_json::Value,
    preview: bool,
}

impl Draft {
    fn new(app: &App) -> Result<Self, String> {
        let document = app.document.as_ref().ok_or("Open a project first")?;
        let Some(ObjectId::Prop(index)) = app.selected else {
            return Err("Select a utility HVAC duct first".into());
        };
        let prop = document
            .level()
            .props
            .get(index)
            .ok_or("The duct no longer exists")?;
        let ports = world_ports(document.level(), prop)?;
        let mut targets = Vec::new();
        for (other_index, other) in document.level().props.iter().enumerate() {
            if other_index == index
                || !app
                    .focus
                    .included(document.level(), ObjectId::Prop(other_index))
            {
                continue;
            }
            if let Ok(other_ports) = world_ports(document.level(), other) {
                for (end, port) in other_ports.into_iter().enumerate() {
                    targets.push(Target {
                        index: other_index,
                        end,
                        model: other.model.clone(),
                        port,
                        scale: other.scale,
                    });
                }
            }
        }
        targets.sort_by(|a, b| {
            a.port
                .position
                .distance_squared(ports[0].position)
                .total_cmp(&b.port.position.distance_squared(ports[0].position))
        });
        targets.truncate(32);
        Ok(Self {
            index,
            end: 0,
            target: 0,
            targets,
            proposed: document.transform(ObjectId::Prop(index))?,
            expected: document.raw().clone(),
            preview: false,
        })
    }
}

fn rotation_pose(mut pose: Transform, rotation: Quat) -> Transform {
    let (yaw, pitch, roll) = rotation.to_euler(EulerRot::YXZ);
    pose.yaw_degrees = yaw.to_degrees();
    pose.pitch_degrees = pitch.to_degrees();
    pose.roll_degrees = roll.to_degrees();
    pose
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Validated finite metre-space orthonormal connector frames use bounded floating-point geometry."
)]
fn snap_pose(
    level: &LevelDef,
    prop: &PropDef,
    end: usize,
    target: &Target,
    pose: Transform,
) -> Result<Transform, String> {
    if (prop.scale - target.scale).abs() > 0.0001 {
        return Err(
            "Connected ducts need matching scales; set Scale in Pose before joining".into(),
        );
    }
    let port = local_ports(prop)?.get(end).ok_or("Unknown connector end")?;
    let n = Vec3::from_array(port.outward_normal);
    let h = Vec3::from_array(port.height_axis);
    let source = Quat::from_mat3(&Mat3::from_cols(n, h, n.cross(h)));
    let normal = -target.port.normal;
    let mut rotations = [Quat::IDENTITY; 2];
    for (slot, height) in rotations
        .iter_mut()
        .zip([target.port.height, -target.port.height])
    {
        *slot = (Quat::from_mat3(&Mat3::from_cols(normal, height, normal.cross(height)))
            * source.conjugate())
        .normalize();
    }
    let preferred = Quat::from_euler(
        EulerRot::YXZ,
        pose.yaw_degrees.to_radians(),
        pose.pitch_degrees.to_radians(),
        pose.roll_degrees.to_radians(),
    );
    let rotation = rotations
        .into_iter()
        .max_by(|a, b| a.dot(preferred).abs().total_cmp(&b.dot(preferred).abs()))
        .ok_or("Missing connector frame")?;
    let origin =
        target.port.position - rotation.mul_vec3(Vec3::from_array(port.position)) * prop.scale;
    let floor = LevelSurfaces::new(level)
        .floor_y_at(origin.x, origin.z)
        .unwrap_or(0.0);
    let mut result = rotation_pose(pose, rotation);
    result.position = [origin.x, origin.y - floor, origin.z];
    Ok(result)
}

fn preview_bounds(
    level: &LevelDef,
    index: usize,
    target: Option<usize>,
) -> Result<crate::engine::spatial::Aabb, String> {
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let surfaces = LevelSurfaces::new(level);
    let mut bounds = crate::engine::spatial::Aabb::EMPTY;
    for member in std::iter::once(index).chain(target) {
        let prop = level
            .props
            .get(member)
            .ok_or("The preview duct no longer exists")?;
        let floor = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let occupied = prop.placement_bounds(floor, catalog.get(&prop.model).size);
        bounds.expand(occupied.min);
        bounds.expand(occupied.max);
    }
    Ok(bounds)
}

fn scene_preview(
    document: &super::document::Document,
    draft: &Draft,
    command: Command,
) -> Result<(LevelDef, crate::engine::spatial::Aabb), String> {
    let preview = document.preview_transform(ObjectId::Prop(draft.index), draft.proposed)?;
    let target = if command == Command::Snap {
        draft.targets.get(draft.target).map(|choice| choice.index)
    } else {
        None
    };
    let bounds = preview_bounds(&preview, draft.index, target)?;
    Ok((preview, bounds))
}

impl App {
    pub(super) fn duct_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Duct(command) = action else {
            return Ok(false);
        };
        if command == Command::Open {
            self.popup = Some(Popup::Duct(Box::new(Draft::new(self)?)));
            return Ok(true);
        }
        let Some(Popup::Duct(draft)) = &mut self.popup else {
            return Err("Open Duct joins first".into());
        };
        let document = self.document.as_ref().ok_or("Open a project first")?;
        if document.raw() != &draft.expected {
            return Err("The scene changed; reopen Duct joins".into());
        }
        let prop = document
            .level()
            .props
            .get(draft.index)
            .ok_or("The duct no longer exists")?;
        match command {
            Command::Edit => {
                draft.preview = false;
                return Ok(true);
            }
            Command::Open => return Err("Reopen Duct joins".into()),
            Command::End => {
                draft.end = usize::from(draft.end == 0);
                return Ok(true);
            }
            Command::Target(next) => {
                if !draft.targets.is_empty() {
                    draft.target = if next {
                        draft
                            .target
                            .saturating_add(1)
                            .checked_rem(draft.targets.len())
                            .unwrap_or(0)
                    } else {
                        draft
                            .target
                            .checked_sub(1)
                            .unwrap_or_else(|| draft.targets.len().saturating_sub(1))
                    };
                }
                return Ok(true);
            }
            Command::Face(axis) => {
                let direction = [
                    Vec3::X,
                    Vec3::NEG_X,
                    Vec3::Z,
                    Vec3::NEG_Z,
                    Vec3::Y,
                    Vec3::NEG_Y,
                ]
                .get(axis)
                .copied()
                .ok_or("Unknown duct direction")?;
                let mut current = prop.clone();
                current.rotation_degrees = draft.proposed.yaw_degrees;
                current.pitch_degrees = draft.proposed.pitch_degrees;
                current.roll_degrees = draft.proposed.roll_degrees;
                let port = world_ports(document.level(), &current)?
                    .get(draft.end)
                    .copied()
                    .ok_or("Unknown connector end")?;
                draft.proposed = rotation_pose(
                    draft.proposed,
                    Quat::from_rotation_arc(port.normal, direction).mul_quat(current.orientation()),
                );
            }
            Command::Snap => {
                let target = draft
                    .targets
                    .get(draft.target)
                    .ok_or("Place a second duct before joining ends")?;
                draft.proposed =
                    snap_pose(document.level(), prop, draft.end, target, draft.proposed)?;
            }
            Command::Apply => {
                let id = ObjectId::Prop(draft.index);
                let pose = draft.proposed;
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .set_transform(id, pose)?;
                self.popup = None;
                self.changed();
                self.status =
                    "Duct pose applied as one undo step. End arrows show outward directions."
                        .into();
                return Ok(true);
            }
        }
        let (preview, bounds) = scene_preview(document, draft, command)?;
        draft.preview = true;
        self.viewport.rebuild(&preview);
        self.viewport.frame_bounds(bounds);
        self.status = "Duct preview only. Apply commits; Cancel restores source. Same-scale rectangular ends align at flange faces.".into();
        Ok(true)
    }
}

pub(super) fn draw(draft: &Draft, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let action = |command| Action::Duct(command);
    if draft.preview {
        draw_preview(vertices, hits);
        return;
    }
    // Keep the native viewport visible while previewing connector poses.
    ui::rect(
        vertices,
        [116.0, 45.0, 354.0, 194.0],
        [0.075, 0.11, 0.14, 0.96],
    );
    ui::text(
        vertices,
        "DUCT JOINS / decorative sheet metal",
        126.0,
        54.0,
        ui::INK,
        65,
    );
    ui::text(
        vertices,
        "A = green, B = pink; arrows point out of each end.",
        126.0,
        66.0,
        ui::MUTED,
        72,
    );
    ui::popup_button(
        vertices,
        hits,
        [126.0, 80.0, 150.0, 13.0],
        if draft.end == 0 {
            "Source end: A"
        } else {
            "Source end: B"
        },
        action(Command::End),
    );
    for (index, label) in ["+X", "-X", "+Z", "-Z", "Up", "Down"]
        .into_iter()
        .enumerate()
    {
        let slot = u16::try_from(index).unwrap_or(0);
        ui::popup_button(
            vertices,
            hits,
            [f32::from(slot).mul_add(55.0, 126.0), 100.0, 49.0, 13.0],
            label,
            action(Command::Face(index)),
        );
    }
    draw_targets(draft, vertices, hits);
}

fn draw_targets(draft: &Draft, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let action = |command| Action::Duct(command);
    if let Some(target) = draft.targets.get(draft.target) {
        ui::text(
            vertices,
            &format!(
                "Target prop {} / end {}",
                target.index.saturating_add(1),
                if target.end == 0 { "A" } else { "B" }
            ),
            126.0,
            125.0,
            ui::INK,
            65,
        );
        ui::text(vertices, &target.model, 126.0, 137.0, ui::MUTED, 65);
    } else {
        ui::text(
            vertices,
            "Place another duct to enable an end-to-end join.",
            126.0,
            125.0,
            ui::MUTED,
            65,
        );
    }
    for (x, label, command) in [
        (126.0, "< Target", Command::Target(false)),
        (238.0, "Target >", Command::Target(true)),
        (350.0, "Preview join", Command::Snap),
    ] {
        ui::popup_button(
            vertices,
            hits,
            [x, 151.0, 101.0, 13.0],
            label,
            action(command),
        );
    }
    ui::text(
        vertices,
        if draft.preview {
            "PREVIEW / source unchanged"
        } else {
            "Choose an end, direction or target; then preview."
        },
        126.0,
        175.0,
        ui::INK,
        70,
    );
    ui::text(
        vertices,
        "Joins match scale and rectangular orientation; no resizing.",
        126.0,
        187.0,
        ui::MUTED,
        70,
    );
    ui::text(
        vertices,
        "Use Pose for free pitch/roll. Ducts are not crouch tunnels.",
        126.0,
        199.0,
        ui::MUTED,
        70,
    );
    ui::popup_button(
        vertices,
        hits,
        [126.0, 219.0, 158.0, 13.0],
        "Apply pose",
        action(Command::Apply),
    );
    ui::popup_button(
        vertices,
        hits,
        [296.0, 219.0, 158.0, 13.0],
        "Cancel",
        Action::ClosePopup,
    );
}

fn draw_preview(vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let action = |command| Action::Duct(command);
    ui::rect(
        vertices,
        [122.0, 207.0, 348.0, 31.0],
        [0.075, 0.11, 0.14, 1.0],
    );
    ui::text(
        vertices,
        "DUCT PREVIEW / source unchanged",
        130.0,
        211.0,
        ui::INK,
        65,
    );
    for (x, label, command) in [
        (130.0, "Edit joins", Command::Edit),
        (242.0, "Apply pose", Command::Apply),
    ] {
        ui::popup_button(
            vertices,
            hits,
            [x, 222.0, 105.0, 12.0],
            label,
            action(command),
        );
    }
    ui::popup_button(
        vertices,
        hits,
        [354.0, 222.0, 105.0, 12.0],
        "Cancel",
        Action::ClosePopup,
    );
}

#[cfg(test)]
#[path = "tests/ducts.rs"]
mod tests;

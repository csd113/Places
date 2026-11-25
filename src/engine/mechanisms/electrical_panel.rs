//! Placement and persistent mechanism state for the wall-mounted cabinet.

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::engine::{
    collision::{CONTACT_EPS, DoorCollider, WallAabb},
    level::{
        ComponentDef, LevelDef, LevelSurfaces, PropDef, RoomDef, WalkableFloor, WallAxis, WallDef,
        wall_solid_slices_profiled,
    },
    spatial::Aabb,
};

pub const MODEL: &str = "maintenance:electrical_panel";
pub const BODY: &str = "environments/maintenance/props/electrical_panel_body.glb";
const DOOR: &str = "environments/maintenance/props/electrical_panel_door.glb";
const LEVER: &str = "environments/maintenance/props/electrical_panel_lever.glb";
const RISER: &str = "environments/maintenance/props/electrical_panel_conduits.glb";
const COLLARS: &str = "environments/maintenance/props/electrical_panel_conduit_collars.glb";
const CLAMP: &str = "environments/maintenance/props/electrical_panel_conduit_clamp.glb";
pub const MOVING_PARTS: [&str; 2] = [DOOR, LEVER];
pub const CONDUIT_PARTS: [&str; 3] = [RISER, COLLARS, CLAMP];
pub const INTERNAL_MODELS: [(&str, &str); 5] = [
    ("maintenance:electrical_panel_door", DOOR),
    ("maintenance:electrical_panel_lever", LEVER),
    ("maintenance:electrical_panel_conduits", RISER),
    ("maintenance:electrical_panel_conduit_collars", COLLARS),
    ("maintenance:electrical_panel_conduit_clamp", CLAMP),
];
pub const CAPABILITY: &str = "electrical-panel-v1";
pub const SIZE: [f32; 3] = [0.58, 0.72, 0.18];
pub const MAX_PANELS: usize = 64;
/// One Full circuit can bake a bounded whole room's fixtures as a group.
pub const MAX_CIRCUIT_SOURCES: usize = 64;
pub(crate) const MOUNT_GAP: f32 = 0.002;
const DOOR_HINGE: Vec3 = Vec3::new(-0.276, 0.0, 0.164);
const LEVER_AXLE: Vec3 = Vec3::new(0.0, 0.020, 0.077);
const DOOR_THICKNESS: f32 = 0.022;

/// Explicit fixed mechanism, separate from a passive model placement.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Unwired panels can start up or down. A wired panel takes the circuit's
    /// authored initial state so the physical lever agrees with the output.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub initially_on: bool,
}

#[derive(Clone, Copy)]
pub struct PartPose {
    pub origin: Vec3,
    pub rotation: Quat,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    id: String,
    pub(crate) open: bool,
    pub(crate) power: bool,
    opening: f32,
    lever: f32,
}

#[derive(Clone, Debug)]
pub struct Panel {
    pub id: String,
    pub door_id: String,
    origin: Vec3,
    rotation: Quat,
    pub yaw: f32,
    initial_power: bool,
    state: Record,
}

#[must_use]
pub fn present(level: &LevelDef) -> bool {
    level
        .props
        .iter()
        .any(|prop| prop.electrical_panel.is_some())
}

pub fn asset_paths() -> impl Iterator<Item = &'static str> {
    std::iter::once(BODY)
        .chain(MOVING_PARTS)
        .chain(CONDUIT_PARTS)
}

pub(crate) fn owns_circuit(level: &LevelDef, group: Option<&str>) -> bool {
    group.is_some_and(|name| circuit_names(level).any(|group_name| group_name == name))
}

pub(crate) fn circuit_names(level: &LevelDef) -> impl Iterator<Item = &str> {
    level
        .props
        .iter()
        .filter(|prop| prop.electrical_panel.is_some())
        .filter_map(|prop| prop.light_switch.as_deref())
}

/// Electrical cabinets have an upright, unit-scale body and one native use component.
/// # Errors
/// Rejects incompatible rig placements, conflicting movement or too many cabinets.
pub fn validate(level: &LevelDef) -> Result<(), String> {
    if !present(level) {
        return Ok(());
    }
    let ids = level.prop_instance_ids();
    let surfaces = LevelSurfaces::new(level);
    let mut count = 0_usize;
    for (index, (prop, id)) in level.props.iter().zip(ids).enumerate() {
        if prop.electrical_panel.is_none() {
            continue;
        }
        count = count.saturating_add(1);
        if count > MAX_PANELS {
            return Err(format!(
                "A level supports at most {MAX_PANELS} electrical panels"
            ));
        }
        let has_use = prop.components.iter().any(|component| {
            matches!(
                component,
                ComponentDef::Interactable {
                    enabled: true,
                    prompt: _,
                    reach: _,
                    label: _
                }
            )
        });
        if prop.model != MODEL
            || !prop.solid
            || !has_use
            || prop.physics.is_some()
            || prop.float.is_some()
            || prop.span.is_some()
            || prop.animated_door.is_some()
            || prop.escalator.is_some()
            || prop.shadow_only
            || prop.roll_degrees != 0.0
            || prop.pitch_degrees != 0.0
            || (prop.scale - 1.0).abs() > f32::EPSILON
            || !prop.material_overrides.is_empty()
            || !prop.emission_overrides.is_empty()
            || level.routes.iter().any(|route| route.id == id)
        {
            return Err(format!(
                "Prop {index} electrical panel needs an upright fixed cabinet at unit scale with an enabled interaction"
            ));
        }
        if !has_backing(level, &surfaces, prop) {
            return Err(format!(
                "Prop {index} electrical panel must sit flush against a solid wall face. Move along the wall or place it again on a supported face."
            ));
        }
    }
    Ok(())
}

fn has_backing(level: &LevelDef, surfaces: &LevelSurfaces<'_>, cabinet: &PropDef) -> bool {
    let Some(floor) = surfaces.floor_y_at(cabinet.x, cabinet.z) else {
        return false;
    };
    let centre = floor + cabinet.y;
    let half_height = SIZE[1] * 0.5;
    has_wall_span(
        level,
        surfaces,
        cabinet,
        SIZE[0] * 0.5,
        centre - half_height,
        centre + half_height,
    )
}

fn has_wall_span(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    cabinet: &PropDef,
    half_width: f32,
    bottom: f32,
    top: f32,
) -> bool {
    let forward = cabinet.orientation().mul_vec3(Vec3::Z);
    level.walls.iter().any(|wall| {
        let (x, z) = wall.min_corner();
        let (normal, cross, near, far, along) = match wall.axis() {
            WallAxis::X => (forward.z, cabinet.z, z, z + wall.depth.abs(), cabinet.x - x),
            WallAxis::Z => (forward.x, cabinet.x, x, x + wall.width.abs(), cabinet.z - z),
        };
        if normal.abs() < 1.0 - f32::EPSILON {
            return false;
        }
        let gap = if normal > 0.0 {
            cross - far
        } else {
            near - cross
        };
        (-CONTACT_EPS..=2.0 * MOUNT_GAP).contains(&gap)
            && covers_wall(
                surfaces,
                wall,
                along - half_width,
                along + half_width,
                bottom,
                top,
            )
    })
}

fn covers_wall(
    surfaces: &LevelSurfaces<'_>,
    wall: &WallDef,
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
) -> bool {
    let ceiling = |along| surfaces.clear_ceiling_height_along(wall, along);
    let mut spans = wall_solid_slices_profiled(wall, ceiling, &surfaces.wall_profile_breaks(wall));
    spans.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut coverage = left;
    for slice in spans {
        let start = left.max(slice.start);
        let end = right.min(slice.end);
        let roof = |along| wall.y + wall.height.unwrap_or_else(|| ceiling(along));
        if end > coverage
            && start <= coverage + CONTACT_EPS
            && bottom + CONTACT_EPS >= slice.bottom
            && top <= slice.top.min(roof(start)).min(roof(end)) + CONTACT_EPS
        {
            coverage = end;
            if coverage + CONTACT_EPS >= right {
                return true;
            }
        }
    }
    false
}

/// The cabinet uses center height and a back plane at local Z=0.
#[must_use]
pub fn body_bounds(matrix: glam::Mat4) -> Aabb {
    let [width, height, depth] = SIZE;
    let mut bounds = Aabb::EMPTY;
    for x in [-width * 0.5, width * 0.5] {
        for y in [-height * 0.5, height * 0.5] {
            for z in [0.0, depth] {
                bounds.expand(matrix.transform_point3(Vec3::new(x, y, z)).to_array());
            }
        }
    }
    bounds
}

impl Panel {
    fn new(prop: &PropDef, origin: Vec3, id: String, initial_power: bool) -> Self {
        let state = Record {
            id: id.clone(),
            open: false,
            power: initial_power,
            opening: 0.0,
            lever: if initial_power { 1.0 } else { 0.0 },
        };
        Self {
            door_id: format!("@panel:{id}:door"),
            id,
            origin,
            rotation: prop.orientation(),
            yaw: prop.rotation_degrees,
            initial_power,
            state,
        }
    }

    #[must_use]
    pub fn resolve(level: &LevelDef) -> Vec<Self> {
        if !present(level) {
            return Vec::new();
        }
        let ids = level.prop_instance_ids();
        let surfaces = LevelSurfaces::new(level);
        level
            .props
            .iter()
            .zip(ids)
            .filter_map(|(prop, id)| {
                let definition = prop.electrical_panel?;
                if !has_backing(level, &surfaces, prop) {
                    return None;
                }
                let floor = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
                let initial_power =
                    prop.light_switch
                        .as_deref()
                        .map_or(definition.initially_on, |group| {
                            level.ceiling_lights.iter().any(|light| {
                                light.switch_group.as_deref() == Some(group) && light.enabled
                            })
                        });
                Some(Self::new(
                    prop,
                    Vec3::new(prop.x, floor + prop.y, prop.z),
                    id,
                    initial_power,
                ))
            })
            .collect()
    }

    fn door_pose(&self, opening: f32) -> PartPose {
        PartPose {
            origin: self
                .rotation
                .mul_vec3(DOOR_HINGE)
                .mul_add(Vec3::ONE, self.origin),
            rotation: self
                .rotation
                .mul_quat(Quat::from_rotation_y(-1.75 * opening)),
        }
    }

    #[must_use]
    pub fn parts(&self) -> [PartPose; 2] {
        [
            self.door_pose(self.state.opening),
            PartPose {
                origin: self
                    .rotation
                    .mul_vec3(LEVER_AXLE)
                    .mul_add(Vec3::ONE, self.origin),
                rotation: self.rotation.mul_quat(Quat::from_rotation_x(
                    (-1.676_f32).mul_add(self.state.lever, 0.838),
                )),
            },
        ]
    }

    #[must_use]
    pub fn body(&self) -> WallAabb {
        let bounds = body_bounds(glam::Mat4::from_rotation_translation(
            self.rotation,
            self.origin,
        ));
        WallAabb::with_y(
            bounds.min[0],
            bounds.min[1],
            bounds.min[2],
            bounds.max[0] - bounds.min[0],
            bounds.max[1] - bounds.min[1],
            bounds.max[2] - bounds.min[2],
        )
    }

    fn leaf_at(&self, opening: f32) -> DoorCollider {
        let pose = self.door_pose(opening);
        let direction = pose.rotation.mul_vec3(Vec3::X);
        DoorCollider::from_pose(
            [pose.origin.x, pose.origin.y - 0.348, pose.origin.z],
            [direction.x, direction.z],
            0.548,
            DOOR_THICKNESS,
            0.696,
        )
    }

    #[must_use]
    pub fn leaf(&self) -> DoorCollider {
        self.leaf_at(self.state.opening)
    }

    #[must_use]
    pub const fn opening_requested(&self) -> bool {
        self.state.open
    }

    #[must_use]
    pub fn lever_accessible(&self) -> bool {
        self.state.open && self.state.opening >= 1.0
    }

    #[must_use]
    pub const fn powered(&self) -> bool {
        self.state.power
    }

    pub(crate) const fn toggle_door(&mut self) {
        self.state.open = !self.state.open;
    }

    pub(crate) const fn toggle_lever(&mut self) -> bool {
        self.state.power = !self.state.power;
        self.state.power
    }

    pub(crate) fn advance(&mut self, delta: f32, blocked: impl Fn(&DoorCollider) -> bool) -> bool {
        if !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        let mut changed = false;
        // Bound the angular sweep in one update, even after a slow frame.
        for _ in 0..4_u8 {
            let step = delta.min(0.05) * 0.75;
            let next = approach(self.state.opening, self.state.open, step);
            if next.to_bits() == self.state.opening.to_bits() || blocked(&self.leaf_at(next)) {
                break;
            }
            self.state.opening = next;
            changed = true;
        }
        self.state.lever = approach(self.state.lever, self.state.power, delta * 6.0);
        changed
    }

    pub(crate) const fn reset(&mut self) {
        self.state.open = false;
        self.state.opening = 0.0;
        self.state.power = self.initial_power;
        self.state.lever = if self.initial_power { 1.0 } else { 0.0 };
    }

    pub(crate) fn checkpoint(&self) -> Record {
        self.state.clone()
    }

    pub(crate) fn restore(&mut self, record: &Record) -> Result<(), String> {
        if record.id != self.id
            || ![record.opening, record.lever]
                .iter()
                .all(|v| (0.0..=1.0).contains(v))
        {
            return Err("Invalid electrical panel checkpoint".into());
        }
        self.state.clone_from(record);
        Ok(())
    }
}

fn approach(value: f32, on: bool, step: f32) -> f32 {
    if on {
        (value + step).min(1.0)
    } else {
        (value - step).max(0.0)
    }
}

pub(crate) fn conduit_height(
    level: &LevelDef,
    floor: &WalkableFloor,
    surfaces: &LevelSurfaces<'_>,
    cabinet: &PropDef,
) -> Option<f32> {
    if !has_backing(level, surfaces, cabinet) {
        return None;
    }
    let placement = cabinet.placement_matrix(surfaces.floor_y_at(cabinet.x, cabinet.z)?);
    let middle = placement.transform_point3(Vec3::new(0.0, 0.36, 0.068));
    let inside = |room: &RoomDef, at: Vec3| {
        let (x0, x1, z0, z1) = room.bounds();
        (x0..=x1).contains(&at.x) && (z0..=z1).contains(&at.z)
    };
    // Resolve the room of the cabinet, not the first roof in plan view.
    // An open room is an explicit result and must never borrow the roof above it.
    let room = level
        .room_iter()
        .filter(|room| {
            inside(room, middle)
                && room.floor_y <= middle.y - 0.36
                && room.ceiling_y_at(middle.x, middle.z) > middle.y
        })
        .max_by(|a, b| a.floor_y.total_cmp(&b.floor_y))?;
    if room.ceiling.is_open() {
        return None;
    }
    let ceiling = room.ceiling_y_at(middle.x, middle.z);
    let height = ceiling - middle.y;
    if height < 0.02 || !has_wall_span(level, surfaces, cabinet, 0.199, middle.y, ceiling) {
        return None;
    }
    for x in [-0.191, -0.055, 0.055, 0.191] {
        for z in [0.038, 0.098] {
            let at = placement.transform_point3(Vec3::new(x, 0.36, z));
            if !inside(room, at)
                || (room.ceiling_y_at(at.x, at.z) - ceiling).abs() > 0.002
                || floor
                    .lowest_room_underside_hull(Vec2::new(at.x, at.z), 0.001, at.y)
                    .is_some_and(|bottom| bottom < ceiling - 0.002)
            {
                return None;
            }
        }
    }
    Some(height)
}

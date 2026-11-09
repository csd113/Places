//! Authored passenger lifts. One pose owns cabin, sliding leaves and collision.
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::engine::collision::DoorCollider;
use crate::engine::interact::Interactable;
use crate::engine::spatial::Aabb;

pub const WIDTH: f32 = 2.4;
pub const DEPTH: f32 = 2.4;
pub const HEIGHT: f32 = 2.55;
/// Closed-door presentation time; cabins never translate during this interval.
pub const RIDE_SECONDS: f32 = 1.8;
/// Initial authoring limit; format and numbered-button assets allow future expansion.
pub const MAX_STOPS: usize = 8;
/// Default linked-cabin groups; authored landing sets retain eight fixed stops.
pub const MAX_LINKED_STOPS: usize = 3;
pub const MAX_LINK_DESTINATIONS: usize = MAX_LINKED_STOPS.saturating_sub(1);
pub const DOOR_WIDTH: f32 = 1.35;
pub const DOOR_HEIGHT: f32 = 2.2;
/// Concealed stock overlaps the clear opening, then retracts past its edge.
pub const DOOR_LEAF_WIDTH: f32 = DOOR_WIDTH * 0.5 + 0.02;
pub const DOOR_LEAF_TRAVEL: f32 = DOOR_WIDTH * 0.5 + 0.006;
pub const DOOR_LEAF_BOTTOM: f32 = -0.01;
pub const DOOR_LEAF_TOP: f32 = DOOR_HEIGHT + 0.02;
pub const DOOR_LEAF_SEAL: f32 = 0.004;
pub const DOOR_LEAF_FRONT: f32 = -0.0225;
pub const DOOR_LEAF_BACK: f32 = 0.03;
// The inner pair stays behind the landing wall from the passenger viewpoint,
// so hall lintels/floors cannot clip into the closed cabin while it travels.
pub const CABIN_DOOR_Z: f32 = 0.30;
pub const LANDING_DOOR_Z: f32 = 0.15;
/// A separate, authored door icon follows the dynamic destination rows.
pub const DOOR_BUTTON_GLYPH: u16 = 15;
#[must_use]
pub fn door_button_height(destinations: usize) -> f32 {
    let rows = u16::try_from(destinations.min(MAX_STOPS.saturating_sub(1))).unwrap_or(0);
    f32::from(rows.saturating_sub(1))
        .mul_add(-0.11, 1.44)
        .min(1.30)
}
/// Authored rigid components; the catalog keeps paths out of level definitions.
pub const MODEL_IDS: [&str; 4] = [
    "core:elevator_cabin_70s",
    "core:elevator_frame_70s",
    "core:elevator_leaf_70s",
    "core:elevator_button_70s",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub label: String,
    pub y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrival_elevator: Option<String>,
}

impl Stop {
    #[must_use]
    pub const fn is_landing(&self) -> bool {
        self.level.is_none() && self.arrival_elevator.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElevatorDef {
    pub id: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    pub stops: Vec<Stop>,
    #[serde(default)]
    pub initial_stop: usize,
    #[serde(default = "speed")]
    pub speed_mps: f32,
}
const fn speed() -> f32 {
    1.5
}

impl ElevatorDef {
    #[must_use]
    pub fn point(&self, x: f32, y: f32, z: f32) -> Vec3 {
        let (sin, cos) = self.rotation_degrees.to_radians().sin_cos();
        Vec3::new(
            cos.mul_add(x, sin.mul_add(z, self.x)),
            y,
            (-sin).mul_add(x, cos.mul_add(z, self.z)),
        )
    }
    #[must_use]
    pub fn local(&self, point: Vec3) -> Vec3 {
        let (sin, cos) = self.rotation_degrees.to_radians().sin_cos();
        let x = point.x - self.x;
        let z = point.z - self.z;
        Vec3::new(cos.mul_add(x, -sin * z), point.y, sin.mul_add(x, cos * z))
    }
}

/// Bounded authoring admission, shared by editor, compiler and package loading.
/// # Errors
/// Rejects unsafe dimensions, duplicated landings, IDs or undeclared transitions.
pub fn validate(level: &crate::engine::level::LevelDef) -> Result<(), String> {
    if level
        .elevators
        .iter()
        .map(|e| e.stops.iter().filter(|s| s.is_landing()).count())
        .sum::<usize>()
        > 8
    {
        return Err("At most 8 fixed elevator cabins per level".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for def in &level.elevators {
        if def.id.is_empty()
            || def.id.len() > 64
            || !def
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !ids.insert(&def.id)
        {
            return Err("Elevators need unique alphanumeric IDs (64 bytes maximum)".into());
        }
        if [def.x, def.z, def.rotation_degrees]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 100_000.0)
            || !def.speed_mps.is_finite()
            || !(0.25..=4.0).contains(&def.speed_mps)
            || !(1..=MAX_STOPS).contains(&def.stops.len())
            || def
                .stops
                .get(def.initial_stop)
                .is_none_or(|stop| !stop.is_landing())
        {
            return Err(format!(
                "Elevator {}: invalid position, speed, stops or initial landing",
                def.id
            ));
        }
        if def.stops.iter().filter(|stop| !stop.is_landing()).count() > MAX_LINK_DESTINATIONS {
            return Err("Each elevator can link to at most two other cabins for now".into());
        }
        for (i, stop) in def.stops.iter().enumerate() {
            if stop.label.is_empty()
                || stop.label.len() > 48
                || stop.label.chars().any(char::is_control)
                || !stop.y.is_finite()
                || stop.y.abs() > 10_000.0
                || stop
                    .level
                    .as_ref()
                    .is_some_and(|id| !level.transitions.contains(id))
                || stop
                    .arrival_elevator
                    .as_ref()
                    .is_some_and(|id| id.is_empty() || id.len() > 64)
            {
                return Err(format!(
                    "Elevator {}: invalid stop {} or undeclared destination",
                    def.id,
                    i.saturating_add(1)
                ));
            }
            if stop.level.is_none()
                && let Some(arrival) = &stop.arrival_elevator
                && (arrival == &def.id
                    || level.elevators.iter().all(|target| &target.id != arrival))
            {
                return Err(format!(
                    "Elevator {}: unknown local arrival {arrival}",
                    def.id
                ));
            }
            if stop.is_landing()
                && def
                    .stops
                    .iter()
                    .take(i)
                    .any(|other| other.is_landing() && (other.y - stop.y).abs() < HEIGHT + 0.2)
            {
                return Err(format!(
                    "Elevator {}: local landings need at least 2.75 m separation",
                    def.id
                ));
            }
        }
    }
    validate_link_groups(level)?;
    validate_shafts(level)
}

fn validate_link_groups(level: &crate::engine::level::LevelDef) -> Result<(), String> {
    for lift in &level.elevators {
        let mut group = std::collections::BTreeSet::from([lift.id.as_str()]);
        loop {
            let before = group.len();
            for source in &level.elevators {
                for stop in &source.stops {
                    if stop.level.is_none()
                        && let Some(target) = stop.arrival_elevator.as_deref()
                        && (group.contains(source.id.as_str()) || group.contains(target))
                    {
                        let _source = group.insert(source.id.as_str());
                        let _target = group.insert(target);
                    }
                }
            }
            if group.len() > MAX_LINKED_STOPS {
                return Err("Elevator groups support three stops for now (two destination buttons per cabin)".into());
            }
            if before == group.len() {
                break;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
    Moving,
    Loading,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    id: String,
    stop: usize,
    open: bool,
}

pub struct Elevator {
    pub def: ElevatorDef,
    pub y: f32,
    pub opening: f32,
    pub phase: Phase,
    pub at: usize,
    target: usize,
    travel_elapsed: f32,
    pub obstructed: bool,
    ambience: f32,
    ambience_count: u32,
    departure_sent: bool,
}

#[derive(Clone, Debug)]
pub struct Departure {
    pub level: Option<String>,
    pub arrival: String,
    pub source: String,
    /// Another fixed landing in the same authored elevator, if present.
    pub landing: Option<usize>,
}

/// Persistent cabin coordinates have no world-local handles or render data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CabinFrame {
    pub origin: Vec3,
    pub rotation_degrees: f32,
}

/// Apply the cabin's rigid transform using finite f32 vector operations.
#[must_use]
pub fn map_cabin_point(from: Vec3, to: Vec3, turn: glam::Quat, point: Vec3) -> Vec3 {
    let offset = turn.mul_vec3(Vec3::new(
        point.x - from.x,
        point.y - from.y,
        point.z - from.z,
    ));
    Vec3::new(to.x + offset.x, to.y + offset.y, to.z + offset.z)
}

/// Conservative offline bounds of cabin stock with the active doorway open.
/// Live leaves still decide whether an NPC may enter an occupied landing.
#[must_use]
pub fn navigation_obstacles(
    level: &crate::engine::level::LevelDef,
) -> Vec<crate::engine::collision::WallAabb> {
    let mut result = Vec::new();
    for def in &level.elevators {
        for (index, stop) in def.stops.iter().enumerate().filter(|(_, s)| s.is_landing()) {
            let mut lift = Elevator::new(def);
            lift.at = index;
            lift.y = stop.y;
            lift.opening = 1.0;
            lift.phase = Phase::Open;
            for collider in lift.colliders().into_iter().filter(|c| {
                c.hinge_y >= stop.y - 0.1 && c.hinge_y + c.height <= stop.y + HEIGHT + 0.1
            }) {
                let mut x0 = f32::INFINITY;
                let mut x1 = f32::NEG_INFINITY;
                let mut z0 = f32::INFINITY;
                let mut z1 = f32::NEG_INFINITY;
                for t in [0.0_f32, 1.0] {
                    for side in [-1.0_f32, 1.0] {
                        let (x, z) = collider.point_at(t, side);
                        x0 = x0.min(x);
                        x1 = x1.max(x);
                        z0 = z0.min(z);
                        z1 = z1.max(z);
                    }
                }
                result.push(crate::engine::collision::WallAabb::with_y(
                    x0,
                    collider.hinge_y,
                    z0,
                    x1 - x0,
                    collider.height,
                    z1 - z0,
                ));
            }
        }
    }
    result
}

impl CabinFrame {
    #[must_use]
    pub fn horizontal_bounds(&self) -> (f32, f32, f32, f32) {
        let turn = glam::Quat::from_rotation_y(self.rotation_degrees.to_radians());
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for x in [-1.2_f32, 1.2] {
            for z in [-0.2_f32, 2.4] {
                let point = map_cabin_point(Vec3::ZERO, self.origin, turn, Vec3::new(x, 0.0, z));
                minimum = minimum.min(point);
                maximum = maximum.max(point);
            }
        }
        (minimum.x, maximum.x, minimum.z, maximum.z)
    }
    #[must_use]
    pub fn from_elevator(elevator: &Elevator) -> Self {
        Self {
            origin: elevator.def.point(0.0, elevator.y, 0.0),
            rotation_degrees: elevator.def.rotation_degrees,
        }
    }
    #[must_use]
    pub fn turn_to(&self, target: &Self) -> glam::Quat {
        glam::Quat::from_rotation_y((target.rotation_degrees - self.rotation_degrees).to_radians())
    }
    #[must_use]
    pub fn local(&self, point: Vec3) -> Vec3 {
        map_cabin_point(
            self.origin,
            Vec3::ZERO,
            glam::Quat::from_rotation_y(-self.rotation_degrees.to_radians()),
            point,
        )
    }
    #[must_use]
    pub fn contains_bounds(&self, min: Vec3, max: Vec3) -> bool {
        [min.x, max.x].into_iter().all(|x| {
            [min.y, max.y].into_iter().all(|y| {
                [min.z, max.z].into_iter().all(|z| {
                    let p = self.local(Vec3::new(x, y, z));
                    p.x.abs() < 1.19 && p.z > 0.365 && p.z < 2.39 && p.y >= -0.025 && p.y < HEIGHT
                })
            })
        })
    }
}
#[derive(Default)]
pub struct Step {
    pub sound: Option<&'static str>,
    pub departure: Option<Departure>,
}

impl Elevator {
    #[must_use]
    pub fn new(def: &ElevatorDef) -> Self {
        let y = def.stops.get(def.initial_stop).map_or(0.0, |stop| stop.y);
        Self {
            def: def.clone(),
            y,
            opening: 0.0,
            phase: Phase::Closed,
            at: def.initial_stop,
            target: def.initial_stop,
            travel_elapsed: 0.0,
            obstructed: false,
            ambience: 8.0,
            ambience_count: 0,
            departure_sent: false,
        }
    }
    #[must_use]
    pub fn contains(&self, feet: Vec3, margin: f32) -> bool {
        let local = self.def.local(feet);
        local.x.abs() < WIDTH.mul_add(0.5, -margin)
            && local.z > 0.12 + margin
            && local.z < DEPTH - 0.1 - margin
            && (local.y - self.y).abs() < 0.25
    }
    pub fn request(&mut self, stop: usize, cabin: bool, feet: Option<Vec3>) -> bool {
        if matches!(
            self.phase,
            Phase::Closing | Phase::Moving | Phase::Loading | Phase::Opening
        ) || self.def.stops.get(stop).is_none()
            || (cabin && feet.is_none_or(|point| !self.contains(point, 0.25)))
            || (!cabin
                && self
                    .def
                    .stops
                    .get(stop)
                    .is_some_and(|sin| !sin.is_landing()))
        {
            return false;
        }
        self.target = stop;
        self.departure_sent = false;
        self.phase = if stop == self.at && self.def.stops.get(stop).is_some_and(Stop::is_landing) {
            Phase::Opening
        } else {
            Phase::Closing
        };
        true
    }
    /// Operate the doors at this landing without queuing a destination.
    /// Reopening cancels an unstarted trip; moving/loading cabins stay sealed.
    pub fn toggle_doors(&mut self, feet: Option<Vec3>) -> bool {
        if matches!(self.phase, Phase::Moving | Phase::Loading)
            || feet.is_none_or(|point| !self.contains(point, 0.25))
        {
            return false;
        }
        self.target = self.at;
        self.departure_sent = false;
        self.phase = match self.phase {
            Phase::Closed | Phase::Closing => Phase::Opening,
            Phase::Open | Phase::Opening => Phase::Closing,
            Phase::Moving | Phase::Loading => return false,
        };
        true
    }
    pub const fn reopen(&mut self) {
        self.target = self.at;
        self.departure_sent = false;
        self.phase = Phase::Opening;
    }
    pub const fn finish_departure(&mut self) {
        self.phase = Phase::Closed;
        self.opening = 0.0;
        self.target = self.at;
        self.departure_sent = false;
    }
    pub const fn arrive(&mut self) {
        self.opening = 0.0;
        self.reopen();
    }
    /// Select a fixed landing only at the sealed passenger-transfer boundary.
    pub fn arrive_at(&mut self, index: usize) -> bool {
        let Some(stop) = self.def.stops.get(index).filter(|stop| stop.is_landing()) else {
            return false;
        };
        self.y = stop.y;
        self.at = index;
        self.arrive();
        true
    }
    #[must_use]
    pub fn recovery_checkpoint(&self) -> Checkpoint {
        Checkpoint {
            id: self.def.id.clone(),
            stop: self.at,
            open: true,
        }
    }
    #[must_use]
    pub const fn settled(&self) -> bool {
        matches!(self.phase, Phase::Open | Phase::Closed)
    }
    /// # Errors
    /// Moving or loading lifts must reach a safe landing before saving.
    pub fn checkpoint(&self) -> Result<Checkpoint, String> {
        if !self.settled() {
            return Err(
                "Wait for the elevator to stop and its doors to settle before saving".into(),
            );
        }
        Ok(Checkpoint {
            id: self.def.id.clone(),
            stop: self.at,
            open: self.phase == Phase::Open,
        })
    }
    /// # Errors
    /// Rejects unknown identities and non-local landing records.
    pub fn restore(&mut self, record: &Checkpoint) -> Result<(), String> {
        let stop = self
            .def
            .stops
            .get(record.stop)
            .filter(|sin| sin.is_landing())
            .ok_or("Invalid elevator checkpoint landing")?;
        if record.id != self.def.id {
            return Err("Elevator checkpoint identity mismatch".into());
        }
        self.y = stop.y;
        self.at = record.stop;
        self.target = record.stop;
        self.opening = if record.open { 1.0 } else { 0.0 };
        self.phase = if record.open {
            Phase::Open
        } else {
            Phase::Closed
        };
        self.obstructed = false;
        self.departure_sent = false;
        Ok(())
    }
    fn leaf(&self, y: f32, side: f32, z: f32, opening: f32) -> DoorCollider {
        let x = door_leaf_x(side, opening) - DOOR_LEAF_SEAL;
        let hinge = self.def.point(
            x,
            y + DOOR_LEAF_BOTTOM,
            z + f32::midpoint(DOOR_LEAF_FRONT, DOOR_LEAF_BACK),
        );
        let (sin, cos) = self.def.rotation_degrees.to_radians().sin_cos();
        DoorCollider::from_pose(
            hinge.into(),
            [cos, -sin],
            DOOR_LEAF_WIDTH + DOOR_LEAF_SEAL,
            DOOR_LEAF_BACK - DOOR_LEAF_FRONT,
            DOOR_LEAF_TOP - DOOR_LEAF_BOTTOM,
        )
    }

    #[must_use]
    pub fn colliders(&self) -> Vec<DoorCollider> {
        let mut boxes = Vec::new();
        self.visit_colliders(|collider| boxes.push(collider));
        boxes
    }
    /// Visits the authoritative fixed-cabin and sliding-leaf poses without allocating.
    pub fn visit_colliders(&self, mut visit: impl FnMut(DoorCollider)) {
        for (landing, stop) in self
            .def
            .stops
            .iter()
            .enumerate()
            .filter(|(_, stop)| stop.is_landing())
        {
            self.visit_cabin_colliders(
                stop.y,
                if landing == self.at {
                    self.opening
                } else {
                    0.0
                },
                &mut visit,
            );
        }
        for (i, stop) in self
            .def
            .stops
            .iter()
            .enumerate()
            .filter(|(_, sin)| sin.is_landing())
        {
            for (x0, x1) in [(-0.795, -0.715), (0.715, 0.795)] {
                visit(self.box_collider_at(stop.y, [x0, -0.004, -0.17], [x1, 2.28, -0.105]));
            }
            for (x0, x1) in [(-1.425, -0.675), (0.675, 1.425)] {
                visit(self.box_collider_at(stop.y, [x0, -0.012, 0.195], [x1, 2.23, 0.225]));
            }
            for (x0, x1) in [(-0.79, -0.710), (0.710, 0.79)] {
                visit(self.box_collider_at(stop.y, [x0, -0.012, -0.105], [x1, 2.23, 0.025]));
            }
            visit(self.box_collider_at(stop.y, [-0.715, DOOR_HEIGHT, -0.15], [0.715, 2.29, 0.24]));
            let opening = if i == self.at && self.phase != Phase::Moving {
                self.opening
            } else {
                0.0
            };
            for side in [-1.0, 1.0] {
                visit(self.leaf(stop.y, side, LANDING_DOOR_Z, opening));
            }
        }
    }
    fn visit_cabin_colliders(&self, y: f32, opening: f32, visit: &mut impl FnMut(DoorCollider)) {
        for collider in [
            self.box_collider_at(y, [-1.2, -0.08, 0.0], [1.2, 0.0, 2.4]),
            self.box_collider_at(y, [-1.2, HEIGHT, 0.0], [1.2, HEIGHT + 0.08, 2.4]),
            self.box_collider_at(y, [-1.28, 0.0, 0.0], [-1.2, HEIGHT, 2.48]),
            self.box_collider_at(y, [1.2, 0.0, 0.0], [1.28, HEIGHT, 2.48]),
            self.box_collider_at(y, [-1.2, 0.0, 2.4], [1.2, HEIGHT, 2.48]),
            self.box_collider_at(y, [-1.2, 0.0, 0.02], [-0.68, HEIGHT, 0.405]),
            self.box_collider_at(y, [0.68, 0.0, 0.02], [1.2, HEIGHT, 0.405]),
            self.box_collider_at(y, [-0.68, DOOR_HEIGHT, 0.02], [0.68, HEIGHT, 0.405]),
            self.box_collider_at(y, [-1.14, 0.95, 0.55], [-1.06, 1.03, 2.25]),
            self.box_collider_at(y, [1.06, 0.95, 0.55], [1.14, 1.03, 2.25]),
            self.box_collider_at(y, [-1.11, 0.95, 2.26], [1.11, 1.03, 2.34]),
        ] {
            visit(collider);
        }
        visit(self.box_collider_at(y, [-1.45, 0.0, 0.02], [-1.2, HEIGHT, 0.36]));
        visit(self.box_collider_at(y, [1.2, 0.0, 0.02], [1.45, HEIGHT, 0.36]));
        for side in [-1.0, 1.0] {
            visit(self.leaf(y, side, CABIN_DOOR_Z, opening));
        }
    }
    fn box_collider_at(&self, y: f32, min: [f32; 3], max: [f32; 3]) -> DoorCollider {
        let hinge = self
            .def
            .point(min[0], y + min[1], f32::midpoint(min[2], max[2]));
        let (sin, cos) = self.def.rotation_degrees.to_radians().sin_cos();
        DoorCollider::from_pose(
            hinge.into(),
            [cos, -sin],
            max[0] - min[0],
            max[2] - min[2],
            max[1] - min[1],
        )
    }
    #[must_use]
    pub fn controls(&self) -> Vec<Interactable> {
        let mut controls = Vec::new();
        let mut row = 0_u16;
        for (i, stop) in self.def.stops.iter().enumerate() {
            if i != self.at {
                let row_height = f32::from(row);
                controls.push(self.control(
                    i,
                    true,
                    [0.94, row_height.mul_add(-0.11, self.y + 1.58), 0.40],
                    &format!("Floor {}", stop.label),
                ));
                row = row.saturating_add(1);
            }
            if stop.is_landing() {
                controls.push(self.control(
                    i,
                    false,
                    [-0.96, stop.y + 1.08, -0.12],
                    &format!("Call elevator / {}", stop.label),
                ));
            }
        }
        let prompt = match self.phase {
            Phase::Closed | Phase::Closing => "Open doors",
            Phase::Open | Phase::Opening => "Close doors",
            Phase::Moving | Phase::Loading => "Doors locked during travel",
        };
        let mut doors = self.control(
            self.at,
            true,
            [0.94, self.y + door_button_height(usize::from(row)), 0.40],
            prompt,
        );
        doors.id = format!("@elevator:{}:door", self.def.id);
        controls.push(doors);
        controls
    }
    fn control(&self, i: usize, cabin: bool, local: [f32; 3], prompt: &str) -> Interactable {
        let position = self.def.point(local[0], local[1], local[2]);
        let half = Vec3::new(0.09, 0.045, 0.04);
        let extent =
            crate::engine::spatial::rotated_half_extents(half.x, half.z, self.def.rotation_degrees);
        let bound = Vec3::new(extent.0, half.y, extent.1);
        Interactable {
            id: format!(
                "@elevator:{}:{}:{i}",
                self.def.id,
                if cabin { "floor" } else { "call" }
            ),
            display_name: prompt.into(),
            prompt: prompt.into(),
            reach: 2.0,
            anchor: position,
            bounds: Aabb {
                min: [
                    position.x - bound.x,
                    position.y - bound.y,
                    position.z - bound.z,
                ],
                max: [
                    position.x + bound.x,
                    position.y + bound.y,
                    position.z + bound.z,
                ],
            },
            yaw_degrees: self.def.rotation_degrees,
            size: [half.x * 2.0, half.y * 2.0, half.z * 2.0],
            own_box: None,
            door_index: None,
            leaf_range: None,
            body_range: None,
            enabled: true,
        }
    }
    fn advance_loading(&mut self, delta: f32, feet: Vec3) -> Option<Departure> {
        self.travel_elapsed += delta;
        if !self.contains(feet, 0.25) {
            self.reopen();
            return None;
        }
        let stop = self.def.stops.get(self.target)?;
        if self.departure_sent || self.travel_elapsed < RIDE_SECONDS {
            return None;
        }
        self.departure_sent = true;
        Some(Departure {
            level: stop.level.clone(),
            source: self.def.id.clone(),
            landing: None,
            arrival: stop
                .arrival_elevator
                .clone()
                .unwrap_or_else(|| self.def.id.clone()),
        })
    }
    /// Fixed-cabin ride presentation and swept leaf closure; no car translation.
    pub fn advance(
        &mut self,
        elapsed: f32,
        feet: Vec3,
        mut blocked: impl FnMut(&DoorCollider) -> bool,
    ) -> Step {
        let mut result = Step::default();
        let delta = if elapsed.is_finite() {
            elapsed.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.obstructed = false;
        match self.phase {
            Phase::Opening => {
                self.opening = (self.opening + delta / 1.4).min(1.0);
                if self.opening >= 1.0 {
                    self.phase = Phase::Open;
                }
            }
            Phase::Closing => {
                let proposed = (self.opening - delta / 1.4).max(0.0);
                let mut obstructed = false;
                for side in [-1.0, 1.0] {
                    for z in [LANDING_DOOR_Z, CABIN_DOOR_Z] {
                        obstructed |= blocked(&self.leaf(self.y, side, z, proposed));
                    }
                }
                let local = self.def.local(feet);
                obstructed |= (local.y - self.y).abs() < 1.8
                    && local.x.abs() < DOOR_WIDTH.mul_add(0.5, 0.3)
                    && local.z.abs() < 0.45;
                if obstructed {
                    self.obstructed = true;
                    self.reopen();
                } else {
                    self.opening = proposed;
                    if proposed <= 0.0 {
                        if self.target == self.at {
                            self.phase = Phase::Closed;
                        } else if self
                            .def
                            .stops
                            .get(self.target)
                            .is_some_and(|stop| !stop.is_landing())
                        {
                            self.phase = Phase::Loading;
                            self.travel_elapsed = 0.0;
                        } else {
                            self.travel_elapsed = 0.0;
                            self.phase = Phase::Moving;
                        }
                    }
                }
            }
            Phase::Moving => {
                self.travel_elapsed += delta;
                if self.travel_elapsed >= RIDE_SECONDS && !self.departure_sent {
                    if self.contains(feet, 0.0) {
                        self.departure_sent = true;
                        result.departure = Some(Departure {
                            level: None,
                            arrival: self.def.id.clone(),
                            source: self.def.id.clone(),
                            landing: Some(self.target),
                        });
                    } else if CabinFrame::from_elevator(self).contains_bounds(feet, feet) {
                        // An airborne rider is still inside the source cabin.
                        // Preserve it and reopen, matching linked-trip admission;
                        // treating this as a hall call strands its passengers.
                        self.reopen();
                    } else {
                        // A hall call opens its fixed cabin; nothing is carried.
                        let _arrived = self.arrive_at(self.target);
                        result.sound = Some("elevator_arrive");
                    }
                }
            }
            Phase::Loading => result.departure = self.advance_loading(delta, feet),
            Phase::Closed | Phase::Open => {}
        }
        self.ambience -= delta;
        if self.ambience <= 0.0 {
            self.ambience_count = self.ambience_count.wrapping_add(1);
            self.ambience =
                f32::from(u16::try_from(self.ambience_count % 5).unwrap_or(0)).mul_add(3.7, 14.0);
            if self.def.local(feet).xz().length() < 10.0 && self.phase != Phase::Moving {
                result.sound = Some("elevator_shaft");
            }
        }
        result
    }
}
use glam::Vec3Swizzles;

#[must_use]
pub fn diagnostic(elevators: &[Elevator]) -> serde_json::Value {
    elevators.iter().map(|e| serde_json::json!({"id":e.def.id,"y":e.y,"opening":e.opening,"phase":e.phase,"stop":e.at,"target":e.target,"obstructed":e.obstructed})).collect()
}

#[must_use]
pub fn cabin_support(elevators: &[Elevator], point: Vec2, max_y: f32) -> Option<f32> {
    elevators
        .iter()
        .flat_map(|e| {
            e.def
                .stops
                .iter()
                .filter(|stop| stop.is_landing())
                .filter_map(move |stop| {
                    let local = e.def.local(Vec3::new(point.x, stop.y, point.y));
                    (local.x.abs() < WIDTH * 0.5
                        && local.z > 0.0
                        && local.z < DEPTH
                        && stop.y <= max_y)
                        .then_some(stop.y)
                })
        })
        .max_by(f32::total_cmp)
}

fn validate_shafts(level: &crate::engine::level::LevelDef) -> Result<(), String> {
    if level.elevators.is_empty() {
        return Ok(());
    }
    let walls = level.collision_aabbs();
    let floor = crate::engine::level::WalkableFloor::from_level(level);
    let ceiling = crate::engine::level::WalkableCeiling::from_level(level);
    let mut objects = level.doors.len().saturating_mul(2);
    for def in &level.elevators {
        let local: Vec<_> = def.stops.iter().filter(|stop| stop.is_landing()).collect();
        objects = objects.saturating_add(local.len().saturating_mul(6));
        if objects > 128 {
            return Err(
                "Elevators and hinged doors together exceed the reserved 128 dynamic objects"
                    .into(),
            );
        }
        let low = local
            .iter()
            .map(|s| s.y)
            .min_by(f32::total_cmp)
            .ok_or("Elevator has no local landing")?;
        let high = local
            .iter()
            .map(|s| s.y)
            .max_by(f32::total_cmp)
            .ok_or("Elevator has no local landing")?;
        let (sin, cos) = def.rotation_degrees.to_radians().sin_cos();
        let start = def.point(-1.19, low + 0.01, 1.32);
        let shaft = DoorCollider::from_pose(
            start.into(),
            [cos, -sin],
            2.38,
            2.14,
            high - low + HEIGHT - 0.02,
        );
        if walls
            .iter()
            .any(|wall| crate::engine::entities::door_pose_hits_box(&shaft, wall))
        {
            return Err(format!(
                "Elevator {}: cabin travel crosses static geometry; use a clear side shaft",
                def.id
            ));
        }
        for x in [-1.15, 0.0, 1.15] {
            for z in [0.3, 1.2, 2.3] {
                let point = def.point(x, low, z);
                let sample = Vec2::new(point.x, point.z);
                let mut blocked = false;
                ceiling.for_each_hull_contact(sample, 0.04, low, high - low + HEIGHT, |_| {
                    blocked = true;
                });
                blocked |= floor
                    .hull_support(sample, 0.04, high + HEIGHT - 0.01)
                    .is_some_and(|(y, _)| y > low + 0.01);
                if blocked {
                    return Err(format!(
                        "Elevator {}: a room floor or ceiling crosses the shaft; leave the cabin outside stacked room footprints",
                        def.id
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The same eased door pose is sampled by the collider and visible leaves.
#[must_use]
pub fn slide_fraction(progress: f32) -> f32 {
    let bounded = progress.clamp(0.0, 1.0);
    bounded * bounded * 2.0_f32.mul_add(-bounded, 3.0)
}

/// Shared visible/collision slide origin, including concealed jamb overlap.
#[must_use]
pub fn door_leaf_x(side: f32, opening: f32) -> f32 {
    let fraction = slide_fraction(opening);
    if side < 0.0 {
        -DOOR_LEAF_TRAVEL.mul_add(fraction, DOOR_LEAF_WIDTH)
    } else {
        DOOR_LEAF_TRAVEL * fraction
    }
}

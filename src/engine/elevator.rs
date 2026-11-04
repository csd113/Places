//! Authored passenger lifts. One pose owns cabin, sliding leaves and collision.
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::engine::collision::DoorCollider;
use crate::engine::interact::Interactable;
use crate::engine::spatial::Aabb;

pub const WIDTH: f32 = 2.4;
pub const DEPTH: f32 = 2.4;
pub const HEIGHT: f32 = 2.55;
pub const DOOR_WIDTH: f32 = 1.35;
pub const DOOR_HEIGHT: f32 = 2.2;
// Both pairs slide inside the authored 0..0.2 m landing wall, not across its hall face.
pub const CABIN_DOOR_Z: f32 = 0.07;
pub const LANDING_DOOR_Z: f32 = 0.15;
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
    if level.elevators.len() > 8 {
        return Err("At most 8 elevators per level".into());
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
            || !(1..=8).contains(&def.stops.len())
            || def
                .stops
                .get(def.initial_stop)
                .is_none_or(|stop| stop.level.is_some())
        {
            return Err(format!(
                "Elevator {}: invalid position, speed, stops or initial landing",
                def.id
            ));
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
                    .is_some_and(|id| id.is_empty() || id.len() > 64 || stop.level.is_none())
            {
                return Err(format!(
                    "Elevator {}: invalid stop {} or undeclared destination",
                    def.id,
                    i.saturating_add(1)
                ));
            }
            if stop.level.is_none()
                && def
                    .stops
                    .iter()
                    .take(i)
                    .any(|other| other.level.is_none() && (other.y - stop.y).abs() < HEIGHT + 0.2)
            {
                return Err(format!(
                    "Elevator {}: local landings need at least 2.75 m separation",
                    def.id
                ));
            }
        }
    }
    validate_shafts(level)
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
    travel_start: f32,
    travel_time: f32,
    travel_elapsed: f32,
    pub obstructed: bool,
    ambience: f32,
    ambience_count: u32,
    departure_sent: bool,
}

#[derive(Clone, Debug)]
pub struct Departure {
    pub level: String,
    pub arrival: String,
}
#[derive(Default)]
pub struct Step {
    pub carry_y: f32,
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
            travel_start: y,
            travel_time: 0.0,
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
                    .is_some_and(|sin| sin.level.is_some()))
        {
            return false;
        }
        self.target = stop;
        self.departure_sent = false;
        self.phase = if stop == self.at
            && self
                .def
                .stops
                .get(stop)
                .is_some_and(|sin| sin.level.is_none())
        {
            Phase::Opening
        } else {
            Phase::Closing
        };
        true
    }
    pub const fn reopen(&mut self) {
        self.target = self.at;
        self.departure_sent = false;
        self.phase = Phase::Opening;
    }
    pub const fn arrive(&mut self) {
        self.opening = 0.0;
        self.reopen();
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
            .filter(|sin| sin.level.is_none())
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
        let half = DOOR_WIDTH * 0.5;
        let fraction = slide_fraction(opening);
        let x = if side < 0.0 {
            half.mul_add(-fraction, -half)
        } else {
            half.mul_add(fraction, 0.003)
        };
        let hinge = self.def.point(x, y, z);
        let (sin, cos) = self.def.rotation_degrees.to_radians().sin_cos();
        DoorCollider::from_pose(hinge.into(), [cos, -sin], half - 0.003, 0.045, DOOR_HEIGHT)
    }
    #[must_use]
    pub fn colliders(&self) -> Vec<DoorCollider> {
        let mut boxes = vec![
            self.box_collider([-1.2, -0.08, 0.0], [1.2, 0.0, 2.4]),
            self.box_collider([-1.2, HEIGHT, 0.0], [1.2, HEIGHT + 0.08, 2.4]),
            self.box_collider([-1.28, 0.0, 0.0], [-1.2, HEIGHT, 2.48]),
            self.box_collider([1.2, 0.0, 0.0], [1.28, HEIGHT, 2.48]),
            self.box_collider([-1.2, 0.0, 2.4], [1.2, HEIGHT, 2.48]),
            self.box_collider([-1.2, 0.0, 0.02], [-0.675, HEIGHT, 0.26]),
            self.box_collider([0.675, 0.0, 0.02], [1.2, HEIGHT, 0.26]),
            self.box_collider([-0.675, DOOR_HEIGHT, 0.02], [0.675, HEIGHT, 0.26]),
            self.box_collider([-1.14, 0.95, 0.55], [-1.06, 1.03, 2.25]),
            self.box_collider([1.06, 0.95, 0.55], [1.14, 1.03, 2.25]),
            self.box_collider([-1.11, 0.95, 2.26], [1.11, 1.03, 2.34]),
        ];
        for side in [-1.0, 1.0] {
            boxes.push(self.leaf(self.y, side, CABIN_DOOR_Z, self.opening));
        }
        for (i, stop) in self
            .def
            .stops
            .iter()
            .enumerate()
            .filter(|(_, sin)| sin.level.is_none())
        {
            let opening = if i == self.at && self.phase != Phase::Moving {
                self.opening
            } else {
                0.0
            };
            for side in [-1.0, 1.0] {
                boxes.push(self.leaf(stop.y, side, LANDING_DOOR_Z, opening));
            }
        }
        boxes
    }
    fn box_collider(&self, min: [f32; 3], max: [f32; 3]) -> DoorCollider {
        let hinge = self
            .def
            .point(min[0], self.y + min[1], f32::midpoint(min[2], max[2]));
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
        for (i, stop) in self.def.stops.iter().enumerate() {
            let row = f32::from(u16::try_from(i).unwrap_or(0));
            controls.push(self.control(
                i,
                true,
                [0.94, row.mul_add(-0.11, self.y + 1.58), 0.30],
                &format!("Floor {}", stop.label),
            ));
            if stop.level.is_none() {
                controls.push(self.control(
                    i,
                    false,
                    [-0.96, stop.y + 1.08, -0.12],
                    &format!("Call elevator / {}", stop.label),
                ));
            }
        }
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
            enabled: true,
        }
    }
    /// Continuous smoothstep ride and swept leaf closure; capped steps prevent crushing.
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
                        if self
                            .def
                            .stops
                            .get(self.target)
                            .is_some_and(|stop| stop.level.is_some())
                        {
                            self.phase = Phase::Loading;
                        } else {
                            self.travel_start = self.y;
                            self.travel_elapsed = 0.0;
                            self.travel_time = self.def.stops.get(self.target).map_or(1.0, |sin| {
                                ((sin.y - self.y).abs() / self.def.speed_mps * 1.5).max(1.0)
                            });
                            self.phase = Phase::Moving;
                            result.sound = Some("elevator_shaft");
                        }
                    }
                }
            }
            Phase::Moving => {
                self.travel_elapsed = (self.travel_elapsed + delta).min(self.travel_time);
                let t = self.travel_elapsed / self.travel_time;
                let smooth = t * t * (2.0_f32.mul_add(-t, 3.0));
                let old = self.y;
                let riding = self.contains(feet, 0.0);
                self.y = self.def.stops.get(self.target).map_or(old, |stop| {
                    (stop.y - self.travel_start).mul_add(smooth, self.travel_start)
                });
                if riding {
                    result.carry_y = self.y - old;
                }
                if t >= 1.0 {
                    self.at = self.target;
                    self.phase = Phase::Opening;
                    result.sound = Some("elevator_arrive");
                }
            }
            Phase::Loading => {
                if !self.departure_sent {
                    if let Some(stop) = self.def.stops.get(self.target)
                        && let Some(level) = &stop.level
                    {
                        result.departure = Some(Departure {
                            level: level.clone(),
                            arrival: stop
                                .arrival_elevator
                                .clone()
                                .unwrap_or_else(|| self.def.id.clone()),
                        });
                    }
                    self.departure_sent = true;
                }
            }
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
        .filter_map(|e| {
            let local = e.def.local(Vec3::new(point.x, e.y, point.y));
            (local.x.abs() < WIDTH * 0.5 && local.z > 0.0 && local.z < DEPTH && e.y <= max_y)
                .then_some(e.y)
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
        let local: Vec<_> = def
            .stops
            .iter()
            .filter(|stop| stop.level.is_none())
            .collect();
        objects = objects.saturating_add(3_usize.saturating_add(local.len().saturating_mul(3)));
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

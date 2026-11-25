//! Escalator placement, support lane and authoritative tread clock.

use glam::{Vec2, Vec3};

use crate::engine::entity::PoseCue;
use crate::engine::level::{
    ComponentDef, LevelDef, LevelSurfaces, PropDef, RampDef, RampSurface, RoomDef,
};

pub const MODEL: &str = "mall:escalator";
pub const CLIP: &str = "escalator_run";
const HALF_WIDTH: f32 = 0.6;
const RISE_START: f32 = 0.6;
const DEFAULT_RISE_END: f32 = 6.6;
const DEFAULT_BELT_END: f32 = 7.2;
const DEFAULT_RISE: f32 = 3.0;
const MIN_RISE: f32 = 0.75;
const MAX_RISE: f32 = 12.0;
const PITCH_M: f32 = 0.3;
const SPEED_MPS: f32 = 0.6;
const GRADE: f32 = 0.5;
pub(crate) const SUPPORT_EPS: f32 = 0.0001;
const APPROACH_M: f32 = 0.45;
const BOARDING_CLEARANCE: f32 = 2.0;
const SHELL_HALF_WIDTH: f32 = 0.718;
pub(crate) const SHELL_DEPTH: f32 = 0.285;
const AXES: [(Vec2, f32); 4] = [
    (Vec2::Y, 0.0),
    (Vec2::X, 90.0),
    (Vec2::NEG_Y, 180.0),
    (Vec2::NEG_X, 270.0),
];

mod rig;
pub(crate) use rig::fit_model;

/// A common footprint and tread layout for the model, collision and clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dimensions {
    pub(crate) rise: f32,
    rise_end: f32,
    pub(crate) length: f32,
    pub(crate) pitch: f32,
    count: u16,
}

impl Dimensions {
    pub(crate) fn new(rise: f32) -> Result<Self, String> {
        if !(MIN_RISE..=MAX_RISE).contains(&rise) {
            return Err(format!("Escalator rise must be {MIN_RISE}–{MAX_RISE} m"));
        }
        let (rise_end, length) = if is_default_rise(&rise) {
            (DEFAULT_RISE_END, DEFAULT_BELT_END)
        } else {
            (
                rise.mul_add(2.0, RISE_START),
                rise.mul_add(2.0, 2.0 * RISE_START),
            )
        };
        let count = (1_u16..=84)
            .find(|count| f32::from(*count).mul_add(PITCH_M, SUPPORT_EPS) >= length)
            .ok_or("Escalator exceeds the tread budget")?;
        Ok(Self {
            rise,
            rise_end,
            length,
            pitch: length / f32::from(count),
            count,
        })
    }

    pub(crate) fn from_length(length: f32) -> Result<Self, String> {
        Self::new(2.0_f32.mul_add(-RISE_START, length) * GRADE).map_err(|_rise_error| {
            "Escalator length must be 2.7–25.2 m, including both integral landings".into()
        })
    }

    fn height(self, along: f32) -> f32 {
        if along >= self.rise_end {
            self.rise
        } else {
            (along - RISE_START).max(0.0) * GRADE
        }
    }
}

const fn default_rise() -> f32 {
    DEFAULT_RISE
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "Serde skip predicates take references."
)]
const fn is_default_rise(rise: &f32) -> bool {
    rise.to_bits() == DEFAULT_RISE.to_bits()
}

#[derive(Clone, Copy)]
struct PlanBounds(f32, f32, f32, f32);

impl PlanBounds {
    fn contains(self, other: Self) -> bool {
        self.0 <= other.0 + SUPPORT_EPS
            && self.1 >= other.1 - SUPPORT_EPS
            && self.2 <= other.2 + SUPPORT_EPS
            && self.3 >= other.3 - SUPPORT_EPS
    }

    fn overlaps(self, other: Self) -> bool {
        self.1.min(other.1) - self.0.max(other.0) > SUPPORT_EPS
            && self.3.min(other.3) - self.2.max(other.2) > SUPPORT_EPS
    }

    fn intersection(self, other: Self) -> Option<Self> {
        self.overlaps(other).then(|| {
            Self(
                self.0.max(other.0),
                self.1.min(other.1),
                self.2.max(other.2),
                self.3.min(other.3),
            )
        })
    }

    fn subtract(self, cut: Self, remaining: &mut Vec<Self>) {
        if !self.overlaps(cut) {
            remaining.push(self);
            return;
        }
        let x0 = self.0.max(cut.0);
        let x1 = self.1.min(cut.1);
        let z0 = self.2.max(cut.2);
        let z1 = self.3.min(cut.3);
        remaining.extend(
            [
                Self(self.0, x0, self.2, self.3),
                Self(x1, self.1, self.2, self.3),
                Self(x0, x1, self.2, z0),
                Self(x0, x1, z1, self.3),
            ]
            .into_iter()
            .filter(|fragment| {
                fragment.1 - fragment.0 > SUPPORT_EPS && fragment.3 - fragment.2 > SUPPORT_EPS
            }),
        );
    }
}

impl From<(f32, f32, f32, f32)> for PlanBounds {
    fn from((x0, x1, z0, z1): (f32, f32, f32, f32)) -> Self {
        Self(x0, x1, z0, z1)
    }
}

/// Direction of travel on the fixed rig, independently of its placement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Up,
    Down,
}

impl Direction {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "Serde's skip predicate must accept a reference."
    )]
    const fn is_up(&self) -> bool {
        matches!(self, Self::Up)
    }

    const fn speed(self) -> f32 {
        match self {
            Self::Up => SPEED_MPS,
            Self::Down => -SPEED_MPS,
        }
    }
}

/// Connected belts require matching building platforms; a free belt has only
/// its own integral end plates and never claims an architectural floor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    #[default]
    Legacy,
    Connected,
    Free,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "Serde skip predicates take references."
)]
const fn is_legacy(placement: &Placement) -> bool {
    matches!(placement, Placement::Legacy)
}

/// Presence installs the fixed tread lane and specifies its travel and power.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    #[serde(default)]
    pub initial_running: bool,
    #[serde(default, skip_serializing_if = "Direction::is_up")]
    pub direction: Direction,
    #[serde(default = "default_rise", skip_serializing_if = "is_default_rise")]
    pub rise: f32,
    #[serde(default, skip_serializing_if = "is_legacy")]
    pub placement: Placement,
}

impl Definition {
    pub(crate) const fn custom_rise(&self) -> bool {
        !is_default_rise(&self.rise)
    }

    pub(crate) const fn requires_fit_capability(&self) -> bool {
        self.custom_rise()
            || !is_legacy(&self.placement)
            || matches!(self.direction, Direction::Down)
    }
}

pub(crate) const CAPABILITY: &str = "escalator-fit-v1";

pub(crate) fn requires_fit_capability(level: &LevelDef) -> bool {
    level
        .props
        .iter()
        .filter_map(|prop| prop.escalator.as_ref())
        .any(Definition::requires_fit_capability)
}

/// World-space pitch line under the moving visual steps. The footplate
/// endpoints share the actual lower and upper landing heights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lane {
    origin: Vec3,
    along: Vec2,
    direction: Direction,
    dimensions: Dimensions,
    placement: Placement,
    upper_room: Option<usize>,
}

#[derive(Clone, Copy)]
pub(crate) struct SupportStrip {
    pub(crate) room: usize,
    pub(crate) surface: RampSurface,
    pub(crate) overlay: bool,
}

impl Lane {
    #[must_use]
    pub fn new(prop: &PropDef, base_y: f32) -> Option<Self> {
        let definition = prop.escalator.as_ref()?;
        let yaw = prop.rotation_degrees.rem_euclid(360.0);
        let along = AXES
            .into_iter()
            .find_map(|(axis, degrees)| ((yaw - degrees).abs() < 1.0e-4).then_some(axis))?;
        Some(Self {
            origin: Vec3::new(prop.x, base_y, prop.z),
            along,
            direction: definition.direction,
            dimensions: Dimensions::new(definition.rise).ok()?,
            placement: definition.placement,
            upper_room: None,
        })
    }

    fn plan(self, along: f32, across: f32) -> Vec2 {
        Vec2::new(
            self.along
                .y
                .mul_add(across, self.along.x.mul_add(along, self.origin.x)),
            (-self.along.x).mul_add(across, self.along.y.mul_add(along, self.origin.z)),
        )
    }

    fn bounds(self, start: f32, end: f32) -> PlanBounds {
        self.plan_bounds(start, end, HALF_WIDTH)
    }

    fn plan_bounds(self, start: f32, end: f32, half_width: f32) -> PlanBounds {
        let a = self.plan(start, -half_width);
        let b = self.plan(end, half_width);
        PlanBounds(a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y))
    }

    fn clear_of(self, bounds: crate::engine::collision::WallAabb) -> bool {
        let apron = if self.is_free() { 0.0 } else { APPROACH_M };
        let envelope = self.plan_bounds(-apron, self.dimensions.length + apron, SHELL_HALF_WIDTH);
        envelope
            .intersection(PlanBounds(
                bounds.min_x,
                bounds.max_x,
                bounds.min_z,
                bounds.max_z,
            ))
            .is_none_or(|rect| self.clear_vertical_band(rect, bounds.min_y, bounds.max_y))
    }

    fn clear_vertical_band(self, rect: PlanBounds, low_y: f32, high_y: f32) -> bool {
        let PlanBounds(x0, x1, z0, z1) = rect;
        let a = self.coordinates(Vec3::new(x0, 0.0, z0)).0;
        let b = self.coordinates(Vec3::new(x1, 0.0, z1)).0;
        high_y <= self.height(a.min(b)) - SHELL_DEPTH + SUPPORT_EPS
            || low_y + SUPPORT_EPS >= self.height(a.max(b)) + BOARDING_CLEARANCE
    }

    #[must_use]
    pub(crate) fn support_strips(
        self,
        level: &LevelDef,
        owner: usize,
    ) -> Option<[SupportStrip; 3]> {
        let host = level.rooms.get(owner)?;
        let upper = self.upper_room.unwrap_or(owner);
        let above_floor = self.is_free() || self.upper_room.is_some();
        let strip = |room, room_y, start, end, overlay| SupportStrip {
            room,
            surface: self.strip(room_y, start, end),
            overlay,
        };
        Some([
            strip(
                owner,
                host.floor_y,
                0.0,
                RISE_START,
                above_floor && !self.foot_shares_floor(level, host),
            ),
            strip(
                owner,
                host.floor_y,
                RISE_START,
                self.dimensions.rise_end,
                above_floor,
            ),
            strip(
                upper,
                level.rooms.get(upper)?.floor_y,
                self.dimensions.rise_end,
                self.dimensions.length,
                self.is_free(),
            ),
        ])
    }

    pub(crate) fn cuts_deck_in(self, room: usize, owner: usize) -> bool {
        !self.is_free() && self.upper_room.unwrap_or(owner) == room
    }

    fn attach_upper_room(mut self, level: &LevelDef, owner: usize) -> Self {
        if self.placement != Placement::Connected {
            return self;
        }
        let upper_y = self.origin.y + self.dimensions.rise;
        let upper_plate = self.bounds(
            self.dimensions.rise_end,
            self.dimensions.length + APPROACH_M,
        );
        if level
            .rooms
            .get(owner)
            .is_some_and(|host| flat_landing(level, host, upper_plate, upper_y))
        {
            return self;
        }
        let incline = self.plan_bounds(0.0, self.dimensions.rise_end, SHELL_HALF_WIDTH);
        self.upper_room = level.rooms.iter().enumerate().find_map(|(index, room)| {
            (index != owner
                && (room.floor_y - upper_y).abs() <= SUPPORT_EPS
                && !incline.overlaps(room.bounds().into())
                && PlanBounds::from(room.bounds()).contains(upper_plate)
                && flat_landing(level, room, upper_plate, upper_y))
            .then_some(index)
        });
        self
    }

    fn strip(self, room_y: f32, start: f32, end: f32) -> RampSurface {
        let PlanBounds(x, end_x, z, end_z) = self.bounds(start, end);
        let forward = self.along.x + self.along.y > 0.0;
        let (from, to) = if forward { (start, end) } else { (end, start) };
        let base = self.height(from) - room_y;
        RampDef {
            x,
            z,
            width: end_x - x,
            depth: end_z - z,
            offset_y: base,
            rise: self.height(to) - self.height(from),
            material: None,
            shine: None,
            edge_material: None,
            edge_shine: None,
        }
        .surface()
    }

    fn coordinates(self, point: Vec3) -> (f32, f32) {
        let x = point.x - self.origin.x;
        let z = point.z - self.origin.z;
        (
            self.along.x.mul_add(x, self.along.y * z),
            self.along.y.mul_add(x, -self.along.x * z),
        )
    }

    fn height(self, along: f32) -> f32 {
        self.origin.y + self.dimensions.height(along)
    }

    /// A selected support can still straddle a side or the terminal comb.
    /// Drive actors only while their centre remains aboard.
    #[must_use]
    pub fn contains(self, feet: Vec3) -> bool {
        let (along, across) = self.coordinates(feet);
        self.on_belt(along, 0.0) && across.abs() < HALF_WIDTH
    }

    // Include the boarding comb and exclude the dismount comb. For contact
    // anchors only, allow roundoff at the boarding edge but not past the exit.
    fn on_belt(self, along: f32, boarding_eps: f32) -> bool {
        match self.direction {
            Direction::Up => (-boarding_eps..self.dimensions.length).contains(&along),
            Direction::Down => along > 0.0 && along <= self.dimensions.length + boarding_eps,
        }
    }

    #[must_use]
    pub const fn planar_velocity(self) -> Vec2 {
        let speed = self.direction.speed();
        Vec2::new(self.along.x * speed, self.along.y * speed)
    }

    /// Only the approach-side rail post is a manual control target. The
    /// model's full interaction box would swallow aim at riders and pickups.
    pub(crate) fn control_proxy(self) -> (crate::engine::spatial::Aabb, [f32; 3]) {
        let center = self.plan(0.29, 0.75);
        let side = 0.12;
        let bottom = self.origin.y + 0.55;
        let height = 0.53;
        (
            crate::engine::spatial::Aabb {
                min: [center.x - side, bottom, center.y - side],
                max: [center.x + side, bottom + height, center.y + side],
            },
            [2.0 * side, height, 2.0 * side],
        )
    }

    /// Bounds used to split the floor grid exactly at its lane edges.
    pub(crate) fn floor_cuts(self) -> (f32, f32, f32, f32) {
        let PlanBounds(x0, x1, z0, z1) =
            self.bounds(self.dimensions.rise_end, self.dimensions.length);
        (x0, x1, z0, z1)
    }

    pub(crate) fn support_height_at(self, x: f32, z: f32) -> Option<f32> {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        ((0.0..=self.dimensions.length).contains(&along) && across.abs() <= HALF_WIDTH)
            .then(|| self.height(along))
    }

    /// The metal landing plate owns the top flat; the overlapping floor tile
    /// is structural support, not another render surface at the same height.
    pub(crate) fn covers_upper_plate(self, x: f32, z: f32) -> bool {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        (self.dimensions.rise_end..self.dimensions.length).contains(&along)
            && across.abs() < HALF_WIDTH
    }

    pub(crate) fn covers_crest_face(self, x: f32, z: f32) -> bool {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        (along - self.dimensions.rise_end).abs() <= SUPPORT_EPS && across.abs() < HALF_WIDTH
    }

    pub(crate) const fn is_free(self) -> bool {
        matches!(self.placement, Placement::Free)
    }

    pub(crate) fn foot_shares_floor(self, level: &LevelDef, room: &RoomDef) -> bool {
        flat_landing(level, room, self.bounds(0.0, RISE_START), self.origin.y)
    }

    pub(crate) fn preview(self) -> Vec<[[f32; 3]; 3]> {
        [
            (0.0, RISE_START),
            (RISE_START, self.dimensions.rise_end),
            (self.dimensions.rise_end, self.dimensions.length),
        ]
        .into_iter()
        .flat_map(|(a, b)| {
            let point = |along, across| {
                let position = self.plan(along, across);
                [position.x, self.height(along) + 0.01, position.y]
            };
            let left = point(a, -HALF_WIDTH);
            let right = point(b, HALF_WIDTH);
            [
                [left, point(a, HALF_WIDTH), right],
                [left, right, point(b, -HALF_WIDTH)],
            ]
        })
        .collect()
    }

    /// Rapier floor anchors identify actual support, not an item's centre
    /// being inside a volume. Side and underside manifolds never drive it.
    #[must_use]
    pub fn tangent(self, point: Vec3, normal: Vec3) -> Option<Vec3> {
        let (along, across) = self.coordinates(point);
        if normal.y <= 0.75
            || !self.on_belt(along, SUPPORT_EPS)
            || across.abs() > HALF_WIDTH + SUPPORT_EPS
        {
            return None;
        }
        let flat = self.planar_velocity();
        let horizontal = Vec3::new(flat.x, 0.0, flat.y);
        // Keep the horizontal belt speed on the incline while matching the
        // measured support normal. It is the same rate as the tread clock.
        Some(Vec3::new(
            flat.x,
            -horizontal.dot(normal) / normal.y,
            flat.y,
        ))
    }
}

fn flat_landing(level: &LevelDef, room: &RoomDef, rect: PlanBounds, y: f32) -> bool {
    // Apply the same last-region priority as the real floor. Check only its
    // visible fragments, and prove union coverage without sampling across gaps.
    let mut unclaimed = vec![rect];
    let mut remaining = Vec::new();
    for region in level.floor_regions.iter().rev() {
        let bounds = PlanBounds::from(region.bounds());
        if !unclaimed.iter().any(|part| part.overlaps(bounds)) {
            continue;
        }
        if (room.floor_y + region.offset() - y).abs() > SUPPORT_EPS {
            return false;
        }
        for &part in &unclaimed {
            part.subtract(bounds, &mut remaining);
        }
        unclaimed.clear();
        std::mem::swap(&mut unclaimed, &mut remaining);
        if unclaimed.is_empty() {
            return true;
        }
    }
    (room.floor_y - y).abs() <= SUPPORT_EPS
}

fn valid_landings(level: &LevelDef, room: &RoomDef, lane: Lane) -> bool {
    let length = lane.dimensions.length;
    let complete = if lane.is_free() {
        lane.bounds(0.0, length)
    } else {
        lane.bounds(-APPROACH_M, length + APPROACH_M)
    };
    PlanBounds::from(room.bounds()).contains(complete)
        && !level
            .ramps
            .iter()
            .any(|ramp| PlanBounds::from(ramp.bounds()).overlaps(complete))
        && !level
            .stairs
            .iter()
            .any(|stairs| PlanBounds::from(stairs.bounds()).overlaps(complete))
        && (lane.is_free()
            || (flat_landing(
                level,
                room,
                lane.bounds(-APPROACH_M, RISE_START),
                lane.origin.y,
            ) && flat_landing(
                level,
                lane.upper_room
                    .and_then(|index| level.rooms.get(index))
                    .unwrap_or(room),
                lane.bounds(lane.dimensions.rise_end, length + APPROACH_M),
                lane.origin.y + lane.dimensions.rise,
            )))
}

fn clearance(level: &LevelDef, room: &RoomDef, lane: Lane) -> Result<(), String> {
    let length = lane.dimensions.length;
    let envelope = lane.bounds(0.0, length);
    if lane.is_free()
        && LevelSurfaces::new(level)
            .floor_y_at(lane.origin.x, lane.origin.z)
            .is_some_and(|floor_y| floor_y > lane.origin.y + SUPPORT_EPS)
    {
        return Err("The free escalator base cannot start below its carrier floor".into());
    }
    let has_headroom = |carrier: &RoomDef, along: &[f32]| {
        carrier.ceiling.is_open()
            || along.iter().all(|&at| {
                [-HALF_WIDTH, HALF_WIDTH].into_iter().all(|side| {
                    let p = lane.plan(at, side);
                    carrier.ceiling_y_at(p.x, p.y) + SUPPORT_EPS
                        >= lane.height(at) + BOARDING_CLEARANCE
                })
            })
    };
    if !has_headroom(room, &[0.0, RISE_START, lane.dimensions.rise_end, length])
        || lane
            .upper_room
            .and_then(|index| level.rooms.get(index))
            .is_some_and(|upper| {
                !has_headroom(upper, &[lane.dimensions.rise_end, length + APPROACH_M])
            })
    {
        return Err("Escalator needs 2 m of headroom above the belt".into());
    }
    if level.rooms.iter().enumerate().any(|(index, other)| {
        !std::ptr::eq(room, other)
            && Some(index) != lane.upper_room
            && envelope
                .intersection(other.bounds().into())
                .is_some_and(|rect| {
                    // Other rooms contribute a deck slab, not a solid volume.
                    other.floor_y > lane.origin.y + SUPPORT_EPS
                        && !lane.clear_vertical_band(
                            rect,
                            other.floor_y - crate::engine::level::MIN_STRUCTURE_M,
                            other.floor_y,
                        )
                })
    }) {
        return Err("Another storey crosses the escalator envelope; use an open connection".into());
    }
    clear_solids(level, lane)?;
    if level.floor_regions.iter().any(|region| {
        let (x0, x1, z0, z1) = region.bounds();
        let along_min = [Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z1)]
            .into_iter()
            .map(|point| lane.coordinates(point).0)
            .min_by(f32::total_cmp)
            .unwrap_or(0.0);
        let covered = lane.bounds(
            0.0,
            if lane.is_free() {
                length
            } else {
                lane.dimensions.rise_end
            },
        );
        covered.overlaps(region.bounds().into())
            && room.floor_y + region.offset() > lane.height(along_min) + SUPPORT_EPS
    }) {
        return Err("A raised floor crosses the incline; leave its run open".into());
    }
    Ok(())
}

fn clear_solids(level: &LevelDef, lane: Lane) -> Result<(), String> {
    if level
        .placement_collision_aabbs(&LevelSurfaces::new(level))
        .into_iter()
        .any(|solid| !lane.clear_of(solid))
    {
        return Err("A solid wall, guardrail, or prop crosses the escalator or its landing".into());
    }
    Ok(())
}

/// Geometry-only draft used before the editor commits the mechanism preset.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fit {
    pub(crate) lane: Lane,
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pub(crate) dimensions: Dimensions,
    pub(crate) placement: Placement,
    pub(crate) open_crest: bool,
}

impl Fit {
    fn try_new(
        level: &LevelDef,
        origin: Vec3,
        axis: (Vec2, f32),
        dimensions: Dimensions,
        placement: Placement,
    ) -> Result<Self, String> {
        let (along, yaw) = axis;
        let surfaces = LevelSurfaces::new(level);
        let owner = surfaces
            .room_index_at(origin.x, origin.z)
            .ok_or("Escalator base must be in a room")?;
        let room = level
            .rooms
            .get(owner)
            .ok_or("Escalator base must be in a room")?;
        let lane = Lane {
            origin,
            along,
            direction: Direction::Up,
            dimensions,
            placement,
            upper_room: None,
        }
        .attach_upper_room(level, owner);
        if !valid_landings(level, room, lane) {
            return Err(if lane.is_free() { "The escalator does not fit in a clear room footprint" }
                else { "Need flat, full-width landings and a clear straight run. In an atrium, use an open lower room and an elevated room at the crest." }.into());
        }
        clearance(level, room, lane)?;
        let footprint = lane.plan_bounds(0.0, dimensions.length, SHELL_HALF_WIDTH);
        if support_lanes(level, &surfaces)
            .into_iter()
            .any(|(_, prior)| {
                footprint.overlaps(prior.plan_bounds(
                    0.0,
                    prior.dimensions.length,
                    SHELL_HALF_WIDTH,
                ))
            })
        {
            return Err("The new escalator overlaps another belt".into());
        }
        Ok(Self {
            lane,
            origin,
            yaw,
            dimensions,
            placement,
            open_crest: lane.is_free()
                && !flat_landing(
                    level,
                    room,
                    lane.bounds(dimensions.length, dimensions.length + APPROACH_M),
                    origin.y + dimensions.rise,
                ),
        })
    }

    pub(crate) fn free(
        level: &LevelDef,
        origin: Vec3,
        turn: u8,
        dimensions: Dimensions,
    ) -> Result<Self, String> {
        let axis = AXES
            .get(usize::from(turn))
            .copied()
            .ok_or("Invalid escalator orientation")?;
        Self::try_new(level, origin, axis, dimensions, Placement::Free)
    }

    pub(crate) fn connect(level: &LevelDef, lower: Vec3, upper: Vec3) -> Result<Self, String> {
        let dimensions = Dimensions::new(upper.y - lower.y)?;
        let foot = Vec2::new(lower.x, lower.z);
        let crest = Vec2::new(upper.x, upper.z);
        let surfaces = LevelSurfaces::new(level);
        let room = surfaces
            .room_at(lower.x, lower.z)
            .ok_or("Pick a lower floor inside the room")?;
        let mut problem = "Pick the upper landing straight ahead, near its open edge".to_string();
        for axis @ (along, _) in AXES {
            let across = Vec2::new(along.y, -along.x);
            let start = foot.dot(along);
            let end = crest.dot(along);
            if end <= start || (crest.dot(across) - foot.dot(across)).abs() > 2.0 * HALF_WIDTH {
                continue;
            }
            let edges = level
                .floor_regions
                .iter()
                .map(crate::engine::level::FloorRegionDef::bounds)
                .chain(
                    level
                        .rooms
                        .iter()
                        .filter(|carrier| (carrier.floor_y - upper.y).abs() <= SUPPORT_EPS)
                        .map(RoomDef::bounds),
                )
                .flat_map(|(x0, x1, z0, z1)| {
                    [
                        Vec2::new(x0, z0).dot(along) - dimensions.rise_end,
                        Vec2::new(x1, z1).dot(along) - dimensions.rise_end,
                    ]
                });
            for at in [start, end - dimensions.length, end - dimensions.rise_end]
                .into_iter()
                .chain(edges)
            {
                if (at - start).abs() > 2.0 || (at + dimensions.length - end).abs() > 2.0 {
                    continue;
                }
                for side in [
                    f32::midpoint(foot.dot(across), crest.dot(across)),
                    foot.dot(across),
                    crest.dot(across),
                ] {
                    let centre = Vec2::new(
                        along.x.mul_add(at, across.x * side),
                        along.y.mul_add(at, across.y * side),
                    );
                    let result = Self::try_new(
                        level,
                        Vec3::new(centre.x, lower.y, centre.y),
                        axis,
                        dimensions,
                        Placement::Connected,
                    );
                    let fit = match result {
                        Ok(fit) => fit,
                        Err(error) => {
                            problem = error;
                            continue;
                        }
                    };
                    // Snap only along the selected platform, not to another
                    // island or storey that happens to have the same height.
                    let upper_carrier = fit
                        .lane
                        .upper_room
                        .and_then(|index| level.rooms.get(index))
                        .unwrap_or(room);
                    let selected_extents = [
                        (room, start.min(at), at + RISE_START, lower.y),
                        (
                            upper_carrier,
                            at + dimensions.rise_end,
                            end.max(at + dimensions.length),
                            upper.y,
                        ),
                    ];
                    if selected_extents.into_iter().all(|(carrier, a, b, y)| {
                        let rect = fit.lane.bounds(a - at, b - at);
                        PlanBounds::from(carrier.bounds()).contains(rect)
                            && flat_landing(level, carrier, rect, y)
                    }) {
                        return Ok(fit);
                    }
                }
            }
        }
        Err(problem)
    }
}

/// Validate placement without consulting visual geometry or mutating the room.
///
/// # Errors
/// Rejects unsupported rigs, transforms, footprint, ownership and count.
pub fn validate(level: &LevelDef) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    if level
        .props
        .iter()
        .filter(|prop| prop.escalator.is_some())
        .count()
        > 16
    {
        return Err("A level supports at most 16 escalators".into());
    }
    let mut occupied: Vec<(&RoomDef, PlanBounds)> = Vec::new();
    for (index, prop) in level
        .props
        .iter()
        .enumerate()
        .filter(|(_, prop)| prop.escalator.is_some())
    {
        let definition = prop
            .escalator
            .as_ref()
            .ok_or("Missing escalator definition")?;
        let _dimensions =
            Dimensions::new(definition.rise).map_err(|error| format!("Prop {index}: {error}"))?;
        if prop.model != MODEL
            || (prop.scale - 1.0).abs() > 1.0e-4
            || prop.pitch_degrees.abs() > 1.0e-4
            || prop.roll_degrees.abs() > 1.0e-4
            || prop.span.is_some()
            || prop.solid
            || prop.occludes
            || prop.shadow_only
            || prop.physics.is_some()
            || prop.float.is_some()
            || prop.animated_door.is_some()
            || prop.feltling.is_some()
            || prop.tall_guest.is_some()
            || level
                .routes
                .iter()
                .any(|route| prop.id.as_deref() == Some(route.id.as_str()))
        {
            return Err(format!(
                "Prop {index} escalator requires the upright, unscaled Mall rig"
            ));
        }
        let Some((room, lane)) = placed_lane(level, &surfaces, prop)
            .and_then(|(owner, lane)| level.rooms.get(owner).map(|room| (room, lane)))
            .filter(|(room, lane)| valid_landings(level, room, *lane))
        else {
            return Err(format!(
                "Prop {index} escalator needs a cardinal yaw and a clear room footprint; Connect Floors also needs full-width approaches at the foot and +{:.2} m crest",
                definition.rise
            ));
        };
        let footprint = lane.bounds(0.0, lane.dimensions.length);
        if definition.custom_rise() || !is_legacy(&definition.placement) {
            clearance(level, room, lane).map_err(|error| format!("Prop {index}: {error}"))?;
        }
        if occupied
            .iter()
            .any(|&(prior_room, prior)| std::ptr::eq(room, prior_room) && footprint.overlaps(prior))
        {
            return Err(format!("Prop {index} escalator overlaps a tread lane"));
        }
        occupied.push((room, footprint));
        if !prop.components.iter().any(|component| {
            matches!(component,
                ComponentDef::Animation { clip, playing: _, looped: true, speed: _ } if clip == CLIP
            )
        }) {
            return Err(format!("Prop {index} escalator requires the {CLIP} loop"));
        }
    }
    Ok(())
}

#[must_use]
pub fn support_lanes(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<(usize, Lane)> {
    level
        .props
        .iter()
        .filter_map(|prop| placed_lane(level, surfaces, prop))
        .collect()
}

fn placed_lane(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    prop: &PropDef,
) -> Option<(usize, Lane)> {
    let _definition = prop.escalator.as_ref()?;
    let owner = surfaces.room_index_at(prop.x, prop.z)?;
    let y = surfaces.floor_y_at(prop.x, prop.z)?;
    Some((
        owner,
        Lane::new(prop, y + prop.y)?.attach_upper_room(level, owner),
    ))
}

pub struct Escalator {
    pub id: String,
    pub lane: Lane,
    running: bool,
    frame_power: bool,
    initial_running: bool,
    phase: f32,
}

impl Escalator {
    #[must_use]
    pub fn resolve(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<Self> {
        let mut resolved = Vec::new();
        for (prop, id) in level.props.iter().zip(level.prop_instance_ids()) {
            let Some(definition) = &prop.escalator else {
                continue;
            };
            let Some((_, lane)) = placed_lane(level, surfaces, prop) else {
                continue;
            };
            let running = definition.initial_running;
            resolved.push(Self {
                id,
                lane,
                running,
                frame_power: running,
                initial_running: running,
                phase: 0.0,
            });
        }
        resolved
    }

    #[must_use]
    pub const fn running(&self) -> bool {
        self.running
    }

    #[must_use]
    pub const fn powered_this_frame(&self) -> bool {
        self.frame_power
    }

    pub const fn power(&mut self, toggle: bool) {
        self.running = !toggle || !self.running;
    }

    pub const fn reset(&mut self) {
        self.running = self.initial_running;
        self.frame_power = self.initial_running;
        self.phase = 0.0;
    }

    pub(crate) fn begin_frame(&mut self, seconds: f32) -> Option<f32> {
        self.frame_power = self.running;
        if !self.frame_power || seconds <= 0.0 {
            return None;
        }
        self.phase = seconds
            .mul_add(
                self.lane.direction.speed() / self.lane.dimensions.pitch,
                self.phase,
            )
            .rem_euclid(1.0);
        Some(self.phase)
    }

    #[must_use]
    pub fn cue(&self) -> PoseCue {
        PoseCue::Sample {
            name: CLIP.to_string(),
            progress: self.phase,
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "direction": self.lane.direction, "running": self.running(),
            "frame_power": self.frame_power, "phase": self.phase
        })
    }
}

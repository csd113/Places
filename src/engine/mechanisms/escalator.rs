//! A fixed escalator's support lane and its authoritative tread clock.

use glam::{Vec2, Vec3};

use crate::engine::entity::PoseCue;
use crate::engine::level::{
    ComponentDef, LevelDef, LevelSurfaces, PropDef, RampDef, RampSurface, RoomDef,
};

pub const MODEL: &str = "mall:escalator";
pub const CLIP: &str = "escalator_run";
const HALF_WIDTH: f32 = 0.6;
const RISE_START: f32 = 0.6;
const RISE_END: f32 = 6.6;
const BELT_END: f32 = 7.2;
const RISE: f32 = 3.0;
const PITCH_M: f32 = 0.3;
const SPEED_MPS: f32 = 0.6;
const GRADE: f32 = 0.5;
pub(crate) const SUPPORT_EPS: f32 = 0.0001;
const APPROACH_M: f32 = 0.45;

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

/// Presence installs the fixed tread lane and specifies its power on reset.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    #[serde(default)]
    pub initial_running: bool,
}

/// World-space pitch line under the moving visual steps. The footplate
/// endpoints share the actual lower and upper landing heights.
#[derive(Clone, Copy, PartialEq)]
pub struct Lane {
    origin: Vec3,
    along: Vec2,
}

impl Lane {
    #[must_use]
    pub fn new(prop: &PropDef, base_y: f32) -> Option<Self> {
        let yaw = prop.rotation_degrees.rem_euclid(360.0);
        let along = [Vec2::Y, Vec2::X, Vec2::NEG_Y, Vec2::NEG_X]
            .into_iter()
            .zip([0.0_f32, 90.0, 180.0, 270.0])
            .find_map(|(direction, degrees)| {
                ((yaw - degrees).abs() < 1.0e-4).then_some(direction)
            })?;
        Some(Self {
            origin: Vec3::new(prop.x, base_y, prop.z),
            along,
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
        let a = self.plan(start, -HALF_WIDTH);
        let b = self.plan(end, HALF_WIDTH);
        PlanBounds(a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y))
    }

    #[must_use]
    pub fn ramps(self, room_y: f32) -> [RampSurface; 3] {
        [
            (0.0, RISE_START),
            (RISE_START, RISE_END),
            (RISE_END, BELT_END),
        ]
        .map(|(start, end)| self.strip(room_y, start, end))
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
        (along - RISE_START)
            .clamp(0.0, RISE_END - RISE_START)
            .mul_add(GRADE, self.origin.y)
    }

    /// A selected support can still straddle a side or the terminal comb.
    /// Drive actors only while their centre remains aboard.
    #[must_use]
    pub fn contains(self, feet: Vec3) -> bool {
        let (along, across) = self.coordinates(feet);
        (0.0..BELT_END).contains(&along) && across.abs() < HALF_WIDTH
    }

    #[must_use]
    pub const fn planar_velocity(self) -> Vec2 {
        Vec2::new(self.along.x * SPEED_MPS, self.along.y * SPEED_MPS)
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
        let PlanBounds(x0, x1, z0, z1) = self.bounds(RISE_END, BELT_END);
        (x0, x1, z0, z1)
    }

    pub(crate) fn support_height_at(self, x: f32, z: f32) -> Option<f32> {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        ((0.0..=BELT_END).contains(&along) && across.abs() <= HALF_WIDTH)
            .then(|| self.height(along))
    }

    /// The metal landing plate owns the top flat; the overlapping floor tile
    /// is structural support, not another render surface at the same height.
    pub(crate) fn covers_upper_plate(self, x: f32, z: f32) -> bool {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        (RISE_END..BELT_END).contains(&along) && across.abs() < HALF_WIDTH
    }

    pub(crate) fn covers_crest_face(self, x: f32, z: f32) -> bool {
        let (along, across) = self.coordinates(Vec3::new(x, 0.0, z));
        (along - RISE_END).abs() <= SUPPORT_EPS && across.abs() < HALF_WIDTH
    }

    /// Rapier floor anchors identify actual support, not an item's centre
    /// being inside a volume. Side and underside manifolds never drive it.
    #[must_use]
    pub fn tangent(self, point: Vec3, normal: Vec3) -> Option<Vec3> {
        let (along, across) = self.coordinates(point);
        if normal.y <= 0.75
            || along < -SUPPORT_EPS
            || along >= BELT_END
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
    let complete = lane.bounds(-APPROACH_M, BELT_END + APPROACH_M);
    PlanBounds::from(room.bounds()).contains(complete)
        && !level
            .ramps
            .iter()
            .any(|ramp| PlanBounds::from(ramp.bounds()).overlaps(complete))
        && !level
            .stairs
            .iter()
            .any(|stairs| PlanBounds::from(stairs.bounds()).overlaps(complete))
        && flat_landing(
            level,
            room,
            lane.bounds(-APPROACH_M, RISE_START),
            lane.origin.y,
        )
        && flat_landing(
            level,
            room,
            lane.bounds(RISE_END, BELT_END + APPROACH_M),
            lane.origin.y + RISE,
        )
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
        let placement = surfaces
            .room_at(prop.x, prop.z)
            .zip(surfaces.floor_y_at(prop.x, prop.z));
        let Some((room, lane)) = placement
            .and_then(|(room, y)| Lane::new(prop, y + prop.y).map(|lane| (room, lane)))
            .filter(|(room, lane)| valid_landings(level, room, *lane))
        else {
            return Err(format!(
                "Prop {index} escalator needs a cardinal yaw and full-width level approaches at the foot and +3 m crest in one room"
            ));
        };
        let footprint = lane.bounds(0.0, BELT_END);
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
        .filter_map(|prop| {
            let _definition = prop.escalator.as_ref()?;
            let room = surfaces.room_index_at(prop.x, prop.z)?;
            let floor_y = surfaces.floor_y_at(prop.x, prop.z)?;
            Lane::new(prop, floor_y + prop.y).map(|lane| (room, lane))
        })
        .collect()
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
            let Some(floor_y) = surfaces.floor_y_at(prop.x, prop.z) else {
                continue;
            };
            let Some(lane) = Lane::new(prop, floor_y + prop.y) else {
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
            .mul_add(SPEED_MPS / PITCH_M, self.phase)
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
            "id": self.id, "running": self.running(), "frame_power": self.frame_power, "phase": self.phase
        })
    }
}

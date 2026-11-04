//! Resolve one wall-fitted pose for both preview and the atomic placement edit.
use super::{Document, ElevatorDef};
use crate::engine::level::{LevelSurfaces, WallAxis};
use glam::Vec3;
use serde_json::{Value, json};

const WIDTH: f32 = 1.5;
const HEIGHT: f32 = 2.25;
const EPSILON: f32 = 0.0005;

pub(super) struct Placement {
    pub at: [f32; 3],
    pub rotation: f32,
    pub host: Option<usize>,
    opening: Option<(Option<usize>, f32)>,
}
impl Placement {
    pub const fn definition(&self) -> ElevatorDef {
        ElevatorDef {
            id: String::new(),
            x: self.at[0],
            z: self.at[2],
            rotation_degrees: self.rotation,
            initial_stop: 0,
            speed_mps: 1.5,
            stops: Vec::new(),
        }
    }
    pub fn fit_opening(&self, raw: &mut Value) -> Result<(), String> {
        let (Some(host), Some((existing, offset))) = (self.host, self.opening) else {
            return Ok(());
        };
        let wall = raw
            .get_mut("walls")
            .and_then(Value::as_array_mut)
            .and_then(|walls| walls.get_mut(host))
            .ok_or("Missing host wall")?;
        let openings = super::super::document::record_array_mut(wall, "openings")?;
        let opening =
            json!({"kind":"door", "offset":offset, "width":WIDTH,"height":HEIGHT,"sill":0_f32});
        if let Some(index) = existing {
            let record = openings.get_mut(index).ok_or("Missing opening")?;
            // Keep authored opening metadata while fitting its manufactured aperture.
            record["offset"] = json!(offset);
            record["width"] = json!(WIDTH);
            record["height"] = json!(HEIGHT);
        } else {
            openings.push(opening);
        }
        Ok(())
    }
}

fn nearby_wall(doc: &Document, at: [f32; 3]) -> Option<usize> {
    let level = doc.level();

    level
        .walls
        .iter()
        .enumerate()
        .filter_map(|(index, wall)| {
            let (x, z) = wall.min_corner();
            let (along, cross) = match wall.axis() {
                WallAxis::X => (at[0] - x, at[2] - z),
                WallAxis::Z => (at[2] - z, at[0] - x),
            };
            let distance = cross.abs().min((cross - wall.thickness()).abs());
            ((at[1] - wall.y).abs() < 0.05
                && along >= 0.0
                && along <= wall.length()
                && distance <= 0.6)
                .then_some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// `at` is the unsnapped floor hit. The face coordinate is never grid-rounded.
pub(super) fn resolve(
    doc: &Document,
    mut at: [f32; 3],
    rotation: f32,
    eye: Option<Vec3>,
    hint: Option<usize>,
    grid: Option<f32>,
) -> Result<Placement, String> {
    let level = doc.level();
    let host = hint.or_else(|| nearby_wall(doc, at));
    let mut result = Placement {
        at,
        rotation,
        host,
        opening: None,
    };
    if let Some(index) = host {
        let wall = level.walls.get(index).ok_or("Missing host wall")?;
        // The retained rear cabin body starts at Z=.24; the shared shaft
        // admission starts at .25. Keep host walls inside the front pockets.
        if wall.thickness() > 0.24 + EPSILON {
            return Err("Not enough space: this wall is too thick for the elevator doorway (maximum 0.24 m). Use a thinner landing wall.".into());
        }
        let (x, z) = wall.min_corner();
        let (start, cross, coordinate, eye_cross) = match wall.axis() {
            WallAxis::X => (x, z, at[0], eye.map(|p| p.z)),
            WallAxis::Z => (z, x, at[2], eye.map(|p| p.x)),
        };
        let inward = rotation.to_radians().sin_cos();
        let positive = eye_cross.map_or_else(
            || match wall.axis() {
                WallAxis::X => inward.1 >= 0.0,
                WallAxis::Z => inward.0 >= 0.0,
            },
            |value| value <= wall.thickness().mul_add(0.5, cross),
        );
        let mut along = coordinate - start;
        if let Some(step) = grid {
            along = (along / step).round() * step;
        }
        let existing =
            wall.openings.iter().enumerate().find(|(_, opening)| {
                along >= opening.offset - 0.15 && along <= opening.end() + 0.15
            });
        if let Some((_, opening)) = existing {
            if !opening.is_door()
                || !opening.reaches_floor()
                || opening.glass.is_some()
                || opening.surround.is_some()
                || opening.recess.is_some()
            {
                return Err("This opening has a window, pane, recess or surround. Choose a clear floor-level doorway for the elevator.".into());
            }
            along = opening.width.mul_add(0.5, opening.offset);
        }
        let offset = WIDTH.mul_add(-0.5, along);
        refuse_attached_door(level, wall, offset)?;
        if offset < -EPSILON || offset + WIDTH > wall.length() + EPSILON {
            return Err("Not enough space: the elevator needs a 1.50 m doorway within this wall. Move the pointer away from the wall end.".into());
        }
        if wall
            .height
            .unwrap_or_else(|| LevelSurfaces::new(level).clear_ceiling_height_along(wall, along))
            < HEIGHT - EPSILON
        {
            return Err(
                "Not enough space: the elevator doorway needs a wall at least 2.25 m high.".into(),
            );
        }
        for (other, opening) in wall.openings.iter().enumerate() {
            if existing.is_some_and(|(selected, _)| selected == other) {
                continue;
            }
            if offset < opening.end() + 0.01 && offset + WIDTH > opening.offset - 0.01 {
                return Err(
                    "Not enough space: the fitted elevator doorway would overlap another opening."
                        .into(),
                );
            }
        }
        let face = if positive {
            cross
        } else {
            cross + wall.thickness()
        };
        at = match wall.axis() {
            WallAxis::X => [start + along, wall.y, face],
            WallAxis::Z => [face, wall.y, start + along],
        };
        result.at = at;
        result.rotation = match (wall.axis(), positive) {
            (WallAxis::X, true) => 0.0,
            (WallAxis::X, false) => 180.0,
            (WallAxis::Z, true) => 90.0,
            (WallAxis::Z, false) => 270.0,
        };
        result.opening = Some((existing.map(|(i, _)| i), offset));
    } else if let Some(step) = grid {
        result.at[0] = (at[0] / step).round() * step;
        result.at[2] = (at[2] / step).round() * step;
    }
    clearance(doc, &result)?;
    Ok(result)
}

/// Separating axes retain the real narrow rear body, including rotated free
/// placement. A coarse three-metre square wrongly rejects sufficient gaps.
fn intersects(def: &ElevatorDef, low: Vec3, high: Vec3, other_low: Vec3, other_high: Vec3) -> bool {
    if low.y >= other_high.y - EPSILON || high.y <= other_low.y + EPSILON {
        return false;
    }
    let corners = [
        (low.x, low.z),
        (low.x, high.z),
        (high.x, low.z),
        (high.x, high.z),
    ]
    .map(|(x, z)| def.point(x, 0.0, z));
    let other = [
        (other_low.x, other_low.z),
        (other_low.x, other_high.z),
        (other_high.x, other_low.z),
        (other_high.x, other_high.z),
    ]
    .map(|(x, z)| Vec3::new(x, 0.0, z));
    let (sin, cos) = def.rotation_degrees.to_radians().sin_cos();
    [
        Vec3::X,
        Vec3::Z,
        Vec3::new(cos, 0.0, -sin),
        Vec3::new(sin, 0.0, cos),
    ]
    .into_iter()
    .all(|axis| {
        let range = |points: &[Vec3; 4]| {
            points
                .iter()
                .map(|point| point.dot(axis))
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), value| {
                    (lo.min(value), hi.max(value))
                })
        };
        let (lo, hi) = range(&corners);
        let (other_lo, other_hi) = range(&other);
        hi.min(other_hi) - lo.max(other_lo) > EPSILON
    })
}
fn clearance(doc: &Document, placement: &Placement) -> Result<(), String> {
    let def = placement.definition();
    let floor = placement.at[1];
    let level = doc.level();
    // Real stock envelope: wide shallow pockets, narrower deep cabin shell.
    let sections = [
        (
            Vec3::new(-1.45, floor, 0.018),
            Vec3::new(1.45, floor + 2.57, 0.362),
        ),
        (
            Vec3::new(-1.28, floor, 0.24),
            Vec3::new(1.28, floor + 2.63, 2.48),
        ),
    ];
    let approach = def.point(0.0, floor, -0.3);
    for (index, room) in level.rooms.iter().enumerate() {
        let (left, right, front, back) = room.bounds();
        let is_approach = (room.floor_y - floor).abs() < 0.05
            && approach.x >= left - EPSILON
            && approach.x <= right + EPSILON
            && approach.z >= front - EPSILON
            && approach.z <= back + EPSILON;
        if is_approach
            || placement.host.is_none()
                && placement.at[0] >= left
                && placement.at[0] <= right
                && placement.at[2] >= front
                && placement.at[2] <= back
                && (room.floor_y - floor).abs() < 0.05
        {
            check_ceiling(&def, &sections, room, index)?;
            continue;
        }
        if sections.iter().any(|&(low, high)| {
            intersects(
                &def,
                low,
                high,
                Vec3::new(left, room.floor_y, front),
                Vec3::new(right, room.floor_y + room.height, back),
            )
        }) {
            return Err(format!(
                "Not enough space: the elevator cabin would extend into Room {}. It needs 2.48 m behind the landing face, with a 2.56 m wide cabin and 2.90 m front pockets.",
                index.saturating_add(1)
            ));
        }
    }
    let surfaces = LevelSurfaces::new(level);
    for (index, wall) in level.walls.iter().enumerate() {
        if Some(index) == placement.host {
            continue;
        }
        let (x, z) = wall.min_corner();
        for slice in surfaces.wall_solid_slices(wall) {
            let (low, high) = match wall.axis() {
                WallAxis::X => (
                    Vec3::new(x + slice.start, slice.bottom, z),
                    Vec3::new(x + slice.end, slice.top, z + wall.thickness()),
                ),
                WallAxis::Z => (
                    Vec3::new(x, slice.bottom, z + slice.start),
                    Vec3::new(x + wall.thickness(), slice.top, z + slice.end),
                ),
            };
            if sections
                .iter()
                .any(|&(a, b)| intersects(&def, a, b, low, high))
            {
                return Err(format!(
                    "Not enough space: Wall {} intersects the elevator cabin. Widen the shaft or choose another wall.",
                    index.saturating_add(1)
                ));
            }
        }
    }
    Ok(())
}

fn check_ceiling(
    def: &ElevatorDef,
    sections: &[(Vec3, Vec3)],
    room: &crate::engine::level::RoomDef,
    index: usize,
) -> Result<(), String> {
    let (left, right, front, back) = room.bounds();
    let floor = room.floor_y;
    for &(low, high) in sections {
        if !intersects(
            def,
            low,
            high,
            Vec3::new(left, floor, front),
            Vec3::new(right, floor + 100.0, back),
        ) {
            continue;
        }
        for (x, z) in [
            (low.x, low.z),
            (low.x, high.z),
            (high.x, low.z),
            (high.x, high.z),
        ] {
            let point = def.point(x, 0.0, z);
            let ceiling = room.ceiling_y_at(point.x.clamp(left, right), point.z.clamp(front, back));
            if high.y > ceiling + EPSILON {
                return Err(format!(
                    "Not enough space: the elevator roof would intersect Room {}'s ceiling (2.63 m cabin height required).",
                    index.saturating_add(1)
                ));
            }
        }
    }
    Ok(())
}

fn refuse_attached_door(
    level: &crate::engine::level::LevelDef,
    wall: &crate::engine::level::WallDef,
    offset: f32,
) -> Result<(), String> {
    let (x, z) = wall.min_corner();
    let surfaces = LevelSurfaces::new(level);
    for door in &level.doors {
        let (along, cross) = match wall.axis() {
            WallAxis::X => (door.x - x, door.z - z),
            WallAxis::Z => (door.z - z, door.x - x),
        };
        let bottom = surfaces.floor_y_at(door.x, door.z).unwrap_or(0.0) + door.y;
        if along >= offset - 0.05
            && along <= offset + WIDTH + 0.05
            && cross >= -0.1
            && cross <= wall.thickness() + 0.1
            && bottom < wall.y + HEIGHT
            && bottom + door.height > wall.y
        {
            return Err("This doorway already contains a hinged door. Remove that door before fitting an elevator.".into());
        }
    }
    Ok(())
}

//! Room edges keep their opposite edge and world-space contents fixed. Related
//! perimeter walls/door leaves follow the moved edge in the same transaction.
use super::{Document, ObjectId, objects, persistence};
use crate::engine::level::{LevelDef, RoomDef, WallAxis};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    West,
    East,
    North,
    South,
}
impl Edge {
    pub(crate) const ALL: [Self; 4] = [Self::West, Self::East, Self::North, Self::South];
    pub(crate) const fn axis(self) -> usize {
        match self {
            Self::West | Self::East => 0,
            Self::North | Self::South => 2,
        }
    }
    pub(crate) const fn negative(self) -> bool {
        matches!(self, Self::West | Self::North)
    }
    pub(crate) fn handle(self, room: &RoomDef) -> [f32; 3] {
        let (left, right, near, far) = room.bounds();
        let y = room.floor_y + 0.12;
        match self {
            Self::West => [left, y, f32::midpoint(near, far)],
            Self::East => [right, y, f32::midpoint(near, far)],
            Self::North => [f32::midpoint(left, right), y, near],
            Self::South => [f32::midpoint(left, right), y, far],
        }
    }
}
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.011
}
fn inside(room: &RoomDef, x: f32, z: f32) -> bool {
    let (left, right, front, back) = room.bounds();
    x >= left - 0.011 && x <= right + 0.011 && z >= front - 0.011 && z <= back + 0.011
}
impl Document {
    pub(crate) fn room_edge_candidate(
        &self,
        id: ObjectId,
        edge: Edge,
        delta: f32,
    ) -> Result<Value, String> {
        let ObjectId::Room(index) = id else {
            return Err("Select a room to resize".into());
        };
        let old = self
            .level()
            .rooms
            .get(index)
            .ok_or("Room no longer exists")?;
        if !delta.is_finite() || delta.abs() > 100.0 {
            return Err("Resize distance must be finite and within 100m".into());
        }
        let mut new = old.clone();
        match edge {
            Edge::West => {
                new.x += delta;
                new.width -= delta;
            }
            Edge::East => new.width += delta,
            Edge::North => {
                new.z += delta;
                new.depth -= delta;
            }
            Edge::South => new.depth += delta,
        }
        if new.width < 0.5 || new.depth < 0.5 {
            return Err("A room must retain at least 0.50m width and depth".into());
        }
        if near(delta, 0.0) {
            return Ok(self.raw().clone());
        }
        let (a, b, c, d) = old.bounds();
        let old_edge = match edge {
            Edge::West => a,
            Edge::East => b,
            Edge::North => c,
            Edge::South => d,
        };
        let (na, nb, nc, nd) = new.bounds();
        for (other_index, other) in self.level().rooms.iter().enumerate() {
            if other_index == index
                || other.floor_y >= old.eave_y() - 0.01
                || other.eave_y() <= old.floor_y + 0.01
            {
                continue;
            }
            let (oa, ob, oc, od) = other.bounds();
            if na < ob - 0.01 && nb > oa + 0.01 && nc < od - 0.01 && nd > oc + 0.01 {
                return Err(format!(
                    "Resize would overlap Room{} on this storey",
                    other_index.saturating_add(1)
                ));
            }
            let shared = if edge.axis() == 0 {
                (near(oa, old_edge) || near(ob, old_edge)) && oc < d && od > c
            } else {
                (near(oc, old_edge) || near(od, old_edge)) && oa < b && ob > a
            };
            if shared {
                return Err("This edge joins another room. Resize an outer edge, or edit the connected footprints together with precise controls.".into());
            }
        }
        check_support(self.level(), old, &new)?;
        let mut raw = self.raw().clone();
        let room = objects::object_mut(&mut raw, id)?;
        for (key, value) in [
            ("x", new.x),
            ("z", new.z),
            ("width", new.width),
            ("depth", new.depth),
        ] {
            drop(room.insert(key.into(), json!(value)));
        }
        resize_walls(&mut raw, self.level(), old, edge, old_edge, delta)?;
        let _validated = persistence::validate_raw(&raw, self.level_id())?;
        Ok(raw)
    }
    pub(crate) fn resize_room_edge(
        &mut self,
        id: ObjectId,
        edge: Edge,
        delta: f32,
    ) -> Result<(), String> {
        let raw = self.room_edge_candidate(id, edge, delta)?;
        self.accept_edit(raw)
    }
}

fn check_support(level: &LevelDef, old: &RoomDef, new: &RoomDef) -> Result<(), String> {
    // Expansion leaves all content in world coordinates. A shrink must not
    // strand any supported instance, modifier, route, or local floor override.
    if new.width >= old.width && new.depth >= old.depth {
        return Ok(());
    }
    for prop in &level.props {
        if inside(old, prop.x, prop.z) {
            let size = prop
                .size
                .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE);
            let bounds = prop.placement_bounds(old.floor_y, size);
            if !inside(new, bounds.min[0], bounds.min[2])
                || !inside(new, bounds.max[0], bounds.max[2])
            {
                return Err(format!(
                    "Shrink would remove support from {}. Move it first.",
                    prop.id.as_ref().unwrap_or(&prop.model)
                ));
            }
        }
    }
    if inside(old, level.spawn.x, level.spawn.z) && !inside(new, level.spawn.x, level.spawn.z) {
        return Err("Shrink would remove the player spawn's support".into());
    }
    for light in &level.ceiling_lights {
        if inside(old, light.x, light.z) && !inside(new, light.x, light.z) {
            return Err("Shrink would leave a light outside its room; move it first".into());
        }
    }
    let raw = serde_json::to_value(level).map_err(|e| e.to_string())?;
    for key in [
        "ramps",
        "stairs",
        "floor_regions",
        "floor_patches",
        "water",
        "volumes",
        "spawn_points",
        "fog_regions",
        "ground_fog",
        "routes",
        "guardrails",
        "half_walls",
        "columns",
        "archways",
        "thresholds",
        "baseboards",
        "arc_walls",
        "pillars",
    ] {
        if raw
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|records| {
                records
                    .iter()
                    .any(|record| intersects_removed(record, old, new))
            })
        {
            return Err(format!(
                "Shrinking requires reviewing {key}; expansion keeps their world coordinates intact."
            ));
        }
    }
    Ok(())
}

fn resize_walls(
    raw: &mut Value,
    level: &LevelDef,
    room: &RoomDef,
    edge: Edge,
    at: f32,
    delta: f32,
) -> Result<(), String> {
    let mut moved = Vec::new();
    for (index, wall) in level.walls.iter().enumerate() {
        if (wall.y - room.floor_y).abs() > 0.05 {
            continue;
        }
        let perpendicular = match wall.axis() {
            WallAxis::X => edge.axis() == 2,
            WallAxis::Z => edge.axis() == 0,
        };
        let (start, end, cross) = if edge.axis() == 0 {
            (
                wall.x,
                wall.x + wall.width,
                f32::mul_add(wall.depth, 0.5, wall.z),
            )
        } else {
            (
                wall.z,
                wall.z + wall.depth,
                f32::mul_add(wall.width, 0.5, wall.x),
            )
        };
        let (low, high) = if edge.axis() == 0 {
            (room.z, room.z + room.depth)
        } else {
            (room.x, room.x + room.width)
        };
        if perpendicular
            && at >= start - 0.011
            && at <= end + 0.011
            && cross >= low
            && cross <= high
        {
            let (wall_low, wall_high) = if edge.axis() == 0 {
                (wall.z, wall.z + wall.depth)
            } else {
                (wall.x, wall.x + wall.width)
            };
            if wall_low < low - 0.25 || wall_high > high + 0.25 {
                return Err("This boundary wall spans another space. Split/review that wall before resizing.".into());
            }
            // Wall crosses this edge on its narrow axis; move its full slab and openings.
            let record = objects::object_mut(raw, ObjectId::Wall(index))?;
            drop(record.insert(
                if edge.axis() == 0 { "x" } else { "z" }.into(),
                json!(start + delta),
            ));
            moved.push(wall);
        } else if !perpendicular && cross >= low - 0.25 && cross <= high + 0.25 {
            let anchor = if edge.negative() { start } else { end };
            if !near(anchor, at) {
                continue;
            }
            let (position, length) = if edge.negative() {
                (start + delta, end - start - delta)
            } else {
                (start, end - start + delta)
            };
            if length < 0.05 {
                return Err("Resize would collapse a perimeter wall".into());
            }
            let shift = position - start;
            if wall
                .openings
                .iter()
                .any(|opening| opening.offset - shift < 0.0 || opening.end() - shift > length)
            {
                return Err("Resize would cut through an opening; move the opening first".into());
            }
            let record = objects::object_mut(raw, ObjectId::Wall(index))?;
            drop(record.insert(
                if edge.axis() == 0 { "x" } else { "z" }.into(),
                json!(position),
            ));
            drop(record.insert(
                if edge.axis() == 0 { "width" } else { "depth" }.into(),
                json!(length),
            ));
            for opening in record
                .get_mut("openings")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let value = opening
                    .get("offset")
                    .and_then(Value::as_f64)
                    .ok_or("Opening has no offset")?;
                let object = opening.as_object_mut().ok_or("Invalid opening")?;
                drop(object.insert("offset".into(), json!(value - f64::from(shift))));
            }
        }
    }
    check_fixed_fittings(level, room, &moved)?;
    move_decals(raw, level, room, &moved, edge, delta)?;
    move_fittings(raw, level, room, &moved, edge, delta)
}

fn move_fittings(
    raw: &mut Value,
    level: &LevelDef,
    room: &RoomDef,
    moved: &[&crate::engine::level::WallDef],
    edge: Edge,
    delta: f32,
) -> Result<(), String> {
    for (index, door) in level.doors.iter().enumerate() {
        let floor = crate::engine::level::LevelSurfaces::new(level)
            .floor_y_at(door.x, door.z)
            .unwrap_or(0.0);
        if near(floor, room.floor_y)
            && moved.iter().any(|wall| {
                door.x >= wall.x - 0.05
                    && door.x <= wall.x + wall.width + 0.05
                    && door.z >= wall.z - 0.05
                    && door.z <= wall.z + wall.depth + 0.05
            })
        {
            let record = objects::object_mut(raw, ObjectId::Door(index))?;
            let (key, value) = if edge.axis() == 0 {
                ("x", door.x)
            } else {
                ("z", door.z)
            };
            drop(record.insert(key.into(), json!(value + delta)));
        }
    }
    for (index, light) in level.ceiling_lights.iter().enumerate() {
        if light.mount == crate::engine::level::LightMount::Wall
            && light
                .y
                .is_some_and(|y| y >= room.floor_y && y <= room.eave_y())
            && moved.iter().any(|wall| {
                light.x >= wall.x - 0.2
                    && light.x <= wall.x + wall.width + 0.2
                    && light.z >= wall.z - 0.2
                    && light.z <= wall.z + wall.depth + 0.2
            })
        {
            let record = objects::object_mut(raw, ObjectId::Light(index))?;
            let (key, value) = if edge.axis() == 0 {
                ("x", light.x)
            } else {
                ("z", light.z)
            };
            drop(record.insert(key.into(), json!(value + delta)));
        }
    }
    Ok(())
}
// Unknown positional forms are conservatively reviewed; known distant rectangles
// do not prevent shrinking an unrelated room.
fn intersects_removed(record: &Value, old: &RoomDef, new: &RoomDef) -> bool {
    let coordinate = |key| record.get(key).and_then(Value::as_f64);
    if let (Some(x), Some(z)) = (coordinate("x"), coordinate("z")) {
        let width = coordinate("width")
            .or_else(|| coordinate("radius").map(|v| v * 2.0_f64))
            .unwrap_or(0.0);
        let depth = coordinate("depth")
            .or_else(|| coordinate("radius").map(|v| v * 2.0_f64))
            .unwrap_or(0.0);
        let radius = coordinate("radius").unwrap_or(0.0);
        let (left, right, front, back) = (
            x - radius,
            x + width - radius,
            z - radius,
            z + depth - radius,
        );
        let overlaps = left <= f64::from(old.x + old.width)
            && right >= f64::from(old.x)
            && front <= f64::from(old.z + old.depth)
            && back >= f64::from(old.z);
        let retained = left >= f64::from(new.x)
            && right <= f64::from(new.x + new.width)
            && front >= f64::from(new.z)
            && back <= f64::from(new.z + new.depth);
        overlaps && !retained
    } else {
        true
    }
}

fn fitting_near(walls: &[&crate::engine::level::WallDef], x: f32, z: f32) -> bool {
    walls.iter().any(|wall| {
        x >= wall.x - 0.2
            && x <= wall.x + wall.width + 0.2
            && z >= wall.z - 0.2
            && z <= wall.z + wall.depth + 0.2
    })
}
fn check_fixed_fittings(
    level: &LevelDef,
    room: &RoomDef,
    walls: &[&crate::engine::level::WallDef],
) -> Result<(), String> {
    for prop in &level.props {
        if prop.physics.is_none()
            && prop.y > 0.2
            && inside(room, prop.x, prop.z)
            && fitting_near(walls, prop.x, prop.z)
        {
            return Err(format!(
                "Fixed elevated prop {} may be attached to this wall. Move/review it before resizing.",
                prop.id.as_ref().unwrap_or(&prop.model)
            ));
        }
    }
    Ok(())
}
fn move_decals(
    raw: &mut Value,
    level: &LevelDef,
    room: &RoomDef,
    walls: &[&crate::engine::level::WallDef],
    edge: Edge,
    delta: f32,
) -> Result<(), String> {
    for (index, decal) in level.decals.iter().enumerate() {
        if decal.y >= room.floor_y
            && decal.y <= room.eave_y()
            && decal
                .surface
                .normal()
                .get(edge.axis())
                .is_some_and(|normal| normal.abs() > 0.5)
            && fitting_near(walls, decal.x, decal.z)
        {
            let record = raw
                .get_mut("decals")
                .and_then(Value::as_array_mut)
                .and_then(|records| records.get_mut(index))
                .and_then(Value::as_object_mut)
                .ok_or("Decal record missing")?;
            let (key, value) = if edge.axis() == 0 {
                ("x", decal.x)
            } else {
                ("z", decal.z)
            };
            drop(record.insert(key.into(), json!(value + delta)));
        }
    }
    Ok(())
}

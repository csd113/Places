//! Wall edits preserve world-space attachments and commit as one transaction.
use super::{Document, ObjectId, Transform, objects, persistence};
use crate::engine::level::{LevelDef, LevelSurfaces, WallAxis, WallDef};
use serde_json::{Value, json};

pub fn endpoints(wall: &WallDef) -> [[f32; 3]; 2] {
    let y = wall.y + 0.12;
    match wall.axis() {
        WallAxis::X => [
            [wall.x, y, wall.depth.mul_add(0.5, wall.z)],
            [wall.x + wall.width, y, wall.depth.mul_add(0.5, wall.z)],
        ],
        WallAxis::Z => [
            [wall.width.mul_add(0.5, wall.x), y, wall.z],
            [wall.width.mul_add(0.5, wall.x), y, wall.z + wall.depth],
        ],
    }
}
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.015
}
fn attached(wall: &WallDef, x: f32, z: f32) -> bool {
    x >= wall.x - 0.2
        && x <= wall.x + wall.width + 0.2
        && z >= wall.z - 0.2
        && z <= wall.z + wall.depth + 0.2
}
fn conflict(level: &LevelDef, wall: &WallDef, skip: Option<usize>) -> Result<(), String> {
    for (i, other) in level.walls.iter().enumerate() {
        if Some(i) == skip
            || wall.y >= other.y + other.height.unwrap_or(3.0) - 0.01
            || other.y >= wall.y + wall.height.unwrap_or(3.0) - 0.01
        {
            continue;
        }
        let overlap_x = (wall.x + wall.width).min(other.x + other.width) - wall.x.max(other.x);
        let overlap_z = (wall.z + wall.depth).min(other.z + other.depth) - wall.z.max(other.z);
        if overlap_x > 0.015 && overlap_z > 0.015 {
            let junction = wall.axis() != other.axis()
                && (overlap_x <= wall.thickness().max(other.thickness()) + 0.015
                    && overlap_z <= wall.thickness().max(other.thickness()) + 0.015);
            if !junction {
                return Err(format!(
                    "Wall overlaps Wall {}. Snap to an endpoint or move it clear.",
                    i.saturating_add(1)
                ));
            }
        }
    }
    Ok(())
}
impl Document {
    pub(crate) fn wall_segment_candidate(
        &self,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<(Value, ObjectId), String> {
        if start.into_iter().chain(end).any(|v| !v.is_finite()) {
            return Err("Wall points must be finite".into());
        }
        let dx = (end[0] - start[0]).abs();
        let dz = (end[2] - start[2]).abs();
        if dx.max(dz) < 0.5 {
            return Err("Click an endpoint at least 0.50m from the start".into());
        }
        if dx.max(dz) > 100.0 {
            return Err("Wall segments are limited to 100m".into());
        }
        let along_x = dx >= dz;
        let x = if along_x {
            start[0].min(end[0])
        } else {
            start[0] - 0.1
        };
        let z = if along_x {
            start[2] - 0.1
        } else {
            start[2].min(end[2])
        };
        let floor = start[1];
        let mid = [
            f32::midpoint(start[0], end[0]),
            f32::midpoint(start[2], end[2]),
        ];
        let _room = self
            .level()
            .rooms
            .iter()
            .find(|r| {
                let (left, right, front, back) = r.bounds();
                mid[0] >= left
                    && mid[0] <= right
                    && mid[1] >= front
                    && mid[1] <= back
                    && near(r.floor_y, floor)
                    && start[0] >= left - 0.01
                    && start[0] <= right + 0.01
                    && end[0] >= left - 0.01
                    && end[0] <= right + 0.01
                    && start[2] >= front - 0.01
                    && start[2] <= back + 0.01
                    && end[2] >= front - 0.01
                    && end[2] <= back + 0.01
            })
            .ok_or("Keep the wall on the same room floor; create or extend the room first")?;
        let mut raw = self.raw().clone();
        let records = objects::array_mut(&mut raw, "walls")?;
        let id = ObjectId::Wall(records.len());
        records.push(json!({"x":x,"z":z,"y":floor,"width":if along_x {dx}else{0.2},"depth":if along_x {0.2}else{dz}}));
        let (level, _) = persistence::validate_raw(&raw, self.level_id())?;
        conflict(
            self.level(),
            level.walls.last().ok_or("Missing wall")?,
            None,
        )?;
        Ok((raw, id))
    }

    pub(crate) fn wall_edit_candidate(
        &self,
        index: usize,
        pose: Transform,
    ) -> Result<Value, String> {
        let old = self
            .level()
            .walls
            .get(index)
            .ok_or("Wall no longer exists")?;
        let mut raw = self.raw().clone();
        objects::set_transform(&mut raw, self.level(), ObjectId::Wall(index), pose)?;
        let new: WallDef = serde_json::from_value(
            raw.get("walls")
                .and_then(|a| a.get(index))
                .ok_or("Missing wall")?
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        let delta = [new.x - old.x, new.y - old.y, new.z - old.z];
        let resized = !near(old.width, new.width) || !near(old.depth, new.depth);
        if old.axis() != new.axis() {
            let fitted = has_fittings(self.level(), old);
            if fitted {
                return Err(
                    "Walls with fittings keep their axis. Draw a new segment to turn a corner."
                        .into(),
                );
            }
            conflict(self.level(), &new, Some(index))?;
            let _validated = persistence::validate_raw(&raw, self.level_id())?;
            return Ok(raw);
        }
        let origin_shift = if old.axis() == WallAxis::X {
            delta[0]
        } else {
            delta[2]
        };
        let perpendicular = if old.axis() == WallAxis::X {
            delta[2]
        } else {
            delta[0]
        };
        if resized && (perpendicular.abs() > 0.001 || delta[1].abs() > 0.001) {
            return Err("Resize the length or move the wall in separate steps".into());
        }
        let mut proposed = old.clone();
        proposed.x = new.x;
        proposed.y = new.y;
        proposed.z = new.z;
        proposed.width = new.width;
        proposed.depth = new.depth;
        proposed.height = new.height;
        if proposed.length() < 0.5 {
            return Err("Keep at least 0.50m of wall".into());
        }
        let new_length = proposed.length();
        for opening in &mut proposed.openings {
            if resized {
                opening.offset -= origin_shift;
            }
            if opening.offset < 0.0 || opening.end() > new_length {
                return Err("Resize would cut through an opening. Move the doorway or use the opposite endpoint.".into());
            }
        }
        // Keep existing junctions closed. Unconnected partitions remain freely movable.
        let old_ends = endpoints(old);
        let new_ends = endpoints(&proposed);
        for (i, other) in self.level().walls.iter().enumerate() {
            if i == index || !near(old.y, other.y) {
                continue;
            }
            for (before, after) in old_ends.iter().zip(new_ends) {
                if attached(other, before[0], before[2]) && !attached(other, after[0], after[2]) {
                    return Err(format!(
                        "This endpoint joins Wall {}. Resize the room edge to move a connected perimeter, or drag the free endpoint.",
                        i.saturating_add(1)
                    ));
                }
            }
        }
        conflict(self.level(), &proposed, Some(index))?;
        let record = objects::object_mut(&mut raw, ObjectId::Wall(index))?;
        if resized {
            for opening in record
                .get_mut("openings")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let offset = opening
                    .get("offset")
                    .and_then(Value::as_f64)
                    .ok_or("Missing opening offset")?;
                drop(
                    opening
                        .as_object_mut()
                        .ok_or("Invalid opening")?
                        .insert("offset".into(), json!(offset - f64::from(origin_shift))),
                );
            }
        }
        move_fittings(&mut raw, self.level(), old, &proposed, resized, delta)?;
        let _validated = persistence::validate_raw(&raw, self.level_id())?;
        Ok(raw)
    }

    pub(crate) fn doorway_candidate(
        &self,
        index: usize,
        at: f32,
        door: bool,
    ) -> Result<(Value, ObjectId), String> {
        let wall = self
            .level()
            .walls
            .get(index)
            .ok_or("Click a wall to place a doorway")?;
        let mut raw = self.raw().clone();
        let height = wall.height.unwrap_or_else(|| {
            LevelSurfaces::new(self.level()).clear_ceiling_height_along(wall, at)
        });
        if wall
            .openings
            .iter()
            .any(|o| at >= o.offset && at <= o.end() && o.glass.is_some())
        {
            return Err("This opening contains glazing. Remove or reconfigure its pane before fitting a door.".into());
        }
        let existing = wall
            .openings
            .iter()
            .find(|o| at >= o.offset && at <= o.end() && o.reaches_floor() && o.is_door());
        if existing.is_some() && !door {
            return Err(
                "This wall already has a doorway here. Choose Add Door to fit a functional leaf."
                    .into(),
            );
        }
        let opening = if let Some(o) = existing {
            (o.offset, o.width, o.height)
        } else {
            if wall.length() < 1.2 || height < 2.25 {
                return Err("A doorway needs a wall at least 1.20m long and 2.25m high".into());
            }
            let offset = (at - 0.5).clamp(0.1, wall.length() - 1.1);
            if wall
                .openings
                .iter()
                .any(|o| offset < o.end() + 0.05 && offset + 1.0 > o.offset - 0.05)
            {
                return Err(
                    "Doorway conflicts with another opening. Choose a clear part of the wall."
                        .into(),
                );
            }
            let r = objects::object_mut(&mut raw, ObjectId::Wall(index))?;
            let entries = r
                .entry("openings")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Invalid openings")?;
            entries
                .push(json!({"kind":"door","offset":offset,"width":1.0_f32,"height":2.2_f32,"sill":0.0_f32}));
            (offset, 1.0, 2.2)
        };
        let id = if door {
            fit_door(&mut raw, self.level(), wall, opening.into())?
        } else {
            ObjectId::Wall(index)
        };
        let _validated = persistence::validate_raw(&raw, self.level_id())?;
        Ok((raw, id))
    }
}

fn move_fittings(
    raw: &mut Value,
    level: &LevelDef,
    old: &WallDef,
    proposed: &WallDef,
    resized: bool,
    delta: [f32; 3],
) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    for (i, door) in level.doors.iter().enumerate() {
        let base = surfaces.floor_y_at(door.x, door.z).unwrap_or(0.0) + door.y;
        if attached(old, door.x, door.z)
            && base >= old.y - 0.05
            && base < old.y + old.height.unwrap_or(3.0)
        {
            if resized {
                if !attached(proposed, door.x, door.z) {
                    return Err("Resize would detach a door; move it first".into());
                }
            } else {
                let door_record = objects::object_mut(raw, ObjectId::Door(i))?;
                drop(door_record.insert("x".into(), json!(door.x + delta[0])));
                drop(door_record.insert("z".into(), json!(door.z + delta[2])));
                let new_floor = surfaces
                    .floor_y_at(door.x + delta[0], door.z + delta[2])
                    .ok_or("Move would leave a door outside a room")?;
                drop(door_record.insert("y".into(), json!(base + delta[1] - new_floor)));
            }
        }
    }
    for prop in &level.props {
        if prop.physics.is_none()
            && prop.y > 0.2
            && attached(old, prop.x, prop.z)
            && delta.iter().any(|d| d.abs() > 0.001)
        {
            return Err(format!(
                "Fixed elevated prop {} may be wall mounted. Move or review it first.",
                prop.id.as_ref().unwrap_or(&prop.model)
            ));
        }
    }
    for (i, light) in level.ceiling_lights.iter().enumerate() {
        if light.mount == crate::engine::level::LightMount::Wall
            && attached(old, light.x, light.z)
            && light
                .y
                .is_some_and(|y| y >= old.y && y < old.y + old.height.unwrap_or(3.0))
        {
            if resized && !attached(proposed, light.x, light.z) {
                return Err("Resize would detach a wall light; move it first".into());
            }
            if !resized {
                let r = objects::object_mut(raw, ObjectId::Light(i))?;
                drop(r.insert("x".into(), json!(light.x + delta[0])));
                drop(r.insert("z".into(), json!(light.z + delta[2])));
                drop(r.insert("y".into(), json!(light.y.unwrap_or(old.y) + delta[1])));
            }
        }
    }
    for (i, decal) in level.decals.iter().enumerate() {
        if attached(old, decal.x, decal.z)
            && decal.surface.normal()[1].abs() < 0.5
            && decal.y >= old.y
            && decal.y < old.y + old.height.unwrap_or(3.0)
        {
            if resized && !attached(proposed, decal.x, decal.z) {
                return Err("Resize would detach a decal; move it first".into());
            }
            if !resized {
                let r = raw
                    .get_mut("decals")
                    .and_then(Value::as_array_mut)
                    .and_then(|r| r.get_mut(i))
                    .and_then(Value::as_object_mut)
                    .ok_or("Missing decal")?;
                for (k, v) in [
                    ("x", decal.x + delta[0]),
                    ("y", decal.y + delta[1]),
                    ("z", decal.z + delta[2]),
                ] {
                    drop(r.insert(k.into(), json!(v)));
                }
            }
        }
    }
    Ok(())
}

fn has_fittings(level: &LevelDef, old: &WallDef) -> bool {
    !old.openings.is_empty()
        || level.doors.iter().any(|d| attached(old, d.x, d.z))
        || level
            .ceiling_lights
            .iter()
            .any(|l| l.mount == crate::engine::level::LightMount::Wall && attached(old, l.x, l.z))
        || level
            .decals
            .iter()
            .any(|d| d.surface.normal()[1].abs() < 0.5 && attached(old, d.x, d.z))
        || level
            .props
            .iter()
            .any(|p| p.y > 0.2 && attached(old, p.x, p.z))
}

fn fit_door(
    raw: &mut Value,
    level: &LevelDef,
    wall: &WallDef,
    [offset, width, opening_height]: [f32; 3],
) -> Result<ObjectId, String> {
    if width < 0.65 || opening_height < 1.9 {
        return Err("This opening is too small for a functional door".into());
    }
    let (x, z, yaw) = match wall.axis() {
        WallAxis::X => (
            wall.x + offset + 0.04,
            f32::mul_add(wall.depth, 0.5, wall.z),
            0.0,
        ),
        WallAxis::Z => (
            f32::mul_add(wall.width, 0.5, wall.x),
            wall.z + offset + width - 0.04,
            90.0,
        ),
    };
    if level
        .doors
        .iter()
        .any(|d| (d.x - x).abs() < 0.2 && (d.z - z).abs() < 0.2)
    {
        return Err("A functional door is already fitted here".into());
    }
    let floor = LevelSurfaces::new(level)
        .floor_y_at(x, z)
        .ok_or("Door hinge needs a room floor")?;
    let id = objects::add(raw, level, super::ObjectKind::Door, [x, 0.0, z], None)?;
    let r = objects::object_mut(raw, id)?;
    for (key, value) in [
        ("rotation_degrees", yaw),
        ("width", width - 0.08),
        ("height", opening_height - 0.05),
        ("y", wall.y - floor),
    ] {
        drop(r.insert(key.into(), json!(value)));
    }
    drop(r.insert("bidirectional".into(), json!(true)));
    Ok(id)
}

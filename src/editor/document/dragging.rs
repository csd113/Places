//! A room drag moves an unambiguous isolated room and its contained authoring
//! records together. Shared/stacked rooms and coordinate-bearing scripts are
//! refused with a specific reason instead of silently breaking relationships.
use super::{Document, ObjectId, Transform, objects};
use serde_json::{Value, json};
fn move_contained_records(raw: &mut Value, bounds: (f32, f32, f32, f32), delta: [f32; 3]) {
    let (left, right, near, far) = bounds;
    for key in [
        "props",
        "walls",
        "ceiling_lights",
        "doors",
        "sound_emitters",
        "volumes",
        "spawn_points",
    ] {
        if let Some(records) = raw.get_mut(key).and_then(Value::as_array_mut) {
            for record in records {
                let px = record.get("x").and_then(Value::as_f64).unwrap_or(0.0_f64);
                let pz = record.get("z").and_then(Value::as_f64).unwrap_or(0.0_f64);
                if px < f64::from(left) - 0.01_f64
                    || px > f64::from(right) + 0.01_f64
                    || pz < f64::from(near) - 0.01_f64
                    || pz > f64::from(far) + 0.01_f64
                {
                    continue;
                }
                let Some(object) = record.as_object_mut() else {
                    continue;
                };
                drop(object.insert("x".into(), json!(px + f64::from(delta[0]))));
                drop(object.insert("z".into(), json!(pz + f64::from(delta[2]))));
                let vertical = if matches!(key, "props" | "doors" | "spawn_points") {
                    &[][..]
                } else {
                    &["y", "bottom_y", "top_y"][..]
                };
                for name in vertical {
                    if let Some(value) = object.get(*name).and_then(Value::as_f64) {
                        drop(object.insert((*name).into(), json!(value + f64::from(delta[1]))));
                    }
                }
            }
        }
    }
}
fn move_fog(raw: &mut Value, bounds: (f32, f32, f32, f32), delta: [f32; 3]) -> Result<(), String> {
    let (left, right, near, far) = bounds;
    for (key, z_axis) in [("fog_regions", 2_usize), ("ground_fog", 1_usize)] {
        let Some(records) = raw.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        for record in records {
            let read = |corner: &str, axis| {
                record
                    .get(corner)
                    .and_then(|v| v.get(axis))
                    .and_then(Value::as_f64)
                    .ok_or("Invalid fog box")
            };
            let (x0, x1, z0, z1) = (
                read("min", 0)?,
                read("max", 0)?,
                read("min", z_axis)?,
                read("max", z_axis)?,
            );
            if x0 >= f64::from(right)
                || x1 <= f64::from(left)
                || z0 >= f64::from(far)
                || z1 <= f64::from(near)
            {
                continue;
            }
            if x0 < f64::from(left)
                || x1 > f64::from(right)
                || z0 < f64::from(near)
                || z1 > f64::from(far)
            {
                return Err("A fog box crosses this room boundary. Review its ownership before moving the room.".into());
            }
            for corner in ["min", "max"] {
                let values = record
                    .get_mut(corner)
                    .and_then(Value::as_array_mut)
                    .ok_or("Invalid fog box")?;
                for (axis, value) in values.iter_mut().enumerate() {
                    let component = if z_axis == 1 && axis == 1 { 2 } else { axis };
                    *value = json!(
                        value.as_f64().ok_or("Invalid fog coordinate")?
                            + f64::from(*delta.get(component).ok_or("Invalid fog axis")?)
                    );
                }
            }
            let object = record.as_object_mut().ok_or("Invalid fog record")?;
            for name in ["ground_y", "top_y"] {
                if let Some(value) = object.get(name).and_then(Value::as_f64) {
                    drop(object.insert(name.into(), json!(value + f64::from(delta[1]))));
                }
            }
        }
    }
    Ok(())
}

fn check_crossing_objects(
    level: &crate::engine::level::LevelDef,
    room: &crate::engine::level::RoomDef,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<(), String> {
    let (left, right, near, far) = room.bounds();
    let intersects = |b: (f32, f32, f32, f32)| b.0 < right && b.1 > left && b.2 < far && b.3 > near;
    let contains = |b: (f32, f32, f32, f32)| {
        b.0 >= left - 0.01 && b.1 <= right + 0.01 && b.2 >= near - 0.01 && b.3 <= far + 0.01
    };
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    let crosses = level
        .walls
        .iter()
        .map(|w| (w.x, w.x + w.width, w.z, w.z + w.depth))
        .chain(
            level
                .volumes
                .iter()
                .map(|v| (v.x, v.x + v.width, v.z, v.z + v.depth)),
        )
        .chain(level.props.iter().map(|prop| {
            let size = prop
                .size
                .or_else(|| catalog.placeable(&prop.model).and_then(|entry| entry.size))
                .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE);
            let envelope =
                prop.placement_bounds(surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0), size);
            (
                envelope.min[0],
                envelope.max[0],
                envelope.min[2],
                envelope.max[2],
            )
        }))
        .any(|b| intersects(b) && !contains(b));
    if crosses {
        return Err("A wall, trigger or prop crosses this room boundary. Review its ownership before moving the room.".into());
    }
    Ok(())
}

impl Document {
    pub(in crate::editor) fn drag_candidate(
        &self,
        id: ObjectId,
        pose: Transform,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<Value, String> {
        let mut raw = self.raw().clone();
        if let ObjectId::Room(index) = id {
            let room = self
                .level()
                .rooms
                .get(index)
                .ok_or("Room no longer exists")?;
            let (left, right, near, far) = room.bounds();
            if self.level().rooms.iter().enumerate().any(|(i, r)| {
                let (a, b, c, d) = r.bounds();
                i != index
                    && a <= right + 0.01
                    && b >= left - 0.01
                    && c <= far + 0.01
                    && d >= near - 0.01
            }) {
                return Err("This room shares a boundary or storey. Move its related rooms explicitly with numeric controls; an isolated-room drag cannot infer that group.".into());
            }
            for key in [
                "ramps",
                "stairs",
                "floor_regions",
                "floor_patches",
                "water",
                "decals",
                "routes",
                "scripts",
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
                    .is_some_and(|a| !a.is_empty())
                {
                    return Err(format!(
                        "Room drag needs review of {key}; use precise room controls to preserve those authored relationships."
                    ));
                }
            }
            check_crossing_objects(self.level(), room, catalog)?;
            let [x, y, z] = pose.position;
            let delta = [x - room.x, y - room.floor_y, z - room.z];
            move_contained_records(&mut raw, room.bounds(), delta);
            move_fog(&mut raw, room.bounds(), delta)?;
            if let Some(spawn) = raw.get_mut("spawn").and_then(Value::as_object_mut)
                && spawn
                    .get("x")
                    .and_then(Value::as_f64)
                    .is_some_and(|spawn_x| {
                        spawn_x >= f64::from(left) && spawn_x <= f64::from(right)
                    })
                && spawn
                    .get("z")
                    .and_then(Value::as_f64)
                    .is_some_and(|spawn_z| spawn_z >= f64::from(near) && spawn_z <= f64::from(far))
            {
                for (key, distance) in [("x", delta[0]), ("z", delta[2])] {
                    if let Some(value) = spawn.get(key).and_then(Value::as_f64) {
                        drop(spawn.insert(key.into(), json!(value + f64::from(distance))));
                    }
                }
            }
        }
        objects::set_transform(&mut raw, self.level(), id, pose)?;
        let _validated = super::persistence::validate_raw(&raw, self.level_id())?;
        Ok(raw)
    }
    pub(in crate::editor) fn apply_drag(
        &mut self,
        id: ObjectId,
        pose: Transform,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<(), String> {
        let candidate = self.drag_candidate(id, pose, catalog)?;
        self.accept_edit(candidate)
    }
}

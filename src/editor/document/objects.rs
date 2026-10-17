use serde_json::{Map, Value};

use crate::engine::level::{LevelDef, LevelSurfaces};

use super::geometry;

/// Supported editor collections. Other source collections remain untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Room,
    Wall,
    Prop,
    Ramp,
    FloorRegion,
    Light,
    Audio,
    Entity,
    Door,
    Trigger,
    FogRegion,
    GroundFog,
    SpawnPoint,
}

/// A selection addresses an array entry in the current document revision.
/// The UI clears/remaps selections after structural edits and history changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectId {
    Room(usize),
    Wall(usize),
    Prop(usize),
    Ramp(usize),
    FloorRegion(usize),
    Light(usize),
    Audio(usize),
    Door(usize),
    Trigger(usize),
    FogRegion(usize),
    GroundFog(usize),
    SpawnPoint(usize),
}

impl ObjectId {
    pub(crate) const fn collection(self) -> (&'static str, usize) {
        match self {
            Self::Room(index) => ("rooms", index),
            Self::Wall(index) => ("walls", index),
            Self::Prop(index) => ("props", index),
            Self::Ramp(index) => ("ramps", index),
            Self::FloorRegion(index) => ("floor_regions", index),
            Self::Light(index) => ("ceiling_lights", index),
            Self::Audio(index) => ("sound_emitters", index),
            Self::Door(index) => ("doors", index),
            Self::Trigger(index) => ("volumes", index),
            Self::FogRegion(index) => ("fog_regions", index),
            Self::GroundFog(index) => ("ground_fog", index),
            Self::SpawnPoint(index) => ("spawn_points", index),
        }
    }
}

/// Scalar authoring transform. Room/wall positions are minimum corners; a
/// prop's position is its floor-relative base and size is uniform scale.
///
/// Ramp/region Y is room-relative offset; ramp size Y is signed rise.
/// A region has no independent thickness, so its size Y is a fixed 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub size: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub roll_degrees: f32,
}

/// A concise object-list entry with the same IDs used by viewport picking.
#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub id: ObjectId,
    pub label: String,
    pub transform: Transform,
}

pub(super) fn enumerate(level: &LevelDef) -> Vec<ObjectInfo> {
    let rooms = level.rooms.iter().enumerate().map(|(index, _)| {
        (
            ObjectId::Room(index),
            format!("Floor {}", index.saturating_add(1)),
        )
    });
    let walls = level.walls.iter().enumerate().map(|(index, _)| {
        (
            ObjectId::Wall(index),
            format!("Wall {}", index.saturating_add(1)),
        )
    });
    let props = level.props.iter().enumerate().map(|(index, prop)| {
        (
            ObjectId::Prop(index),
            prop.id.as_ref().unwrap_or(&prop.model).clone(),
        )
    });
    let ramps = level.ramps.iter().enumerate().map(|(index, _)| {
        (
            ObjectId::Ramp(index),
            format!("Ramp {}", index.saturating_add(1)),
        )
    });
    let regions = level.floor_regions.iter().enumerate().map(|(index, _)| {
        (
            ObjectId::FloorRegion(index),
            format!("Region {}", index.saturating_add(1)),
        )
    });
    rooms
        .chain(walls)
        .chain(props)
        .chain(ramps)
        .chain(regions)
        .filter_map(|(id, label)| {
            transform(level, id).ok().map(|transform| ObjectInfo {
                id,
                label,
                transform,
            })
        })
        .chain(super::gameplay::enumerate(level))
        .collect()
}

pub(super) fn transform(level: &LevelDef, id: ObjectId) -> Result<Transform, String> {
    if matches!(
        id,
        ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_)
            | ObjectId::SpawnPoint(_)
    ) {
        return super::gameplay::transform(level, id);
    }
    match id {
        ObjectId::Room(index) => level.rooms.get(index).map(|room| Transform {
            position: [room.x, room.floor_y, room.z],
            size: [room.width, room.height, room.depth],
            yaw_degrees: 0.0_f32,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        }),
        ObjectId::Wall(index) => level.walls.get(index).map(|wall| Transform {
            position: [wall.x, wall.y, wall.z],
            size: [
                wall.width,
                wall.height.unwrap_or_else(|| {
                    LevelSurfaces::new(level)
                        .clear_ceiling_height_along(wall, wall.length() * 0.5_f32)
                }),
                wall.depth,
            ],
            yaw_degrees: 0.0_f32,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        }),
        ObjectId::Prop(index) => level.props.get(index).map(|prop| Transform {
            position: [prop.x, prop.y, prop.z],
            size: [prop.scale; 3],
            yaw_degrees: prop.rotation_degrees,
            pitch_degrees: prop.pitch_degrees,
            roll_degrees: prop.roll_degrees,
        }),
        ObjectId::Ramp(index) => level.ramps.get(index).map(|ramp| Transform {
            position: [ramp.x, ramp.offset_y, ramp.z],
            size: [ramp.width, ramp.rise, ramp.depth],
            yaw_degrees: 0.0_f32,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        }),
        ObjectId::FloorRegion(index) => level.floor_regions.get(index).map(|region| Transform {
            position: [region.x, region.offset_y, region.z],
            size: [region.width, 1.0_f32, region.depth],
            yaw_degrees: 0.0_f32,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        }),
        ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => None,
    }
    .ok_or_else(|| "The selected object no longer exists".into())
}

pub(super) fn set_transform(
    raw: &mut Value,
    level: &LevelDef,
    id: ObjectId,
    proposed: Transform,
) -> Result<(), String> {
    if !proposed
        .position
        .iter()
        .chain(&proposed.size)
        .all(|value| value.is_finite())
        || !proposed.yaw_degrees.is_finite()
        || !proposed.pitch_degrees.is_finite()
        || !proposed.roll_degrees.is_finite()
        || proposed.size.iter().enumerate().any(|(axis, value)| {
            *value <= 0.0_f32 && !(matches!(id, ObjectId::Ramp(_)) && axis == 1)
        })
    {
        return Err("Transform requires finite coordinates and positive sizes; only ramp rise may be negative".into());
    }
    if !matches!(id, ObjectId::Prop(_))
        && (proposed.pitch_degrees != 0.0 || proposed.roll_degrees != 0.0)
    {
        return Err("Pitch and roll belong to prop placements".into());
    }
    if matches!(
        id,
        ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_)
            | ObjectId::SpawnPoint(_)
    ) {
        return super::gameplay::set_transform(raw, level, id, proposed);
    }
    let current = transform(level, id)?;
    let object = object_mut(raw, id)?;
    match id {
        ObjectId::Prop(_) => {
            let [scale, scale_y, scale_z] = proposed.size;
            if (scale - scale_y).abs() > 0.0001_f32 || (scale - scale_z).abs() > 0.0001_f32 {
                return Err("Props support uniform scale; set all scale axes equally".into());
            }
            set_position(object, proposed.position, "y");
            set_number(object, "scale", scale);
            set_number(object, "rotation_degrees", proposed.yaw_degrees);
            set_number(object, "pitch_degrees", proposed.pitch_degrees);
            set_number(object, "roll_degrees", proposed.roll_degrees);
        }
        ObjectId::Room(_) | ObjectId::Wall(_) => {
            let mut actual = proposed;
            let quarters = proposed.yaw_degrees / 90.0_f32;
            if (quarters - quarters.round()).abs() > 0.0001_f32 {
                return Err("Floors and walls use 90 degree footprint rotations".into());
            }
            if proposed.yaw_degrees.abs() > 0.0001_f32 {
                require_simple_rotation(object, id)?;
                if quarters.rem_euclid(2.0_f32).abs() > 0.0001_f32 {
                    let [width, height, depth] = proposed.size;
                    let [x, y, z] = proposed.position;
                    actual.position = [
                        (width - depth).mul_add(0.5_f32, x),
                        y,
                        (depth - width).mul_add(0.5_f32, z),
                    ];
                    actual.size = [depth, height, width];
                }
            }
            let vertical_key = if matches!(id, ObjectId::Room(_)) {
                "floor_y"
            } else {
                "y"
            };
            set_position(object, actual.position, vertical_key);
            let [width, height, depth] = actual.size;
            set_number(object, "width", width);
            set_number(object, "depth", depth);
            // Omitted wall height follows the engine's local ceiling. A move
            // must not turn that profile into an arbitrary fixed-height wall.
            if matches!(id, ObjectId::Room(_))
                || object.get("height").is_some_and(|value| !value.is_null())
                || (height - current.size.get(1).copied().unwrap_or(height)).abs() > 0.0001_f32
            {
                set_number(object, "height", height);
            }
        }
        ObjectId::Ramp(_) | ObjectId::FloorRegion(_) => {
            geometry::set_transform(object, id, proposed)?;
        }
        ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => return Err("Unsupported geometry transform".into()),
    }
    Ok(())
}

fn require_simple_rotation(object: &Map<String, Value>, id: ObjectId) -> Result<(), String> {
    let complex = match id {
        ObjectId::Room(_) => {
            object
                .get("ceiling")
                .and_then(|ceiling| ceiling.get("kind"))
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "flat" && kind != "open")
                || object.contains_key("ceiling_tile_origin")
                || object.contains_key("ceiling_tile_rotation_degrees")
        }
        ObjectId::Wall(_) => ["openings", "faces", "face_shine"].iter().any(|key| {
            object.get(*key).is_some_and(|value| match value {
                Value::Array(entries) => !entries.is_empty(),
                Value::Object(entries) => !entries.is_empty(),
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
            })
        }),
        ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => false,
    };
    if complex {
        return Err(
            "Rotation of profiled geometry or per-face/opening data requires later editor tools"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn add(
    raw: &mut Value,
    level: &LevelDef,
    kind: ObjectKind,
    position: [f32; 3],
    asset: Option<&str>,
) -> Result<ObjectId, String> {
    if position.iter().any(|value| !value.is_finite()) {
        return Err("Object coordinates must be finite".into());
    }
    if matches!(
        kind,
        ObjectKind::Light
            | ObjectKind::Audio
            | ObjectKind::Entity
            | ObjectKind::Door
            | ObjectKind::Trigger
            | ObjectKind::FogRegion
            | ObjectKind::GroundFog
            | ObjectKind::SpawnPoint
    ) {
        return super::gameplay::add(raw, level, kind, position, asset);
    }
    let [x, y, z] = position;
    let (collection, mut object) = match kind {
        ObjectKind::Room => (
            "rooms",
            serde_json::json!({"x":x, "floor_y":y, "z":z, "width":4.0_f32, "depth":4.0_f32, "height":2.7_f32}),
        ),
        ObjectKind::Wall => (
            "walls",
            serde_json::json!({"x":x, "y":y, "z":z, "width":4.0_f32, "depth":0.2_f32, "height":2.7_f32}),
        ),
        ObjectKind::Prop => {
            pin_prop_ids(raw, level)?;
            (
                "props",
                serde_json::json!({"id":unique_prop_id(raw)?, "model":asset.unwrap_or("core:chair"), "x":x, "y":y, "z":z, "solid":true}),
            )
        }
        ObjectKind::Ramp => (
            "ramps",
            serde_json::json!({"x":x, "offset_y":y, "z":z, "width":3.0_f32, "depth":1.0_f32, "rise":0.5_f32}),
        ),
        ObjectKind::FloorRegion => (
            "floor_regions",
            serde_json::json!({"x":x, "offset_y":y, "z":z, "width":2.0_f32, "depth":2.0_f32}),
        ),
        ObjectKind::Light
        | ObjectKind::Audio
        | ObjectKind::Entity
        | ObjectKind::Door
        | ObjectKind::Trigger
        | ObjectKind::FogRegion
        | ObjectKind::GroundFog
        | ObjectKind::SpawnPoint => return Err("Unsupported geometry kind".into()),
    };
    if kind != ObjectKind::Prop
        && let Some(material) = asset
    {
        drop(
            object
                .as_object_mut()
                .ok_or("Invalid object")?
                .insert("material".into(), material.into()),
        );
    }
    let entries = array_mut(raw, collection)?;
    let index = entries.len();
    entries.push(object);
    Ok(match kind {
        ObjectKind::Room => ObjectId::Room(index),
        ObjectKind::Wall => ObjectId::Wall(index),
        ObjectKind::Prop => ObjectId::Prop(index),
        ObjectKind::Ramp => ObjectId::Ramp(index),
        ObjectKind::FloorRegion => ObjectId::FloorRegion(index),
        ObjectKind::Light
        | ObjectKind::Audio
        | ObjectKind::Entity
        | ObjectKind::Door
        | ObjectKind::Trigger
        | ObjectKind::FogRegion
        | ObjectKind::GroundFog
        | ObjectKind::SpawnPoint => return Err("Unsupported geometry kind".into()),
    })
}

pub(super) fn delete(raw: &mut Value, level: &LevelDef, id: ObjectId) -> Result<(), String> {
    super::gameplay::pin_gameplay_ids(raw, level)?;
    if matches!(id, ObjectId::Prop(_)) {
        pin_prop_ids(raw, level)?;
    }
    let (collection, index) = id.collection();
    let entries = array_mut(raw, collection)?;
    if index >= entries.len() {
        return Err("The selected object no longer exists".into());
    }
    drop(entries.remove(index));
    Ok(())
}

pub(super) fn duplicate(
    raw: &mut Value,
    level: &LevelDef,
    id: ObjectId,
) -> Result<ObjectId, String> {
    super::gameplay::pin_gameplay_ids(raw, level)?;
    if matches!(id, ObjectId::FogRegion(_) | ObjectId::GroundFog(_)) {
        return super::fog::duplicate(raw, level, id);
    }
    if let ObjectId::Prop(index) = id {
        let _prop = level.props.get(index).ok_or("Prop no longer exists")?;
        pin_prop_ids(raw, level)?;
    }
    let mut object = object_mut(raw, id)?.clone();
    drop(object.remove("editor_instance_id"));
    let current = transform(level, id)?;
    let [x, y, z] = current.position;
    set_position(
        &mut object,
        [x + 0.5_f32, y, z + 0.5_f32],
        match id {
            ObjectId::Room(_) => "floor_y",
            ObjectId::Ramp(_) | ObjectId::FloorRegion(_) => "offset_y",
            ObjectId::Wall(_)
            | ObjectId::Prop(_)
            | ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_)
            | ObjectId::SpawnPoint(_) => "y",
        },
    );
    if matches!(id, ObjectId::Prop(_)) {
        drop(object.insert("id".into(), unique_prop_id(raw)?.into()));
    } else if matches!(
        id,
        ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::SpawnPoint(_)
    ) {
        drop(object.insert(
            "id".into(),
            super::gameplay::unique_id(raw, "editor_copy")?.into(),
        ));
    }
    if matches!(id, ObjectId::Trigger(_)) {
        drop(object.remove("y"));
        set_number(&mut object, "bottom_y", y);
        set_number(&mut object, "top_y", y + current.size[1]);
    }
    if let ObjectId::SpawnPoint(point_index) = id
        && let Some(group_id) = level
            .spawn_points
            .get(point_index)
            .and_then(|point| point.group.as_deref())
        && let Some(group) = level.spawn_groups.iter().find(|group| group.id == group_id)
    {
        let copy_group = super::gameplay::unique_id(raw, "editor_spawn_group")?;
        array_mut(raw, "spawn_groups")?.push(
            serde_json::json!({"id":copy_group,"at_most_one_active":group.at_most_one_active}),
        );
        drop(object.insert("group".into(), copy_group.into()));
    }
    let (collection, _) = id.collection();
    let entries = array_mut(raw, collection)?;
    let index = entries.len();
    entries.push(Value::Object(object));
    Ok(match id {
        ObjectId::Room(_) => ObjectId::Room(index),
        ObjectId::Wall(_) => ObjectId::Wall(index),
        ObjectId::Prop(_) => ObjectId::Prop(index),
        ObjectId::Ramp(_) => ObjectId::Ramp(index),
        ObjectId::FloorRegion(_) => ObjectId::FloorRegion(index),
        ObjectId::Light(_) => ObjectId::Light(index),
        ObjectId::Audio(_) => ObjectId::Audio(index),
        ObjectId::Door(_) => ObjectId::Door(index),
        ObjectId::Trigger(_) => ObjectId::Trigger(index),
        ObjectId::FogRegion(_) => ObjectId::FogRegion(index),
        ObjectId::GroundFog(_) => ObjectId::GroundFog(index),
        ObjectId::SpawnPoint(_) => ObjectId::SpawnPoint(index),
    })
}

pub(super) fn require_passive_prop(level: &LevelDef, index: usize) -> Result<(), String> {
    let prop = level
        .props
        .get(index)
        .ok_or("The selected prop no longer exists")?;
    if !prop.components.is_empty()
        || !prop.bindings.is_empty()
        || prop.physics.is_some()
        || prop.float.is_some()
        || !prop.lights.is_empty()
        || prop.span.is_some()
    {
        return Err(
            "Duplicating or replacing active/fitted props requires later editor tools".into(),
        );
    }
    Ok(())
}

pub(super) fn pin_prop_ids(raw: &mut Value, level: &LevelDef) -> Result<(), String> {
    for (index, id) in level.prop_instance_ids().into_iter().enumerate() {
        let object = object_mut(raw, ObjectId::Prop(index))?;
        if object
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            drop(object.insert("id".into(), id.into()));
        }
    }
    Ok(())
}

fn unique_prop_id(raw: &Value) -> Result<String, String> {
    // The shared validator checks the complete namespace. Searching the raw
    // tree also avoids collisions with unsupported future instance records.
    for index in 1_u32..=1_000_000 {
        let candidate = format!("editor_prop_{index}");
        if !has_id(raw, &candidate) {
            return Ok(candidate);
        }
    }
    Err("No free editor prop identity remains".into())
}

fn has_id(value: &Value, candidate: &str) -> bool {
    match value {
        Value::Array(entries) => entries.iter().any(|entry| has_id(entry, candidate)),
        Value::Object(entries) => {
            entries.get("id").and_then(Value::as_str) == Some(candidate)
                || entries.values().any(|entry| has_id(entry, candidate))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

pub(super) fn set_string(
    raw: &mut Value,
    id: ObjectId,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if value.trim().is_empty() || value.starts_with("pack:") {
        return Err("Choose a built-in asset identity".into());
    }
    drop(object_mut(raw, id)?.insert(key.into(), value.into()));
    Ok(())
}

pub(super) fn set_prop_collision(
    raw: &mut Value,
    id: ObjectId,
    size: [f32; 3],
    solid: bool,
) -> Result<(), String> {
    if !matches!(id, ObjectId::Prop(_)) {
        return Err("Collision dimensions belong to a prop".into());
    }
    let object = object_mut(raw, id)?;
    drop(object.insert("size".into(), serde_json::json!(size)));
    drop(object.insert("solid".into(), solid.into()));
    Ok(())
}

pub(super) fn set_position(
    object: &mut Map<String, Value>,
    position: [f32; 3],
    vertical_key: &str,
) {
    let [x, y, z] = position;
    set_number(object, "x", x);
    set_number(object, vertical_key, y);
    set_number(object, "z", z);
}

pub(super) fn set_number(object: &mut Map<String, Value>, key: &str, value: f32) {
    drop(object.insert(key.into(), Value::from(value)));
}

pub(super) fn object_mut(raw: &mut Value, id: ObjectId) -> Result<&mut Map<String, Value>, String> {
    let (collection, index) = id.collection();
    array_mut(raw, collection)?
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "The selected object no longer exists".into())
}

pub(super) fn array_mut<'a>(
    raw: &'a mut Value,
    collection: &str,
) -> Result<&'a mut Vec<Value>, String> {
    let root = raw.as_object_mut().ok_or("Scene must be a JSON object")?;
    root.entry(collection)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| format!("Unsupported scene collection: {collection}"))
}

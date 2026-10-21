//! Gameplay authoring mutates the authoritative source through shared validation.

use serde_json::{Value, json};

use crate::engine::level::{LevelDef, LevelSurfaces};

use super::objects::{array_mut, object_mut, set_number, set_position};
use super::{ObjectId, ObjectInfo, ObjectKind, Transform};
mod behavior;
mod properties;
mod shadow_only;
mod triggers;
pub(super) fn validate_drafts(raw: &Value) -> Result<(), String> {
    triggers::validate_drafts(raw)
}

/// A focused property supported by the contextual gameplay inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplaySetting {
    Brightness,
    Range,
    Red,
    Green,
    Blue,
    Emission,
    Enabled,
    Switchable,
    Gain,
    AudioNear,
    AudioFar,
    Loop,
    Playing,
    Spatial,
    Physics,
    Pickup,
    Mass,
    Solid,
    ShadowOnly,
    Bidirectional,
    Locked,
    Swing,
    Speed,
    Once,
    Cooldown,
    Aggregate,
    ExitEvent,
    FogDensity,
    FogFalloff,
    Lifetime,
}

impl GameplaySetting {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Brightness => "brightness",
            Self::Range => "range",
            Self::Red | Self::Green | Self::Blue => "color",
            Self::Emission => "emission",
            Self::Enabled => "enabled",
            Self::Switchable => "switchable",
            Self::Gain => "gain",
            Self::AudioNear | Self::AudioFar => "attenuation",
            Self::Loop => "looped",
            Self::Playing => "playing",
            Self::Spatial => "spatial",
            Self::Physics => "physics",
            Self::Pickup => "pickup",
            Self::Mass => "mass_kg",
            Self::Solid => "solid",
            Self::ShadowOnly => "shadow_only",
            Self::Bidirectional => "bidirectional",
            Self::Locked => "locked",
            Self::Swing => "swing_degrees",
            Self::Speed => "open_speed_degrees",
            Self::Once => "once",
            Self::Cooldown => "cooldown_seconds",
            Self::Aggregate => "occupancy",
            Self::ExitEvent => "on",
            Self::FogDensity => "density",
            Self::FogFalloff => "falloff_m",
            Self::Lifetime => "lifetime_seconds",
        }
    }
}

pub(super) fn enumerate(level: &LevelDef) -> Vec<ObjectInfo> {
    let records = level
        .ceiling_lights
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                ObjectId::Light(i),
                v.id.as_ref().unwrap_or(&v.fixture).clone(),
            )
        })
        .chain(
            level
                .sound_emitters
                .iter()
                .enumerate()
                .map(|(i, v)| (ObjectId::Audio(i), format!("Sound / {}", v.id))),
        )
        .chain(
            level
                .doors
                .iter()
                .enumerate()
                .map(|(i, v)| (ObjectId::Door(i), format!("Door / {}", v.id))),
        )
        .chain(level.volumes.iter().enumerate().map(|(i, v)| {
            (
                ObjectId::Trigger(i),
                format!(
                    "Trigger / {}",
                    v.id.clone()
                        .unwrap_or_else(|| format!("{}", i.saturating_add(1)))
                ),
            )
        }));
    let all_records = records.chain(
        level
            .spawn_points
            .iter()
            .enumerate()
            .map(|(i, v)| (ObjectId::SpawnPoint(i), format!("Spawn / {}", v.id))),
    );
    let mut result: Vec<_> = all_records
        .filter_map(|(id, label)| {
            transform(level, id).ok().map(|transform| ObjectInfo {
                id,
                label,
                transform,
            })
        })
        .collect();
    result.extend(super::fog::enumerate(level));
    result
}

const fn scalar_transform(position: [f32; 3], size: [f32; 3], yaw_degrees: f32) -> Transform {
    Transform {
        position,
        size,
        yaw_degrees,
        pitch_degrees: 0.0_f32,
        roll_degrees: 0.0_f32,
    }
}

pub(super) fn transform(level: &LevelDef, id: ObjectId) -> Result<Transform, String> {
    let value = match id {
        ObjectId::Light(i) => level.ceiling_lights.get(i).map(|v| {
            let y =
                v.y.unwrap_or_else(|| LevelSurfaces::new(level).ceiling_y_at(v.x, v.z));
            scalar_transform([v.x, y, v.z], [0.5_f32; 3], v.rotation_degrees)
        }),
        ObjectId::SpawnPoint(i) => level.spawn_points.get(i).map(|v| {
            scalar_transform(
                [
                    v.x,
                    v.y.unwrap_or_else(|| {
                        LevelSurfaces::new(level)
                            .floor_y_at(v.x, v.z)
                            .unwrap_or(0.0_f32)
                    }),
                    v.z,
                ],
                [0.5_f32; 3],
                v.yaw_degrees,
            )
        }),
        ObjectId::Audio(i) => level
            .sound_emitters
            .get(i)
            .map(|v| scalar_transform([v.x, v.y, v.z], [0.5_f32; 3], 0.0_f32)),
        ObjectId::Door(i) => level.doors.get(i).map(|v| {
            scalar_transform(
                [v.x, v.y, v.z],
                [v.width, v.height, v.thickness],
                v.rotation_degrees,
            )
        }),
        ObjectId::Trigger(i) => level.volumes.get(i).map(|v| {
            let (bottom, top) = v.resolved_y_bounds(level);
            scalar_transform(
                [v.x, bottom, v.z],
                [v.width, top - bottom, v.depth],
                0.0_f32,
            )
        }),
        ObjectId::FogRegion(_) | ObjectId::GroundFog(_) => return super::fog::transform(level, id),
        ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_) => None,
    };
    value.ok_or_else(|| "The selected gameplay object no longer exists".into())
}

pub(super) fn set_transform(
    raw: &mut Value,
    level: &LevelDef,
    id: ObjectId,
    value: Transform,
) -> Result<(), String> {
    if matches!(id, ObjectId::FogRegion(_) | ObjectId::GroundFog(_)) {
        return super::fog::set_transform(raw, level, id, value);
    }
    if value.pitch_degrees.abs() > 0.0001_f32 || value.roll_degrees.abs() > 0.0001_f32 {
        return Err("This object stays upright; props support pitch and roll".into());
    }
    let object = object_mut(raw, id)?;
    match id {
        ObjectId::Light(index) => {
            let _light = level
                .ceiling_lights
                .get(index)
                .ok_or("Light no longer exists")?;
            set_position(object, value.position, "y");
            set_number(object, "rotation_degrees", value.yaw_degrees);
        }
        ObjectId::SpawnPoint(_) => {
            set_position(object, value.position, "y");
            set_number(object, "yaw_degrees", value.yaw_degrees);
        }
        ObjectId::Audio(_) => set_position(object, value.position, "y"),
        ObjectId::Door(_) => {
            set_position(object, value.position, "y");
            set_number(object, "rotation_degrees", value.yaw_degrees);
            for (key, number) in ["width", "height", "thickness"].into_iter().zip(value.size) {
                set_number(object, key, number);
            }
        }
        ObjectId::Trigger(_) => {
            if value.yaw_degrees.abs() > 0.0001_f32 {
                return Err("Trigger volumes use axis-aligned boxes".into());
            }
            set_position(object, value.position, "bottom_y");
            set_number(object, "top_y", value.position[1] + value.size[1]);
            set_number(object, "width", value.size[0]);
            set_number(object, "depth", value.size[2]);
        }
        ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_) => return Err("Choose a gameplay object".into()),
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
    let [x, y, z] = position;
    let id = unique_id(raw, "editor")?;
    let (collection, object, constructor): (&str, Value, fn(usize) -> ObjectId) = match kind {
        ObjectKind::Light => (
            "ceiling_lights",
            json!({"id":id,"fixture":asset.unwrap_or("core:fluorescent_panel_01"),"x":x,"z":z,"align":"none","brightness":1.0_f32}),
            ObjectId::Light,
        ),
        ObjectKind::Audio => (
            "sound_emitters",
            json!({"id":id,"x":x,"y":y,"z":z,"sound":asset.unwrap_or("ambience_hvac"),"gain":1.0_f32,"looped":true,"playing":true,"spatial":false}),
            ObjectId::Audio,
        ),
        ObjectKind::SpawnPoint => {
            let template = unique_id(raw, "editor_template")?;
            array_mut(raw,"spawn_templates")?.push(json!({"id":template,"model":asset.unwrap_or("mannequin"),"scale":1.0_f32,"lifetime_seconds":30.0_f32}));
            let group = unique_id(raw, "editor_spawn_group")?;
            array_mut(raw, "spawn_groups")?.push(json!({"id":group,"at_most_one_active":true}));
            let entries = array_mut(raw, "spawn_points")?;
            let index = entries.len();
            entries.push(json!({"id":id,"x":x,"y":y,"z":z,"template":template,"group":group}));
            return Ok(ObjectId::SpawnPoint(index));
        }
        ObjectKind::Entity => {
            let index = level.props.len();
            array_mut(raw,"props")?.push(json!({"id":id,"model":asset.unwrap_or("mannequin"),"x":x,"y":0.0_f32,"z":z,"scale":1.0_f32,"components":[]}));
            return Ok(ObjectId::Prop(index));
        }
        ObjectKind::Door => (
            "doors",
            json!({"id":id,"x":x,"y":0.0_f32,"z":z,"width":0.9_f32,"height":2.1_f32,"initial_state":"closed","components":[{"component":"interactable","prompt":"Open / close"}],"bindings":[{"on":"interact","actions":[{"action":"toggle"}]}]}),
            ObjectId::Door,
        ),
        ObjectKind::Trigger => (
            "volumes",
            json!({"id":id,"x":x,"z":z,"width":2.0_f32,"depth":2.0_f32,"bottom_y":y,"top_y":y+2.0_f32,"bindings":[]}),
            ObjectId::Trigger,
        ),
        ObjectKind::FogRegion | ObjectKind::GroundFog => {
            return super::fog::add(raw, level, kind, position);
        }
        ObjectKind::Room
        | ObjectKind::Wall
        | ObjectKind::Prop
        | ObjectKind::Ramp
        | ObjectKind::FloorRegion => return Err("Choose a gameplay creation type".into()),
    };
    let entries = array_mut(raw, collection)?;
    let index = entries.len();
    entries.push(object);
    Ok(constructor(index))
}

pub(super) fn unique_id(raw: &Value, prefix: &str) -> Result<String, String> {
    for index in 1_u32..=1_000_000 {
        let candidate = format!("{prefix}_{index}");
        if !has_id(raw, &candidate) {
            return Ok(candidate);
        }
    }
    Err("No free editor identity remains".into())
}

pub(super) fn pin_gameplay_ids(raw: &mut Value, level: &LevelDef) -> Result<(), String> {
    pin_collection_ids(raw, level.light_instance_ids(), ObjectId::Light)?;
    pin_collection_ids(raw, level.volume_instance_ids(), ObjectId::Trigger)
}
fn pin_collection_ids(
    raw: &mut Value,
    ids: Vec<String>,
    constructor: fn(usize) -> ObjectId,
) -> Result<(), String> {
    for (index, identity) in ids.into_iter().enumerate() {
        let object = object_mut(raw, constructor(index))?;
        if object
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            drop(object.insert("id".into(), identity.into()));
        }
    }
    Ok(())
}

fn has_id(raw: &Value, id: &str) -> bool {
    match raw {
        Value::Object(entries) => {
            entries.get("id").and_then(Value::as_str) == Some(id)
                || entries.values().any(|entry| has_id(entry, id))
        }
        Value::Array(entries) => entries.iter().any(|entry| has_id(entry, id)),
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Number(_) => false,
    }
}

fn unused_physics_group(level: &LevelDef) -> Result<String, String> {
    for index in 1_u32..=1_000_000 {
        let candidate = format!("editor_physics_{index}");
        if !level.props.iter().any(|prop| {
            prop.physics
                .as_ref()
                .and_then(|profile| profile.reset_group.as_deref())
                == Some(candidate.as_str())
        }) {
            return Ok(candidate);
        }
    }
    Err("No free physics group remains".into())
}

fn binding_mut(
    raw: &mut Value,
    id: ObjectId,
) -> Result<&mut serde_json::Map<String, Value>, String> {
    let object = object_mut(raw, id)?;
    let draft = object.get("editor_trigger_policy").cloned();
    let entries = object
        .entry("bindings")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Invalid bindings")?;
    if entries.is_empty() {
        let mut binding = draft.unwrap_or_else(|| json!({"on":"enter_volume"}));
        drop(
            binding
                .as_object_mut()
                .ok_or("Invalid trigger policy")?
                .insert("actions".into(), json!([])),
        );
        entries.push(binding);
    }
    entries
        .first_mut()
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Invalid trigger binding".into())
}

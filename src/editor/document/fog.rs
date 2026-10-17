//! Distinct box and ground fog authoring over the shared engine atmosphere.
//! Raw records remain authoritative so optional and unknown data survive edits.

use serde_json::Value;

use super::Document;
use super::objects::{self, ObjectId, ObjectInfo, ObjectKind, Transform};
use crate::engine::level::{LevelDef, MAX_FOG_REGIONS};

impl Document {
    /// Changes one volume's extinction in one undo step.
    ///
    /// # Errors
    /// Rejects non-fog selections, stale IDs and invalid density values.
    pub fn set_fog_density(&mut self, id: ObjectId, density: f32) -> Result<(), String> {
        if !density.is_finite() {
            return Err("Fog density must be finite".into());
        }
        self.set_fog_value(id, "density", serde_json::json!(density))
    }

    /// Changes one volume's display sRGB tint. `None` follows global fog.
    ///
    /// # Errors
    /// Rejects non-fog selections, stale IDs and invalid colour channels.
    pub fn set_fog_color(&mut self, id: ObjectId, color: Option<[f32; 3]>) -> Result<(), String> {
        if color.is_some_and(|tint| tint.iter().any(|value| !value.is_finite())) {
            return Err("Fog colour channels must be finite".into());
        }
        self.set_fog_value(id, "color", serde_json::json!(color))
    }

    /// Changes the horizontal edge softness in metres.
    ///
    /// # Errors
    /// Rejects non-fog selections, stale IDs and invalid edge values.
    pub fn set_fog_falloff(&mut self, id: ObjectId, falloff: f32) -> Result<(), String> {
        if !falloff.is_finite() {
            return Err("Fog edge softness must be finite".into());
        }
        self.set_fog_value(id, "falloff_m", serde_json::json!(falloff))
    }

    fn set_fog_value(&mut self, id: ObjectId, key: &str, value: Value) -> Result<(), String> {
        if !matches!(id, ObjectId::FogRegion(_) | ObjectId::GroundFog(_)) {
            return Err("Select regional or ground fog to edit atmosphere".into());
        }
        let mut edited = self.selected.raw.clone();
        let object = objects::object_mut(&mut edited, id)?;
        if value.is_null() {
            drop(object.remove(key));
        } else {
            drop(object.insert(key.into(), value));
        }
        self.accept_edit(edited)
    }
}

pub(super) fn enumerate(level: &LevelDef) -> Vec<ObjectInfo> {
    level
        .fog_regions
        .iter()
        .enumerate()
        .map(|(index, fog)| (ObjectId::FogRegion(index), format!("Fog / {}", fog.id)))
        .chain(
            level
                .ground_fog
                .iter()
                .enumerate()
                .map(|(index, fog)| (ObjectId::GroundFog(index), format!("Ground / {}", fog.id))),
        )
        .filter_map(|(id, label)| {
            transform(level, id).ok().map(|transform| ObjectInfo {
                id,
                label,
                transform,
            })
        })
        .collect()
}

pub(super) fn transform(level: &LevelDef, id: ObjectId) -> Result<Transform, String> {
    let (position, size) = if let ObjectId::FogRegion(index) = id {
        let fog = level
            .fog_regions
            .get(index)
            .ok_or("Fog volume no longer exists")?;
        let [min_x, min_y, min_z] = fog.min;
        let [max_x, max_y, max_z] = fog.max;
        (fog.min, [max_x - min_x, max_y - min_y, max_z - min_z])
    } else if let ObjectId::GroundFog(index) = id {
        let fog = level
            .ground_fog
            .get(index)
            .ok_or("Ground fog no longer exists")?;
        (
            [fog.min[0], fog.ground_y, fog.min[1]],
            [fog.max[0] - fog.min[0], fog.height, fog.max[1] - fog.min[1]],
        )
    } else {
        return Err("Select a fog volume".into());
    };
    Ok(Transform {
        position,
        size,
        yaw_degrees: 0.0,
        pitch_degrees: 0.0,
        roll_degrees: 0.0,
    })
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
        || proposed.size.iter().any(|value| *value <= 0.0)
        || [
            proposed.yaw_degrees,
            proposed.pitch_degrees,
            proposed.roll_degrees,
        ]
        .iter()
        .any(|value| !value.is_finite() || value.abs() > f32::EPSILON)
    {
        return Err(
            "Fog requires finite coordinates, positive dimensions and axis-aligned bounds".into(),
        );
    }
    let current = transform(level, id)?;
    let object = objects::object_mut(raw, id)?;
    let [x, y, z] = proposed.position;
    let [width, height, depth] = proposed.size;
    if let ObjectId::FogRegion(index) = id {
        let fog = level
            .fog_regions
            .get(index)
            .ok_or("Fog volume no longer exists")?;
        drop(object.insert("min".into(), serde_json::json!([x, y, z])));
        drop(object.insert(
            "max".into(),
            serde_json::json!([x + width, y + height, z + depth]),
        ));
        // Retain authored height profiles relative to the box, including
        // full-density regional boxes whose base/top both equal max Y.
        for (key, authored) in [("ground_y", fog.ground_y), ("top_y", fog.top_y)] {
            if let Some(value) = authored {
                let fraction = (value - current.position[1]) / current.size[1];
                objects::set_number(object, key, height.mul_add(fraction, y));
            }
        }
    } else if matches!(id, ObjectId::GroundFog(_)) {
        drop(object.insert("min".into(), serde_json::json!([x, z])));
        drop(object.insert("max".into(), serde_json::json!([x + width, z + depth])));
        objects::set_number(object, "ground_y", y);
        objects::set_number(object, "height", height);
    } else {
        return Err("Select a fog volume".into());
    }
    Ok(())
}

pub(super) fn add(
    raw: &mut Value,
    level: &LevelDef,
    kind: ObjectKind,
    position: [f32; 3],
) -> Result<ObjectId, String> {
    if level
        .fog_regions
        .len()
        .saturating_add(level.ground_fog.len())
        >= MAX_FOG_REGIONS
    {
        return Err(format!(
            "Regional and ground fog share a limit of {MAX_FOG_REGIONS} volumes"
        ));
    }
    if position.iter().any(|value| !value.is_finite()) {
        return Err("Fog coordinates must be finite".into());
    }
    let [x, y, z] = position;
    let (collection, object) = if kind == ObjectKind::FogRegion {
        (
            "fog_regions",
            serde_json::json!({
                "id": unique_id(level, "fog")?, "min": [x,y,z],
                "max": [x + 4.0, y + 3.0, z + 4.0], "density": 0.08_f32,
                "ground_y": y + 3.0, "top_y": y + 3.0, "falloff_m": 0.5_f32
            }),
        )
    } else if kind == ObjectKind::GroundFog {
        (
            "ground_fog",
            serde_json::json!({
                "id": unique_id(level, "ground_fog")?, "min": [x,z],
                "max": [x + 4.0, z + 4.0], "ground_y": y,
                "height": 0.8_f32, "density": 0.12_f32, "falloff_m": 0.5_f32
            }),
        )
    } else {
        return Err("Choose regional or ground fog".into());
    };
    let entries = objects::array_mut(raw, collection)?;
    let index = entries.len();
    entries.push(object);
    Ok(if kind == ObjectKind::FogRegion {
        ObjectId::FogRegion(index)
    } else {
        ObjectId::GroundFog(index)
    })
}

pub(super) fn duplicate(
    raw: &mut Value,
    level: &LevelDef,
    id: ObjectId,
) -> Result<ObjectId, String> {
    if level
        .fog_regions
        .len()
        .saturating_add(level.ground_fog.len())
        >= MAX_FOG_REGIONS
    {
        return Err(format!(
            "Regional and ground fog share a limit of {MAX_FOG_REGIONS} volumes"
        ));
    }
    let mut moved = transform(level, id)?;
    moved.position[0] += 0.5;
    moved.position[2] += 0.5;
    let mut scratch = raw.clone();
    set_transform(&mut scratch, level, id, moved)?;
    let mut copy = objects::object_mut(&mut scratch, id)?.clone();
    drop(copy.remove("editor_instance_id"));
    let prefix = if matches!(id, ObjectId::GroundFog(_)) {
        "ground_fog"
    } else {
        "fog"
    };
    drop(copy.insert("id".into(), unique_id(level, prefix)?.into()));
    let (collection, _) = id.collection();
    let entries = objects::array_mut(raw, collection)?;
    let index = entries.len();
    entries.push(Value::Object(copy));
    Ok(if matches!(id, ObjectId::GroundFog(_)) {
        ObjectId::GroundFog(index)
    } else {
        ObjectId::FogRegion(index)
    })
}

fn unique_id(level: &LevelDef, prefix: &str) -> Result<String, String> {
    for index in 1..=MAX_FOG_REGIONS.saturating_add(1) {
        let candidate = format!("{prefix}_{index}");
        if !level.fog_regions.iter().any(|fog| fog.id == candidate)
            && !level.ground_fog.iter().any(|fog| fog.id == candidate)
        {
            return Ok(candidate);
        }
    }
    Err("No free fog identity remains".into())
}

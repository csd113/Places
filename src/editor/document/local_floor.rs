//! A storey-local override is ordinary owned room geometry, never a global region.
use super::{
    Document, ObjectId,
    objects::{array_mut, object_mut, set_number},
};
use serde_json::Value;
impl Document {
    /// A synchronous preview keeps the exact typed result of full admission.
    /// Raw source still enters the checked commit gateway on release/apply.
    pub(in crate::editor) fn validated_preview(
        &self,
        raw: Value,
    ) -> Result<(Value, crate::engine::level::LevelDef), String> {
        let level = self.validate_candidate(&raw)?;
        Ok((raw, level))
    }

    pub(in crate::editor) fn validate_candidate(
        &self,
        raw: &Value,
    ) -> Result<crate::engine::level::LevelDef, String> {
        super::persistence::validate_raw(raw, self.level_id()).map(|(level, _)| level)
    }
    pub(in crate::editor) fn apply_authoring_edit(
        &mut self,
        expected: &Value,
        raw: Value,
    ) -> Result<(), String> {
        if self.raw() != expected {
            return Err("Scene changed; reopen the form".into());
        }
        self.accept_edit(raw)
    }
    pub(in crate::editor) fn local_floor_candidate(
        &self,
        index: usize,
        record: &Value,
    ) -> Result<Value, String> {
        let room = self
            .level()
            .rooms
            .get(index)
            .ok_or("Select the owning room")?;
        if !room.ceiling.is_flat() && !room.ceiling.is_open() {
            return Err("Local surface sections require a flat or open ceiling".into());
        }
        let number = |key: &str| {
            record
                .get(key)
                .and_then(Value::as_f64)
                .map(|n| n.to_string().parse::<f32>())
                .transpose()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Missing {key}"))
        };
        let x = number("x")?;
        let z = number("z")?;
        let width = number("width")?;
        let depth = number("depth")?;
        let offset = number("offset_y")?;
        if [x, z, width, depth, offset].iter().any(|n| !n.is_finite())
            || width <= 0.0
            || depth <= 0.0
            || x < room.x
            || z < room.z
            || x + width > room.x + room.width
            || z + depth > room.z + room.depth
        {
            return Err("Override must be a finite rectangle inside the selected room".into());
        }
        let cut = (x, x + width, z, z + depth);
        super::geometry::require_clear_cut(self, ObjectId::Room(index), cut, room.floor_y)?;
        if self
            .level()
            .floor_regions
            .iter()
            .any(|r| super::geometry::overlaps(cut, r.bounds()))
            || self
                .level()
                .floor_patches
                .iter()
                .any(|r| super::geometry::overlaps(cut, r.bounds()))
        {
            return Err("A global region/patch overlaps this rectangle. Remove it explicitly before creating a local override".into());
        }
        let mut raw = self.raw().clone();
        let original = object_mut(&mut raw, ObjectId::Room(index))?.clone();
        let mut sections = Vec::new();
        for (section_index, [sx, sz, sw, sd]) in [
            [room.x, room.z, x - room.x, room.depth],
            [
                x + width,
                room.z,
                room.x + room.width - x - width,
                room.depth,
            ],
            [x, room.z, width, z - room.z],
            [x, z + depth, width, room.z + room.depth - z - depth],
            [x, z, width, depth],
        ]
        .into_iter()
        .enumerate()
        {
            if sw <= 0.0 || sd <= 0.0 {
                continue;
            }
            let mut section = original.clone();
            drop(section.remove("editor_instance_id"));
            for (key, value) in [("x", sx), ("z", sz), ("width", sw), ("depth", sd)] {
                set_number(&mut section, key, value);
            }
            if section_index == 4 {
                set_number(&mut section, "floor_y", room.floor_y + offset);
                set_number(&mut section, "height", room.height - offset);
                if let Some(material) = record.get("material") {
                    drop(section.insert("material".into(), material.clone()));
                }
            }
            sections.push(Value::Object(section));
        }
        if offset.abs() > 0.001 {
            let edge_material = self.level().defaults.wall.as_str();
            let base = room.floor_y.min(room.floor_y + offset);
            for (wx, wz, ww, wd) in [
                (x, z, width, 0.01_f32),
                (x, z + depth - 0.01, width, 0.01),
                (x, z, 0.01, depth),
                (x + width - 0.01, z, 0.01, depth),
            ] {
                array_mut(&mut raw,"walls")?.push(serde_json::json!({"x":wx,"z":wz,"y":base,"width":ww,"depth":wd,"height":offset.abs(),"material":edge_material}));
            }
        }
        drop(array_mut(&mut raw, "rooms")?.splice(index..index.saturating_add(1), sections));
        Ok(raw)
    }
}

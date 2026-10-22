//! Built-in entity behavior presets configure the shared component runtime.

use serde_json::{Value, json};

use super::super::objects::{array_mut, object_mut};
use super::super::{Document, ObjectId, objects};

impl Document {
    /// Selects fitted mannequin artwork without duplicating geometry or clips.
    ///
    /// # Errors
    /// Rejects incompatible objects and expired catalog entries atomically.
    pub fn set_character_appearance(
        &mut self,
        id: ObjectId,
        index: usize,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<(), String> {
        let selected_texture = match index {
            0 | 1 => None,
            2 => Some("core:mannequin_skin_man"),
            3 => Some("core:mannequin_skin_woman"),
            4 => Some("core:mannequin_skin_zombie"),
            5 => Some("core:mannequin_skin_robot"),
            6 => Some("core:mannequin_skin_scientist"),
            _ => return Err("Choose a mannequin appearance".into()),
        };
        let mut raw = self.raw().clone();
        let object = if let ObjectId::SpawnPoint(point_slot) = id {
            let template = &self
                .level()
                .spawn_points
                .get(point_slot)
                .ok_or("Missing spawn point")?
                .template;
            let slot = self
                .level()
                .spawn_templates
                .iter()
                .position(|entry| &entry.id == template)
                .ok_or("Missing template")?;
            array_mut(&mut raw, "spawn_templates")?
                .get_mut(slot)
                .and_then(Value::as_object_mut)
                .ok_or("Invalid template")?
        } else if matches!(id, ObjectId::Prop(_)) {
            object_mut(&mut raw, id)?
        } else {
            return Err("Choose a mannequin or spawn point".into());
        };
        if !matches!(
            object.get("model").and_then(Value::as_str),
            Some("mannequin" | "core:mannequin_black")
        ) {
            return Err("Appearance requires the canonical mannequin model".into());
        }
        if let Some(texture) = selected_texture {
            if catalog.texture_path(texture).is_none() {
                return Err("Appearance texture is missing".into());
            }
            drop(object.insert("character_texture".into(), texture.into()));
        } else {
            drop(object.remove("character_texture"));
        }
        drop(
            object.insert(
                "model".into(),
                if index == 1 {
                    "core:mannequin_black"
                } else {
                    "mannequin"
                }
                .into(),
            ),
        );
        self.accept_edit(raw)
    }

    /// Assigns a reusable typed behavior to a placed entity or spawn template.
    /// Other components and bindings survive. The shared navigation bake owns
    /// movement; this does not embed map or entity behavior in the editor.
    ///
    /// # Errors
    /// Rejects incompatible objects and invalid capabilities atomically.
    pub fn set_entity_behavior(
        &mut self,
        id: ObjectId,
        preset: usize,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<(), String> {
        let behavior = match preset {
            0 => None,
            1 => Some("idler"),
            2 => Some("wanderer"),
            3 => Some("follower"),
            4 => Some("prey"),
            _ => return Err("Choose a built-in entity behavior".into()),
        };
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        let object = if let ObjectId::SpawnPoint(index) = id {
            let template = &self
                .level()
                .spawn_points
                .get(index)
                .ok_or("Spawn point no longer exists")?
                .template;
            let template_index = self
                .level()
                .spawn_templates
                .iter()
                .position(|definition| &definition.id == template)
                .ok_or("Spawn template no longer exists")?;
            array_mut(&mut raw, "spawn_templates")?
                .get_mut(template_index)
                .and_then(Value::as_object_mut)
                .ok_or("Invalid spawn template")?
        } else if matches!(id, ObjectId::Prop(_)) {
            object_mut(&mut raw, id)?
        } else {
            return Err("Choose a placed entity or spawn point".into());
        };
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .ok_or("Entity model is missing")?;
        let size = catalog
            .placeable(model)
            .and_then(|entry| entry.size)
            .unwrap_or([0.6_f32, 1.8_f32, 0.6_f32]);
        let scale = object
            .get("scale")
            .cloned()
            .map_or(Ok(1.0_f32), serde_json::from_value::<f32>)
            .map_err(|error| error.to_string())?;
        let radius = (size[0].max(size[2]) * scale * 0.5_f32).clamp(0.05_f32, 1.2_f32);
        let height = (size[1] * scale).clamp(0.1_f32, 3.0_f32);
        let components = object
            .entry("components")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid components")?;
        components.retain(|component| {
            !matches!(
                component.get("component").and_then(Value::as_str),
                Some("ai" | "nav_agent")
            )
        });
        if let Some(kind) = behavior {
            components.push(json!({"component":"nav_agent","radius":radius,"height":height,"speed_mps":1.2_f32}));
            components.push(json!({"component":"ai","behavior":kind,"reacts_to":["player"]}));
        }
        self.accept_edit(raw)
    }

    /// Reuses an existing spawn template while preserving point placement/group.
    ///
    /// # Errors
    /// Rejects unknown templates or non-spawn selections.
    pub fn set_spawn_template(&mut self, id: ObjectId, template: &str) -> Result<(), String> {
        if !matches!(id, ObjectId::SpawnPoint(_)) {
            return Err("Choose a spawn point".into());
        }
        if !self
            .level()
            .spawn_templates
            .iter()
            .any(|definition| definition.id == template)
        {
            return Err("Spawn template no longer exists".into());
        }
        let mut raw = self.raw().clone();
        drop(object_mut(&mut raw, id)?.insert("template".into(), template.into()));
        self.accept_edit(raw)
    }
}

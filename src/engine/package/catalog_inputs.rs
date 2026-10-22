//! Versioned, map-local catalog closure shared by the compiler and player.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::level::LevelDef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogInputs {
    pub revision: u32,
    pub entries: BTreeMap<String, Option<Value>>,
}

impl CatalogInputs {
    /// Includes absent lookups, fixture models, and recursive material/image
    /// references. Adding a formerly missing asset therefore invalidates it.
    /// # Errors
    /// Rejects malformed catalog JSON or missing asset collection.
    pub fn for_level(level: &LevelDef, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Catalog exceeds input limit".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let _validated_catalog = crate::engine::loader::PropCatalog::from_json_str(text)?;
        let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let assets = value
            .get("assets")
            .and_then(Value::as_array)
            .ok_or("Catalog has no assets array")?;
        let index: BTreeMap<&str, &Value> = assets
            .iter()
            .filter_map(|entry| {
                entry
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id, entry))
            })
            .collect();
        let mut pending: BTreeSet<String> =
            crate::engine::materials::referenced_material_ids(level)
                .into_iter()
                .collect();
        pending.extend(level.decals.iter().map(|decal| decal.material.clone()));
        pending.extend(level.props.iter().map(|prop| prop.model.clone()));
        pending.extend(
            level
                .props
                .iter()
                .filter_map(|prop| prop.character_texture.clone())
                .chain(
                    level
                        .spawn_templates
                        .iter()
                        .filter_map(|template| template.character_texture.clone()),
                ),
        );
        pending.extend(
            level
                .spawn_templates
                .iter()
                .map(|template| template.model.clone()),
        );
        for light in &level.ceiling_lights {
            let _added = pending.insert(light.fixture.clone());
            if let Some(model) = crate::engine::lighting::fixture_model_id(&light.fixture) {
                let _model_added = pending.insert(model.into());
            }
        }
        if let Some(sky) = &level.sky {
            let _added = pending.insert(sky.texture.trim().into());
        }
        let mut entries = BTreeMap::new();
        while let Some(id) = pending.pop_first() {
            if entries.contains_key(&id) {
                continue;
            }
            let record = index.get(id.as_str()).copied();
            if let Some(definition) = record {
                for field in [
                    "texture",
                    "normal_texture",
                    "emissive_mask",
                    "uv_response_mask",
                    "baseboard",
                ] {
                    if let Some(reference) = definition.get(field).and_then(Value::as_str) {
                        let _added = pending.insert(reference.into());
                    }
                }
            }
            drop(entries.insert(id, record.cloned()));
        }
        Ok(Self {
            revision: 1,
            entries,
        })
    }

    /// # Errors
    /// Rejects unsupported closure versions or serialization errors.
    pub fn identity(&self) -> Result<String, String> {
        if self.revision != 1 {
            return Err("Unsupported catalog closure revision; rebuild package".into());
        }
        Ok(super::hash::sha256_hex(
            &crate::engine::canonical_json::canonical_json_bytes(self)?,
        ))
    }
}

#[cfg(test)]
#[path = "../tests/catalog_inputs.rs"]
mod tests;

//! Versioned, map-local catalog closure shared by the compiler and player.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::assets::{AssetCatalog, MAX_CATALOG_JSON_BYTES};
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
        let catalog = AssetCatalog::from_json_str(catalog_text(bytes)?)?;
        Self::for_validated_catalog(level, bytes, &catalog)
    }

    /// Compute the closure from the raw records paired with their validated parser output.
    pub(crate) fn for_validated_catalog(
        level: &LevelDef,
        bytes: &[u8],
        catalog: &AssetCatalog,
    ) -> Result<Self, String> {
        Self::collect(level, bytes, Some(catalog))
    }

    pub(super) fn matches_installed(
        &self,
        level: &LevelDef,
        bytes: &[u8],
        recorded_raw_sha256: &str,
    ) -> Result<bool, String> {
        if Self::for_level(level, bytes)? == *self {
            return Ok(true);
        }
        // Legacy closures omitted normalized and implicit lookups. Accept
        // a legacy match here only with the exact original catalog bytes.
        Ok(super::hash::sha256_hex(bytes) == recorded_raw_sha256
            && Self::collect(level, bytes, None)? == *self)
    }

    fn collect(
        level: &LevelDef,
        bytes: &[u8],
        catalog: Option<&AssetCatalog>,
    ) -> Result<Self, String> {
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
                    .map(|id| (if catalog.is_some() { id.trim() } else { id }, entry))
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
        if catalog.is_some() {
            pending.extend(level.sky_background_texture().map(str::to_owned));
            if !level.elevators.is_empty() {
                pending.extend(crate::engine::elevator::MODEL_IDS.map(str::to_owned));
            }
            if crate::engine::electrical_panel::present(level) {
                pending.extend(
                    crate::engine::electrical_panel::INTERNAL_MODELS.map(|(id, _)| id.to_owned()),
                );
            }
        } else if let Some(sky) = &level.sky {
            let _added = pending.insert(sky.texture.trim().into());
        }
        pending.extend(
            level
                .global_illuminators
                .iter()
                .filter_map(crate::engine::level::GlobalIlluminatorDef::visual_texture)
                .map(str::to_owned),
        );
        let mut entries = BTreeMap::new();
        while let Some(id) = pending.pop_first() {
            if entries.contains_key(&id) {
                continue;
            }
            let record = index.get(id.as_str()).copied();
            match catalog {
                Some(parsed) => {
                    if let Some(definition) = parsed.get(&id) {
                        pending.extend(
                            [
                                &definition.texture,
                                &definition.normal_texture,
                                &definition.emissive_mask,
                                &definition.uv_response_mask,
                                &definition.baseboard,
                            ]
                            .into_iter()
                            .flatten()
                            .cloned(),
                        );
                    }
                }
                None => {
                    if let Some(definition) = record {
                        pending.extend(legacy_references(definition).map(str::to_owned));
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

fn catalog_text(bytes: &[u8]) -> Result<&str, String> {
    if bytes.len() > MAX_CATALOG_JSON_BYTES {
        return Err("Catalog exceeds input limit".into());
    }
    std::str::from_utf8(bytes).map_err(|error| error.to_string())
}

fn legacy_references(definition: &Value) -> impl Iterator<Item = &str> {
    [
        "texture",
        "normal_texture",
        "emissive_mask",
        "uv_response_mask",
        "baseboard",
    ]
    .into_iter()
    .filter_map(|field| definition.get(field).and_then(Value::as_str))
}

#[cfg(test)]
#[path = "../tests/catalog_inputs.rs"]
mod tests;

use serde::{Deserialize, Serialize};

use crate::engine::{assets::AssetCatalog, audio::AudioCatalog, level::LevelDef};

/// Kind is separate from identity: an image ID is never accepted as a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinAssetKind {
    Material,
    Texture,
    Model,
    Fixture,
    Decal,
    Sound,
}

/// Installed-catalog reference for authoring tools. Map fields retain these
/// exact existing IDs; physical paths and table indices are never authored IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinAssetRef {
    pub kind: BuiltinAssetKind,
    pub id: String,
}

impl BuiltinAssetRef {
    /// Validates against the same parsed catalogs the engine uses.
    ///
    /// # Errors
    /// Rejects absent or wrongly typed IDs, including reserved custom `pack:` IDs.
    pub fn validate(
        &self,
        catalog: &AssetCatalog,
        audio: Option<&AudioCatalog>,
    ) -> Result<(), String> {
        let found = !self.id.starts_with("pack:")
            && match self.kind {
                BuiltinAssetKind::Material => catalog.material(&self.id).is_some(),
                BuiltinAssetKind::Texture => catalog
                    .get(&self.id)
                    .is_some_and(super::super::assets::AssetEntry::is_texture),
                BuiltinAssetKind::Model => catalog.placeable(&self.id).is_some(),
                BuiltinAssetKind::Fixture => catalog
                    .get(&self.id)
                    .is_some_and(super::super::assets::AssetEntry::is_light),
                BuiltinAssetKind::Decal => catalog
                    .get(&self.id)
                    .is_some_and(|entry| entry.asset_type.as_str() == "decal"),
                BuiltinAssetKind::Sound => audio.is_some_and(|cues| cues.contains(&self.id)),
            };
        if !found {
            return Err(format!(
                "unknown or incompatible built-in {:?} ID '{}'",
                self.kind, self.id
            ));
        }
        Ok(())
    }
}

/// Shared source-asset check for the compiler and future editor.
///
/// The material set comes directly from `MaterialTable`, including all defaults/face rules.
/// Audio action validation remains in the existing semantic validator.
///
/// # Errors
/// Returns an unknown or incompatible built-in reference before baking.
pub fn validate_level_assets(level: &LevelDef, catalog: &AssetCatalog) -> Result<(), String> {
    let mut references = Vec::new();
    references.extend(
        crate::engine::materials::referenced_material_ids(level)
            .into_iter()
            .map(|id| BuiltinAssetRef {
                kind: BuiltinAssetKind::Material,
                id,
            }),
    );
    references.extend(
        level
            .props
            .iter()
            .map(|prop| &prop.model)
            .chain(level.spawn_templates.iter().map(|template| &template.model))
            .map(|id| BuiltinAssetRef {
                kind: BuiltinAssetKind::Model,
                id: id.clone(),
            }),
    );
    references.extend(level.ceiling_lights.iter().map(|light| BuiltinAssetRef {
        kind: BuiltinAssetKind::Fixture,
        id: light.fixture.clone(),
    }));
    references.extend(level.decals.iter().map(|decal| BuiltinAssetRef {
        kind: BuiltinAssetKind::Decal,
        id: decal.material.clone(),
    }));
    if let Some(sky) = &level.sky {
        references.push(BuiltinAssetRef {
            kind: BuiltinAssetKind::Texture,
            id: sky.texture.clone(),
        });
    }
    for (model, selected_texture) in level
        .props
        .iter()
        .map(|prop| (&prop.model, &prop.character_texture))
        .chain(
            level
                .spawn_templates
                .iter()
                .map(|template| (&template.model, &template.character_texture)),
        )
    {
        if let Some(texture) = selected_texture {
            if !matches!(model.as_str(), "mannequin" | "core:mannequin_black") {
                return Err(format!(
                    "Character texture requires the canonical mannequin, got `{model}`"
                ));
            }
            references.push(BuiltinAssetRef {
                kind: BuiltinAssetKind::Texture,
                id: texture.clone(),
            });
        }
    }
    for reference in references {
        reference.validate(catalog, None)?;
    }
    Ok(())
}

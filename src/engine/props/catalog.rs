//! Placement view of the authoritative asset catalog.

use std::path::Path;

use crate::engine::assets::parse_hex_color;

/// Neutral fallback colour used for unknown prop models, `#8a8a8a`.
const PROP_FALLBACK_COLOR_HEX: &str = "#8a8a8a";

/// Neutral grey used for unknown prop models.
fn prop_fallback_color() -> [f32; 3] {
    parse_hex_color(PROP_FALLBACK_COLOR_HEX).unwrap_or([0.541, 0.541, 0.541])
}

/// One catalog entry describing a placeable prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropCatalogEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub model: Option<String>,
    pub solid: bool,
}

/// Catalog of placeable assets (environment props and entities) available to
/// levels.
///
/// This is the placement view of the authoritative [`crate::engine::assets::AssetCatalog`]:
/// levels reference a logical id such as `core:chair` or `spooner-man`, and the
/// catalog maps it to the canonical model resource under the asset root.
/// Entities resolve through exactly the same lookup, so `spooner-man` keeps
/// working unchanged.
///
/// Lookups always succeed: unknown or non-placeable ids resolve to a generated
/// fallback entry using [`crate::engine::level::PROP_FALLBACK_SIZE`] and a neutral
/// colour, so a level referencing a missing prop still loads with an obvious
/// placeholder instead of failing or rendering the wrong model.
#[derive(Debug, Clone, Default)]
pub struct PropCatalog {
    assets: crate::engine::assets::AssetCatalog,
}

impl PropCatalog {
    /// Empty catalog; every lookup falls back.
    #[must_use]
    pub fn builtin() -> Self {
        Self::default()
    }

    /// Parses an asset catalog document into the placement view.
    ///
    /// The document's one entry collection is `assets`; duplicate logical ids
    /// are rejected.
    /// # Errors
    ///
    /// Returns a message when the document is not valid JSON, when an id is
    /// duplicated or malformed, or when an entry declares a malformed
    /// class/theme/type/source/resource path.
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        Ok(Self {
            assets: crate::engine::assets::AssetCatalog::from_json_str(json)?,
        })
    }

    /// Loads a catalog from `path`, returning `None` when the file is missing
    /// or invalid. Never panics.
    #[must_use]
    pub fn load_from_path(path: &Path) -> Option<Self> {
        Some(Self {
            assets: crate::engine::assets::AssetCatalog::load_from_path(path)?,
        })
    }

    /// Loads the shipped asset catalog, falling back to an empty catalog when
    /// no `assets/catalog.json` can be found.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            assets: crate::engine::assets::AssetCatalog::load_default(),
        }
    }

    /// The authoritative catalog behind the placement view.
    #[must_use]
    pub const fn assets(&self) -> &crate::engine::assets::AssetCatalog {
        &self.assets
    }

    /// The declared environment themes, in catalog order.
    #[must_use]
    pub fn themes(&self) -> &[crate::engine::assets::AssetThemeDef] {
        self.assets.themes()
    }

    /// Resolves a logical asset id, or a generated fallback entry for unknown
    /// or non-placeable ids.
    #[must_use]
    pub fn get(&self, model: &str) -> PropCatalogEntry {
        if let Some(entry) = self.assets.placeable(model) {
            return placeable_entry(entry);
        }
        PropCatalogEntry {
            id: model.to_string(),
            name: model.to_string(),
            category: "Other".into(),
            size: crate::engine::level::PROP_FALLBACK_SIZE,
            color: prop_fallback_color(),
            model: None,
            solid: false,
        }
    }

    /// Number of placeable catalog entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.assets.placeable_entries().len()
    }

    /// Every placeable entry, ordered by id so validation and reports are stable.
    #[must_use]
    pub fn entries(&self) -> Vec<PropCatalogEntry> {
        self.assets
            .placeable_entries()
            .into_iter()
            .map(placeable_entry)
            .collect()
    }

    /// True when the catalog defines this exact placeable id.
    #[must_use]
    pub fn contains(&self, model: &str) -> bool {
        self.assets.placeable(model).is_some()
    }

    /// Linear albedo preset for a placed static model; decoded geometry and
    /// textures remain shared by canonical resource path.
    #[must_use]
    pub fn tint(&self, model: &str) -> Option<[f32; 3]> {
        self.assets.placeable(model).and_then(|entry| entry.tint)
    }

    /// True when the catalog has no placeable entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Converts a catalog entry into the runtime's placeable view, applying the
/// neutral fallbacks for missing optional metadata.
fn placeable_entry(entry: &crate::engine::assets::AssetEntry) -> PropCatalogEntry {
    PropCatalogEntry {
        id: entry.id.clone(),
        name: entry.display_name.clone(),
        category: entry
            .category
            .clone()
            .unwrap_or_else(|| "Other".to_string()),
        size: entry
            .size
            .filter(|size| size.iter().all(|value| value.is_finite() && *value > 0.0))
            .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE),
        color: entry.color.unwrap_or_else(prop_fallback_color),
        model: entry.model.clone(),
        solid: entry.solid,
    }
}

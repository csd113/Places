//! Material definitions carried inside a level pack.
//!
//! A pack's `materials.json` is authored data, not catalog data: it names PNG
//! bytes inside the pack (or a logical catalog texture) and may override the
//! tiling period and tint. Both the historical string form and the object form
//! parse.

use std::collections::HashMap;
use std::rc::Rc;

use crate::assets::DEFAULT_TILE_METRES;

use super::DEFAULT_TINT;
use super::image::{RawImage, TextureCache, decode_png};

/// One material a pack's `materials.json` declares.
///
/// Both the historical string form (`"pack:wall": "textures/wall.png"`) and the
/// object form (`{"texture": ..., "tile_metres": ..., "tint": [...]}`) parse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackMaterialDef {
    /// Path inside the pack, or a logical catalog texture id.
    pub texture: String,
    /// World metres per repeat; `None` keeps [`DEFAULT_TILE_METRES`].
    pub tile_metres: Option<f32>,
    pub tint: Option<[f32; 3]>,
}

impl PackMaterialDef {
    #[must_use]
    pub fn tile_metres(&self) -> f32 {
        self.tile_metres
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_TILE_METRES)
    }

    #[must_use]
    pub fn tint(&self) -> [f32; 3] {
        self.tint.unwrap_or(DEFAULT_TINT)
    }
}

/// The material definitions and raw textures extracted from one level pack.
///
/// `namespace` keeps two packs that both ship `textures/wall.png` from sharing
/// a cache entry in one session.
#[derive(Clone, Debug, Default)]
pub struct PackMaterials {
    namespace: String,
    definitions: HashMap<String, PackMaterialDef>,
    /// Raw PNG bytes keyed by the alias paths the pack extractor registered.
    textures: HashMap<String, Rc<[u8]>>,
}

impl PackMaterials {
    /// Builds the pack material view from `materials.json` and the extracted
    /// texture blobs. Malformed JSON yields no definitions (the pack's own
    /// `pack:` ids then fall back to the direct `textures/<name>.png` lookup,
    /// exactly as before).
    #[must_use]
    pub fn new(
        namespace: impl Into<String>,
        materials_json: Option<&str>,
        textures: HashMap<String, Rc<[u8]>>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            definitions: parse_materials_json(materials_json),
            textures,
        }
    }

    /// True when the pack carries no material definitions and no textures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.textures.is_empty()
    }

    #[must_use]
    pub const fn definitions(&self) -> &HashMap<String, PackMaterialDef> {
        &self.definitions
    }

    /// The definition for a `pack:` material id, if the pack declares one.
    #[must_use]
    pub fn definition(&self, material_id: &str) -> Option<&PackMaterialDef> {
        self.definitions.get(material_id)
    }

    /// The texture path a `pack:` material id resolves to.
    ///
    /// A declared definition wins; otherwise the historical direct-name
    /// candidates apply. A match is only returned when the pack actually
    /// carries bytes for the path.
    #[must_use]
    pub fn texture_for(&self, material_id: &str) -> Option<String> {
        if let Some(definition) = self.definition(material_id)
            && !definition.texture.is_empty()
            && self.lookup(&definition.texture).is_some()
        {
            return Some(definition.texture.clone());
        }
        let name = material_id.strip_prefix("pack:").unwrap_or(material_id);
        [
            format!("textures/{name}.png"),
            format!("{name}.png"),
            format!("textures/{name}"),
            name.to_string(),
        ]
        .into_iter()
        .find(|candidate| self.lookup(candidate).is_some())
    }

    /// The raw PNG bytes behind a pack-relative path (or a catalog texture id
    /// the pack reuses), with the pack's own alias rules.
    #[must_use]
    pub fn lookup(&self, path: &str) -> Option<Rc<[u8]>> {
        let normalized = path.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
        self.textures
            .get(&normalized)
            .or_else(|| self.textures.get(file_name))
            .map(Rc::clone)
    }

    /// Decodes one pack texture through the session cache.
    pub(super) fn decode_cached(
        &self,
        cache: &mut TextureCache,
        path: &str,
    ) -> Result<(Rc<RawImage>, String), String> {
        let key = self.cache_key(path);
        if let Some(image) = cache.get(&key) {
            return Ok((image, key));
        }
        let bytes = self
            .lookup(path)
            .ok_or_else(|| format!("`{path}` is not present in the pack"))?;
        let image = decode_png(&bytes).map_err(|error| format!("`{path}`: {error}"))?;
        let image = cache.insert(key.clone(), image);
        Ok((image, key))
    }

    /// Decodes one pack texture into the session cache.
    /// # Errors
    ///
    /// Returns a message when the path is absent from the pack or the bytes are
    /// not a valid PNG.
    pub fn decode_texture(
        &self,
        cache: &mut TextureCache,
        path: &str,
    ) -> Result<Rc<RawImage>, String> {
        self.decode_cached(cache, path).map(|(image, _key)| image)
    }

    /// The session-unique cache/dedupe key of one pack texture.
    #[must_use]
    pub fn cache_key(&self, path: &str) -> String {
        format!("pack:{}:{}", self.namespace, path.replace('\\', "/"))
    }

    /// Maps a session key back to the pack-relative path, when it is one of
    /// this pack's keys.
    #[must_use]
    pub fn path_of_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        key.strip_prefix(&format!("pack:{}:", self.namespace))
    }
}

/// Parses `materials.json` into material definitions.
///
/// Accepts `{"materials": {...}}` and a flat object, with string values
/// (`"pack:wall": "textures/wall.png"`) or objects carrying `texture`/`file`/
/// `diffuse`, plus the optional `tile_metres` and `tint` fields. Unknown fields
/// are ignored and malformed values fall back to the defaults, so an older or
/// newer pack keeps loading.
#[must_use]
pub fn parse_materials_json(json_str: Option<&str>) -> HashMap<String, PackMaterialDef> {
    let mut result = HashMap::new();
    let Some(json_str) = json_str else {
        return result;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return result;
    };
    let table = value
        .get("materials")
        .and_then(|materials| materials.as_object())
        .or_else(|| value.as_object());
    let Some(table) = table else {
        return result;
    };
    for (key, value) in table {
        if key == "materials" {
            continue;
        }
        let definition = if let Some(path) = value.as_str() {
            PackMaterialDef {
                texture: path.to_string(),
                ..PackMaterialDef::default()
            }
        } else {
            let Some(path) = value
                .get("texture")
                .or_else(|| value.get("file"))
                .or_else(|| value.get("diffuse"))
                .and_then(|texture| texture.as_str())
            else {
                continue;
            };
            PackMaterialDef {
                texture: path.to_string(),
                tile_metres: value.get("tile_metres").and_then(parse_tile_metres),
                tint: value.get("tint").and_then(parse_tint_value),
            }
        };
        if !definition.texture.is_empty() {
            result.insert(key.clone(), definition);
        }
    }
    result
}

/// Reads an optional `tile_metres` number.
///
/// JSON numbers are `f64`; the material model stores `f32`, so the value is
/// narrowed here exactly as it always has been. Non-finite and non-positive
/// results are discarded when the field is read
/// ([`PackMaterialDef::tile_metres`]).
fn parse_tile_metres(value: &serde_json::Value) -> Option<f32> {
    // `f64 -> f32` can round (that is the point of the field) and saturates to
    // an infinity for absurd JSON, which the reader filters out.
    #[allow(clippy::cast_possible_truncation)]
    let narrowed = value.as_f64()? as f32;
    Some(narrowed)
}

/// Parses a `[r, g, b]` tint array from JSON.
fn parse_tint_value(value: &serde_json::Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    let mut tint = [0.0f32; 3];
    if array.len() != tint.len() {
        return None;
    }
    for (slot, channel) in tint.iter_mut().zip(array) {
        // `f64 -> f32` rounds to the nearest `f32`; a value whose rounded form
        // falls outside `[0, 1]` is rejected on the next line, so only
        // correctly rounded in-range channels are stored.
        #[allow(clippy::cast_possible_truncation)]
        let value = channel.as_f64()? as f32;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return None;
        }
        *slot = value;
    }
    Some(tint)
}

//! Material definitions carried inside a level pack.
//!
//! A pack's `materials.json` is authored data, not catalog data: it names PNG
//! bytes inside the pack (or a logical catalog texture) and may override the
//! tiling period and tint. Both the string shorthand and the object form
//! parse.

use std::collections::HashMap;
use std::sync::Arc;

use crate::assets::DEFAULT_TILE_METRES;

use super::image::{RawImage, TextureCache, texture_content_key};
use super::{
    DEFAULT_EMISSION_INTENSITY, DEFAULT_REFLECTION_STRENGTH, DEFAULT_TINT, MAX_EMISSION_INTENSITY,
    MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, ReflectionMode,
};

/// One material a pack's `materials.json` declares.
///
/// Both the string shorthand (`"pack:wall": "textures/wall.png"`) and the
/// object form (`{"texture": ..., "tile_metres": ..., "tint": [...]}`) parse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackMaterialDef {
    /// Path inside the pack, or a logical catalog texture id.
    pub texture: String,
    /// World metres per repeat; `None` keeps [`DEFAULT_TILE_METRES`].
    pub tile_metres: Option<f32>,
    pub tint: Option<[f32; 3]>,
    /// Emissive colour, when the pack authors one.
    pub emissive: Option<[f32; 3]>,
    /// Scalar multiplier; `None` keeps [`DEFAULT_EMISSION_INTENSITY`].
    pub emissive_intensity: Option<f32>,
    /// Pack path, or logical catalog texture id, of the emissive mask.
    pub emissive_mask: Option<String>,
    /// Pack path, or logical catalog texture id, of the normal map.
    pub normal_texture: Option<String>,
    pub normal_strength: Option<f32>,
    /// Sheen strength (white) and optional explicit sheen colour.
    pub specular: Option<f32>,
    pub specular_color: Option<[f32; 3]>,
    /// Author-facing glossiness, `0.0` matte .. `1.0` extremely glossy.
    pub shine: Option<f32>,
    /// `none` | `probe` | `planar`. Absent means no reflection at all.
    pub reflection_mode: Option<String>,
    /// `0.0..=1.0`; `None` keeps [`DEFAULT_REFLECTION_STRENGTH`].
    pub reflection_strength: Option<f32>,
    /// `opaque` | `cutout` | `blend`.
    pub alpha_mode: Option<String>,
    pub opacity: Option<f32>,
    pub alpha_cutoff: Option<f32>,
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

    /// The emission this definition describes.
    ///
    /// A definition without an emissive colour is non-emissive and can never
    /// pick up a stray intensity or mask. The mask is left as `None`: its
    /// table index only exists once the resolver has interned the texture.
    #[must_use]
    pub fn emission(&self) -> MaterialEmission {
        let Some(color) = self.emissive else {
            return MaterialEmission::NONE;
        };
        MaterialEmission::new(
            color,
            self.emissive_intensity
                .unwrap_or(DEFAULT_EMISSION_INTENSITY),
        )
        .sanitized()
    }

    /// The surface response this definition describes.
    ///
    /// The normal map is left `None` here for the same reason as the emissive
    /// mask: it is a texture-table index the resolver fills in. A pack that
    /// authors neither a normal map nor a sheen gets [`MaterialResponse::NONE`],
    /// the flat default surface.
    #[must_use]
    pub fn response(&self) -> MaterialResponse {
        let sheen = self.specular.unwrap_or(0.0);
        let specular = self
            .specular_color
            .map_or([sheen; 3], |color| color.map(|channel| channel * sheen));
        let roughness = self
            .shine
            .map_or(super::DEFAULT_ROUGHNESS, super::roughness_from_shine);
        MaterialResponse {
            normal: None,
            normal_strength: self
                .normal_strength
                .unwrap_or(super::DEFAULT_NORMAL_STRENGTH),
            specular,
            roughness,
        }
        .sanitized()
    }

    /// The reflection contract this definition describes.
    ///
    /// A definition that names no mode reflects nothing, and a mode without a
    /// strength gets [`DEFAULT_REFLECTION_STRENGTH`]: marking a surface is one
    /// word in the catalog, and how strong it is stays a separate decision.
    #[must_use]
    pub fn reflection(&self) -> MaterialReflection {
        let Some(mode) = self
            .reflection_mode
            .as_deref()
            .and_then(ReflectionMode::parse)
        else {
            return MaterialReflection::NONE;
        };
        MaterialReflection::new(
            mode,
            self.reflection_strength
                .unwrap_or(DEFAULT_REFLECTION_STRENGTH),
        )
        .sanitized()
    }

    /// The alpha contract this definition describes.
    #[must_use]
    pub fn alpha(&self) -> MaterialAlpha {
        MaterialAlpha {
            mode: self
                .alpha_mode
                .as_deref()
                .and_then(super::AlphaMode::parse)
                .unwrap_or_default(),
            opacity: self.opacity.unwrap_or(1.0),
            cutoff: self.alpha_cutoff.unwrap_or(super::DEFAULT_ALPHA_CUTOFF),
        }
        .sanitized()
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
    textures: HashMap<String, Arc<[u8]>>,
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
        textures: HashMap<String, Arc<[u8]>>,
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

    /// The definition for a `pack:` material id, if the pack declares one.
    #[must_use]
    pub fn definition(&self, material_id: &str) -> Option<&PackMaterialDef> {
        self.definitions.get(material_id)
    }

    /// The texture path a `pack:` material id resolves to.
    ///
    /// A declared definition wins; otherwise the direct-name
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
    pub fn lookup(&self, path: &str) -> Option<Arc<[u8]>> {
        let normalized = path.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
        self.textures
            .get(&normalized)
            .or_else(|| self.textures.get(file_name))
            .map(Arc::clone)
    }

    /// Decodes one pack texture through the session cache.
    pub(super) fn decode_cached(
        &self,
        cache: &mut TextureCache,
        path: &str,
    ) -> Result<(Arc<RawImage>, String), String> {
        let bytes = self
            .lookup(path)
            .ok_or_else(|| format!("`{path}` is not present in the pack"))?;
        let logical = format!("pack:{}:{}", self.namespace, path.replace('\\', "/"));
        cache
            .decode_encoded(&logical, &bytes)
            .map_err(|error| format!("`{path}`: {error}"))
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
    ) -> Result<Arc<RawImage>, String> {
        self.decode_cached(cache, path).map(|(image, _key)| image)
    }

    /// The session-unique cache/dedupe key of one pack texture.
    #[must_use]
    pub fn cache_key(&self, path: &str) -> String {
        let logical = format!("pack:{}:{}", self.namespace, path.replace('\\', "/"));
        self.lookup(path).map_or_else(
            || logical.clone(),
            |bytes| texture_content_key(&logical, &bytes),
        )
    }
}

/// Parses `materials.json` into material definitions.
///
/// Accepts `{"materials": {...}}` and a flat object, with string values
/// (`"pack:wall": "textures/wall.png"`) or objects carrying `texture`/`file`/
/// plus the optional `tile_metres`, `tint`, `emissive`,
/// `emissive_intensity`, `emissive_mask` and `shine` fields. Unknown fields are
/// ignored and malformed values fall back to the defaults, so a pack written
/// against a different engine revision keeps loading. The string shorthand is
/// emission-free by construction.
#[must_use]
pub fn parse_materials_json(json_str: Option<&str>) -> HashMap<String, PackMaterialDef> {
    let mut result = HashMap::new();
    let Some(source_json) = json_str else {
        return result;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source_json) else {
        return result;
    };
    let table = value
        .get("materials")
        .and_then(|materials| materials.as_object())
        .or_else(|| value.as_object());
    let Some(definitions) = table else {
        return result;
    };
    for (key, material_value) in definitions {
        if key == "materials" {
            continue;
        }
        let definition = if let Some(path) = material_value.as_str() {
            PackMaterialDef {
                texture: path.to_string(),
                ..PackMaterialDef::default()
            }
        } else {
            let Some(path) = material_value
                .get("texture")
                .and_then(|texture| texture.as_str())
            else {
                continue;
            };
            PackMaterialDef {
                texture: path.to_string(),
                tile_metres: material_value
                    .get("tile_metres")
                    .and_then(parse_tile_metres),
                tint: material_value.get("tint").and_then(parse_unit_rgb),
                emissive: material_value.get("emissive").and_then(parse_unit_rgb),
                emissive_intensity: material_value
                    .get("emissive_intensity")
                    .and_then(parse_emissive_intensity),
                emissive_mask: material_value
                    .get("emissive_mask")
                    .and_then(|mask| mask.as_str())
                    .map(str::trim)
                    .filter(|mask| !mask.is_empty())
                    .map(str::to_string),
                normal_texture: material_value
                    .get("normal_texture")
                    .and_then(|texture| texture.as_str())
                    .map(str::trim)
                    .filter(|texture| !texture.is_empty())
                    .map(str::to_string),
                normal_strength: material_value
                    .get("normal_strength")
                    .and_then(parse_unit_number),
                specular: material_value.get("specular").and_then(parse_unit_number),
                specular_color: material_value
                    .get("specular_color")
                    .and_then(parse_unit_rgb),
                shine: material_value.get("shine").and_then(parse_unit_number),
                alpha_mode: material_value
                    .get("alpha_mode")
                    .and_then(|mode| mode.as_str())
                    .map(str::trim)
                    .filter(|mode| !mode.is_empty())
                    .map(str::to_string),
                opacity: material_value.get("opacity").and_then(parse_unit_number),
                alpha_cutoff: material_value
                    .get("alpha_cutoff")
                    .and_then(parse_unit_number),
                reflection_mode: material_value
                    .get("reflection_mode")
                    .and_then(|mode| mode.as_str())
                    .map(str::trim)
                    .filter(|mode| !mode.is_empty())
                    .map(str::to_string),
                reflection_strength: material_value
                    .get("reflection_strength")
                    .and_then(parse_unit_number),
            }
        };
        if !definition.texture.is_empty() {
            drop(result.insert(key.clone(), definition));
        }
    }
    result
}

/// Narrows a JSON scalar to the material's finite `f32` representation.
fn finite_material_number(number: f64) -> Option<f32> {
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "Material fields are stored as f32; conversion rounds once, and non-finite results are rejected before storage"
    )]
    let narrowed = number as f32;
    narrowed.is_finite().then_some(narrowed)
}

/// Reads an optional `tile_metres` number; the accessor checks positivity.
fn parse_tile_metres(value: &serde_json::Value) -> Option<f32> {
    finite_material_number(value.as_f64()?)
}

/// Reads an intensity, checking its authored range before rounding to `f32`.
fn parse_emissive_intensity(value: &serde_json::Value) -> Option<f32> {
    let number = value.as_f64()?;
    if !(0.0_f64..=f64::from(MAX_EMISSION_INTENSITY)).contains(&number) {
        return None;
    }
    finite_material_number(number)
}

/// Reads a unit interval, checking its authored range before rounding to `f32`.
fn parse_unit_number(value: &serde_json::Value) -> Option<f32> {
    let number = value.as_f64()?;
    if !(0.0_f64..=1.0_f64).contains(&number) {
        return None;
    }
    finite_material_number(number)
}

/// Parses a `[r, g, b]` unit-RGB array (a tint or emissive colour) from JSON.
fn parse_unit_rgb(value: &serde_json::Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    let mut tint = [0.0f32; 3];
    if array.len() != tint.len() {
        return None;
    }
    for (slot, channel) in tint.iter_mut().zip(array) {
        *slot = parse_unit_number(channel)?;
    }
    Some(tint)
}

//! Material slots and embedded texture decoding for an assembled model.

use std::sync::Arc;

use super::Doc;
use crate::engine::gltf::accessor::{
    buffer_view, json_f32, json_usize, numeric_array, read_u32_be,
};
use crate::engine::gltf::{GltfError, KHR_MATERIALS_EMISSIVE_STRENGTH};
use crate::engine::level::MAX_PROP_TEXTURE_SIZE;
use crate::engine::materials::{
    AlphaMode, DEFAULT_ALPHA_CUTOFF, MaterialAlpha, MaterialEmission, decode_png,
};

/// `baseColorFactor` of a material that does not declare one: no multiply.
const DEFAULT_BASE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// One material resolved into the values a primitive needs.
#[derive(Clone, Copy)]
pub(super) struct ResolvedMaterial {
    pub(super) response: crate::engine::materials::MaterialResponse,
    /// `baseColorFactor` multiplied into every vertex of the material.
    pub(super) color: [f32; 4],
    /// Index into [`PropModel::textures`](super::super::PropModel::textures), or `None` for an untextured material.
    pub(super) texture: Option<u16>,
    /// Material emission, already sanitised.
    pub(super) emission: MaterialEmission,
    /// Material alpha contract, already sanitised.
    pub(super) alpha: MaterialAlpha,
    pub(super) double_sided: bool,
}

impl Default for ResolvedMaterial {
    fn default() -> Self {
        Self {
            response: crate::engine::materials::MaterialResponse::NONE,
            color: DEFAULT_BASE_COLOR,
            texture: None,
            emission: MaterialEmission::NONE,
            alpha: MaterialAlpha::OPAQUE,
            double_sided: false,
        }
    }
}

impl Doc<'_> {
    /// Resolves one declared material (or the synthetic default slot),
    /// decoding and caching anything it references.
    pub(super) fn resolve_material(&mut self, index: usize) -> Result<ResolvedMaterial, GltfError> {
        if let Some(cached) = self.material_cache.get(index).copied().flatten() {
            return Ok(cached);
        }
        if index >= self.default_material_index {
            return Err(GltfError::new(format!(
                "primitive references material {index}, which does not exist"
            )));
        }
        let json = self.json;
        let material = json
            .get("materials")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(index))
            .ok_or_else(|| GltfError::new(format!("material {index} does not exist")))?;
        let pbr = material.get("pbrMetallicRoughness");
        let color = match pbr.and_then(|properties| properties.get("baseColorFactor")) {
            Some(value) => numeric_array::<4>(value, &format!("material {index} baseColorFactor"))?,
            None => DEFAULT_BASE_COLOR,
        };
        if color.iter().any(|channel| !channel.is_finite()) {
            return Err(GltfError::new(format!(
                "material {index} has a non-finite baseColorFactor"
            )));
        }
        let texture = match pbr.and_then(|properties| properties.get("baseColorTexture")) {
            Some(reference) => Some(self.resolve_texture_reference(
                reference,
                &format!("material {index} baseColorTexture"),
            )?),
            None => None,
        };
        let emission = self.resolve_emission(material, index)?;
        let alpha = Self::resolve_alpha(material, index)?;
        let mut response = imported_response(pbr, color)?;
        if let Some(extras) = material.get("extras") {
            let strength = match extras.get("uv_response") {
                Some(value) => json_f32(value).ok_or_else(|| {
                    GltfError::new(format!("material {index} uv_response must be a number"))
                })?,
                None => 0.0,
            };
            if !strength.is_finite()
                || !(0.0..=crate::engine::materials::MAX_UV_RESPONSE).contains(&strength)
            {
                return Err(GltfError::new(format!(
                    "material {index} uv_response must be between 0 and 2"
                )));
            }
            let mask = if let Some(reference) = extras.get("uv_response_mask") {
                if strength <= 0.0 {
                    return Err(GltfError::new(format!(
                        "material {index} uv_response_mask requires positive uv_response"
                    )));
                }
                Some(u32::from(self.resolve_texture_reference(
                    reference,
                    &format!("material {index} uv_response_mask"),
                )?))
            } else {
                None
            };
            let [tint_red, tint_green, tint_blue, _] = color;
            let tint = [tint_red, tint_green, tint_blue];
            if strength > 0.0 && tint.iter().any(|channel| !(0.0..=1.0).contains(channel)) {
                return Err(GltfError::new(format!(
                    "material {index} UV pigment baseColorFactor must be between 0 and 1"
                )));
            }
            response.fluorescence = crate::engine::materials::MaterialFluorescence {
                strength,
                mask,
                tint,
            }
            .sanitized();
        }
        let double_sided = match material.get("doubleSided") {
            None => false,
            Some(serde_json::Value::Bool(value)) => *value,
            Some(_) => {
                return Err(GltfError::new(format!(
                    "material {index} doubleSided is not a boolean"
                )));
            }
        };
        let resolved = ResolvedMaterial {
            response,
            color,
            texture,
            emission,
            alpha,
            double_sided,
        };
        if let Some(slot) = self.material_cache.get_mut(index) {
            *slot = Some(resolved);
        }
        Ok(resolved)
    }

    /// Reads `alphaMode` and `alphaCutoff` into a sanitised
    /// [`MaterialAlpha`].
    ///
    /// `MASK` becomes the game's alpha-tested [`AlphaMode::Cutout`] at the
    /// authored cutoff (defaulting to [`DEFAULT_ALPHA_CUTOFF`]); `BLEND`
    /// becomes [`AlphaMode::Blend`]. A glTF `baseColorFactor` alpha is
    /// already multiplied into every vertex colour by the importer, so the
    /// alpha contract carries the mode only (opacity `1.0`) and the renderer's
    /// translucent routes draw the asset without a per-placement exception.
    /// `OPAQUE` is the default.
    fn resolve_alpha(
        material: &serde_json::Value,
        index: usize,
    ) -> Result<MaterialAlpha, GltfError> {
        let mode = match material.get("alphaMode") {
            Some(serde_json::Value::String(value)) => value.as_str(),
            Some(_) => {
                return Err(GltfError::new(format!(
                    "material {index} alphaMode is not a string"
                )));
            }
            None => "OPAQUE",
        };
        if mode == "BLEND" {
            return Ok(MaterialAlpha::blend(1.0));
        }
        if mode != "MASK" {
            return Ok(MaterialAlpha::OPAQUE);
        }
        let cutoff = match material.get("alphaCutoff") {
            Some(value) => json_f32(value).ok_or_else(|| {
                GltfError::new(format!("material {index} alphaCutoff is not a number"))
            })?,
            None => DEFAULT_ALPHA_CUTOFF,
        };
        Ok(MaterialAlpha {
            mode: AlphaMode::Cutout,
            opacity: 1.0,
            cutoff,
        }
        .sanitized())
    }

    /// Reads `emissiveFactor`, `KHR_materials_emissive_strength` and
    /// `emissiveTexture` into a sanitised [`MaterialEmission`].
    fn resolve_emission(
        &mut self,
        material: &serde_json::Value,
        index: usize,
    ) -> Result<MaterialEmission, GltfError> {
        let color = match material.get("emissiveFactor") {
            Some(value) => numeric_array::<3>(value, &format!("material {index} emissiveFactor"))?,
            None => [0.0; 3],
        };
        let extension = material
            .get("extensions")
            .and_then(|value| value.get(KHR_MATERIALS_EMISSIVE_STRENGTH));
        let intensity = match extension.and_then(|value| value.get("emissiveStrength")) {
            Some(value) => json_f32(value).ok_or_else(|| {
                GltfError::new(format!("material {index} emissiveStrength is not a number"))
            })?,
            None => 1.0,
        };
        let mask = match material.get("emissiveTexture") {
            Some(reference) => Some(self.resolve_texture_reference(
                reference,
                &format!("material {index} emissiveTexture"),
            )?),
            None => None,
        };
        Ok(MaterialEmission::new(color, intensity)
            .with_mask(mask.map(u32::from))
            .sanitized())
    }

    /// Resolves a `baseColorTexture`/`emissiveTexture` reference to an index
    /// in [`PropModel::textures`](super::super::PropModel::textures).
    fn resolve_texture_reference(
        &mut self,
        reference: &serde_json::Value,
        label: &str,
    ) -> Result<u16, GltfError> {
        let index = reference
            .get("index")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new(format!("{label} has no texture index")))?;
        if let Some(tex_coord) = reference.get("texCoord") {
            let coord_index = json_usize(tex_coord)
                .ok_or_else(|| GltfError::new(format!("{label} texCoord is not a number")))?;
            if coord_index != 0 {
                return Err(GltfError::new(format!(
                    "{label} uses texCoord {coord_index}; only TEXCOORD_0 is supported"
                )));
            }
        }
        let json = self.json;
        let texture = json
            .get("textures")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(index))
            .ok_or_else(|| {
                GltfError::new(format!(
                    "{label} references texture {index}, which does not exist"
                ))
            })?;
        let source = texture
            .get("source")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new(format!("texture {index} has no image source")))?;
        self.decode_image(source)
    }

    /// Decodes one embedded PNG image, once per image index.
    fn decode_image(&mut self, image_index: usize) -> Result<u16, GltfError> {
        if let Some(existing) = self.texture_of_image.get(&image_index) {
            return Ok(*existing);
        }
        let json = self.json;
        let image = json
            .get("images")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(image_index))
            .ok_or_else(|| GltfError::new(format!("image {image_index} does not exist")))?;
        if image.get("uri").is_some() {
            return Err(GltfError::new(
                "external or data-URI images are not supported; embed the PNG in the GLB",
            ));
        }
        let mime = image
            .get("mimeType")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if mime != "image/png" {
            return Err(GltfError::new(format!(
                "unsupported texture mime type '{mime}'; only image/png is supported"
            )));
        }
        let view_index = image
            .get("bufferView")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new("image has no bufferView"))?;
        let png = buffer_view(self.json, self.binary, view_index, "image")?;
        if let Some((width, height)) = png_dimensions(png)
            && (width > MAX_PROP_TEXTURE_SIZE || height > MAX_PROP_TEXTURE_SIZE)
        {
            return Err(GltfError::new(format!(
                "texture is {width}x{height}; \
                 the prop limit is {MAX_PROP_TEXTURE_SIZE}x{MAX_PROP_TEXTURE_SIZE}"
            )));
        }
        let decoded = decode_png(png).map_err(|error| {
            GltfError::new(format!("embedded texture is not a valid PNG: {error}"))
        })?;
        let index = u16::try_from(self.textures.len()).map_err(|error| {
            GltfError::new(format!("prop model has too many textures: {error}"))
        })?;
        self.textures.push(Arc::new(decoded));
        let _previous_value = self.texture_of_image.insert(image_index, index);
        Ok(index)
    }
}

/// Import only authored scalar controls. Legacy omitted controls remain matte.
/// This is a compact stylized sheen, not a full metallic BRDF.
fn imported_response(
    pbr: Option<&serde_json::Value>,
    color: [f32; 4],
) -> Result<crate::engine::materials::MaterialResponse, GltfError> {
    let scalar = |field: &str, default: f32| -> Result<f32, GltfError> {
        let Some(value) = pbr.and_then(|p| p.get(field)) else {
            return Ok(default);
        };
        let parsed = numeric_array::<1>(&serde_json::json!([value]), field)?;
        let [number] = parsed;
        if !(0.0..=1.0).contains(&number) {
            return Err(GltfError::new(format!("{field} must be in 0..=1")));
        }
        Ok(number)
    };
    let roughness = scalar("roughnessFactor", 1.0)?;
    let metallic = scalar("metallicFactor", 0.0)?;
    let [r, g, b, _alpha] = color;
    let specular = [r, g, b].map(|channel| {
        (0.55 * metallic).mul_add(channel, 0.04 * (1.0 - metallic)) * (1.0 - roughness)
    });
    Ok(crate::engine::materials::MaterialResponse {
        specular,
        roughness,
        ..crate::engine::materials::MaterialResponse::NONE
    })
}

/// Width and height read straight out of a PNG's `IHDR` header.
///
/// Checking the header first lets an oversized texture be rejected by name
/// before the decoder allocates anything for it.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    if bytes.get(12..16) != Some(b"IHDR".as_slice()) {
        return None;
    }
    let width = read_u32_be(bytes, 16).ok()?;
    let height = read_u32_be(bytes, 20).ok()?;
    Some((width, height))
}

//! GLB container parsing, document admission and import entry points.

use super::accessor::{json_usize, read_u32_le};
use super::mesh::decode_model;
use super::{
    GltfError, KHR_MATERIALS_EMISSIVE_STRENGTH, MAX_ENCODED_GLB_BYTES, PropModel, list_len,
};
use crate::engine::level::{MAX_PROP_ANIMATIONS, MAX_PROP_IMAGES, MAX_PROP_MATERIALS};

pub(super) const GLB_MAGIC: u32 = 0x4654_6C67;
pub(super) const CHUNK_JSON: u32 = 0x4E4F_534A;
pub(super) const CHUNK_BIN: u32 = 0x004E_4942;

pub(super) fn parse_glb_with_limits(
    bytes: &[u8],
    uv_max: f32,
    triangle_limit: usize,
) -> Result<PropModel, GltfError> {
    let (json, binary) = parse_container(bytes)?;
    let materials = validate_document_root(&json)?;
    decode_model(&json, binary, materials, uv_max, triangle_limit)
}

/// Rejects document-level features the prop renderer cannot draw.
///
/// Returns the number of declared materials on success.
fn validate_document_root(json: &serde_json::Value) -> Result<usize, GltfError> {
    validate_extensions(json)?;
    validate_asset_version(json)?;

    let materials = list_len(json, "materials");
    if materials > MAX_PROP_MATERIALS {
        return Err(GltfError::new(format!(
            "prop model declares {materials} materials; the engine ceiling is {MAX_PROP_MATERIALS}"
        )));
    }
    let images = list_len(json, "images");
    if images > MAX_PROP_IMAGES {
        return Err(GltfError::new(format!(
            "prop model embeds {images} images; the engine ceiling is {MAX_PROP_IMAGES}"
        )));
    }
    let animations = list_len(json, "animations");
    if animations > MAX_PROP_ANIMATIONS {
        return Err(GltfError::new(format!(
            "prop model declares {animations} animations; the engine ceiling is {MAX_PROP_ANIMATIONS}"
        )));
    }
    Ok(materials)
}

/// Rejects every glTF extension except `KHR_materials_emissive_strength`.
fn validate_extensions(json: &serde_json::Value) -> Result<(), GltfError> {
    for key in ["extensionsUsed", "extensionsRequired"] {
        let Some(list) = json.get(key).and_then(|value| value.as_array()) else {
            continue;
        };
        for name in list.iter().filter_map(|entry| entry.as_str()) {
            if name != KHR_MATERIALS_EMISSIVE_STRENGTH {
                return Err(GltfError::new(format!(
                    "glTF extensions are not supported by the prop reader: {name} \
                     (only {KHR_MATERIALS_EMISSIVE_STRENGTH} is allowed)"
                )));
            }
        }
    }
    reject_unknown_extensions(json)
}

/// Walks the document and rejects any `extensions` object naming an extension
/// other than the emissive-strength one.
fn reject_unknown_extensions(value: &serde_json::Value) -> Result<(), GltfError> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                // `extras` is arbitrary application data, not glTF structure.
                if key == "extras" {
                    continue;
                }
                if key == "extensions" {
                    let Some(extensions) = child.as_object() else {
                        continue;
                    };
                    for name in extensions.keys() {
                        if name != KHR_MATERIALS_EMISSIVE_STRENGTH {
                            return Err(GltfError::new(format!(
                                "glTF extension {name} is not supported; \
                                 only {KHR_MATERIALS_EMISSIVE_STRENGTH} may be used"
                            )));
                        }
                    }
                    continue;
                }
                reject_unknown_extensions(child)?;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                reject_unknown_extensions(item)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

/// Requires the glTF 2.0 asset declaration.
fn validate_asset_version(json: &serde_json::Value) -> Result<(), GltfError> {
    match json
        .get("asset")
        .and_then(|asset| asset.get("version"))
        .and_then(|value| value.as_str())
    {
        Some("2.0") => Ok(()),
        Some(other) => Err(GltfError::new(format!(
            "asset.version '{other}' is not glTF 2.0"
        ))),
        None => Err(GltfError::new(
            "asset.version is missing; the file is not a glTF 2.0 asset",
        )),
    }
}

pub(super) fn parse_container(bytes: &[u8]) -> Result<(serde_json::Value, &[u8]), GltfError> {
    if bytes.len() > MAX_ENCODED_GLB_BYTES {
        return Err(GltfError::new("encoded GLB exceeds the 64 MiB size limit"));
    }
    if bytes.len() < 12 {
        return Err(GltfError::new("file is too small to be a GLB"));
    }
    let magic = read_u32_le(bytes, 0)?;
    let version = read_u32_le(bytes, 4)?;
    let declared_length = usize::try_from(read_u32_le(bytes, 8)?).map_err(|error| {
        GltfError::new(format!(
            "GLB declared length does not fit this target: {error}"
        ))
    })?;
    if magic != GLB_MAGIC {
        return Err(GltfError::new(
            "not a GLB file; prop models must be self-contained .glb assets",
        ));
    }
    if version != 2 {
        return Err(GltfError::new(format!(
            "unsupported glTF container version {version}; only glTF 2.0 is supported"
        )));
    }
    if declared_length != bytes.len() {
        return Err(GltfError::new(
            "GLB header length does not match the file size",
        ));
    }

    let mut offset: usize = 12;
    let mut json: Option<serde_json::Value> = None;
    let mut binary = None;
    while offset < declared_length {
        if offset
            .checked_add(8)
            .is_none_or(|header_end| header_end > declared_length)
        {
            return Err(GltfError::new("GLB chunk header is truncated"));
        }
        let length = usize::try_from(read_u32_le(bytes, offset)?).map_err(|error| {
            GltfError::new(format!(
                "GLB chunk length does not fit this target: {error}"
            ))
        })?;
        let kind_offset = offset
            .checked_add(4)
            .ok_or_else(|| GltfError::new("GLB chunk offset overflows"))?;
        let kind = read_u32_le(bytes, kind_offset)?;
        if offset == 12 && kind != CHUNK_JSON {
            return Err(GltfError::new("the first GLB chunk must be JSON"));
        }
        if !length.is_multiple_of(4) {
            return Err(GltfError::new(
                "GLB chunk length is not aligned to four bytes",
            ));
        }
        let start = offset
            .checked_add(8)
            .ok_or_else(|| GltfError::new("GLB chunk offset overflows"))?;
        let Some(end) = start.checked_add(length) else {
            return Err(GltfError::new("GLB chunk length overflows"));
        };
        if end > declared_length || end > bytes.len() {
            return Err(GltfError::new("GLB chunk is truncated"));
        }
        match kind {
            CHUNK_JSON => {
                if json.is_some() {
                    return Err(GltfError::new("GLB has more than one JSON chunk"));
                }
                let text = std::str::from_utf8(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| GltfError::new("GLB chunk is truncated"))?,
                )
                .map_err(|error| {
                    GltfError::new(format!("GLB JSON chunk is not valid UTF-8: {error}"))
                })?;
                let value: serde_json::Value =
                    serde_json::from_str(text.trim_end_matches(['\0', ' ']))
                        .map_err(|error| GltfError::new(format!("Invalid glTF JSON: {error}")))?;
                json = Some(value);
            }
            CHUNK_BIN => {
                if binary.is_some() {
                    return Err(GltfError::new("GLB has more than one binary chunk"));
                }
                binary = Some(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| GltfError::new("GLB chunk is truncated"))?,
                );
            }
            _ => {}
        }
        offset = end;
    }

    let json_chunk = json.ok_or_else(|| GltfError::new("GLB has no JSON chunk"))?;
    let payload = validate_binary_buffer(&json_chunk, binary.unwrap_or_default())?;
    Ok((json_chunk, payload))
}

/// Only the GLB's one local buffer is available to every view. Restrict reads
/// to its declared length, excluding the container's optional alignment bytes.
fn validate_binary_buffer<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
) -> Result<&'a [u8], GltfError> {
    let buffers = json
        .get("buffers")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| GltfError::new("self-contained GLB has no buffers array"))?;
    let [buffer] = buffers.as_slice() else {
        return Err(GltfError::new(
            "self-contained GLB must declare exactly one buffer",
        ));
    };
    if buffer.get("uri").is_some() {
        return Err(GltfError::new(
            "external or data-URI buffers are not supported",
        ));
    }
    let length = buffer
        .get("byteLength")
        .and_then(json_usize)
        .filter(|length| *length > 0)
        .ok_or_else(|| GltfError::new("GLB buffer has no positive byteLength"))?;
    if binary
        .len()
        .checked_sub(length)
        .is_none_or(|padding| padding > 3)
    {
        return Err(GltfError::new(
            "GLB binary chunk does not match buffer byteLength",
        ));
    }
    if let Some(views) = json
        .get("bufferViews")
        .and_then(serde_json::Value::as_array)
    {
        for view in views {
            if view.get("buffer").and_then(json_usize) != Some(0) {
                return Err(GltfError::new(
                    "bufferView must reference the GLB's local buffer 0",
                ));
            }
        }
    }
    binary
        .get(..length)
        .ok_or_else(|| GltfError::new("GLB buffer is truncated"))
}

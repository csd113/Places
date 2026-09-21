//! Minimal, dependency-free GLB (binary glTF 2.0) reader for prop models.
//!
//! The prop pipeline in `tools/props` only ever emits one narrow, deliberately
//! boring GLB profile, and this reader accepts exactly that profile plus a few
//! harmless variations (see `assets/props/README.md` for the asset rules):
//!
//! * GLB container, glTF 2.0, one scene/node/mesh/primitive;
//! * `POSITION` (float32), `TEXCOORD_0` (float32 or normalised integer),
//!   `COLOR_0` (optional; float32 or normalised integer), 16/32-bit indices;
//! * `mode: 4` (triangles) only, no skins, no morph targets, no animation;
//! * one PNG texture embedded in a bufferView (self-contained, no external files);
//! * no glTF extensions.
//!
//! Everything else produces a descriptive [`GltfError`] so a malformed asset
//! degrades into the loader's placeholder box instead of panicking or looping.

use crate::level::{MAX_PROP_TEXTURE_SIZE, MAX_PROP_TRIANGLES, MAX_PROP_VERTICES};
use crate::loader::RawImage;

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

const COMPONENT_FLOAT: u32 = 5126;
const COMPONENT_UBYTE: u32 = 5121;
const COMPONENT_USHORT: u32 = 5123;
const COMPONENT_UINT: u32 = 5125;

const MODE_TRIANGLES: u32 = 4;

/// A parse failure with a message meant for a developer reading the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfError(pub String);

impl std::fmt::Display for GltfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for GltfError {}

impl GltfError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// One vertex of a loaded prop model: position, baked diffuse tint and UV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

/// A decoded, ready-to-render prop model.
#[derive(Clone, Debug)]
pub struct PropModel {
    /// Model-space vertices with baked per-face shading in `color`.
    pub vertices: Vec<PropVertex>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u16>,
    /// Embedded diffuse texture, decoded to 8-bit RGBA.
    pub texture: RawImage,
    /// Triangle count (a multiple of three indices), used for budget checks.
    pub triangles: usize,
    /// Number of materials declared by the asset (props must use exactly one).
    pub materials: usize,
}

impl PropModel {
    /// Axis-aligned model-space bounds, or `None` for an empty mesh.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?;
        let mut min = first.pos;
        let mut max = first.pos;
        for vertex in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.pos[axis]);
                max[axis] = max[axis].max(vertex.pos[axis]);
            }
        }
        Some((min, max))
    }
}

// ------------------------------------------------------------------- parsing

/// Parses a self-contained GLB prop asset.
pub fn parse_glb(bytes: &[u8]) -> Result<PropModel, GltfError> {
    let (json, binary) = parse_container(bytes)?;

    if let Some(list) = json
        .get("extensionsUsed")
        .and_then(|value| value.as_array())
        && !list.is_empty()
    {
        let names: Vec<&str> = list.iter().filter_map(|item| item.as_str()).collect();
        return Err(GltfError::new(format!(
            "glTF extensions are not supported by the GLES2 prop renderer: {}",
            names.join(", ")
        )));
    }
    if json.get("skins").is_some() {
        return Err(GltfError::new("skinned meshes are not supported"));
    }
    if json.get("animations").is_some() {
        return Err(GltfError::new("animated prop assets are not supported"));
    }

    // Nodes must be transform-free: instance transforms come from the level.
    if let Some(nodes) = json.get("nodes").and_then(|value| value.as_array()) {
        for node in nodes {
            if node.get("mesh").is_none() {
                continue;
            }
            let has_transform = node.get("matrix").is_some()
                || node.get("translation").is_some()
                || node.get("rotation").is_some()
                || node.get("scale").is_some()
                || node.get("children").is_some();
            if has_transform {
                return Err(GltfError::new(
                    "node transforms are not supported; props are authored in engine space \
                     with the origin at the floor-contact point",
                ));
            }
        }
    }

    let meshes = json
        .get("meshes")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no meshes"))?;
    if meshes.len() != 1 {
        return Err(GltfError::new(format!(
            "expected exactly one mesh, found {}",
            meshes.len()
        )));
    }
    let primitives = meshes[0]
        .get("primitives")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("mesh has no primitives"))?;

    let mut vertices: Vec<PropVertex> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    for primitive in primitives {
        let mode = primitive
            .get("mode")
            .and_then(|value| value.as_u64())
            .unwrap_or(4) as u32;
        if mode != MODE_TRIANGLES {
            return Err(GltfError::new(format!(
                "primitive mode {mode} is not TRIANGLES (4)"
            )));
        }
        let attributes = primitive
            .get("attributes")
            .and_then(|value| value.as_object())
            .ok_or_else(|| GltfError::new("primitive has no attributes"))?;

        let positions = read_vec(&json, &binary, attribute(attributes, "POSITION")?, 3)?;
        let Some(uv_accessor) = attributes.get("TEXCOORD_0") else {
            return Err(GltfError::new(
                "primitive has no TEXCOORD_0; every prop must be UV mapped",
            ));
        };
        let uvs = read_vec(
            &json,
            &binary,
            accessor_index(uv_accessor, "TEXCOORD_0")?,
            2,
        )?;
        let colors = match attributes.get("COLOR_0") {
            Some(value) => read_vec(&json, &binary, accessor_index(value, "COLOR_0")?, 4)?,
            None => vec![vec![1.0, 1.0, 1.0, 1.0]; positions.len()],
        };
        if positions.len() != uvs.len() || positions.len() != colors.len() {
            return Err(GltfError::new(
                "POSITION, TEXCOORD_0 and COLOR_0 attribute counts differ",
            ));
        }

        let base = vertices.len();
        for index in 0..positions.len() {
            vertices.push(PropVertex {
                pos: [
                    positions[index][0],
                    positions[index][1],
                    positions[index][2],
                ],
                uv: [uvs[index][0], uvs[index][1]],
                color: [
                    colors[index][0],
                    colors[index][1],
                    colors[index][2],
                    if colors[index].len() == 4 {
                        colors[index][3]
                    } else {
                        1.0
                    },
                ],
            });
        }

        let local_indices = match primitive.get("indices") {
            Some(value) => read_indices(&json, &binary, accessor_index(value, "indices")?)?,
            None => (0..positions.len() as u32).collect(),
        };
        if local_indices.len() % 3 != 0 {
            return Err(GltfError::new(
                "index count is not a multiple of three; props must be triangle lists",
            ));
        }
        for value in local_indices {
            let absolute = base + value as usize;
            if absolute >= base + positions.len() {
                return Err(GltfError::new(format!(
                    "index {value} points outside the primitive's vertices"
                )));
            }
            if absolute > u16::MAX as usize {
                return Err(GltfError::new(format!(
                    "mesh needs more than {MAX_PROP_VERTICES} vertices; lower the prop's detail"
                )));
            }
            indices.push(absolute as u16);
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        return Err(GltfError::new("mesh contains no triangles"));
    }
    if vertices.len() > MAX_PROP_VERTICES {
        return Err(GltfError::new(format!(
            "mesh has {} vertices; the prop limit is {MAX_PROP_VERTICES}",
            vertices.len()
        )));
    }
    let triangles = indices.len() / 3;
    if triangles > MAX_PROP_TRIANGLES {
        return Err(GltfError::new(format!(
            "mesh has {triangles} triangles; the PocketCHIP prop ceiling is {MAX_PROP_TRIANGLES}"
        )));
    }
    for vertex in &vertices {
        for value in vertex
            .pos
            .iter()
            .chain(vertex.uv.iter())
            .chain(vertex.color.iter())
        {
            if !value.is_finite() {
                return Err(GltfError::new(
                    "mesh contains a non-finite vertex value; the asset is malformed",
                ));
            }
        }
        if vertex.uv[0] < -0.01
            || vertex.uv[0] > 1.01
            || vertex.uv[1] < -0.01
            || vertex.uv[1] > 1.01
        {
            return Err(GltfError::new(format!(
                "UV {:.3},{:.3} lies outside 0..1; props use non-tiling UVs",
                vertex.uv[0], vertex.uv[1]
            )));
        }
    }

    let texture = read_texture(&json, &binary)?;
    let materials = json
        .get("materials")
        .and_then(|value| value.as_array())
        .map(|list| list.len())
        .unwrap_or(0);
    if materials != 1 {
        return Err(GltfError::new(format!(
            "prop model declares {materials} materials; every prop uses exactly one diffuse material"
        )));
    }
    Ok(PropModel {
        vertices,
        indices,
        texture,
        triangles,
        materials,
    })
}

fn parse_container(bytes: &[u8]) -> Result<(serde_json::Value, Vec<u8>), GltfError> {
    if bytes.len() < 12 {
        return Err(GltfError::new("file is too small to be a GLB"));
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let declared_length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
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
    if declared_length > bytes.len() {
        return Err(GltfError::new("GLB header length exceeds the file size"));
    }

    let mut offset = 12;
    let mut json: Option<serde_json::Value> = None;
    let mut binary: Vec<u8> = Vec::new();
    while offset + 8 <= declared_length {
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
        let start = offset + 8;
        let Some(end) = start.checked_add(length) else {
            return Err(GltfError::new("GLB chunk length overflows"));
        };
        if end > declared_length || end > bytes.len() {
            return Err(GltfError::new("GLB chunk is truncated"));
        }
        match kind {
            CHUNK_JSON => {
                let text = std::str::from_utf8(&bytes[start..end])
                    .map_err(|_| GltfError::new("GLB JSON chunk is not valid UTF-8"))?;
                let value: serde_json::Value =
                    serde_json::from_str(text.trim_end_matches(['\0', ' ']))
                        .map_err(|error| GltfError::new(format!("Invalid glTF JSON: {error}")))?;
                json = Some(value);
            }
            CHUNK_BIN => binary = bytes[start..end].to_vec(),
            _ => {}
        }
        offset = end;
    }

    let json = json.ok_or_else(|| GltfError::new("GLB has no JSON chunk"))?;
    Ok((json, binary))
}

struct AccessorView<'a> {
    data: &'a [u8],
    stride: usize,
    element_size: usize,
    count: usize,
    component_type: u32,
    normalized: bool,
    components: usize,
}

fn accessor_view<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    index: usize,
    components: usize,
) -> Result<AccessorView<'a>, GltfError> {
    let accessors = json
        .get("accessors")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no accessors"))?;
    let accessor = accessors
        .get(index)
        .ok_or_else(|| GltfError::new(format!("accessor {index} does not exist")))?;

    let declared_components = match accessor.get("type").and_then(|value| value.as_str()) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        other => {
            return Err(GltfError::new(format!(
                "unsupported accessor type {:?}",
                other.unwrap_or("(missing)")
            )));
        }
    };
    if declared_components != components {
        return Err(GltfError::new(format!(
            "accessor {index} has {declared_components} components; expected {components}"
        )));
    }

    let component_type = accessor
        .get("componentType")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no componentType")))?
        as u32;
    let component_size = match component_type {
        COMPONENT_FLOAT => 4,
        COMPONENT_UBYTE => 1,
        COMPONENT_USHORT => 2,
        COMPONENT_UINT => 4,
        other => {
            return Err(GltfError::new(format!(
                "unsupported accessor componentType {other}"
            )));
        }
    };
    let count = accessor
        .get("count")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no count")))?
        as usize;

    let view_index = accessor
        .get("bufferView")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no bufferView")))?
        as usize;
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(view_index)
        .ok_or_else(|| GltfError::new(format!("bufferView {view_index} does not exist")))?;

    let view_offset = view
        .get("byteOffset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let view_length =
        view.get("byteLength")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| GltfError::new("bufferView has no byteLength"))? as usize;
    let accessor_offset = accessor
        .get("byteOffset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let start = view_offset
        .checked_add(accessor_offset)
        .ok_or_else(|| GltfError::new("accessor byte offset overflows"))?;
    let end = start
        .checked_add(view_length)
        .ok_or_else(|| GltfError::new("bufferView length overflows"))?;
    if end > binary.len() {
        return Err(GltfError::new(
            "bufferView extends past the end of the binary chunk; the GLB is truncated",
        ));
    }

    let element_size = component_size * components;
    let stride = view
        .get("byteStride")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(element_size);
    let required = if count == 0 {
        0
    } else {
        (count - 1) * stride + element_size
    };
    if required > end - start {
        return Err(GltfError::new(format!(
            "accessor {index} declares {count} elements but its bufferView is too small"
        )));
    }

    Ok(AccessorView {
        data: &binary[start..end],
        stride,
        element_size,
        count,
        component_type,
        normalized: accessor
            .get("normalized")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        components,
    })
}

fn read_vec(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
    components: usize,
) -> Result<Vec<Vec<f32>>, GltfError> {
    let view = accessor_view(json, binary, index, components)?;
    let component_size = view.element_size / view.components;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element * view.stride;
        let mut values = Vec::with_capacity(view.components);
        for component in 0..view.components {
            let offset = base + component * component_size;
            let value = match view.component_type {
                COMPONENT_FLOAT => {
                    f32::from_le_bytes(view.data[offset..offset + 4].try_into().unwrap())
                }
                COMPONENT_UBYTE => {
                    let raw = view.data[offset];
                    if view.normalized {
                        raw as f32 / 255.0
                    } else {
                        raw as f32
                    }
                }
                COMPONENT_USHORT => {
                    let raw = u16::from_le_bytes(view.data[offset..offset + 2].try_into().unwrap());
                    if view.normalized {
                        raw as f32 / 65_535.0
                    } else {
                        raw as f32
                    }
                }
                _ => u32::from_le_bytes(view.data[offset..offset + 4].try_into().unwrap()) as f32,
            };
            values.push(value);
        }
        out.push(values);
    }
    Ok(out)
}

fn read_indices(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<u32>, GltfError> {
    let view = accessor_view(json, binary, index, 1)?;
    let component_size = view.element_size;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let offset = element * view.stride;
        out.push(match view.component_type {
            COMPONENT_UBYTE => view.data[offset] as u32,
            COMPONENT_USHORT => {
                u16::from_le_bytes(view.data[offset..offset + 2].try_into().unwrap()) as u32
            }
            COMPONENT_UINT => u32::from_le_bytes(view.data[offset..offset + 4].try_into().unwrap()),
            other => {
                return Err(GltfError::new(format!(
                    "componentType {other} cannot be used for indices (size {component_size})"
                )));
            }
        });
    }
    Ok(out)
}

fn attribute(
    attributes: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<usize, GltfError> {
    attributes
        .get(key)
        .ok_or_else(|| GltfError::new(format!("primitive has no {key} attribute")))
        .and_then(|value| accessor_index(value, key))
}

fn accessor_index(value: &serde_json::Value, key: &str) -> Result<usize, GltfError> {
    value
        .as_u64()
        .map(|index| index as usize)
        .ok_or_else(|| GltfError::new(format!("{key} is not an accessor index")))
}

fn read_texture(json: &serde_json::Value, binary: &[u8]) -> Result<RawImage, GltfError> {
    let textures = json
        .get("textures")
        .and_then(|value| value.as_array())
        .filter(|list| !list.is_empty())
        .ok_or_else(|| GltfError::new("prop model has no texture"))?;
    let images = json
        .get("images")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no images"))?;
    let source = textures[0]
        .get("source")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| GltfError::new("texture has no image source"))? as usize;
    let image = images
        .get(source)
        .ok_or_else(|| GltfError::new(format!("image {source} does not exist")))?;
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
        .and_then(|value| value.as_u64())
        .ok_or_else(|| GltfError::new("image has no bufferView"))? as usize;
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(view_index)
        .ok_or_else(|| GltfError::new(format!("bufferView {view_index} does not exist")))?;
    let offset = view
        .get("byteOffset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let length =
        view.get("byteLength")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| GltfError::new("image bufferView has no byteLength"))? as usize;
    let Some(end) = offset.checked_add(length) else {
        return Err(GltfError::new("image bufferView length overflows"));
    };
    if end > binary.len() {
        return Err(GltfError::new(
            "image bufferView extends past the binary chunk",
        ));
    }
    let png = &binary[offset..end];
    let image = crate::loader::decode_png(png)
        .map_err(|error| GltfError::new(format!("embedded texture is not a valid PNG: {error}")))?;
    if image.width > MAX_PROP_TEXTURE_SIZE || image.height > MAX_PROP_TEXTURE_SIZE {
        return Err(GltfError::new(format!(
            "texture is {}x{}; the PocketCHIP prop limit is {MAX_PROP_TEXTURE_SIZE}x{MAX_PROP_TEXTURE_SIZE}",
            image.width, image.height
        )));
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real, shipped prop asset: the parser must accept what the toolkit writes.
    const CHAIR_GLB: &[u8] = include_bytes!("../assets/props/models/chair.glb");
    /// 1x1 opaque PNG used by the synthetic fixtures below.
    const PIXEL_PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 56, 81, 17, 245, 31,
        0, 6, 64, 2, 154, 192, 122, 5, 31, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn glb_container(json: &str, binary: &[u8]) -> Vec<u8> {
        let mut json_chunk = json.as_bytes().to_vec();
        while !json_chunk.len().is_multiple_of(4) {
            json_chunk.push(b' ');
        }
        let mut bin_chunk = binary.to_vec();
        while !bin_chunk.len().is_multiple_of(4) {
            bin_chunk.push(0);
        }
        let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        out.extend_from_slice(&json_chunk);
        out.extend_from_slice(&(bin_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&bin_chunk);
        out
    }

    /// A one-triangle GLB with 16-bit indices, normalised byte colours and an
    /// embedded PNG, i.e. the narrow profile the toolkit emits.
    fn minimal_triangle_glb() -> Vec<u8> {
        let (json, binary) = minimal_triangle_parts();
        glb_container(&json, &binary)
    }

    /// The JSON and binary chunks used by [`minimal_triangle_glb`], exposed so
    /// tests can mutate one part of the document.
    fn minimal_triangle_parts() -> (String, Vec<u8>) {
        let mut binary = Vec::new();
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        for position in positions {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        while binary.len() % 4 != 0 {
            binary.push(0);
        }
        let uv_offset = binary.len();
        for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for value in uv {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let color_offset = binary.len();
        for color in [[255u8, 128, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
            binary.extend_from_slice(&color);
        }
        while binary.len() % 2 != 0 {
            binary.push(0);
        }
        let index_offset = binary.len();
        for index in [0u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let image_offset = binary.len();
        binary.extend_from_slice(PIXEL_PNG);

        let json = format!(
            r#"{{
              "asset": {{"version": "2.0"}},
              "scene": 0,
              "scenes": [{{"nodes": [0]}}],
              "nodes": [{{"mesh": 0}}],
              "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "TEXCOORD_0": 1, "COLOR_0": 2}}, "indices": 3, "material": 0, "mode": 4}}]}}],
              "accessors": [
                {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}},
                {{"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"}},
                {{"bufferView": 2, "componentType": 5121, "normalized": true, "count": 3, "type": "VEC4"}},
                {{"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"}}
              ],
              "bufferViews": [
                {{"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962}},
                {{"buffer": 0, "byteOffset": {uv_offset}, "byteLength": 24, "target": 34962}},
                {{"buffer": 0, "byteOffset": {color_offset}, "byteLength": 12, "target": 34962}},
                {{"buffer": 0, "byteOffset": {index_offset}, "byteLength": 6, "target": 34963}},
                {{"buffer": 0, "byteOffset": {image_offset}, "byteLength": {png_length}}}
              ],
              "buffers": [{{"byteLength": {buffer_length}}}],
              "images": [{{"bufferView": 4, "mimeType": "image/png"}}],
              "samplers": [{{"magFilter": 9729, "minFilter": 9987, "wrapS": 33071, "wrapT": 33071}}],
              "textures": [{{"sampler": 0, "source": 0}}],
              "materials": [{{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}}]
            }}"#,
            png_length = PIXEL_PNG.len(),
            buffer_length = binary.len()
        );
        (json, binary)
    }

    #[test]
    fn parses_the_shipped_chair_asset() {
        let model = parse_glb(CHAIR_GLB).expect("shipped chair.glb must parse");
        assert!(model.triangles > 0);
        assert_eq!(model.indices.len(), model.triangles * 3);
        assert_eq!(model.texture.width, 64);
        assert_eq!(model.texture.height, 64);
        assert_eq!(model.texture.rgba.len(), 64 * 64 * 4);

        // Origin convention: base on y = 0, horizontally centred, metres.
        let (low, high) = model.bounds().expect("chair has vertices");
        assert!(low[1].abs() < 0.012, "chair base sits at {}", low[1]);
        assert!(((low[0] + high[0]) * 0.5).abs() < 0.02);
        assert!(((low[2] + high[2]) * 0.5).abs() < 0.02);
        assert!((high[0] - low[0] - 0.5).abs() < 0.05);
        assert!((high[1] - low[1] - 0.9).abs() < 0.05);
    }

    #[test]
    fn parses_normalised_colours_and_16_bit_indices() {
        let model = parse_glb(&minimal_triangle_glb()).expect("synthetic triangle parses");
        assert_eq!(model.triangles, 1);
        assert_eq!(model.vertices.len(), 3);
        assert!((model.vertices[0].color[0] - 1.0).abs() < 1e-6);
        assert!((model.vertices[0].color[1] - 128.0 / 255.0).abs() < 1e-3);
        assert!((model.vertices[1].color[1] - 1.0).abs() < 1e-6);
        assert_eq!(model.texture.width, 1);
    }

    #[test]
    fn rejects_malformed_assets_with_actionable_messages() {
        let json_cases: [(&str, &str, &str); 4] = [
            (
                "empty mesh list",
                r#"{"asset":{"version":"2.0"},"meshes":[]}"#,
                "mesh",
            ),
            (
                "unsupported extension",
                r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_materials_clearcoat"],"meshes":[{"primitives":[]}]}"#,
                "extensions",
            ),
            (
                "skinned mesh",
                r#"{"asset":{"version":"2.0"},"skins":[{}],"meshes":[{"primitives":[]}]}"#,
                "skinned",
            ),
            (
                "animated asset",
                r#"{"asset":{"version":"2.0"},"animations":[{}],"meshes":[{"primitives":[]}]}"#,
                "animated",
            ),
        ];
        for (label, json, expected) in json_cases {
            let bytes = glb_container(json, b"\0\0\0\0");
            let error = parse_glb(&bytes).expect_err(&format!("{label} must not parse"));
            assert!(
                error.0.to_lowercase().contains(expected),
                "{label}: message {:?} should mention {expected:?}",
                error.0
            );
        }

        let raw_cases: [(&str, &[u8]); 2] = [
            ("not a GLB at all", b"hello world, definitely not a model"),
            ("empty file", &[]),
        ];
        for (label, payload) in raw_cases {
            let error = parse_glb(payload).expect_err(&format!("{label} must not parse"));
            assert!(
                error.0.contains("GLB"),
                "{label}: message {:?} should mention GLB",
                error.0
            );
        }
    }

    #[test]
    fn rejects_unsupported_primitive_shapes() {
        let json =
            r#"{"asset":{"version":"2.0"},"meshes":[{"primitives":[{"attributes":{},"mode":1}]}]}"#;
        let error =
            parse_glb(&glb_container(json, b"\0\0\0\0")).expect_err("mode 1 is not triangles");
        assert!(error.0.contains("TRIANGLES"), "{}", error.0);

        let (mut json, binary) = minimal_triangle_parts();
        json = json.replace("\"TEXCOORD_0\": 1, ", "");
        let error = parse_glb(&glb_container(&json, &binary)).expect_err("a prop needs UVs");
        assert!(
            error.0.to_lowercase().contains("uv"),
            "a prop without UVs must say so: {}",
            error.0
        );
    }

    #[test]
    fn rejects_truncated_and_oversized_containers() {
        let mut truncated = minimal_triangle_glb();
        truncated.truncate(truncated.len() - 40);
        assert!(parse_glb(&truncated).is_err());

        let mut declared_too_long = minimal_triangle_glb();
        declared_too_long[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = parse_glb(&declared_too_long).expect_err("oversized header");
        assert!(error.0.contains("exceeds"), "{}", error.0);
    }
}

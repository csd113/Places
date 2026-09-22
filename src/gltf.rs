//! Minimal, dependency-free GLB (binary glTF 2.0) reader for prop models.
//!
//! The prop pipeline in `tools/props` only ever emits one narrow, deliberately
//! boring GLB profile, and this reader accepts exactly that profile plus a few
//! harmless variations (see `assets/README.md` for the asset rules):
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
    #[must_use]
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
///
/// # Errors
///
/// Returns a [`GltfError`] naming the first problem found: a container that is
/// not a self-contained glTF 2.0 GLB, a feature the GLES2 prop renderer cannot
/// draw (extensions, skins, animations or node transforms), a mesh that is not
/// a single triangle list inside the prop budgets, or vertex data that is
/// non-finite or outside the documented UV range.
pub fn parse_glb(bytes: &[u8]) -> Result<PropModel, GltfError> {
    let (json, binary) = parse_container(bytes)?;
    validate_document_root(&json)?;
    let primitives = mesh_primitives(&json)?;

    let mut vertices: Vec<PropVertex> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    for primitive in primitives {
        read_primitive(&json, &binary, primitive, &mut vertices, &mut indices)?;
    }

    let triangles = validate_mesh(&vertices, &indices)?;
    let texture = read_texture(&json, &binary)?;
    let materials = json
        .get("materials")
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len);
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

/// Rejects document features the GLES2 prop renderer cannot draw.
fn validate_document_root(json: &serde_json::Value) -> Result<(), GltfError> {
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
    Ok(())
}

/// The single mesh's primitive list.
fn mesh_primitives(json: &serde_json::Value) -> Result<&[serde_json::Value], GltfError> {
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
    meshes[0]
        .get("primitives")
        .and_then(|value| value.as_array())
        .map(std::vec::Vec::as_slice)
        .ok_or_else(|| GltfError::new("mesh has no primitives"))
}

/// Reads one primitive's vertices and triangle indices into the model.
fn read_primitive(
    json: &serde_json::Value,
    binary: &[u8],
    primitive: &serde_json::Value,
    vertices: &mut Vec<PropVertex>,
    indices: &mut Vec<u16>,
) -> Result<(), GltfError> {
    let mode = primitive
        .get("mode")
        .and_then(json_u32)
        .unwrap_or(MODE_TRIANGLES);
    if mode != MODE_TRIANGLES {
        return Err(GltfError::new(format!(
            "primitive mode {mode} is not TRIANGLES (4)"
        )));
    }
    let attributes = primitive
        .get("attributes")
        .and_then(|value| value.as_object())
        .ok_or_else(|| GltfError::new("primitive has no attributes"))?;

    let positions = read_vec(json, binary, attribute(attributes, "POSITION")?, 3)?;
    let Some(uv_accessor) = attributes.get("TEXCOORD_0") else {
        return Err(GltfError::new(
            "primitive has no TEXCOORD_0; every prop must be UV mapped",
        ));
    };
    let uvs = read_vec(json, binary, accessor_index(uv_accessor, "TEXCOORD_0")?, 2)?;
    let colors = match attributes.get("COLOR_0") {
        Some(value) => read_vec(json, binary, accessor_index(value, "COLOR_0")?, 4)?,
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
        Some(value) => read_indices(json, binary, accessor_index(value, "indices")?)?,
        None => (0..positions.len())
            .map(|value| u32::try_from(value).unwrap_or(u32::MAX))
            .collect(),
    };
    if local_indices.len() % 3 != 0 {
        return Err(GltfError::new(
            "index count is not a multiple of three; props must be triangle lists",
        ));
    }
    for value in local_indices {
        let absolute = base + usize::try_from(value).unwrap_or(usize::MAX);
        if absolute >= base + positions.len() {
            return Err(GltfError::new(format!(
                "index {value} points outside the primitive's vertices"
            )));
        }
        if absolute > usize::from(u16::MAX) {
            return Err(GltfError::new(format!(
                "mesh needs more than {MAX_PROP_VERTICES} vertices; lower the prop's detail"
            )));
        }
        indices.push(u16::try_from(absolute).unwrap_or(u16::MAX));
    }
    Ok(())
}

/// Checks the assembled mesh against the prop budgets and data invariants.
///
/// Returns the triangle count on success, so the caller can store it without
/// recomputing it from the index list.
fn validate_mesh(vertices: &[PropVertex], indices: &[u16]) -> Result<usize, GltfError> {
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
    for vertex in vertices {
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
    Ok(triangles)
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

/// `usize` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in this target's pointer width.
fn json_usize(value: &serde_json::Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}

/// `u32` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in 32 bits.
fn json_u32(value: &serde_json::Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
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

    let declared_components = accessor_components(accessor);
    if declared_components != components {
        return Err(GltfError::new(format!(
            "accessor {index} has {declared_components} components; expected {components}"
        )));
    }
    let component_type = accessor
        .get("componentType")
        .and_then(json_u32)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no componentType")))?;
    let component_size = component_size(component_type)?;
    let count = accessor
        .get("count")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no count")))?;
    let element_size = component_size * components;
    let (data, stride) = accessor_data(json, binary, accessor, index, element_size, count)?;

    Ok(AccessorView {
        data,
        stride,
        element_size,
        count,
        component_type,
        normalized: accessor
            .get("normalized")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        components,
    })
}

/// Number of components an accessor's `type` string declares.
fn accessor_components(accessor: &serde_json::Value) -> usize {
    match accessor.get("type").and_then(|value| value.as_str()) {
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        Some("SCALAR") => 1,
        // A missing or unknown type is reported by the caller's size check as
        // zero components, which no accessor request can match.
        _ => 0,
    }
}

/// Bytes per element of an accessor component type.
fn component_size(component_type: u32) -> Result<usize, GltfError> {
    match component_type {
        COMPONENT_FLOAT | COMPONENT_UINT => Ok(4),
        COMPONENT_UBYTE => Ok(1),
        COMPONENT_USHORT => Ok(2),
        other => Err(GltfError::new(format!(
            "unsupported accessor componentType {other}"
        ))),
    }
}

/// The binary slice an accessor addresses, plus its element stride in bytes.
fn accessor_data<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    accessor: &serde_json::Value,
    index: usize,
    element_size: usize,
    count: usize,
) -> Result<(&'a [u8], usize), GltfError> {
    let view_index = accessor
        .get("bufferView")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no bufferView")))?;
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(view_index)
        .ok_or_else(|| GltfError::new(format!("bufferView {view_index} does not exist")))?;

    let view_offset = view.get("byteOffset").and_then(json_usize).unwrap_or(0);
    let view_length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("bufferView has no byteLength"))?;
    let accessor_offset = accessor.get("byteOffset").and_then(json_usize).unwrap_or(0);
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

    let stride = view
        .get("byteStride")
        .and_then(json_usize)
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
    Ok((&binary[start..end], stride))
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
                        f32::from(raw) / 255.0
                    } else {
                        f32::from(raw)
                    }
                }
                COMPONENT_USHORT => {
                    let raw = u16::from_le_bytes(view.data[offset..offset + 2].try_into().unwrap());
                    if view.normalized {
                        f32::from(raw) / 65_535.0
                    } else {
                        f32::from(raw)
                    }
                }
                other => {
                    return Err(GltfError::new(format!(
                        "componentType {other} cannot be used for vertex attributes"
                    )));
                }
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
            COMPONENT_UBYTE => u32::from(view.data[offset]),
            COMPONENT_USHORT => u32::from(u16::from_le_bytes(
                view.data[offset..offset + 2].try_into().unwrap(),
            )),
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
    json_usize(value).ok_or_else(|| GltfError::new(format!("{key} is not an accessor index")))
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
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("texture has no image source"))?;
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
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("image has no bufferView"))?;
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(view_index)
        .ok_or_else(|| GltfError::new(format!("bufferView {view_index} does not exist")))?;
    let offset = view.get("byteOffset").and_then(json_usize).unwrap_or(0);
    let length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("image bufferView has no byteLength"))?;
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
mod tests;

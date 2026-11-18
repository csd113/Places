//! Bounded glTF buffer views and typed accessor decoding.

use super::GltfError;

pub(super) const COMPONENT_FLOAT: u32 = 5126;
const COMPONENT_UBYTE: u32 = 5121;
const COMPONENT_USHORT: u32 = 5123;
const COMPONENT_UINT: u32 = 5125;

/// Copies a JSON array of exactly `N` numbers into an `f32` array.
pub(super) fn numeric_array<const N: usize>(
    value: &serde_json::Value,
    label: &str,
) -> Result<[f32; N], GltfError> {
    let items = value
        .as_array()
        .ok_or_else(|| GltfError::new(format!("{label} must be an array of {N} numbers")))?;
    if items.len() != N {
        return Err(GltfError::new(format!(
            "{label} must be an array of {N} numbers, found {}",
            items.len()
        )));
    }
    let mut out = [0.0f32; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = json_f32(item).ok_or_else(|| {
            GltfError::new(format!("{label} contains a value that is not a number"))
        })?;
    }
    Ok(out)
}

/// `f32` value of a JSON number, or `None` when it is absent or not a number.
///
/// glTF scalar values are authored as JSON numbers and consumed as `f32` by
/// the renderer, so the narrowing cast is the specified conversion; values
/// beyond `f32`'s range become infinite and are rejected by the finiteness
/// checks at the call sites.
pub(super) fn json_f32(value: &serde_json::Value) -> Option<f32> {
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "glTF scalars are f32 by definition"
    )] // glTF scalars are f32 by definition
    let scalar = value.as_f64()? as f32;
    Some(scalar)
}

/// The binary slice one bufferView addresses.
pub(super) fn buffer_view<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    index: usize,
    what: &str,
) -> Result<&'a [u8], GltfError> {
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(index)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView {index} does not exist")))?;
    let offset = json_byte_field(
        view.get("byteOffset"),
        &format!("{what} bufferView {index} byteOffset"),
    )?
    .unwrap_or(0);
    let length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView has no byteLength")))?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView length overflows")))?;
    if end > binary.len() {
        return Err(GltfError::new(format!(
            "{what} bufferView extends past the binary chunk; the GLB is truncated"
        )));
    }
    binary
        .get(offset..end)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView is out of range")))
}

/// Copies the first `N` values out of a decoded component list.
///
/// [`read_vec`] always returns exactly the requested number of components, so
/// this only fails for a caller that asked for more components than the
/// accessor declares.
pub(super) fn components<const N: usize>(values: &[f32]) -> Result<[f32; N], GltfError> {
    values
        .get(..N)
        .and_then(|slice| <[f32; N]>::try_from(slice).ok())
        .ok_or_else(|| {
            GltfError::new("accessor declares fewer components than the attribute needs")
        })
}

/// Reads `N` bytes at `offset` as a fixed-size array.
///
/// Accessor bounds are validated before reading, but a truncated or malformed
/// asset must surface an error instead of a panic, so every read stays checked.
fn read_le_bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], GltfError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| GltfError::new("accessor byte range overflows"))?;
    let slice = data
        .get(offset..end)
        .ok_or_else(|| GltfError::new("accessor data is truncated"))?;
    <[u8; N]>::try_from(slice)
        .map_err(|error| GltfError::new(format!("accessor data is truncated: {error}")))
}

/// Little-endian `u32` at `offset`, or an error when the data is truncated.
pub(super) fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, GltfError> {
    read_le_bytes::<4>(data, offset).map(u32::from_le_bytes)
}

/// Big-endian `u32` at `offset`, or an error when the data is truncated.
pub(super) fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, GltfError> {
    read_le_bytes::<4>(data, offset).map(u32::from_be_bytes)
}

/// Little-endian `u16` at `offset`, or an error when the data is truncated.
fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, GltfError> {
    read_le_bytes::<2>(data, offset).map(u16::from_le_bytes)
}

/// Little-endian `f32` at `offset`, or an error when the data is truncated.
fn read_f32_le(data: &[u8], offset: usize) -> Result<f32, GltfError> {
    read_le_bytes::<4>(data, offset).map(f32::from_le_bytes)
}

pub(super) struct AccessorView<'a> {
    /// The accessor's bufferView: exactly `binary[view_offset..view_end]`.
    data: &'a [u8],
    /// Byte offset of the first element within `data`: the accessor's own
    /// `byteOffset` (`data` already starts at the view's `byteOffset`).
    offset: usize,
    stride: usize,
    element_size: usize,
    pub(super) count: usize,
    pub(super) component_type: u32,
    normalized: bool,
    components: usize,
}

/// `usize` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in this target's pointer width.
pub(super) fn json_usize(value: &serde_json::Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}

/// `u32` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in 32 bits.
pub(super) fn json_u32(value: &serde_json::Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}

pub(super) fn accessor_view<'a>(
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
    if accessor.get("sparse").is_some() {
        return Err(GltfError::new(format!(
            "accessor {index} is sparse; sparse accessors are not supported"
        )));
    }

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
    let element_size = component_size
        .checked_mul(components)
        .ok_or_else(|| GltfError::new("accessor element size overflows"))?;
    let data = accessor_data(json, binary, accessor, index, element_size, count)?;

    Ok(AccessorView {
        data: data.bytes,
        offset: data.offset,
        stride: data.stride,
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
        Some("MAT4") => 16,
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

/// One bufferView's slice, plus the addressing an accessor applies inside it.
struct AccessorData<'a> {
    /// The whole bufferView: `binary[view_offset..view_offset + view_length]`.
    bytes: &'a [u8],
    /// The accessor's `byteOffset` within `bytes`.
    offset: usize,
    /// The element stride in bytes (`byteStride`, or the element size).
    stride: usize,
}

/// A JSON byte offset or stride that must be a non-negative integer when
/// present.
///
/// glTF byte offsets and strides are unsigned integers. A present negative,
/// fractional or out-of-range value is malformed and is rejected instead of
/// being silently treated as absent (which would decode the wrong bytes).
fn json_byte_field(
    value: Option<&serde_json::Value>,
    what: &str,
) -> Result<Option<usize>, GltfError> {
    value.map_or(Ok(None), |json_value| {
        json_usize(json_value).map(Some).ok_or_else(|| {
            GltfError::new(format!(
                "{what} must be a non-negative integer, got {json_value}"
            ))
        })
    })
}

/// The binary slice an accessor addresses, plus its element stride and offset.
fn accessor_data<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    accessor: &serde_json::Value,
    index: usize,
    element_size: usize,
    count: usize,
) -> Result<AccessorData<'a>, GltfError> {
    if count == 0 {
        return Err(GltfError::new(format!(
            "accessor {index} declares zero elements; an accessor must hold at least one"
        )));
    }
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

    // A bufferView's `byteLength` is relative to its own `byteOffset`, so the
    // view is exactly `binary[view_offset..view_end]`. Checking that view end
    // (not the accessor's end) is what rejects a crossing accessor even when
    // the binary chunk continues past the declared view.
    let view_offset = json_byte_field(
        view.get("byteOffset"),
        &format!("bufferView {view_index} byteOffset"),
    )?
    .unwrap_or(0);
    let view_length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("bufferView has no byteLength"))?;
    let view_end = view_offset
        .checked_add(view_length)
        .ok_or_else(|| GltfError::new("bufferView length overflows"))?;
    if view_end > binary.len() {
        return Err(GltfError::new(
            "bufferView extends past the end of the binary chunk; the GLB is truncated",
        ));
    }

    let accessor_offset = json_byte_field(
        accessor.get("byteOffset"),
        &format!("accessor {index} byteOffset"),
    )?
    .unwrap_or(0);
    let declared_stride = json_byte_field(
        view.get("byteStride"),
        &format!("bufferView {view_index} byteStride"),
    )?;
    let stride = declared_stride.unwrap_or(element_size);
    if count > 1
        && let Some(declared) = declared_stride
    {
        // A zero stride would read the same bytes for every element; glTF
        // requires at least the element size (and a multiple of 4) when
        // `byteStride` is authored at all.
        if declared == 0 {
            return Err(GltfError::new(format!(
                "accessor {index} declares a zero byteStride with {count} elements"
            )));
        }
        if declared < element_size {
            return Err(GltfError::new(format!(
                "accessor {index} declares byteStride {declared} smaller than its \
                 {element_size}-byte elements"
            )));
        }
        if !declared.is_multiple_of(4) {
            return Err(GltfError::new(format!(
                "accessor {index} declares byteStride {declared}; glTF requires a multiple of 4"
            )));
        }
    }
    // The accessor's `byteOffset` is relative to the view: every element it
    // addresses has to fit inside the declared view. The arithmetic is
    // checked so malformed metadata cannot wrap into an in-bounds read.
    let relative_end = count
        .checked_sub(1)
        .and_then(|last| last.checked_mul(stride))
        .and_then(|span| accessor_offset.checked_add(span))
        .and_then(|end| end.checked_add(element_size))
        .ok_or_else(|| GltfError::new("accessor byte length overflows"))?;
    if relative_end > view_length {
        return Err(GltfError::new(format!(
            "accessor {index} declares {count} elements but its bufferView is too small"
        )));
    }
    Ok(AccessorData {
        bytes: binary
            .get(view_offset..view_end)
            .ok_or_else(|| GltfError::new("accessor data is truncated"))?,
        offset: accessor_offset,
        stride,
    })
}

pub(super) fn read_vec(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
    components: usize,
) -> Result<Vec<Vec<f32>>, GltfError> {
    let view = accessor_view(json, binary, index, components)?;
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .and_then(|skip| skip.checked_add(view.offset))
            .ok_or_else(|| GltfError::new("accessor element offset overflows"))?;
        let mut values = Vec::with_capacity(view.components);
        for component in 0..view.components {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("accessor component offset overflows"))?;
            let value = match view.component_type {
                COMPONENT_FLOAT => read_f32_le(view.data, offset)?,
                COMPONENT_UBYTE => {
                    let raw = view
                        .data
                        .get(offset)
                        .copied()
                        .ok_or_else(|| GltfError::new("accessor data is truncated"))?;
                    if view.normalized {
                        f32::from(raw) / 255.0
                    } else {
                        f32::from(raw)
                    }
                }
                COMPONENT_USHORT => {
                    let raw = read_u16_le(view.data, offset)?;
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

pub(super) fn read_indices(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<u32>, GltfError> {
    let view = accessor_view(json, binary, index, 1)?;
    let component_size = view.element_size;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let offset = element
            .checked_mul(view.stride)
            .and_then(|skip| skip.checked_add(view.offset))
            .ok_or_else(|| GltfError::new("accessor element offset overflows"))?;
        out.push(match view.component_type {
            COMPONENT_UBYTE => u32::from(
                view.data
                    .get(offset)
                    .copied()
                    .ok_or_else(|| GltfError::new("accessor data is truncated"))?,
            ),
            COMPONENT_USHORT => u32::from(read_u16_le(view.data, offset)?),
            COMPONENT_UINT => read_u32_le(view.data, offset)?,
            other => {
                return Err(GltfError::new(format!(
                    "componentType {other} cannot be used for indices (size {component_size})"
                )));
            }
        });
    }
    Ok(out)
}

/// Reads `JOINTS_0`: four unsigned byte or unsigned short joint slots per
/// vertex, never normalised.
pub(super) fn read_joints(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<[u16; 4]>, GltfError> {
    let view = accessor_view(json, binary, index, 4)?;
    if view.normalized {
        return Err(GltfError::new(
            "JOINTS_0 must not be normalised; joint indices are plain integers",
        ));
    }
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("JOINTS_0 accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .and_then(|skip| skip.checked_add(view.offset))
            .ok_or_else(|| GltfError::new("JOINTS_0 element offset overflows"))?;
        let mut values = [0u16; 4];
        for (component, slot) in values.iter_mut().enumerate() {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("JOINTS_0 component offset overflows"))?;
            *slot = match view.component_type {
                COMPONENT_UBYTE => u16::from(
                    view.data
                        .get(offset)
                        .copied()
                        .ok_or_else(|| GltfError::new("JOINTS_0 data is truncated"))?,
                ),
                COMPONENT_USHORT => read_u16_le(view.data, offset)?,
                other => {
                    return Err(GltfError::new(format!(
                        "JOINTS_0 must use unsigned byte or unsigned short components, \
                         not componentType {other}"
                    )));
                }
            };
        }
        out.push(values);
    }
    Ok(out)
}

/// Reads `WEIGHTS_0`: four float32 or normalised integer joint weights per
/// vertex.
pub(super) fn read_weights(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<[f32; 4]>, GltfError> {
    let view = accessor_view(json, binary, index, 4)?;
    let integer = matches!(view.component_type, COMPONENT_UBYTE | COMPONENT_USHORT);
    if integer && !view.normalized {
        return Err(GltfError::new(
            "WEIGHTS_0 integer components must be normalised; \
             use float32 or a normalised integer accessor",
        ));
    }
    if !integer && view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new(format!(
            "WEIGHTS_0 must use float32 or normalised integer components, \
             not componentType {}",
            view.component_type
        )));
    }
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("WEIGHTS_0 accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .and_then(|skip| skip.checked_add(view.offset))
            .ok_or_else(|| GltfError::new("WEIGHTS_0 element offset overflows"))?;
        let mut values = [0.0f32; 4];
        for (component, slot) in values.iter_mut().enumerate() {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("WEIGHTS_0 component offset overflows"))?;
            *slot = match view.component_type {
                COMPONENT_FLOAT => read_f32_le(view.data, offset)?,
                COMPONENT_UBYTE => {
                    f32::from(
                        view.data
                            .get(offset)
                            .copied()
                            .ok_or_else(|| GltfError::new("WEIGHTS_0 data is truncated"))?,
                    ) / 255.0
                }
                COMPONENT_USHORT => f32::from(read_u16_le(view.data, offset)?) / 65_535.0,
                other => {
                    return Err(GltfError::new(format!(
                        "WEIGHTS_0 componentType {other} is not supported"
                    )));
                }
            };
        }
        out.push(values);
    }
    Ok(out)
}

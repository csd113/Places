//! KTX 2.0 container writer/reader for the prepared lighting payloads.
//!
//! The prepared payloads are deliberately the portable reference subset of
//! KTX 2.0 that every conformant loader supports without optional features:
//!
//! * uncompressed `VK_FORMAT_R8G8B8A8_UNORM` atlas and probe texels, and
//!   uncompressed `VK_FORMAT_R16G16B16A16_SFLOAT` halves for prefiltered
//!   payloads;
//! * no supercompression, no vendor schemes, no Basis transcoding;
//! * one mip level for the atlas (the lightmap payload has no mip chain) and a
//!   caller-supplied chain for a prefiltered payload;
//! * 2D array pages (`layerCount >= 1`, `faceCount == 1`) or one cube map per
//!   probe payload (`layerCount == 0`, `faceCount == 6`);
//! * the default `rd` orientation (row-major from the top-left), which is what
//!   the bake already produces, with no key/value metadata.
//!
//! The reader accepts exactly that subset and rejects everything else by name:
//! a supercompressed, block-compressed or metadata-carrying payload is not
//! something this runtime contract promises to read. Rejections happen before
//! any large allocation: dimensions, level counts, declared lengths and offsets
//! are bounded and cross-checked against the actual file size.
//!
//! Layout (all integers little-endian), following the KTX 2.0 specification:
//!
//! ```text
//! identifier    12 bytes  AB 4B 54 58 20 32 30 BB 0D 0A 1A 0A
//! vkFormat      u32       R8G8B8A8_UNORM (37) or R16G16B16A16_SFLOAT (97)
//! typeSize      u32       1, or 2 for half-float texels
//! pixelWidth    u32
//! pixelHeight   u32
//! pixelDepth    u32       0
//! layerCount    u32       0 or N
//! faceCount     u32       1 or 6
//! levelCount    u32
//! supercompressionScheme u32 0
//! dfdByteOffset u32
//! dfdByteLength u32
//! kvdByteOffset u32       0
//! kvdByteLength u32       0
//! sgdByteOffset u64       0
//! sgdByteLength u64       0
//! levels[levelCount]      3 x u64 (byteOffset, byteLength, uncompressedByteLength)
//! dfd           dfdByteLength bytes, 4-byte aligned start
//! level images  base level first, 4-byte aligned starts
//! ```
//!
//! The DFD is the standard four-sample RGBSDA descriptor. For RGBA8 UNORM it
//! carries `dfdTotalSize` 92, sample bit lengths 7 = 8 bits, channel types
//! RED/GREEN/BLUE/ALPHA at bit offsets 0/8/16/24, `bytesPlane[0] = 4`, and
//! `transferFunction = LINEAR` because an sRGB Vulkan variant exists. For
//! RGBA16F the same descriptor carries 16-bit sample encodings: bit offsets
//! 0/16/32/48, `bitLength = 15`, `bytesPlane[0] = 8` and `sampleUpper =
//! 0x3F800000`.

/// The 12-byte KTX 2 identifier.
pub const KTX2_IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

/// `VK_FORMAT_R8G8B8A8_UNORM`: the primary texture format this contract
/// transports.
pub const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;

/// `VK_FORMAT_R16G16B16A16_SFLOAT`: half-float RGBA texels, 8 bytes each.
pub const VK_FORMAT_R16G16B16A16_SFLOAT: u32 = 97;

/// Total size of the data format descriptor this writer emits.
const DFD_TOTAL_SIZE: u32 = 92;

/// Identifier and fixed KTX2 header fields before the per-level index.
const FIXED_HEADER_BYTES: u64 = 80;
const LEVEL_INDEX_BYTES: u64 = 24;

/// The writer's complete header, one level index and aligned descriptor.
/// Atlas planning reserves these bytes before budgeting image data.
pub(crate) fn single_level_header_bytes() -> u64 {
    align4(
        FIXED_HEADER_BYTES
            .saturating_add(LEVEL_INDEX_BYTES)
            .saturating_add(u64::from(DFD_TOTAL_SIZE)),
    )
}

/// [`DFD_TOTAL_SIZE`] as an array length.
#[expect(
    clippy::as_conversions,
    reason = "The fixed descriptor size is 92 and fits every supported usize; TryFrom is unavailable in a Rust 1.99 const initializer."
)]
const DFD_BYTES: usize = DFD_TOTAL_SIZE as usize;

/// Largest accepted image edge, in texels, before allocation.
pub const MAX_EDGE: u32 = 8192;

/// Largest accepted total level data, in bytes.
pub const MAX_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;

/// Largest accepted level count in one payload.
pub const MAX_LEVELS: u32 = 16;

/// Largest accepted layer count in one payload.
pub const MAX_LAYERS: u32 = 1024;

/// A decoded RGBA8 KTX2 payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ktx2Rgba8 {
    /// Base level edge length in texels.
    pub edge: u32,
    /// Layer count (`0` for a cube map).
    pub layers: u32,
    /// Face count (`6` for a cube map, else `1`).
    pub faces: u32,
    /// Mip levels, base level first; every level holds `max(layers, 1) * faces`
    /// tightly packed images of `edge >> level` texels in RGBA8 order.
    pub levels: Vec<Vec<u8>>,
}

impl Ktx2Rgba8 {
    /// Number of images in the base level.
    #[must_use]
    pub fn image_count(&self) -> u32 {
        self.layers.max(1).saturating_mul(self.faces.max(1))
    }

    /// Bytes one image of `edge` texels occupies.
    #[must_use]
    pub fn image_bytes(edge: u32) -> u64 {
        TexelFormat::RGBA8.image_bytes(edge)
    }
}

/// A decoded RGBA16F KTX2 payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ktx2Rgba16f {
    /// Base level edge length in texels.
    pub edge: u32,
    /// Layer count (`0` for a cube map).
    pub layers: u32,
    /// Face count (`6` for a cube map, else `1`).
    pub faces: u32,
    /// Mip levels, base level first; every level holds `max(layers, 1) * faces`
    /// tightly packed images of `edge >> level` texels as little-endian f16
    /// RGBA texels (8 bytes each).
    pub levels: Vec<Vec<u8>>,
}

impl Ktx2Rgba16f {
    /// Number of images in the base level.
    #[must_use]
    pub fn image_count(&self) -> u32 {
        self.layers.max(1).saturating_mul(self.faces.max(1))
    }

    /// Bytes one image of `edge` texels occupies.
    #[must_use]
    pub fn image_bytes(edge: u32) -> u64 {
        TexelFormat::RGBA16F.image_bytes(edge)
    }
}

/// Converts an `f32` to IEEE 754 binary16 bits with round-to-nearest-even.
///
/// Infinities stay infinite, NaN becomes a quiet NaN carrying the high
/// mantissa bits as its payload, magnitudes above the binary16 range overflow
/// to infinity, and magnitudes below half the smallest subnormal round to
/// signed zero. Implemented with integer bit manipulation only.
#[must_use]
pub fn f32_to_f16_bits(value: f32) -> u16 {
    const SIGN: u32 = 0x8000_0000;
    const EXPONENT: u32 = 0x7F80_0000;
    const MANTISSA: u32 = 0x007F_FFFF;

    let bits = value.to_bits();
    let sign = bits & SIGN;
    let exponent = bits & EXPONENT;
    let mantissa = bits & MANTISSA;

    if exponent == EXPONENT {
        // Infinity maps to infinity; NaN becomes a quiet NaN carrying the
        // high binary32 mantissa bits.
        let payload = if mantissa == 0 {
            0
        } else {
            (mantissa >> 13_i32) | 0x0200
        };
        return u16::try_from((sign >> 16) | 0x7C00 | payload).unwrap_or(u16::MAX);
    }

    // Rebias binary32's exponent (bias 127) to binary16's (bias 15): the
    // result is the binary16 exponent, negative for subnormal or zero results.
    let target = i64::from(exponent >> 23_i32).saturating_sub(112);
    if target >= 0x1F {
        return u16::try_from((sign >> 16) | 0x7C00).unwrap_or(u16::MAX);
    }
    if target <= 0 {
        if target < -10 {
            // At or below half the smallest subnormal: signed zero.
            return u16::try_from(sign >> 16).unwrap_or(0);
        }
        // Subnormal result: shift the implicit-leading-bit significand down
        // and round to nearest even.
        let shift = u32::try_from(14_i64.saturating_sub(target)).unwrap_or(24);
        let significand = mantissa | 0x0080_0000;
        let half = 1_u32 << shift.saturating_sub(1);
        let mut rounded = significand >> shift;
        let remainder = significand & (1_u32 << shift).saturating_sub(1);
        if remainder > half || (remainder == half && rounded & 1 == 1) {
            rounded = rounded.wrapping_add(1);
        }
        return u16::try_from((sign >> 16) | rounded).unwrap_or(u16::MAX);
    }

    // Normal result: keep ten mantissa bits, rounding to nearest even. A
    // carry out of the mantissa increments the exponent, so the largest
    // finite value rounds up to infinity exactly as IEEE 754 requires.
    let mut rounded = (u32::try_from(target).unwrap_or(0) << 10_i32) | (mantissa >> 13_i32);
    let remainder = mantissa & 0x1FFF;
    if remainder > 0x1000 || (remainder == 0x1000 && rounded & 1 == 1) {
        rounded = rounded.wrapping_add(1);
    }
    u16::try_from((sign >> 16) | rounded).unwrap_or(u16::MAX)
}

/// Converts IEEE 754 binary16 bits to the nearest `f32`.
///
/// Every binary16 value is exactly representable in binary32, so the
/// conversion is exact: zero signs, subnormals, infinities and NaN payloads
/// are all preserved. Implemented with integer bit manipulation only.
#[must_use]
pub fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16_i32;
    let exponent = u32::from((bits >> 10_i32) & 0x1F);
    let mantissa = u32::from(bits & 0x03FF);

    let value = if exponent == 0 {
        if mantissa == 0 {
            sign // Signed zero.
        } else {
            // Subnormal: normalize the leading mantissa bit into an implicit
            // binary32 significand.
            let raw = bits & 0x03FF;
            let shift = raw.leading_zeros().saturating_sub(5);
            let normalized = (u32::from(raw) << shift) & 0x03FF;
            let normalized_exponent = 113_u32.saturating_sub(shift);
            sign | (normalized_exponent << 23_i32) | (normalized << 13_i32)
        }
    } else if exponent == 0x1F {
        sign | 0x7F80_0000 | (mantissa << 13_i32) // Infinity or NaN payload.
    } else {
        // Rebias the exponent from binary16's 15 to binary32's 127.
        sign | (exponent.saturating_add(112) << 23_i32) | (mantissa << 13_i32)
    };
    f32::from_bits(value)
}

/// Encodes a 2D array of equally sized RGBA8 images as one KTX2 file.
///
/// # Errors
///
/// Returns an error for a zero or oversized edge, an empty or oversized layer
/// list, an image whose byte length does not equal `edge * edge * 4`, or a
/// payload beyond [`MAX_PAYLOAD_BYTES`].
pub fn write_rgba8_2d_array(edge: u32, layers: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if edge == 0 || edge > MAX_EDGE {
        return Err(format!("KTX2 edge {edge} is out of range"));
    }
    if layers.is_empty() {
        return Err("KTX2 payload has no layers".to_string());
    }
    let layer_count = u32::try_from(layers.len())
        .map_err(|error| format!("KTX2 payload has too many layers: {error}"))?;
    if layer_count > MAX_LAYERS {
        return Err(format!(
            "KTX2 payload has {layer_count} layers (limit {MAX_LAYERS})"
        ));
    }
    encode(edge, layer_count, 1, layers)
}

/// Encodes one cube map of six equally sized RGBA8 faces as one KTX2 file.
///
/// # Errors
///
/// Returns an error for a zero or oversized edge, an unpaired edge, a face list
/// that is not exactly six images, a face whose byte length does not equal
/// `edge * edge * 4`, or a payload beyond [`MAX_PAYLOAD_BYTES`].
pub fn write_rgba8_cube(edge: u32, faces: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if edge == 0 || edge > MAX_EDGE {
        return Err(format!("KTX2 edge {edge} is out of range"));
    }
    if faces.len() != 6 {
        return Err(format!("KTX2 cube has {} faces (expected 6)", faces.len()));
    }
    if !edge.is_multiple_of(2) {
        return Err("KTX2 cube edge must be even".to_string());
    }
    encode(edge, 0, 6, faces)
}

/// Encodes one cube map with a caller-supplied mip chain as one KTX2 file.
///
/// `levels` is ordered base level first: level `L` holds exactly six faces of
/// `edge >> L` texels, each `(edge >> L) * (edge >> L) * 4` bytes of RGBA8 with
/// no padding. The chain must not run past one texel, the base edge must be
/// even, and the whole payload must fit [`MAX_PAYLOAD_BYTES`].
///
/// # Errors
///
/// Returns an error for a zero or oversized base edge, an odd base edge, an
/// empty or oversized chain, a level whose faces do not match its dimensions,
/// or a payload beyond [`MAX_PAYLOAD_BYTES`].
pub fn write_rgba8_cube_with_mips(edge: u32, levels: &[[Vec<u8>; 6]]) -> Result<Vec<u8>, String> {
    if edge == 0 || edge > MAX_EDGE {
        return Err(format!("KTX2 edge {edge} is out of range"));
    }
    if !edge.is_multiple_of(2) {
        return Err("KTX2 cube edge must be even".to_string());
    }
    if levels.is_empty() {
        return Err("KTX2 mip chain has no levels".to_string());
    }
    let level_slices: Vec<&[Vec<u8>]> = levels.iter().map(<[Vec<u8>; 6]>::as_slice).collect();
    encode_levels(&TexelFormat::RGBA8, edge, 0, 6, &level_slices)
}

/// Encodes a linear HDR reflection cube, including its roughness mip chain.
///
/// # Errors
/// Returns an error for invalid dimensions, face lengths or payload bounds.
pub fn write_rgba16f_cube_with_mips(edge: u32, levels: &[[Vec<u8>; 6]]) -> Result<Vec<u8>, String> {
    if edge == 0 || edge > MAX_EDGE || !edge.is_multiple_of(2) {
        return Err(format!(
            "KTX2 cube edge {edge} must be positive, even and at most {MAX_EDGE}"
        ));
    }
    if levels.is_empty() {
        return Err("KTX2 mip chain has no levels".to_string());
    }
    let slices: Vec<&[Vec<u8>]> = levels.iter().map(<[Vec<u8>; 6]>::as_slice).collect();
    encode_levels(&TexelFormat::RGBA16F, edge, 0, 6, &slices)
}

/// Encodes a 2D array of equally sized RGBA16F images as one KTX2 file.
///
/// Every layer is one tightly packed `edge` x `edge` image of little-endian
/// half-float RGBA texels (four `f16` components, 8 bytes per texel). The
/// payload carries a single mip level.
///
/// # Errors
///
/// Returns an error for a zero or oversized edge, an empty or oversized layer
/// list, an image whose byte length does not equal `edge * edge * 8`, or a
/// payload beyond [`MAX_PAYLOAD_BYTES`].
pub fn write_rgba16f_2d_array(edge: u32, layers: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if edge == 0 || edge > MAX_EDGE {
        return Err(format!("KTX2 edge {edge} is out of range"));
    }
    if layers.is_empty() {
        return Err("KTX2 payload has no layers".to_string());
    }
    let layer_count = u32::try_from(layers.len())
        .map_err(|error| format!("KTX2 payload has too many layers: {error}"))?;
    if layer_count > MAX_LAYERS {
        return Err(format!(
            "KTX2 payload has {layer_count} layers (limit {MAX_LAYERS})"
        ));
    }
    encode_levels(&TexelFormat::RGBA16F, edge, layer_count, 1, &[layers])
}

/// One texel format in this module's writer/reader contract.
struct TexelFormat {
    /// `vkFormat` header value.
    vk_format: u32,
    /// `typeSize` header value: bytes per component.
    type_size: u32,
    /// Bytes per RGBA texel.
    bytes_per_texel: u64,
    /// Vulkan format name used in error messages.
    name: &'static str,
    /// Descriptor description used in error messages.
    dfd_name: &'static str,
    /// The exact DFD this writer emits and its reader accepts.
    dfd: fn() -> [u8; DFD_BYTES],
}

impl TexelFormat {
    /// RGBA8 UNORM: one byte per component, four per texel.
    const RGBA8: Self = Self {
        vk_format: VK_FORMAT_R8G8B8A8_UNORM,
        type_size: 1,
        bytes_per_texel: 4,
        name: "R8G8B8A8_UNORM",
        dfd_name: "RGBA8 UNORM",
        dfd: rgba8_dfd,
    };

    /// RGBA16F: a little-endian binary16 per component, eight per texel.
    const RGBA16F: Self = Self {
        vk_format: VK_FORMAT_R16G16B16A16_SFLOAT,
        type_size: 2,
        bytes_per_texel: 8,
        name: "R16G16B16A16_SFLOAT",
        dfd_name: "RGBA16F",
        dfd: rgba16f_dfd,
    };

    /// Bytes one `edge` x `edge` image occupies.
    fn image_bytes(&self, edge: u32) -> u64 {
        u64::from(edge)
            .saturating_mul(u64::from(edge))
            .saturating_mul(self.bytes_per_texel)
    }
}

/// Encodes a single-level payload, the shape the atlas and probe writers use.
fn encode(edge: u32, layers: u32, faces: u32, images: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    encode_levels(&TexelFormat::RGBA8, edge, layers, faces, &[images])
}

#[expect(
    clippy::too_many_lines,
    reason = "one cohesive container writer: header, DFD and level data"
)] // one cohesive container writer: header, DFD and level data
fn encode_levels(
    format: &TexelFormat,
    edge: u32,
    layers: u32,
    faces: u32,
    level_images: &[&[Vec<u8>]],
) -> Result<Vec<u8>, String> {
    if level_images.is_empty() {
        return Err("KTX2 mip chain has no levels".to_string());
    }
    let level_count = u32::try_from(level_images.len())
        .map_err(|error| format!("KTX2 mip chain has too many levels: {error}"))?;
    if level_count > MAX_LEVELS {
        return Err(format!(
            "KTX2 mip chain has {level_count} levels (limit {MAX_LEVELS})"
        ));
    }

    let mut level_bytes = Vec::with_capacity(level_images.len());
    let mut payload: u64 = 0;
    for (level, images) in level_images.iter().enumerate() {
        let level_index = u32::try_from(level)
            .map_err(|error| format!("KTX2 level index is too large: {error}"))?;
        let image_edge = edge >> level_index;
        if image_edge == 0 {
            return Err(format!("KTX2 mip level {level} is smaller than one texel"));
        }
        let expected = format.image_bytes(image_edge);
        for (index, image) in images.iter().enumerate() {
            if u64::try_from(image.len()).unwrap_or(u64::MAX) != expected {
                return Err(format!(
                    "KTX2 level {level} image {index} holds {} bytes, expected {expected}",
                    image.len()
                ));
            }
        }
        let bytes = expected
            .checked_mul(u64::try_from(images.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| "KTX2 level size overflows".to_string())?;
        payload = payload
            .checked_add(bytes)
            .ok_or_else(|| "KTX2 payload size overflows".to_string())?;
        level_bytes.push(bytes);
    }
    if payload > MAX_PAYLOAD_BYTES {
        return Err(format!(
            "KTX2 payload is {payload} bytes (limit {MAX_PAYLOAD_BYTES})"
        ));
    }

    let level_index_size = u64::from(level_count)
        .checked_mul(LEVEL_INDEX_BYTES)
        .ok_or_else(|| "KTX2 header size overflows".to_string())?;
    let dfd_offset = FIXED_HEADER_BYTES
        .checked_add(level_index_size)
        .ok_or_else(|| "KTX2 header size overflows".to_string())?;
    let data_offset = align4(
        dfd_offset
            .checked_add(u64::from(DFD_TOTAL_SIZE))
            .ok_or_else(|| "KTX2 DFD offset overflows".to_string())?,
    );
    let mut level_offsets = Vec::with_capacity(level_images.len());
    let mut cursor = data_offset;
    for bytes in &level_bytes {
        let start = align4(cursor);
        level_offsets.push(start);
        cursor = start
            .checked_add(*bytes)
            .ok_or_else(|| "KTX2 file size overflows".to_string())?;
    }
    let capacity =
        usize::try_from(cursor).map_err(|error| format!("KTX2 file is too large: {error}"))?;

    let mut out: Vec<u8> = Vec::with_capacity(capacity);
    out.extend_from_slice(&KTX2_IDENTIFIER);
    push_u32(&mut out, format.vk_format);
    push_u32(&mut out, format.type_size);
    push_u32(&mut out, edge);
    push_u32(&mut out, edge);
    push_u32(&mut out, 0); // pixelDepth
    push_u32(&mut out, layers);
    push_u32(&mut out, faces);
    push_u32(&mut out, level_count);
    push_u32(&mut out, 0); // supercompressionScheme
    let dfd_offset_u32 =
        u32::try_from(dfd_offset).map_err(|error| format!("KTX2 DFD offset overflows: {error}"))?;
    push_u32(&mut out, dfd_offset_u32);
    push_u32(&mut out, DFD_TOTAL_SIZE);
    push_u32(&mut out, 0); // kvdByteOffset
    push_u32(&mut out, 0); // kvdByteLength
    push_u64(&mut out, 0); // sgdByteOffset
    push_u64(&mut out, 0); // sgdByteLength
    // Level index, base level first.
    for (offset, bytes) in level_offsets.iter().zip(&level_bytes) {
        push_u64(&mut out, *offset);
        push_u64(&mut out, *bytes);
        push_u64(&mut out, *bytes);
    }
    debug_assert_eq!(
        u64::try_from(out.len()).unwrap_or(u64::MAX),
        dfd_offset,
        "KTX2 header and level index must end at the declared descriptor offset"
    );
    out.extend_from_slice(&(format.dfd)());
    while u64::try_from(out.len()).unwrap_or(u64::MAX) < data_offset {
        out.push(0);
    }
    for (offset, images) in level_offsets.iter().zip(level_images) {
        while u64::try_from(out.len()).unwrap_or(u64::MAX) < *offset {
            out.push(0);
        }
        for image in *images {
            out.extend_from_slice(image);
        }
    }
    debug_assert_eq!(
        out.len(),
        capacity,
        "KTX2 payload must match its checked allocation size"
    );
    Ok(out)
}

/// The format-independent fields of a decoded payload.
struct Payload {
    /// Base level edge length in texels.
    edge: u32,
    /// Layer count (`0` for a cube map).
    layers: u32,
    /// Face count (`6` for a cube map, else `1`).
    faces: u32,
    /// Mip levels, base level first.
    levels: Vec<Vec<u8>>,
}

/// Decodes a KTX2 file written by this module's RGBA8 writer.
///
/// # Errors
///
/// Returns an error for a wrong identifier, a format outside the supported
/// subset (compressed, supercompressed, sRGB, non-RGBA8, both layers and
/// faces, or a malformed descriptor), dimensions or counts out of range, a
/// declared level length that does not match the dimensions, truncation or
/// trailing data.
pub fn read_rgba8(bytes: &[u8]) -> Result<Ktx2Rgba8, String> {
    let payload = read_payload(bytes, &TexelFormat::RGBA8)?;
    Ok(Ktx2Rgba8 {
        edge: payload.edge,
        layers: payload.layers,
        faces: payload.faces,
        levels: payload.levels,
    })
}

/// Decodes a KTX2 file written by this module's RGBA16F writer.
///
/// # Errors
///
/// Returns an error for a wrong identifier, a format outside the supported
/// subset (compressed, supercompressed, sRGB, non-RGBA16F, both layers and
/// faces, or a malformed descriptor), dimensions or counts out of range, a
/// declared level length that does not match the dimensions, truncation or
/// trailing data.
pub fn read_rgba16f(bytes: &[u8]) -> Result<Ktx2Rgba16f, String> {
    let payload = read_payload(bytes, &TexelFormat::RGBA16F)?;
    Ok(Ktx2Rgba16f {
        edge: payload.edge,
        layers: payload.layers,
        faces: payload.faces,
        levels: payload.levels,
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "one cohesive strict-subset container reader"
)] // one cohesive strict-subset container reader
fn read_payload(bytes: &[u8], format: &TexelFormat) -> Result<Payload, String> {
    let reader = Slice::new(bytes);
    if reader.take(0, 12)? != &KTX2_IDENTIFIER[..] {
        return Err("file is not a KTX2 container".to_string());
    }
    let vk_format = reader.u32_at(12)?;
    if vk_format != format.vk_format {
        return Err(format!(
            "KTX2 vkFormat {vk_format} is not {} ({})",
            format.name, format.vk_format
        ));
    }
    let type_size = reader.u32_at(16)?;
    if type_size != format.type_size {
        return Err(format!(
            "KTX2 typeSize {type_size} is not {}",
            format.type_size
        ));
    }
    let width = reader.u32_at(20)?;
    let height = reader.u32_at(24)?;
    let depth = reader.u32_at(28)?;
    let layers = reader.u32_at(32)?;
    let faces = reader.u32_at(36)?;
    let levels = reader.u32_at(40)?;
    let scheme = reader.u32_at(44)?;
    if width == 0 || height == 0 || width != height || width > MAX_EDGE {
        return Err(format!("KTX2 dimensions {width}x{height} are invalid"));
    }
    if depth != 0 {
        return Err("KTX2 payload has a depth dimension".to_string());
    }
    if scheme != 0 {
        return Err(format!(
            "KTX2 supercompression scheme {scheme} is not supported"
        ));
    }
    if !matches!(faces, 1 | 6) {
        return Err(format!("KTX2 faceCount {faces} is not 1 or 6"));
    }
    if faces == 6 && layers != 0 {
        return Err("KTX2 payload is both an array and a cube".to_string());
    }
    if layers > MAX_LAYERS {
        return Err(format!("KTX2 layer count {layers} exceeds {MAX_LAYERS}"));
    }
    if levels == 0 || levels > MAX_LEVELS {
        return Err(format!("KTX2 levelCount {levels} is out of range"));
    }
    let dfd_offset = u64::from(reader.u32_at(48)?);
    let dfd_length = u64::from(reader.u32_at(52)?);
    let kvd_offset = u64::from(reader.u32_at(56)?);
    let kvd_length = u64::from(reader.u32_at(60)?);
    let sgd_offset = reader.u64_at(64)?;
    let sgd_length = reader.u64_at(72)?;
    let file_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if kvd_length != 0 || kvd_offset != 0 {
        return Err("KTX2 key/value metadata is not part of this payload contract".to_string());
    }
    if sgd_length != 0 || sgd_offset != 0 {
        return Err("KTX2 supercompression global data is not supported".to_string());
    }
    if dfd_length != u64::from(DFD_TOTAL_SIZE) {
        return Err(format!(
            "KTX2 DFD length {dfd_length} is not {DFD_TOTAL_SIZE}"
        ));
    }
    let expected_dfd = 80_u64.saturating_add(u64::from(levels).saturating_mul(24));
    if dfd_offset != expected_dfd {
        return Err("KTX2 DFD offset is not where this layout places it".to_string());
    }
    let dfd_end = dfd_offset
        .checked_add(dfd_length)
        .ok_or_else(|| "KTX2 DFD end overflows".to_string())?;
    if dfd_end > file_len {
        return Err("KTX2 DFD runs past the file".to_string());
    }
    let dfd_start = usize::try_from(dfd_offset)
        .map_err(|error| format!("KTX2 DFD offset is too large: {error}"))?;
    let dfd_end_usize = usize::try_from(dfd_end)
        .map_err(|error| format!("KTX2 DFD offset is too large: {error}"))?;
    let dfd = bytes
        .get(dfd_start..dfd_end_usize)
        .ok_or_else(|| "KTX2 DFD runs past the file".to_string())?;
    validate_dfd(dfd, format)?;

    // Level index entries, base level first.
    let mut level_offsets = Vec::with_capacity(usize::try_from(levels).unwrap_or(0));
    for index in 0..levels {
        let base = 80_u64.saturating_add(u64::from(index).saturating_mul(24));
        let offset = reader.u64_at(base)?;
        let length = reader.u64_at(base.saturating_add(8))?;
        let uncompressed = reader.u64_at(base.saturating_add(16))?;
        if offset % 4 != 0 {
            return Err(format!("KTX2 level {index} offset is not 4-byte aligned"));
        }
        if length != uncompressed {
            return Err(format!("KTX2 level {index} declares unequal lengths"));
        }
        level_offsets.push((offset, length));
    }
    let mut decoded = Vec::with_capacity(level_offsets.len());
    for (index, (offset, length)) in level_offsets.iter().enumerate() {
        let level_index = u32::try_from(index)
            .map_err(|error| format!("KTX2 level index is too large: {error}"))?;
        let image_edge = width >> level_index;
        if image_edge == 0 {
            return Err(format!("KTX2 level {index} is smaller than one texel"));
        }
        let expected = format
            .image_bytes(image_edge)
            .checked_mul(u64::from(layers.max(1)))
            .and_then(|value| value.checked_mul(u64::from(faces)))
            .ok_or_else(|| "KTX2 level size overflows".to_string())?;
        if *length != expected {
            return Err(format!(
                "KTX2 level {index} declares {length} bytes, expected {expected}"
            ));
        }
        if *length > MAX_PAYLOAD_BYTES {
            return Err(format!(
                "KTX2 level {index} exceeds {MAX_PAYLOAD_BYTES} bytes"
            ));
        }
        let start = usize::try_from(*offset)
            .map_err(|error| format!("KTX2 level offset is too large: {error}"))?;
        let end = start
            .checked_add(
                usize::try_from(*length)
                    .map_err(|error| format!("KTX2 level is too large: {error}"))?,
            )
            .ok_or_else(|| "KTX2 level range overflows".to_string())?;
        let data = bytes
            .get(start..end)
            .ok_or_else(|| format!("KTX2 level {index} runs past the file"))?;
        decoded.push(data.to_vec());
    }
    if decoded.is_empty() {
        return Err("KTX2 payload has no image data".to_string());
    }
    if decoded.iter().any(Vec::is_empty) {
        return Err("KTX2 payload has an empty level".to_string());
    }
    // The last level must end exactly at the end of the file: trailing bytes
    // are either a truncation marker or undeclared data.
    let last_end = level_offsets
        .last()
        .map_or(0, |(offset, length)| offset.saturating_add(*length));
    if last_end != file_len {
        return Err(format!(
            "KTX2 payload has {} trailing byte(s)",
            file_len.saturating_sub(last_end)
        ));
    }
    Ok(Payload {
        edge: width,
        layers,
        faces,
        levels: decoded,
    })
}

/// The standard four-sample RGBSDA descriptor for RGBA8 UNORM.
fn rgba8_dfd() -> [u8; DFD_BYTES] {
    let mut dfd = [0_u8; DFD_BYTES];
    let mut at = 0_usize;
    write_u32(&mut dfd, &mut at, DFD_TOTAL_SIZE);
    write_u32(&mut dfd, &mut at, 0); // vendorId 0, descriptorType 0
    write_u16(&mut dfd, &mut at, 2); // versionNumber
    write_u16(&mut dfd, &mut at, 88); // descriptorBlockSize (56 for 2 samples + 32 for 2 more)
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_MODEL_RGBSDA
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_PRIMARIES_BT709
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_TRANSFER_LINEAR
    write_u8(&mut dfd, &mut at, 0); // flags
    for _ in 0_i32..4_i32 {
        write_u8(&mut dfd, &mut at, 0); // texelBlockDimension: 1x1x1x1
    }
    write_u8(&mut dfd, &mut at, 4); // bytesPlane[0]
    for _ in 1_i32..8_i32 {
        write_u8(&mut dfd, &mut at, 0);
    }
    for (bit_offset, channel) in [(0_u16, 0_u8), (8, 1), (16, 2), (24, 3)] {
        write_u16(&mut dfd, &mut at, bit_offset);
        write_u8(&mut dfd, &mut at, 7); // 8-bit component: bitLength = 8 - 1
        write_u8(&mut dfd, &mut at, channel); // qualifiers 0 | channel type
        for _ in 0_i32..4_i32 {
            write_u8(&mut dfd, &mut at, 0); // samplePosition
        }
        write_u32(&mut dfd, &mut at, 0); // sampleLower
        write_u32(&mut dfd, &mut at, 255); // sampleUpper
    }
    debug_assert_eq!(
        at,
        dfd.len(),
        "RGBA descriptor fields must fill the declared descriptor"
    );
    dfd
}

/// The standard four-sample RGBSDA descriptor for RGBA16F.
///
/// It differs from [`rgba8_dfd`] only in the sample encoding, which a DFD
/// carries as `bitLength = 15` for a 16-bit component: the channel types sit
/// at bit offsets 0/16/32/48, `bytesPlane[0]` is 8, and `sampleUpper` is
/// `0x3F800000` (1.0 as binary32 bits).
fn rgba16f_dfd() -> [u8; DFD_BYTES] {
    let mut dfd = [0_u8; DFD_BYTES];
    let mut at = 0_usize;
    write_u32(&mut dfd, &mut at, DFD_TOTAL_SIZE);
    write_u32(&mut dfd, &mut at, 0); // vendorId 0, descriptorType 0
    write_u16(&mut dfd, &mut at, 2); // versionNumber
    write_u16(&mut dfd, &mut at, 88); // descriptorBlockSize
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_MODEL_RGBSDA
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_PRIMARIES_BT709
    write_u8(&mut dfd, &mut at, 1); // KHR_DF_TRANSFER_LINEAR
    write_u8(&mut dfd, &mut at, 0); // flags
    for _ in 0_i32..4_i32 {
        write_u8(&mut dfd, &mut at, 0); // texelBlockDimension: 1x1x1x1
    }
    write_u8(&mut dfd, &mut at, 8); // bytesPlane[0]
    for _ in 1_i32..8_i32 {
        write_u8(&mut dfd, &mut at, 0);
    }
    for (bit_offset, channel) in [(0_u16, 0_u8), (16, 1), (32, 2), (48, 3)] {
        write_u16(&mut dfd, &mut at, bit_offset);
        write_u8(&mut dfd, &mut at, 15); // 16-bit component: bitLength = 16 - 1
        write_u8(&mut dfd, &mut at, channel); // qualifiers 0 | channel type
        for _ in 0_i32..4_i32 {
            write_u8(&mut dfd, &mut at, 0); // samplePosition
        }
        write_u32(&mut dfd, &mut at, 0); // sampleLower
        write_u32(&mut dfd, &mut at, 0x3F80_0000); // sampleUpper: 1.0
    }
    debug_assert_eq!(
        at,
        dfd.len(),
        "HDR descriptor fields must fill the declared descriptor"
    );
    dfd
}

fn validate_dfd(dfd: &[u8], format: &TexelFormat) -> Result<(), String> {
    if dfd != (format.dfd)() {
        return Err(format!(
            "KTX2 DFD is not the {} descriptor this contract expects",
            format.dfd_name
        ));
    }
    Ok(())
}

const fn align4(value: u64) -> u64 {
    value.saturating_add(3) & !3
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u8(out: &mut [u8], at: &mut usize, value: u8) {
    if let Some(slot) = out.get_mut(*at) {
        *slot = value;
    }
    *at = at.saturating_add(1);
}

fn write_u16(out: &mut [u8], at: &mut usize, value: u16) {
    for byte in value.to_le_bytes() {
        write_u8(out, at, byte);
    }
}

fn write_u32(out: &mut [u8], at: &mut usize, value: u32) {
    for byte in value.to_le_bytes() {
        write_u8(out, at, byte);
    }
}

/// A bounds-checked view over the raw file used by the reader.
struct Slice<'a> {
    bytes: &'a [u8],
}

impl<'a> Slice<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take(&self, offset: u64, length: usize) -> Result<&'a [u8], String> {
        let start = usize::try_from(offset)
            .map_err(|error| format!("KTX2 offset is too large: {error}"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| "KTX2 range overflows".to_string())?;
        self.bytes
            .get(start..end)
            .ok_or_else(|| "KTX2 file is truncated".to_string())
    }

    fn u32_at(&self, offset: u64) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .take(offset, 4)?
            .try_into()
            .map_err(|error| format!("KTX2 file is truncated: {error}"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64_at(&self, offset: u64) -> Result<u64, String> {
        let bytes: [u8; 8] = self
            .take(offset, 8)?
            .try_into()
            .map_err(|error| format!("KTX2 file is truncated: {error}"))?;
        Ok(u64::from_le_bytes(bytes))
    }
}

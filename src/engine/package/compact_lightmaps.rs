//! Bounded tile-local storage for linear HDR lightmap plane pairs.
//!
//! A 16x16 tile stores six independent linear ranges and 63 coefficient bits
//! per texel (RGB11, XYZ10). Difficult tiles retain all six binary16 channels
//! exactly. Both modes expand to the existing irradiance/moment RGBA16F plane
//! order before filtering; their implicit alpha is always 0.5. This is a disk
//! codec, not a hardware-filterable GPU texture format.

use super::ktx2::{f16_bits_to_f32, f32_to_f16_bits};

const MAGIC: [u8; 4] = *b"PLC1";
/// Compact payload revision; independent of the solved-light representation.
pub const VERSION: u16 = 1;
const TILE_EDGE: u32 = 16;
const HEADER_BYTES: usize = 24;
const BLOCK_HEADER_BYTES: usize = 4;
const RANGE_BYTES: usize = 48;
const MODE_COMPACT: u8 = 1;
const MODE_EXACT: u8 = 2;
const RESERVED_ALPHA: u16 = 0x3800;
const MAX_EXPANDED_BYTES: u64 = super::MAX_LIGHTMAP_ATLAS_BYTES.saturating_sub(64 * 1024);
const MAX_CODES: [u16; 6] = [2047, 2047, 2047, 1023, 1023, 1023];
const BIT_OFFSETS: [u32; 6] = [0, 11, 22, 33, 43, 53];
// Two binary16 relative ULPs plus four smallest binary16 subnormals. Energy
// channels have independent relative bounds so dim colours cannot disappear.
const COEFFICIENT_RELATIVE: f64 = 0.001_953_125;
const LIGHT_RELATIVE: f64 = 0.003_906_25;
const ABSOLUTE_ERROR: f64 = 0.000_000_238_418_579_101_562_5;
const MOMENT_ROUNDING: f64 = 3.0_f64 / 16_777_216.0_f64;
const NORMALS: [[f32; 3]; 10] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
    [0.577_350_26, 0.577_350_26, 0.577_350_26],
    [-0.577_350_26, 0.577_350_26, 0.577_350_26],
    [0.577_350_26, -0.577_350_26, 0.577_350_26],
    [0.577_350_26, 0.577_350_26, -0.577_350_26],
];

/// Encoding decisions, useful for reporting actual storage precision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactLightmapStats {
    /// Tile pairs using the bounded 63-bit representation.
    pub compact_blocks: usize,
    /// Tile pairs retaining the six source halves exactly.
    pub exact_blocks: usize,
}

/// Decoded plane pairs, directly usable by the existing linear atlas reader.
#[derive(Debug, PartialEq, Eq)]
pub struct CompactLightmapImage {
    /// Square page edge in texels.
    pub edge: u32,
    /// Base pages in each independent illumination group.
    pub page_count: u32,
    /// Base group plus independently switchable contribution groups.
    pub group_count: u32,
    /// RGBA16F planes: each page's irradiance then its signed moment, with
    /// groups in existing package order. Alphas are canonical 0.5.
    pub rgba16f_planes: Vec<u8>,
    /// Actual tile modes read from the payload.
    pub stats: CompactLightmapStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct Range {
    minimum: f32,
    step: f32,
}

#[derive(Clone, Copy)]
struct Shape {
    edge: u32,
    pages: u32,
    groups: u32,
    plane_bytes: usize,
    expanded_bytes: usize,
    pairs: usize,
}

impl Shape {
    fn new(edge: u32, pages: u32, groups: u32) -> Result<Self, String> {
        if edge == 0 || edge > super::MAX_LIGHTMAP_PAGE_EDGE {
            return Err("compact lightmap page edge is out of range".to_string());
        }
        if pages == 0 || usize::try_from(pages).unwrap_or(usize::MAX) > super::MAX_LIGHTMAP_PAGES {
            return Err("compact lightmap page count is out of range".to_string());
        }
        if groups == 0
            || usize::try_from(groups).unwrap_or(usize::MAX)
                > super::MAX_SWITCHABLE_LIGHTS.saturating_add(1)
        {
            return Err("compact lightmap group count is out of range".to_string());
        }
        let plane = u64::from(edge)
            .checked_mul(u64::from(edge))
            .and_then(|texels| texels.checked_mul(8))
            .ok_or_else(|| "compact lightmap plane size overflows".to_string())?;
        let pair_count = u64::from(pages)
            .checked_mul(u64::from(groups))
            .ok_or_else(|| "compact lightmap pair count overflows".to_string())?;
        let expanded = plane
            .checked_mul(pair_count)
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or_else(|| "compact lightmap expanded size overflows".to_string())?;
        if expanded > MAX_EXPANDED_BYTES {
            return Err(format!(
                "compact lightmap expanded payload exceeds {MAX_EXPANDED_BYTES} bytes"
            ));
        }
        Ok(Self {
            edge,
            pages,
            groups,
            plane_bytes: usize::try_from(plane)
                .map_err(|error| format!("compact lightmap plane size: {error}"))?,
            expanded_bytes: usize::try_from(expanded)
                .map_err(|error| format!("compact lightmap expanded size: {error}"))?,
            pairs: usize::try_from(pair_count)
                .map_err(|error| format!("compact lightmap pair count: {error}"))?,
        })
    }

    fn pair_start(self, pair: usize) -> Result<usize, String> {
        pair.checked_mul(self.plane_bytes)
            .and_then(|offset| offset.checked_mul(2))
            .ok_or_else(|| "compact lightmap pair offset overflows".to_string())
    }

    fn tile_offsets(self, tile_x: u32, tile_y: u32) -> Result<Vec<usize>, String> {
        let width = self.edge.saturating_sub(tile_x).min(TILE_EDGE);
        let height = self.edge.saturating_sub(tile_y).min(TILE_EDGE);
        let mut offsets = Vec::with_capacity(256);
        for dy in 0..height {
            for dx in 0..width {
                let x = tile_x
                    .checked_add(dx)
                    .ok_or_else(|| "compact lightmap tile x overflows".to_string())?;
                let y = tile_y
                    .checked_add(dy)
                    .ok_or_else(|| "compact lightmap tile y overflows".to_string())?;
                let offset = u64::from(y)
                    .checked_mul(u64::from(self.edge))
                    .and_then(|row| row.checked_add(u64::from(x)))
                    .and_then(|index| index.checked_mul(8))
                    .ok_or_else(|| "compact lightmap texel offset overflows".to_string())?;
                offsets.push(
                    usize::try_from(offset)
                        .map_err(|error| format!("compact lightmap texel offset: {error}"))?,
                );
            }
        }
        Ok(offsets)
    }
}

/// Encode existing canonical half-float plane pairs without changing their
/// chart allocation, linear-light interpretation or independent group order.
///
/// # Errors
/// Rejects unsupported dimensions/groups, expanded size over the shared typed atlas budget, wrong
/// input lengths, noncanonical alpha, nonfinite/negative irradiance, nonfinite
/// moments and moments beyond the energy bound including binary16 rounding.
pub fn encode(
    edge: u32,
    page_count: u32,
    group_count: u32,
    rgba16f_planes: &[u8],
) -> Result<Vec<u8>, String> {
    encode_with_stats(edge, page_count, group_count, rgba16f_planes).map(|(bytes, _)| bytes)
}

/// Encode plane pairs and report the automatic exact fallback decisions.
///
/// # Errors
/// Returns the same bounded input/coefficients errors as [`encode`].
pub fn encode_with_stats(
    edge: u32,
    page_count: u32,
    group_count: u32,
    rgba16f_planes: &[u8],
) -> Result<(Vec<u8>, CompactLightmapStats), String> {
    let shape = Shape::new(edge, page_count, group_count)?;
    if rgba16f_planes.len() != shape.expanded_bytes {
        return Err("compact lightmap input plane length does not match its shape".to_string());
    }
    encode_plane_pairs(shape, |pair| {
        let start = shape.pair_start(pair)?;
        let middle = start
            .checked_add(shape.plane_bytes)
            .ok_or("lightmap plane offset overflows")?;
        let end = middle
            .checked_add(shape.plane_bytes)
            .ok_or("lightmap plane offset overflows")?;
        Ok((
            rgba16f_planes
                .get(start..middle)
                .ok_or("missing irradiance plane")?,
            rgba16f_planes
                .get(middle..end)
                .ok_or("missing moment plane")?,
        ))
    })
}

/// Encode separate planes without materializing a second contiguous atlas.
/// Every plane and the aggregate shape are checked before encoding any tile.
pub(crate) fn encode_layers_with_stats(
    edge: u32,
    pages: u32,
    groups: u32,
    layers: &[Vec<u8>],
) -> Result<(Vec<u8>, CompactLightmapStats), String> {
    let shape = Shape::new(edge, pages, groups)?;
    if layers.len() != shape.pairs.saturating_mul(2)
        || layers.iter().any(|plane| plane.len() != shape.plane_bytes)
    {
        return Err("compact lightmap input plane length does not match its shape".into());
    }
    encode_plane_pairs(shape, |pair| {
        let first = pair
            .checked_mul(2)
            .ok_or("lightmap plane index overflows")?;
        Ok((
            layers
                .get(first)
                .ok_or("missing irradiance plane")?
                .as_slice(),
            layers
                .get(first.saturating_add(1))
                .ok_or("missing moment plane")?
                .as_slice(),
        ))
    })
}

fn encode_plane_pairs<'a>(
    shape: Shape,
    planes: impl Fn(usize) -> Result<(&'a [u8], &'a [u8]), String>,
) -> Result<(Vec<u8>, CompactLightmapStats), String> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&u16::try_from(TILE_EDGE).unwrap_or(u16::MAX).to_le_bytes());
    for value in [shape.edge, shape.pages, shape.groups, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    let mut stats = CompactLightmapStats::default();
    for pair in 0..shape.pairs {
        crate::engine::progress::check_cancelled()?;
        let pair_planes = planes(pair)?;
        for tile_y in (0..shape.edge).step_by(16) {
            for tile_x in (0..shape.edge).step_by(16) {
                let offsets = shape.tile_offsets(tile_x, tile_y)?;
                let mut texels = Vec::with_capacity(offsets.len());
                for offset in offsets {
                    let bits = read_texel(pair_planes, offset)?;
                    validate_texel(bits.map(f16_bits_to_f32))?;
                    texels.push(bits);
                }
                if let Some((ranges, words)) = compact_tile(&texels) {
                    out.extend_from_slice(&[MODE_COMPACT, 0, 0, 0]);
                    for range in ranges {
                        out.extend_from_slice(&range.minimum.to_le_bytes());
                        out.extend_from_slice(&range.step.to_le_bytes());
                    }
                    for word in words {
                        out.extend_from_slice(&word.to_le_bytes());
                    }
                    stats.compact_blocks = stats.compact_blocks.saturating_add(1);
                } else {
                    out.extend_from_slice(&[MODE_EXACT, 0, 0, 0]);
                    for bits in texels {
                        for channel in bits {
                            out.extend_from_slice(&channel.to_le_bytes());
                        }
                    }
                    stats.exact_blocks = stats.exact_blocks.saturating_add(1);
                }
            }
        }
    }
    if u64::try_from(out.len()).unwrap_or(u64::MAX) > super::MAX_LIGHTMAP_ATLAS_BYTES {
        return Err("compact lightmap encoded record exceeds the atlas byte bound".to_string());
    }
    Ok((out, stats))
}

fn read_rgb(bytes: &[u8], offset: usize) -> Result<([u16; 3], u16), String> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| "compact lightmap source texel range overflows".to_string())?;
    let row = bytes
        .get(offset..end)
        .ok_or_else(|| "compact lightmap source texel is truncated".to_string())?;
    let [red, green, blue, alpha] = row.as_chunks::<2>().0 else {
        return Err("compact lightmap source texel is truncated".to_string());
    };
    Ok((
        [
            u16::from_le_bytes(*red),
            u16::from_le_bytes(*green),
            u16::from_le_bytes(*blue),
        ],
        u16::from_le_bytes(*alpha),
    ))
}

fn read_texel(planes: (&[u8], &[u8]), offset: usize) -> Result<[u16; 6], String> {
    let ([red, green, blue], mean_alpha) = read_rgb(planes.0, offset)?;
    let ([moment_x, moment_y, moment_z], moment_alpha) = read_rgb(planes.1, offset)?;
    if mean_alpha != RESERVED_ALPHA || moment_alpha != RESERVED_ALPHA {
        return Err("compact lightmap source alpha is not the reserved 0.5".to_string());
    }
    Ok([red, green, blue, moment_x, moment_y, moment_z])
}

fn validate_texel(values: [f32; 6]) -> Result<(), String> {
    if !values.iter().all(|value| value.is_finite()) {
        return Err("compact lightmap has a nonfinite half-float coefficient".to_string());
    }
    let [red, green, blue, moment_x, moment_y, moment_z] = values.map(f64::from);
    if red < 0.0_f64 || green < 0.0_f64 || blue < 0.0_f64 {
        return Err("compact lightmap irradiance is negative".to_string());
    }
    let energy = red + green + blue;
    let moment = moment_x
        .mul_add(moment_x, moment_y.mul_add(moment_y, moment_z * moment_z))
        .sqrt();
    // Independent half rounding can erase all three tiny means while retaining
    // a subnormal moment. The existing shader skips direction below its energy
    // epsilon; preserve these real prepared values within the rounding bound.
    if moment > energy.mul_add(1.0_f64 + COEFFICIENT_RELATIVE, MOMENT_ROUNDING) {
        return Err("compact lightmap signed moment exceeds its irradiance energy".to_string());
    }
    Ok(())
}

fn compact_tile(texels: &[[u16; 6]]) -> Option<([Range; 6], Vec<u64>)> {
    let mut minimum = [f32::INFINITY; 6];
    let mut maximum = [f32::NEG_INFINITY; 6];
    for bits in texels {
        for ((low, high), value) in minimum
            .iter_mut()
            .zip(&mut maximum)
            .zip(bits.map(f16_bits_to_f32))
        {
            *low = low.min(value);
            *high = high.max(value);
        }
    }
    let mut ranges = [Range::default(); 6];
    for ((((range, low), high), code_maximum), channel) in ranges
        .iter_mut()
        .zip(minimum)
        .zip(maximum)
        .zip(MAX_CODES)
        .zip(0..6)
    {
        *range = Range {
            minimum: low,
            step: (high - low) / f32::from(code_maximum),
        };
        if !range_valid(*range, channel, code_maximum) {
            return None;
        }
    }
    let mut words = Vec::with_capacity(texels.len());
    for bits in texels {
        let source = bits.map(f16_bits_to_f32);
        let mut restored = [0.0_f32; 6];
        let mut word = 0_u64;
        for ((((range, maximum_code), bit_offset), value), decoded) in ranges
            .iter()
            .zip(MAX_CODES)
            .zip(BIT_OFFSETS)
            .zip(source)
            .zip(&mut restored)
        {
            let code = if range.step > 0.0 {
                quantized_code((value - range.minimum) / range.step, maximum_code)
            } else {
                0
            };
            word |= u64::from(code) << bit_offset;
            *decoded = f16_bits_to_f32(f32_to_f16_bits(
                range.step.mul_add(f32::from(code), range.minimum),
            ));
        }
        if validate_texel(restored).is_err() || !within_error(source, restored) {
            return None;
        }
        words.push(word);
    }
    Some((ranges, words))
}

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Finite quantization coordinates are rounded and clamped to the supplied u16 code maximum before conversion."
)]
fn quantized_code(value: f32, maximum: u16) -> u16 {
    value.round().clamp(0.0, f32::from(maximum)) as u16
}

const fn lightmap_texel(values: [f32; 6]) -> crate::engine::lighting::lightmap::LightmapTexel {
    let [red, green, blue, moment_x, moment_y, moment_z] = values;
    crate::engine::lighting::lightmap::LightmapTexel {
        irradiance: [red, green, blue],
        direction: [moment_x, moment_y, moment_z],
        axis: [0.5, 0.5],
    }
}

fn within_error(source: [f32; 6], restored: [f32; 6]) -> bool {
    if source
        .iter()
        .zip(restored)
        .all(|(original, decoded)| original.to_bits() == decoded.to_bits())
    {
        return true;
    }
    let reference = lightmap_texel(source);
    let candidate = lightmap_texel(restored);
    let source_energy: f64 = reference.irradiance.iter().copied().map(f64::from).sum();
    let restored_energy: f64 = candidate.irradiance.iter().copied().map(f64::from).sum();
    if (source_energy > 1.0e-6_f64) != (restored_energy > 1.0e-6_f64) {
        return false;
    }
    for (original_channel, decoded_channel) in reference.irradiance.iter().zip(candidate.irradiance)
    {
        let original_value = f64::from(*original_channel);
        let decoded_value = f64::from(decoded_channel);
        if (original_value > 0.0_f64 && decoded_value <= 0.0_f64)
            || (original_value - decoded_value).abs()
                > (original_value * COEFFICIENT_RELATIVE).max(ABSOLUTE_ERROR)
        {
            return false;
        }
    }
    for (original, decoded) in reference.direction.iter().zip(candidate.direction) {
        if (f64::from(*original) - f64::from(decoded)).abs()
            > source_energy.mul_add(COEFFICIENT_RELATIVE, ABSOLUTE_ERROR)
        {
            return false;
        }
    }
    // F(h,n)=1+2max(dot(h,n),0)-|h| is 3-Lipschitz in h=g/sum(I).
    // This additionally bounds all unit normals; the explicit normal checks
    // below protect the shader's epsilon branch and dark/grazing responses.
    if source_energy > 1.0e-6_f64 && restored_energy > 1.0e-6_f64 {
        let squared_difference: f64 = reference
            .direction
            .iter()
            .zip(candidate.direction)
            .map(|(original, decoded)| {
                let difference =
                    f64::from(*original) / source_energy - f64::from(decoded) / restored_energy;
                difference * difference
            })
            .sum();
        let difference = squared_difference.sqrt();
        let source_moment_squared: f64 = reference
            .direction
            .iter()
            .copied()
            .map(f64::from)
            .map(|value| value * value)
            .sum();
        let restored_moment_squared: f64 = candidate
            .direction
            .iter()
            .copied()
            .map(f64::from)
            .map(|value| value * value)
            .sum();
        let maximum_factor = 1.0_f64
            + (source_moment_squared.sqrt() / source_energy)
                .max(restored_moment_squared.sqrt() / restored_energy);
        for (original_channel, decoded_channel) in
            reference.irradiance.iter().zip(candidate.irradiance)
        {
            let original_value = f64::from(*original_channel);
            let decoded_value = f64::from(decoded_channel);
            let bound = (3.0_f64 * original_value.min(decoded_value)).mul_add(
                difference,
                maximum_factor * (original_value - decoded_value).abs(),
            );
            if bound
                > original_value
                    .max(decoded_value)
                    .mul_add(LIGHT_RELATIVE, ABSOLUTE_ERROR)
            {
                return false;
            }
        }
    }
    for normal in NORMALS {
        for (original_channel, decoded_channel) in reference
            .light_at(normal)
            .into_iter()
            .zip(candidate.light_at(normal))
        {
            let original_value = f64::from(original_channel);
            let decoded_value = f64::from(decoded_channel);
            if (original_value - decoded_value).abs()
                > original_value
                    .max(decoded_value)
                    .mul_add(LIGHT_RELATIVE, ABSOLUTE_ERROR)
            {
                return false;
            }
        }
    }
    true
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let end = self
            .position
            .checked_add(N)
            .ok_or_else(|| "compact lightmap read range overflows".to_string())?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "compact lightmap payload is truncated".to_string())?;
        let value = <[u8; N]>::try_from(slice)
            .map_err(|error| format!("compact lightmap field: {error}"))?;
        self.position = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, String> {
        self.take().map(u32::from_le_bytes)
    }

    const fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }
}

/// Decode bounded compact lightmaps into the original linear half planes.
///
/// # Errors
/// Rejects unsupported versions/modes, dimensions/groups, oversized expanded
/// output, nonfinite/invalid ranges and coefficients, nonzero reserved bits,
/// truncated blocks and trailing bytes. Invalid data is never normalized.
pub fn decode(bytes: &[u8]) -> Result<CompactLightmapImage, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > super::MAX_LIGHTMAP_ATLAS_BYTES {
        return Err("compact lightmap encoded record exceeds the atlas byte bound".to_string());
    }
    let mut reader = Reader { bytes, position: 0 };
    if reader.take::<4>()? != MAGIC {
        return Err("compact lightmap magic is invalid".to_string());
    }
    let version = u16::from_le_bytes(reader.take()?);
    if version != VERSION {
        return Err(format!(
            "compact lightmap version {version} is not supported"
        ));
    }
    if u32::from(u16::from_le_bytes(reader.take()?)) != TILE_EDGE {
        return Err("compact lightmap tile edge is not supported".to_string());
    }
    let edge = reader.u32()?;
    let pages = reader.u32()?;
    let groups = reader.u32()?;
    if reader.u32()? != 0 {
        return Err("compact lightmap header reserved bits are nonzero".to_string());
    }
    let shape = Shape::new(edge, pages, groups)?;
    validate_block_lengths(&mut reader, shape)?;
    reader.position = HEADER_BYTES;
    let mut planes = vec![0_u8; shape.expanded_bytes];
    let mut stats = CompactLightmapStats::default();
    for pair in 0..shape.pairs {
        let start = shape.pair_start(pair)?;
        for tile_y in (0..edge).step_by(16) {
            for tile_x in (0..edge).step_by(16) {
                let offsets = shape.tile_offsets(tile_x, tile_y)?;
                let mode = block_mode(&mut reader)?;
                let ranges = if mode == MODE_COMPACT {
                    stats.compact_blocks = stats.compact_blocks.saturating_add(1);
                    read_ranges(&mut reader)?
                } else {
                    stats.exact_blocks = stats.exact_blocks.saturating_add(1);
                    [Range::default(); 6]
                };
                for offset in offsets {
                    let bits = read_block_texel(&mut reader, mode, ranges)?;
                    validate_texel(bits.map(f16_bits_to_f32))?;
                    write_texel(&mut planes, shape, start, offset, bits)?;
                }
            }
        }
    }
    if reader.remaining() != 0 {
        return Err("compact lightmap payload has trailing bytes".to_string());
    }
    Ok(CompactLightmapImage {
        edge: shape.edge,
        page_count: shape.pages,
        group_count: shape.groups,
        rgba16f_planes: planes,
        stats,
    })
}

// Validate the complete framing before allocating the expanded output. A tiny
// attacker-controlled payload cannot request a large output from its header.
fn validate_block_lengths(reader: &mut Reader<'_>, shape: Shape) -> Result<(), String> {
    for _ in 0..shape.pairs {
        for tile_y in (0..shape.edge).step_by(16) {
            for tile_x in (0..shape.edge).step_by(16) {
                let mode = block_mode(reader)?;
                let width = shape.edge.saturating_sub(tile_x).min(TILE_EDGE);
                let height = shape.edge.saturating_sub(tile_y).min(TILE_EDGE);
                let texels = usize::try_from(width)
                    .ok()
                    .and_then(|value| value.checked_mul(usize::try_from(height).ok()?))
                    .ok_or_else(|| "compact lightmap block size overflows".to_string())?;
                let payload = texels
                    .checked_mul(if mode == MODE_COMPACT { 8 } else { 12 })
                    .and_then(|length| {
                        length.checked_add(if mode == MODE_COMPACT { RANGE_BYTES } else { 0 })
                    })
                    .ok_or_else(|| "compact lightmap block payload size overflows".to_string())?;
                let end = reader
                    .position
                    .checked_add(payload)
                    .ok_or_else(|| "compact lightmap block range overflows".to_string())?;
                if end > reader.bytes.len() {
                    return Err("compact lightmap block is truncated".to_string());
                }
                reader.position = end;
            }
        }
    }
    if reader.remaining() != 0 {
        return Err("compact lightmap payload has trailing bytes".to_string());
    }
    Ok(())
}

fn block_mode(reader: &mut Reader<'_>) -> Result<u8, String> {
    let [mode, reserved_a, reserved_b, reserved_c] = reader.take::<BLOCK_HEADER_BYTES>()?;
    if reserved_a != 0 || reserved_b != 0 || reserved_c != 0 {
        return Err("compact lightmap block reserved bits are nonzero".to_string());
    }
    if mode != MODE_COMPACT && mode != MODE_EXACT {
        return Err(format!(
            "compact lightmap block mode {mode} is not supported"
        ));
    }
    Ok(mode)
}

fn read_ranges(reader: &mut Reader<'_>) -> Result<[Range; 6], String> {
    let mut ranges = [Range::default(); 6];
    for ((range, maximum), channel) in ranges.iter_mut().zip(MAX_CODES).zip(0..6) {
        let minimum = f32::from_le_bytes(reader.take()?);
        let step = f32::from_le_bytes(reader.take()?);
        if !range_valid(Range { minimum, step }, channel, maximum) {
            return Err("compact lightmap coefficient range is invalid".to_string());
        }
        *range = Range { minimum, step };
    }
    Ok(ranges)
}

fn range_valid(range: Range, channel: usize, maximum: u16) -> bool {
    let upper = range.step.mul_add(f32::from(maximum), range.minimum);
    range.minimum.is_finite()
        && range.step.is_finite()
        && range.step >= 0.0
        && range.step.to_bits() != 0x8000_0000
        && upper.is_finite()
        && range.minimum.abs() <= 65_504.0
        && upper.abs() <= 65_504.0
        && (channel >= 3 || range.minimum >= 0.0)
}

fn read_block_texel(
    reader: &mut Reader<'_>,
    mode: u8,
    ranges: [Range; 6],
) -> Result<[u16; 6], String> {
    let mut bits = [0_u16; 6];
    if mode == MODE_EXACT {
        for value in &mut bits {
            *value = u16::from_le_bytes(reader.take()?);
        }
    } else {
        let word = u64::from_le_bytes(reader.take()?);
        if word & (1_u64 << 63_u32) != 0 {
            return Err("compact lightmap texel reserved bit is nonzero".to_string());
        }
        for (((value, range), maximum), offset) in
            bits.iter_mut().zip(ranges).zip(MAX_CODES).zip(BIT_OFFSETS)
        {
            let shifted = word
                .checked_shr(offset)
                .ok_or_else(|| "compact lightmap coefficient shift is out of range".to_string())?;
            let code = u16::try_from(shifted & u64::from(maximum))
                .map_err(|error| format!("compact lightmap coefficient code: {error}"))?;
            if range.step.to_bits() == 0 && code != 0 {
                return Err("compact lightmap constant range has a nonzero code".to_string());
            }
            *value = f32_to_f16_bits(range.step.mul_add(f32::from(code), range.minimum));
        }
    }
    Ok(bits)
}

fn write_texel(
    planes: &mut [u8],
    shape: Shape,
    pair_start: usize,
    offset: usize,
    bits: [u16; 6],
) -> Result<(), String> {
    let mean = pair_start
        .checked_add(offset)
        .ok_or_else(|| "compact lightmap output offset overflows".to_string())?;
    let moment = mean
        .checked_add(shape.plane_bytes)
        .ok_or_else(|| "compact lightmap output moment offset overflows".to_string())?;
    let [red, green, blue, moment_x, moment_y, moment_z] = bits;
    for (start, values) in [
        (mean, [red, green, blue, RESERVED_ALPHA]),
        (moment, [moment_x, moment_y, moment_z, RESERVED_ALPHA]),
    ] {
        let end = start
            .checked_add(8)
            .ok_or_else(|| "compact lightmap output texel range overflows".to_string())?;
        let target = planes
            .get_mut(start..end)
            .ok_or_else(|| "compact lightmap output texel is outside its plane".to_string())?;
        for (slot, half) in target.as_chunks_mut::<2>().0.iter_mut().zip(values) {
            *slot = half.to_le_bytes();
        }
    }
    Ok(())
}

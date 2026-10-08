//! Offline prefiltering of captured reflection probes.
//!
//! The compiler captures one sharp mirror image per cube face; this module
//! turns that capture into the roughness mip chain a package stores, so the
//! player samples a prefiltered cube instead of faking a blur with extra taps.
//!
//! The filter is pure CPU and deliberately simple:
//!
//! * Level 0 is a verbatim copy of the captured faces.
//! * Level `L` of a chain of `levels` carries roughness `L / (levels - 1)`,
//!   clamped to `[0, 1]`, and half the edge of the level before it (down to one
//!   texel).
//! * Every texel's value is a cone average of level 0: `K` deterministic taps
//!   are laid out inside a spherical cap around the texel's own direction
//!   (area-uniform, golden-angle spiral) and weighted by a cosine-power lobe
//!   whose exponent grows as roughness falls. This is the standard offline
//!   roughness convolution, run once at build time.
//! * Every tap is read with bilinear filtering across the six faces: each
//!   bilinear corner is turned back into a direction, which renormalisation maps
//!   to whichever face actually owns that direction, so a tap near a face edge
//!   reads the correct neighbouring face instead of clamping.
//!
//! The cube face order and up vectors mirror the renderer's capture order
//! (`+X/-X/+Y/-Y/+Z/-Z` with the reference's up vectors); the reflection module
//! pins the two lists equal by test. The direction-to-texel mapping is the
//! WebGPU cube-map convention, so a direction sampled from the chain matches a
//! GPU `textureSampleLevel` of the same cube.
//!
//! Everything is sequential `f32` accumulation with no threads, no clock and no
//! hash iteration: two builds of the same capture produce byte-identical chains.
//! Production output is linear RGBA16F; the legacy numeric RGBA8 fixture
//! codec is retained only for orientation/filter tests.

use glam::Vec3;

/// Maximum mip levels a packaged probe chain may carry.
///
/// A 64-texel face halves down to one texel in seven levels; the cap also keeps
/// the package record's level field small enough for the uniform's four-bit
/// encoding.
pub const MAX_PROBE_MIPS: u32 = 7;

/// The six cube face directions, in the GL cube order the capture uses.
///
/// Mirrored from the reflection module's `CUBE_FACE_DIRECTIONS`; a test there
/// pins the two lists equal.
pub const CUBE_FACE_DIRECTIONS: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],  // +X
    [-1.0, 0.0, 0.0], // -X
    [0.0, 1.0, 0.0],  // +Y
    [0.0, -1.0, 0.0], // -Y
    [0.0, 0.0, 1.0],  // +Z
    [0.0, 0.0, -1.0], // -Z
];

/// The up vector of each cube face, in the same order as
/// [`CUBE_FACE_DIRECTIONS`].
pub const CUBE_FACE_UPS: [[f32; 3]; 6] = [
    [0.0, -1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
    [0.0, -1.0, 0.0],
    [0.0, -1.0, 0.0],
];

/// Golden angle, the spiral's angular step, in radians.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// The mip level count a face edge of `edge` texels supports: the base level
/// plus every halving down to one texel, capped at [`MAX_PROBE_MIPS`].
///
/// A zero edge (nothing to capture) still answers one level, so callers can use
/// this as a bound without special-casing an empty probe set.
#[must_use]
pub const fn mip_levels_for(edge: u32) -> u32 {
    if edge == 0 {
        return 1;
    }
    let levels = edge.ilog2().saturating_add(1);
    if levels > MAX_PROBE_MIPS {
        MAX_PROBE_MIPS
    } else {
        levels
    }
}

/// Builds the full prefiltered mip chain for one captured cube.
///
/// `faces` are the six RGBA8 base faces in the capture's cube order, each
/// exactly `edge * edge * 4` bytes. The returned chain is ordered base level
/// first: level `L` holds six tightly packed faces of `max(1, edge >> L)`
/// texels, and level 0 is a copy of `faces`.
///
/// # Errors
///
/// Returns an error when `edge` is zero, `levels` is zero or exceeds
/// [`mip_levels_for`], or a source face does not hold `edge * edge * 4` bytes.
#[cfg(test)]
pub fn prefilter_cube(
    edge: u32,
    faces: &[Vec<u8>; 6],
    levels: u32,
) -> Result<Vec<[Vec<u8>; 6]>, String> {
    prefilter_encoded_cube(edge, faces, levels, false)
}

/// Prefilters linear HDR faces without a display curve or unit-range clamp.
pub fn prefilter_hdr_cube(
    edge: u32,
    faces: &[Vec<u8>; 6],
    levels: u32,
) -> Result<Vec<[Vec<u8>; 6]>, String> {
    prefilter_encoded_cube(edge, faces, levels, true)
}

fn prefilter_encoded_cube(
    edge: u32,
    faces: &[Vec<u8>; 6],
    levels: u32,
    hdr: bool,
) -> Result<Vec<[Vec<u8>; 6]>, String> {
    if edge == 0 {
        return Err("probe face edge is zero".to_string());
    }
    let max_levels = mip_levels_for(edge);
    if levels == 0 || levels > max_levels {
        return Err(format!(
            "probe mip chain has {levels} levels; a {edge}-texel face allows 1..={max_levels}"
        ));
    }
    let base_bytes = face_bytes(edge, hdr)?;
    for (face, data) in faces.iter().enumerate() {
        if data.len() != base_bytes {
            return Err(format!(
                "probe face {face} holds {} bytes, expected {base_bytes}",
                data.len()
            ));
        }
    }

    let mut chain: Vec<[Vec<u8>; 6]> = Vec::with_capacity(usize::try_from(levels).unwrap_or(1));
    chain.push(faces.clone());
    for level in 1..levels {
        let level_edge = edge >> level;
        let roughness = level_roughness(level, levels);
        let mip: [Vec<u8>; 6] = std::array::from_fn(|face| {
            let mut out = Vec::with_capacity(face_bytes(level_edge, hdr).unwrap_or(0));
            for row in 0..level_edge {
                for col in 0..level_edge {
                    let s = texel_centre(col, level_edge);
                    let t = texel_centre(row, level_edge);
                    let direction = direction_for_texel(face, s, t);
                    let colour = cone_average(faces, edge, direction, roughness, hdr);
                    for channel in colour {
                        if hdr {
                            out.extend_from_slice(
                                &crate::package::ktx2::f32_to_f16_bits(channel).to_le_bytes(),
                            );
                        } else {
                            out.push(to_u8(channel));
                        }
                    }
                }
            }
            out
        });
        chain.push(mip);
    }
    Ok(chain)
}

/// Roughness of level `level` in a chain of `levels`:
/// `level / (levels - 1)`, clamped to `[0, 1]`.
fn level_roughness(level: u32, levels: u32) -> f32 {
    if levels <= 1 {
        return 0.0;
    }
    let level_f = count_to_f32(level);
    let last_f = count_to_f32(levels.saturating_sub(1));
    (level_f / last_f).clamp(0.0, 1.0)
}

/// Deterministic tap count for a roughness: 8 at a mirror, 64 at full
/// roughness, growing linearly between.
const fn tap_count(roughness: f32) -> u32 {
    let scaled = roughness.mul_add(56.0, 8.0).round().clamp(8.0, 64.0);
    // `scaled` is finite and clamped to [8, 64] before the cast.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`scaled` is finite and clamped to [8, 64] before the cast."
    )]
    let count = scaled as u32;
    count
}

/// Cosine-lobe exponent for a roughness: a tight lobe at a mirror, a broad one
/// at full roughness.
fn lobe_exponent(roughness: f32) -> f32 {
    let alpha = roughness.clamp(0.0, 1.0);
    let tightness = alpha.mul_add(alpha, 1.0e-3);
    (2.0 / tightness).mul_add(1.0, -2.0).clamp(0.5, 2048.0)
}

/// Cone average of the base level around `direction`, in linear `RGBA`.
fn cone_average(
    source: &[Vec<u8>; 6],
    edge: u32,
    direction: Vec3,
    roughness: f32,
    hdr: bool,
) -> [f32; 4] {
    let (right, up) = tangent_basis(direction);
    // The cap opens from a point at a mirror to a hemisphere at full
    // roughness; `cos` of the half-angle bounds the polar distribution.
    let cap_cos = (std::f32::consts::FRAC_PI_2 * roughness.clamp(0.0, 1.0)).cos();
    let taps = tap_count(roughness);
    let exponent = lobe_exponent(roughness);
    let mut sum = [0.0_f32; 4];
    let mut weight_sum = 0.0_f32;
    for tap in 0..taps {
        let tap_f = count_to_f32(tap);
        let count_f = count_to_f32(taps);
        // The first tap is the texel's own direction (the mirror sample); the
        // rest spiral out area-uniformly towards the cap edge.
        let u = tap_f / count_f;
        let cos_theta = (-u).mul_add(1.0 - cap_cos, 1.0);
        let sin_theta = cos_theta.mul_add(-cos_theta, 1.0).max(0.0).sqrt();
        let phi = GOLDEN_ANGLE * tap_f;
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "glam vector operators use floating point components without integer overflow or a panic path."
        )]
        // glam vector products are per-element f32 arithmetic with no overflow or panic path
        let tangent = right * (sin_theta * phi.cos()) + up * (sin_theta * phi.sin());
        let tap_direction = direction.mul_add(Vec3::splat(cos_theta), tangent);
        let weight = direction.dot(tap_direction).max(0.0).powf(exponent);
        if weight <= 0.0 || !weight.is_finite() {
            continue;
        }
        let sample = sample_direction(source, edge, tap_direction, hdr);
        for (channel, value) in sample.iter().enumerate() {
            if let Some(slot) = sum.get_mut(channel) {
                *slot = weight.mul_add(*value, *slot);
            }
        }
        weight_sum += weight;
    }
    if weight_sum > 0.0 {
        let mut out = [0.0_f32; 4];
        for (channel, value) in sum.iter().enumerate() {
            if let Some(slot) = out.get_mut(channel) {
                *slot = value / weight_sum;
            }
        }
        out
    } else {
        sample_direction(source, edge, direction, hdr)
    }
}

/// An orthonormal basis for the cone around `direction`.
fn tangent_basis(direction: Vec3) -> (Vec3, Vec3) {
    let (x, y, z) = (direction.x.abs(), direction.y.abs(), direction.z.abs());
    let helper = if x <= y && x <= z {
        Vec3::X
    } else if y <= z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let right = normalized(direction.cross(helper));
    let up = normalized(right.cross(direction));
    (right, up)
}

/// The direction a face texel centre stands for.
fn direction_for_texel(face: usize, s: f32, t: f32) -> Vec3 {
    let (Some(face_dir), Some(face_up)) = (CUBE_FACE_DIRECTIONS.get(face), CUBE_FACE_UPS.get(face))
    else {
        return Vec3::X;
    };
    let direction_vector = Vec3::from_array(*face_dir);
    let up_vector = Vec3::from_array(*face_up);
    let right = direction_vector.cross(up_vector);
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "glam vector operators use floating point components without integer overflow or a panic path."
    )]
    // per-element f32 arithmetic with no overflow or panic path
    let direction = direction_vector + right * s + up_vector * t;
    normalized(direction)
}

/// The face and face-local `(s, t)` in `[-1, 1]` a direction maps to.
///
/// `s` grows towards the face's right and `t` towards its up; the mapping
/// matches the WebGPU cube-map convention the capture stores.
fn face_uv(direction: Vec3) -> (usize, f32, f32) {
    let mut best_face = 0_usize;
    let mut best_dot = f32::NEG_INFINITY;
    for (face, face_dir) in CUBE_FACE_DIRECTIONS.iter().enumerate() {
        let dot = direction.dot(Vec3::from_array(*face_dir));
        if dot > best_dot {
            best_face = face;
            best_dot = dot;
        }
    }
    if best_dot <= 0.0 || !best_dot.is_finite() {
        return (0, 0.0, 0.0);
    }
    let (Some(face_dir), Some(face_up)) = (
        CUBE_FACE_DIRECTIONS.get(best_face),
        CUBE_FACE_UPS.get(best_face),
    ) else {
        return (0, 0.0, 0.0);
    };
    let direction_vector = Vec3::from_array(*face_dir);
    let up_vector = Vec3::from_array(*face_up);
    let right = direction_vector.cross(up_vector);
    let s = direction.dot(right) / best_dot;
    let t = direction.dot(up_vector) / best_dot;
    (best_face, s, t)
}

/// Bilinear sample of the base level for one direction, across face edges.
fn sample_direction(source: &[Vec<u8>; 6], edge: u32, direction: Vec3, hdr: bool) -> [f32; 4] {
    let length = direction.length();
    if length <= 0.0 || !length.is_finite() {
        return [0.0; 4];
    }
    let (face, s, t) = face_uv(direction);
    let edge_f = u32_to_f32(edge);
    let scale = 0.5 * edge_f;
    let x = (s + 1.0).mul_add(scale, -0.5);
    let y = (t + 1.0).mul_add(scale, -0.5);
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let mut out = [0.0_f32; 4];
    for (dx, wx) in [(0.0_f32, 1.0 - fx), (1.0, fx)] {
        for (dy, wy) in [(0.0_f32, 1.0 - fy), (1.0, fy)] {
            let weight = wx * wy;
            if weight <= 0.0 {
                continue;
            }
            // The corner's own texel centre, which may lie past the face edge;
            // renormalising its direction selects the neighbouring face.
            let s_corner = ((x0 + dx) + 0.5).mul_add(2.0 / edge_f, -1.0);
            let t_corner = ((y0 + dy) + 0.5).mul_add(2.0 / edge_f, -1.0);
            let corner = texel_at(source, edge, face, s_corner, t_corner, hdr);
            for (channel, value) in corner.iter().enumerate() {
                if let Some(slot) = out.get_mut(channel) {
                    *slot = weight.mul_add(*value, *slot);
                }
            }
        }
    }
    out
}

/// One nearest-texel read at a face-local `(s, t)`, which may be past the face
/// edge: the direction is renormalised and remapped to the face that owns it.
fn texel_at(source: &[Vec<u8>; 6], edge: u32, face: usize, s: f32, t: f32, hdr: bool) -> [f32; 4] {
    let direction = direction_for_texel(face, s, t);
    let (mapped_face, mapped_s, mapped_t) = face_uv(direction);
    let edge_f = u32_to_f32(edge);
    let limit = (edge_f - 1.0).max(0.0);
    let x = f32::midpoint(mapped_s, 1.0)
        .mul_add(edge_f, -0.5)
        .round()
        .clamp(0.0, limit);
    let y = f32::midpoint(mapped_t, 1.0)
        .mul_add(edge_f, -0.5)
        .round()
        .clamp(0.0, limit);
    let Some(data) = source.get(mapped_face) else {
        return [0.0; 4];
    };
    let Some(row) = usize::try_from(texel_index(y)).ok() else {
        return [0.0; 4];
    };
    let Some(col) = usize::try_from(texel_index(x)).ok() else {
        return [0.0; 4];
    };
    let Some(row_edge) = usize::try_from(edge).ok() else {
        return [0.0; 4];
    };
    let Some(at) = row
        .checked_mul(row_edge)
        .and_then(|value| value.checked_add(col))
        .and_then(|value| value.checked_mul(if hdr { 8 } else { 4 }))
    else {
        return [0.0; 4];
    };
    let Some(pixel) = data.get(at..at.saturating_add(if hdr { 8 } else { 4 })) else {
        return [0.0; 4];
    };
    if hdr {
        let mut out = [0.0; 4];
        for (slot, pair) in out.iter_mut().zip(pixel.as_chunks::<2>().0) {
            *slot = crate::package::ktx2::f16_bits_to_f32(u16::from_le_bytes(*pair));
        }
        out
    } else if let [r, g, b, a] = pixel {
        [
            f32::from(*r) * (1.0 / 255.0),
            f32::from(*g) * (1.0 / 255.0),
            f32::from(*b) * (1.0 / 255.0),
            f32::from(*a) * (1.0 / 255.0),
        ]
    } else {
        [0.0; 4]
    }
}

/// The `s` or `t` coordinate of a texel centre, in `[-1, 1]`.
fn texel_centre(index: u32, edge: u32) -> f32 {
    let edge_f = u32_to_f32(edge);
    (count_to_f32(index) + 0.5).mul_add(2.0 / edge_f, -1.0)
}

/// A clamped texel coordinate as an unsigned index.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The finite floored texel coordinate is clamped to the small image edge before its u32 conversion."
)]
// `value` is clamped to [0, edge - 1] with edge a small texel count before
// the cast, so it is finite, non-negative and well inside u32.
const fn texel_index(value: f32) -> u32 {
    value as u32
}

/// Rounds one accumulated `[0, 1]` channel to `u8`, guarding non-finite input.
const fn to_u8(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    let scaled = value.mul_add(255.0, 0.5).clamp(0.0, 255.0);
    // `scaled` is finite and clamped to [0, 255] before the cast.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`scaled` is finite and clamped to [0, 255] before the cast."
    )]
    let byte = scaled as u8;
    byte
}

/// Bytes one `edge` x `edge` RGBA8 face occupies.
fn face_bytes(edge: u32, hdr: bool) -> Result<usize, String> {
    let edge_pixels =
        usize::try_from(edge).map_err(|error| format!("probe face edge is too large: {error}"))?;
    edge_pixels
        .checked_mul(edge_pixels)
        .and_then(|value| value.checked_mul(if hdr { 8 } else { 4 }))
        .ok_or_else(|| "probe face size overflows".to_string())
}

/// A possibly-zero vector scaled to unit length.
fn normalized(value: Vec3) -> Vec3 {
    let length = value.length();
    if length > 0.0 && length.is_finite() {
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "glam vector division uses floating point components without integer overflow or a panic path."
        )]
        // per-element f32 division with no overflow or panic path
        let unit = value / length;
        unit
    } else {
        Vec3::ZERO
    }
}

/// A small count as `f32`.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "counts are probe texel dimensions, far below 2^24"
)] // counts are probe texel dimensions, far below 2^24
const fn count_to_f32(value: u32) -> f32 {
    value as f32
}

/// An edge as `f32`.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "probe face edges are texel counts, far below 2^24"
)] // probe face edges are texel counts, far below 2^24
const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::arithmetic_side_effects,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    /// A cube of solid RGBA faces.
    fn solid_faces(edge: u32, colour: [u8; 4]) -> [Vec<u8>; 6] {
        let bytes = usize::try_from(edge).unwrap() * usize::try_from(edge).unwrap() * 4;
        std::array::from_fn(|_| {
            let mut face = Vec::with_capacity(bytes);
            for _ in 0..bytes / 4 {
                face.extend_from_slice(&colour);
            }
            face
        })
    }

    /// All-zero faces with one bright texel on face 0.
    fn cube_with_bright_texel(edge: u32, col: u32, row: u32) -> [Vec<u8>; 6] {
        let mut faces = solid_faces(edge, [0, 0, 0, 255]);
        let stride = usize::try_from(edge).unwrap() * 4;
        let at = usize::try_from(row).unwrap() * stride + usize::try_from(col).unwrap() * 4;
        faces[0][at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
        faces
    }

    /// The number of texels with any non-zero RGB channel in a chain.
    fn lit_texels(chain: &[[Vec<u8>; 6]], level: usize) -> usize {
        chain[level]
            .iter()
            .map(|face| {
                face.as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|px| px[..3].iter().any(|&b| b != 0))
                    .count()
            })
            .sum()
    }

    #[test]
    fn mip_levels_follow_the_face_edge() {
        assert_eq!(mip_levels_for(0), 1);
        assert_eq!(mip_levels_for(1), 1);
        assert_eq!(mip_levels_for(2), 2);
        assert_eq!(mip_levels_for(8), 4);
        assert_eq!(mip_levels_for(48), 6);
        assert_eq!(mip_levels_for(64), 7);
        // The cap holds for an oversized edge.
        assert_eq!(mip_levels_for(4096), MAX_PROBE_MIPS);
    }

    #[test]
    fn a_constant_cube_prefilters_to_a_constant_chain() {
        let colour = [37, 128, 200, 255];
        let faces = solid_faces(8, colour);
        let chain = prefilter_cube(8, &faces, mip_levels_for(8)).expect("chain");
        assert_eq!(chain.len(), 4);
        for (level, mip_faces) in chain.iter().enumerate() {
            for face in mip_faces {
                assert!(
                    face.as_chunks::<4>().0.iter().all(|px| px == &colour),
                    "level {level} must preserve the constant colour"
                );
            }
        }
    }

    #[test]
    fn a_bright_texel_spreads_over_later_levels() {
        let faces = cube_with_bright_texel(16, 8, 8);
        let chain = prefilter_cube(16, &faces, mip_levels_for(16)).expect("chain");
        assert_eq!(chain.len(), 5);
        assert_eq!(lit_texels(&chain, 0), 1, "the capture has one bright texel");
        let spread = lit_texels(&chain, 1);
        assert!(spread > 1, "the cone must spill into neighbouring texels");
        let last = &chain[4];
        let value = last[0][0];
        assert!(value > 0, "the bright texel must reach the coarsest level");
        assert!(
            value < 255,
            "the average must be weighted down by the black room"
        );
    }

    #[test]
    fn the_chain_edges_halve_to_one_texel() {
        let edge = 16;
        let faces = solid_faces(edge, [10, 20, 30, 255]);
        let chain = prefilter_cube(edge, &faces, mip_levels_for(edge)).expect("chain");
        let mut expected = edge;
        for (level, mip_faces) in chain.iter().enumerate() {
            let bytes = usize::try_from(expected).unwrap() * usize::try_from(expected).unwrap() * 4;
            for face in mip_faces {
                assert_eq!(face.len(), bytes, "level {level} edge {expected}");
            }
            expected /= 2;
        }
        assert_eq!(expected, 0, "the chain ends at one texel");
    }

    #[test]
    fn two_prefilters_of_the_same_cube_are_byte_identical() {
        let edge = 8;
        let mut faces = solid_faces(edge, [0, 0, 0, 255]);
        for (face_index, face) in faces.iter_mut().enumerate() {
            for (texel, pixel) in face.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[
                    u8::try_from(texel.wrapping_mul(31).wrapping_add(face_index * 7) % 256)
                        .unwrap(),
                    u8::try_from(texel.wrapping_mul(17).wrapping_add(face_index * 13) % 256)
                        .unwrap(),
                    u8::try_from(texel.wrapping_mul(53).wrapping_add(face_index * 3) % 256)
                        .unwrap(),
                    255,
                ]);
            }
        }
        let first = prefilter_cube(edge, &faces, mip_levels_for(edge)).expect("first");
        let second = prefilter_cube(edge, &faces, mip_levels_for(edge)).expect("second");
        assert_eq!(first, second, "the filter must be deterministic");
    }

    #[test]
    fn hdr_filter_preserves_energy_above_one_and_is_repeatable() {
        let pixel: Vec<u8> = [4.0_f32, 2.0, 0.5, 1.0]
            .into_iter()
            .flat_map(|value| crate::package::ktx2::f32_to_f16_bits(value).to_le_bytes())
            .collect();
        let faces = std::array::from_fn(|_| pixel.repeat(64));
        let first = prefilter_hdr_cube(8, &faces, 4).expect("HDR chain");
        assert_eq!(first, prefilter_hdr_cube(8, &faces, 4).expect("repeat"));
        for level in first {
            for face in level {
                for texel in face.as_chunks::<8>().0 {
                    for (bytes, expected) in texel
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .zip([4.0_f32, 2.0, 0.5, 1.0])
                    {
                        let actual =
                            crate::package::ktx2::f16_bits_to_f32(u16::from_le_bytes(*bytes));
                        assert!((actual - expected).abs() < 0.01);
                    }
                }
            }
        }
    }

    #[test]
    fn an_odd_face_or_level_shape_is_refused() {
        let faces = solid_faces(8, [1, 2, 3, 255]);
        assert!(prefilter_cube(0, &faces, 1).is_err(), "zero edge");
        assert!(prefilter_cube(8, &faces, 0).is_err(), "zero levels");
        assert!(
            prefilter_cube(8, &faces, mip_levels_for(8) + 1).is_err(),
            "more levels than the edge supports"
        );
        let mut short = faces;
        let _removed_value = short[3].pop();
        assert!(prefilter_cube(8, &short, 1).is_err(), "short face");
    }

    #[test]
    fn the_face_convention_maps_each_direction_to_its_own_face() {
        for (face, direction) in CUBE_FACE_DIRECTIONS.iter().enumerate() {
            let (mapped, s, t) = face_uv(Vec3::from_array(*direction));
            assert_eq!(mapped, face, "a face direction selects its own face");
            assert!(
                s.abs() < 1.0e-6 && t.abs() < 1.0e-6,
                "the centre maps to (0, 0)"
            );
        }
    }
}

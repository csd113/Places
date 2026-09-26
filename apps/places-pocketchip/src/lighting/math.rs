//! Pure lighting maths: no allocation, no state, every input sanitised.
//!
//! Each function is a bounded, continuous mapping from authored numbers to
//! brightness, so a malformed level can never produce NaN, an out-of-range
//! colour or a division by zero.

use super::color::LightColor;
use super::tuning::{
    AMBIENT_LEVEL, BASELINE_MAX, FixtureKind, LIGHT_GRID_CELL_M, MAX_LIGHT_GRID_CELLS,
    MAX_LIGHT_INTENSITY, MAX_WALL_LIGHT_SEGMENTS, MIN_ROOM_AREA_M2, REFERENCE_CEILING_HEIGHT_M,
    REFERENCE_LIGHT_AREA_M2, fixture_profile_for_kind,
};

/// Sanitises an authored fixture intensity for baking.
///
/// Non-finite values fall back to the standard fixture, negatives clamp to
/// zero output and absurd values clamp to [`MAX_LIGHT_INTENSITY`]; the result is
/// always finite and non-negative.
#[must_use]
pub fn sanitize_intensity(intensity: f32) -> f32 {
    if intensity.is_nan() {
        return 1.0;
    }
    if intensity.is_infinite() {
        return if intensity > 0.0 {
            MAX_LIGHT_INTENSITY
        } else {
            0.0
        };
    }
    intensity.clamp(0.0, MAX_LIGHT_INTENSITY)
}

/// Gentle ceiling-height correction applied to one fixture's output.
///
/// A lower ceiling makes the same fixture more effective, a taller one less;
/// the correction is a bounded power law, never an inverse square.
#[must_use]
pub fn ceiling_height_factor(height_m: f32) -> f32 {
    if !height_m.is_finite() || height_m <= 0.0 {
        return 1.0;
    }
    let height = height_m.clamp(0.5, 100.0);
    (REFERENCE_CEILING_HEIGHT_M / height).sqrt()
}

/// Effective fixture power: authored intensity times the height correction.
#[must_use]
pub fn effective_power(intensity: f32, ceiling_height_m: f32) -> f32 {
    sanitize_intensity(intensity) * ceiling_height_factor(ceiling_height_m)
}

/// Whether a ceiling fixture's panel is turned 90 degrees from its default.
///
/// The canonical rule shared by baked lighting and fixture geometry: rounding
/// the authored rotation to the nearest whole degree and testing it against
/// 180 keeps a `90` panel turned, a `180` panel back to default, and fractional
/// rotations identical in both places. The level editor mirrors this rule.
#[must_use]
pub const fn fixture_is_turned(rotation_degrees: f32) -> bool {
    if !rotation_degrees.is_finite() {
        return false;
    }
    // `round` yields an integral `f32`, so no fractional part can be lost;
    // magnitudes past `i64::MAX` saturate exactly as this cast always did.
    #[allow(clippy::cast_possible_truncation)]
    let whole_degrees = rotation_degrees.round() as i64;
    whole_degrees.rem_euclid(180) != 0
}

/// Half-extents of a fixture's luminous panel in world X/Z after rotation.
///
/// Mirrors the panel geometry emitted by `crate::render`: the default 1.2 x 0.6
/// panel runs along X, and a turned fixture swaps its axes.
#[must_use]
pub const fn fixture_half_extents(rotation_degrees: f32) -> (f32, f32) {
    fixture_half_extents_for(FixtureKind::FluorescentPanel, rotation_degrees)
}

/// [`fixture_half_extents`] for any fixture family.
///
/// Delegates to the family's generic [`LightShape`], so the bake, the drawn
/// fixture geometry and the light source share one rotation rule and cannot
/// drift apart by a rounding step.
#[must_use]
pub const fn fixture_half_extents_for(kind: FixtureKind, rotation_degrees: f32) -> (f32, f32) {
    fixture_profile_for_kind(kind)
        .shape()
        .half_extents_rotated(rotation_degrees)
}

/// Smoothly saturating brightness component of a normalised light density.
///
/// `n / (1 + n)`: continuous, monotonic, zero at zero, asymptotically 1 as the
/// density grows, and numerically safe for every input (NaN maps to 0,
/// infinities map to 0 or 1).
#[must_use]
pub fn saturating_brightness(normalized_density: f32) -> f32 {
    if normalized_density.is_nan() {
        return 0.0;
    }
    if normalized_density.is_infinite() {
        return if normalized_density > 0.0 { 1.0 } else { 0.0 };
    }
    let n = normalized_density.max(0.0);
    n / (1.0 + n)
}

/// Smooth falloff curve shared by local pools and opening blends.
///
/// `(1 - t)^2 * (1 + 2t)` is `1 - smoothstep(t)`: it equals 1 at `t = 0`,
/// reaches 0 at `t = 1` and has a zero derivative at both ends, so lit regions
/// fade in and out without visible rings.
#[must_use]
pub fn smooth_falloff(t: f32) -> f32 {
    if t.is_nan() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;
    u * u * 2.0f32.mul_add(t, 1.0)
}

/// Logarithmic compression of a normalised fixture density.
///
/// The fixture density of a room (`power / area x REFERENCE_LIGHT_AREA_M2`) is
/// fed through `ln(1 + n)` before [`saturating_brightness`]. The logarithm is
/// what lets one clip cover the game's whole range: a sparse 13 m grid and a
/// dense closet grid differ by a factor of ~100 in raw density, but only by a
/// factor of ~4 after compression, so the sparse room is not dark and the
/// dense room is not blown out. Zero density stays exactly zero and the curve
/// is continuous, monotonic and safe for every input.
#[must_use]
pub fn compressed_density(normalized_density: f32) -> f32 {
    if normalized_density.is_nan() {
        return 0.0;
    }
    if normalized_density <= 0.0 {
        return 0.0;
    }
    if normalized_density.is_infinite() {
        return f32::INFINITY;
    }
    normalized_density.ln_1p()
}

/// Baseline illumination of a room from its floor area and the summed emitted
/// colour of the fixtures it owns.
///
/// Each channel is treated as an independent scalar light: the effective power
/// of the channel is spread over the floor area, compressed logarithmically and
/// mapped onto `[AMBIENT_LEVEL, BASELINE_MAX]` by the saturating curve. The
/// baseline is the room-wide fill, not the whole range: the remaining headroom
/// is what local fixture pools (and therefore every shadow they cast) live in;
/// see [`BASELINE_MAX`]. A room with no fixtures returns exactly
/// [`ambient_color`]; the result is always finite and inside
/// `[AMBIENT_LEVEL, BASELINE_MAX]`.
#[must_use]
pub fn room_baseline(area_m2: f32, effective_power: LightColor) -> LightColor {
    let area = if area_m2.is_finite() {
        area_m2.max(MIN_ROOM_AREA_M2)
    } else {
        MIN_ROOM_AREA_M2
    };
    let channel = |power: f32| -> f32 {
        // An undefined power emits nothing; +infinity means "as bright as the
        // curve allows" rather than an error, matching the old scalar contract.
        let power = if power.is_finite() {
            power.max(0.0)
        } else if power > 0.0 {
            f32::INFINITY
        } else {
            0.0
        };
        let density = power / area;
        let normalized = compressed_density(density * REFERENCE_LIGHT_AREA_M2);
        let component = saturating_brightness(normalized);
        (BASELINE_MAX - AMBIENT_LEVEL)
            .mul_add(component, AMBIENT_LEVEL)
            .clamp(AMBIENT_LEVEL, BASELINE_MAX)
    };
    LightColor {
        r: channel(effective_power.r),
        g: channel(effective_power.g),
        b: channel(effective_power.b),
    }
}

/// Number of baked-lighting grid cells along one surface axis of `extent_m`.
///
/// Always at least 1 and never more than [`MAX_LIGHT_GRID_CELLS`], so floor and
/// ceiling geometry is bounded no matter how large a room is.
#[must_use]
pub fn light_grid_cells(extent_m: f32) -> u32 {
    if !extent_m.is_finite() || extent_m <= 0.0 {
        return 1;
    }
    // `extent_m` is finite and positive, so the ceiling is a finite
    // non-negative integral value; the cast saturates rather than wraps and
    // the clamp bounds the result to `1..=MAX_LIGHT_GRID_CELLS`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = ((extent_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_LIGHT_GRID_CELLS);
    cells
}

/// Number of segments one wall face is split into along its length, so baked
/// lighting can vary along long walls without unbounded geometry.
#[must_use]
pub fn wall_light_segments(length_m: f32) -> u32 {
    if !length_m.is_finite() || length_m <= 0.0 {
        return 1;
    }
    // `length_m` is finite and positive, so the ceiling is a finite
    // non-negative integral value; the cast saturates rather than wraps and
    // the clamp bounds the result to `1..=MAX_WALL_LIGHT_SEGMENTS`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let segments = ((length_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_WALL_LIGHT_SEGMENTS);
    segments
}

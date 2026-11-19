//! Pure lighting maths: no allocation, no state, every input sanitised.
//!
//! Each function is a bounded, continuous mapping from authored numbers to
//! brightness, so a malformed level can never produce NaN, an out-of-range
//! colour or a division by zero.

use super::color::LightColor;
use super::tuning::{
    AMBIENT_LEVEL, BASELINE_MAX, FixtureKind, LIGHT_GRID_CELL_M, MAX_LIGHT_GRID_CELLS,
    MAX_LIGHT_INTENSITY, MAX_WALL_LIGHT_SEGMENTS, MAX_ZONE_GRID_CELLS, MIN_ROOM_AREA_M2,
    REFERENCE_CEILING_HEIGHT_M, REFERENCE_LIGHT_AREA_M2, ZONE_GRID_CELL_M, ambient_color,
    fixture_profile_for_kind,
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
/// rotations identical in both places.
#[must_use]
pub const fn fixture_is_turned(rotation_degrees: f32) -> bool {
    if !rotation_degrees.is_finite() {
        return false;
    }
    // `round` yields an integral `f32`, so no fractional part can be lost;
    // magnitudes past `i64::MAX` saturate exactly as this cast always did.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "`round` yields an integral `f32`, so no fractional part can be lost; magnitudes past `i64::MAX` saturate exactly as this cast always did."
    )]
    let whole_degrees = rotation_degrees.round() as i64;
    whole_degrees.rem_euclid(180) != 0
}

/// Horizontal axes shared by the Static footprint and the Full emitter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum EmitterYaw {
    AlongX,
    AlongZ,
    Angled(glam::Mat2),
}

impl EmitterYaw {
    pub(super) const fn fixture(degrees: f32) -> Self {
        if fixture_is_turned(degrees) {
            Self::AlongZ
        } else {
            Self::AlongX
        }
    }

    pub(super) fn attached(degrees: f32) -> Self {
        // Both finite shapes are symmetric through 180 degrees. Keep the
        // original world-axis tap order at exact right angles.
        match degrees % 180.0 {
            0.0 => Self::AlongX,
            90.0 | -90.0 => Self::AlongZ,
            // Positive yaw about world Y rotates local +X towards world -Z.
            yaw => Self::Angled(glam::Mat2::from_angle(-yaw.to_radians())),
        }
    }

    pub(super) fn line_direction(self) -> [f32; 3] {
        match self {
            Self::AlongX => [1.0, 0.0, 0.0],
            Self::AlongZ => [0.0, 0.0, 1.0],
            Self::Angled(basis) => [basis.x_axis.x, 0.0, basis.x_axis.y],
        }
    }

    pub(super) const fn footprint(self, (half_width, half_depth): (f32, f32)) -> EmitterFootprint {
        let (half_extents, basis) = match self {
            Self::AlongX => (glam::Vec2::new(half_width, half_depth), None),
            Self::AlongZ => (glam::Vec2::new(half_depth, half_width), None),
            Self::Angled(basis) => (glam::Vec2::new(half_width, half_depth), Some(basis)),
        };
        EmitterFootprint {
            half_extents,
            basis,
        }
    }
}

/// Static treats a line as a thin rectangle. Its nearest point and shadow taps
/// lie in the actual emitter plane, while spatial indices use the enclosing AABB.
#[derive(Clone, Copy)]
pub(super) struct EmitterFootprint {
    half_extents: glam::Vec2,
    basis: Option<glam::Mat2>,
}

impl EmitterFootprint {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The unit axes scale finite float extents."
    )]
    pub(super) fn half_axes(self) -> (glam::Vec2, glam::Vec2) {
        let basis = self.basis.unwrap_or(glam::Mat2::IDENTITY);
        (
            basis.x_axis * self.half_extents.x,
            basis.y_axis * self.half_extents.y,
        )
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Summing the bounded float projections encloses both half-axes."
    )]
    pub(super) fn world_half_extents(self) -> (f32, f32) {
        if self.basis.is_none() {
            return (self.half_extents.x, self.half_extents.y);
        }
        let (u, v) = self.half_axes();
        let bounds = u.abs() + v.abs();
        (bounds.x, bounds.y)
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Float vector projection and subtraction compute the distance to the footprint."
    )]
    pub(super) fn distance_squared(self, delta: glam::Vec2) -> f32 {
        let local = self.basis.map_or(delta, |basis| basis.transpose() * delta);
        let gap = (local.abs() - self.half_extents).max(glam::Vec2::ZERO);
        gap.y.mul_add(gap.y, gap.x * gap.x)
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Float vector projection and clamping compute the nearest point."
    )]
    pub(super) fn closest_point(self, [x, y, z]: [f32; 3], point: [f32; 3]) -> [f32; 3] {
        let extents = self.half_extents.max(glam::Vec2::ZERO);
        let centre = glam::Vec2::new(x, z);
        let point_xz = glam::Vec2::new(point[0], point[2]);
        let nearest = self.basis.map_or_else(
            || point_xz.clamp(centre - extents, centre + extents),
            |basis| {
                let local = basis.transpose() * (point_xz - centre);
                centre + basis * local.clamp(-extents, extents)
            },
        );
        [nearest.x, y, nearest.y]
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Float vector projection places bounded quadrature taps in the emitter plane."
    )]
    pub(super) fn tap(self, [x, y, z]: [f32; 3], offset: [f32; 2]) -> [f32; 3] {
        let extents = self.half_extents.max(glam::Vec2::ZERO);
        let tap_offset = glam::Vec2::from_array(offset);
        let centre = glam::Vec2::new(x, z);
        let origin = self.basis.map_or_else(
            || tap_offset.mul_add(extents, centre),
            |basis| centre + basis * (tap_offset * extents),
        );
        [origin.x, y, origin.y]
    }
}

/// Half-extents of a fixture's outer body in world X/Z after rotation.
///
/// The default 1.2 x 0.6 m body runs along X, and a turned fixture swaps its
/// axes. Source taps use the profile's separate luminous aperture.
#[must_use]
pub const fn fixture_half_extents(rotation_degrees: f32) -> (f32, f32) {
    fixture_half_extents_for(FixtureKind::FluorescentPanel, rotation_degrees)
}

/// [`fixture_half_extents`] for any fixture family.
///
/// Delegates to the family's outer [`LightShape`], keeping mounting clearance
/// and drawn body bounds on the same rotation rule. The emitting shape uses
/// the same rotation rule with its own luminous bounds.
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
    let unit_t = t.clamp(0.0, 1.0);
    let u = 1.0 - unit_t;
    u * u * 2.0f32.mul_add(unit_t, 1.0)
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
/// The strongest channel's effective power is spread over the floor area and
/// compressed logarithmically by the saturating curve. One shared amplitude
/// then scales the summed RGB power, preserving its hue above the neutral
/// ambient floor and retaining the exact white-light calibration. The
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
    // An undefined power emits nothing; +infinity saturates its channel.
    let powers = effective_power.to_array().map(|power| {
        if power.is_finite() {
            power.max(0.0)
        } else if power > 0.0 {
            f32::INFINITY
        } else {
            0.0
        }
    });
    let maximum = powers.into_iter().fold(0.0_f32, f32::max);
    if maximum <= 0.0 {
        return ambient_color();
    }
    let density = maximum / area;
    let normalized = compressed_density(density * REFERENCE_LIGHT_AREA_M2);
    let component = saturating_brightness(normalized);
    LightColor::from(powers.map(|power| {
        let ratio = if maximum.is_infinite() {
            if power.is_infinite() { 1.0 } else { 0.0 }
        } else {
            power / maximum
        };
        (BASELINE_MAX - AMBIENT_LEVEL)
            .mul_add(component * ratio, AMBIENT_LEVEL)
            .clamp(AMBIENT_LEVEL, BASELINE_MAX)
    }))
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
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`extent_m` is finite and positive, so the ceiling is a finite non-negative integral value; the cast saturates rather than wraps and the clamp bounds the result to `1..=MAX_LIGHT_GRID_CELLS`."
    )]
    let cells = ((extent_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_LIGHT_GRID_CELLS);
    cells
}

/// Number of baseline-zone grid cells along one room axis of `extent_m`.
///
/// The zone grid is a lookup structure, not geometry: it must be fine enough
/// that a narrow air region owns a cell centre, so it uses its own finer cell
/// size and a larger cap than the light grid. Always at least 1 and never more
/// than [`MAX_ZONE_GRID_CELLS`].
#[must_use]
pub fn zone_grid_cells(extent_m: f32) -> u32 {
    if !extent_m.is_finite() || extent_m <= 0.0 {
        return 1;
    }
    // `extent_m` is finite and positive, so the ceiling is a finite
    // non-negative integral value; the cast saturates rather than wraps and
    // the clamp bounds the result to `1..=MAX_ZONE_GRID_CELLS`.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`extent_m` is finite and positive, so the ceiling is a finite non-negative integral value; the cast saturates rather than wraps and the clamp bounds the result to `1..=MAX_ZONE_GRID_CELLS`."
    )]
    let cells = ((extent_m / ZONE_GRID_CELL_M).ceil() as u32).clamp(1, MAX_ZONE_GRID_CELLS);
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
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`length_m` is finite and positive, so the ceiling is a finite non-negative integral value; the cast saturates rather than wraps and the clamp bounds the result to `1..=MAX_WALL_LIGHT_SEGMENTS`."
    )]
    let segments = ((length_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_WALL_LIGHT_SEGMENTS);
    segments
}

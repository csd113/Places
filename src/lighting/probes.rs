//! The prepared irradiance field moving objects sample at runtime.
//!
//! A level's static surfaces carry their light in the lightmap atlas; a moving
//! object cannot sample a texture that is only correct for the point it was
//! baked at, so the compiler also solves a coarse 3D field of irradiance probes
//! over the mapped world. The player interpolates that prepared field at an
//! object's position, which is a handful of loads — no bake, no ray, no
//! visibility query.
//!
//! Each probe stores the same compact linear HDR values a lightmap texel
//! stores (an irradiance mean, the vector sum of the per-channel first moments
//! and the reserved axis pair), plus the room it belongs to. Interpolation only
//! mixes probes of the sample's own room, avoiding cross-room interpolation.
//! Internal dividers within one room need separate consumer handling. A sample whose
//! neighbourhood has no probe of its room falls back to a bounded nearest
//! neighbor of that room, then returns `None` for the caller's fallback policy.
//! A valid zero-valued probe represents deliberate darkness.
//!
//! Probes are baked from the same transport solve as the lightmap atlas: air
//! points receive every visible emitter's direct contribution plus one
//! ray-traced diffuse gather against the solved static surfaces, so a room's
//! brightness, colour and directional character reach a character exactly as
//! they reach the walls.

// The probe field is a numeric grid: explicit `f32` arithmetic keeps bakes
// reproducible, lattice indices are bounded before conversion, and arrays are
// indexed by constants. Those lint shapes are allowed for the module as a
// unit.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    clippy::suboptimal_flops,
    clippy::too_many_lines
)]

use crate::lighting::lightmap::LightmapTexel;

/// Magic of the compiled probe-field record.
pub const PROBE_FIELD_MAGIC: [u8; 4] = *b"PLPF";

/// Version of the compiled probe-field record.
///
/// * `1` — the dominant-axis encoding (a per-channel amplitude plus an
///   octahedral axis).
/// * `2` — the linear moment representation: `direction` is the vector sum of
///   the per-channel first moments and the axis pair is reserved. A version-1
///   field is rejected.
pub const PROBE_FIELD_RECORD_VERSION: u16 = 2;

/// Preferred probe spacing, in metres.
pub const PROBE_SPACING_M: f32 = 1.5;

/// Cap on probe cells per axis.
pub const MAX_PROBE_CELLS: usize = 64;

/// Cap on the total probe count.
pub const MAX_PROBES: usize = 1 << 18;

/// Largest accepted serialized probe field, in bytes.
pub const MAX_PROBE_FIELD_BYTES: u64 = 256 * 1024 * 1024;

/// One prepared irradiance probe.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProbeSample {
    /// Isotropic irradiance mean, linear HDR.
    pub irradiance: [f32; 3],
    /// Vector sum of the per-channel first moments, linear HDR, signed.
    pub direction: [f32; 3],
    /// Reserved: writers store `[0.5, 0.5]`, consumers ignore it.
    pub axis: [f32; 2],
    /// Owning room index, or `-1` for a probe in no room (never sampled).
    pub room: i32,
}

impl ProbeSample {
    /// True when this probe belongs to a room and can be sampled.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.room >= 0
    }

    /// The probe's stored values as a lightmap texel.
    #[must_use]
    pub fn texel(self) -> LightmapTexel {
        LightmapTexel {
            irradiance: self.irradiance,
            direction: self.direction,
            axis: self.axis,
        }
    }
}

/// One prepared probe field: a uniform grid over the mapped world.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeField {
    /// World position of the grid's lower boundary. Probe centres are
    /// `min + (index + 0.5) * cell_m`, in metres.
    pub min: [f32; 3],
    /// Cell edge, in metres.
    pub cell_m: f32,
    /// Grid dimensions in probes.
    pub dims: [u32; 3],
    /// Probes, `x` fastest then `y` then `z`.
    pub probes: Vec<ProbeSample>,
}

impl ProbeField {
    /// Probes per axis as `usize`.
    #[must_use]
    pub fn dims_usize(&self) -> [usize; 3] {
        [
            usize::try_from(self.dims[0]).unwrap_or(0),
            usize::try_from(self.dims[1]).unwrap_or(0),
            usize::try_from(self.dims[2]).unwrap_or(0),
        ]
    }

    /// True when the buffer matches the declared dimensions.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        let dims = self.dims_usize();
        dims.iter().all(|value| *value > 0)
            && dims
                .iter()
                .try_fold(1usize, |total, value| total.checked_mul(*value))
                .is_some_and(|expected| self.probes.len() == expected)
    }

    /// Counts and world extent, for the developer report.
    #[must_use]
    pub fn summary(&self) -> (usize, usize) {
        let valid = self.probes.iter().filter(|probe| probe.is_valid()).count();
        (self.probes.len(), valid)
    }

    /// Labels every probe with the room it occupies.
    ///
    /// `room_of` returns the room containing a world position, or `None` for a
    /// position in no room or inside solid geometry. An unlabelled probe is
    /// never sampled, so a probe baked in a wall or outside the level cannot
    /// leak its values to a moving object.
    pub fn assign_rooms<F>(&mut self, room_of: F)
    where
        F: Fn([f32; 3]) -> Option<usize>,
    {
        if !self.is_consistent() {
            return;
        }
        let dims = self.dims_usize();
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let Some(index) = z
                        .checked_mul(dims[1])
                        .and_then(|value| value.checked_add(y))
                        .and_then(|value| value.checked_mul(dims[0]))
                        .and_then(|value| value.checked_add(x))
                    else {
                        continue;
                    };
                    let position = [
                        self.min[0] + (x as f32 + 0.5) * self.cell_m,
                        self.min[1] + (y as f32 + 0.5) * self.cell_m,
                        self.min[2] + (z as f32 + 0.5) * self.cell_m,
                    ];
                    if let Some(probe) = self.probes.get_mut(index) {
                        probe.room = match room_of(position) {
                            Some(room) => i32::try_from(room).unwrap_or(i32::MAX),
                            None => -1,
                        };
                    }
                }
            }
        }
    }

    /// The probe at a lattice position, if inside the grid.
    #[must_use]
    pub fn probe(&self, x: usize, y: usize, z: usize) -> Option<ProbeSample> {
        let dims = self.dims_usize();
        if x >= dims[0] || y >= dims[1] || z >= dims[2] {
            return None;
        }
        let index = z
            .checked_mul(dims[1])?
            .checked_add(y)?
            .checked_mul(dims[0])?
            .checked_add(x)?;
        self.probes.get(index).copied()
    }

    /// The interpolated field at a world position, restricted to `room`.
    ///
    /// Returns `None` when the position cannot be resolved to any probe of the
    /// requested room: the caller then falls back to its own light sampling, so
    /// an unresolvable position degrades instead of going black.
    #[must_use]
    pub fn sample(&self, position: [f32; 3], room: Option<usize>) -> Option<LightmapTexel> {
        if !self.is_consistent() || self.cell_m <= 0.0 || !position.iter().all(|v| v.is_finite()) {
            return None;
        }
        let room = room.map(|room| i32::try_from(room).unwrap_or(i32::MAX));
        let coords = [
            (position[0] - self.min[0]) / self.cell_m - 0.5,
            (position[1] - self.min[1]) / self.cell_m - 0.5,
            (position[2] - self.min[2]) / self.cell_m - 0.5,
        ];
        let base = [coords[0].floor(), coords[1].floor(), coords[2].floor()];
        let frac = [
            (coords[0] - base[0]).clamp(0.0, 1.0),
            (coords[1] - base[1]).clamp(0.0, 1.0),
            (coords[2] - base[2]).clamp(0.0, 1.0),
        ];
        let mut total = [0.0_f32; 3];
        let mut moment = [0.0_f32; 3];
        let mut weight_sum = 0.0_f32;
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let weight = (if dx == 0 { 1.0 - frac[0] } else { frac[0] })
                        * (if dy == 0 { 1.0 - frac[1] } else { frac[1] })
                        * (if dz == 0 { 1.0 - frac[2] } else { frac[2] });
                    if weight <= 0.0 {
                        continue;
                    }
                    let Some(probe) = lattice_probe(
                        self,
                        base[0] + dx as f32,
                        base[1] + dy as f32,
                        base[2] + dz as f32,
                    ) else {
                        continue;
                    };
                    if !probe.is_valid() {
                        continue;
                    }
                    if let Some(room) = room
                        && probe.room != room
                    {
                        continue;
                    }
                    weight_sum += weight;
                    for channel in 0..3 {
                        if let (Some(slot), Some(value)) =
                            (total.get_mut(channel), probe.irradiance.get(channel))
                        {
                            *slot += weight * value;
                        }
                        if let (Some(slot), Some(value)) =
                            (moment.get_mut(channel), probe.direction.get(channel))
                        {
                            *slot += weight * value;
                        }
                    }
                }
            }
        }
        if weight_sum > 0.0 {
            let inverse = 1.0 / weight_sum;
            for channel in 0..3 {
                if let Some(slot) = total.get_mut(channel) {
                    *slot *= inverse;
                }
                if let Some(slot) = moment.get_mut(channel) {
                    *slot *= inverse;
                }
            }
            return Some(LightmapTexel {
                irradiance: total,
                direction: moment,
                // The axis is reserved; the moment is the whole directional
                // payload now.
                axis: [0.5, 0.5],
            });
        }
        // Bounded fallback: the nearest valid probe of the requested room
        // within a two-cell neighbourhood, then nothing (the caller's own
        // sample takes over).
        let mut best: Option<(f32, ProbeSample)> = None;
        for dz in -2_isize..=2 {
            for dy in -2_isize..=2 {
                for dx in -2_isize..=2 {
                    let Some(probe) = lattice_probe(
                        self,
                        base[0] + dx as f32,
                        base[1] + dy as f32,
                        base[2] + dz as f32,
                    ) else {
                        continue;
                    };
                    if !probe.is_valid() {
                        continue;
                    }
                    if let Some(room) = room
                        && probe.room != room
                    {
                        continue;
                    }
                    let distance = dx.abs() + dy.abs() + dz.abs();
                    let distance = f32::from(u16::try_from(distance).unwrap_or(u16::MAX));
                    if best.is_none_or(|(current, _)| distance < current) {
                        best = Some((distance, probe));
                    }
                }
            }
        }
        best.map(|(_, probe)| probe.texel())
    }

    /// The display-space light a moving object reads at a world position.
    ///
    /// The moving-object path has no surface normal, so it reads the stored
    /// isotropic term — the orientation-free component the static shader
    /// reconstructs around. The value goes through the same tone map the
    /// static shader applies; this is the value the object's per-instance
    /// light uniform carries. (The full reconstruction's sphere mean is
    /// `I * (1 - |g| / (2k))`; the dynamic path deliberately uses the
    /// isotropic term, exactly as it did before the moment representation.)
    #[must_use]
    pub fn sample_display(&self, position: [f32; 3], room: Option<usize>) -> Option<[f32; 3]> {
        let texel = self.sample(position, room)?;
        let display = crate::lighting::transport::soft_clip(texel.irradiance);
        if display.iter().all(|value| value.is_finite()) {
            Some(display)
        } else {
            None
        }
    }

    /// The unit direction of the field's accumulated first moment at a
    /// position.
    ///
    /// The stored `direction` is the vector sum of the per-channel first
    /// moments; its normalised direction is the single axis that best explains
    /// the field, the quantity the historical octahedral axis carried. A
    /// field with no net moment returns `None`.
    #[must_use]
    pub fn sample_direction(&self, position: [f32; 3], room: Option<usize>) -> Option<[f32; 3]> {
        let texel = self.sample(position, room)?;
        let length = texel.direction[2]
            .mul_add(
                texel.direction[2],
                texel.direction[1]
                    .mul_add(texel.direction[1], texel.direction[0] * texel.direction[0]),
            )
            .sqrt();
        if !length.is_finite() || length <= 1.0e-9 {
            return None;
        }
        Some([
            texel.direction[0] / length,
            texel.direction[1] / length,
            texel.direction[2] / length,
        ])
    }

    /// Encodes the field as the binary record the package carries.
    ///
    /// # Errors
    ///
    /// Returns an error for an inconsistent grid, invalid numeric data,
    /// an out-of-range dimension or a field beyond [`MAX_PROBE_FIELD_BYTES`].
    pub fn write(&self) -> Result<Vec<u8>, String> {
        if !self.is_consistent() {
            return Err("probe field dimensions do not match its probe count".to_string());
        }
        for (axis, value) in self.dims.iter().enumerate() {
            if *value == 0 || usize::try_from(*value).unwrap_or(usize::MAX) > MAX_PROBE_CELLS {
                return Err(format!("probe field dimension {axis} is out of range"));
            }
        }
        if !self.min.iter().all(|value| value.is_finite()) {
            return Err("probe field origin is not finite".to_string());
        }
        if !self.cell_m.is_finite() || self.cell_m <= 0.0 {
            return Err("probe field cell size is out of range".to_string());
        }
        for (origin, count) in self.min.iter().zip(self.dims) {
            if !(origin + self.cell_m * count as f32).is_finite() {
                return Err("probe field world extent is not finite".to_string());
            }
        }
        for (index, probe) in self.probes.iter().enumerate() {
            probe
                .validate()
                .map_err(|error| format!("probe {index}: {error}"))?;
        }
        let count = self.probes.len();
        if count > MAX_PROBES {
            return Err(format!(
                "probe field holds {count} probes (limit {MAX_PROBES})"
            ));
        }
        let bytes = 4_usize
            .saturating_add(2)
            .saturating_add(12)
            .saturating_add(4)
            .saturating_add(12)
            .saturating_add(4)
            .saturating_add(count.saturating_mul(36));
        if u64::try_from(bytes).unwrap_or(u64::MAX) > MAX_PROBE_FIELD_BYTES {
            return Err(format!(
                "probe field is {bytes} bytes (limit {MAX_PROBE_FIELD_BYTES})"
            ));
        }
        let mut out = Vec::with_capacity(bytes);
        out.extend_from_slice(&PROBE_FIELD_MAGIC);
        out.extend_from_slice(&PROBE_FIELD_RECORD_VERSION.to_le_bytes());
        for value in self.min {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.cell_m.to_le_bytes());
        for value in self.dims {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(
            &u32::try_from(count)
                .map_err(|_| "probe count is too large".to_string())?
                .to_le_bytes(),
        );
        for probe in &self.probes {
            for value in probe.irradiance {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in probe.direction {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in probe.axis {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&probe.room.to_le_bytes());
        }
        Ok(out)
    }

    /// Decodes a binary field record.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong magic or version, a malformed or
    /// out-of-range header, a wrong declared length, non-finite values, an
    /// out-of-range room, or trailing bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(4)? != &PROBE_FIELD_MAGIC[..] {
            return Err("probe field is not a PLPF record".to_string());
        }
        let version = cursor.u16()?;
        if version != PROBE_FIELD_RECORD_VERSION {
            return Err(format!(
                "probe field version {version} is not supported (this build reads {PROBE_FIELD_RECORD_VERSION})"
            ));
        }
        let min = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
        let cell_m = cursor.f32()?;
        let dims = [cursor.u32()?, cursor.u32()?, cursor.u32()?];
        let count =
            usize::try_from(cursor.u32()?).map_err(|_| "probe count is too large".to_string())?;
        if !min.iter().all(|value| value.is_finite()) {
            return Err("probe field origin is not finite".to_string());
        }
        if !cell_m.is_finite() || cell_m <= 0.0 {
            return Err("probe field cell size is out of range".to_string());
        }
        if dims.iter().any(|value| {
            *value == 0 || usize::try_from(*value).unwrap_or(usize::MAX) > MAX_PROBE_CELLS
        }) {
            return Err("probe field dimension is out of range".to_string());
        }
        let expected = dims
            .iter()
            .try_fold(1usize, |total, value| {
                total.checked_mul(usize::try_from(*value).unwrap_or(usize::MAX))
            })
            .ok_or_else(|| "probe field dimensions overflow".to_string())?;
        if count != expected || count > MAX_PROBES {
            return Err("probe field probe count does not match its dimensions".to_string());
        }
        let payload_bytes = count
            .checked_mul(36)
            .ok_or_else(|| "probe field size overflows".to_string())?;
        if cursor.remaining() != payload_bytes {
            return Err("probe field length does not match its probe count".to_string());
        }
        let mut probes = Vec::with_capacity(count);
        for _ in 0..count {
            let irradiance = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let direction = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let axis = [cursor.f32()?, cursor.f32()?];
            let room = cursor.i32()?;
            let probe = ProbeSample {
                irradiance,
                direction,
                axis,
                room,
            };
            probe.validate()?;
            probes.push(probe);
        }
        if !cursor.is_at_end() {
            return Err(format!(
                "probe field has {} trailing byte(s)",
                cursor.remaining()
            ));
        }
        for (origin, count) in min.iter().zip(dims) {
            if !(origin + cell_m * count as f32).is_finite() {
                return Err("probe field world extent is not finite".to_string());
            }
        }
        Ok(Self {
            min,
            cell_m,
            dims,
            probes,
        })
    }
}

impl ProbeSample {
    /// Validates a record without silently repairing or normalizing its energy.
    ///
    /// # Errors
    /// Returns an error for non-finite or negative energy, an unbounded moment,
    /// an invalid reserved axis or an out-of-range room label.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self
            .irradiance
            .iter()
            .chain(&self.direction)
            .chain(&self.axis)
            .all(|value| value.is_finite())
        {
            return Err("probe field holds a non-finite value".to_string());
        }
        if self.irradiance.iter().any(|value| *value < 0.0) {
            return Err("probe field holds negative irradiance".to_string());
        }
        // Use f64 for the invariant: squaring valid HDR f32 components must
        // not overflow before checking |moment| <= sum RGB means.
        let mean: f64 = self.irradiance.iter().map(|value| f64::from(*value)).sum();
        let moment = self
            .direction
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            .sqrt();
        if moment > mean * (1.0 + 32.0 * f64::from(f32::EPSILON)) {
            return Err("probe field moment exceeds its irradiance energy".to_string());
        }
        if self.axis.iter().any(|value| !(0.0..=1.0).contains(value)) {
            return Err("probe field reserved axis is out of range".to_string());
        }
        if self.room < -1 || self.room > i32::from(i16::MAX) {
            return Err(format!("probe field room {} is out of range", self.room));
        }
        Ok(())
    }

    /// The probe with non-finite values zeroed, the irradiance clamped
    /// non-negative, the signed moment kept with its sign, and the reserved
    /// axis folded into `0..=1`.
    #[must_use]
    pub fn normalized(self) -> Self {
        let mut out = Self {
            room: self.room,
            ..Self::default()
        };
        for channel in 0..3 {
            let irradiance = self.irradiance.get(channel).copied().unwrap_or(0.0);
            let direction = self.direction.get(channel).copied().unwrap_or(0.0);
            out.irradiance[channel] = if irradiance.is_finite() {
                irradiance.max(0.0)
            } else {
                0.0
            };
            out.direction[channel] = if direction.is_finite() {
                direction
            } else {
                0.0
            };
        }
        for (index, value) in self.axis.iter().enumerate() {
            let value = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.5
            };
            out.axis[index] = value;
        }
        out
    }
}

/// The probe at a floating-point lattice position, if inside the grid.
fn lattice_probe(field: &ProbeField, x: f32, y: f32, z: f32) -> Option<ProbeSample> {
    let dims = field.dims_usize();
    let mut lattice = [0usize; 3];
    for (axis, value) in [x, y, z].into_iter().enumerate() {
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // `value` is finite and non-negative; the range check below rejects
        // anything that would truncate past the grid.
        let cell = value as usize;
        if cell >= dims.get(axis).copied().unwrap_or(0) {
            return None;
        }
        lattice[axis] = cell;
    }
    field.probe(lattice[0], lattice[1], lattice[2])
}

/// A bounds-checked little-endian cursor over a byte slice.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(length)
            .ok_or_else(|| "probe field range overflows".to_string())?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| "probe field is truncated".to_string())?;
        self.at = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| "probe field is truncated".to_string())?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "probe field is truncated".to_string())?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "probe field is truncated".to_string())?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    fn is_at_end(&self) -> bool {
        self.remaining() == 0
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwraps, indexing and permissive float comparison are
    // idiomatic here; production lints stay enforced above.
    #![allow(
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::*;

    fn field() -> ProbeField {
        ProbeField {
            min: [0.0, 0.0, 0.0],
            cell_m: 1.0,
            dims: [2, 1, 1],
            probes: vec![
                ProbeSample {
                    irradiance: [0.5, 0.25, 0.0],
                    direction: [0.25, 0.0, 0.0],
                    axis: [0.5, 1.0],
                    room: 3,
                },
                ProbeSample {
                    irradiance: [0.1; 3],
                    direction: [0.0; 3],
                    axis: [0.5, 0.5],
                    room: 3,
                },
            ],
        }
    }

    #[test]
    fn a_round_trip_preserves_every_probe_value() {
        let field = field();
        let bytes = field.write().expect("field writes");
        let decoded = ProbeField::read(&bytes).expect("field reads");
        assert_eq!(decoded, field);
        assert!(ProbeField::read(&bytes[..bytes.len() - 1]).is_err());
        assert!(ProbeField::read(b"nope").is_err());
    }

    /// The record version bumped with the linear moment representation, so a
    /// stale version-1 field must be rejected rather than decoded as if its
    /// `direction` were a dominant-lobe amplitude.
    #[test]
    fn a_version_1_probe_field_is_rejected() {
        let bytes = field().write().expect("field writes");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2);
        let mut stale = bytes;
        stale[4] = 1;
        stale[5] = 0;
        assert!(ProbeField::read(&stale).is_err());
    }

    #[test]
    fn interpolation_mixes_only_the_requested_room_and_falls_back() {
        let field = field();
        // Halfway between the two probe centres (0.5 and 1.5).
        let sample = field
            .sample([1.0, 0.0, 0.0], Some(3))
            .expect("room 3 samples");
        assert!((sample.irradiance[0] - 0.3).abs() < 1e-5);
        // A different room cannot read them...
        assert!(field.sample([1.0, 0.0, 0.0], Some(4)).is_none());
        // ...and outside the grid the bounded fallback still finds room 3
        // within two cells of the sample's lattice position.
        let fallback = field
            .sample([2.5, 0.0, 0.0], Some(3))
            .expect("fallback finds the room");
        assert!((fallback.irradiance[0] - 0.1).abs() < 1e-5);
        // Far outside the bounded fallback there is nothing to read, so the
        // caller falls back to its own light sampling.
        assert!(field.sample([9.0, 0.0, 0.0], Some(3)).is_none());
        // With no room constraint the nearest probe answers.
        assert!(field.sample([2.5, 0.0, 0.0], None).is_some());
    }

    #[test]
    fn the_display_and_direction_paths_follow_the_moment_representation() {
        let field = field();
        let position = [1.0, 0.0, 0.0];
        let sample = field.sample(position, Some(3)).expect("room 3 samples");
        assert_eq!(sample.axis, [0.5, 0.5], "the axis is reserved");
        // The display value is the sphere mean of the reconstruction, which is
        // exactly the isotropic term through the same tone map.
        let display = field
            .sample_display(position, Some(3))
            .expect("the display path samples");
        let expected = crate::lighting::transport::soft_clip(sample.irradiance);
        for channel in 0..3 {
            assert!(
                (display[channel] - expected[channel]).abs() < 1.0e-6,
                "display channel {channel}: {} vs {}",
                display[channel],
                expected[channel]
            );
        }
        // The direction is the normalised moment vector (not a decoded axis).
        let direction = field
            .sample_direction(position, Some(3))
            .expect("the direction path samples");
        assert!((direction[0] - 1.0).abs() < 1.0e-6, "{direction:?}");
        assert!(direction[1].abs() < 1.0e-6 && direction[2].abs() < 1.0e-6);
    }

    #[test]
    fn an_inconsistent_grid_is_rejected_by_the_codec() {
        let mut field = field();
        field.probes.pop();
        assert!(field.write().is_err());
        field.dims = [0, 1, 1];
        assert!(field.write().is_err());
    }
    #[test]
    fn nonfinite_headers_and_probe_values_cannot_be_written() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for component in 0..8 {
                let mut value = field();
                match component {
                    0..=2 => value.probes[0].irradiance[component] = invalid,
                    3..=5 => value.probes[0].direction[component - 3] = invalid,
                    _ => value.probes[0].axis[component - 6] = invalid,
                }
                assert!(value.write().is_err());
            }
            let mut value = field();
            value.min[0] = invalid;
            assert!(value.write().is_err());
            value.min[0] = 0.0;
            value.cell_m = invalid;
            assert!(value.write().is_err());
        }
    }

    #[test]
    fn malformed_energy_and_corrupted_records_are_rejected_without_repair() {
        let mut value = field();
        value.probes[0].irradiance[0] = -0.001;
        assert!(value.write().is_err());
        value = field();
        value.probes[0].direction = [10.0; 3];
        assert!(value.write().is_err());
        value = field();
        value.probes[0].room = -2;
        assert!(value.write().is_err());
        let bytes = field().write().expect("valid bytes");
        for invalid in [f32::NAN, f32::INFINITY, -0.01] {
            let mut corrupted = bytes.clone();
            corrupted[38..42].copy_from_slice(&invalid.to_le_bytes());
            assert!(ProbeField::read(&corrupted).is_err());
        }
    }

    #[test]
    fn linear_hdr_precision_and_signed_moments_survive_without_gamma_or_exposure() {
        let mut value = field();
        value.min = [-110.25, 32.0, 70.75];
        value.probes[0].irradiance = [16.0, 0.125, 0.000_01];
        value.probes[0].direction = [-3.0, 4.0, -2.0];
        let bytes = value.write().expect("linear HDR write");
        assert_eq!(bytes.len(), 38 + 36 * value.probes.len());
        assert_eq!(ProbeField::read(&bytes).expect("linear HDR read"), value);
        assert_eq!(value.write().expect("repeated write"), bytes);
    }
}

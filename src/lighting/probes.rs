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
//! and the reserved axis pair), plus the room it belongs to. Runtime blends
//! valid probes within two cells, restricted to the sample's room and existing
//! connected-area metadata. Missing samples request an explicit authored
//! environment fallback; valid dark samples are never brightened.
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
    pub fn is_valid(&self) -> bool {
        self.room >= 0 && self.values_valid()
    }

    /// Finite, non-negative energy and a physically bounded first moment.
    fn values_valid(&self) -> bool {
        let k: f64 = self.irradiance.iter().map(|v| f64::from(*v)).sum();
        let length = self
            .direction
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        self.irradiance.iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= 65_504.0)
            && self.direction.iter().all(|v| v.is_finite())
            && self.axis.iter().all(|v| v.is_finite())
            // Surface lighting uses half floats; exclude values that overflow
            // its representation or the shared reconstruction arithmetic.
            && length <= k.mul_add(1.000_01, 1.0e-6)
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

/// One contribution to an explicitly requested runtime lighting trace.
#[derive(Clone, Copy, Debug)]
pub struct ProbeContribution {
    pub id: usize,
    pub world_position: [f32; 3],
    pub distance_m: f64,
    pub weight: f64,
    pub probe: ProbeSample,
}

/// One prepared probe field: a uniform grid over the mapped world.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeField {
    /// World-space lower grid corner; probe centres add half a cell.
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
    /// Compact Shepard weights `(1 - distance / 2)^2 / distance^2`, in
    /// lattice units, blend valid probes within two cells. Weights vanish at
    /// the support boundary, including across sparse grid cells. An exact
    /// centre reads that probe alone. No probe of another room is used.
    /// `None` requests the caller's environment fallback.
    #[must_use]
    pub fn sample(&self, position: [f32; 3], room: Option<usize>) -> Option<LightmapTexel> {
        self.sample_filtered(position, room, |_| true)
    }

    /// Samples with the caller's existing region metadata, without changing
    /// the serialized field or relabelling compiler-owned room indices.
    #[must_use]
    pub fn sample_filtered<F>(
        &self,
        position: [f32; 3],
        room: Option<usize>,
        accept: F,
    ) -> Option<LightmapTexel>
    where
        F: Fn([f32; 3]) -> bool,
    {
        let mut energy = [0.0_f64; 3];
        let mut moment = [0.0_f64; 3];
        let sum = self.visit_candidates(position, room, accept, |_, _, probe, weight| {
            for channel in 0..3 {
                energy[channel] += weight * f64::from(probe.irradiance[channel]);
                moment[channel] += weight * f64::from(probe.direction[channel]);
            }
        })?;
        if sum <= 0.0 {
            return None;
        }
        // A normalized convex combination of validated f32 values fits f32.
        #[allow(clippy::cast_possible_truncation)]
        let texel = LightmapTexel {
            irradiance: energy.map(|v| (v / sum) as f32),
            direction: moment.map(|v| (v / sum) as f32),
            axis: [0.5; 2],
        };
        Some(texel)
    }

    /// Selected probe IDs, world positions, metre distances, normalized
    /// weights and original values. Allocates only when explicitly requested.
    #[must_use]
    pub fn sample_diagnostics(
        &self,
        position: [f32; 3],
        room: Option<usize>,
    ) -> Vec<ProbeContribution> {
        self.sample_diagnostics_filtered(position, room, |_| true)
    }

    /// The same filtered selection as [`Self::sample_filtered`], for reports.
    #[must_use]
    pub fn sample_diagnostics_filtered<F>(
        &self,
        position: [f32; 3],
        room: Option<usize>,
        accept: F,
    ) -> Vec<ProbeContribution>
    where
        F: Fn([f32; 3]) -> bool,
    {
        let mut candidates = Vec::new();
        let sum = self
            .visit_candidates(
                position,
                room,
                accept,
                |id, world_position, probe, weight| {
                    let distance_m = world_position
                        .iter()
                        .zip(position)
                        .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    candidates.push(ProbeContribution {
                        id,
                        world_position,
                        distance_m,
                        weight,
                        probe,
                    });
                },
            )
            .unwrap_or(0.0);
        for candidate in &mut candidates {
            candidate.weight /= sum;
        }
        candidates
    }

    fn visit_candidates<F, A>(
        &self,
        position: [f32; 3],
        room: Option<usize>,
        accept: A,
        mut visit: F,
    ) -> Option<f64>
    where
        A: Fn([f32; 3]) -> bool,
        F: FnMut(usize, [f32; 3], ProbeSample, f64),
    {
        if !self.is_consistent()
            || !self.cell_m.is_finite()
            || self.cell_m <= 0.0
            || !self
                .min
                .iter()
                .chain(position.iter())
                .all(|v| v.is_finite())
        {
            return None;
        }
        let room = room.map(i32::try_from).transpose().ok()?;
        let coords: [f64; 3] = std::array::from_fn(|axis| {
            (f64::from(position[axis]) - f64::from(self.min[axis])) / f64::from(self.cell_m) - 0.5
        });
        let dims = self.dims_usize();
        if coords
            .iter()
            .zip(dims)
            .any(|(v, dim)| *v < -2.0 || *v > dim as f64 + 1.0)
        {
            return None;
        }
        let mut ranges = [(0usize, 0usize); 3];
        for axis in 0..3 {
            // Coordinates are finite and bounded to the grid's small support.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let low = (coords[axis] - 2.0).ceil().max(0.0) as usize;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let high = (coords[axis] + 2.0).floor().max(0.0) as usize;
            ranges[axis] = (low, high.min(dims[axis].saturating_sub(1)));
        }
        // Exact lookup is checked first so no previously visited candidate
        // survives when the sample coincides with a valid probe centre.
        let mut exact = None;
        let mut sum = 0.0;
        for pass in 0..2 {
            for z in ranges[2].0..=ranges[2].1 {
                for y in ranges[1].0..=ranges[1].1 {
                    for x in ranges[0].0..=ranges[0].1 {
                        let probe = self.probe(x, y, z)?;
                        if !probe.is_valid() || room.is_some_and(|r| r != probe.room) {
                            continue;
                        }
                        let d2 = [x, y, z]
                            .iter()
                            .zip(coords)
                            .map(|(a, b)| (*a as f64 - b).powi(2))
                            .sum::<f64>();
                        let id = (z * dims[1] + y) * dims[0] + x;
                        let lattice = [x, y, z];
                        let world_position = std::array::from_fn(|axis| {
                            self.min[axis] + (lattice[axis] as f32 + 0.5) * self.cell_m
                        });
                        if !accept(world_position) {
                            continue;
                        }
                        if pass == 0 {
                            if d2 <= 1.0e-20
                                || world_position.map(f32::to_bits) == position.map(f32::to_bits)
                            {
                                exact = Some((id, world_position, probe));
                            }
                            continue;
                        }
                        if d2 >= 4.0 {
                            continue;
                        }
                        let weight = (1.0 - d2.sqrt() / 2.0).powi(2) / d2;
                        sum += weight;
                        visit(id, world_position, probe, weight);
                    }
                }
            }
            if let Some((id, world_position, probe)) = exact {
                visit(id, world_position, probe, 1.0);
                return Some(1.0);
            }
        }
        Some(sum)
    }

    /// The display-space light a moving object reads at a world position.
    ///
    /// This diagnostic convenience method reads the stored
    /// isotropic term — the orientation-free component the static shader
    /// reconstructs around. The value goes through the same tone map the
    /// static shader applies; runtime entities instead carry the raw
    /// coefficients and reconstruct against their posed surface normals. (The full reconstruction's sphere mean is
    /// `I * (1 - |g| / (2k))`; this diagnostic deliberately uses the
    /// isotropic term.)
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
    /// Returns an error for an inconsistent grid, an out-of-range dimension or
    /// a field beyond [`MAX_PROBE_FIELD_BYTES`].
    pub fn write(&self) -> Result<Vec<u8>, String> {
        if !self.is_consistent() {
            return Err("probe field dimensions do not match its probe count".to_string());
        }
        for (axis, value) in self.dims.iter().enumerate() {
            if *value == 0 || usize::try_from(*value).unwrap_or(usize::MAX) > MAX_PROBE_CELLS {
                return Err(format!("probe field dimension {axis} is out of range"));
            }
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
        if cursor.remaining() != count.saturating_mul(36) {
            return Err("probe field payload length does not match its dimensions".to_string());
        }
        let mut probes = Vec::with_capacity(count);
        for _ in 0..count {
            let irradiance = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let direction = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let axis = [cursor.f32()?, cursor.f32()?];
            let room = cursor.i32()?;
            if !irradiance.iter().all(|value| value.is_finite())
                || !direction.iter().all(|value| value.is_finite())
                || !axis.iter().all(|value| value.is_finite())
            {
                return Err("probe field holds a non-finite value".to_string());
            }
            if room < -1 || room > i32::from(i16::MAX) {
                return Err(format!("probe field room {room} is out of range"));
            }
            let probe = ProbeSample {
                irradiance,
                direction,
                axis,
                room,
            };
            if !probe.values_valid() {
                return Err("probe field holds invalid energy or moment values".to_string());
            }
            probes.push(probe);
        }
        if !cursor.is_at_end() {
            return Err(format!(
                "probe field has {} trailing byte(s)",
                cursor.remaining()
            ));
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
    fn exact_centres_hdr_and_signed_moments_survive_without_conversion() {
        let mut f = field();
        f.probes[0].irradiance = [4.0, 2.0, 1.0];
        f.probes[0].direction = [-1.0, 2.0, -0.5];
        let read = ProbeField::read(&f.write().expect("write")).expect("read");
        assert_eq!(read, f);
        let position = [0.5; 3];
        let sample = read.sample(position, Some(3)).expect("exact centre");
        assert_eq!(sample.irradiance, f.probes[0].irradiance);
        assert_eq!(sample.direction, f.probes[0].direction);
        let candidates = read.sample_diagnostics(position, Some(3));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, 0);
        assert_eq!(candidates[0].weight, 1.0);
        assert_eq!(candidates[0].distance_m, 0.0);
    }

    #[test]
    fn shifted_non_integral_grid_centres_resolve_exactly_in_world_space() {
        let mut f = field();
        f.min = [-2400.25, 1200.3, -8100.4];
        f.cell_m = 1.830_469_6;
        let position = f.min.map(|v| v + 0.5 * f.cell_m);
        let sample = f.sample(position, Some(3)).expect("centre");
        assert_eq!(sample.irradiance, f.probes[0].irradiance);
        assert_eq!(sample.direction, f.probes[0].direction);
        assert_eq!(f.sample_diagnostics(position, Some(3)).len(), 1);
    }

    #[test]
    fn weights_are_normalized_and_movement_is_continuous_in_three_dimensions() {
        let mut f = field();
        f.dims = [2, 2, 2];
        f.probes = (0_u16..8)
            .map(|i| ProbeSample {
                irradiance: [f32::from(i) * 0.1; 3],
                room: 3,
                axis: [0.5; 2],
                ..ProbeSample::default()
            })
            .collect();
        for axis in 0..3 {
            let mut last: Option<f32> = None;
            for step in 0_u16..=200 {
                let mut p = [1.0; 3];
                p[axis] = f32::from(step).mul_add(0.01, 0.1);
                let sample = f.sample(p, Some(3)).expect("sample");
                let candidates = f.sample_diagnostics(p, Some(3));
                let sum: f64 = candidates.iter().map(|c| c.weight).sum();
                assert!((sum - 1.0).abs() < 1.0e-12);
                assert!(
                    candidates
                        .iter()
                        .all(|c| c.weight.is_finite() && c.weight >= 0.0)
                );
                if let Some(previous) = last {
                    assert!((sample.irradiance[0] - previous).abs() < 0.02);
                }
                last = Some(sample.irradiance[0]);
                assert_eq!(
                    f.sample(p, Some(3)),
                    Some(sample),
                    "stationary samples are deterministic"
                );
            }
        }
    }

    #[test]
    fn sparse_neighbours_blend_without_nearest_probe_snaps() {
        let mut f = field();
        f.dims = [3, 1, 1];
        f.probes.insert(
            1,
            ProbeSample {
                room: -1,
                ..ProbeSample::default()
            },
        );
        let left = f.sample([1.49, 0.5, 0.5], Some(3)).expect("left");
        let right = f.sample([1.51, 0.5, 0.5], Some(3)).expect("right");
        assert!((left.irradiance[0] - right.irradiance[0]).abs() < 0.02);
        let middle = f.sample([1.5, 0.5, 0.5], Some(3)).expect("middle");
        assert!((middle.irradiance[0] - 0.3).abs() < 1.0e-6);
        assert!(f.sample([20.0; 3], Some(3)).is_none());
        assert!(
            f.sample_filtered([1.5, 0.5, 0.5], Some(3), |p| p[0] < 1.0)
                .is_some()
        );
        assert!(
            f.sample_filtered([1.5, 0.5, 0.5], Some(3), |_| false)
                .is_none()
        );
    }

    #[test]
    fn malformed_data_is_rejected_and_never_poisoned_or_sanitized_to_black() {
        for bad in [f32::NAN, f32::INFINITY, -1.0, f32::MAX] {
            let mut f = field();
            f.probes[0].irradiance[0] = bad;
            let bytes = f.write().expect("fixture encodes invalid bytes");
            assert!(ProbeField::read(&bytes).is_err());
            let sample = f
                .sample([0.5; 3], Some(3))
                .expect("other valid neighbour answers");
            assert!(
                sample
                    .irradiance
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.0 && *v < 1.0)
            );
            f.probes[1].irradiance[0] = bad;
            assert!(f.sample([0.5; 3], Some(3)).is_none());
        }
        let mut f = field();
        f.probes[0].direction = [100.0; 3];
        assert!(ProbeField::read(&f.write().expect("bytes")).is_err());
        for bad in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
            f.cell_m = bad;
            assert!(f.sample([0.5; 3], Some(3)).is_none());
        }
        assert!(field().sample([f32::NAN; 3], Some(3)).is_none());
        assert!(field().sample([f32::MAX; 3], Some(3)).is_none());
        let mut empty = field();
        empty.probes.clear();
        assert!(empty.sample([0.5; 3], Some(3)).is_none());
    }

    #[test]
    fn raw_shipped_payload_agrees_bit_for_bit_with_runtime_decode() {
        use std::io::Read;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(include_bytes!(
            "../../assets/levels/places_demo.placesmap"
        )))
        .expect("archive");
        let manifest: serde_json::Value =
            serde_json::from_reader(archive.by_name("manifest.json").expect("manifest"))
                .expect("json");
        for variant in manifest["variants"].as_array().expect("variants") {
            let Some(entry) = variant["entries"]["irradiance"].as_str() else {
                continue;
            };
            let mut bytes = Vec::new();
            archive
                .by_name(entry)
                .expect("field")
                .read_to_end(&mut bytes)
                .expect("bytes");
            let f = ProbeField::read(&bytes).expect("runtime decode");
            assert_eq!(f.write().expect("encode"), bytes);
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
}

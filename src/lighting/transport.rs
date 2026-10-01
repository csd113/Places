//! Offline HDR static light transport for the prepared lightmaps.
//!
//! This module is the compiler's transport solver. It replaces the historical
//! display-space pool/blend heuristic for the *prepared* lightmap path with a
//! real visibility-tested solve in linear HDR:
//!
//! 1. **Direct illumination.** Every light source is sampled on its authored
//!    shape (point, rectangle or line) with a fixed tap pattern, each tap
//!    shadow-tested against the static triangle set. The visible fraction of the
//!    emitter is the soft shadow term, so an extended panel produces a
//!    penumbra instead of a hard edge.
//! 2. **Diffuse bounces.** Every receiver traces a deterministic hemisphere
//!    gather and reads the hit triangle's solved irradiance through a bounded,
//!    visibility-tested surface cache. Each order reads the previous order,
//!    giving colored indirect illumination without reinjecting direct light.
//! 3. **Directional encoding.** Every contribution is stored as a mean plus a
//!    first moment: `irradiance += 0.5 w`, `moment += 0.5 w * omega`, where
//!    `omega` points from the receiver toward the light. The compact stored
//!    form sums the per-channel moments into one vector `g = sum_c m_c`
//!    ([`LightmapTexel::direction`]) and reconstructs the calibrated sharp
//!    cosine
//!    `light_c(n) = max(0, I_c + (I_c / sum I) * (2 * max(0, dot(g, n)) - |g|))`,
//!    which is exact for any number of contributions sharing one direction
//!    (`2 * I_c * max(0, cos)`), evaluates its nonlinear step on the scalar
//!    `dot(g, n)` of the interpolated moment (so a hardware interpolation
//!    between texels cannot sweep a discontinuous parameterisation), and
//!    collapses to the isotropic mean where the moment cancels, so tiled and
//!    curved surfaces react to their real normals instead of receiving one
//!    isotropic value.
//!
//! Units and normalization
//! ------------------------
//! Values are linear HDR "display light": the same scale the historical bake
//! produced for a white surface (`LOCAL_LIGHT_STRENGTH * intensity *
//! height_factor * shape * visibility`), extended above 1.0 by real transport.
//! The authored falloff curve and range are kept as the game's calibrated
//! fixture response; direct light is not divided by distance squared, which
//! would replace the shipped pool shape. Bounce uses a diffuse angular integral
//! with an explicit calibration constant
//! ([`BOUNCE_GAIN`]) so the indirect term is bounded and testable.
//!
//! Determinism and parallelism
//! ---------------------------
//! Every receiver's value is a pure function of its index and the immutable
//! scene, so a parallel solve and a serial solve produce identical values (the
//! tests compare them under a tight tolerance). Work is partitioned by output
//! index with scoped threads; there is no global lock, no shared accumulator
//! and no per-worker allocation of the scene. A cancellation flag is polled
//! between output indices.
//!
//! What is still runtime
//! ---------------------
//! Nothing in this module runs in the player. The player decodes the solved
//! pages and samples them; the probe field the renderer interpolates for moving
//! objects is solved here too, offline.

// The transport solver is a numeric kernel: it evaluates fixed-size
// three-vectors with explicit `f32` arithmetic (so serial and parallel runs
// are bit-identical), converts bounded lattice indices after enforcing their
// caps, and indexes arrays by constants. Those are exactly the shapes the
// cast/float/index lints flag, so they are allowed here as a unit; no other
// module inherits them.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_if,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::imprecise_flops,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_range_loop,
    clippy::question_mark,
    clippy::redundant_locals,
    clippy::single_match_else,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_arguments,
    clippy::unnecessary_wraps,
    clippy::while_let_loop
)]

use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) mod probe_audit;

use crate::lighting::lightmap::{Chart, LightmapFailure, LightmapPatch, LightmapTexel, PatchKind};
use crate::lighting::probes::{ProbeField, ProbeSample};
use crate::lighting::{AMBIENT_LEVEL, LevelLighting, LightFalloff, LightShape};

/// Minimum normal offset, equal to four single-precision rounding steps at
/// unit scale. Actual receivers scale this bound with their world coordinates.
/// This is numerical separation, not a geometric centimetre-sized displacement.
pub const SURFACE_OFFSET_M: f32 = 4.0 * f32::EPSILON;

/// No metric dead zone: even a sub-millimetre neighbouring stair face blocks.
/// The double-precision intersection excludes only non-positive distances.
pub const RAY_EPS_M: f32 = 0.0;

/// Largest number of triangles a transport scene may contain.
///
/// The level format's own vertex budget keeps real content far below this; the
/// bound exists so a malformed asset cannot make the BVH build allocate
/// unbounded memory.
pub const MAX_TRANSPORT_TRIANGLES: usize = 4_194_304;

/// Triangles per BVH leaf.
const BVH_LEAF_TRIANGLES: usize = 4;

/// Depth cap of the BVH recursion, so a pathological triangle soup cannot
/// recurse without bound.
const BVH_MAX_DEPTH: u32 = 40;

/// Calibration of the bounce estimator.
///
/// The bounce pass integrates the diffuse transfer unbiassed (uniform
/// hemisphere ray sampling with the sample's own solid-angle weight), so the
/// physical value is `1`. The constant exists as the single documented knob a
/// recalibration of the authored direct strength would use together; it is not
/// an ambient floor and never adds light where no ray found a surface.
pub const BOUNCE_GAIN: f32 = 1.0;

/// Most bounce rays one receiver traces per bounce pass.
pub const MAX_BOUNCE_RAYS: usize = 256;

/// Bounce rays one receiver traces when the caller does not choose a budget.
pub const DEFAULT_BOUNCE_RAYS: usize = 48;

/// Edge length of one bounce-cache cell, in metres.
///
/// A bounce ray reads the previous pass's solved light at its hit point from a
/// coarse 3D grid; smaller cells sharpen the indirect transfer and cost memory,
/// bounded by [`MAX_CACHE_CELLS`] per axis.
const CACHE_CELL_M: f32 = 0.75;

/// Cap on bounce-cache cells per axis.
const MAX_CACHE_CELLS: usize = 96;

/// HDR values at or below this pass through the display tone map unchanged, so
/// the calibrated look of the shipped light levels is preserved exactly and
/// only genuine highlights compress. The shader uses the same value.
pub const SOFT_KNEE: f32 = 0.8;

/// The display conversion the renderer applies to reconstructed HDR light.
///
/// Below [`SOFT_KNEE`] the value passes through; above it a C1-continuous
/// exponential shoulder compresses toward 1.0:
/// `knee + (1 - knee) * (1 - exp(-(x - knee) / (1 - knee)))`.
/// The knee's derivative is 1 on both sides, so a lit surface at the knee is
/// unchanged by the conversion.
#[must_use]
pub fn soft_clip(color: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0_f32; 3];
    for channel in 0..3 {
        let value = color.get(channel).copied().unwrap_or(0.0);
        out[channel] = soft_clip_channel(value);
    }
    out
}

/// One channel of [`soft_clip`], as a scalar.
#[must_use]
pub fn soft_clip_channel(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    if value <= SOFT_KNEE {
        return value.max(0.0);
    }
    let overflow = value - SOFT_KNEE;
    let shoulder = 1.0 - SOFT_KNEE;
    SOFT_KNEE + shoulder * (1.0 - (-(overflow / shoulder)).exp())
}

/// Revision of the transport solver's maths and constants.
///
/// The lightmap content key includes [`solver_fingerprint`], so changing the
/// solver's equations or calibration invalidates every cached atlas instead of
/// silently reusing pages the previous solver produced.
#[must_use]
pub fn solver_fingerprint() -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for bits in [
        SOLVER_REVISION,
        u64::from(crate::lighting::probes::PROBE_FIELD_RECORD_VERSION),
        u64::from(crate::lighting::probes::PROBE_SPACING_M.to_bits()),
        u64::from(crate::lighting::probes::MAX_PROBE_CELLS as u32),
        u64::from(PROBE_BAKE_RAYS as u32),
        u64::from(PROBE_CLEARANCE_M.to_bits()),
        u64::from(SOFT_KNEE.to_bits()),
        u64::from(BOUNCE_GAIN.to_bits()),
        u64::from(MAX_BOUNCE_RAYS as u32),
        u64::from(CACHE_CELL_M.to_bits()),
        u64::from(MAX_CACHE_CELLS as u32),
        u64::from(SURFACE_OFFSET_M.to_bits()),
        u64::from(RAY_EPS_M.to_bits()),
        u64::from(super::tuning::WATER_EXTINCTION_PER_M[0].to_bits()),
        u64::from(super::tuning::WATER_EXTINCTION_PER_M[1].to_bits()),
        u64::from(super::tuning::WATER_EXTINCTION_PER_M[2].to_bits()),
    ] {
        for byte in bits.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// Bumped whenever the transport equations, bounce sampling or filtering
/// change in a way that alters solved values.
///
/// * `1` — the initial offline transport solve.
/// * `2` — the linear moment representation: the stored direction is the
///   vector sum of the per-channel first moments and the octahedral dominant
///   axis is gone, so every value a version-1 solver produced is invalid.
/// * `3` — the moment reconstruction is the calibrated sharp cosine
///   (`2 * max(0, dot(g, n)) - |g|`), which also changes the radiance the
///   bounce passes and the probe field read back, so version-2 pages are
///   invalid.
/// * `4` — water surfaces and alpha-transparent architecture panes transmit
///   instead of occluding, water attenuates by submerged depth, and the base
///   solve lifts every floor/wall/skirt chart and probe room to its authored
///   baseline fill (one uniform scalar per chart/room, so internal contrast
///   survives; ceilings keep their physical solve), so every version-3 atlas
///   is invalid.
/// * `5` — scale-aware ray origins, watertight intersections, and a continuous
///   receiver-local baseline response shared by ceilings and probes replace
///   chart/room mean corrections.
/// * `6` — bounce-cache cells retain each triangle's representative and bounce
///   ray sequences are independent of chart ordering.
/// * `7` — each diffuse order transports only the previous order, preventing
///   repeated first-bounce energy in Full quality; surface irradiance retains
///   the exact cosine integral instead of reusing compressed angular moments.
/// * `8` — probes gather sky once with shared antipodal sphere samples; cache
///   interpolation is visibility-tested, capped grids are centered in their
///   actual bounds, compiler probe placement rejects solid/non-air cells, and
///   prop cutout cards transmit rather than manufacturing opaque card shadows.
/// * `9` — static model triangles are real surface receivers, with neutral
///   albedo and triangular chart padding instead of legacy position samples.
pub const SOLVER_REVISION: u64 = 9;

/// Largest worker count the solver will start.
pub const MAX_TRANSPORT_WORKERS: usize = 12;

/// One triangle of the transport scene, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportTriangle {
    /// First corner.
    pub p0: [f32; 3],
    /// Second corner.
    pub p1: [f32; 3],
    /// Third corner.
    pub p2: [f32; 3],
    /// Geometric normal, normalized and pointing away from the solid.
    pub normal: [f32; 3],
    /// Diffuse albedo in `0..=1`, already multiplied by the texture's average
    /// colour.
    pub albedo: [f32; 3],
    /// True when a ray passes straight through this triangle and it
    /// contributes no bounce albedo.
    ///
    /// The shipped renderer draws a water volume's surface and an
    /// alpha-transparent architecture pane as openings the vertex-lit bake
    /// transmits light through, so the transport solve marks them here instead
    /// of treating them as solid blockers. Both `any_hit`/`occluded` and
    /// `intersect` skip a transmissive triangle, which means a shadow ray
    /// ignores it and a bounce ray continues to the next real hit behind it.
    pub transmissive: bool,
}

impl TransportTriangle {
    /// Builds a triangle, rejecting a degenerate or non-finite one.
    #[must_use]
    pub fn new(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], albedo: [f32; 3]) -> Option<Self> {
        if !p0
            .iter()
            .chain(p1.iter())
            .chain(p2.iter())
            .chain(albedo.iter())
            .all(|v| v.is_finite())
        {
            return None;
        }
        let e1 = sub(p1, p0);
        let e2 = sub(p2, p0);
        let cross = cross3(e1, e2);
        let area = length(cross);
        if !area.is_finite() || area <= 1.0e-12 {
            return None;
        }
        let normal = scale(cross, 1.0 / area);
        let albedo = [
            albedo[0].clamp(0.0, 1.0),
            albedo[1].clamp(0.0, 1.0),
            albedo[2].clamp(0.0, 1.0),
        ];
        Some(Self {
            p0,
            p1,
            p2,
            normal,
            albedo,
            transmissive: false,
        })
    }

    /// Marks this triangle transmissive (or solid) in builder form.
    ///
    /// See [`TransportTriangle::transmissive`]; triangles start solid.
    #[must_use]
    pub const fn with_transmissive(mut self, transmissive: bool) -> Self {
        self.transmissive = transmissive;
        self
    }

    /// Two-sided area, in square metres.
    #[must_use]
    pub fn area(&self) -> f32 {
        0.5 * length(cross3(sub(self.p1, self.p0), sub(self.p2, self.p0)))
    }
}

/// The emitting shape of one transport light, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmitterShape {
    /// A single point.
    Point,
    /// A rectangle: `u` and `v` are the world-space half-extent vectors.
    Rect {
        /// Half-extent along one panel axis.
        u: [f32; 3],
        /// Half-extent along the other panel axis.
        v: [f32; 3],
    },
    /// A straight tube: `direction` is the unit axis, `half_length` its half
    /// extent.
    Line {
        /// Unit direction of the tube.
        direction: [f32; 3],
        /// Half the tube length, in metres.
        half_length: f32,
    },
}

impl EmitterShape {
    /// The largest distance from the centre to any point of the shape.
    #[must_use]
    pub fn bounding_radius(&self) -> f32 {
        match *self {
            Self::Point => 0.0,
            Self::Rect { u, v } => length(add(u, v)).max(length(sub(u, v))),
            Self::Line { half_length, .. } => half_length.max(0.0),
        }
    }

    /// The shape's sample points, stratified over its extent.
    ///
    /// `taps` is clamped to `1..=3` per axis. A point yields one sample; a
    /// rectangle a `taps x taps` grid; a line `taps` points along its axis.
    #[must_use]
    pub fn samples(&self, taps: u8) -> Vec<[f32; 3]> {
        let taps = usize::from(taps.clamp(1, 3));
        match *self {
            Self::Point => vec![[0.0; 3]],
            Self::Rect { u, v } => {
                let mut out = Vec::with_capacity(taps.saturating_mul(taps));
                for i in 0..taps {
                    for j in 0..taps {
                        let a = tap_offset(i, taps);
                        let b = tap_offset(j, taps);
                        out.push(add(scale(u, a), scale(v, b)));
                    }
                }
                out
            }
            Self::Line {
                direction,
                half_length,
            } => {
                let mut out = Vec::with_capacity(taps);
                for i in 0..taps {
                    let a = tap_offset(i, taps);
                    out.push(scale(direction, half_length * a));
                }
                out
            }
        }
    }
}

/// Offset of tap `index` of `count`, in `-1..=1`.
fn tap_offset(index: usize, count: usize) -> f32 {
    if count <= 1 {
        return 0.0;
    }
    let last = f32::from(u16::try_from(count.saturating_sub(1)).unwrap_or(u16::MAX)).max(1.0);
    let index = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
    index.mul_add(2.0 / last, -1.0)
}

/// One resolved light in the transport scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportEmitter {
    /// World-space centre of the emitting shape.
    pub position: [f32; 3],
    /// The shape itself.
    pub shape: EmitterShape,
    /// Emitted colour, each channel in `0..=1`.
    pub color: [f32; 3],
    /// Authored intensity.
    pub intensity: f32,
    /// Distance at which the contribution reaches zero, in metres.
    pub range: f32,
    /// The authored falloff curve.
    pub falloff: LightFalloff,
    /// Ceiling-height correction of the owning room.
    pub height_factor: f32,
    /// True when this is a ceiling-mounted fixture: its reach is measured
    /// horizontally from the emitting shape (the historical directional pool),
    /// so a tall chamber's floor stays lit, and the receiver's own normal
    /// supplies the incidence falloff through the directional reconstruction.
    pub directional: bool,
    /// `Some(light_index)` when this emitter is a switchable fixture: its
    /// contribution is solved separately so a runtime switch can select it.
    pub switchable: Option<usize>,
}

impl TransportEmitter {
    fn is_valid(&self) -> bool {
        self.position
            .iter()
            .chain(&self.color)
            .all(|value| value.is_finite())
            && self.color.iter().all(|value| (0.0..=1.0).contains(value))
            && self.intensity.is_finite()
            && self.intensity >= 0.0
            && self.range.is_finite()
            && self.range > 0.0
            && self.height_factor.is_finite()
            && self.height_factor >= 0.0
            && match self.shape {
                EmitterShape::Point => true,
                EmitterShape::Rect { u, v } => {
                    u.iter().chain(&v).all(|value| value.is_finite())
                        && self.shape.bounding_radius().is_finite()
                }
                EmitterShape::Line {
                    direction,
                    half_length,
                } => {
                    direction.iter().all(|value| value.is_finite())
                        && (length(direction) - 1.0).abs() < 1.0e-4
                        && half_length.is_finite()
                        && half_length >= 0.0
                }
            }
    }

    /// Builds the transport emitter for one baked light.
    ///
    /// `switchable` is the light's index when its fixture is a switchable one,
    /// so the solver can split its contribution into its own layer set.
    #[must_use]
    pub fn from_baked(light: &crate::lighting::BakedLight, switchable: Option<usize>) -> Self {
        let source = light.source;
        let (half_w, half_d) = source.half_extents();
        let shape = match source.shape {
            LightShape::Point => EmitterShape::Point,
            LightShape::Rect { .. } => EmitterShape::Rect {
                u: [half_w, 0.0, 0.0],
                v: [0.0, 0.0, half_d],
            },
            LightShape::Line { .. } => {
                // The tube's local X axis after the same yaw rule the fixture
                // geometry uses: a turned fixture swaps its extents, which for a
                // tube means the axis runs along Z.
                let turned = crate::lighting::fixture_is_turned(source.rotation_degrees);
                let direction = if turned {
                    [0.0, 0.0, 1.0]
                } else {
                    [1.0, 0.0, 0.0]
                };
                let half_length = if turned { half_d } else { half_w };
                EmitterShape::Line {
                    direction,
                    half_length,
                }
            }
        };
        Self {
            position: source.position,
            shape,
            color: [
                source.color.r.clamp(0.0, 1.0),
                source.color.g.clamp(0.0, 1.0),
                source.color.b.clamp(0.0, 1.0),
            ],
            intensity: if source.intensity.is_finite() {
                source.intensity.max(0.0)
            } else {
                0.0
            },
            range: if source.range.is_finite() {
                source.range.max(1.0e-3)
            } else {
                1.0
            },
            falloff: source.falloff,
            height_factor: if light.height_factor.is_finite() {
                light.height_factor.max(0.0)
            } else {
                1.0
            },
            directional: light.directional,
            switchable,
        }
    }

    /// The horizontal half-extents of the emitting shape in world X/Z.
    fn horizontal_extents(&self) -> (f32, f32) {
        match self.shape {
            EmitterShape::Point => (0.0, 0.0),
            EmitterShape::Rect { u, v } => (u[0].abs() + v[0].abs(), u[2].abs() + v[2].abs()),
            EmitterShape::Line {
                direction,
                half_length,
            } => (
                direction[0].abs() * half_length,
                direction[2].abs() * half_length,
            ),
        }
    }

    /// Distance from `point` to the emitter's horizontal footprint, in metres.
    fn horizontal_distance(&self, point: [f32; 3]) -> f32 {
        let (half_w, half_d) = self.horizontal_extents();
        let dx = ((point[0] - self.position[0]).abs() - half_w).max(0.0);
        let dz = ((point[2] - self.position[2]).abs() - half_d).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }

    /// True when this emitter can reach `point` at all.
    fn reaches(&self, point: [f32; 3]) -> bool {
        if self.directional {
            return self.horizontal_distance(point) <= self.range + self.shape.bounding_radius();
        }
        let d = sub(point, self.position);
        let reach = self.range + self.shape.bounding_radius();
        dot(d, d) <= reach * reach
    }

    /// The contribution of this emitter at `point` with unit albedo, plus the
    /// mean direction toward it.
    ///
    /// `scene` supplies the shadow tests; `taps` is the per-axis emitter tap
    /// count. Returns `(weight, direction)` where both are zero when nothing is
    /// visible.
    #[must_use]
    pub fn direct(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        taps: u8,
    ) -> ([f32; 3], [f32; 3]) {
        self.direct_from(scene, point, point, taps)
    }

    fn visibility(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        ray_origin: [f32; 3],
        taps: u8,
    ) -> (f32, [f32; 3]) {
        let samples = self.shape.samples(taps);
        let total = samples.len().max(1);
        let mut visible = 0usize;
        let mut direction = [0.0_f32; 3];
        let mut centre_direction = [0.0_f32; 3];
        let mut first = true;
        for offset in samples {
            let sample = add(self.position, offset);
            if scene.occluded(ray_origin, sample) {
                continue;
            }
            visible = visible.saturating_add(1);
            let to_light = sub(sample, point);
            let distance = length(to_light);
            if distance > 1.0e-6 {
                let unit = scale(to_light, 1.0 / distance);
                direction = add(direction, unit);
                if first {
                    centre_direction = unit;
                    first = false;
                }
            }
        }
        if visible == 0 {
            return (0.0, [0.0; 3]);
        }
        let visible_fraction = visible as f32 / total as f32;
        (visible_fraction, normalize_or(direction, centre_direction))
    }

    /// Evaluate at the shared shading point while tracing from its safe origin.
    fn direct_from(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        ray_origin: [f32; 3],
        taps: u8,
    ) -> ([f32; 3], [f32; 3]) {
        if !self.intensity.is_finite() || self.intensity <= 0.0 {
            return ([0.0; 3], [0.0; 3]);
        }
        if !self.reaches(point) {
            return ([0.0; 3], [0.0; 3]);
        }
        let (visible_fraction, average) = self.visibility(scene, point, ray_origin, taps);
        if visible_fraction <= 0.0 {
            return ([0.0; 3], [0.0; 3]);
        }
        // The shape term uses the emitter's own reach convention on the
        // authored falloff curve; the visible tap fraction is the soft shadow.
        let falloff_distance = if self.directional {
            self.horizontal_distance(point)
        } else {
            length(sub(self.position, point))
        };
        let shape = self.falloff.factor(falloff_distance / self.range);
        if !shape.is_finite() || shape <= 0.0 {
            return ([0.0; 3], [0.0; 3]);
        }
        let strength = crate::lighting::LOCAL_LIGHT_STRENGTH
            * self.intensity
            * self.height_factor
            * shape
            * visible_fraction;
        if !strength.is_finite() {
            // Propagate overflow to the solve's numeric validator instead of
            // silently removing an invalid emitter from the lighting field.
            return ([f32::NAN; 3], [0.0; 3]);
        }
        if strength <= 0.0 {
            return ([0.0; 3], [0.0; 3]);
        }
        let weight = [
            self.color[0] * strength,
            self.color[1] * strength,
            self.color[2] * strength,
        ];
        (weight, average)
    }
}

/// A decoded lightmap receiver: one atlas texel with its world position and
/// orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportReceiver {
    /// World-space position of the texel, already offset off its surface.
    pub position: [f32; 3],
    /// Ray origin inset by a numerical bound from the chart boundary. Shading
    /// and cache positions stay shared across coplanar chart edges.
    pub ray_origin: [f32; 3],
    /// Surface normal at the texel.
    pub normal: [f32; 3],
    /// Diffuse albedo at the texel.
    pub albedo: [f32; 3],
    /// World-space area the texel covers, in square metres.
    pub area: f32,
    /// Index of the scene triangle the texel belongs to, or `u32::MAX` when
    /// no triangle was close enough. The bounce cache uses it to read a hit
    /// surface's own light instead of a co-planar surface across a wall.
    pub surface: u32,
    /// Per-channel water extinction this receiver's gathered light passes
    /// through, computed once from the scene's water bodies: `[1; 3]` above
    /// every surface, and `exp(-sigma_c * depth)` per overlapped body for a
    /// receiver below one. It multiplies the direct weights, the bounce gains
    /// and the authored baseline target exactly once each, so the physical
    /// solve and the chart/probe fill describe one water tint.
    pub attenuation: [f32; 3],
}

/// One resolved water body the solve transmits through and attenuates in.
///
/// Mirrors [`crate::level::WaterVolume`] as plain scene data: the level's own
/// resolution decides the footprint, surface and bottom, and the solver only
/// needs the AABB, the surface plane and the extinction to decide how much
/// light reaches a point below the surface. The corresponding surface
/// triangles are marked [`TransportTriangle::transmissive`] at build time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportWaterBody {
    /// Footprint minimum X.
    pub x0: f32,
    /// Footprint maximum X.
    pub x1: f32,
    /// Footprint minimum Z.
    pub z0: f32,
    /// Footprint maximum Z.
    pub z1: f32,
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the resolved bottom.
    pub bottom_y: f32,
    /// Per-channel extinction, per metre of vertical submerged path.
    pub extinction: [f32; 3],
}

impl TransportWaterBody {
    /// One body from a resolved level water volume, with the solver's
    /// calibrated extinction.
    #[must_use]
    pub fn from_volume(volume: &crate::level::WaterVolume) -> Self {
        Self {
            x0: volume.x0.min(volume.x1),
            x1: volume.x0.max(volume.x1),
            z0: volume.z0.min(volume.z1),
            z1: volume.z0.max(volume.z1),
            surface_y: volume.surface_y,
            bottom_y: volume.bottom_y,
            extinction: super::tuning::WATER_EXTINCTION_PER_M,
        }
    }
}

/// One probe's authored baseline target and the room it resolves to.
///
/// The target is the room area's baseline above [`AMBIENT_LEVEL`] at the
/// probe's own position, before water attenuation; the room is the same
/// [`LevelLighting::room_index_at_height`] resolution the compiler's own probe
/// labelling uses; the position drives continuous fill lookup independently of
/// room groups or chart boundaries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeTarget {
    /// World position used by the continuous fixture-support field.
    pub position: [f32; 3],
    /// Target per channel, before water attenuation.
    pub target: [f32; 3],
    /// Resolved room index, or `-1` outside every room.
    pub room: i32,
}

/// One solved chart: the receiver set plus its HDR texels.
#[derive(Clone, Debug, PartialEq)]
pub struct SolvedChart {
    /// The receivers, row-major like the chart.
    pub receivers: Vec<TransportReceiver>,
    /// The solved texels, row-major like the chart.
    pub texels: Vec<LightmapTexel>,
}

/// The complete transport result of one variant.
#[derive(Clone, Debug, PartialEq)]
pub struct TransportSolution {
    /// The always-on solve, one entry per chart in plan order.
    pub charts: Vec<SolvedChart>,
    /// One entry per switchable light, in the order the caller supplied: the
    /// light index and its own contribution per chart.
    pub switchable: Vec<(usize, Vec<SolvedChart>)>,
    /// Direct rays cast.
    pub direct_rays: usize,
    /// Bounce rays cast.
    pub bounce_rays: usize,
    /// Occupied cells in the last bounce pass's cache.
    pub cache_cells: usize,
}

/// One solve: the chart result plus the optional probe field.
#[derive(Clone, Debug, PartialEq)]
pub struct TransportSolve {
    /// The per-chart solved texels.
    pub solution: TransportSolution,
    /// The prepared irradiance field for moving objects, when requested. Probe
    /// rooms are unassigned (`-1`) until the compiler labels them.
    pub probes: Option<ProbeField>,
}

/// Solver budget and quality selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolveOptions {
    /// Emitter taps per axis, clamped to `1..=3`.
    pub taps_per_axis: u8,
    /// Diffuse bounce gathers, clamped to `0..=3`.
    pub bounces: u8,
    /// Bounce rays one receiver traces per bounce pass, clamped to
    /// `1..=MAX_BOUNCE_RAYS`.
    pub gather_samples: usize,
    /// Worker threads; `1` selects the serial reference path.
    pub workers: usize,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            taps_per_axis: 2,
            bounces: 1,
            gather_samples: 32,
            workers: 1,
        }
    }
}

/// The static triangle set, its BVH and the lights, ready to solve against.
///
/// Built once per package variant by the compiler and then queried from every
/// worker; nothing in it changes during a solve.
pub struct TransportScene {
    triangles: Vec<TransportTriangle>,
    order: Vec<u32>,
    nodes: Vec<BvhNode>,
    emitters: Vec<TransportEmitter>,
    /// Resolved water bodies the solve transmits through and attenuates in.
    water: Vec<TransportWaterBody>,
    /// Per-texel authored baseline target in receiver order (see
    /// [`receiver_targets`]); empty when the scene predates a real chart plan.
    receiver_target: Vec<[f32; 3]>,
    /// Per-probe authored baseline target and room in flat probe order (see
    /// [`probe_targets`]); empty when no probe lattice was supplied.
    probe_target: Vec<ProbeTarget>,
    /// Radiance an escaping bounce ray sees, per linear channel. Zero unless
    /// the level authored a sky with a nonzero `ambient`.
    sky_radiance: [f32; 3],
}

impl std::fmt::Debug for TransportScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportScene")
            .field("triangles", &self.triangles.len())
            .field("nodes", &self.nodes.len())
            .field("emitters", &self.emitters.len())
            .field("water", &self.water.len())
            .field("receiver_target", &self.receiver_target.len())
            .field("probe_target", &self.probe_target.len())
            .finish()
    }
}

/// One BVH node: a bounds box and either a leaf range or a child index.
#[derive(Clone, Copy, Debug)]
struct BvhNode {
    min: [f32; 3],
    max: [f32; 3],
    /// First entry of this node's range in [`TransportScene::order`].
    first: u32,
    /// Leaf triangle count; `0` for an interior node.
    count: u32,
    /// Right child index for an interior node.
    right: u32,
}

/// Calls `visit(u, v)` for every texel of one chart, rows then columns, in the
/// order [`TransportScene::receivers`] emits receivers.
///
/// Every walk that must line up with the solved texel list (receiver
/// construction, [`receiver_targets`], [`probe_targets`]) goes through this
/// one loop so the orders cannot drift apart. A zero-sized axis emits nothing.
fn for_each_texel(chart: &Chart, mut visit: impl FnMut(f32, f32)) {
    let width = usize::try_from(chart.width).unwrap_or(0);
    let height = usize::try_from(chart.height).unwrap_or(0);
    if width == 0 || height == 0 {
        return;
    }
    for j in 0..height {
        let v = texel_axis(j, height);
        for i in 0..width {
            let u = texel_axis(i, width);
            visit(u, v);
        }
    }
}

/// Separate the origin by a bound on single-precision position rounding.
/// The bound depends on world-coordinate magnitude, never on ray length, so
/// a long grazing ray cannot acquire a larger geometric light-leak allowance.
pub(crate) fn receiver_position(point: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let magnitude = point
        .iter()
        .fold(1.0_f32, |scale, value| scale.max(value.abs()));
    let offset = SURFACE_OFFSET_M * magnitude;
    [
        normal[0].mul_add(offset, point[0]),
        normal[1].mul_add(offset, point[1]),
        normal[2].mul_add(offset, point[2]),
    ]
}

/// Select the owning side of an edge before the normal separation. A texel
/// exactly on a two-sided wall/riser cannot infer that side from the hit
/// triangle's winding. Inset only boundary samples by a few rounding steps
/// toward their patch interior; this cannot cross the adjoining solid face.
fn receiver_ray_origin(patch: &LightmapPatch, u: f32, v: f32, normal: [f32; 3]) -> [f32; 3] {
    let point = patch.point_at(u, v);
    let magnitude = point
        .iter()
        .fold(1.0_f32, |scale, value| scale.max(value.abs()));
    let inset = 2.0 * SURFACE_OFFSET_M * magnitude;
    let (width, height) = patch.extent_m();
    let inset_axis = |coordinate: f32, extent: f32| {
        if extent > 0.0 {
            let margin = (inset / extent).min(0.25);
            coordinate.clamp(margin, 1.0 - margin)
        } else {
            coordinate
        }
    };
    receiver_position(
        patch.point_at(inset_axis(u, width), inset_axis(v, height)),
        normal,
    )
}

/// The authored baseline target of one position inside a resolved room: the
/// room area's baseline less [`AMBIENT_LEVEL`], per channel, clamped at zero.
///
/// This is the smooth floor's value at zero physical illumination: a
/// fixture-free room's baseline is exactly [`AMBIENT_LEVEL`], so its target is
/// exactly zero and deliberate darkness is preserved.
fn target_in_room(lighting: &LevelLighting, room: usize, x: f32, z: f32) -> [f32; 3] {
    let baseline = lighting.baseline_in_room(room, x, z).to_array();
    let mut out = [0.0_f32; 3];
    for channel in 0..3 {
        let value = baseline.get(channel).copied().unwrap_or(AMBIENT_LEVEL) - AMBIENT_LEVEL;
        out[channel] = if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        };
    }
    out
}

/// The authored baseline target and resolved room at one world position: the
/// baseline of the room whose air contains it, or zero and `-1` outside every
/// room.
fn target_and_room_at(lighting: &LevelLighting, point: [f32; 3]) -> ([f32; 3], i32) {
    lighting
        .room_index_at_height(point[0], point[1], point[2])
        .map_or(([0.0; 3], -1), |room| {
            (
                target_in_room(lighting, room, point[0], point[2]),
                i32::try_from(room).unwrap_or(i32::MAX),
            )
        })
}

/// Architectural receivers share the same spatial fill, including ceilings.
const fn chart_receives_fill(kind: PatchKind) -> bool {
    matches!(
        kind,
        PatchKind::Floor | PatchKind::Wall | PatchKind::Skirt | PatchKind::Ceiling
    )
}

/// Smooth, chart-independent protection against a broken near-black solve.
/// At zero light this adds the authored target; at four times the target it
/// joins the unchanged physical solve with a continuous first derivative.
/// The output slope is always at least one half, unlike a hard minimum which
/// erases every gradient below its threshold. Zero targets preserve darkness.
fn baseline_fill(current: f32, target: f32) -> f32 {
    if !target.is_finite() || target <= 0.0 || !current.is_finite() {
        return 0.0;
    }
    let remaining = (1.0 - 0.25 * (current / target)).max(0.0);
    target * remaining * remaining
}

/// Add the smooth floor at the receiver normal without changing the stored
/// directional moment. Inverting the reconstruction's common colour ratio
/// makes the requested lift exact, even for coloured directional light.
fn fill_texel(texel: &mut LightmapTexel, normal: [f32; 3], target: [f32; 3]) {
    let current = texel.light_at(normal);
    let fill =
        std::array::from_fn::<_, 3, _>(|channel| baseline_fill(current[channel], target[channel]));
    let added: f32 = fill.iter().sum();
    if added <= 0.0 {
        return;
    }
    let new_mean = texel.irradiance.iter().sum::<f32>() + added;
    let new_light = current.iter().sum::<f32>() + added;
    for channel in 0..3 {
        texel.irradiance[channel] = (current[channel] + fill[channel]) * (new_mean / new_light);
    }
}

/// The authored baseline target of every chart texel, in exactly the order
/// [`TransportScene::receivers`] builds receivers.
///
/// Each texel's room is the patch's own resolved `room` hint when set (the
/// face's authoritative room), else the room containing its patch point. The
/// target is the room area's baseline above [`AMBIENT_LEVEL`], not the full
/// vertex-lit sample. The continuous soft floor preserves at least half the
/// physical gradient and becomes the identity above four times the target. A texel in no room gets zero.
/// The lookup is the baked zone grid, so it stays cheap per texel.
///
/// Targets are produced for *every* chart so the list stays index-aligned
/// with [`TransportScene::receivers`]; [`chart_receives_fill`] is the single
/// decision of which kinds the fill pass consumes
/// ([`TransportScene::apply_chart_fill`]).
#[must_use]
pub fn receiver_targets(
    lighting: &LevelLighting,
    charts: &[(LightmapPatch, Chart)],
) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for (patch, chart) in charts {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        out.reserve(width.saturating_mul(height));
        if width == 0 || height == 0 {
            continue;
        }
        for_each_texel(chart, |u, v| {
            let point = patch.point_at(u, v);
            let room = patch
                .room
                .or_else(|| lighting.room_index_at_height(point[0], point[1], point[2]));
            out.push(room.map_or([0.0; 3], |room| {
                target_in_room(lighting, room, point[0], point[2])
            }));
        });
    }
    out
}

/// The authored baseline target and room of every probe, in the flat probe
/// order [`bake_probe_field`] writes.
///
/// The probe lattice is the one the chart receiver set implies, so the target
/// vector lines up index for index with the baked field. A probe is an air
/// point: its room is resolved by height at its own position, and a probe
/// outside every room gets zero and room `-1`, exactly like a texel.
#[must_use]
pub fn probe_targets(
    lighting: &LevelLighting,
    charts: &[(LightmapPatch, Chart)],
) -> Vec<ProbeTarget> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    for (patch, chart) in charts {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        positions.reserve(width.saturating_mul(height));
        if width == 0 || height == 0 {
            continue;
        }
        let normal = patch_normal(patch);
        for_each_texel(chart, |u, v| {
            positions.push(receiver_position(patch.point_at(u, v), normal));
        });
    }
    let Some((min, cell, dims)) = probe_lattice(positions.iter()) else {
        return Vec::new();
    };
    let count = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let (x, y, z) = lattice_from_index(index, dims);
        let position = [
            min[0] + (x as f32 + 0.5) * cell,
            min[1] + (y as f32 + 0.5) * cell,
            min[2] + (z as f32 + 0.5) * cell,
        ];
        let (target, room) = target_and_room_at(lighting, position);
        out.push(ProbeTarget {
            position,
            target,
            room,
        });
    }
    out
}

/// The origin, cell size and counts of the probe lattice a receiver set
/// implies, or `None` when the set is empty or not finite.
///
/// [`bake_probe_field`] and [`probe_targets`] both derive the lattice here, so
/// the baked field and its targets cannot disagree about the grid.
fn probe_lattice<'a>(
    positions: impl IntoIterator<Item = &'a [f32; 3]>,
) -> Option<([f32; 3], f32, [usize; 3])> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in positions {
        if !position.iter().all(|value| value.is_finite()) {
            return None;
        }
        for axis in 0..3 {
            if let (Some(value), Some(low), Some(high)) =
                (position.get(axis), min.get_mut(axis), max.get_mut(axis))
            {
                *low = low.min(*value);
                *high = high.max(*value);
            }
        }
    }
    if !min[0].is_finite() || !min.iter().all(|value| value.is_finite()) {
        return None;
    }
    if !max.iter().all(|value| value.is_finite()) {
        return None;
    }
    let cap = crate::lighting::probes::MAX_PROBE_CELLS as f32;
    for axis in 0..3 {
        if max[axis] - min[axis] < crate::lighting::probes::PROBE_SPACING_M {
            max[axis] = min[axis] + crate::lighting::probes::PROBE_SPACING_M;
        }
    }
    let extent = std::array::from_fn::<_, 3, _>(|axis| max[axis] - min[axis]);
    if !extent
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
    {
        return None;
    }
    let cell = extent
        .iter()
        .fold(crate::lighting::probes::PROBE_SPACING_M, |cell, extent| {
            cell.max(extent / cap)
        });
    let dims = std::array::from_fn(|axis| (extent[axis] / cell).ceil().clamp(1.0, cap) as usize);
    // Keep every axis centered in the authored bounds, including an axis
    // smaller than a cell after a large world forces coarser spacing.
    for axis in 0..3 {
        min[axis] = f32::midpoint(min[axis], max[axis]) - 0.5 * dims[axis] as f32 * cell;
    }
    Some((min, cell, dims))
}

impl TransportScene {
    /// Builds the scene and its acceleration structure.
    ///
    /// Returns `None` for malformed triangles/emitters or a triangle count
    /// beyond [`MAX_TRANSPORT_TRIANGLES`]. Geometry preparation counts skipped
    /// degenerate triangles before constructing this validated scene.
    #[must_use]
    pub fn new(triangles: Vec<TransportTriangle>, emitters: Vec<TransportEmitter>) -> Option<Self> {
        if triangles.len() > MAX_TRANSPORT_TRIANGLES
            || triangles.iter().any(|triangle| {
                TransportTriangle::new(triangle.p0, triangle.p1, triangle.p2, triangle.albedo)
                    .is_none()
                    || !triangle.normal.iter().all(|value| value.is_finite())
                    || (length(triangle.normal) - 1.0).abs() > 1.0e-4
                    || triangle
                        .albedo
                        .iter()
                        .any(|value| !(0.0..=1.0).contains(value))
            })
            || emitters.iter().any(|emitter| !emitter.is_valid())
        {
            return None;
        }
        let count = triangles.len();
        let mut order: Vec<u32> = (0..count)
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect();
        let centroids: Vec<[f32; 3]> = triangles
            .iter()
            .map(|triangle| scale(add(add(triangle.p0, triangle.p1), triangle.p2), 1.0 / 3.0))
            .collect();
        let mut nodes: Vec<BvhNode> = Vec::with_capacity(count.saturating_mul(2).max(1));
        if count > 0 {
            build_node(&triangles, &centroids, &mut order, &mut nodes, 0, count, 0);
        }
        Some(Self {
            triangles,
            order,
            nodes,
            emitters,
            water: Vec::new(),
            receiver_target: Vec::new(),
            probe_target: Vec::new(),
            sky_radiance: [0.0; 3],
        })
    }

    /// Attaches the resolved water bodies the solve transmits through and
    /// attenuates in.
    #[must_use]
    pub fn with_water(mut self, water: Vec<TransportWaterBody>) -> Self {
        self.water = water;
        self
    }

    /// Attaches the per-texel authored baseline targets in receiver order.
    ///
    /// The targets come from [`receiver_targets`] and must cover the same
    /// chart set the solve is called with; a length mismatch fails the solve
    /// rather than silently disabling or misaligning the chart fill.
    #[must_use]
    pub fn with_receiver_target(mut self, target: Vec<[f32; 3]>) -> Self {
        self.receiver_target = target;
        self
    }

    /// Attaches the per-probe authored baseline targets and rooms in flat
    /// probe order.
    ///
    /// The targets come from [`probe_targets`] and are indexed exactly like
    /// the probe lattice [`bake_probe_field`] writes; a length mismatch
    /// fails the bake instead of silently disabling its fill.
    #[must_use]
    pub fn with_probe_target(mut self, target: Vec<ProbeTarget>) -> Self {
        self.probe_target = target;
        self
    }

    /// Attaches the sky radiance an escaping bounce ray sees.
    ///
    /// Zero (the default) preserves the historical "an interior has no sky"
    /// behaviour exactly; a level's `sky.ambient` is the only producer.
    #[must_use]
    pub fn with_sky(mut self, radiance: [f32; 3]) -> Self {
        self.sky_radiance = radiance;
        self
    }

    /// The per-channel water attenuation a point at `position` receives.
    ///
    /// Every body whose footprint contains the point and whose surface is at
    /// or above it contributes `exp(-sigma_c * depth)` with `depth` clamped to
    /// the body's resolved bottom; overlapping bodies multiply. A point above
    /// every surface, outside every footprint, or with a non-finite
    /// coordinate gets `[1; 3]`.
    #[must_use]
    pub fn attenuation_at(&self, position: [f32; 3]) -> [f32; 3] {
        if !position.iter().all(|value| value.is_finite()) {
            return [1.0; 3];
        }
        let mut out = [1.0_f32; 3];
        for body in &self.water {
            if position[0] < body.x0
                || position[0] > body.x1
                || position[2] < body.z0
                || position[2] > body.z1
                || position[1] > body.surface_y
            {
                continue;
            }
            let reach = (body.surface_y - body.bottom_y).max(0.0);
            let depth = (body.surface_y - position[1]).clamp(0.0, reach);
            if depth <= 0.0 {
                continue;
            }
            for channel in 0..3 {
                let sigma = body
                    .extinction
                    .get(channel)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);
                if let Some(slot) = out.get_mut(channel) {
                    *slot *= (-sigma * depth).exp();
                }
            }
        }
        out
    }

    /// Number of triangles in the scene.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Number of emitters in the scene.
    #[must_use]
    pub fn emitter_count(&self) -> usize {
        self.emitters.len()
    }

    /// The emitters.
    #[must_use]
    pub fn emitters(&self) -> &[TransportEmitter] {
        &self.emitters
    }

    /// True when the straight segment `a -> b` crosses any triangle.
    ///
    /// Every positive-distance blocker counts at the origin. Only the far
    /// endpoint has a floating-point reconstruction allowance, so an emitter
    /// lying on its own face does not shadow itself after ray normalization.
    #[must_use]
    pub fn occluded(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let direction = sub(b, a);
        let distance = length(direction);
        if !distance.is_finite() || distance <= RAY_EPS_M {
            return false;
        }
        let unit = scale(direction, 1.0 / distance);
        let endpoint_scale = b
            .iter()
            .fold(distance.max(1.0), |scale, value| scale.max(value.abs()));
        let max_t = distance - SURFACE_OFFSET_M * endpoint_scale;
        if max_t <= 0.0 {
            return false;
        }
        self.any_hit(a, unit, max_t)
    }

    /// Any triangle hit along `origin + t * direction` for `t` in
    /// `(RAY_EPS_M, max_t)`.
    fn any_hit(&self, origin: [f32; 3], direction: [f32; 3], max_t: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = [
            safe_inverse(direction[0]),
            safe_inverse(direction[1]),
            safe_inverse(direction[2]),
        ];
        let mut stack: [u32; 64] = [0; 64];
        let mut depth = 0_usize;
        let Some(first) = self.nodes.first() else {
            return false;
        };
        if !slab_hit(first, origin, inv, max_t) {
            return false;
        }
        let mut node_index = 0_u32;
        loop {
            let Some(node) = self
                .nodes
                .get(usize::try_from(node_index).unwrap_or(usize::MAX))
            else {
                break;
            };
            if node.count > 0 {
                let start = usize::try_from(node.first).unwrap_or(usize::MAX);
                let end = start.saturating_add(usize::try_from(node.count).unwrap_or(usize::MAX));
                for entry in start..end {
                    let Some(index) = self.order.get(entry) else {
                        continue;
                    };
                    let Some(triangle) = self
                        .triangles
                        .get(usize::try_from(*index).unwrap_or(usize::MAX))
                    else {
                        continue;
                    };
                    if triangle.transmissive {
                        continue;
                    }
                    if let Some(t) = ray_triangle(origin, direction, triangle)
                        && t > RAY_EPS_M
                        && t < max_t
                    {
                        return true;
                    }
                }
            } else {
                let left = node.first;
                let right = node.right;
                let left_hit = self
                    .nodes
                    .get(usize::try_from(left).unwrap_or(usize::MAX))
                    .is_some_and(|child| slab_hit(child, origin, inv, max_t));
                let right_hit = self
                    .nodes
                    .get(usize::try_from(right).unwrap_or(usize::MAX))
                    .is_some_and(|child| slab_hit(child, origin, inv, max_t));
                if left_hit && right_hit {
                    if depth >= stack.len() {
                        // The stack is sized for the depth cap; falling back to
                        // a full scan keeps a deepest-level miss correct rather
                        // than silently unshadowed.
                        return self.linear_any_hit(origin, direction, max_t);
                    }
                    if let Some(slot) = stack.get_mut(depth) {
                        *slot = right;
                    }
                    depth = depth.saturating_add(1);
                    node_index = left;
                    continue;
                }
                if left_hit {
                    node_index = left;
                    continue;
                }
                if right_hit {
                    node_index = right;
                    continue;
                }
            }
            if depth == 0 {
                break;
            }
            depth = depth.saturating_sub(1);
            node_index = stack.get(depth).copied().unwrap_or(0);
        }
        false
    }

    /// The nearest triangle hit along `origin + t * direction`, with its
    /// distance and index.
    ///
    /// Used by the bounce pass, which needs both the hit surface's albedo and
    /// the world point where the cache is read. Traversal visits the nearer
    /// child first and prunes a node whose entry is already past the best hit.
    #[must_use]
    pub fn intersect(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, usize)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = [
            safe_inverse(direction[0]),
            safe_inverse(direction[1]),
            safe_inverse(direction[2]),
        ];
        let mut best: Option<(f32, usize)> = None;
        let mut stack: [u32; 64] = [0; 64];
        let mut depth = 0_usize;
        let mut node_index = 0_u32;
        loop {
            let Some(node) = self
                .nodes
                .get(usize::try_from(node_index).unwrap_or(usize::MAX))
            else {
                break;
            };
            let limit = best.map_or(f32::INFINITY, |(distance, _)| distance);
            if !slab_hit(node, origin, inv, limit) {
                // fall through to the pop below
            } else if node.count > 0 {
                let start = usize::try_from(node.first).unwrap_or(usize::MAX);
                let end = start.saturating_add(usize::try_from(node.count).unwrap_or(usize::MAX));
                for entry in start..end {
                    let Some(index) = self.order.get(entry) else {
                        continue;
                    };
                    let Some(triangle) = self
                        .triangles
                        .get(usize::try_from(*index).unwrap_or(usize::MAX))
                    else {
                        continue;
                    };
                    if triangle.transmissive {
                        continue;
                    }
                    if let Some(t) = ray_triangle(origin, direction, triangle)
                        && t > RAY_EPS_M
                        && best.is_none_or(|(distance, _)| t < distance)
                    {
                        best = Some((t, usize::try_from(*index).unwrap_or(usize::MAX)));
                    }
                }
            } else {
                // Visit the nearer child first so the best hit prunes sooner.
                let left = node.first;
                let right = node.right;
                let left_entry = self
                    .nodes
                    .get(usize::try_from(left).unwrap_or(usize::MAX))
                    .and_then(|child| slab_entry(child, origin, inv, limit));
                let right_entry = self
                    .nodes
                    .get(usize::try_from(right).unwrap_or(usize::MAX))
                    .and_then(|child| slab_entry(child, origin, inv, limit));
                match (left_entry, right_entry) {
                    (Some(left_t), Some(right_t)) => {
                        let (near, far, far_t) = if left_t <= right_t {
                            (left, right, right_t)
                        } else {
                            (right, left, left_t)
                        };
                        if depth >= stack.len() {
                            break;
                        }
                        if let Some(slot) = stack.get_mut(depth) {
                            *slot = far;
                        }
                        depth = depth.saturating_add(1);
                        let _ = far_t;
                        node_index = near;
                        continue;
                    }
                    (Some(_), None) => {
                        node_index = left;
                        continue;
                    }
                    (None, Some(_)) => {
                        node_index = right;
                        continue;
                    }
                    (None, None) => {}
                }
            }
            if depth == 0 {
                break;
            }
            depth = depth.saturating_sub(1);
            node_index = stack.get(depth).copied().unwrap_or(0);
        }
        best
    }

    /// Fallback full scan used only when the traversal stack saturates.
    fn linear_any_hit(&self, origin: [f32; 3], direction: [f32; 3], max_t: f32) -> bool {
        self.triangles.iter().any(|triangle| {
            !triangle.transmissive
                && ray_triangle(origin, direction, triangle)
                    .is_some_and(|t| t > RAY_EPS_M && t < max_t)
        })
    }

    /// The nearest triangle's albedo and index within `radius` metres.
    ///
    /// Used at scene-build time to attach receiver albedo and surface identity
    /// to lightmap texels, which sit on (or one bias step off) their own
    /// surface. The search is BVH-accelerated and prunes by the point-to-box
    /// distance, so it is logarithmic in the triangle count rather than a full
    /// scan.
    #[must_use]
    pub fn surface_sample(&self, point: [f32; 3], radius: f32) -> Option<([f32; 3], usize)> {
        self.near_surface(point, radius, false)
    }

    fn near_surface(
        &self,
        point: [f32; 3],
        radius: f32,
        solid_only: bool,
    ) -> Option<([f32; 3], usize)> {
        if self.nodes.is_empty() || !point.iter().all(|value| value.is_finite()) {
            return None;
        }
        let radius_sq = if radius.is_finite() && radius > 0.0 {
            radius * radius
        } else {
            0.0
        };
        let mut best: Option<(f32, usize, [f32; 3])> = None;
        let mut stack: [u32; 64] = [0; 64];
        let mut depth = 0_usize;
        let mut node_index = 0_u32;
        loop {
            let Some(node) = self
                .nodes
                .get(usize::try_from(node_index).unwrap_or(usize::MAX))
            else {
                break;
            };
            if point_box_distance_squared(point, node.min, node.max) <= radius_sq {
                if node.count > 0 {
                    let start = usize::try_from(node.first).unwrap_or(usize::MAX);
                    let end =
                        start.saturating_add(usize::try_from(node.count).unwrap_or(usize::MAX));
                    for entry in start..end {
                        let Some(index) = self.order.get(entry) else {
                            continue;
                        };
                        let Some(triangle) = self
                            .triangles
                            .get(usize::try_from(*index).unwrap_or(usize::MAX))
                        else {
                            continue;
                        };
                        if solid_only && triangle.transmissive {
                            continue;
                        }
                        let triangle_index = usize::try_from(*index).unwrap_or(usize::MAX);
                        let distance = point_triangle_distance(point, triangle);
                        if distance.is_finite()
                            && distance <= radius
                            && best.is_none_or(|(current, _, _)| distance < current)
                        {
                            best = Some((distance, triangle_index, triangle.albedo));
                        }
                    }
                } else {
                    let left = node.first;
                    let right = node.right;
                    if depth >= stack.len() {
                        break;
                    }
                    if let Some(slot) = stack.get_mut(depth) {
                        *slot = right;
                    }
                    depth = depth.saturating_add(1);
                    node_index = left;
                    continue;
                }
            }
            if depth == 0 {
                break;
            }
            depth = depth.saturating_sub(1);
            node_index = stack.get(depth).copied().unwrap_or(0);
        }
        best.map(|(_, index, albedo)| (albedo, index))
    }

    /// True when a probe is clear of opaque surfaces and is not detectably
    /// inside a closed outward-wound solid (including placed prop geometry).
    /// Six nearest-hit directions provide a conservative solid test: only six
    /// exiting backfaces reject a volume. Open/non-manifold meshes still rely
    /// on the compiler's authored floor, ceiling and wall-volume checks.
    #[must_use]
    pub fn probe_is_clear(&self, position: [f32; 3]) -> bool {
        if !position.iter().all(|value| value.is_finite())
            || self
                .near_surface(position, PROBE_CLEARANCE_M, true)
                .is_some()
        {
            return false;
        }
        let directions = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        !directions.iter().all(|direction| {
            self.intersect(position, *direction)
                .and_then(|(_, index)| self.triangles.get(index))
                .is_some_and(|triangle| dot(triangle.normal, *direction) > 0.0)
        })
    }

    /// Convenience wrapper returning only the nearest triangle's albedo.
    #[must_use]
    pub fn surface_albedo(
        &self,
        point: [f32; 3],
        _normal: [f32; 3],
        radius: f32,
    ) -> Option<[f32; 3]> {
        self.surface_sample(point, radius).map(|(albedo, _)| albedo)
    }

    fn inputs_are_valid(&self) -> bool {
        let nonnegative = |values: &[f32]| {
            values
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
        };
        nonnegative(&self.sky_radiance)
            && self
                .receiver_target
                .iter()
                .all(|target| nonnegative(target))
            && self.probe_target.iter().all(|target| {
                nonnegative(&target.target)
                    && target.position.iter().all(|value| value.is_finite())
                    && (-1..=i32::from(i16::MAX)).contains(&target.room)
            })
            && self.water.iter().all(|water| {
                [
                    water.x0,
                    water.x1,
                    water.z0,
                    water.z1,
                    water.surface_y,
                    water.bottom_y,
                ]
                .iter()
                .all(|value| value.is_finite())
                    && water.x0 <= water.x1
                    && water.z0 <= water.z1
                    && water.bottom_y <= water.surface_y
                    && nonnegative(&water.extinction)
            })
    }

    /// Solves one variant's chart set.
    ///
    /// # Errors
    ///
    /// Returns [`LightmapFailure::InvalidConfig`] for a zero worker count,
    /// [`LightmapFailure::FillNonFinite`] when a chart's texel count is not
    /// exactly the plan's, and never returns a partial result.
    pub fn solve(
        &self,
        charts: &[(LightmapPatch, Chart)],
        options: SolveOptions,
        cancel: Option<&AtomicBool>,
    ) -> Result<TransportSolution, LightmapFailure> {
        Ok(self
            .solve_with_probes(charts, options, cancel, false)?
            .solution)
    }

    /// [`Self::solve`], also preparing the irradiance field when `bake_probes`
    /// is set.
    ///
    /// The probes are solved from the same base pass that produced the chart
    /// atlas, so the field and the lightmaps can never describe different
    /// worlds.
    ///
    /// # Errors
    ///
    /// Same as [`Self::solve`].
    pub fn solve_with_probes(
        &self,
        charts: &[(LightmapPatch, Chart)],
        options: SolveOptions,
        cancel: Option<&AtomicBool>,
        bake_probes: bool,
    ) -> Result<TransportSolve, LightmapFailure> {
        if options.workers == 0 {
            return Err(LightmapFailure::InvalidConfig);
        }
        if !self.inputs_are_valid() {
            return Err(LightmapFailure::FillNonFinite);
        }
        let workers = options.workers.clamp(1, MAX_TRANSPORT_WORKERS);
        let taps = options.taps_per_axis.clamp(1, 3);
        let bounces = options.bounces.min(3);
        let bounce_samples = options.gather_samples.clamp(1, MAX_BOUNCE_RAYS);
        let base_emitters: Vec<usize> = self
            .emitters
            .iter()
            .enumerate()
            .filter(|(_, emitter)| emitter.switchable.is_none() && emitter.intensity > 0.0)
            .map(|(index, _)| index)
            .collect();
        let switchable_emitters: Vec<(usize, usize)> = self
            .emitters
            .iter()
            .enumerate()
            .filter_map(|(index, emitter)| emitter.switchable.map(|light| (light, index)))
            .collect();
        let mut direct_rays = 0usize;
        let mut bounce_rays = 0usize;
        let mut cache_cells = 0usize;
        let mut probes: Option<ProbeField> = None;
        let base = self.solve_pass(
            charts,
            &base_emitters,
            true,
            taps,
            bounces,
            bounce_samples,
            workers,
            cancel,
            &mut direct_rays,
            &mut bounce_rays,
            &mut cache_cells,
            if bake_probes { Some(&mut probes) } else { None },
        )?;
        let mut switchable = Vec::with_capacity(switchable_emitters.len());
        for (light_index, emitter) in &switchable_emitters {
            let solved = self.solve_pass(
                charts,
                std::slice::from_ref(emitter),
                false,
                taps,
                bounces,
                bounce_samples,
                workers,
                cancel,
                &mut direct_rays,
                &mut bounce_rays,
                &mut cache_cells,
                None,
            )?;
            switchable.push((*light_index, solved));
        }
        Ok(TransportSolve {
            solution: TransportSolution {
                charts: base,
                switchable,
                direct_rays,
                bounce_rays,
                cache_cells,
            },
            probes,
        })
    }

    /// One solve over a fixed emitter subset.
    ///
    /// `apply_fill` gates the authored baseline fill: the base solve gets it, a
    /// switchable fixture's own layer must not (the runtime already adds that
    /// layer on top of the base, so a fill in both would double it).
    #[allow(clippy::too_many_arguments)] // internal pass driver; the arguments are the pass's whole budget
    fn solve_pass(
        &self,
        charts: &[(LightmapPatch, Chart)],
        emitters: &[usize],
        apply_fill: bool,
        taps: u8,
        bounces: u8,
        bounce_samples: usize,
        workers: usize,
        cancel: Option<&AtomicBool>,
        direct_rays: &mut usize,
        bounce_rays: &mut usize,
        cache_cells: &mut usize,
        probes: Option<&mut Option<ProbeField>>,
    ) -> Result<Vec<SolvedChart>, LightmapFailure> {
        let receivers = self.receivers(charts)?;
        let count = receivers.len();
        if !self.receiver_target.is_empty() && self.receiver_target.len() != count {
            return Err(LightmapFailure::FillSize);
        }
        let mut accumulators = self.direct_pass(&receivers, emitters, taps, workers, cancel)?;
        if accumulators.iter().any(|value| !value.is_finite()) {
            return Err(LightmapFailure::FillNonFinite);
        }
        *direct_rays = direct_rays.saturating_add(
            count
                .saturating_mul(emitters.len())
                .saturating_mul(usize::from(taps.max(1))),
        );
        audit_stage(
            "direct",
            charts,
            &receivers,
            accumulators
                .iter()
                .zip(&receivers)
                .map(|(value, receiver)| compress_surface(value, receiver.normal)),
        );
        if bounces > 0
            && (!emitters.is_empty() || self.sky_radiance.iter().any(|channel| *channel > 0.0))
        {
            let mut bounce_count = 0usize;
            // The cache is derived from the receiver positions alone, which do
            // not move between passes; only the solved values it is read
            // against change. Build it once for every bounce pass.
            let cache = RadianceCache::build(&receivers);
            *cache_cells = cache.occupied_cells();
            let mut previous_order = accumulators.clone();
            for pass_index in 0..bounces {
                // Each pass traces uniform-hemisphere rays against the previous
                // pass's solved light, so one pass is one diffuse bounce and
                // two passes are two. The estimator is unbiased: a cosine
                // response is not baked into the ray density, so summing every
                // traced sample with its own solid-angle weight cannot lose the
                // energy a truncated point-light list lost.
                let gained = self.bounce_pass(
                    &receivers,
                    &previous_order,
                    &cache,
                    bounce_samples,
                    pass_index,
                    apply_fill,
                    workers,
                    cancel,
                )?;
                bounce_count = bounce_count
                    .saturating_add(count.saturating_mul(bounce_samples.clamp(1, MAX_BOUNCE_RAYS)));
                for (slot, value) in accumulators.iter_mut().zip(&gained) {
                    add_scaled(slot, value, 1.0);
                }
                if accumulators
                    .iter()
                    .chain(&gained)
                    .any(|value| !value.is_finite())
                {
                    return Err(LightmapFailure::FillNonFinite);
                }
                previous_order = gained;
            }
            audit_stage(
                "bounced",
                charts,
                &receivers,
                accumulators
                    .iter()
                    .zip(&receivers)
                    .map(|(value, receiver)| compress_surface(value, receiver.normal)),
            );
            *bounce_rays = bounce_rays.saturating_add(bounce_count);
        }
        if let Some(probes) = probes {
            *probes = Some(bake_probe_field(
                self,
                &receivers,
                &accumulators,
                taps,
                bounces,
                workers,
                cancel,
            )?);
        }
        // Controlled filtering: a chart-space luma-guided 3x3 pass on the
        // accumulated values removes the per-texel gather noise a point VPL
        // set leaves without blurring across a change in received light.
        let mut filtered = filter_accumulators(self, charts, &receivers, &accumulators)?;
        audit_stage("filtered", charts, &receivers, filtered.iter().copied());
        if apply_fill {
            filtered = self.apply_chart_fill(charts, &receivers, filtered, workers, cancel)?;
        }
        if filtered
            .iter()
            .any(|texel| !texel.is_finite() || texel.irradiance.iter().any(|value| *value < 0.0))
        {
            return Err(LightmapFailure::FillNonFinite);
        }
        audit_stage("filled", charts, &receivers, filtered.iter().copied());
        assemble_solved_charts(charts, &receivers, &filtered)
    }

    /// The receiver list for every chart texel, with albedo resolved from the
    /// scene and water attenuation resolved from its bodies.
    fn receivers(
        &self,
        charts: &[(LightmapPatch, Chart)],
    ) -> Result<Vec<TransportReceiver>, LightmapFailure> {
        let mut out = Vec::new();
        for (patch, chart) in charts {
            let width = usize::try_from(chart.width).unwrap_or(0);
            let height = usize::try_from(chart.height).unwrap_or(0);
            out.reserve(width.saturating_mul(height));
            if width == 0 || height == 0 {
                continue;
            }
            let normal = patch_normal(patch);
            let surface_area = length(cross3(patch.u_axis, patch.v_axis))
                * if patch.kind == PatchKind::Prop {
                    0.5
                } else {
                    1.0
                };
            let texel_area = surface_area
                / (f32::from(u16::try_from(width).unwrap_or(u16::MAX))
                    * f32::from(u16::try_from(height).unwrap_or(u16::MAX)));
            let area = if texel_area.is_finite() && texel_area > 0.0 {
                texel_area
            } else {
                1.0e-4
            };
            for_each_texel(chart, |u, v| {
                let point = patch.point_at(u, v);
                let position = receiver_position(point, normal);
                let ray_origin = receiver_ray_origin(patch, u, v, normal);
                let sampled = self.surface_sample(ray_origin, 0.2);
                let albedo = sampled.map_or([0.55, 0.55, 0.55], |(albedo, _)| albedo);
                let surface = sampled.map_or(u32::MAX, |(_, index)| {
                    u32::try_from(index).unwrap_or(u32::MAX)
                });
                out.push(TransportReceiver {
                    position,
                    ray_origin,
                    normal,
                    albedo,
                    area,
                    surface,
                    attenuation: self.attenuation_at(position),
                });
            });
        }
        if out.iter().any(|receiver| {
            !receiver
                .position
                .iter()
                .chain(&receiver.ray_origin)
                .chain(&receiver.normal)
                .chain(&receiver.albedo)
                .chain(&receiver.attenuation)
                .all(|value| value.is_finite())
        }) {
            return Err(LightmapFailure::FillNonFinite);
        }
        Ok(out)
    }

    /// Direct illumination for every receiver.
    ///
    /// Every sampled weight passes through the receiver's own water
    /// attenuation exactly once ([`TransportReceiver::attenuation`]), so a
    /// submerged texel gathers a tinted, dimmed direct term.
    fn direct_pass(
        &self,
        receivers: &[TransportReceiver],
        emitters: &[usize],
        taps: u8,
        workers: usize,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<Accumulator>, LightmapFailure> {
        parallel_map(receivers.len(), workers, cancel, |index| {
            let Some(receiver) = receivers.get(index) else {
                return Accumulator::default();
            };
            let mut accumulator = Accumulator::default();
            for emitter_index in emitters {
                let Some(emitter) = self.emitters.get(*emitter_index) else {
                    continue;
                };
                let (weight, direction) =
                    emitter.direct_from(self, receiver.position, receiver.ray_origin, taps);
                accumulate_surface_lobe(
                    &mut accumulator,
                    attenuate(weight, receiver.attenuation),
                    direction,
                    receiver.normal,
                );
            }
            accumulator
        })
    }

    /// Continuous support of the authored emitter ranges, independent of
    /// chart partitioning and surface orientation. Directional sources retain
    /// their horizontal reach so a tall ceiling can illuminate its floor.
    /// A target-only scene retains its explicit recovery target; a scene of
    /// switchable emitters contributes no permanent support.
    fn baseline_support(&self, point: [f32; 3]) -> f32 {
        if self.emitters.is_empty() {
            return 1.0;
        }
        self.emitters
            .iter()
            .filter(|emitter| emitter.switchable.is_none() && emitter.intensity > 0.0)
            .map(|emitter| {
                let distance = if emitter.directional {
                    emitter.horizontal_distance(point)
                } else {
                    length(sub(emitter.position, point))
                };
                let support = emitter.falloff.factor(distance / emitter.range);
                if !support.is_finite() || support <= 0.0 {
                    return 0.0;
                }
                // Authored recovery fill cannot cross a sealed blocker either.
                support * emitter.visibility(self, point, point, 3).0
            })
            .filter(|value| value.is_finite())
            .fold(0.0_f32, f32::max)
    }

    /// Applies the same continuous response at every architectural receiver.
    /// Chart size, mean, neighbouring texels and tessellation never enter the
    /// correction. Water attenuates the target before evaluating the response;
    /// only the always-on base solve calls this pass.
    fn apply_chart_fill(
        &self,
        charts: &[(LightmapPatch, Chart)],
        receivers: &[TransportReceiver],
        texels: Vec<LightmapTexel>,
        workers: usize,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<LightmapTexel>, LightmapFailure> {
        if self.receiver_target.len() != receivers.len() {
            return Ok(texels);
        }
        let eligible: Vec<_> = charts
            .iter()
            .flat_map(|(patch, chart)| {
                let count = usize::try_from(chart.width)
                    .unwrap_or(0)
                    .saturating_mul(usize::try_from(chart.height).unwrap_or(0));
                std::iter::repeat_n(chart_receives_fill(patch.kind), count)
            })
            .collect();
        if eligible.len() != texels.len() || receivers.len() != texels.len() {
            return Err(LightmapFailure::FillSize);
        }
        // Visibility-supported fill is independent per texel. Share the
        // immutable scene and preserve each texel's exact arithmetic order.
        parallel_map(texels.len(), workers, cancel, |index| {
            let mut texel = texels[index];
            let receiver = &receivers[index];
            let target = self.receiver_target[index];
            if eligible[index] && target.iter().any(|channel| *channel > 0.0) {
                fill_texel(
                    &mut texel,
                    receiver.normal,
                    scale(
                        attenuate(target, receiver.attenuation),
                        self.baseline_support(receiver.position),
                    ),
                );
            }
            texel
        })
    }

    /// Dynamic-object probes use the same local response, avoiding a second
    /// room-mean correction that depends on the probe lattice's coverage.
    fn apply_probe_fill(&self, baked: &mut [(ProbeSample, [f32; 3])]) {
        if self.probe_target.len() != baked.len() {
            return;
        }
        for (target, (probe, attenuation)) in self.probe_target.iter().zip(baked.iter_mut()) {
            if target.room < 0
                || target.target.iter().all(|channel| *channel <= 0.0)
                || !self.probe_is_clear(target.position)
            {
                continue;
            }
            let support = self.baseline_support(target.position);
            for channel in 0..3 {
                let current = probe.irradiance[channel];
                probe.irradiance[channel] += baseline_fill(
                    current,
                    target.target[channel] * attenuation[channel] * support,
                );
            }
        }
    }

    /// One bounce pass: uniform-hemisphere ray samples against the cache.
    ///
    /// Every receiver uses the same per-pass deterministic angular sequence,
    /// making serial and parallel results identical and preventing chart order
    /// or tessellation from changing the sampled directions. A ray that escapes
    /// the scene contributes the level's sky radiance — zero without a `sky`,
    /// which is the historical "an interior has no sky" behaviour — and a ray
    /// that hits a surface contributes that surface's solved outgoing radiance
    /// times its albedo, weighted by the sample's solid angle.
    fn bounce_pass(
        &self,
        receivers: &[TransportReceiver],
        current: &[Accumulator],
        cache: &RadianceCache,
        samples: usize,
        pass_index: u8,
        include_sky: bool,
        workers: usize,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<Accumulator>, LightmapFailure> {
        let count = samples.clamp(1, MAX_BOUNCE_RAYS);
        let inverse_count = 1.0 / f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        parallel_map(receivers.len(), workers, cancel, |index| {
            let Some(receiver) = receivers.get(index) else {
                return Accumulator::default();
            };
            let mut accumulator = Accumulator::default();
            // A shared sequence makes the estimator depend on geometry, not
            // chart ordering or how many texels another chart happened to add.
            let mut state = ray_seed(0, pass_index);
            for _ in 0..count {
                let (u1, u2) = next_pair(&mut state);
                let direction = hemisphere_sample(receiver.normal, u1, u2);
                let origin = receiver.ray_origin;
                let Some((distance, triangle_index)) = self.intersect(origin, direction) else {
                    // An escaping ray sees the level's sky radiance, which is
                    // zero unless the level authored `sky.ambient`: the night
                    // dome is the only environment term the solver has. The
                    // same per-sample weight as a bounce is used, so one
                    // calibration governs both and the term is bounded.
                    if include_sky
                        && pass_index == 0
                        && self.sky_radiance.iter().any(|channel| *channel > 0.0)
                    {
                        let weight = [
                            self.sky_radiance[0] * 2.0 * inverse_count * BOUNCE_GAIN,
                            self.sky_radiance[1] * 2.0 * inverse_count * BOUNCE_GAIN,
                            self.sky_radiance[2] * 2.0 * inverse_count * BOUNCE_GAIN,
                        ];
                        accumulate_surface_lobe(
                            &mut accumulator,
                            attenuate(weight, receiver.attenuation),
                            direction,
                            receiver.normal,
                        );
                    }
                    continue;
                };
                let hit = add(origin, scale(direction, distance));
                let Some(triangle) = self.triangles.get(triangle_index) else {
                    continue;
                };
                // The lookup is keyed by the hit triangle and checks cache
                // visibility, including dividers within that triangle.
                let cached = cache.sample_surface(self, hit, triangle_index, receivers, current);
                let radiance = cached.surface_light;
                // The sampled incoming radiance, scaled by the sample's solid
                // angle (uniform hemisphere sampling covers 2*pi over `count`
                // samples) and multiplied by the hit surface's albedo. The
                // receiver's own albedo is deliberately absent: the shader
                // multiplies the stored light by the receiver's base colour
                // exactly once. The receiver's water attenuation multiplies
                // the whole pass's gain exactly once, so a submerged texel
                // gathers a tinted bounce and never a double-attenuated one.
                let weight = [
                    triangle.albedo[0] * radiance[0] * 2.0 * inverse_count * BOUNCE_GAIN,
                    triangle.albedo[1] * radiance[1] * 2.0 * inverse_count * BOUNCE_GAIN,
                    triangle.albedo[2] * radiance[2] * 2.0 * inverse_count * BOUNCE_GAIN,
                ];
                accumulate_surface_lobe(
                    &mut accumulator,
                    attenuate(weight, receiver.attenuation),
                    direction,
                    receiver.normal,
                );
            }
            accumulator
        })
    }
    /// Reconstructs the light a receiver sees at one normal, in HDR units.
    #[must_use]
    pub fn intensity_at(
        &self,
        position: [f32; 3],
        normal: [f32; 3],
        options: SolveOptions,
    ) -> LightmapTexel {
        let receiver = TransportReceiver {
            position,
            ray_origin: position,
            normal,
            albedo: [0.0; 3],
            area: 0.0,
            surface: u32::MAX,
            attenuation: self.attenuation_at(position),
        };
        let taps = options.taps_per_axis.clamp(1, 3);
        let mut accumulator = Accumulator::default();
        for emitter in &self.emitters {
            if emitter.switchable.is_some() || emitter.intensity <= 0.0 {
                continue;
            }
            let (weight, direction) =
                emitter.direct_from(self, receiver.position, receiver.ray_origin, taps);
            accumulate_surface_lobe(
                &mut accumulator,
                attenuate(weight, receiver.attenuation),
                direction,
                receiver.normal,
            );
        }
        compress_surface(&accumulator, normal)
    }
}

/// Opt-in stage measurements before display compression, grouped by room and
/// architectural family. Area weighting prevents tiny charts dominating a report.
fn audit_stage(
    stage: &str,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    values: impl Iterator<Item = LightmapTexel>,
) {
    if !crate::logging::verbose() {
        return;
    }
    let mut regions = std::collections::BTreeMap::<String, (f32, f32, f32, f32)>::new();
    let mut samples = receivers.iter().zip(values);
    for (patch, chart) in charts {
        let key = format!("room={:?} kind={:?}", patch.room, patch.kind);
        let entry = regions.entry(key).or_default();
        for _ in 0..chart.width.saturating_mul(chart.height) {
            let Some((receiver, texel)) = samples.next() else {
                break;
            };
            let light = texel.light_at(receiver.normal);
            let luma = channel_luminance(light);
            entry.0 += receiver.area;
            entry.1 += receiver.area * luma;
            entry.2 = entry.2.max(luma);
            // At 1.4 the calibrated shoulder retains <5% of an input
            // gradient. Report lost lighting contrast, not emissive pixels.
            if luma > 1.4 {
                entry.3 += receiver.area;
            }
        }
    }
    for (region, (area, sum, peak, shoulder)) in regions {
        if area > 0.0 {
            crate::logging::info(format_args!(
                "[transport-audit] stage={stage} {region} mean={:.6} peak={peak:.6} shoulder_fraction={:.6}",
                sum / area,
                shoulder / area
            ));
        }
    }
}

fn assemble_solved_charts(
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    filtered: &[LightmapTexel],
) -> Result<Vec<SolvedChart>, LightmapFailure> {
    let mut out = Vec::with_capacity(charts.len());
    let mut offset = 0usize;
    for (_, chart) in charts {
        let texels = usize::try_from(chart.width)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(chart.height).unwrap_or(0));
        let end = offset.saturating_add(texels);
        let mut receivers_out = Vec::with_capacity(texels);
        let mut texels_out = Vec::with_capacity(texels);
        for index in offset..end {
            let Some(receiver) = receivers.get(index) else {
                return Err(LightmapFailure::FillSize);
            };
            let Some(value) = filtered.get(index) else {
                return Err(LightmapFailure::FillSize);
            };
            receivers_out.push(*receiver);
            texels_out.push(*value);
        }
        if texels_out.len() != texels {
            return Err(LightmapFailure::FillSize);
        }
        out.push(SolvedChart {
            receivers: receivers_out,
            texels: texels_out,
        });
        offset = end;
    }
    if offset != receivers.len() {
        return Err(LightmapFailure::FillSize);
    }
    Ok(out)
}

/// One receiver's accumulated lobe before the shader reconstruction.
///
/// The solver accumulates the exact first-order field: a mean `irradiance`
/// per channel and, per channel, a direction-weighted moment
/// `sum 0.5 * w_c * omega`. The compact stored form is derived from this by
/// [`compress`], so the moment is only an intermediate, never a lossy
/// container.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Accumulator {
    irradiance: [f32; 3],
    /// `moment[channel][axis]` = `sum 0.5 * w_channel * omega_axis`.
    moment: [[f32; 3]; 3],
    /// Exact cosine integral at the receiver normal, before directional compression.
    surface_light: [f32; 3],
}

impl Accumulator {
    fn is_finite(&self) -> bool {
        self.irradiance
            .iter()
            .chain(self.moment.iter().flatten())
            .chain(&self.surface_light)
            .all(|value| value.is_finite())
    }
}

/// Adds one contribution to the accumulated field with the 0.5 mean split the
/// stored reconstruction inverts.
fn accumulate_lobe(accumulator: &mut Accumulator, weight: [f32; 3], direction: [f32; 3]) {
    for channel in 0..3 {
        let Some(w) = weight.get(channel) else {
            continue;
        };
        if *w <= 0.0 {
            continue;
        }
        if let Some(slot) = accumulator.irradiance.get_mut(channel) {
            *slot += 0.5 * w;
        }
        let Some(moment) = accumulator.moment.get_mut(channel) else {
            continue;
        };
        for axis in 0..3 {
            if let (Some(slot), Some(component)) = (moment.get_mut(axis), direction.get(axis)) {
                *slot += 0.5 * w * component;
            }
        }
    }
}

/// Retain the cosine integral before the moment representation loses the
/// individual directions. Opposing grazing lights have zero irradiance at an
/// upward receiver even though their stored means are positive.
fn accumulate_surface_lobe(
    accumulator: &mut Accumulator,
    weight: [f32; 3],
    direction: [f32; 3],
    normal: [f32; 3],
) {
    accumulate_lobe(accumulator, weight, direction);
    let cosine = dot(direction, normal).max(0.0);
    for channel in 0..3 {
        accumulator.surface_light[channel] += weight[channel] * cosine;
    }
}

/// Preserve the exact cosine integral at the geometric normal while keeping
/// the compact directional response for normal maps. Compression alone is not
/// an energy integral: a uniform hemisphere reconstructed 50% too brightly.
fn compress_surface(accumulator: &Accumulator, normal: [f32; 3]) -> LightmapTexel {
    let mut texel = compress(accumulator);
    let reconstructed: f32 = texel.light_at(normal).iter().sum();
    let target: f32 = accumulator.surface_light.iter().sum();
    if reconstructed <= 0.0 {
        texel.irradiance = accumulator.surface_light;
        texel.direction = [0.0; 3];
        return texel;
    }
    let mean: f32 = texel.irradiance.iter().sum();
    texel.irradiance = accumulator
        .surface_light
        .map(|value| value * mean / reconstructed);
    texel.direction = scale(texel.direction, target / reconstructed);
    texel
}

/// Floating-point encoding tolerance, independent of artistic brightness.
fn surface_energy_matches(texel: LightmapTexel, normal: [f32; 3], expected: [f32; 3]) -> bool {
    texel.is_finite()
        && texel
            .light_at(normal)
            .iter()
            .zip(expected)
            .all(|(actual, target)| {
                target.is_finite() && (*actual - target).abs() <= 1.0e-5 + 1.0e-4 * target.abs()
            })
}

/// Multiplies one sampled weight by a receiver's per-channel attenuation.
fn attenuate(weight: [f32; 3], attenuation: [f32; 3]) -> [f32; 3] {
    [
        weight[0] * attenuation[0],
        weight[1] * attenuation[1],
        weight[2] * attenuation[2],
    ]
}

/// Compresses an accumulated field into the stored moment form.
///
/// The stored `direction` is the vector sum of the per-channel moments
/// `g = sum_c m_c`; the stored `irradiance` is the accumulated mean and the
/// reserved `axis` is written `[0.5, 0.5]`. The reconstruction
/// `max(0, I_c + (I_c / sum I) * (2 * max(0, dot(g, n)) - |g|))` is exact for a
/// single shared direction of any colour and evaluates its nonlinear step on
/// the scalar `dot(g, n)`, so a texel that gathers several directions (`g`
/// partially cancels) reconstructs to the mean plus a smaller directional term
/// instead of picking one dominant lobe.
fn compress(accumulator: &Accumulator) -> LightmapTexel {
    let mut direction = [0.0_f32; 3];
    for channel in 0..3 {
        if let Some(moment) = accumulator.moment.get(channel) {
            for axis in 0..3 {
                if let Some(value) = moment.get(axis) {
                    if let Some(slot) = direction.get_mut(axis) {
                        *slot += value;
                    }
                }
            }
        }
    }
    LightmapTexel {
        irradiance: accumulator.irradiance,
        direction,
        axis: [0.5, 0.5],
    }
}

/// Per-channel luminance weights used only for thresholds and filtering.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

fn channel_luminance(color: [f32; 3]) -> f32 {
    color[0] * LUMA[0] + color[1] * LUMA[1] + color[2] * LUMA[2]
}

/// Bakes the prepared irradiance field a moving object samples at runtime.
///
/// Every probe cell inside a room gets the same direct emitters the lightmap
/// solve used plus one ray-traced diffuse gather against the solved static
/// surfaces, so the field carries the room's real brightness, colour and
/// dominant direction. Probes outside every room, and probes the compiler
/// labels afterwards as inside a wall, stay invalid and are never sampled.
/// Water attenuation applies to the probe's direct and gathered light exactly
/// as it does to a chart receiver. Each probe then uses the same continuous
/// baseline response as static receivers, preserving at least half its local
/// physical gradient without depending on room means or lattice coverage.
///
/// # Errors
///
/// Returns [`LightmapFailure::InvalidConfig`] for an empty or unaddressable
/// receiver set and propagates cancellation as a fill failure.
#[allow(clippy::too_many_arguments)] // one bake, every input explicit
fn bake_probe_field(
    scene: &TransportScene,
    receivers: &[TransportReceiver],
    values: &[Accumulator],
    taps: u8,
    bounces: u8,
    workers: usize,
    cancel: Option<&AtomicBool>,
) -> Result<ProbeField, LightmapFailure> {
    let Some((min, cell, dims)) =
        probe_lattice(receivers.iter().map(|receiver| &receiver.position))
    else {
        return Err(LightmapFailure::InvalidConfig);
    };
    let count = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
    if count == 0 || count > crate::lighting::probes::MAX_PROBES {
        return Err(LightmapFailure::InvalidConfig);
    }
    if !scene.probe_target.is_empty() && scene.probe_target.len() != count {
        return Err(LightmapFailure::FillSize);
    }
    let cache = RadianceCache::build(receivers);
    let rays = PROBE_BAKE_RAYS;
    let audit = std::env::var_os(probe_audit::DUMP_ENV).is_some();
    if audit {
        probe_audit::dump_receivers(scene, receivers, values, bounces).map_err(|error| {
            crate::logging::warn(error);
            LightmapFailure::FillSize
        })?;
    }
    let baked = parallel_map(count, workers, cancel, |index| {
        let (x, y, z) = lattice_from_index(index, dims);
        let position = [
            min[0] + (x as f32 + 0.5) * cell,
            min[1] + (y as f32 + 0.5) * cell,
            min[2] + (z as f32 + 0.5) * cell,
        ];
        bake_probe(
            scene, receivers, values, &cache, position, index, taps, audit,
        )
    })?;
    let (mut baked, diagnostics): (Vec<_>, Vec<_>) = baked
        .into_iter()
        .map(|(probe, attenuation, diagnostic)| ((probe, attenuation), diagnostic))
        .unzip();
    // Use the same local support field and continuous response as the atlas.
    scene.apply_probe_fill(&mut baked);
    if baked.iter().any(|(probe, _)| probe.validate().is_err()) {
        return Err(LightmapFailure::FillNonFinite);
    }
    let probes = baked.into_iter().map(|(probe, _)| probe).collect();
    let field = ProbeField {
        min,
        cell_m: cell,
        dims: [
            u32::try_from(dims[0]).unwrap_or(u32::MAX),
            u32::try_from(dims[1]).unwrap_or(u32::MAX),
            u32::try_from(dims[2]).unwrap_or(u32::MAX),
        ],
        probes,
    };
    probe_audit::dump_bake(&field, &diagnostics, rays, bounces).map_err(|error| {
        crate::logging::warn(error);
        LightmapFailure::FillSize
    })?;
    Ok(field)
}

fn bake_probe(
    scene: &TransportScene,
    receivers: &[TransportReceiver],
    values: &[Accumulator],
    cache: &RadianceCache,
    position: [f32; 3],
    index: usize,
    taps: u8,
    audit: bool,
) -> (ProbeSample, [f32; 3], Option<probe_audit::ProbeAudit>) {
    let rays = PROBE_BAKE_RAYS;
    let inverse_rays = 1.0 / rays as f32;
    let attenuation = scene.attenuation_at(position);
    let target_room = scene
        .probe_target
        .get(index)
        .map_or(-1, |target| target.room);
    let clear = scene.probe_is_clear(position);
    let skip = !clear || (!scene.probe_target.is_empty() && target_room < 0);
    if skip {
        return (
            ProbeSample {
                room: -1,
                axis: [0.5; 2],
                ..ProbeSample::default()
            },
            attenuation,
            audit.then(|| {
                probe_audit::ProbeAudit::new(position, target_room, &Accumulator::default())
            }),
        );
    }
    let mut accumulator = Accumulator::default();
    let mut visible_emitters = Vec::new();
    for (emitter_index, emitter) in scene.emitters.iter().enumerate() {
        if emitter.switchable.is_some() || emitter.intensity <= 0.0 {
            continue;
        }
        let (weight, direction) = emitter.direct(scene, position, taps);
        accumulate_lobe(&mut accumulator, attenuate(weight, attenuation), direction);
        if audit && weight.iter().any(|channel| *channel > 0.0) {
            visible_emitters.push(emitter_index);
        }
    }
    let mut diagnostic = audit.then(|| {
        probe_audit::ProbeAudit::new(
            position,
            scene
                .probe_target
                .get(index)
                .map_or(-1, |target| target.room),
            &accumulator,
        )
    });
    if let Some(diagnostic) = &mut diagnostic {
        diagnostic.visible_emitters = visible_emitters;
    }
    let mut indirect = Accumulator::default();
    for ray in 0..rays {
        let direction = probe_direction(ray);
        let Some((distance, triangle_index)) = scene.intersect(position, direction) else {
            if let Some(diagnostic) = &mut diagnostic {
                diagnostic.escaping_rays += 1;
            }
            // Environment radiance is an incoming contribution just like
            // a reflected surface, with the same sphere-mean convention.
            accumulate_lobe(
                &mut indirect,
                attenuate(scale(scene.sky_radiance, 2.0 * inverse_rays), attenuation),
                direction,
            );
            continue;
        };
        let Some(triangle) = scene.triangles.get(triangle_index) else {
            continue;
        };
        let hit = add(position, scale(direction, distance));
        let cached = cache.sample_surface(scene, hit, triangle_index, receivers, values);
        let radiance = cached.surface_light;
        if let Some(diagnostic) = &mut diagnostic {
            diagnostic.surface_hits += 1;
            if radiance.iter().all(|channel| *channel <= 0.0) {
                diagnostic.zero_radiance_hits += 1;
            }
        }
        let weight = [
            triangle.albedo[0] * radiance[0] * 2.0 * inverse_rays,
            triangle.albedo[1] * radiance[1] * 2.0 * inverse_rays,
            triangle.albedo[2] * radiance[2] * 2.0 * inverse_rays,
        ];
        accumulate_lobe(&mut indirect, attenuate(weight, attenuation), direction);
    }
    if let Some(diagnostic) = &mut diagnostic {
        let texel = compress(&indirect);
        diagnostic.indirect = texel.irradiance;
        diagnostic.indirect_moment = texel.direction;
    }
    add_scaled(&mut accumulator, &indirect, 1.0);
    let texel = compress(&accumulator);
    (
        crate::lighting::probes::ProbeSample {
            irradiance: texel.irradiance,
            direction: texel.direction,
            axis: texel.axis,
            room: -1,
        },
        attenuation,
        diagnostic,
    )
}

/// Bounce rays one probe traces; a probe is an air point, so its gather is
/// cheaper than a texel's and the field is filtered by interpolation instead.
pub const PROBE_BAKE_RAYS: usize = 64;

/// Minimum air clearance around a probe, in metres. This is a placement
/// constraint, not the much smaller numerical ray-origin offset.
pub const PROBE_CLEARANCE_M: f32 = 0.05;

/// Shared stratified antipodal directions. Pairing cancels a constant
/// environment's moment exactly; sharing directions makes neighboring probes
/// depend on changes in geometry/light rather than independent random noise.
fn probe_direction(ray: usize) -> [f32; 3] {
    let pair = ray / 2;
    let half = PROBE_BAKE_RAYS / 2;
    let y = (pair as f32 + 0.5) / half as f32;
    let radius = (1.0 - y * y).sqrt();
    let phi = pair as f32 * 2.399_963_1;
    let (sin, cos) = phi.sin_cos();
    let direction = [radius * cos, y, radius * sin];
    if ray.is_multiple_of(2) {
        direction
    } else {
        scale(direction, -1.0)
    }
}

/// The lattice coordinates of a flat probe index.
fn lattice_from_index(index: usize, dims: [usize; 3]) -> (usize, usize, usize) {
    let x = index % dims[0].max(1);
    let y = (index / dims[0].max(1)) % dims[1].max(1);
    let z = index / dims[0].max(1) / dims[1].max(1);
    (x, y, z)
}

/// A chart-space luma-guided filter over the accumulated values.
///
/// Every chart is planar, so the filter only has to guard against a
/// discontinuity in the solved values themselves, which is what a doorway into
/// a dark room or a hard shadow boundary looks like. A texel only mixes with a
/// neighbour whose irradiance luminance is close to its own, so the filter
/// removes gather noise without crossing a real lighting edge.
///
/// The similarity metric compares the neighbour's *irradiance* luminance, not
/// a reconstruction of its directional light. The stored values are linear in
/// the accumulator, so the irradiance is a smooth, edge-preserving signal; the
/// old metric compared a nonlinear reconstruction at each texel's own normal,
/// which let direction noise veto legitimate smoothing and lock the noise in.
fn filter_accumulators(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    values: &[Accumulator],
) -> Result<Vec<LightmapTexel>, LightmapFailure> {
    let count = values.len();
    let mut out: Vec<LightmapTexel> = vec![LightmapTexel::ZERO; count];
    let mut offset = 0usize;
    for (patch, chart) in charts {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        let texels = width.saturating_mul(height);
        if width == 0 || height == 0 {
            continue;
        }
        for j in 0..height {
            for i in 0..width {
                let index = offset
                    .saturating_add(j.saturating_mul(width))
                    .saturating_add(i);
                let Some(center) = values.get(index) else {
                    continue;
                };
                let center_luma = channel_luminance(center.irradiance);
                let mut total_weight = 1.0_f32;
                let mut total = *center;
                for (di, dj) in [(-1_i32, 0_i32), (1, 0), (0, -1), (0, 1)] {
                    let Some(ni) = i.checked_add_signed(di as isize) else {
                        continue;
                    };
                    let Some(nj) = j.checked_add_signed(dj as isize) else {
                        continue;
                    };
                    if ni >= width || nj >= height {
                        continue;
                    }
                    let neighbour = offset
                        .saturating_add(nj.saturating_mul(width))
                        .saturating_add(ni);
                    let Some(source) = values.get(neighbour) else {
                        continue;
                    };
                    if neighbour != index {
                        let (Some(center_receiver), Some(source_receiver)) =
                            (receivers.get(index), receivers.get(neighbour))
                        else {
                            return Err(LightmapFailure::FillSize);
                        };
                        // Charts can span a solid divider just as triangles can.
                        // A denoiser must not migrate light across that boundary.
                        if scene.occluded(center_receiver.ray_origin, source_receiver.ray_origin) {
                            continue;
                        }
                    }
                    let diff = (channel_luminance(source.irradiance) - center_luma).abs();
                    let weight = 1.0 / (1.0 + 8.0 * diff);
                    if weight <= 1.0e-4 {
                        continue;
                    }
                    total_weight += weight;
                    add_scaled(&mut total, source, weight);
                }
                let mut averaged = Accumulator::default();
                add_scaled(&mut averaged, &total, 1.0 / total_weight);
                let exact = averaged.surface_light;
                if let Some(slot) = out.get_mut(index) {
                    *slot = compress_surface(&averaged, patch_normal(patch));
                    if !surface_energy_matches(*slot, patch_normal(patch), exact) {
                        crate::logging::warn(format_args!(
                            "[transport] energy mismatch at room={:?} kind={:?} origin={:?} texel=({i},{j}): integrated={exact:?} encoded={:?}; inspect transport compression before shipping this package",
                            patch.room,
                            patch.kind,
                            patch.origin,
                            slot.light_at(patch_normal(patch))
                        ));
                        return Err(LightmapFailure::TransportEnergy);
                    }
                }
            }
        }
        offset = offset.saturating_add(texels);
    }
    Ok(out)
}

/// The previous pass's solved light on a coarse 3D grid.
///
/// A bounce ray reads this cache at its hit point, which is what turns one
/// bounce pass into a full diffuse gather without a receiver-to-receiver
/// quadratic loop. Each cell keeps one representative for every triangle
/// occupying it, so adjacent charts cannot evict each other's light. A lookup
/// only accepts representatives on the hit surface's own side: a cell
/// that straddles a wall therefore cannot migrate a lit room's light through
/// the solid, and a hit whose neighbourhood holds no receiver on its side reads
/// zero instead of a neighbour's light.
struct RadianceCache {
    min: [f32; 3],
    cell: f32,
    dims: [usize; 3],
    /// One receiver per (cell, triangle), chosen nearest the cell centre.
    cells: Vec<Vec<usize>>,
}

impl RadianceCache {
    fn build(receivers: &[TransportReceiver]) -> Self {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for receiver in receivers {
            for axis in 0..3 {
                if let (Some(value), Some(low), Some(high)) = (
                    receiver.position.get(axis),
                    min.get_mut(axis),
                    max.get_mut(axis),
                ) {
                    *low = low.min(*value);
                    *high = high.max(*value);
                }
            }
        }
        if !min[0].is_finite() {
            return Self {
                min: [0.0; 3],
                cell: CACHE_CELL_M,
                dims: [0, 0, 0],
                cells: Vec::new(),
            };
        }
        let extent = [
            (max[0] - min[0]).max(0.0),
            (max[1] - min[1]).max(0.0),
            (max[2] - min[2]).max(0.0),
        ];
        let mut dims = [1usize; 3];
        let mut cell = CACHE_CELL_M;
        for axis in 0..3 {
            let requested = (extent[axis] / CACHE_CELL_M).ceil().max(1.0);
            let cap = f32::from(u16::try_from(MAX_CACHE_CELLS).unwrap_or(u16::MAX)).max(1.0);
            let capped = requested.min(cap);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Bounded by MAX_CACHE_CELLS (96), so the cast is exact.
            let count = capped as usize;
            dims[axis] = count.max(1);
            // The cell size is the largest one that keeps every receiver
            // inside the grid, so the cache is both bounded and complete.
            cell = cell.max(extent[axis] / f32::from(u16::try_from(count.max(1)).unwrap_or(1)));
        }
        let cell = if cell.is_finite() && cell > 1.0e-4 {
            cell
        } else {
            CACHE_CELL_M
        };
        let total = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
        let mut cells: Vec<Vec<usize>> = vec![Vec::new(); total];
        for (index, receiver) in receivers.iter().enumerate() {
            let Some(cell_index) = cache_cell(receiver.position, min, cell, dims) else {
                continue;
            };
            let centre = cell_centre(cell_index, min, cell, dims);
            let distance = (receiver.position[0] - centre[0]).powi(2)
                + (receiver.position[1] - centre[1]).powi(2)
                + (receiver.position[2] - centre[2]).powi(2);
            let Some(entries) = cells.get_mut(cell_index) else {
                continue;
            };
            if let Some(previous) = entries.iter_mut().find(|entry| {
                receivers
                    .get(**entry)
                    .is_some_and(|candidate| candidate.surface == receiver.surface)
            }) {
                let old_distance = receivers.get(*previous).map_or(f32::INFINITY, |candidate| {
                    dot(
                        sub(candidate.position, centre),
                        sub(candidate.position, centre),
                    )
                });
                if distance < old_distance {
                    *previous = index;
                }
            } else {
                entries.push(index);
            }
        }
        Self {
            min,
            cell,
            dims,
            cells,
        }
    }

    /// Number of cells that hold a representative receiver.
    fn occupied_cells(&self) -> usize {
        self.cells.iter().filter(|cell| !cell.is_empty()).count()
    }

    /// Select the hit triangle's representative without borrowing another face's
    /// light, including the opposite face of a thin wall in the same cell.
    fn representative(
        &self,
        cell: usize,
        surface: usize,
        receivers: &[TransportReceiver],
    ) -> Option<usize> {
        self.cells.get(cell)?.iter().copied().find(|index| {
            receivers.get(*index).is_some_and(|receiver| {
                usize::try_from(receiver.surface).unwrap_or(usize::MAX) == surface
            })
        })
    }

    /// Interpolated cached light at `point`, restricted to representatives on
    /// `normal`'s side of the surface.
    fn sample_surface(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        surface: usize,
        receivers: &[TransportReceiver],
        values: &[Accumulator],
    ) -> Accumulator {
        if self.cells.is_empty() || !point.iter().all(|value| value.is_finite()) {
            return Accumulator::default();
        }
        let coords = [
            (point[0] - self.min[0]) / self.cell - 0.5,
            (point[1] - self.min[1]) / self.cell - 0.5,
            (point[2] - self.min[2]) / self.cell - 0.5,
        ];
        let base = [coords[0].floor(), coords[1].floor(), coords[2].floor()];
        let frac = [
            (coords[0] - base[0]).clamp(0.0, 1.0),
            (coords[1] - base[1]).clamp(0.0, 1.0),
            (coords[2] - base[2]).clamp(0.0, 1.0),
        ];
        let mut total = Accumulator::default();
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
                    let cell = [
                        base[0] + dx as f32,
                        base[1] + dy as f32,
                        base[2] + dz as f32,
                    ];
                    let Some(index) = lattice_cell(cell, self.dims) else {
                        continue;
                    };
                    let Some(receiver) = self.representative(index, surface, receivers) else {
                        continue;
                    };
                    let (Some(receiver), Some(value)) =
                        (receivers.get(receiver), values.get(receiver))
                    else {
                        continue;
                    };
                    // The hit surface's own receivers only: a co-planar surface
                    // across a wall is a different triangle and must never
                    // contribute, however close its cell is.
                    if usize::try_from(receiver.surface).unwrap_or(usize::MAX) != surface {
                        continue;
                    }
                    // A single architectural triangle can straddle a divider.
                    // Surface identity alone cannot isolate its two sides.
                    if scene.occluded(receiver.ray_origin, point) {
                        continue;
                    }
                    weight_sum += weight;
                    add_scaled(&mut total, value, weight);
                }
            }
        }
        if weight_sum <= 0.0 {
            return self.nearest_visible_surface(scene, point, surface, base, receivers, values);
        }
        let inverse = 1.0 / weight_sum;
        for channel in 0..3 {
            total.surface_light[channel] *= inverse;
            if let Some(slot) = total.irradiance.get_mut(channel) {
                *slot *= inverse;
            }
            if let Some(moment) = total.moment.get_mut(channel) {
                for axis in 0..3 {
                    if let Some(slot) = moment.get_mut(axis) {
                        *slot *= inverse;
                    }
                }
            }
        }
        total
    }
    fn nearest_visible_surface(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        surface: usize,
        base: [f32; 3],
        receivers: &[TransportReceiver],
        values: &[Accumulator],
    ) -> Accumulator {
        // No interpolatable representative: fall back to the nearest
        // representative on the correct side within a bounded
        // neighbourhood, then to zero.
        let mut best: Option<(f32, Accumulator)> = None;
        for dz in -2_isize..=2 {
            for dy in -2_isize..=2 {
                for dx in -2_isize..=2 {
                    let cell = [
                        base[0] + dx as f32,
                        base[1] + dy as f32,
                        base[2] + dz as f32,
                    ];
                    let Some(index) = lattice_cell(cell, self.dims) else {
                        continue;
                    };
                    let Some(receiver) = self.representative(index, surface, receivers) else {
                        continue;
                    };
                    let (Some(receiver), Some(value)) =
                        (receivers.get(receiver), values.get(receiver))
                    else {
                        continue;
                    };
                    // The bounded fallback only reads the hit surface's own
                    // receivers: a nearby face of a different surface is
                    // exactly the co-planar-across-a-wall case the cache
                    // must never migrate light through.
                    if usize::try_from(receiver.surface).unwrap_or(usize::MAX) != surface {
                        continue;
                    }
                    let distance = dx.abs() + dy.abs() + dz.abs();
                    let distance = f32::from(u16::try_from(distance).unwrap_or(u16::MAX));
                    // Keep the original nearest/tie ordering, but do not trace
                    // a candidate that cannot replace an already visible one.
                    if best.is_some_and(|(current, _)| distance >= current) {
                        continue;
                    }
                    if scene.occluded(receiver.ray_origin, point) {
                        continue;
                    }
                    best = Some((distance, *value));
                }
            }
        }
        best.map_or_else(Accumulator::default, |(_, value)| value)
    }
}

/// Adds one accumulator scaled by `weight` into `total`.
fn add_scaled(total: &mut Accumulator, value: &Accumulator, weight: f32) {
    for channel in 0..3 {
        total.surface_light[channel] += weight * value.surface_light[channel];
        if let (Some(slot), Some(source)) = (
            total.irradiance.get_mut(channel),
            value.irradiance.get(channel),
        ) {
            *slot += weight * source;
        }
        if let (Some(slot), Some(source)) =
            (total.moment.get_mut(channel), value.moment.get(channel))
        {
            for axis in 0..3 {
                if let (Some(component), Some(source)) = (slot.get_mut(axis), source.get(axis)) {
                    *component += weight * source;
                }
            }
        }
    }
}

/// The world-space centre of one lattice cell.
fn cell_centre(index: usize, min: [f32; 3], cell: f32, dims: [usize; 3]) -> [f32; 3] {
    let mut remaining = index;
    let mut lattice = [0usize; 3];
    for axis in (0..3).rev() {
        let size = dims.get(axis).copied().unwrap_or(1).max(1);
        if let Some(slot) = lattice.get_mut(axis) {
            *slot = remaining % size;
        }
        remaining /= size;
    }
    [
        min[0] + (lattice[0] as f32 + 0.5) * cell,
        min[1] + (lattice[1] as f32 + 0.5) * cell,
        min[2] + (lattice[2] as f32 + 0.5) * cell,
    ]
}

/// The flat grid index of a world position, or `None` when outside the grid.
fn cache_cell(position: [f32; 3], min: [f32; 3], cell: f32, dims: [usize; 3]) -> Option<usize> {
    let lattice = [
        (position[0] - min[0]) / cell,
        (position[1] - min[1]) / cell,
        (position[2] - min[2]) / cell,
    ];
    lattice_cell(lattice, dims)
}

/// The flat grid index of a lattice cell, or `None` when outside the grid.
fn lattice_cell(lattice: [f32; 3], dims: [usize; 3]) -> Option<usize> {
    let mut index = 0usize;
    for axis in 0..3 {
        let value = lattice.get(axis).copied().unwrap_or(-1.0);
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // The value is finite and non-negative; the range check against `dims`
        // rejects anything that would truncate past the grid.
        let cell = value as usize;
        if cell >= dims.get(axis).copied().unwrap_or(0) {
            return None;
        }
        index = index
            .checked_mul(dims.get(axis).copied().unwrap_or(1))
            .and_then(|base| base.checked_add(cell))?;
    }
    Some(index)
}

/// A deterministic ray sequence seed; surface gathers share sequence zero,
/// while probes use their lattice index.
fn ray_seed(receiver: usize, pass: u8) -> u64 {
    (receiver as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(u64::from(pass).wrapping_mul(0xD1B5_4A32_D192_ED03))
        | 1
}

/// The next two canonical random fractions of a SplitMix64 sequence.
fn next_pair(state: &mut u64) -> (f32, f32) {
    let first = next_unit(state);
    let second = next_unit(state);
    (first, second)
}

/// One `0..1` value from a SplitMix64 sequence.
fn next_unit(state: &mut u64) -> f32 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // The top 24 bits are uniform enough for a ray direction, and 2^24 is
    // exactly representable in f32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // The shift keeps the value inside 24 bits by construction.
    let value = ((z >> 40) as u32) as f32 / 16_777_216.0;
    value.min(1.0)
}

/// A uniform sample on the hemisphere around `normal`.
///
/// The Duff et al. branchless orthonormal basis keeps the sampling frame
/// continuous across the normal's sign changes, so a chart boundary cannot
/// produce a visible seam from the ray directions alone.
fn hemisphere_sample(normal: [f32; 3], u1: f32, u2: f32) -> [f32; 3] {
    let sign = if normal[2] >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + normal[2]);
    let b = normal[0] * normal[1] * a;
    let t1 = [
        1.0 + sign * normal[0] * normal[0] * a,
        sign * b,
        -sign * normal[0],
    ];
    let t2 = [b, sign + normal[1] * normal[1] * a, -normal[1]];
    let phi = 2.0 * std::f32::consts::PI * u1;
    let cos_theta = u2.clamp(0.0, 1.0);
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let x = sin_theta * cos_phi;
    let y = sin_theta * sin_phi;
    [
        t1[0] * x + t2[0] * y + normal[0] * cos_theta,
        t1[1] * x + t2[1] * y + normal[1] * cos_theta,
        t1[2] * x + t2[2] * y + normal[2] * cos_theta,
    ]
}
/// Builds one BVH node over `order[start..end]` and returns its index.
fn build_node(
    triangles: &[TransportTriangle],
    centroids: &[[f32; 3]],
    order: &mut [u32],
    nodes: &mut Vec<BvhNode>,
    start: usize,
    end: usize,
    depth: u32,
) -> u32 {
    let node_index = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
    nodes.push(BvhNode {
        min: [0.0; 3],
        max: [0.0; 3],
        first: u32::try_from(start).unwrap_or(0),
        count: 0,
        right: 0,
    });
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut centroid_min = [f32::INFINITY; 3];
    let mut centroid_max = [f32::NEG_INFINITY; 3];
    for entry in start..end {
        let Some(index) = order.get(entry).copied() else {
            continue;
        };
        let Some(triangle) = triangles.get(usize::try_from(index).unwrap_or(usize::MAX)) else {
            continue;
        };
        for point in [triangle.p0, triangle.p1, triangle.p2] {
            for axis in 0..3 {
                if let Some(slot) = min.get_mut(axis) {
                    *slot = slot.min(point[axis]);
                }
                if let Some(slot) = max.get_mut(axis) {
                    *slot = slot.max(point[axis]);
                }
            }
        }
        if let Some(centroid) = centroids.get(usize::try_from(index).unwrap_or(usize::MAX)) {
            for axis in 0..3 {
                if let Some(slot) = centroid_min.get_mut(axis) {
                    *slot = slot.min(centroid[axis]);
                }
                if let Some(slot) = centroid_max.get_mut(axis) {
                    *slot = slot.max(centroid[axis]);
                }
            }
        }
    }
    if let Some(node) = nodes.get_mut(usize::try_from(node_index).unwrap_or(usize::MAX)) {
        node.min = min;
        node.max = max;
    }
    let count = end.saturating_sub(start);
    if count <= BVH_LEAF_TRIANGLES || depth >= BVH_MAX_DEPTH {
        if let Some(node) = nodes.get_mut(usize::try_from(node_index).unwrap_or(usize::MAX)) {
            node.count = u32::try_from(count).unwrap_or(u32::MAX);
        }
        return node_index;
    }
    let extent = sub(centroid_max, centroid_min);
    let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
        0
    } else if extent[1] >= extent[2] {
        1
    } else {
        2
    };
    let mid = start.saturating_add(count / 2);
    if let Some(slice) = order.get_mut(start..end) {
        slice.select_nth_unstable_by(mid.saturating_sub(start), |a, b| {
            let ca = centroids
                .get(usize::try_from(*a).unwrap_or(usize::MAX))
                .map_or(0.0, |value| value[axis]);
            let cb = centroids
                .get(usize::try_from(*b).unwrap_or(usize::MAX))
                .map_or(0.0, |value| value[axis]);
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let left = build_node(
        triangles,
        centroids,
        order,
        nodes,
        start,
        mid,
        depth.saturating_add(1),
    );
    let right = build_node(
        triangles,
        centroids,
        order,
        nodes,
        mid,
        end,
        depth.saturating_add(1),
    );
    if let Some(node) = nodes.get_mut(usize::try_from(node_index).unwrap_or(usize::MAX)) {
        node.first = left;
        node.right = right;
        node.count = 0;
    }
    node_index
}

/// True when the ray can reach the node's box within `max_t`.
fn slab_hit(node: &BvhNode, origin: [f32; 3], inverse: [f32; 3], max_t: f32) -> bool {
    slab_entry(node, origin, inverse, max_t).is_some()
}

/// The entry distance of the ray into the node's box, when it hits within
/// `max_t`.
fn slab_entry(node: &BvhNode, origin: [f32; 3], inverse: [f32; 3], max_t: f32) -> Option<f32> {
    let mut t_min = 0.0_f32;
    let mut t_max = max_t;
    for axis in 0..3 {
        let Some(o) = origin.get(axis) else {
            return None;
        };
        let Some(inv) = inverse.get(axis) else {
            return None;
        };
        let Some(min) = node.min.get(axis) else {
            return None;
        };
        let Some(max) = node.max.get(axis) else {
            return None;
        };
        let mut near = (min - o) * inv;
        let mut far = (max - o) * inv;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t_min = t_min.max(near);
        t_max = t_max.min(far);
        if t_min > t_max {
            return None;
        }
    }
    Some(t_min.max(0.0))
}

/// Squared distance from a point to an axis-aligned box (zero inside it).
fn point_box_distance_squared(point: [f32; 3], min: [f32; 3], max: [f32; 3]) -> f32 {
    let mut sum = 0.0_f32;
    for axis in 0..3 {
        let Some(value) = point.get(axis) else {
            return f32::INFINITY;
        };
        let (Some(lower), Some(upper)) = (min.get(axis), max.get(axis)) else {
            return f32::INFINITY;
        };
        let delta = if value < lower {
            lower - value
        } else if value > upper {
            value - upper
        } else {
            0.0
        };
        sum += delta * delta;
    }
    sum
}

/// Double-sided watertight ray/triangle test. Translate before projecting onto
/// the dominant ray axis, then evaluate the three oriented edge functions in
/// f64. Shared edges use the same products in reverse order in either triangle,
/// avoiding cracks and f32 cancellation on long, oblique rays. No barycentric
/// epsilon expands geometry and no fixed distance discards nearby blockers.
fn ray_triangle(
    origin: [f32; 3],
    direction: [f32; 3],
    triangle: &TransportTriangle,
) -> Option<f32> {
    let dominant = if direction[0].abs() > direction[1].abs() {
        if direction[0].abs() > direction[2].abs() {
            0
        } else {
            2
        }
    } else if direction[1].abs() > direction[2].abs() {
        1
    } else {
        2
    };
    let depth = f64::from(direction[dominant]);
    if depth.abs() <= f64::MIN_POSITIVE || !depth.is_finite() {
        return None;
    }
    let horizontal = (dominant + 1) % 3;
    let vertical = (horizontal + 1) % 3;
    let shear_x = f64::from(direction[horizontal]) / depth;
    let shear_y = f64::from(direction[vertical]) / depth;
    let project = |point: [f32; 3]| {
        let translated = [
            f64::from(point[0]) - f64::from(origin[0]),
            f64::from(point[1]) - f64::from(origin[1]),
            f64::from(point[2]) - f64::from(origin[2]),
        ];
        [
            translated[horizontal] - shear_x * translated[dominant],
            translated[vertical] - shear_y * translated[dominant],
            translated[dominant] / depth,
        ]
    };
    let a = project(triangle.p0);
    let b = project(triangle.p1);
    let c = project(triangle.p2);
    let edge_a = c[0] * b[1] - c[1] * b[0];
    let edge_b = a[0] * c[1] - a[1] * c[0];
    let edge_c = b[0] * a[1] - b[1] * a[0];
    if (edge_a < 0.0 || edge_b < 0.0 || edge_c < 0.0)
        && (edge_a > 0.0 || edge_b > 0.0 || edge_c > 0.0)
    {
        return None;
    }
    let determinant = edge_a + edge_b + edge_c;
    if determinant.abs() <= f64::MIN_POSITIVE {
        return None;
    }
    let distance = ((edge_a * a[2] + edge_b * b[2] + edge_c * c[2]) / determinant) as f32;
    if !distance.is_finite() {
        return None;
    }
    // At a shared corner the normal offset can leave the origin exactly on
    // the adjoining face. Its winding resolves the boundary: entering solid
    // blocks immediately, leaving the face is a harmless zero-distance hit.
    // With this projection determinant * depth is -dot(ray, geometric normal).
    if distance.abs() < f32::MIN_POSITIVE && determinant * depth > 0.0 {
        Some(f32::MIN_POSITIVE)
    } else {
        Some(distance)
    }
}

/// Squared distance from a point to a triangle (Ericson, Real-Time Collision
/// Detection §5.1.5).
fn point_triangle_distance(point: [f32; 3], triangle: &TransportTriangle) -> f32 {
    let ab = sub(triangle.p1, triangle.p0);
    let ac = sub(triangle.p2, triangle.p0);
    let ap = sub(point, triangle.p0);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return length(ap);
    }
    let bp = sub(point, triangle.p1);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return length(bp);
    }
    let vc = d1.mul_add(d4, -(d3 * d2));
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denominator = d1 - d3;
        let v = if denominator.abs() > 1.0e-12 {
            d1 / denominator
        } else {
            0.0
        };
        return length(sub(point, add(triangle.p0, scale(ab, v))));
    }
    let cp = sub(point, triangle.p2);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return length(cp);
    }
    let vb = d5.mul_add(d2, -(d1 * d6));
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denominator = d2 - d6;
        let w = if denominator.abs() > 1.0e-12 {
            d2 / denominator
        } else {
            0.0
        };
        return length(sub(point, add(triangle.p0, scale(ac, w))));
    }
    let va = d3.mul_add(d6, -(d5 * d4));
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denominator = (d4 - d3) + (d5 - d6);
        let w = if denominator.abs() > 1.0e-12 {
            (d4 - d3) / denominator
        } else {
            0.0
        };
        return length(sub(
            point,
            add(triangle.p1, scale(sub(triangle.p2, triangle.p1), w)),
        ));
    }
    let denominator = va + vb + vc;
    if denominator.abs() <= 1.0e-12 {
        return length(ap);
    }
    let v = vb / denominator;
    let w = vc / denominator;
    let closest = add(triangle.p0, add(scale(ab, v), scale(ac, w)));
    length(sub(point, closest))
}

/// The patch's outward normal, the same winding rule the historical face bake
/// used (`u = p0 -> p1`, `v = p0 -> p3`).
#[must_use]
pub fn patch_normal(patch: &LightmapPatch) -> [f32; 3] {
    let cross = cross3(patch.u_axis, patch.v_axis);
    let len = length(cross);
    if len.is_finite() && len > 1.0e-9 {
        scale(cross, 1.0 / len)
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Local coordinate of texel `index` along one axis of `count` texels,
/// spanning the patch inclusively exactly like the historical fill.
#[must_use]
pub fn texel_axis(index: usize, count: usize) -> f32 {
    let count = u16::try_from(count).unwrap_or(u16::MAX);
    if count <= 1 {
        return 0.5;
    }
    let index = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
    let last = f32::from(count.saturating_sub(1));
    (index / last).clamp(0.0, 1.0)
}

/// Runs `task` over `0..count` on up to `workers` scoped threads, preserving
/// index order in the returned vector.
///
/// Every index is written by exactly one thread into its own slot, so the
/// result is bit-identical to a serial run. `task` must be pure.
fn parallel_map<T: Send>(
    count: usize,
    workers: usize,
    cancel: Option<&AtomicBool>,
    task: impl Fn(usize) -> T + Sync,
) -> Result<Vec<T>, LightmapFailure> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if workers <= 1 || count == 1 {
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(LightmapFailure::FillSize);
            }
            out.push(task(index));
        }
        return Ok(out);
    }
    let workers = workers.min(MAX_TRANSPORT_WORKERS).min(count);
    let mut slots: Vec<Option<T>> = (0..count).map(|_| None).collect();
    let mut cancelled = false;
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        // Interleave spatially ordered receivers so one dense region cannot
        // leave a single worker processing the expensive tail of a bake.
        for worker in 0..workers {
            let task = &task;
            let cancel = cancel;
            let handle = std::thread::Builder::new()
                .name(format!("transport-solve-{worker}"))
                .spawn_scoped(scope, move || {
                    let mut values = Vec::with_capacity((count - worker).div_ceil(workers));
                    for index in (worker..count).step_by(workers) {
                        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                            break;
                        }
                        values.push(task(index));
                    }
                    values
                });
            match handle {
                Ok(handle) => handles.push((worker, handle)),
                Err(_) => {
                    cancelled = true;
                    break;
                }
            }
        }
        for (worker, handle) in handles {
            if let Ok(values) = handle.join() {
                for (offset, value) in values.into_iter().enumerate() {
                    let index = worker + offset * workers;
                    slots[index] = Some(value);
                }
            }
        }
    });
    if cancelled {
        // Spawning failed after some threads ran; the serial path is the
        // documented fallback and can never leave holes.
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(LightmapFailure::FillSize);
            }
            out.push(task(index));
        }
        return Ok(out);
    }
    let mut out = Vec::with_capacity(count);
    for slot in slots {
        match slot {
            Some(value) => out.push(value),
            None => {
                if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    return Err(LightmapFailure::FillSize);
                }
                return Err(LightmapFailure::FillSize);
            }
        }
    }
    Ok(out)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(v: [f32; 3], factor: f32) -> [f32; 3] {
    [v[0] * factor, v[1] * factor, v[2] * factor]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

fn normalize_or(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len = length(v);
    if len.is_finite() && len > 1.0e-6 {
        scale(v, 1.0 / len)
    } else {
        fallback
    }
}

fn safe_inverse(value: f32) -> f32 {
    if value.abs() <= 1.0e-20 {
        if value.is_sign_negative() {
            -1.0e20
        } else {
            1.0e20
        }
    } else {
        1.0 / value
    }
}

#[cfg(test)]
mod tests;

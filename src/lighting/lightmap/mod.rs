//! Prepared HDR lightmaps for the static world geometry.
//!
//! The lightmap path stores the offline transport solve
//! ([`crate::lighting::transport`]) per texel in a linear HDR atlas. Every texel
//! carries an *irradiance* mean and the vector sum of the per-channel first
//! moments, which the fragment shader reconstructs as the calibrated sharp
//! cosine
//! `light_c(n) = max(0, I_c + (I_c / sum I) * (2 * max(0, dot(g, n)) - |g|))`.
//! The nonlinear step runs on the scalar `dot(g, n)` of the interpolated moment
//! vector, so the hardware interpolation between texels cannot sweep a
//! discontinuous parameterisation (the seam bug of the old octahedral-axis
//! encoding). The surface texture is multiplied by that reconstructed light
//! exactly once; albedo never appears in the stored light.
//!
//! ```text
//! mesh emitter                     transport solver (bake time)
//!    quad -> LightmapPatch      solve() -> chart.width x chart.height texels
//!         -> Chart                 -> page texels + dilated gutter
//!         -> Vertex::lightmap       -> one RGBA16F layer pair per page
//! ```
//!
//! The vertex-lit path stays alive as an exact fallback: a vertex whose
//! [`crate::render::LIGHTMAP_NONE`] page byte is set never samples the atlas,
//! and a build whose lightmaps could not be produced is rebuilt with
//! [`LightmapMode::Off`], which reproduces the historical vertex colours.
//!
//! Module layout
//! -------------
//! ```text
//! mod.rs     the frozen types shared by the emitter, the solver and the renderer
//! atlas.rs   the deterministic MAXRECTS packer, HDR pages and PNG debugging
//! plan.rs    the per-level plan built while the mesh is emitted
//! cache.rs   the deterministic content key and the level lightmap cache
//! tests.rs   unit tests for the whole tree
//! ```

mod atlas;
mod cache;
mod plan;

#[cfg(test)]
mod tests;

pub use atlas::{
    ChartAllocator, LightmapAtlas, LightmapPage, page_png_bytes, read_page_texel, write_page_png,
};
pub use cache::{LIGHTMAP_FORMAT_VERSION, LightmapCache, content_key, content_key_with_extra};
pub use plan::{LevelLightmaps, LightmapMode, LightmapPlan, LightmapStats, SwitchableLightmaps};

/// The two stored linear HDR terms of one lightmap texel.
///
/// `irradiance` is the isotropic mean term (`0.5 * sum_c w_c`) in the linear
/// HDR "display light" units the offline transport solver produces. Static
/// surface terms are calibrated to preserve the exact cosine integral at the
/// geometric normal; the raw angular moments alone are not that integral.
/// `direction` is the **vector sum of the per-channel first moments**,
/// `g = sum_c m_c` with `m_c = 0.5 * sum_contributions w_c * omega`, where
/// `omega` is the unit world-space direction from the receiver toward the
/// contribution; the components are signed. `axis` is **reserved**: writers
/// store `[0.5, 0.5]` and consumers ignore it (the two `Rgba16Float` planes
/// keep their alpha channels for format stability).
///
/// The shader and [`Self::light_at`] reconstruct the calibrated sharp cosine
/// from the *interpolated* moment vector:
/// `max(0, I_c + (I_c / sum_c I_c) * (2 * max(0, dot(g, n)) - |g|))`.
/// That form is exact for any number of contributions sharing one direction of
/// any colour (`2 * I_c * max(0, cos)`), evaluates its nonlinear step on the
/// scalar `dot(g, n)` so interpolation cannot fold across an encoding seam,
/// collapses smoothly to the isotropic mean where opposing contributions
/// cancel the moment, is never negative before the clamp (`|g| <= sum I`) and
/// is bounded by `2 * irradiance_c`, so a normal-mapped or curved surface sees
/// a real directional response instead of one uniform value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightmapTexel {
    /// Isotropic irradiance mean term, linear HDR, never negative.
    pub irradiance: [f32; 3],
    /// Vector sum of the per-channel first moments, linear HDR, signed.
    pub direction: [f32; 3],
    /// Reserved: writers store `[0.5, 0.5]`, consumers ignore it.
    pub axis: [f32; 2],
}

impl LightmapTexel {
    /// A black texel.
    pub const ZERO: Self = Self {
        irradiance: [0.0; 3],
        direction: [0.0; 3],
        axis: [0.5, 0.5],
    };

    /// The texel with non-finite channels zeroed and negative values clamped.
    #[must_use]
    pub fn normalized(self) -> Self {
        let mut out = Self::ZERO;
        for (channel, slot) in out.irradiance.iter_mut().enumerate() {
            let value = self.irradiance.get(channel).copied().unwrap_or(0.0);
            *slot = if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            };
        }
        for (channel, slot) in out.direction.iter_mut().enumerate() {
            let value = self.direction.get(channel).copied().unwrap_or(0.0);
            // A moment vector is signed: it keeps its sign and only drops
            // non-finite components, which could otherwise poison every later
            // interpolation.
            *slot = if value.is_finite() { value } else { 0.0 };
        }
        for (slot, value) in out.axis.iter_mut().zip(self.axis) {
            *slot = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.5
            };
        }
        out
    }

    /// The reconstructed light at `normal`, in linear HDR units.
    ///
    /// The moment form: `k` is the sum of the irradiance channels, `g` the
    /// stored first-moment vector and `|g|` its length. The dominant lobe is
    /// evaluated from the *interpolated* moment vector and applied as the
    /// calibrated sharp cosine:
    ///
    /// ```text
    /// light_c = max(0, I_c + (I_c / k) * (2 * max(0, dot(g, n)) - |g|))
    /// ```
    ///
    /// The nonlinear step is a function of the scalar `dot(g, n)`, so every
    /// interpolated quantity stays linear and no interpolation can fold across
    /// an encoding seam. A zero moment returns the isotropic term; a single
    /// shared direction is exact (`2 * I * max(0, cos)`); a near-cancelling
    /// moment collapses to the mean. The result is never negative before the
    /// clamp because `|g| <= k`, and it is bounded by `2 * I_c`. A black texel
    /// (`k` near zero) reconstructs to its irradiance alone.
    #[must_use]
    pub fn light_at(self, normal: [f32; 3]) -> [f32; 3] {
        let k = self.irradiance[0] + self.irradiance[1] + self.irradiance[2];
        let moment = normal[2].mul_add(
            self.direction[2],
            normal[1].mul_add(self.direction[1], normal[0] * self.direction[0]),
        );
        let length = self.direction[2]
            .mul_add(
                self.direction[2],
                self.direction[1].mul_add(self.direction[1], self.direction[0] * self.direction[0]),
            )
            .sqrt();
        let lobe = 2.0_f32.mul_add(moment.max(0.0), -length);
        let mut out = [0.0_f32; 3];
        for (channel, slot) in out.iter_mut().enumerate() {
            let a = self.irradiance.get(channel).copied().unwrap_or(0.0);
            // The asymmetric condition (rather than `k <= 1e-6`) mirrors the
            // shader's `select(..., k > 1e-6)` exactly: a non-finite `k` also
            // falls back to the isotropic term.
            let value = if k > 1.0e-6 {
                (a / k).mul_add(lobe, a)
            } else {
                a
            };
            *slot = if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            };
        }
        out
    }

    /// The exact component-wise sum of two texels.
    ///
    /// Used to layer one prepared contribution onto another in the stored
    /// (pre-reconstruction) domain, such as a switchable fixture's prepared
    /// set on top of the base solve. The stored representation is linear in
    /// both fields, so adding the irradiance and signed moment vectors is
    /// lossless; `axis` is reserved and copied from `self`.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            irradiance: [
                self.irradiance[0] + other.irradiance[0],
                self.irradiance[1] + other.irradiance[1],
                self.irradiance[2] + other.irradiance[2],
            ],
            direction: [
                self.direction[0] + other.direction[0],
                self.direction[1] + other.direction[1],
                self.direction[2] + other.direction[2],
            ],
            axis: self.axis,
        }
        .normalized()
    }

    /// True when every channel is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.irradiance.iter().all(|value| value.is_finite())
            && self.direction.iter().all(|value| value.is_finite())
            && self.axis.iter().all(|value| value.is_finite())
    }
}

/// Why a lightmap build did not produce a usable atlas.
///
/// Every one of these makes the level rebuild with [`LightmapMode::Off`] and
/// draw the historical vertex-lit colours; none of them may leave a surface
/// black.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightmapFailure {
    /// The packer needed more pages than [`LightmapConfig::max_pages`].
    PageOverflow,
    /// A quad was degenerate or carried a non-finite corner.
    DegenerateQuad,
    /// A chart did not fit the page it was allocated on (internal layout bug).
    Layout,
    /// The fill pass returned the wrong number of texels for a chart.
    FillSize,
    /// An offline worker could not start or failed before returning its batch.
    Worker,
    /// The fill pass returned a non-finite colour.
    FillNonFinite,
    /// Directional encoding changed the integrated irradiance at the surface.
    TransportEnergy,
    /// The configured page/budget combination cannot describe an atlas.
    InvalidConfig,
    /// The static scene exceeded the transport solver's triangle budget.
    TransportScene,
    /// The renderer could not upload an atlas page.
    Upload,
}

impl LightmapFailure {
    /// Stable lowercase name, for logs and the developer report.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PageOverflow => "page overflow",
            Self::DegenerateQuad => "degenerate quad",
            Self::Layout => "atlas layout",
            Self::FillSize => "fill size",
            Self::Worker => "worker failure",
            Self::FillNonFinite => "non-finite fill",
            Self::TransportEnergy => "transport energy mismatch",
            Self::InvalidConfig => "invalid config",
            Self::TransportScene => "transport scene",
            Self::Upload => "page upload",
        }
    }
}

/// Largest shipped atlas page budget the world program can sample at once.
///
/// Each page occupies two RGBA16F array layers per prepared illumination group.
/// The vertex's page byte selects that pair; switchable contributions follow
/// the base group. The renderer shares this bound and allocates only resident
/// pages and groups. A plan beyond its profile budget fails explicitly instead
/// of dropping pages.
///
/// Full's measured coplanar model receiver plan at 32 samples/m reserves
/// 10,171,144 texels with inclusive endpoints and gutters. The deterministic
/// inline packer needs eleven pages to retain those physical shadow samples.
/// This costs at most 176 MiB for the base GPU group; package byte and switch-group
/// safety limits remain independent. Lower profiles retain their prior budget.
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize = 11;

/// Low and Medium retain their established eight-page atlas budgets.
/// Medium's 1024px base group costs at most 128 MiB; Low's 512px group 32 MiB.
pub const LIGHTMAP_ATLAS_LOWER_MAX_PAGES: usize = 8;

/// Smallest axis length, in metres, a patch may have.
///
/// Anything thinner than a tenth of a millimetre is not geometry a level can
/// meaningfully author; rejecting it keeps [`LightmapPatch::from_quad`]'s
/// inverse well-conditioned instead of dividing by an almost-zero determinant.
const MIN_PATCH_AXIS_M: f32 = 1.0e-4;

/// Smallest quad area, in square metres, a patch may have.
const MIN_PATCH_AREA_M2: f32 = 1.0e-8;

/// Which family of static surface a lightmap patch belongs to.
///
/// Architectural kinds retain their room-fill contract. Any folded triangle
/// uses a mirrored triangular domain; model triangles use physical transport
/// without authored room fill. All kinds participate in the same HDR atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchKind {
    Floor,
    Ceiling,
    Wall,
    Skirt,
    /// A real static model triangle or coplanar quad; physical transport without room fill.
    Prop,
}

impl PatchKind {
    /// Stable lowercase name, as used by dumps and logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Ceiling => "ceiling",
            Self::Wall => "wall",
            Self::Skirt => "skirt",
            Self::Prop => "prop",
        }
    }
}

/// One planar quad or folded triangle of static geometry, in world space.
///
/// The quad's native mesh uses triangles `(p0, p1, p2)` and `(p0, p2, p3)`
/// with square UV corners. Its exact mapping is therefore piecewise affine:
/// `origin + u*u_axis + v*v_axis + min(u,v)*diagonal_correction`.
/// Keeping the fourth corner matters for gable wall trapezoids: an affine
/// rectangle would sample metres away from their real receiver positions.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct LightmapPatch {
    /// World position of local `(u, v) = (0, 0)`.
    pub origin: [f32; 3],
    /// World vector spanned by `u` over `0..=1`.
    pub u_axis: [f32; 3],
    /// World vector spanned by `v` over `0..=1`.
    pub v_axis: [f32; 3],
    /// Difference between actual `p2` and the parallelogram's `p1 + p3 - p0`.
    /// Zero on rectangles and folded triangles; omitted from those records.
    #[serde(default, skip_serializing_if = "zero_vector")]
    pub diagonal_correction: [f32; 3],
    /// A repeated last corner emits one triangle with UVs `(0,0),(1,0),(0,1)`.
    /// Samples outside that triangle reflect across its diagonal onto real
    /// geometry. This domain is independent of the surface's lighting family.
    pub triangle: bool,
    /// Room hint for [`crate::lighting::LevelLighting::sample_in_room`].
    ///
    /// Transport baseline targets use this authoritative owner for every
    /// architectural family. Wall slices and lintels inherit their parent
    /// face's room so a doorway cannot redirect their fill into a neighbouring
    /// corridor. A patch without an owner resolves the room at each texel.
    pub room: Option<usize>,
    /// Which static surface family this patch covers.
    pub kind: PatchKind,
}

// Older model records omitted the topology flag and used the Prop kind for
// folded triangles. An explicit false must survive for new shared model quads.
fn deserialize_topology_flag<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <bool as serde::Deserialize>::deserialize(deserializer).map(Some)
}

impl<'de> serde::Deserialize<'de> for LightmapPatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Record {
            origin: [f32; 3],
            u_axis: [f32; 3],
            v_axis: [f32; 3],
            #[serde(default)]
            diagonal_correction: [f32; 3],
            #[serde(default, deserialize_with = "deserialize_topology_flag")]
            triangle: Option<bool>,
            room: Option<usize>,
            kind: PatchKind,
        }
        let record = <Record as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self {
            origin: record.origin,
            u_axis: record.u_axis,
            v_axis: record.v_axis,
            diagonal_correction: record.diagonal_correction,
            triangle: record.triangle.unwrap_or(record.kind == PatchKind::Prop),
            room: record.room,
            kind: record.kind,
        })
    }
}

/// Why a quad cannot become a lightmap patch.
///
/// See [`LightmapPatch::rejection`]: the two kinds are handled differently by
/// [`LightmapPlan`], because a sliver is invisible while a malformed quad is
/// not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchRejection {
    /// No usable area: a sub-millimetre axis, a near-zero area, or a
    /// non-finite coordinate on an otherwise floating-point-degenerate quad.
    /// The quad is invisible, so leaving it vertex-lit changes no pixel.
    Sliver,
    /// Visible but broken: a bow-tie or a wildly non-planar loop. It must not
    /// be silently left unlit, so the whole build fails over instead.
    Malformed,
}

impl LightmapPatch {
    /// Builds the patch for one planar quad, from its four world corners in
    /// the winding the mesh emits (`p0, p1, p2, p3` of the quad, counter-clockwise
    /// as a single loop).
    /// `u` runs `p0 -> p1` and `v` runs `p0 -> p3`. Returns `None` for a
    /// degenerate or non-finite quad: an axis shorter than
    /// [`MIN_PATCH_AXIS_M`], an area below [`MIN_PATCH_AREA_M2`], or any
    /// non-finite coordinate. A rejected quad never becomes a chart; the plan
    /// records the failure and the level falls back to vertex lighting, so a
    /// malformed vertex can never become a silently black patch.
    #[must_use]
    pub fn from_quad(kind: PatchKind, corners: [[f32; 3]; 4], room: Option<usize>) -> Option<Self> {
        if Self::rejection(&corners).is_some() {
            return None;
        }
        let [p0, p1, p2, p3] = corners;
        let u_axis = subtract(p1, p0);
        let v_axis = subtract(p3, p0);
        let triangle = corners_coincident(p2, p3);
        let diagonal_correction = if triangle {
            [0.0; 3]
        } else {
            subtract(subtract(p2, p1), v_axis)
        };
        Some(Self {
            origin: p0,
            u_axis,
            v_axis,
            diagonal_correction,
            triangle,
            room,
            kind,
        })
    }

    /// Why a quad cannot become a patch, or `None` when it can.
    ///
    /// The distinction matters to [`LightmapPlan`]: a [`PatchRejection::Sliver`]
    /// is *invisible* (an axis below [`MIN_PATCH_AXIS_M`], an area below
    /// [`MIN_PATCH_AREA_M2`], a non-finite coordinate on a zero-area quad), so
    /// skipping that one quad locally is unobservable and must not cost a level
    /// its whole lightmap. A [`PatchRejection::Malformed`] quad is visible but
    /// broken (a bow-tie or a wildly non-planar loop): leaving it unlit would be
    /// a visible error, so the build fails over to the exact vertex-lit mesh.
    #[must_use]
    pub fn rejection(corners: &[[f32; 3]; 4]) -> Option<PatchRejection> {
        if !corners
            .iter()
            .all(|corner| corner.iter().all(|value| value.is_finite()))
        {
            return Some(PatchRejection::Sliver);
        }
        let [p0, p1, p2, p3] = *corners;
        let u_axis = subtract(p1, p0);
        let v_axis = subtract(p3, p0);
        let u_len = length(u_axis);
        let v_len = length(v_axis);
        if u_len < MIN_PATCH_AXIS_M || v_len < MIN_PATCH_AXIS_M {
            return Some(PatchRejection::Sliver);
        }
        // The two triangles of the quad are (p0, p1, p2) and (p0, p2, p3), so a
        // patch is valid exactly when the crossed area of the frame is nonzero.
        let area = length(cross(u_axis, v_axis));
        if !area.is_finite() || area < MIN_PATCH_AREA_M2 {
            return Some(PatchRejection::Sliver);
        }
        // Both native triangles must be coplanar and retain the frame's
        // winding. Taper alone is not malformed: the exact mapping supports
        // unequal opposing edges without an arbitrary deformation limit.
        // Coincident last corners intentionally emit just one triangle.
        if !corners_coincident(p2, p3) {
            let diagonal = subtract(p2, p0);
            let frame_normal = cross(u_axis, v_axis);
            let first_normal = cross(u_axis, diagonal);
            let second_normal = cross(diagonal, v_axis);
            if dot(first_normal, frame_normal) <= 0.0
                || dot(second_normal, frame_normal) <= 0.0
                || dot(diagonal, frame_normal).abs() > area * u_len.hypot(v_len) * 1.0e-5
            {
                return Some(PatchRejection::Malformed);
            }
        }
        None
    }

    /// Whether this chart covers one triangle, independent of its family.
    /// Legacy model records acquire their missing flag during deserialization.
    #[must_use]
    pub const fn is_triangular(&self) -> bool {
        self.triangle
    }

    /// World position corresponding exactly to the mesh's UV interpolation.
    /// Coordinates are clamped to `0..=1`; a triangle's unused chart half
    /// reflects onto its real geometry to provide valid diagonal edge samples.
    #[must_use]
    pub fn point_at(&self, u: f32, v: f32) -> [f32; 3] {
        let mut unit_u = finite_unit(u);
        let mut unit_v = finite_unit(v);
        if self.is_triangular() && unit_u + unit_v > 1.0 {
            (unit_u, unit_v) = (1.0 - unit_v, 1.0 - unit_u);
        }
        let diagonal = if self.is_triangular() {
            0.0
        } else {
            unit_u.min(unit_v)
        };
        std::array::from_fn(|axis| {
            self.diagonal_correction
                .get(axis)
                .copied()
                .unwrap_or(0.0)
                .mul_add(
                    diagonal,
                    self.u_axis.get(axis).copied().unwrap_or(0.0).mul_add(
                        unit_u,
                        self.v_axis
                            .get(axis)
                            .copied()
                            .unwrap_or(0.0)
                            .mul_add(unit_v, self.origin.get(axis).copied().unwrap_or(0.0)),
                    ),
                )
        })
    }

    /// Local UV of a point projected onto the patch plane. Each half solves
    /// its own native triangle frame, then selects the half containing the UV.
    /// Unlike a bilinear inverse, this matches the GPU at every interior point.
    #[must_use]
    pub fn local_of(&self, point: [f32; 3]) -> (f32, f32) {
        let delta = subtract(point, self.origin);
        if self.is_triangular() {
            return local_in_frame(delta, self.u_axis, self.v_axis);
        }
        let first = local_in_frame(
            delta,
            self.u_axis,
            add(self.v_axis, self.diagonal_correction),
        );
        if first.0 >= first.1 {
            first
        } else {
            local_in_frame(
                delta,
                add(self.u_axis, self.diagonal_correction),
                self.v_axis,
            )
        }
    }

    /// Maximum physical span of each UV axis across both native triangles.
    /// A trapezoid's two opposing edges need not have the same length.
    #[must_use]
    pub fn extent_m(&self) -> (f32, f32) {
        let (u, v) = (length(self.u_axis), length(self.v_axis));
        if self.is_triangular() {
            (u, v)
        } else {
            (
                u.max(length(add(self.u_axis, self.diagonal_correction))),
                v.max(length(add(self.v_axis, self.diagonal_correction))),
            )
        }
    }

    /// True polygon area in square metres, summing the two native triangles.
    #[must_use]
    pub fn area_m2(&self) -> f32 {
        if self.is_triangular() {
            length(cross(self.u_axis, self.v_axis)) * 0.5
        } else {
            self.sample_area_m2(1.0, 0.0)
                .midpoint(self.sample_area_m2(0.0, 1.0))
        }
    }

    /// World area per unit UV area at this sample. Triangle charts mirror two
    /// UV halves onto one triangle, so each copy carries half the frame area.
    pub(crate) fn sample_area_m2(&self, u: f32, v: f32) -> f32 {
        if self.is_triangular() {
            length(cross(self.u_axis, self.v_axis)) * 0.5
        } else if u >= v {
            length(cross(
                self.u_axis,
                add(self.v_axis, self.diagonal_correction),
            ))
        } else {
            length(cross(
                add(self.u_axis, self.diagonal_correction),
                self.v_axis,
            ))
        }
    }
}

/// Serde omits the common rectangle/triangle defaults from dense metadata.
fn zero_vector(value: &[f32; 3]) -> bool {
    value
        .iter()
        .all(|component| component.to_bits().trailing_zeros() >= 31)
}

const fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Project a world delta into one well-conditioned planar triangle frame.
fn local_in_frame(delta: [f32; 3], u_axis: [f32; 3], v_axis: [f32; 3]) -> (f32, f32) {
    let uu = dot(u_axis, u_axis);
    let uv = dot(u_axis, v_axis);
    let vv = dot(v_axis, v_axis);
    let determinant = uu.mul_add(vv, -(uv * uv));
    if !determinant.is_finite() || determinant <= f32::EPSILON * uu * vv {
        return (0.0, 0.0);
    }
    let du = dot(delta, u_axis);
    let dv = dot(delta, v_axis);
    (
        vv.mul_add(du, -(uv * dv)) / determinant,
        uu.mul_add(dv, -(uv * du)) / determinant,
    )
}

/// One patch's rectangle inside one atlas page.
///
/// `x`, `y`, `width` and `height` are the *data* rectangle; the padding gutter
/// lives outside it and belongs to this chart alone. Texel `(i, j)` of the chart
/// is atlas texel `(x + i, y + j)`, row-major, with `j` increasing along `v`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Chart {
    /// Atlas page this chart lives on.
    pub page: u16,
    /// Texel origin of the chart's data rectangle.
    pub x: u32,
    /// Texel origin of the chart's data rectangle.
    pub y: u32,
    /// Data width, in texels.
    pub width: u32,
    /// Data height, in texels.
    pub height: u32,
}

impl Chart {
    /// Atlas UV of local `(u, v)` in `0..=1`, quantised for
    /// [`crate::render::Vertex::lightmap`].
    ///
    /// The baker samples both patch endpoints. Map those endpoints to the
    /// first and last texel centres, so bilinear reconstruction evaluates the
    /// same world positions. A one-texel axis maps to its sole centre.
    #[must_use]
    pub fn uv_at(&self, page_edge: u32, u: f32, v: f32) -> [u16; 2] {
        let edge = f32::from(u16::try_from(page_edge).unwrap_or(u16::MAX)).max(1.0);
        let x = f32::from(u16::try_from(self.x.min(page_edge)).unwrap_or(u16::MAX));
        let y = f32::from(u16::try_from(self.y.min(page_edge)).unwrap_or(u16::MAX));
        let width = f32::from(u16::try_from(self.width.saturating_sub(1)).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(self.height.saturating_sub(1)).unwrap_or(u16::MAX));
        let unit_u = if u.is_finite() {
            u.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let unit_v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        [
            quantize_uv(width.mul_add(unit_u, x + 0.5) / edge),
            quantize_uv(height.mul_add(unit_v, y + 0.5) / edge),
        ]
    }
}

/// Quantises an atlas UV in `0..=1` to the 16-bit fixed point the mesh stores.
///
/// The value is clamped first, so a NaN or an out-of-range coordinate becomes
/// the nearest legal sample instead of wrapping to the opposite edge of the
/// page — the same discipline [`crate::render::mesh`] applies to byte colours.
fn quantize_uv(value: f32) -> u16 {
    let clamped = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    // `clamped * 65535 + 0.5` is in [0.5, 65535.5], so the truncating cast only
    // drops the fraction and cannot leave the u16 range.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`clamped * 65535 + 0.5` is in [0.5, 65535.5], so the truncating cast only drops the fraction and cannot leave the u16 range."
    )]
    let scaled = (clamped.mul_add(65_535.0, 0.5)) as u32;
    u16::try_from(scaled).unwrap_or(u16::MAX)
}

/// Texel budget and atlas shape of one lightmap bake.
///
/// `Full` and `Low` bake the *same patch set*: a profile only decides how many
/// texels one metre of surface gets and how large a page may be. A level that
/// fits one 512-texel page at `Low` therefore packs into one or more 1024-texel
/// pages at `Full`, never into a different set of surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapConfig {
    /// Chart texels per world metre, both axes.
    pub texels_per_metre: f32,
    /// Edge length of one square atlas page, in texels.
    pub page_edge: u32,
    /// Hard cap on atlas pages; more than this is a build failure.
    pub max_pages: usize,
    /// Gutter texels surrounding every chart, filled by dilation.
    pub padding: u32,
    /// Bytes per atlas texel resident on the GPU: 16, i.e. two RGBA16F planes
    /// (irradiance and direction). Kept as a config field so the content key
    /// changes if the texel layout ever does.
    pub bytes_per_texel: u32,
}

/// Longest world span, in metres, one lightmap chart may cover.
///
/// Emitters cap their greedy merges with this, so every stamped patch fits a
/// page by construction. It is deliberately **the same number for both quality
/// profiles**: a profile decides texel density and page size, never where the
/// geometry is cut, which is what keeps `Full` and `Low` on one patch set.
///
/// The value reserves both inclusive endpoint samples and the Full profile's
/// two gutters: (1024 - 4 - 1) / 16 metres. At the shipped densities it is
/// never the binding constraint on the demo — its longest patch is 26.3 m — and
/// a level that authors one surface longer than this still splits into several
/// charts rather than failing: a chart wider than a page's usable edge would be
/// clamped by [`LightmapConfig::chart_texels`].
pub const MAX_CHART_SPAN_M: f32 = 63.6875;

impl LightmapConfig {
    /// The shipped settings of one quality profile.
    ///
    /// The chart-span cap ([`MAX_CHART_SPAN_M`]) is shared by both profiles, so
    /// the emitter cuts the world's surfaces in exactly the same places at both
    /// densities; only chart texel counts and page usage differ.
    ///
    /// Full retains architecture's 16 texels/m, matching the shared endpoint
    /// cap, and admits eleven pages for the measured finer model receivers.
    /// Low retains 10 texels/m, 512px pages and its established
    /// eight-page budget. Both preserve the same physical patches and gutters.
    #[must_use]
    pub const fn for_profile(profile: crate::quality::QualityProfile) -> Self {
        match profile {
            crate::quality::QualityProfile::Full => Self {
                texels_per_metre: 16.0,
                page_edge: 1_024,
                max_pages: LIGHTMAP_ATLAS_MAX_PAGES,
                padding: 2,
                bytes_per_texel: 16,
            },
            crate::quality::QualityProfile::Low => Self {
                texels_per_metre: 10.0,
                page_edge: 512,
                max_pages: LIGHTMAP_ATLAS_LOWER_MAX_PAGES,
                padding: 1,
                bytes_per_texel: 16,
            },
        }
    }

    /// Reserved and dilated gutter width for this physical receiver.
    /// Model charts need one texel for the atlas's single-mip bilinear sampler;
    /// architecture retains its established configured padding.
    #[must_use]
    pub const fn padding_for(&self, kind: PatchKind) -> u32 {
        if matches!(kind, PatchKind::Prop) && self.padding > 1 {
            1
        } else {
            self.padding
        }
    }

    /// Bound explicit receiver resolution without changing architectural
    /// allocation. Models may request twice the architectural density; large
    /// opaque surfaces and cutouts still select their established coarser rates.
    fn bounded_surface_density(&self, kind: PatchKind, requested: f32) -> Option<f32> {
        let maximum = self.texels_per_metre
            * if matches!(kind, PatchKind::Prop) {
                2.0
            } else {
                1.0
            };
        if !maximum.is_finite() || maximum <= 0.0 || !requested.is_finite() || requested <= 0.0 {
            None
        } else {
            Some(requested.min(maximum))
        }
    }

    /// Texels of one page a chart's data rectangle may use, after both gutters.
    #[must_use]
    pub const fn usable_edge(&self) -> u32 {
        self.page_edge
            .saturating_sub(self.padding.saturating_mul(2))
    }

    /// Texel size of one patch's chart, both axes at least one texel.
    ///
    /// The result is clamped to [`Self::usable_edge`], so a chart can always be
    /// placed on an empty page; the emitter's chart-span cap means real geometry
    /// never actually needs the clamp.
    #[must_use]
    pub fn chart_texels(&self, patch: &LightmapPatch) -> (u32, u32) {
        let (u_metres, v_metres) = patch.extent_m();
        let cap = self.usable_edge().max(1);
        (
            texels_for(u_metres, self.texels_per_metre, cap),
            texels_for(v_metres, self.texels_per_metre, cap),
        )
    }

    /// Longest world span, in metres, a chart can cover at any density.
    ///
    /// Always [`MAX_CHART_SPAN_M`]: the profiles differ in texel density and
    /// page size, never in the patch set the emitters produce.
    #[must_use]
    pub const fn max_chart_span_m(&self) -> f32 {
        MAX_CHART_SPAN_M
    }
}

/// Endpoint samples one axis needs for `metres` of world surface, capped.
/// A density describes intervals per metre; inclusive endpoints need one
/// additional sample. A positive, finite span therefore has at least two.
fn texels_for(metres: f32, texels_per_metre: f32, cap: u32) -> u32 {
    if !metres.is_finite()
        || !texels_per_metre.is_finite()
        || metres <= 0.0
        || texels_per_metre <= 0.0
    {
        return 1;
    }
    let requested = (metres * texels_per_metre).ceil().max(1.0) + 1.0;
    let cap_f = f32::from(u16::try_from(cap).unwrap_or(u16::MAX)).max(1.0);
    let capped = requested.min(cap_f);
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The finite integral page dimension is capped by a u16 page edge before conversion."
    )]
    // `capped` is finite and in [1, cap], and cap is a page edge bounded by u16.
    let value = capped as u32;
    value.clamp(1, cap)
}

/// `a - b` for two world positions.
/// True when two corners are the same point, within the patch builder's
/// tolerance.
///
/// Used to recognise the folded-triangle quad form (a triangle emitted in the
/// quad form with its last corner repeated), which has no fourth corner and so
/// skips the closing check.
#[must_use]
pub fn corners_coincident(a: [f32; 3], b: [f32; 3]) -> bool {
    let d = subtract(a, b);
    dot(d, d) <= 1.0e-12
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a + b` for two world vectors.
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The length of a world vector.
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

/// The dot product of two world vectors.
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

/// The cross product of two world vectors.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

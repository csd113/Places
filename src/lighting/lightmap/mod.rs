//! Real baked static lightmaps for the static world geometry.
//!
//! The lightmap path adds one thing on top of the baked-vertex lighting model:
//! the light that the vertex-colour path carries is instead baked into a texel
//! atlas, and the fragment shader multiplies the surface texture by the atlas
//! texel. Nothing about the *lighting model* changes — every texel is one
//! [`crate::lighting::LevelLighting::sample_in_room`] call — only where the
//! result is stored.
//!
//! ```text
//! mesh emitter                    atlas builder (bake time)
//!    quad -> LightmapPatch      fill_chart() -> chart.width x chart.height RGB
//!         -> Chart                 -> page texels + dilated gutter
//!         -> Vertex::lightmap       -> one RGB8 texture per page
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
//! mod.rs     the frozen types shared by the emitter, the fill pass and the renderer
//! atlas.rs   the deterministic shelf packer, atlas pages and PNG debugging
//! plan.rs    the per-level plan built while the mesh is emitted
//! fill.rs    the per-texel light evaluation, called by the atlas builder
//! cache.rs   the deterministic content key and the level lightmap cache
//! tests.rs   unit tests for the whole tree
//! ```

mod atlas;
mod cache;
pub(crate) mod fill;
mod plan;

#[cfg(test)]
mod continuity;
#[cfg(test)]
mod tests;

pub use atlas::{LightmapAtlas, LightmapPage, ShelfAllocator, page_png_bytes, write_page_png};
pub use cache::{LIGHTMAP_FORMAT_VERSION, LightmapCache, content_key, content_key_with_extra};
pub use fill::fill_chart;
pub use plan::{LevelLightmaps, LightmapMode, LightmapPlan, LightmapStats};

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
    /// The fill pass returned a non-finite colour.
    FillNonFinite,
    /// The configured page/budget combination cannot describe an atlas.
    InvalidConfig,
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
            Self::FillNonFinite => "non-finite fill",
            Self::InvalidConfig => "invalid config",
            Self::Upload => "page upload",
        }
    }
}

/// Number of atlas pages the world program can sample at once.
///
/// Two texture units are bound for every world draw and the vertex's page byte
/// selects between them, so a bake that needs more than this cannot render its
/// lightmaps correctly and must fall back to vertex lighting instead of
/// dropping pages silently. Mirrors `render::view::LIGHTMAP_PAGE_SLOTS`.
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize = 2;

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
/// The kind has no effect on packing or sampling: it exists for diagnostics,
/// cache reports and tests that want to say what a chart covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchKind {
    Floor,
    Ceiling,
    Wall,
    Skirt,
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
        }
    }
}

/// One planar rectangle of static geometry, in world space.
///
/// Local `(u, v)` in `[0, 1]²` maps to world `origin + u*u_axis + v*v_axis`, and
/// the emitter writes exactly those local coordinates into its vertices, in the
/// order the quad itself winds (`p0 -> p1` is `u`, `p0 -> p3` is `v`). Because
/// the mesh uses the quad's own frame, a floor's texel grid aligns with the
/// world and a wall face is neither stretched nor mirrored relative to the
/// geometry it covers.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LightmapPatch {
    /// World position of local `(u, v) = (0, 0)`.
    pub origin: [f32; 3],
    /// World vector spanned by `u` over `0..=1`.
    pub u_axis: [f32; 3],
    /// World vector spanned by `v` over `0..=1`.
    pub v_axis: [f32; 3],
    /// Room hint for [`crate::lighting::LevelLighting::sample_in_room`].
    pub room: Option<usize>,
    /// Which static surface family this patch covers.
    pub kind: PatchKind,
}

impl LightmapPatch {
    /// Builds the patch for one planar quad, from its four world corners in the
    /// winding the mesh emits (`p0, p1, p2, p3` of the quad, counter-clockwise
    /// as a single loop).
    ///
    /// `u` runs `p0 -> p1` and `v` runs `p0 -> p3`. Returns `None` for a
    /// degenerate or non-finite quad: an axis shorter than
    /// [`MIN_PATCH_AXIS_M`], an area below [`MIN_PATCH_AREA_M2`], or any
    /// non-finite coordinate. A rejected quad never becomes a chart; the plan
    /// records the failure and the level falls back to vertex lighting, so a
    /// malformed vertex can never become a silently black patch.
    #[must_use]
    pub fn from_quad(kind: PatchKind, corners: [[f32; 3]; 4], room: Option<usize>) -> Option<Self> {
        if !corners
            .iter()
            .all(|corner| corner.iter().all(|value| value.is_finite()))
        {
            return None;
        }
        let [p0, p1, p2, p3] = corners;
        let u_axis = subtract(p1, p0);
        let v_axis = subtract(p3, p0);
        let u_len = length(u_axis);
        let v_len = length(v_axis);
        if u_len < MIN_PATCH_AXIS_M || v_len < MIN_PATCH_AXIS_M {
            return None;
        }
        // The two triangles of the quad are (p0, p1, p2) and (p0, p2, p3), so a
        // patch is valid exactly when the crossed area of the frame is nonzero.
        let area = length(cross(u_axis, v_axis));
        if !area.is_finite() || area < MIN_PATCH_AREA_M2 {
            return None;
        }
        // Guard against a bow-tie or a wildly non-planar quad: the fourth
        // corner has to close the loop within its own frame.
        let closing = subtract(add(add(p0, u_axis), v_axis), p2);
        let diag = u_len.hypot(v_len);
        if length(closing) > diag * 0.5 {
            return None;
        }
        Some(Self {
            origin: p0,
            u_axis,
            v_axis,
            room,
            kind,
        })
    }

    /// World position of local `(u, v)`, `origin + u_axis*u + v_axis*v`, with
    /// both coordinates clamped to `0..=1`.
    #[must_use]
    pub fn point_at(&self, u: f32, v: f32) -> [f32; 3] {
        let u = if u.is_finite() {
            u.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        [
            self.u_axis[0].mul_add(u, self.v_axis[0].mul_add(v, self.origin[0])),
            self.u_axis[1].mul_add(u, self.v_axis[1].mul_add(v, self.origin[1])),
            self.u_axis[2].mul_add(u, self.v_axis[2].mul_add(v, self.origin[2])),
        ]
    }

    /// Local `(u, v)` of a world point projected onto the patch's plane, by
    /// solving the 2x2 normal equations of the frame.
    ///
    /// A point off the plane (a texel of a sloped gable patch, say) is
    /// projected onto it, which is what the fill pass wants. A degenerate frame
    /// returns `(0, 0)` rather than a division by zero; [`Self::from_quad`]
    /// never produces one.
    #[must_use]
    pub fn local_of(&self, point: [f32; 3]) -> (f32, f32) {
        let d = subtract(point, self.origin);
        let a = dot(self.u_axis, self.u_axis);
        let b = dot(self.u_axis, self.v_axis);
        let c = dot(self.v_axis, self.v_axis);
        let det = a.mul_add(c, -(b * b));
        if !det.is_finite() || det.abs() <= f32::EPSILON {
            return (0.0, 0.0);
        }
        let du = dot(d, self.u_axis);
        let dv = dot(d, self.v_axis);
        (
            c.mul_add(du, -(b * dv)) / det,
            a.mul_add(dv, -(b * du)) / det,
        )
    }

    /// The two axis lengths, in metres: `(u length, v length)`.
    ///
    /// The chart's texel size is derived from these, so a curved or skewed quad
    /// still gets a texel density close to the configured one on both axes.
    #[must_use]
    pub fn extent_m(&self) -> (f32, f32) {
        (length(self.u_axis), length(self.v_axis))
    }
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
    /// The mapping is the frozen contract: local `u` spans the chart's data
    /// rectangle, `[0, 1] -> [x, x + width]`, and the shader divides by
    /// `page_edge` again. Coordinates are clamped, so a caller can never ask for
    /// a sample outside the chart's own gutter.
    #[must_use]
    pub fn uv_at(&self, page_edge: u32, u: f32, v: f32) -> [u16; 2] {
        let edge = f32::from(u16::try_from(page_edge).unwrap_or(u16::MAX)).max(1.0);
        let x = f32::from(u16::try_from(self.x.min(page_edge)).unwrap_or(u16::MAX));
        let y = f32::from(u16::try_from(self.y.min(page_edge)).unwrap_or(u16::MAX));
        let width = f32::from(u16::try_from(self.width).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(self.height).unwrap_or(u16::MAX));
        let u = if u.is_finite() {
            u.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        [
            quantize_uv(width.mul_add(u, x) / edge),
            quantize_uv(height.mul_add(v, y) / edge),
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
    /// Bytes per atlas texel: 3 for the RGB8 pages the renderer uploads.
    pub bytes_per_texel: u32,
}

/// Longest world span, in metres, one lightmap chart may cover.
///
/// Emitters cap their greedy merges with this, so every stamped patch fits a
/// page by construction. It is deliberately **the same number for both quality
/// profiles**: a profile decides texel density and page size, never where the
/// geometry is cut, which is what keeps `Full` and `Low` on one patch set.
///
/// The value is the tighter of the two page budgets — `Low`'s 512-texel page
/// minus its gutters (510 texels) at its 8 texels per metre — because a chart
/// that fits there fits the denser `Full` page too (63.75 m at 12 texels per
/// metre is 765 texels against a 1020-texel usable edge).
pub const MAX_CHART_SPAN_M: f32 = 63.75;

impl LightmapConfig {
    /// The shipped settings of one quality profile.
    ///
    /// Both profiles target the same texel footprint in metres (`usable_edge /
    /// texels_per_metre` is identical), so the emitter's chart-span cap splits
    /// geometry in exactly the same places at both densities; only chart texel
    /// counts and page usage differ.
    #[must_use]
    pub const fn for_profile(profile: crate::quality::QualityProfile) -> Self {
        match profile {
            crate::quality::QualityProfile::Full => Self {
                texels_per_metre: 12.0,
                page_edge: 1_024,
                max_pages: LIGHTMAP_ATLAS_MAX_PAGES,
                padding: 2,
                bytes_per_texel: 3,
            },
            crate::quality::QualityProfile::Low => Self {
                texels_per_metre: 8.0,
                page_edge: 512,
                max_pages: LIGHTMAP_ATLAS_MAX_PAGES,
                padding: 1,
                bytes_per_texel: 3,
            },
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

/// Texels one axis of a chart needs for `metres` of world surface, capped.
fn texels_for(metres: f32, texels_per_metre: f32, cap: u32) -> u32 {
    if !metres.is_finite() || !texels_per_metre.is_finite() {
        return 1;
    }
    let requested = (metres * texels_per_metre).ceil().max(1.0);
    let cap_f = f32::from(u16::try_from(cap).unwrap_or(u16::MAX)).max(1.0);
    let capped = requested.min(cap_f);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `capped` is finite and in [1, cap], and cap is a page edge bounded by u16.
    let value = capped as u32;
    value.clamp(1, cap)
}

/// `a - b` for two world positions.
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

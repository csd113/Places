//! The per-level lightmap plan built while the static mesh is emitted.
//!
//! The mesh emitter stamps every lightmapped quad as it writes its six vertices:
//!
//! ```text
//! add_quad(scratch, p0, p1, p2, p3)          // the historical emit call
//!     LIGHTMAPS: LightmapPatch::from_quad(p0, p1, p2, p3)   (u = p0->p1, v = p0->p3)
//!                ChartAllocator::allocate(patch) -> Chart
//!                Vertex::lightmap = Chart::uv_at(0,0), (1,0), (1,1), (0,1)
//!                Vertex::lightmap_page = chart.page
//! ```
//!
//! **Orientation is frozen here:** a quad's own winding defines its texture frame
//! — `u` runs `p0 -> p1` and `v` runs `p0 -> p3`, in the exact order the quad's
//! six vertices were passed to [`crate::render::Vertex`]'s emit path. The fill
//! pass reads exactly the same frame through [`LightmapPatch::point_at`]. Nothing
//! may rotate, mirror or rescale a patch relative to its quad; a floor whose
//! quad winds `(x0,z0) -> (x1,z0) -> ...` therefore gets a texel grid aligned
//! with the world, not a rotated one.
//!
//! Chart allocation is inline (one forward pass, best-short-side fit across the
//! open pages, pages in emission order) rather than deferred, because the
//! vertices need their final UVs as they are written and a deferred pass would
//! have to find them again after spatial bucketing and index sharing.
//! Determinism comes from the emitter's fixed order plus the allocator's
//! placement order: the same level always produces the same charts and the
//! same pages.

use super::{
    Chart, ChartAllocator, LightmapConfig, LightmapFailure, LightmapPage, LightmapPatch, PatchKind,
    PatchRejection, corners_coincident,
};
use crate::render::{LIGHTMAP_NONE, Vertex};

/// Whether a level build bakes and draws real lightmaps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightmapMode {
    /// The historical path: baked light is folded into every vertex colour.
    #[default]
    Off,
    /// Bake a lightmap atlas and draw surfaces with the vertex-lit colours kept
    /// as the tint/face-shade term.
    On,
}

/// What one successful lightmap bake produced, in numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightmapStats {
    /// Charts baked (one per lightmapped quad).
    pub charts: usize,
    /// Atlas pages resident.
    pub pages: usize,
    /// Chart data texels the fill pass wrote.
    pub texels: usize,
    /// Page texels uploaded, including gutters and unused page space.
    pub page_texels: usize,
    /// Wall-clock cost of filling and packing the atlas, in milliseconds.
    pub bake_millis: f64,
    /// True when these lightmaps came from the on-disk cache rather than a bake.
    pub cache_hit: bool,
}

/// Everything a drawn level needs to sample its baked light.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelLightmaps {
    /// One RGB8 page per atlas page, in page order.
    pub pages: Vec<LightmapPage>,
    /// Every chart, paired with the patch it covers, in plan order.
    pub charts: Vec<(LightmapPatch, Chart)>,
    /// What the bake cost and produced.
    pub stats: LightmapStats,
    /// Deterministic content key of the inputs this atlas was baked from.
    pub cache_key: String,
}

impl LevelLightmaps {
    /// Number of charts in the plan.
    #[must_use]
    pub const fn chart_count(&self) -> usize {
        self.charts.len()
    }
}

/// The charts and pages accumulated while one level's geometry is emitted.
///
/// A plan is only valid while [`Self::failed`] is false. A failure is sticky and
/// named: the caller must not bake or upload the plan, and rebuilds the level
/// with [`LightmapMode::Off`] instead.
#[derive(Debug)]
pub struct LightmapPlan {
    config: LightmapConfig,
    allocator: ChartAllocator,
    charts: Vec<(LightmapPatch, Chart)>,
    failure: Option<LightmapFailure>,
    /// Invisible sliver quads left vertex-lit (see [`Self::slivers_skipped`]).
    slivers_skipped: usize,
}

impl LightmapPlan {
    /// An empty plan for one level build.
    #[must_use]
    pub const fn new(config: LightmapConfig) -> Self {
        Self {
            allocator: ChartAllocator::new(config),
            config,
            charts: Vec::new(),
            failure: None,
            slivers_skipped: 0,
        }
    }

    /// Number of invisible sliver quads this plan left vertex-lit.
    ///
    /// A sliver is thinner than a lightmap texel can resolve and has no visible
    /// area, so skipping it is unobservable; the count is reported so a level
    /// author can still find the geometry that produced it.
    #[must_use]
    pub const fn slivers_skipped(&self) -> usize {
        self.slivers_skipped
    }

    /// Longest world span a merged quad may cover at this density.
    ///
    /// Emitters cap their merges with this so every stamped patch fits a page
    /// without post-hoc subdivision. It is identical for `Full` and `Low`.
    #[must_use]
    pub const fn max_chart_span_m(&self) -> f32 {
        self.config.max_chart_span_m()
    }

    /// True when a quad could not be charted; the plan must not be baked.
    #[must_use]
    pub const fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// Why the plan failed, if it did.
    #[must_use]
    pub const fn failure(&self) -> Option<LightmapFailure> {
        self.failure
    }

    /// Every chart placed so far, with its patch, in emission order.
    #[must_use]
    pub fn charts(&self) -> &[(LightmapPatch, Chart)] {
        &self.charts
    }

    /// Number of charts placed so far.
    #[must_use]
    pub const fn chart_count(&self) -> usize {
        self.charts.len()
    }

    /// Number of pages the allocator has opened so far.
    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.allocator.page_count()
    }

    /// Stamps one just-emitted quad: builds its patch, allocates a chart and
    /// writes the chart's atlas UVs into the quad's six vertices.
    ///
    /// `vertices` is the emitter's scratch run and `first` is the index of the
    /// quad's first vertex (`vertices.len()` before the emit call). `corners`
    /// are the quad's four corners in its own winding, in the same order they
    /// were passed to the emit call: `u = p0 -> p1`, `v = p0 -> p3`. The six
    /// vertices repeat `p0, p1, p2, p0, p2, p3`; the corner index for each is
    /// `[0, 1, 2, 0, 2, 3]`.
    ///
    /// Returns `true` when the quad was charted. A degenerate quad or a full
    /// page budget records a named failure and leaves the vertices vertex-lit;
    /// the caller then discards the whole build.
    pub fn stamp_emitted(
        &mut self,
        vertices: &mut [Vertex],
        first: usize,
        kind: PatchKind,
        corners: [[f32; 3]; 4],
        room: Option<usize>,
    ) -> bool {
        let Some(patch) = LightmapPatch::from_quad(kind, corners, room) else {
            if LightmapPatch::rejection(&corners) == Some(PatchRejection::Sliver) {
                // An invisible sliver: leave these six vertices vertex-lit and
                // keep every other chart in the level. A level whose content
                // produces one sub-millimetre trim sliver must not lose its
                // whole lightmap, which is what treating this as a build
                // failure used to do.
                self.slivers_skipped = self.slivers_skipped.saturating_add(1);
            } else {
                // A visible malformed quad: fail over to the exact vertex-lit
                // mesh rather than drawing one unlit surface.
                self.fail(LightmapFailure::DegenerateQuad);
            }
            return false;
        };
        let Some(chart) = self.allocator.allocate(&patch) else {
            // `allocate` only fails on the page budget (see `ChartAllocator`).
            self.fail(LightmapFailure::PageOverflow);
            return false;
        };
        let edge = self.config.page_edge;
        // A folded-triangle quad repeats its last corner: the third corner is
        // the triangle's second edge (`v = 1`) and carries no `u`, so its chart
        // coordinate must not be the quad's `(1, 1)`.
        let folded = corners_coincident(corners[2], corners[3]);
        let uvs = if folded {
            [
                chart.uv_at(edge, 0.0, 0.0),
                chart.uv_at(edge, 1.0, 0.0),
                chart.uv_at(edge, 0.0, 1.0),
                chart.uv_at(edge, 0.0, 1.0),
            ]
        } else {
            [
                chart.uv_at(edge, 0.0, 0.0),
                chart.uv_at(edge, 1.0, 0.0),
                chart.uv_at(edge, 1.0, 1.0),
                chart.uv_at(edge, 0.0, 1.0),
            ]
        };
        let page = u8::try_from(chart.page).unwrap_or(LIGHTMAP_NONE);
        for (offset, corner) in [0usize, 1, 2, 0, 2, 3].into_iter().enumerate() {
            let Some(vertex) = vertices.get_mut(first.saturating_add(offset)) else {
                self.fail(LightmapFailure::Layout);
                return false;
            };
            vertex.lightmap = uvs.get(corner).copied().unwrap_or([0, 0]);
            vertex.lightmap_page = page;
        }
        self.charts.push((patch, chart));
        true
    }

    /// Records the first build failure, with the capacity numbers, so a level
    /// that loses its atlas is never silent.
    ///
    /// The named reason is printed once per process per failure kind: the whole
    /// level then rebuilds with the historical vertex-lit mesh. `charts placed`
    /// and `pages used of max_pages` make a genuine budget shortfall
    /// distinguishable from a layout bug.
    fn fail(&mut self, failure: LightmapFailure) {
        if self.failure.is_none() {
            crate::logging::warn_once(
                format!("lightmap-plan-failure:{}", failure.name()),
                format!(
                    "[lightmaps] atlas plan failed ({}): {} chart(s) placed on {} of {} page(s); \
                     rebuilding the level with vertex lighting",
                    failure.name(),
                    self.charts.len(),
                    self.allocator.page_count(),
                    self.config.max_pages,
                ),
            );
        }
        self.failure.get_or_insert(failure);
    }
}

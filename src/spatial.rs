//! Coarse spatial partitioning and view-frustum culling for static geometry.
//!
//! The renderer used to submit one draw per material for the whole level, so a
//! camera pointed away from a prop field still paid almost the full vertex cost
//! (measured on the `PocketCHIP`: 400 chairs behind the camera cost ~28 ms against
//! ~36 ms in front of it). This module provides the two pieces needed to stop
//! that: a world-space axis-aligned bounding box per render batch, and a
//! conservative box/frustum intersection test.
//!
//! The partitioning is deliberately simple — a uniform X/Z grid, no Y
//! subdivision, no hierarchy, no occlusion queries, nothing beyond what OpenGL
//! ES 2.0 needs. A cell is a column of space that covers the full height of
//! whatever stands in it, which stays correct for elevated rooms, gable
//! ceilings and recessed floors: `LevelSurfaces` supplies the vertical extent,
//! and batching only needs the X/Z footprint. Rooms stacked at the same X/Z
//! share a cell, so they share draw ranges; that is a batching granularity
//! limit, not a correctness one.
//!
//! Everything here is `no_std`-friendly pure math plus one `HashMap`-backed
//! builder; nothing allocates per frame.

use std::collections::HashMap;
use std::hash::Hash;

/// World-space axis-aligned bounding box.
///
/// There is deliberately no derived `Default`: `[0.0; 3]` for both corners is a
/// degenerate box *at the origin*, which would silently make every bounds
/// computation include the world origin and make culling far weaker than it
/// looks. The neutral element is [`Aabb::EMPTY`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    /// The empty box: min above max, so any point expands it to a real box.
    pub const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    /// A box containing exactly one point (non-finite input is ignored).
    #[must_use]
    pub fn from_point(point: [f32; 3]) -> Self {
        Self::EMPTY.expanded(point)
    }

    /// Grows the box (in place) to contain `point`. Non-finite coordinates are
    /// ignored so a malformed level cannot poison a batch's bounds into NaN and
    /// make it disappear or, worse, always pass the frustum test.
    pub fn expand(&mut self, point: [f32; 3]) {
        for ((min, max), value) in self
            .min
            .iter_mut()
            .zip(self.max.iter_mut())
            .zip(point.iter())
        {
            if value.is_finite() {
                *min = min.min(*value);
                *max = max.max(*value);
            }
        }
    }

    /// `expand` in builder form.
    #[must_use]
    pub fn expanded(mut self, point: [f32; 3]) -> Self {
        self.expand(point);
        self
    }

    /// Smallest box containing both inputs.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut result = *self;
        result.expand(other.min);
        result.expand(other.max);
        result
    }

    /// True when no finite point was ever added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.min
            .iter()
            .zip(self.max.iter())
            .any(|(min, max)| min > max)
    }

    #[must_use]
    pub const fn centre(&self) -> [f32; 3] {
        [
            f32::midpoint(self.min[0], self.max[0]),
            f32::midpoint(self.min[1], self.max[1]),
            f32::midpoint(self.min[2], self.max[2]),
        ]
    }
}

/// Integer X/Z grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellKey {
    pub x: i32,
    pub z: i32,
}

/// Largest/deepest cell coordinate accepted from level data.
///
/// `f32 -> i32` saturates anyway, but clamping explicitly keeps cell keys stable
/// and comparable for absurd coordinates instead of relying on cast saturation.
const CELL_LIMIT: i32 = 1 << 20;

/// [`CELL_LIMIT`] as an `f32`. `1 << 20` is a power of two, so this is exact.
const CELL_LIMIT_F32: f32 = 1_048_576.0;

/// Smallest useful cell size, in metres.
///
/// The grid is a *partition*, so the number of static draw batches a level can
/// ever produce is bounded by `cells x surface kinds`. That bound is what keeps
/// the optimisation from back-firing into hundreds of tiny draw calls, and this
/// constant is the culling-granularity end of the trade.
pub const SPATIAL_CELL_MIN_METRES: f32 = 12.0;

/// Largest cell size, in metres. Without a ceiling, a very large level would
/// collapse into one batch per material and lose culling entirely.
pub const SPATIAL_CELL_MAX_METRES: f32 = 40.0;

/// Roughly how many cells should span each axis of a level's geometry.
///
/// Levels smaller than `TARGET * MIN` metres keep the minimum cell size; larger
/// ones grow their cells so the cell count (and therefore the batch count) stays
/// bounded instead of growing with the square of the level.
pub const SPATIAL_CELL_TARGET_PER_AXIS: f32 = 8.0;

/// Per-axis world-space cell grid anchored at the world origin.
///
/// A corridor 200 m long and 20 m wide needs fine cells along its length and
/// coarse ones across it, so the two axes are sized independently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellGrid {
    pub x: f32,
    pub z: f32,
}

impl CellGrid {
    /// A square grid with exactly the requested cell size.
    ///
    /// Used by the `LIMINAL_CELL_METRES` tuning override, which must be able to
    /// sweep *outside* the adaptive range: the clamp below is only a guard
    /// against nonsense values, not a policy.
    #[must_use]
    pub fn uniform(cell_metres: f32) -> Self {
        let size = if cell_metres.is_finite() && cell_metres >= 0.5 {
            cell_metres.min(4096.0)
        } else {
            SPATIAL_CELL_MIN_METRES
        };
        Self { x: size, z: size }
    }

    /// A grid sized so a level of this extent keeps a bounded cell count.
    ///
    /// The cell size is the level's extent over
    /// [`SPATIAL_CELL_TARGET_PER_AXIS`], clamped so a small level keeps a useful
    /// culling granularity and a huge one does not become one giant batch.
    #[must_use]
    pub fn for_extent(extent_x: f32, extent_z: f32) -> Self {
        Self {
            x: adaptive_cell(extent_x),
            z: adaptive_cell(extent_z),
        }
    }

    /// The cell containing `point`. Non-finite coordinates fall back to cell
    /// (0, 0) rather than producing an unusable key.
    #[must_use]
    pub fn cell_of(&self, point: [f32; 3]) -> CellKey {
        CellKey {
            x: cell_axis(point[0], self.x),
            z: cell_axis(point[2], self.z),
        }
    }

    /// Cell size to report to the developer log.
    #[must_use]
    pub fn describe(&self) -> String {
        if (self.x - self.z).abs() < 1e-3 {
            format!("{:.0} m", self.x)
        } else {
            format!("{:.0}x{:.0} m", self.x, self.z)
        }
    }
}

impl Default for CellGrid {
    fn default() -> Self {
        Self::uniform(SPATIAL_CELL_MIN_METRES)
    }
}

/// Chooses the adaptive cell size for one axis of a level of this extent.
fn adaptive_cell(extent: f32) -> f32 {
    if !extent.is_finite() || extent <= 0.0 {
        return SPATIAL_CELL_MIN_METRES;
    }
    (extent / SPATIAL_CELL_TARGET_PER_AXIS).clamp(SPATIAL_CELL_MIN_METRES, SPATIAL_CELL_MAX_METRES)
}

/// Convenience wrapper for a square grid, used by tests and simple callers.
#[must_use]
pub fn cell_of(point: [f32; 3], cell_metres: f32) -> CellKey {
    CellGrid::uniform(cell_metres).cell_of(point)
}

fn cell_axis(value: f32, cell_metres: f32) -> i32 {
    if !value.is_finite() || !cell_metres.is_finite() || cell_metres <= 0.0 {
        return 0;
    }
    let index = (value / cell_metres).floor();
    if index <= -CELL_LIMIT_F32 {
        -CELL_LIMIT
    } else if index >= CELL_LIMIT_F32 {
        CELL_LIMIT
    } else {
        // `index` is inside `(-2^20, 2^20)` here, so it is an integral value
        // that fits `i32` exactly: the cast neither truncates nor saturates.
        #[allow(clippy::cast_possible_truncation)]
        {
            index as i32
        }
    }
}

/// View frustum as six world-space half-space planes.
///
/// Each plane is `(a, b, c, d)` with the convention that a point `p` is inside
/// when `a*p.x + b*p.y + c*p.z + d >= 0`. The extraction follows Gribb–Hartmann
/// and is configured for the depth convention the renderer actually uses
/// (see [`DepthRange`]).
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

/// Clip-space depth convention of the projection the frustum is extracted from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthRange {
    /// `z_ndc` in `[-1, 1]`: the classic OpenGL convention
    /// (`glam::Mat4::perspective_rh_gl`).
    NegativeOneToOne,
    /// `z_ndc` in `[0, 1]`: what `glam::Mat4::perspective_rh` produces, and what
    /// this renderer uses.
    ZeroToOne,
}

impl Frustum {
    /// Extracts the six planes from a combined view-projection matrix.
    ///
    /// `mvp` is the same matrix uploaded to `u_mvp`, so the frustum always
    /// matches what the GPU is about to clip against — including pitch, roll,
    /// an odd aspect ratio or a resized drawable.
    #[must_use]
    pub fn from_view_projection(mvp: &glam::Mat4, depth: DepthRange) -> Self {
        // glam stores matrices column-major; rows are the transposed axes.
        let row = |index: usize| -> [f32; 4] {
            [
                mvp.col(0)[index],
                mvp.col(1)[index],
                mvp.col(2)[index],
                mvp.col(3)[index],
            ]
        };
        let row0 = row(0);
        let row1 = row(1);
        let row2 = row(2);
        let row3 = row(3);

        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];

        let near = match depth {
            DepthRange::NegativeOneToOne => add(row3, row2),
            DepthRange::ZeroToOne => row2,
        };

        Self {
            planes: [
                add(row3, row0), // left:   x_clip >= -w
                sub(row3, row0), // right:  x_clip <=  w
                add(row3, row1), // bottom: y_clip >= -w
                sub(row3, row1), // top:    y_clip <=  w
                near,            // near
                sub(row3, row2), // far:    z_clip <=  w
            ],
        }
    }

    /// Builds a frustum from explicit planes (tests and callers that already
    /// have them).
    #[must_use]
    pub const fn from_planes(planes: [[f32; 4]; 6]) -> Self {
        Self { planes }
    }

    #[must_use]
    pub const fn planes(&self) -> &[[f32; 4]; 6] {
        &self.planes
    }

    /// Conservative box/frustum test.
    ///
    /// Returns `false` only when the box is *definitely* fully outside at least
    /// one plane. A box that straddles a plane, or that contains the camera,
    /// always returns `true`, so this can never cull visible geometry.
    #[must_use]
    pub fn intersects_aabb(&self, bounds: &Aabb) -> bool {
        if bounds.is_empty() {
            // Nothing to draw; treat as "not visible" so it costs no draw call.
            return false;
        }
        for plane in &self.planes {
            // The box's furthest corner along the plane normal: if even that is
            // behind the plane, every corner is.
            let furthest = [
                if plane[0] >= 0.0 {
                    bounds.max[0]
                } else {
                    bounds.min[0]
                },
                if plane[1] >= 0.0 {
                    bounds.max[1]
                } else {
                    bounds.min[1]
                },
                if plane[2] >= 0.0 {
                    bounds.max[2]
                } else {
                    bounds.min[2]
                },
            ];
            let distance = plane[2].mul_add(
                furthest[2],
                plane[1].mul_add(furthest[1], plane[0] * furthest[0]),
            ) + plane[3];
            if distance < 0.0 {
                return false;
            }
        }
        true
    }
}

/// Mean position of a vertex run (the empty case is never called).
fn centroid(vertices: &[crate::render::Vertex]) -> [f32; 3] {
    let mut centre = [0.0f32; 3];
    for vertex in vertices {
        centre[0] += vertex.pos[0];
        centre[1] += vertex.pos[1];
        centre[2] += vertex.pos[2];
    }
    let count = {
        // Clamped to `[1, 2^24]`, the exact range of consecutive integers in
        // `f32`, so the cast is lossless for every run a level can build.
        #[allow(clippy::cast_precision_loss)]
        {
            vertices.len().clamp(1, 1 << 24) as f32
        }
    };
    [centre[0] / count, centre[1] / count, centre[2] / count]
}

/// Builds spatially bucketed vertex ranges from a stream of contiguous quads.
///
/// Every emitter in `render.rs` writes whole quads (six vertices) into a scratch
/// buffer. Feeding those runs here splits them by the cell containing each quad's
/// centroid, so a room, a 40-metre wall or a single fixture all end up in the
/// right cells without the emitting code knowing anything about the grid.
///
/// `G` is a caller-defined group label (the renderer uses its surface/material
/// enum). Buckets are keyed by `(group, cell)` and drained group-major, so all
/// cells of one material stay adjacent and a draw loop only has to bind each
/// texture once. `()` is the natural label when a caller does not need one.
pub struct SpatialBuckets<G = ()> {
    grid: CellGrid,
    buckets: HashMap<(G, CellKey), Vec<crate::render::Vertex>>,
}

impl<G: Copy + Ord + Hash> SpatialBuckets<G> {
    /// Creates buckets for the given grid resolution.
    #[must_use]
    pub fn new(cell_metres: f32) -> Self {
        Self::with_grid(CellGrid::uniform(cell_metres))
    }

    /// Creates buckets for an explicit per-axis grid.
    #[must_use]
    pub fn with_grid(grid: CellGrid) -> Self {
        Self {
            grid,
            buckets: HashMap::new(),
        }
    }

    /// Splits a contiguous run of triangle-list vertices into whole quads and
    /// routes each quad to the cell containing its centroid.
    ///
    /// This is the right choice for long surfaces: a 40-metre wall is cut into
    /// cell-sized pieces that can be culled independently. A trailing partial
    /// quad (never produced by the emitters, but cheap to handle) is attached to
    /// the last cell so no vertex is ever dropped.
    pub fn add_quads(&mut self, group: G, vertices: &[crate::render::Vertex]) {
        const QUAD: usize = 6;
        for quad in vertices.chunks(QUAD) {
            let key = (group, self.grid.cell_of(centroid(quad)));
            self.buckets.entry(key).or_default().extend_from_slice(quad);
        }
    }

    /// Adds a whole run to the single cell containing its centroid.
    ///
    /// Used for things that must not be cut in half — a prop placeholder box
    /// straddling a cell boundary should stay one draw range, exactly like the
    /// real prop geometry it stands in for.
    pub fn add_run(&mut self, group: G, vertices: &[crate::render::Vertex]) {
        if vertices.is_empty() {
            return;
        }
        let key = (group, self.grid.cell_of(centroid(vertices)));
        self.buckets
            .entry(key)
            .or_default()
            .extend_from_slice(vertices);
    }

    /// Number of occupied cells (diagnostics and tests).
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.buckets.len()
    }

    /// Emits the buckets in a deterministic order.
    ///
    /// The grid is sparse (`HashMap`), so the iteration order is not stable
    /// between runs. Sorting the keys makes the resulting vertex order, and
    /// therefore the whole built mesh, byte-for-byte reproducible — the
    /// lighting audit relies on that.
    pub fn drain_sorted(&mut self) -> Vec<((G, CellKey), Vec<crate::render::Vertex>)> {
        let mut keys: Vec<(G, CellKey)> = self.buckets.keys().copied().collect();
        keys.sort_unstable();
        keys.into_iter()
            .filter_map(|key| self.buckets.remove(&key).map(|vertices| (key, vertices)))
            .collect()
    }

    /// Converts every bucket into an indexed range.
    ///
    /// Quads arrive as six vertices each. Indexing turns that into four distinct
    /// vertices plus six indices, and also merges quads that share an edge with
    /// identical attributes (adjacent wall segments, neighbouring floor-lighting
    /// rectangles), so the GPU shades roughly a third fewer vertices for exactly
    /// the same triangles.
    ///
    /// A range is flushed and a new one started whenever it would need more than
    /// [`MAX_INDEX_VERTICES`] vertices, which is what keeps the index type at
    /// 16-bit — the only width OpenGL ES 2.0 guarantees without an extension.
    pub fn drain_indexed(&mut self) -> Vec<((G, CellKey), IndexedRange)> {
        let mut keys: Vec<(G, CellKey)> = self.buckets.keys().copied().collect();
        keys.sort_unstable();
        let mut ranges: Vec<((G, CellKey), IndexedRange)> = Vec::with_capacity(keys.len());
        for key in keys {
            let Some(run) = self.buckets.remove(&key) else {
                continue;
            };
            for range in index_run(&run) {
                ranges.push((key, range));
            }
        }
        ranges
    }
}

/// Upper bound on vertices in one 16-bit-indexed range.
///
/// Index 65535 is the largest `GL_UNSIGNED_SHORT` can address, so a range may
/// hold at most 65536 vertices. Batches are flushed before overflowing rather
/// than silently dropping geometry.
pub const MAX_INDEX_VERTICES: usize = 65_536;

/// One contiguous, indexable range of triangle-list geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexedRange {
    pub vertices: Vec<crate::render::Vertex>,
    pub indices: Vec<u16>,
    pub bounds: Aabb,
}

/// Converts one run of `6 * n` triangle-list vertices into indexed ranges.
///
/// Deduplication is deliberately *not* a global pass: a vertex is reused only
/// when every attribute is bit-identical, and indices are always assigned in
/// emission order, so the output depends only on the input run. That keeps the
/// build deterministic without sorting or hashing in a way that could reorder
/// geometry.
fn index_run(run: &[crate::render::Vertex]) -> Vec<IndexedRange> {
    const QUAD: usize = 6;
    let mut ranges: Vec<IndexedRange> = Vec::new();
    let mut current = IndexedRange::default();
    // Keyed by the exact bit pattern of every attribute, so two vertices that
    // differ only in a baked-lighting channel, a UV or a lightmap chart stay
    // separate.
    let mut seen: HashMap<[u32; 12], u16> = HashMap::new();

    for quad in run.chunks(QUAD) {
        // A quad needs at most four new vertices; start a new range instead of
        // overflowing the 16-bit index space. The bound is written as
        // `MAX_INDEX_VERTICES - QUAD` to test `len + QUAD > MAX_INDEX_VERTICES`
        // without an addition that could overflow.
        if current.vertices.len() > MAX_INDEX_VERTICES - QUAD {
            ranges.push(std::mem::take(&mut current));
            seen.clear();
        }

        // Every emitter writes a quad as (p0, p1, p2, p0, p2, p3): four corners
        // with two repeated. Indexing keeps p0..p3 and re-derives the repeat,
        // which is where the six-to-four saving comes from. A short trailing run
        // is treated as a single triangle rather than being discarded.
        let corner_sources = [
            quad.first(),
            quad.get(1),
            quad.get(2),
            if quad.len() >= QUAD {
                quad.last()
            } else {
                None
            },
        ];
        let mut corners = [0u16; 4];
        for (corner, source) in corners.iter_mut().zip(corner_sources) {
            let Some(vertex) = source else {
                break;
            };
            let key = vertex_key(vertex);
            *corner = if let Some(index) = seen.get(&key) {
                *index
            } else {
                let index = u16::try_from(current.vertices.len()).unwrap_or(u16::MAX);
                current.vertices.push(*vertex);
                current.bounds.expand(vertex.pos);
                seen.insert(key, index);
                index
            };
        }
        if quad.len() >= 3 {
            current
                .indices
                .extend_from_slice(&[corners[0], corners[1], corners[2]]);
            // A face can be a triangle encoded in the quad form (the emitter
            // repeats the last corner): its second triangle has no area, so it
            // is dropped rather than stored. `corners` are deduplicated vertex
            // indices, so a repeated corner compares equal exactly when the
            // emitter repeated it.
            if quad.len() >= QUAD && corners[2] != corners[3] {
                current
                    .indices
                    .extend_from_slice(&[corners[0], corners[2], corners[3]]);
            }
        }
    }
    if !current.vertices.is_empty() {
        ranges.push(current);
    }
    ranges
}

/// Exact bit-pattern key for a vertex, used only for equality.
///
/// The lightmap coordinates and page are part of the key: two quads that share
/// a position, colour and tile UV still sample *different* atlas charts, so
/// sharing a vertex between them would make one of the two quads read the
/// other's light. Leaving the lightmap out of the key was correct only while
/// every vertex was vertex-lit.
fn vertex_key(vertex: &crate::render::Vertex) -> [u32; 12] {
    [
        vertex.pos[0].to_bits(),
        vertex.pos[1].to_bits(),
        vertex.pos[2].to_bits(),
        vertex.color[0].to_bits(),
        vertex.color[1].to_bits(),
        vertex.color[2].to_bits(),
        vertex.color[3].to_bits(),
        vertex.uv[0].to_bits(),
        vertex.uv[1].to_bits(),
        u32::from(vertex.lightmap[0]),
        u32::from(vertex.lightmap[1]),
        u32::from(vertex.lightmap_page),
    ]
}

#[cfg(test)]
mod tests;

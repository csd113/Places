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
//! ES 2.0 needs. Liminal levels are single-storey interiors, so a cell is a
//! square of floor space that covers the full height of whatever stands in it.
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
        for (axis, value) in point.iter().enumerate() {
            if value.is_finite() {
                self.min[axis] = self.min[axis].min(*value);
                self.max[axis] = self.max[axis].max(*value);
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
        (0..3).any(|axis| self.min[axis] > self.max[axis])
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
    if index <= -(CELL_LIMIT as f32) {
        -CELL_LIMIT
    } else if index >= CELL_LIMIT as f32 {
        CELL_LIMIT
    } else {
        index as i32
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
    let count = vertices.len().max(1) as f32;
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
        if vertices.is_empty() {
            return;
        }
        let mut start = 0;
        while start < vertices.len() {
            let end = (start + QUAD).min(vertices.len());
            let quad = &vertices[start..end];
            let key = (group, self.grid.cell_of(centroid(quad)));
            self.buckets.entry(key).or_default().extend_from_slice(quad);
            start = end;
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
    // differ only in a baked-lighting channel or a UV stay separate.
    let mut seen: HashMap<[u32; 9], u16> = HashMap::new();

    let mut start = 0;
    while start < run.len() {
        let end = (start + QUAD).min(run.len());
        let quad = &run[start..end];

        // A quad needs at most four new vertices; start a new range instead of
        // overflowing the 16-bit index space.
        if current.vertices.len() + QUAD > MAX_INDEX_VERTICES {
            ranges.push(std::mem::take(&mut current));
            seen.clear();
        }

        // Every emitter writes a quad as (p0, p1, p2, p0, p2, p3): four corners
        // with two repeated. Indexing keeps p0..p3 and re-derives the repeat,
        // which is where the six-to-four saving comes from. A short trailing run
        // is treated as a single triangle rather than being discarded.
        let corner_count = if quad.len() >= QUAD {
            4
        } else {
            quad.len().min(3)
        };
        let mut corners = [0u16; 4];
        for (slot, corner) in corners.iter_mut().enumerate().take(corner_count) {
            // Slot 3 lives at run position 5 in a full quad.
            let source = if corner_count == 4 && slot == 3 {
                5
            } else {
                slot
            };
            let vertex = &quad[source];
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
        if corner_count >= 3 {
            current
                .indices
                .extend_from_slice(&[corners[0], corners[1], corners[2]]);
            if corner_count == 4 {
                current
                    .indices
                    .extend_from_slice(&[corners[0], corners[2], corners[3]]);
            }
        }
        start = end;
    }
    if !current.vertices.is_empty() {
        ranges.push(current);
    }
    ranges
}

/// Exact bit-pattern key for a vertex, used only for equality.
const fn vertex_key(vertex: &crate::render::Vertex) -> [u32; 9] {
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{assert_exact, assert_exact_array};
    use glam::{Mat4, Vec3};

    fn bounds(min: [f32; 3], max: [f32; 3]) -> Aabb {
        Aabb { min, max }
    }

    fn view_projection(eye: Vec3, look_at: Vec3, fov_degrees: f32) -> Mat4 {
        view_projection_up(eye, look_at, fov_degrees, Vec3::Y)
    }

    /// `look_at_rh` needs an `up` that is not parallel to the view direction, so
    /// the straight-down/straight-up cases pass an explicit axis.
    fn view_projection_up(eye: Vec3, look_at: Vec3, fov_degrees: f32, up: Vec3) -> Mat4 {
        let aspect = 480.0 / 272.0;
        let proj = Mat4::perspective_rh(fov_degrees.to_radians(), aspect, 0.1, 100.0);
        let view = Mat4::look_at_rh(eye, look_at, up);
        proj * view
    }

    #[test]
    fn empty_bounds_are_empty_and_never_grow_from_non_finite_input() {
        // An all-non-finite box stays empty rather than becoming infinite.
        let mut empty = Aabb::EMPTY;
        assert!(empty.is_empty());
        empty.expand([f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
        assert!(empty.is_empty(), "no finite component was supplied");

        // Non-finite components are ignored; the finite remainder still counts,
        // because dropping it could only make the bounds too small.
        let mut partial = Aabb::EMPTY;
        partial.expand([f32::NAN, 5.0, f32::INFINITY]);
        partial.expand([2.0, f32::NAN, 3.0]);
        assert!(!partial.is_empty());
        assert_exact(partial.min[0], 2.0);
        assert_exact(partial.min[1], 5.0);
        assert_exact(partial.min[2], 3.0);
        assert_exact(partial.max[1], 5.0);
        assert_exact(partial.max[2], 3.0);

        // An all-NaN box stays useless rather than becoming infinite.
        let mut poisoned = Aabb::EMPTY;
        poisoned.expand([f32::NAN, f32::NAN, f32::NAN]);
        assert!(poisoned.is_empty());
    }

    #[test]
    fn bounds_union_and_centre() {
        let a = Aabb::from_point([0.0, 0.0, 0.0]);
        let b = Aabb::from_point([2.0, 4.0, 6.0]);
        let union = a.union(&b);
        assert_exact_array(union.min, [0.0, 0.0, 0.0]);
        assert_exact_array(union.max, [2.0, 4.0, 6.0]);
        assert_exact_array(union.centre(), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn cells_are_stable_for_negative_and_extreme_coordinates() {
        let cell = 12.0;
        assert_eq!(cell_of([0.0, 0.0, 0.0], cell), CellKey { x: 0, z: 0 });
        assert_eq!(cell_of([11.9, 0.0, -0.1], cell), CellKey { x: 0, z: -1 });
        assert_eq!(cell_of([-0.1, 0.0, 0.0], cell), CellKey { x: -1, z: 0 });
        assert_eq!(cell_of([-12.0, 5.0, -24.0], cell), CellKey { x: -1, z: -2 });
        // Extreme but valid level coordinates must clamp rather than wrap.
        let extreme = cell_of([1.0e30, 0.0, -1.0e30], cell);
        assert_eq!(extreme.x, CELL_LIMIT);
        assert_eq!(extreme.z, -CELL_LIMIT);
        // Degenerate grid parameters fall back to the origin cell.
        assert_eq!(cell_of([5.0, 0.0, 5.0], 0.0), CellKey { x: 0, z: 0 });
        assert_eq!(cell_of([5.0, 0.0, 5.0], -1.0), CellKey { x: 0, z: 0 });
        assert_eq!(cell_of([f32::NAN, 0.0, 0.0], cell), CellKey { x: 0, z: 0 });
    }

    #[test]
    fn a_box_the_camera_is_inside_is_never_culled() {
        let frustum = Frustum::from_view_projection(
            &view_projection(Vec3::ZERO, -Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );
        // The camera sits inside this box; every plane must accept it.
        assert!(frustum.intersects_aabb(&bounds([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])));

        let wrong = Frustum::from_view_projection(
            &view_projection(Vec3::ZERO, -Vec3::Z, 60.0),
            DepthRange::NegativeOneToOne,
        );
        // (The wrong depth convention is still conservative here, which is the
        // property that matters: it must never cull this box either.)
        assert!(wrong.intersects_aabb(&bounds([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])));
    }

    #[test]
    fn a_box_straight_ahead_is_visible_and_the_same_box_behind_is_culled() {
        // Yaw 0 looks along -Z, matching `render_scene`'s forward vector.
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let frustum = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );

        assert!(
            frustum.intersects_aabb(&bounds([-1.0, 0.0, -12.0], [1.0, 2.0, -10.0])),
            "a box ten metres ahead must be visible"
        );
        assert!(
            !frustum.intersects_aabb(&bounds([-1.0, 0.0, 10.0], [1.0, 2.0, 12.0])),
            "the same box behind the camera must be culled"
        );
    }

    #[test]
    fn a_box_far_outside_the_lateral_planes_is_culled() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let frustum = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );
        // Sixty metres to the side while ten metres ahead: outside any 60-degree
        // vertical / ~90-degree horizontal view.
        assert!(!frustum.intersects_aabb(&bounds([59.0, 0.0, -11.0], [61.0, 2.0, -9.0])));
        // Two metres to the side while ten metres ahead: comfortably inside.
        assert!(frustum.intersects_aabb(&bounds([1.9, 0.0, -11.0], [2.1, 2.0, -9.0])));
    }

    #[test]
    fn a_huge_box_crossing_the_frustum_is_never_culled() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let frustum = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );
        // A level-sized batch that spans the camera must survive every plane.
        assert!(frustum.intersects_aabb(&bounds([-500.0, -50.0, -500.0], [500.0, 50.0, 500.0])));
    }

    #[test]
    fn a_box_touching_a_plane_is_kept() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let frustum = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );
        // Exactly on the far plane: `intersects_aabb` is conservative, so
        // "touching" must count as visible rather than being dropped.
        let on_far = bounds([-1.0, 0.0, -100.0], [1.0, 2.0, -100.0]);
        assert!(frustum.intersects_aabb(&on_far));
        // Just past the far plane is definitively outside.
        assert!(!frustum.intersects_aabb(&bounds([-1.0, 0.0, -140.0], [1.0, 2.0, -140.0])));
    }

    #[test]
    fn pitch_and_yaw_rotate_the_visible_set() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        // Looking straight down: a box below the camera is visible, one ahead is not.
        let down = Frustum::from_view_projection(
            &view_projection_up(eye, eye - Vec3::Y, 60.0, Vec3::Z),
            DepthRange::ZeroToOne,
        );
        assert!(down.intersects_aabb(&bounds([-1.0, -6.0, -1.0], [1.0, -4.0, 1.0])));
        assert!(!down.intersects_aabb(&bounds([-1.0, -1.0, -6.0], [1.0, 1.0, -4.0])));

        // Looking straight up: the reverse.
        let up = Frustum::from_view_projection(
            &view_projection_up(eye, eye + Vec3::Y, 60.0, Vec3::Z),
            DepthRange::ZeroToOne,
        );
        assert!(up.intersects_aabb(&bounds([-1.0, 4.0, -1.0], [1.0, 6.0, 1.0])));
        assert!(!up.intersects_aabb(&bounds([-1.0, -6.0, -1.0], [1.0, -4.0, 1.0])));

        // Yawed 90 degrees: -X becomes "ahead".
        let yawed = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::X, 60.0),
            DepthRange::ZeroToOne,
        );
        assert!(yawed.intersects_aabb(&bounds([-12.0, 0.0, -1.0], [-10.0, 2.0, 1.0])));
        assert!(!yawed.intersects_aabb(&bounds([10.0, 0.0, -1.0], [12.0, 2.0, 1.0])));
    }

    #[test]
    fn a_zero_area_box_is_treated_as_invisible() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let frustum = Frustum::from_view_projection(
            &view_projection(eye, eye - Vec3::Z, 60.0),
            DepthRange::ZeroToOne,
        );
        assert!(!frustum.intersects_aabb(&Aabb::EMPTY));
        // A single-point box is non-empty and is judged normally.
        assert!(frustum.intersects_aabb(&Aabb::from_point([0.0, 1.6, -5.0])));
    }

    /// A quad of four distinct corners, written the way the emitters write it:
    /// a triangle list of six vertices with the shared corners repeated.
    fn quad(x0: f32, z0: f32, size: f32) -> Vec<crate::render::Vertex> {
        let vertex = |x: f32, z: f32| crate::render::Vertex {
            pos: [x, 0.0, z],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [x, z],
        };
        let (x1, z1) = (x0 + size, z0 + size);
        vec![
            vertex(x0, z0),
            vertex(x1, z0),
            vertex(x1, z1),
            vertex(x0, z0),
            vertex(x1, z1),
            vertex(x0, z1),
        ]
    }

    #[test]
    fn quads_are_routed_to_their_own_cells_and_keep_every_vertex() {
        let mut run = quad(1.0, 1.0, 1.0);
        run.extend(quad(3.0, 3.0, 1.0));
        run.extend(quad(13.0, 1.0, 1.0));

        let mut buckets = SpatialBuckets::<()>::new(12.0);
        buckets.add_quads((), &run);
        assert_eq!(buckets.cell_count(), 2);

        let drained = buckets.drain_sorted();
        assert_eq!(drained.len(), 2);
        // Sorted by cell key: (0,0) before (1,0).
        assert_eq!(drained[0].0.1, CellKey { x: 0, z: 0 });
        assert_eq!(drained[0].1.len(), 12, "two quads in the origin cell");
        assert_eq!(drained[1].0.1, CellKey { x: 1, z: 0 });
        assert_eq!(drained[1].1.len(), 6, "one quad in the neighbouring cell");

        let total: usize = drained.iter().map(|(_, vertices)| vertices.len()).sum();
        assert_eq!(total, run.len(), "no vertex may be dropped");
    }

    #[test]
    fn indexing_turns_six_vertices_per_quad_into_four() {
        let mut buckets = SpatialBuckets::<()>::new(12.0);
        buckets.add_quads((), &quad(1.0, 1.0, 1.0));
        // A second quad sharing the first quad's far edge: its near corners are
        // bit-identical, so indexing must reuse them.
        buckets.add_quads((), &quad(2.0, 1.0, 1.0));

        let drained = buckets.drain_indexed();
        assert_eq!(drained.len(), 1);
        let range = &drained[0].1;
        assert_eq!(range.indices.len(), 12, "two quads is twelve indices");
        assert_eq!(
            range.vertices.len(),
            6,
            "a shared edge collapses two quads to six distinct vertices"
        );
        // Every vertex must actually be referenced.
        let mut used = vec![false; range.vertices.len()];
        for index in &range.indices {
            used[*index as usize] = true;
        }
        assert!(used.iter().all(|seen| *seen), "no orphan vertices");
        assert!(!range.bounds.is_empty());
    }

    #[test]
    fn indexing_keeps_triangles_in_the_same_winding() {
        let mut buckets = SpatialBuckets::<()>::new(12.0);
        buckets.add_quads((), &quad(0.0, 0.0, 1.0));
        let drained = buckets.drain_indexed();
        let range = &drained[0].1;
        let pos = |index: u16| range.vertices[index as usize].pos;
        assert_exact_array(pos(range.indices[0]), [0.0, 0.0, 0.0]);
        assert_exact_array(pos(range.indices[1]), [1.0, 0.0, 0.0]);
        assert_exact_array(pos(range.indices[2]), [1.0, 0.0, 1.0]);
        assert_exact_array(pos(range.indices[3]), [0.0, 0.0, 0.0]);
        assert_exact_array(pos(range.indices[4]), [1.0, 0.0, 1.0]);
        assert_exact_array(pos(range.indices[5]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn indexing_never_merges_vertices_that_differ_in_any_attribute() {
        // Two quads sharing an edge. Indexed submission may only collapse the
        // shared corners when every attribute matches bit for bit; a difference
        // in UV, baked light or position is a different render vertex and must
        // stay separate.
        type Corner = ([f32; 3], [f32; 2]);

        fn emit(
            corners: &[Corner; 4],
            adjust: &dyn Fn(Corner) -> Corner,
            out: &mut Vec<crate::render::Vertex>,
        ) {
            // Written the way the emitters write a quad: (p0,p1,p2, p0,p2,p3).
            for slot in [0usize, 1, 2, 0, 2, 3] {
                let (pos, uv) = adjust(corners[slot]);
                out.push(crate::render::Vertex {
                    pos,
                    color: [0.5, 1.0, 1.0, 1.0],
                    uv,
                });
            }
        }

        let left: [Corner; 4] = [
            ([0.0, 0.0, 0.0], [0.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0]),
            ([1.0, 0.0, 1.0], [1.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 1.0]),
        ];
        let right: [Corner; 4] = [
            ([1.0, 0.0, 0.0], [1.0, 0.0]),
            ([2.0, 0.0, 0.0], [2.0, 0.0]),
            ([2.0, 0.0, 1.0], [2.0, 1.0]),
            ([1.0, 0.0, 1.0], [1.0, 1.0]),
        ];
        let build = |adjust: &dyn Fn(Corner) -> Corner| {
            let mut out = Vec::new();
            emit(&left, &|corner| corner, &mut out);
            emit(&right, adjust, &mut out);
            out
        };
        let identity = |corner: Corner| corner;

        let identical = index_run(&build(&identity));
        assert_eq!(identical.len(), 1);
        assert_eq!(identical[0].vertices.len(), 6, "shared corners collapse");
        assert_eq!(identical[0].indices.len(), 12);

        // Nudge the right quad's UVs: one extra vertex per differing corner.
        let shifted_uv = index_run(&build(&|(pos, uv): Corner| (pos, [uv[0] + 0.25, uv[1]])));
        assert_eq!(shifted_uv.len(), 1);
        assert_eq!(
            shifted_uv[0].vertices.len(),
            8,
            "a UV difference on a shared edge must not merge"
        );
        assert_eq!(shifted_uv[0].indices.len(), 12);

        // Nudge the right quad's positions, which is how a baked-lighting
        // difference between two rooms shows up.
        let shifted_pos = index_run(&build(&|(pos, uv): Corner| {
            ([pos[0], pos[1] + 0.001, pos[2]], uv)
        }));
        assert_eq!(
            shifted_pos[0].vertices.len(),
            8,
            "a position difference must not merge"
        );
        assert_eq!(shifted_pos[0].indices.len(), 12);
    }

    #[test]
    fn indexing_never_merges_vertices_that_differ_in_baked_lighting() {
        // Two quads sharing an edge where the right quad is lit differently.
        // Baked light lives in the colour channels, so a difference there is a
        // difference in an attribute and the shared corners must stay separate.
        // (This renderer has no per-vertex normal: the baked colour is the only
        // shading attribute, and it is compared exactly like every other one.)
        // Two quads sharing the edge x = 1: four corners each, six distinct.
        let mut run = Vec::new();
        let push = |light: f32, left: bool, out: &mut Vec<crate::render::Vertex>| {
            let corners = if left {
                [(0.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            } else {
                [(1.0f32, 0.0f32), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0)]
            };
            for slot in [0usize, 1, 2, 0, 2, 3] {
                let (x, z) = corners[slot];
                out.push(crate::render::Vertex {
                    pos: [x, 0.0, z],
                    color: [light, light, light, 1.0],
                    uv: [x, z],
                });
            }
        };
        push(0.8, true, &mut run);
        push(0.8, false, &mut run);
        let same = index_run(&run);
        assert_eq!(
            same[0].vertices.len(),
            6,
            "identical lighting shares corners"
        );

        let mut mixed = Vec::new();
        push(0.8, true, &mut mixed);
        push(0.6, false, &mut mixed);
        let different = index_run(&mixed);
        assert_eq!(
            different[0].vertices.len(),
            8,
            "a baked-lighting difference on a shared edge must not merge"
        );
        assert_eq!(different[0].indices.len(), 12);
    }

    #[test]
    fn a_range_is_split_before_it_overflows_16_bit_indices() {
        let mut run: Vec<crate::render::Vertex> = Vec::new();
        let count = MAX_INDEX_VERTICES / 4 + 8;
        for index in 0..count {
            run.extend(quad(index as f32, 0.0, 0.5));
        }
        let ranges = index_run(&run);
        assert!(
            ranges.len() >= 2,
            "an oversized run must split, got {} range(s)",
            ranges.len()
        );
        let mut total_indices = 0usize;
        for range in &ranges {
            assert!(
                range.vertices.len() <= MAX_INDEX_VERTICES,
                "range has {} vertices",
                range.vertices.len()
            );
            assert!(
                range
                    .indices
                    .iter()
                    .all(|index| (*index as usize) < range.vertices.len()),
                "an index pointed outside its own range"
            );
            total_indices += range.indices.len();
        }
        assert_eq!(total_indices, run.len(), "no triangle may be lost");
    }

    #[test]
    fn indexed_draining_is_deterministic() {
        let build = || {
            let mut buckets = SpatialBuckets::<()>::new(12.0);
            for index in 0..8 {
                buckets.add_quads((), &quad(index as f32 * 0.5, 0.0, 0.5));
            }
            buckets.add_quads((), &quad(20.0, 0.0, 0.5));
            buckets.drain_indexed()
        };
        let first = build();
        let second = build();
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1.vertices, b.1.vertices);
            assert_eq!(a.1.indices, b.1.indices);
            assert_eq!(a.1.bounds, b.1.bounds);
        }
    }

    #[test]
    fn add_run_keeps_a_prop_box_in_one_piece_across_a_cell_boundary() {
        let mut straddling = Vec::new();
        for offset in [-0.5f32, 0.5] {
            straddling.extend(quad(1.0, offset, 0.25));
        }
        let mut split = SpatialBuckets::<()>::new(12.0);
        split.add_quads((), &straddling);
        assert_eq!(split.drain_indexed().len(), 2, "per-quad routing splits it");

        let mut whole = SpatialBuckets::<()>::new(12.0);
        whole.add_run((), &straddling);
        assert_eq!(whole.drain_indexed().len(), 1, "whole-run routing keeps it");
    }

    #[test]
    fn the_adaptive_grid_keeps_the_cell_count_bounded() {
        // Small levels keep the finest granularity...
        let small = CellGrid::for_extent(20.0, 20.0);
        assert_exact(small.x, SPATIAL_CELL_MIN_METRES);
        assert_exact(small.z, SPATIAL_CELL_MIN_METRES);

        // ...the grid grows with the level so the cell count stays near the
        // target, and never exceeds the maximum cell size...
        let big = CellGrid::for_extent(264.0, 262.0);
        assert!((big.x - 33.0).abs() < 0.01, "got {}", big.x);
        assert!((big.z - 32.75).abs() < 0.01, "got {}", big.z);
        assert!(264.0 / big.x <= SPATIAL_CELL_TARGET_PER_AXIS + 0.5);
        assert!(264.0 / big.x >= 4.0);

        // ...and an absurd extent is capped rather than becoming one cell.
        let huge = CellGrid::for_extent(1.0e9, 1.0e9);
        assert_exact(huge.x, SPATIAL_CELL_MAX_METRES);

        // A long, thin corridor gets per-axis sizing.
        let corridor = CellGrid::for_extent(200.0, 20.0);
        assert!(corridor.x > corridor.z, "{corridor:?}");
        assert_exact(corridor.z, SPATIAL_CELL_MIN_METRES);

        // Degenerate extents are treated as small.
        assert_exact(
            CellGrid::for_extent(0.0, f32::NAN).x,
            SPATIAL_CELL_MIN_METRES,
        );
    }

    #[test]
    fn the_explicit_override_is_not_clamped_to_the_adaptive_range() {
        // The sweep must be able to go below the adaptive minimum and above the
        // adaptive maximum; `uniform` is a guard, not a policy.
        assert_exact(CellGrid::uniform(4.0).x, 4.0);
        assert_exact(CellGrid::uniform(96.0).x, 96.0);
        // Only genuinely unusable values fall back.
        assert_exact(CellGrid::uniform(0.0).x, SPATIAL_CELL_MIN_METRES);
        assert_exact(CellGrid::uniform(-3.0).x, SPATIAL_CELL_MIN_METRES);
        assert_exact(CellGrid::uniform(f32::NAN).x, SPATIAL_CELL_MIN_METRES);
    }

    #[test]
    fn a_per_axis_grid_keys_each_axis_independently() {
        let grid = CellGrid { x: 20.0, z: 5.0 };
        assert_eq!(grid.cell_of([0.0, 0.0, 14.0]), CellKey { x: 0, z: 2 });
        assert_eq!(grid.cell_of([25.0, 0.0, -6.0]), CellKey { x: 1, z: -2 });
        assert_eq!(grid.cell_of([-0.5, 0.0, -0.5]), CellKey { x: -1, z: -1 });
    }

    #[test]
    fn a_trailing_partial_quad_is_kept_rather_than_dropped() {
        let one = crate::render::Vertex {
            pos: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        let mut buckets = SpatialBuckets::<()>::new(12.0);
        buckets.add_quads((), &[one; 3]);
        let drained = buckets.drain_sorted();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].1.len(), 3);

        // Indexing a run shorter than a quad keeps the data rather than
        // dropping it: one degenerate triangle over the three deduplicated to
        // a single vertex.
        let mut indexed = SpatialBuckets::<()>::new(12.0);
        indexed.add_quads((), &[one; 3]);
        let ranges = indexed.drain_indexed();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].1.vertices.len(), 1);
        assert_eq!(ranges[0].1.indices, vec![0, 0, 0]);
    }

    #[test]
    fn cell_order_is_sorted_rather_than_insertion_order() {
        let vertex = |x: f32| crate::render::Vertex {
            pos: [x, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        let flat_quad = |x: f32| vec![vertex(x); 6];

        let mut forward = SpatialBuckets::<()>::new(12.0);
        for x in [0.0f32, 13.0, 26.0] {
            forward.add_quads((), &flat_quad(x));
        }
        let mut reverse = SpatialBuckets::<()>::new(12.0);
        for x in [26.0f32, 13.0, 0.0] {
            reverse.add_quads((), &flat_quad(x));
        }

        let a = forward.drain_indexed();
        let b = reverse.drain_indexed();
        let keys_a: Vec<CellKey> = a.iter().map(|(((), cell), _)| *cell).collect();
        let keys_b: Vec<CellKey> = b.iter().map(|(((), cell), _)| *cell).collect();
        assert_eq!(
            keys_a, keys_b,
            "cell order must be sorted, not insertion order"
        );
        assert_eq!(
            keys_a,
            vec![
                CellKey { x: 0, z: 0 },
                CellKey { x: 1, z: 0 },
                CellKey { x: 2, z: 0 },
            ]
        );
    }
}

//! Static light visibility: does solid geometry stand between a light and a
//! surface sample?
//!
//! The baked lighting model sums a room baseline and a local pool per fixture.
//! A pool is a distance falloff, so without this module a fixture could light
//! any surface inside [`super::LOCAL_LIGHT_RADIUS_M`] even with opaque geometry
//! in between — the cross-wall bleed and RGB contamination that the wall-boundary
//! repair exists to remove, and (for stacked rooms) the floor-to-floor light
//! leak the vertical-isolation work exists to remove. This module answers one
//! question, once per level load and never per frame:
//!
//! ```text
//! can the straight segment from a fixture's panel to a surface sample pass
//! through solid geometry?
//! ```
//!
//! The geometry it tests is exactly the solid geometry the renderer emits and
//! collision walks through, in three groups:
//!
//! * **Walls.** Every wall is split by
//!   [`crate::level::wall_solid_slices_profiled`] into the same solid columns
//!   the wall mesh and collision use, and each patch of solid wall becomes one
//!   world-space axis-aligned box. A door, window, passage or vent removes the
//!   box it cuts, so a segment that crosses a solid box is blocked while a
//!   segment that passes through an opening's own footprint and height is not;
//!   the solid header above a door still blocks; a low wall or a raised wall
//!   blocks only up to its real top.
//! * **Floor interfaces.** Every floor-grid cell of every room contributes a
//!   zero-thickness horizontal interface at its own surface height. An
//!   interface blocks a segment whose endpoints are on opposite sides of the
//!   plane within the cell's footprint, and nothing else: because it has no
//!   thickness it can never occupy a room's air, so a raised platform, a
//!   staircase step and a lowered basin inside one volume are not mistaken for
//!   sealed floors, and a fixture hanging just below its room's ceiling is
//!   never swallowed by the room above's floor.
//! * **Ceiling slabs.** Every ceiling-grid cell contributes a thin box that
//!   starts *at* the ceiling plane and extends upward. It gives a gable roof a
//!   solid stepped body without ever reaching into the room's own air, so a
//!   segment that enters through the roof from the side is still stopped.
//! * **Static prop bodies.** Every placed prop contributes the boxes derived
//!   from its real model triangles ([`super::occlusion`]), transformed by its
//!   placement: uniform scale, yaw about Y and translation. They are a
//!   separate list on purpose — the wall-only point-containment and
//!   partition-connectivity queries must not see furniture, so a floor sample
//!   under a couch is shaded by the couch rather than walked out from under
//!   it.
//!
//! The boxes are the exact solid extents: they are never inflated or shrunk.
//! Two pieces that meet in the mesh — the wall beside a window, the wall a
//! corner abuts — therefore meet in the visibility set too, with no slit
//! between them. A query that starts exactly on a face is handled by pushing
//! its start point [`SEGMENT_START_EPS_M`] along its own direction instead.
//!
//! Endpoint semantics
//! ------------------
//! A segment whose endpoint lies exactly on a solid's face is **not** blocked
//! by that face: the slab clip requires strictly overlapping parameter ranges.
//! That is what lets a surface sample sit exactly on its own floor or ceiling
//! plane and still receive the light of the fixture it belongs to, while a
//! segment that genuinely crosses the body is blocked.
//!
//! Boxes are collected per *query site*: one range of solid indices per
//! fixture or opening, holding only the solids whose horizontal bounds reach
//! the site's radius. A segment between two points that are both inside the
//! radius cannot leave that disc, so the prefilter is exact rather than
//! approximate. That keeps one visibility query proportional to the few solids
//! near the fixture instead of to the whole level.
//!
//! Everything here is deterministic: boxes are built in wall order then room
//! order, columns in ascending length order and spans in ascending height
//! order, and a query walks its range in index order.

use crate::level::{LevelDef, LevelSurfaces, RoomDef, WallAxis, wall_solid_slices_profiled};

/// Smallest clipped overlap, as a fraction of a segment's own length, that
/// still counts as the segment entering an opaque box.
///
/// A surface sample sits *exactly* on the plane of the floor or ceiling slab it
/// belongs to, and the clip's contract is that such a sample is lit by the
/// fixture it belongs to: the segment only touches the box's face, it does not
/// cross it. In `f32` the entry parameter of such a grazing segment rounds a
/// few ULPs below its exit parameter (measured: `delta * (1/delta)` can be
/// `1.0 - 6.0e-8`), which a plain `enter < exit` then reads as a crossing.
/// Requiring the overlap to exceed this fraction of the segment absorbs that
/// rounding while leaving real occlusion untouched: no solid in a level is
/// thinner than a few millimetres, and a segment would have to pass through one
/// for less than 1/100000 of its own length to slip through (60 µm on a 6 m
/// ray, 0.6 mm on a 64 m ray — the largest range a light may author).
///
/// This is the tolerance half of the clip; [`nudge_segment_start`] is the other
/// half, handling a start point that sits on a face. Neither shrinks a box, so
/// the wall-boundary leak the nudge's documentation warns about cannot return.
const SEGMENT_CLIP_EPS: f32 = 1.0e-5;

/// How far a segment's start point is pushed along its own direction before the
/// slab clip runs, in metres.
///
/// A light mounted flush with a wall face and the closest point of a ceiling
/// panel that overlaps a wall in plan both sit exactly *on* an opaque box's
/// boundary. Without this nudge the slab clip counts the segment as starting
/// inside the solid and the fixture lights nothing. Pushing the start a
/// millimetre along the segment is enough to leave the surface, while the boxes
/// themselves stay at their exact authored size — which is what keeps a solid
/// wall air-tight at the seams between its own columns and at every corner.
///
/// The alternative (shrinking every box) was the source of the wall-boundary
/// leak this module now guards against: a shrunken box leaves a gap of twice
/// this margin wherever two solid pieces meet, and light funnels through the
/// seam. The boxes below are never shrunk.
const SEGMENT_START_EPS_M: f32 = 1.0e-3;

/// Preferred cell size of the uniform grid that answers "is this point inside a
/// wall?" and "does a segment cross a solid near here?".
///
/// Only surface samples and diagnostic queries need it: a room's floor and
/// ceiling grids sample the room's own boundary, which a wall straddling that
/// boundary encloses. The grid keeps the answer to a couple of solids instead
/// of the whole level.
const POINT_GRID_CELL_M: f32 = 4.0;

/// Hard cap on the grid resolution per axis.
///
/// The cell size grows with the level so a very large level still gets a
/// bounded grid (memory and build cost), while a normal level keeps the 4 m
/// cells that make a query touch a handful of solids.
const MAX_GRID_CELLS_PER_AXIS: u32 = 128;

/// How far a wall must reach into a room before the room is worth testing for
/// partitions, in metres.
///
/// A wall that only crosses this far cannot separate a navigable space from the
/// rest of the room, so the flood fill is skipped and the room keeps its
/// historical single baseline.
const PARTITION_MARGIN_M: f32 = 0.75;

/// Tolerance used when grouping a wall's solid slices into length columns, in
/// metres. Slice boundaries come from the same computation, so this only has to
/// absorb float noise.
const WALL_COLUMN_EPS_M: f32 = 1e-4;

/// Thickness of the horizontal body a ceiling contributes, in metres.
///
/// A ceiling body starts at the ceiling plane and extends upward, so it never
/// reaches into the room's air; thickness only has to be enough for a crossing
/// segment to be robustly inside it.
pub const CEILING_SLAB_THICKNESS_M: f32 = 0.2;

/// One opaque axis-aligned box in world space, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Blocker {
    min: [f32; 3],
    max: [f32; 3],
}

impl Blocker {
    /// Builds a box from its two corners, or `None` when it has no volume or a
    /// non-finite bound.
    fn from_corners(min: [f32; 3], max: [f32; 3]) -> Option<Self> {
        if min.iter().chain(max.iter()).all(|value| value.is_finite())
            && min[0] < max[0]
            && min[1] < max[1]
            && min[2] < max[2]
        {
            Some(Self { min, max })
        } else {
            None
        }
    }

    /// True when the two X/Z footprint rectangles touch or overlap.
    fn overlaps_footprint(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
        self.min[0] <= x1 && self.max[0] >= x0 && self.min[2] <= z1 && self.max[2] >= z0
    }

    /// Horizontal distance from `(x, z)` to this box's footprint, in metres
    /// (zero when the point is over the footprint).
    fn footprint_distance(&self, x: f32, z: f32) -> f32 {
        let dx = (self.min[0] - x).max(x - self.max[0]).max(0.0);
        let dz = (self.min[2] - z).max(z - self.max[2]).max(0.0);
        dx.hypot(dz)
    }

    /// True when the point lies inside the box in plan view.
    ///
    /// The box is the wall's real solid extent, so a surfel *on* a wall face
    /// counts as buried: the bake walks such a sample out of the wall before it
    /// measures light, which is what keeps a wall from shadowing its own base.
    fn contains_xz(&self, x: f32, z: f32) -> bool {
        x >= self.min[0] && x <= self.max[0] && z >= self.min[2] && z <= self.max[2]
    }
}

/// One opaque box that may be rotated about the vertical axis.
///
/// Props are placed with a yaw and a uniform scale, so a model-local occlusion
/// box maps to a box that is not axis-aligned. Rather than inflating it to the
/// axis-aligned bounding box of its rotation — which over-shadows badly at
/// 45 degrees — the query transforms the segment into the box's own frame and
/// the existing slab clip runs unchanged. An axis-aligned instance takes the
/// exact same code path it always did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OrientedBox {
    center: [f32; 3],
    half: [f32; 3],
    sin: f32,
    cos: f32,
}

impl OrientedBox {
    /// Builds a rotated box, or `None` when it has no volume or a non-finite
    /// value (a model box is always positive; the guard keeps hand-edited or
    /// degenerate input out of the visibility set).
    pub(super) fn new(center: [f32; 3], half: [f32; 3], yaw: f32) -> Option<Self> {
        if !yaw.is_finite()
            || !center
                .iter()
                .chain(half.iter())
                .all(|value| value.is_finite())
        {
            return None;
        }
        if half.iter().any(|value| *value <= 0.0) {
            return None;
        }
        let (sin, cos) = yaw.sin_cos();
        Some(Self {
            center,
            half,
            sin,
            cos,
        })
    }

    /// True when the two X/Z footprints touch or overlap.
    fn overlaps_footprint(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
        let (min_x, max_x, min_z, max_z) = self.footprint();
        min_x <= x1 && max_x >= x0 && min_z <= z1 && max_z >= z0
    }

    /// World X/Z footprint of the rotated box: the tight axis-aligned bounds
    /// of its four rotated corners. Deliberately conservative when the box is
    /// turned, which only ever admits one more prefilter entry.
    fn footprint(&self) -> (f32, f32, f32, f32) {
        let extent_x = self.half[2].mul_add(self.sin.abs(), self.half[0] * self.cos.abs());
        let extent_z = self.half[2].mul_add(self.cos.abs(), self.half[0] * self.sin.abs());
        (
            self.center[0] - extent_x,
            self.center[0] + extent_x,
            self.center[2] - extent_z,
            self.center[2] + extent_z,
        )
    }

    /// Horizontal distance from `(x, z)` to the footprint, zero over it.
    fn footprint_distance(&self, x: f32, z: f32) -> f32 {
        let (min_x, max_x, min_z, max_z) = self.footprint();
        let dx = (min_x - x).max(x - max_x).max(0.0);
        let dz = (min_z - z).max(z - max_z).max(0.0);
        dx.hypot(dz)
    }

    /// True when the segment crosses this box.
    fn hits(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        if self.sin == 0.0 {
            // A zero yaw (or a negative zero) is the only angle whose sine is
            // exactly zero: the historical axis-aligned box path, bit for bit.
            let blocker = Blocker {
                min: [
                    self.center[0] - self.half[0],
                    self.center[1] - self.half[1],
                    self.center[2] - self.half[2],
                ],
                max: [
                    self.center[0] + self.half[0],
                    self.center[1] + self.half[1],
                    self.center[2] + self.half[2],
                ],
            };
            return segment_hits_box(blocker, from, to);
        }
        let blocker = Blocker {
            min: [-self.half[0], -self.half[1], -self.half[2]],
            max: [self.half[0], self.half[1], self.half[2]],
        };
        segment_hits_box(blocker, self.local_point(from), self.local_point(to))
    }

    /// One world-space point in the box's own frame: translate, then un-yaw.
    fn local_point(&self, point: [f32; 3]) -> [f32; 3] {
        let dx = point[0] - self.center[0];
        let dz = point[2] - self.center[2];
        [
            dx.mul_add(self.cos, -(dz * self.sin)),
            point[1] - self.center[1],
            dz.mul_add(self.cos, dx * self.sin),
        ]
    }
}

/// A zero-thickness horizontal interface over a rectangular X/Z footprint.
///
/// The floor of every room is a stair-step of these, one per floor-grid cell,
/// at that cell's own surface height. It has no body, so it blocks exactly the
/// segments that cross from one side of the plane to the other inside the
/// footprint — and never anything else.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Plane {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
}

impl Plane {
    /// Builds an interface from its footprint and height.
    fn new(x0: f32, x1: f32, z0: f32, z1: f32, y: f32) -> Option<Self> {
        if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
            return None;
        }
        if !y.is_finite() || x1 <= x0 || z1 <= z0 {
            return None;
        }
        Some(Self { x0, x1, z0, z1, y })
    }

    fn overlaps_footprint(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
        self.x0 <= x1 && self.x1 >= x0 && self.z0 <= z1 && self.z1 >= z0
    }

    fn footprint_distance(&self, x: f32, z: f32) -> f32 {
        let dx = (self.x0 - x).max(x - self.x1).max(0.0);
        let dz = (self.z0 - z).max(z - self.z1).max(0.0);
        dx.hypot(dz)
    }

    /// True when the segment crosses this interface inside its footprint.
    ///
    /// An endpoint exactly on the plane does not count: a surface sample lies
    /// on its own floor plane by construction, and the fixture it belongs to
    /// must be able to light it.
    fn hits(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        let from_side = from[1] - self.y;
        let to_side = to[1] - self.y;
        if from_side * to_side >= 0.0 {
            return false;
        }
        let denominator = from_side - to_side;
        if denominator == 0.0 {
            return false;
        }
        let fraction = from_side / denominator;
        if !(0.0..=1.0).contains(&fraction) {
            return false;
        }
        let x = (to[0] - from[0]).mul_add(fraction, from[0]);
        let z = (to[2] - from[2]).mul_add(fraction, from[2]);
        x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1
    }
}

/// One solid the visibility set can test: a ceiling body or a floor interface.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Horizontal {
    Slab(Blocker),
    Floor(Plane),
}

impl Horizontal {
    fn overlaps_footprint(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
        match self {
            Self::Slab(blocker) => blocker.overlaps_footprint(x0, x1, z0, z1),
            Self::Floor(plane) => plane.overlaps_footprint(x0, x1, z0, z1),
        }
    }

    fn footprint_distance(&self, x: f32, z: f32) -> f32 {
        match self {
            Self::Slab(blocker) => blocker.footprint_distance(x, z),
            Self::Floor(plane) => plane.footprint_distance(x, z),
        }
    }

    fn hits(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        match self {
            Self::Slab(blocker) => segment_hits_box(*blocker, from, to),
            Self::Floor(plane) => plane.hits(from, to),
        }
    }
}

/// One place a visibility question is asked from: a fixture panel or a doorway
/// blend, with the horizontal radius its contributions can reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuerySite {
    pub x: f32,
    pub z: f32,
    pub radius: f32,
}

impl QuerySite {
    #[must_use]
    pub const fn new(x: f32, z: f32, radius: f32) -> Self {
        Self { x, z, radius }
    }
}

/// Which list a pooled solid lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SolidIndex {
    Wall(u32),
    Horizontal(u32),
    /// A static prop body, rotation included.
    Prop(u32),
}

/// One solid a site can reach, with the horizontal distance from the site
/// centre to the solid's footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SiteSolid {
    solid: SolidIndex,
    /// Distance from the site centre to the solid's X/Z rectangle, in metres.
    near: f32,
}

/// A uniform X/Z grid mapping a cell to the solids whose footprint overlaps it.
///
/// Stored indices are relative to a slice base, so one grid can be built over a
/// sub-range of a larger list without rewriting every index.
#[derive(Clone, Debug, Default)]
struct PointGrid {
    base: usize,
    min_x: f32,
    min_z: f32,
    cells_x: u32,
    cells_z: u32,
    /// World size of one cell, in metres.
    cell_m: f32,
    /// `(start, end)` into `items`, one entry per cell in row-major order.
    ranges: Vec<(u32, u32)>,
    items: Vec<u32>,
}

impl PointGrid {
    fn build<T: Footprint>(solids: &[T], base: usize) -> Self {
        if solids.is_empty() {
            return Self::default();
        }
        let mut min_x = f32::INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for solid in solids {
            let (x0, x1, z0, z1) = solid.footprint();
            min_x = min_x.min(x0);
            min_z = min_z.min(z0);
            max_x = max_x.max(x1);
            max_z = max_z.max(z1);
        }
        if !min_x.is_finite() || !min_z.is_finite() || !max_x.is_finite() || !max_z.is_finite() {
            return Self::default();
        }
        let span_x = (max_x - min_x).max(0.0);
        let span_z = (max_z - min_z).max(0.0);
        // The cell size grows with the level so the grid stays bounded in
        // memory no matter how large a level is, and the build cost stays
        // proportional to what the solids actually cover rather than to
        // `cells x solids`.
        // `MAX_GRID_CELLS_PER_AXIS` is a small constant, so the conversion is
        // exact.
        #[allow(clippy::cast_precision_loss)]
        let cell_m = (span_x.max(span_z) / MAX_GRID_CELLS_PER_AXIS as f32).max(POINT_GRID_CELL_M);
        if !cell_m.is_finite() || cell_m <= 0.0 {
            return Self::default();
        }
        let spec = GridSpec {
            min_x,
            min_z,
            cell_m,
            cells_x: grid_axis_cells(span_x, cell_m),
            cells_z: grid_axis_cells(span_z, cell_m),
        };
        let (cells_x, cells_z) = (spec.cells_x, spec.cells_z);
        let cell_count = (cells_x as usize).saturating_mul(cells_z as usize);

        // Counting sort of the solids into the cells they cover: one pass to
        // count, a prefix sum, then one pass to place each solid in index
        // order, so every cell's list is ascending and deterministic.
        let mut counts: Vec<u32> = vec![0; cell_count.saturating_add(1)];
        let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(cell_count);
        for solid in solids {
            let (x0, x1, z0, z1) = solid.footprint();
            let Some((ix0, ix1, iz0, iz1)) = spec.covered(x0, x1, z0, z1) else {
                continue;
            };
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    let index = iz.saturating_mul(cells_x).saturating_add(ix) as usize;
                    if let Some(count) = counts.get_mut(index) {
                        *count = count.saturating_add(1);
                    }
                }
            }
        }
        let mut total = 0u32;
        for index in 0..cell_count {
            let start = total;
            total = total.saturating_add(counts.get(index).copied().unwrap_or(0));
            ranges.push((start, total));
        }
        let mut items: Vec<u32> = vec![0; total as usize];
        let mut cursor: Vec<u32> = ranges.iter().map(|(start, _)| *start).collect();
        for (solid_index, solid) in solids.iter().enumerate() {
            let (x0, x1, z0, z1) = solid.footprint();
            let Some((ix0, ix1, iz0, iz1)) = spec.covered(x0, x1, z0, z1) else {
                continue;
            };
            let slot = u32::try_from(solid_index).unwrap_or(u32::MAX);
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    let index = iz.saturating_mul(cells_x).saturating_add(ix) as usize;
                    let Some(place) = cursor.get_mut(index) else {
                        continue;
                    };
                    if let Some(item) = items.get_mut(*place as usize) {
                        *item = slot;
                    }
                    *place = place.saturating_add(1);
                }
            }
        }

        Self {
            base,
            min_x,
            min_z,
            cells_x,
            cells_z,
            cell_m,
            ranges,
            items,
        }
    }

    /// Visits every solid whose footprint overlaps the X/Z rectangle.
    ///
    /// A solid listed in several grid cells may be visited more than once;
    /// callers treat a repeated visit as a repeated, idempotent test.
    fn for_each_in_rect<T: Footprint>(
        &self,
        solids: &[T],
        x0: f32,
        x1: f32,
        z0: f32,
        z1: f32,
        mut visit: impl FnMut(&T),
    ) {
        if self.cells_x == 0 || self.cells_z == 0 {
            return;
        }
        if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
            return;
        }
        let (rect_min_x, rect_max_x) = ordered_pair(x0, x1);
        let (rect_min_z, rect_max_z) = ordered_pair(z0, z1);
        // The cell counts are bounded to `1..=MAX_GRID_CELLS_PER_AXIS`, so the
        // conversion is exact.
        #[allow(clippy::cast_precision_loss)]
        let (grid_width, grid_depth) = (
            self.cell_m * self.cells_x as f32,
            self.cell_m * self.cells_z as f32,
        );
        if rect_max_x < self.min_x
            || rect_max_z < self.min_z
            || rect_min_x > self.min_x + grid_width
            || rect_min_z > self.min_z + grid_depth
        {
            return;
        }
        let ix0 = clamp_cell((rect_min_x - self.min_x) / self.cell_m, self.cells_x);
        let ix1 = clamp_cell((rect_max_x - self.min_x) / self.cell_m, self.cells_x);
        let iz0 = clamp_cell((rect_min_z - self.min_z) / self.cell_m, self.cells_z);
        let iz1 = clamp_cell((rect_max_z - self.min_z) / self.cell_m, self.cells_z);
        for iz in iz0..=iz1 {
            for ix in ix0..=ix1 {
                let index = iz.saturating_mul(self.cells_x).saturating_add(ix) as usize;
                let Some(&(start, end)) = self.ranges.get(index) else {
                    continue;
                };
                let Some(items) = self.items.get(start as usize..end as usize) else {
                    continue;
                };
                for item in items {
                    if let Some(solid) = solids.get(self.base.saturating_add(*item as usize)) {
                        visit(solid);
                    }
                }
            }
        }
    }
}

/// A solid's X/Z footprint: `(x0, x1, z0, z1)`.
trait Footprint {
    fn footprint(&self) -> (f32, f32, f32, f32);
}

impl Footprint for Blocker {
    fn footprint(&self) -> (f32, f32, f32, f32) {
        (self.min[0], self.max[0], self.min[2], self.max[2])
    }
}

impl Footprint for Plane {
    fn footprint(&self) -> (f32, f32, f32, f32) {
        (self.x0, self.x1, self.z0, self.z1)
    }
}

impl Footprint for Horizontal {
    fn footprint(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Slab(blocker) => blocker.footprint(),
            Self::Floor(plane) => plane.footprint(),
        }
    }
}

/// Grid cell containing `(x, z)`, or `None` when the point is outside.
fn grid_cell(
    x: f32,
    z: f32,
    min_x: f32,
    min_z: f32,
    cell_m: f32,
    cells_x: u32,
    cells_z: u32,
) -> Option<(u32, u32)> {
    let ix = ((x - min_x) / cell_m).floor();
    let iz = ((z - min_z) / cell_m).floor();
    if ix < 0.0 || iz < 0.0 {
        return None;
    }
    // `floor` leaves non-negative integral values; the saturating cast and the
    // bounds check reject everything outside the grid.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (ix, iz) = (ix as u32, iz as u32);
    if ix >= cells_x || iz >= cells_z {
        return None;
    }
    Some((ix, iz))
}

/// Number of grid cells spanning `extent`, capped at
/// [`MAX_GRID_CELLS_PER_AXIS`].
fn grid_axis_cells(extent: f32, cell_m: f32) -> u32 {
    if !extent.is_finite() || extent < 0.0 || !cell_m.is_finite() || cell_m <= 0.0 {
        return 1;
    }
    let cells = (extent / cell_m).ceil();
    if !cells.is_finite() {
        return MAX_GRID_CELLS_PER_AXIS;
    }
    // `cells` is finite and non-negative; the cast saturates rather than wraps.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = cells as u32;
    cells.saturating_add(1).clamp(1, MAX_GRID_CELLS_PER_AXIS)
}

/// Geometry of one uniform grid: origin, cell size and cell counts.
#[derive(Clone, Copy, Debug)]
struct GridSpec {
    min_x: f32,
    min_z: f32,
    cell_m: f32,
    cells_x: u32,
    cells_z: u32,
}

impl GridSpec {
    /// The cell range a solid's footprint covers, clamped into the grid.
    fn covered(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> Option<(u32, u32, u32, u32)> {
        if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
            return None;
        }
        let (x0, x1) = ordered_pair(x0, x1);
        let (z0, z1) = ordered_pair(z0, z1);
        Some((
            clamp_cell((x0 - self.min_x) / self.cell_m, self.cells_x),
            clamp_cell((x1 - self.min_x) / self.cell_m, self.cells_x),
            clamp_cell((z0 - self.min_z) / self.cell_m, self.cells_z),
            clamp_cell((z1 - self.min_z) / self.cell_m, self.cells_z),
        ))
    }
}

/// Nearest grid cell index for a non-negative cell coordinate, clamped into
/// `0..cells`.
fn clamp_cell(value: f32, cells: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let last = cells.saturating_sub(1);
    // `value` is finite and positive; the cast saturates rather than wraps.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = value.floor() as u32;
    cell.min(last)
}

/// `(min, max)` of two values.
fn ordered_pair(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The level's solid geometry, prepared for segment and point queries.
///
/// Walls and horizontal solids are kept in separate lists so the "is this point
/// inside a wall?" answer and the partition-connectivity segment query only
/// ever look at walls; the full segment query tests both.
#[derive(Clone, Debug, Default)]
pub(super) struct Occluders {
    walls: Vec<Blocker>,
    /// Ceiling bodies, then floor interfaces, in room order.
    horizontals: Vec<Horizontal>,
    /// Static prop bodies, in level order then model-box order. Deliberately a
    /// separate list: props must never answer the wall-only point and
    /// partition queries, or a floor sample under a couch would be walked out
    /// from under it.
    props: Vec<OrientedBox>,
    /// Walls only: the uniform grid behind point containment and the
    /// partition-connectivity segment queries.
    wall_grid: PointGrid,
}

impl Occluders {
    /// Builds the wall boxes, floor interfaces, ceiling bodies and static prop
    /// occluders of a level.
    #[must_use]
    pub(super) fn build(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut walls: Vec<Blocker> = Vec::new();
        for wall in &level.walls {
            append_wall_blockers(&mut walls, wall, &surfaces);
        }
        let mut horizontals: Vec<Horizontal> = Vec::new();
        append_room_horizontals(&mut horizontals, &surfaces);
        let props = super::occlusion::level_occluders(level, &surfaces);
        Self {
            wall_grid: PointGrid::build(&walls, 0),
            walls,
            horizontals,
            props,
        }
    }

    /// Number of static prop occluder boxes.
    #[must_use]
    pub(super) const fn prop_count(&self) -> usize {
        self.props.len()
    }

    /// True when `(x, z)` lies inside a solid wall, ignoring height.
    ///
    /// A room's floor and ceiling are sampled on the room's own footprint, and
    /// a wall authored across that boundary encloses the outermost sample row.
    /// The bake asks this so it can move such a sample out of the solid before
    /// it measures light, instead of leaving a dark strip along the wall base.
    /// Floor interfaces and ceiling bodies are deliberately excluded: a surface
    /// sample sits on its own floor plane by construction, so counting floors
    /// here would walk every floor vertex toward the middle of its room.
    #[must_use]
    pub(super) fn contains_point(&self, x: f32, z: f32) -> bool {
        if self.wall_grid.cells_x == 0 || self.wall_grid.cells_z == 0 {
            return false;
        }
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let Some(cell) = grid_cell(
            x,
            z,
            self.wall_grid.min_x,
            self.wall_grid.min_z,
            self.wall_grid.cell_m,
            self.wall_grid.cells_x,
            self.wall_grid.cells_z,
        ) else {
            return false;
        };
        let index = cell
            .1
            .saturating_mul(self.wall_grid.cells_x)
            .saturating_add(cell.0) as usize;
        let Some(&(start, end)) = self.wall_grid.ranges.get(index) else {
            return false;
        };
        self.wall_grid
            .items
            .get(start as usize..end as usize)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    self.walls
                        .get(*item as usize)
                        .is_some_and(|wall| wall.contains_xz(x, z))
                })
            })
    }

    /// True when any wall reaches into the room's interior.
    ///
    /// A cheap pre-pass for the partition flood fill: a room whose walls all
    /// hug its boundary cannot be split, so there is no reason to flood-fill
    /// its cells. A wall counts once its footprint crosses more than
    /// [`PARTITION_MARGIN_M`] into the room, which admits every real partition
    /// and rejects a perimeter wall's thickness.
    #[must_use]
    pub(super) fn may_partition(&self, room: &crate::lighting::RoomLighting) -> bool {
        let margin = PARTITION_MARGIN_M;
        let (x0, x1) = if room.x1 - room.x0 > margin * 2.0 {
            (room.x0 + margin, room.x1 - margin)
        } else {
            (room.x0, room.x1)
        };
        let (z0, z1) = if room.z1 - room.z0 > margin * 2.0 {
            (room.z0 + margin, room.z1 - margin)
        } else {
            (room.z0, room.z1)
        };
        let mut found = false;
        self.wall_grid
            .for_each_in_rect(&self.walls, x0, x1, z0, z1, |_| found = true);
        found
    }

    /// True when a wall crosses the segment from `from` to `to`.
    ///
    /// Used by the partition-connectivity flood fill: only walls separate two
    /// areas of one room, and the query is deliberately blind to horizontal
    /// solids so a lowered basin or raised platform never reads as a barrier.
    #[must_use]
    pub(super) fn walls_block(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        if !from.iter().chain(to.iter()).all(|value| value.is_finite()) {
            return true;
        }
        let from = nudge_segment_start(from, to);
        let (x0, x1) = ordered_pair(from[0], to[0]);
        let (z0, z1) = ordered_pair(from[2], to[2]);
        let mut hit = false;
        self.wall_grid
            .for_each_in_rect(&self.walls, x0, x1, z0, z1, |wall| {
                if !hit && segment_hits_box(*wall, from, to) {
                    hit = true;
                }
            });
        hit
    }

    /// True when any solid geometry (wall, floor interface or ceiling body)
    /// crosses the segment.
    ///
    /// A diagnostic query rather than a bake one — the bake goes through the
    /// per-site pools — so the horizontal solids are scanned linearly instead
    /// of paying for a second spatial index that only diagnostics would use.
    #[must_use]
    pub(super) fn blocks(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        if !from.iter().chain(to.iter()).all(|value| value.is_finite()) {
            return true;
        }
        if self.walls_block(from, to) {
            return true;
        }
        let from = nudge_segment_start(from, to);
        if self.horizontals.iter().any(|solid| solid.hits(from, to)) {
            return true;
        }
        self.props.iter().any(|prop| prop.hits(from, to))
    }

    /// Number of wall boxes.
    #[must_use]
    pub(super) const fn wall_count(&self) -> usize {
        self.walls.len()
    }

    /// Order-sensitive fingerprint of every solid a bake query tests against.
    ///
    /// This is the *occluder set itself*, not the inputs it was derived from:
    /// walls, floor interfaces, ceiling bodies and the derived prop boxes, in
    /// their deterministic build order. Two bakes with the same fingerprint
    /// shade every sample identically, which is exactly the property the
    /// lightmap cache key needs — a prop model edit that changes its occlusion
    /// changes this number, while a texture-only edit correctly does not.
    ///
    /// Coordinates are hashed as their exact bit patterns, so the result is
    /// stable across platforms and runs and never depends on formatting.
    #[must_use]
    pub(super) fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        hash_bytes(&mut hash, b"occluders-v1");
        hash_u64(
            &mut hash,
            u64::try_from(self.walls.len()).unwrap_or(u64::MAX),
        );
        for wall in &self.walls {
            for value in wall.min.iter().chain(wall.max.iter()) {
                hash_f32(&mut hash, *value);
            }
        }
        hash_u64(
            &mut hash,
            u64::try_from(self.horizontals.len()).unwrap_or(u64::MAX),
        );
        for solid in &self.horizontals {
            match solid {
                Horizontal::Slab(blocker) => {
                    hash_bytes(&mut hash, b"slab");
                    for value in blocker.min.iter().chain(blocker.max.iter()) {
                        hash_f32(&mut hash, *value);
                    }
                }
                Horizontal::Floor(plane) => {
                    hash_bytes(&mut hash, b"floor");
                    for value in [plane.x0, plane.x1, plane.z0, plane.z1, plane.y] {
                        hash_f32(&mut hash, value);
                    }
                }
            }
        }
        hash_u64(
            &mut hash,
            u64::try_from(self.props.len()).unwrap_or(u64::MAX),
        );
        for prop in &self.props {
            hash_bytes(&mut hash, b"prop");
            for value in prop.center.iter().chain(prop.half.iter()) {
                hash_f32(&mut hash, *value);
            }
            hash_f32(&mut hash, prop.sin);
            hash_f32(&mut hash, prop.cos);
        }
        hash
    }
}

/// FNV-1a step over raw bytes.
fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// FNV-1a step over one `u64`.
fn hash_u64(hash: &mut u64, value: u64) {
    hash_bytes(hash, &value.to_le_bytes());
}

/// FNV-1a step over one `f32`'s exact bit pattern.
fn hash_f32(hash: &mut u64, value: f32) {
    hash_bytes(hash, &value.to_bits().to_le_bytes());
}

/// True when the segment `from`-`to` intersects the box `blocker`.
///
/// The standard slab clip against the segment's own `[0, 1]` parameter range.
/// The clip is strict: a segment whose endpoint lands exactly on a face of the
/// box does not count as entering it, so a surface sample that lies on its own
/// floor or ceiling plane is lit by the fixture it belongs to. Strictness is
/// enforced with [`SEGMENT_CLIP_EPS`] rather than with `enter < exit` alone,
/// because the entry parameter of a grazing segment can round below the exit
/// parameter; see that constant for the measured failure and why the tolerance
/// cannot open a real leak.
fn segment_hits_box(blocker: Blocker, from: [f32; 3], to: [f32; 3]) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for ((&start, &end), (&low, &high)) in from
        .iter()
        .zip(to.iter())
        .zip(blocker.min.iter().zip(blocker.max.iter()))
    {
        let delta = end - start;
        if delta.abs() <= f32::EPSILON {
            if start < low || start > high {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / delta;
        let mut near = (low - start) * inverse;
        let mut far = (high - start) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return false;
        }
    }
    exit - enter > SEGMENT_CLIP_EPS
}

/// Moves a segment's start point [`SEGMENT_START_EPS_M`] along the segment, so a
/// query that begins exactly on a solid's face is tested from just outside it.
///
/// A degenerate segment (start == end) is returned unchanged: there is no
/// direction to nudge along, and a zero-length query inside a solid stays
/// blocked. The displacement is along the unit direction, so it is always the
/// same physical distance and never changes with the query's length.
fn nudge_segment_start(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = delta[0]
        .mul_add(delta[0], delta[1].mul_add(delta[1], delta[2] * delta[2]))
        .sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return from;
    }
    let scale = SEGMENT_START_EPS_M / length;
    [
        delta[0].mul_add(scale, from[0]),
        delta[1].mul_add(scale, from[1]),
        delta[2].mul_add(scale, from[2]),
    ]
}

/// One length column of a wall: a contiguous span along the wall's length axis
/// and every vertical span of solid wall that survives there. A window leaves
/// two spans in one column (the wall below its sill and above its header).
#[derive(Debug)]
struct BlockerColumn {
    start: f32,
    end: f32,
    spans: Vec<(f32, f32)>,
}

/// Appends one opaque box per solid patch of one wall.
fn append_wall_blockers(
    blockers: &mut Vec<Blocker>,
    wall: &crate::level::WallDef,
    surfaces: &LevelSurfaces<'_>,
) {
    let length = wall.length();
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let axis = wall.axis();
    let (origin_x, origin_z) = wall.length_origin();
    let breaks = surfaces.wall_profile_breaks(wall);
    let slices = wall_solid_slices_profiled(
        wall,
        |offset| surfaces.clear_ceiling_height_along(wall, offset),
        &breaks,
    );

    // Group the slices into length columns: one column per contiguous length
    // range, carrying every solid vertical span that survives there (the wall
    // below a window and the wall above it are two spans of one column).
    let mut columns: Vec<BlockerColumn> = Vec::new();
    for slice in &slices {
        match columns.last_mut() {
            Some(column)
                if (column.start - slice.start).abs() <= WALL_COLUMN_EPS_M
                    && (column.end - slice.end).abs() <= WALL_COLUMN_EPS_M =>
            {
                column.spans.push((slice.bottom, slice.top));
            }
            _ => columns.push(BlockerColumn {
                start: slice.start,
                end: slice.end,
                spans: vec![(slice.bottom, slice.top)],
            }),
        }
    }

    for column in columns {
        let (start, end) = (column.start, column.end);
        let (length_min, length_max) = match axis {
            WallAxis::X => (origin_x + start, origin_x + end),
            WallAxis::Z => (origin_z + start, origin_z + end),
        };
        let (across_min, across_max) = match axis {
            WallAxis::X => (z0, z1),
            WallAxis::Z => (x0, x1),
        };
        for (bottom, top) in column.spans {
            // The box is the column's exact solid extent: no shrink, so two
            // columns that meet (a wall beside a window, a wall abutting
            // another at a corner) leave no slit for light to funnel through.
            // Displacing the segment's *start* instead is what keeps a fixture
            // mounted flush with a face from being blocked by its own wall.
            let blocker = match axis {
                WallAxis::X => Blocker::from_corners(
                    [length_min, bottom, across_min],
                    [length_max, top, across_max],
                ),
                WallAxis::Z => Blocker::from_corners(
                    [across_min, bottom, length_min],
                    [across_max, top, length_max],
                ),
            };
            if let Some(blocker) = blocker {
                blockers.push(blocker);
            }
        }
    }
}

/// Appends the horizontal geometry of every room: one floor interface per
/// floor-grid cell at that cell's own surface height, and one ceiling body per
/// ceiling-grid cell starting at the cell's highest ceiling point.
///
/// Floors use the same grid the mesh and collision use, so a raised platform, a
/// staircase step and a lowered basin each contribute at their own height
/// instead of being flattened to the room plane. Ceilings follow the ceiling
/// grid; a gable contributes a stair-step body placed *above* the slope, so it
/// never shadows the room's own surfaces.
fn append_room_horizontals(horizontals: &mut Vec<Horizontal>, surfaces: &LevelSurfaces<'_>) {
    for room in surfaces.rooms() {
        append_floor_interfaces(horizontals, surfaces, room);
        append_ceiling_bodies(horizontals, surfaces, room);
    }
}

/// One zero-thickness floor interface per floor-grid cell, at that cell's own
/// surface height.
fn append_floor_interfaces(
    horizontals: &mut Vec<Horizontal>,
    surfaces: &LevelSurfaces<'_>,
    room: &RoomDef,
) {
    let grid = surfaces.floor_grid(room);
    for (iz, z_span) in grid.zs.windows(2).enumerate() {
        let &[z0, z1] = z_span else {
            continue;
        };
        for (ix, x_span) in grid.xs.windows(2).enumerate() {
            let &[x0, x1] = x_span else {
                continue;
            };
            let y = grid.y_at(room, ix, iz);
            if let Some(plane) = Plane::new(x0, x1, z0, z1, y) {
                horizontals.push(Horizontal::Floor(plane));
            }
        }
    }
}

/// One ceiling body per ceiling-grid cell, starting at the highest ceiling
/// point that cell covers. A flat ceiling contributes one body over the whole
/// room footprint.
fn append_ceiling_bodies(
    horizontals: &mut Vec<Horizontal>,
    surfaces: &LevelSurfaces<'_>,
    room: &RoomDef,
) {
    let (xs, zs) = surfaces.ceiling_grid(room);
    if room.ceiling.is_flat() {
        let (x0, x1, z0, z1) = room.bounds();
        let y = room.ceiling_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1));
        push_ceiling_body(horizontals, x0, x1, z0, z1, y);
        return;
    }
    for z_span in zs.windows(2) {
        let &[z0, z1] = z_span else {
            continue;
        };
        for x_span in xs.windows(2) {
            let &[x0, x1] = x_span else {
                continue;
            };
            let mut highest = f32::NEG_INFINITY;
            for x in [x0, x1] {
                for z in [z0, z1] {
                    highest = highest.max(room.ceiling_y_at(x, z));
                }
            }
            push_ceiling_body(horizontals, x0, x1, z0, z1, highest);
        }
    }
}

/// Pushes one ceiling body if its footprint and height are usable.
fn push_ceiling_body(
    horizontals: &mut Vec<Horizontal>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
) {
    if !y.is_finite() {
        return;
    }
    if let Some(blocker) =
        Blocker::from_corners([x0, y, z0], [x1, y + CEILING_SLAB_THICKNESS_M, z1])
    {
        horizontals.push(Horizontal::Slab(blocker));
    }
}

/// The level's solid geometry, prepared for segment queries.
#[derive(Clone, Debug, Default)]
pub struct Visibility {
    occluders: Occluders,
    /// Solids reachable from each query site, concatenated and ordered by
    /// distance from the site.
    pool: Vec<SiteSolid>,
    /// `(start, end)` into `pool`, one entry per query site in site order.
    ranges: Vec<(u32, u32)>,
    /// Site centre of each range, for the reach cut-off.
    sites: Vec<(f32, f32)>,
}

impl Visibility {
    /// Builds the solid set from a level's geometry, plus one query site per
    /// light or opening that needs a visibility answer.
    #[must_use]
    pub fn build(level: &LevelDef, sites: &[QuerySite]) -> Self {
        Self::build_with_occluders(Occluders::build(level), sites)
    }

    /// [`Self::build`] with the occluder set already computed, so the bake can
    /// build partitions before it knows the doorway query sites.
    #[must_use]
    pub(super) fn build_with_occluders(occluders: Occluders, sites: &[QuerySite]) -> Self {
        let solid_count = occluders
            .walls
            .len()
            .saturating_add(occluders.horizontals.len())
            .saturating_add(occluders.props.len());
        let mut pool: Vec<SiteSolid> =
            Vec::with_capacity(solid_count.saturating_mul(sites.len().min(4)));
        let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(sites.len());
        let mut site_centres: Vec<(f32, f32)> = Vec::with_capacity(sites.len());
        for site in sites {
            let start = u32::try_from(pool.len()).unwrap_or(u32::MAX);
            if site.x.is_finite() && site.z.is_finite() && site.radius.is_finite() {
                let radius = site.radius.max(0.0);
                let (x0, x1) = (site.x - radius, site.x + radius);
                let (z0, z1) = (site.z - radius, site.z + radius);
                for (index, wall) in occluders.walls.iter().enumerate() {
                    if wall.overlaps_footprint(x0, x1, z0, z1) {
                        pool.push(SiteSolid {
                            solid: SolidIndex::Wall(u32::try_from(index).unwrap_or(u32::MAX)),
                            near: wall.footprint_distance(site.x, site.z),
                        });
                    }
                }
                for (index, horizontal) in occluders.horizontals.iter().enumerate() {
                    if horizontal.overlaps_footprint(x0, x1, z0, z1) {
                        pool.push(SiteSolid {
                            solid: SolidIndex::Horizontal(u32::try_from(index).unwrap_or(u32::MAX)),
                            near: horizontal.footprint_distance(site.x, site.z),
                        });
                    }
                }
                for (index, prop) in occluders.props.iter().enumerate() {
                    if prop.overlaps_footprint(x0, x1, z0, z1) {
                        pool.push(SiteSolid {
                            solid: SolidIndex::Prop(u32::try_from(index).unwrap_or(u32::MAX)),
                            near: prop.footprint_distance(site.x, site.z),
                        });
                    }
                }
                // Nearest first, so a query can stop as soon as the next solid
                // is further away than its own reach. Sorting by a partial
                // order is safe: every distance is finite and non-negative.
                if let Some(added) = pool.get_mut(start as usize..) {
                    added.sort_by(|a, b| {
                        a.near
                            .partial_cmp(&b.near)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| solid_order(a.solid, b.solid))
                    });
                }
            }
            let end = u32::try_from(pool.len()).unwrap_or(u32::MAX);
            ranges.push((start, end));
            site_centres.push((site.x, site.z));
        }

        Self {
            occluders,
            pool,
            ranges,
            sites: site_centres,
        }
    }

    /// Number of boxes the level contributes, for the developer summary.
    ///
    /// Wall solids plus the horizontal bodies and interfaces: the count a
    /// developer reads as "how much geometry the bake had to test against".
    #[must_use]
    pub const fn blocker_count(&self) -> usize {
        self.occluders
            .walls
            .len()
            .saturating_add(self.occluders.horizontals.len())
    }

    /// Number of wall-solid boxes (the horizontal solids are the remainder).
    #[must_use]
    pub(super) const fn wall_blocker_count(&self) -> usize {
        self.occluders.wall_count()
    }

    /// Number of static prop occlusion boxes the set tests against.
    #[must_use]
    pub const fn prop_blocker_count(&self) -> usize {
        self.occluders.prop_count()
    }

    /// Fingerprint of the whole occluder set, for the lightmap content key.
    ///
    /// See [`Occluders::fingerprint`]: two bakes with the same value shade
    /// every sample identically, so it is exactly what a cache key needs to
    /// reject a stale atlas after a prop model or a light changes.
    #[must_use]
    pub fn occluder_fingerprint(&self) -> u64 {
        self.occluders.fingerprint()
    }

    /// Number of query sites the set was built for.
    #[must_use]
    pub const fn site_count(&self) -> usize {
        self.ranges.len()
    }

    /// True when `(x, z)` lies inside a solid wall, ignoring height.
    ///
    /// A room's floor and ceiling are sampled on the room's own footprint, and
    /// a wall authored across that boundary encloses the outermost sample row.
    /// The bake asks this so it can move such a sample out of the solid before
    /// it measures light, instead of leaving a dark strip along the wall base.
    #[must_use]
    pub fn contains_point(&self, x: f32, z: f32) -> bool {
        self.occluders.contains_point(x, z)
    }

    /// True when solid geometry crosses the segment from `from` to `to`.
    ///
    /// `site` selects the prefiltered range of the fixture or opening the query
    /// belongs to; a site that was never registered (or one with no reachable
    /// solid) blocks nothing.
    #[must_use]
    pub fn occludes(&self, site: u32, from: [f32; 3], to: [f32; 3]) -> bool {
        if !from.iter().chain(to.iter()).all(|value| value.is_finite()) {
            // A non-finite query is refused rather than answered: refusing
            // would drop a legitimate contribution, and answering "blocked"
            // keeps a malformed sample dark instead of accepting an unknown
            // path.
            return true;
        }
        let Some(&(start, end)) = self.ranges.get(site as usize) else {
            return false;
        };
        let Some(&(site_x, site_z)) = self.sites.get(site as usize) else {
            return false;
        };
        // The segment can only reach as far from the site centre as its own
        // farther endpoint: every point of it is inside the disc through
        // `from` and `to`. A solid whose nearest footprint point is beyond
        // that cannot be crossed, so it is skipped without a geometric test.
        // Taking the actual start point (not the site centre plus a fixed
        // margin) keeps the bound exact for any emitter size — a wide
        // prop-attached panel starts far from its centre.
        let reach = (to[0] - site_x)
            .hypot(to[2] - site_z)
            .max((from[0] - site_x).hypot(from[2] - site_z))
            + SEGMENT_START_EPS_M;
        let Some(entries) = self.pool.get(start as usize..end as usize) else {
            return false;
        };
        let from = nudge_segment_start(from, to);
        for entry in entries {
            if entry.near > reach {
                break;
            }
            match entry.solid {
                SolidIndex::Wall(index) => {
                    if let Some(wall) = self.occluders.walls.get(index as usize)
                        && segment_hits_box(*wall, from, to)
                    {
                        return true;
                    }
                }
                SolidIndex::Horizontal(index) => {
                    if let Some(horizontal) = self.occluders.horizontals.get(index as usize)
                        && horizontal.hits(from, to)
                    {
                        return true;
                    }
                }
                SolidIndex::Prop(index) => {
                    if let Some(prop) = self.occluders.props.get(index as usize)
                        && prop.hits(from, to)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// [`Self::occludes`] over every registered solid, for a query that does not
    /// belong to one fixture (used by tests and diagnostics).
    #[must_use]
    pub fn occludes_anywhere(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.occluders.blocks(from, to)
    }
}

/// Total order over pooled solids, so equal distances sort deterministically.
fn solid_order(a: SolidIndex, b: SolidIndex) -> std::cmp::Ordering {
    match (a, b) {
        (SolidIndex::Wall(x), SolidIndex::Wall(y))
        | (SolidIndex::Horizontal(x), SolidIndex::Horizontal(y))
        | (SolidIndex::Prop(x), SolidIndex::Prop(y)) => x.cmp(&y),
        (SolidIndex::Wall(_), SolidIndex::Horizontal(_) | SolidIndex::Prop(_)) => {
            std::cmp::Ordering::Less
        }
        (SolidIndex::Horizontal(_), SolidIndex::Wall(_)) => std::cmp::Ordering::Greater,
        (SolidIndex::Horizontal(_), SolidIndex::Prop(_)) => std::cmp::Ordering::Less,
        (SolidIndex::Prop(_), SolidIndex::Wall(_) | SolidIndex::Horizontal(_)) => {
            std::cmp::Ordering::Greater
        }
    }
}

#[cfg(test)]
// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::level::LevelDef;

    fn level(json: &str) -> LevelDef {
        LevelDef::from_json(json).unwrap_or_else(|error| panic!("test level must parse: {error}"))
    }

    /// One 4 x 4 m room whose only wall is a full-height partition at x = 2.
    fn split_room() -> LevelDef {
        level(
            r#"{
                "format_version": 1,
                "id": "visibility",
                "name": "Visibility",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }
                ],
                "walls": [
                    { "x": 1.9, "z": 0.0, "width": 0.2, "depth": 4.0, "height": 3.0 }
                ]
            }"#,
        )
    }

    /// Two 4 x 4 m rooms stacked with a 0.2 m slab gap between them.
    fn stacked_rooms() -> LevelDef {
        level(
            r#"{
                "format_version": 1,
                "id": "stacked",
                "name": "Stacked",
                "spawn": { "x": 2.0, "z": 2.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0, "floor_y": 0.0 },
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0, "floor_y": 3.2 }
                ]
            }"#,
        )
    }

    #[test]
    fn segment_is_blocked_by_a_solid_wall() {
        let level = split_room();
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        assert!(visibility.wall_blocker_count() > 0);
        assert!(visibility.occludes(0, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
        // Over the wall top the same segment is clear.
        assert!(!visibility.occludes(0, [0.5, 3.4, 2.0], [3.5, 3.4, 2.0]));
        // Around the wall end it is clear.
        assert!(!visibility.occludes(0, [0.5, 1.5, -0.6], [3.5, 1.5, -0.6]));
    }

    #[test]
    fn doorway_span_passes_but_the_header_blocks() {
        let mut level = split_room();
        level.walls[0].openings.push(crate::level::WallOpeningDef {
            kind: "door".into(),
            offset: 1.0,
            width: 1.0,
            height: 2.1,
            sill: 0.0,
        });
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        // Through the doorway.
        assert!(!visibility.occludes(0, [0.5, 1.0, 1.5], [3.5, 1.0, 1.5]));
        // Through the solid wall beside the doorway.
        assert!(visibility.occludes(0, [0.5, 1.0, 3.5], [3.5, 1.0, 3.5]));
        // Through the header above the doorway.
        assert!(visibility.occludes(0, [0.5, 2.5, 1.5], [3.5, 2.5, 1.5]));
    }

    /// One 4 x 4 m room split by a Z-axis wall at x = 1.9..2.1 with a single
    /// window in it, so the wall's solid columns abut the window's own boxes.
    fn split_room_with_window() -> LevelDef {
        let mut level = split_room();
        level.walls[0].openings.push(crate::level::WallOpeningDef {
            kind: "window".into(),
            offset: 1.0,
            width: 1.0,
            height: 1.0,
            sill: 1.0,
        });
        level
    }

    #[test]
    fn the_seam_beside_an_opening_is_airtight() {
        // The window occupies z = 1.0..2.0, y = 1.0..2.0. A sample three
        // millimetres on the solid side of the jamb, at the window's own
        // height, must still be blocked by the solid wall column beside it:
        // the segment crosses the wall through solid material, not through
        // the hole. Before the exact-box fix each column was shrunk by 5 mm,
        // which left a slit at every jamb for light to funnel through.
        let level = split_room_with_window();
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        assert!(
            visibility.occludes(0, [0.5, 1.5, 0.997], [3.5, 1.5, 0.997]),
            "the solid column beside the window must block"
        );
        // 5 mm on the window side of the jamb is the aperture itself.
        assert!(
            !visibility.occludes(0, [0.5, 1.5, 1.005], [3.5, 1.5, 1.005]),
            "the aperture itself must transmit"
        );
        // Below the sill is solid across the whole window span.
        assert!(
            visibility.occludes(0, [0.5, 0.8, 1.5], [3.5, 0.8, 1.5]),
            "below the sill must block"
        );
    }

    #[test]
    fn abutting_wall_pieces_leave_no_seam() {
        // A wall authored as two collinear pieces that meet exactly at z = 2.0.
        // A segment that crosses the wall in that plane must be blocked by the
        // piece on one side or the other; the old shrink left a slit exactly on
        // the seam.
        let json = r#"{
            "format_version": 1,
            "id": "seam",
            "name": "Seam",
            "spawn": { "x": 0.5, "z": 0.5 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }
            ],
            "walls": [
                { "x": 1.9, "z": 0.0, "width": 0.2, "depth": 2.0, "height": 3.0 },
                { "x": 1.9, "z": 2.0, "width": 0.2, "depth": 2.0, "height": 3.0 }
            ]
        }"#;
        let level = level(json);
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        assert_eq!(
            visibility.wall_blocker_count(),
            2,
            "one box per solid piece"
        );
        assert!(
            visibility.occludes(0, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]),
            "the seam between two abutting pieces must block"
        );
    }

    #[test]
    fn a_surface_mounted_fixture_is_not_blocked_by_its_own_wall() {
        // A sconce authored exactly on the wall's east face (x = 2.1): its
        // segment starts on the box boundary. The start-point nudge keeps the
        // wall from blocking its own fixture, while the same wall still blocks
        // the segment to the other side.
        let level = split_room();
        let visibility = Visibility::build(&level, &[QuerySite::new(2.1, 2.0, 6.0)]);
        assert!(
            !visibility.occludes(0, [2.1, 1.9, 2.0], [3.9, 1.9, 2.0]),
            "a fixture flush on the wall lights the room it faces"
        );
        assert!(
            visibility.occludes(0, [2.1, 1.9, 2.0], [0.1, 1.9, 2.0]),
            "the same fixture cannot light through its own wall"
        );
    }

    #[test]
    fn a_site_that_never_registered_blocks_nothing() {
        let level = split_room();
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 0.5)]);
        assert!(!visibility.occludes(0, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
        assert!(!visibility.occludes(u32::MAX, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
    }

    // ------------------------------------------------ horizontal geometry

    #[test]
    fn floors_and_ceilings_block_vertical_light() {
        let level = stacked_rooms();
        let visibility = Visibility::build(&level, &[]);
        assert!(
            visibility.occludes_anywhere([2.0, 1.5, 2.0], [2.0, 4.5, 2.0]),
            "the lower ceiling and upper floor must block a vertical segment"
        );
        assert!(
            visibility.occludes_anywhere([2.0, 4.5, 2.0], [2.0, 1.5, 2.0]),
            "the same boundary blocks in the other direction"
        );
        assert!(
            !visibility.occludes_anywhere([2.0, 1.0, 2.0], [2.0, 2.5, 2.0]),
            "a segment inside one room must pass"
        );
    }

    #[test]
    fn a_room_is_not_blocked_by_its_own_floor_or_ceiling() {
        // A ceiling panel hangs 1 cm below a 3 m ceiling, and the floor is at
        // 0. Neither the ceiling above the panel nor the floor below the
        // sample may block the panel's own pool.
        let level = stacked_rooms();
        let visibility = Visibility::build(&level, &[]);
        assert!(
            !visibility.occludes_anywhere([2.0, 2.99, 2.0], [2.0, 0.02, 2.0]),
            "a fixture must light its own room's floor"
        );
        assert!(
            !visibility.occludes_anywhere([2.0, 2.99, 2.0], [2.0, 3.0, 2.0]),
            "a fixture must light its own ceiling"
        );
        assert!(
            !visibility.occludes_anywhere([2.0, 3.2, 2.0], [2.0, 3.21, 2.0]),
            "a surface sample on the upper floor is not blocked by its own plane"
        );
    }

    #[test]
    fn a_slanted_segment_to_the_ceiling_is_not_blocked_by_that_ceiling() {
        // A fixture panel hangs 1 cm below its ceiling, and a surface sample
        // lies exactly on the ceiling plane. The clip's contract is that the
        // segment only *touches* the slab's bottom face and does not cross it.
        // In f32 the vertical segment above happened to round safely, but a
        // slanted one (every sample that is not straight above the panel) used
        // to report a crossing and deleted the fixture's whole pool on that
        // sample, which is what drew dark rings around fixtures on a lightmap.
        let level = stacked_rooms();
        let visibility = Visibility::build(&level, &[QuerySite::new(2.0, 2.0, 8.0)]);
        let fixture = [2.0, 2.99, 2.0];
        for step in 0..=3_750_u16 {
            let radius = f32::from(step).mul_add(0.001, 0.25);
            for sample in [
                [2.0 + radius, 3.0, 2.0],
                [2.0, 3.0, 2.0 + radius],
                [2.0 + radius, 3.0, 2.0 - radius],
            ] {
                assert!(
                    !visibility.occludes(0, fixture, sample),
                    "a fixture must light its own ceiling at {sample:?}"
                );
            }
        }
        // The same slab still blocks a segment that genuinely crosses it.
        assert!(
            visibility.occludes(0, fixture, [2.0, 3.001, 2.0]),
            "the ceiling body must still block light through it"
        );
        assert!(
            visibility.occludes(0, fixture, [3.0, 3.001, 2.0]),
            "and it must still block a slanted segment that crosses it"
        );
    }

    #[test]
    fn zero_gap_stacking_still_isolates_without_swallowing_fixtures() {
        // The upper room's floor is authored exactly at the lower room's eave,
        // the densest legal stack: the shared interface must still isolate the
        // two rooms, and the lower fixture hanging 1 cm under the shared plane
        // must still light its own ceiling.
        let level = level(
            r#"{
                "format_version": 1,
                "id": "zero_gap",
                "name": "Zero Gap",
                "spawn": { "x": 2.0, "z": 2.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0, "floor_y": 0.0 },
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0, "floor_y": 3.0 }
                ]
            }"#,
        );
        let visibility = Visibility::build(&level, &[]);
        assert!(
            !visibility.occludes_anywhere([2.0, 2.99, 2.0], [2.0, 3.0, 2.0]),
            "the lower fixture lights the shared ceiling"
        );
        assert!(
            visibility.occludes_anywhere([2.0, 2.99, 2.0], [2.0, 3.5, 2.0]),
            "the shared plane blocks the lower fixture from the upper room"
        );
    }

    #[test]
    fn a_lowered_basin_is_not_sealed_from_its_room() {
        let json = r#"{
            "format_version": 1,
            "id": "basin",
            "name": "Basin",
            "spawn": { "x": 3.0, "z": 3.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
            ],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -1.5 }
            ]
        }"#;
        let level = level(json);
        let visibility = Visibility::build(&level, &[]);
        assert!(
            !visibility.occludes_anywhere([3.0, 2.5, 3.0], [3.0, -1.5, 3.0]),
            "light must reach a lowered basin through its own opening"
        );
        assert!(
            !visibility.occludes_anywhere([3.0, 2.5, 3.0], [3.0, -1.0, 3.0]),
            "the basin's own air stays clear"
        );
        assert!(
            visibility.occludes_anywhere([3.0, 2.5, 3.0], [0.5, -0.5, 0.5]),
            "the deck plane still blocks a segment that crosses it"
        );
        assert!(
            visibility.occludes_anywhere([3.0, 2.5, 3.0], [3.0, -2.5, 3.0]),
            "the basin floor still blocks a segment below itself"
        );
    }

    #[test]
    fn a_raised_platform_does_not_isolate_the_room_above_it() {
        let json = r#"{
            "format_version": 1,
            "id": "platform",
            "name": "Platform",
            "spawn": { "x": 3.0, "z": 3.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
            ],
            "floor_regions": [
                { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "offset_y": 0.5 }
            ]
        }"#;
        let level = level(json);
        let visibility = Visibility::build(&level, &[]);
        assert!(
            !visibility.occludes_anywhere([2.0, 2.5, 2.0], [2.0, 0.5, 2.0]),
            "the room's own fixture must light the platform surface"
        );
        assert!(
            visibility.occludes_anywhere([2.0, 2.5, 2.0], [2.0, 0.1, 2.0]),
            "the platform still blocks a segment into the space under itself"
        );
    }

    #[test]
    fn wall_connectivity_is_blind_to_horizontal_solids() {
        // The partition flood fill asks only about walls; a raised or lowered
        // floor must not read as a barrier between two halves of one volume.
        let level = stacked_rooms();
        let occluders = Occluders::build(&level);
        assert!(!occluders.walls_block([2.0, 1.5, 2.0], [2.0, 4.5, 2.0]));
        assert!(occluders.blocks([2.0, 1.5, 2.0], [2.0, 4.5, 2.0]));
        // Point containment ignores floors, so a floor sample is not "inside a
        // wall" and never gets walked off its own surface.
        assert!(!occluders.contains_point(2.0, 2.0));
    }

    #[test]
    fn a_gable_ceiling_body_sits_above_the_slope() {
        let json = r#"{
            "format_version": 1,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                  "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } }
            ]
        }"#;
        let level = level(json);
        let visibility = Visibility::build(&level, &[]);
        // A fixture under the ridge lights the ridge itself: the stair-step
        // ceiling bodies are all at or above the slope.
        assert!(
            !visibility.occludes_anywhere([2.0, 4.9, 2.0], [2.0, 5.0, 2.0]),
            "the gable's own ridge must not be blocked by its ceiling bodies"
        );
        // A vertical segment from inside the wedge to above the ridge is
        // blocked: the roof is solid, even though it is stepped.
        assert!(
            visibility.occludes_anywhere([2.0, 4.0, 2.0], [2.0, 6.0, 2.0]),
            "the stepped roof must still block upward"
        );
    }

    // --------------------------------------------------- oriented prop boxes

    #[test]
    fn an_axis_aligned_oriented_box_behaves_like_a_plain_box() {
        let boxed = OrientedBox::new([1.0, 2.0, 3.0], [0.5, 1.0, 0.25], 0.0)
            .expect("a positive box builds");
        assert!(boxed.hits([0.0, 2.0, 3.0], [2.0, 2.0, 3.0]));
        assert!(!boxed.hits([0.0, 2.0, 4.0], [2.0, 2.0, 4.0]));
        assert!(!boxed.hits([0.0, 3.5, 3.0], [2.0, 3.5, 3.0]));
    }

    #[test]
    fn a_yawed_box_blocks_only_its_rotated_volume() {
        // A 2.0 x 0.5 m footprint (half extents 1.0 x 0.25) turned 45 degrees.
        // Its axis-aligned bounds reach ~0.88 m on both axes; the corner of
        // that bounding box is outside the rotated box itself.
        let boxed = OrientedBox::new([0.0, 0.0, 0.0], [1.0, 0.5, 0.25], 45.0_f32.to_radians())
            .expect("a positive box builds");
        let bounds = boxed.footprint();
        assert!(bounds.0 < -0.8 && bounds.1 > 0.8, "bounds {bounds:?}");
        // A point on the local +X axis, 0.8 m out, is inside the panel.
        assert!(boxed.hits([0.5, 0.0, -0.6], [0.6, 0.0, -0.5]));
        // The corner of the bounding box is not.
        assert!(
            !boxed.hits([0.8, 0.0, 0.75], [0.8, 0.0, 0.85]),
            "the 45-degree box must not over-shadow its bounding-box corner"
        );
        // Height still clips: a segment above the box passes.
        assert!(!boxed.hits([0.5, 1.0, -0.6], [0.6, 1.0, -0.5]));
    }

    #[test]
    fn a_degenerate_oriented_box_is_rejected() {
        assert!(OrientedBox::new([0.0, 0.0, 0.0], [0.0, 1.0, 1.0], 0.0).is_none());
        assert!(OrientedBox::new([0.0, 0.0, 0.0], [1.0, -1.0, 1.0], 0.0).is_none());
        assert!(OrientedBox::new([f32::NAN, 0.0, 0.0], [1.0, 1.0, 1.0], 0.0).is_none());
        assert!(OrientedBox::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], f32::NAN).is_none());
    }
}

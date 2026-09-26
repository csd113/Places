//! A uniform X/Z grid over the level's collision boxes.
//!
//! The controller, the entity routes, interaction targeting and the floating
//! labels all ask questions of the same set: "which solid boxes are near this
//! disc / point / ray?". Answering them by walking the whole `Vec<WallAabb>` is
//! fine for the shipped demo (a few hundred boxes) and becomes the dominant
//! per-frame cost on a dense level: a 20 000-box world scanned by a 12-substep
//! movement pass is a quarter of a million rectangle tests per frame before
//! support, headroom, aiming and labels are even considered.
//!
//! This index is the bounded replacement. It is built once per level load (or
//! reset) and never allocates while a query runs. Every query returns a
//! **superset** of the boxes the linear scan would consider; the existing exact
//! predicates in [`crate::collision`] and [`crate::interact`] still decide the
//! answer, so behaviour is identical to the linear path by construction and not
//! merely by sampling. A stamp array suppresses the duplicates a multi-cell box
//! would otherwise contribute, so a query sees each box at most once per call —
//! the same thing the linear scan sees.
//!
//! The grid deliberately mirrors [`crate::spatial`]'s coarse philosophy: no
//! hierarchy, no Y subdivision, no occlusion structures, nothing a query does
//! not read. Boxes are inserted into every cell their footprint touches, and
//! the cell size is derived from the box count and the world extent so the
//! average occupancy stays near one box per cell.

use std::cell::{Cell, RefCell};

use glam::Vec3;

use crate::collision::WallAabb;

/// Smallest cell edge, in metres.
///
/// A player-sized query reaches roughly a metre, so a cell smaller than this
/// buys nothing but more empty cells to visit.
const MIN_CELL_M: f32 = 4.0;
/// Largest cell edge, in metres.
///
/// Caps the boxes one query can touch on a sparse world: with the public caps
/// (20 000 walls) and this cell edge, a point query examines the boxes of one
/// 64 m column, and a 4 m ray at most a few dozen.
const MAX_CELL_M: f32 = 64.0;
/// Largest cell count the builder accepts, in cells.
///
/// `1 << 20` cells is 4 MiB of cell offsets. A level that would exceed it gets
/// a proportionally larger cell instead of a proportionally larger allocation.
const MAX_CELLS: u64 = 1 << 20;
/// Longest ray traversal the builder is willing to sample, in steps.
///
/// Past this the sawtooth would no longer be a superset, so the query falls
/// back to visiting every box, which is always correct.
const MAX_RAY_SAMPLES: f32 = 1_000_000.0;

/// A box index and the cell lists it belongs to, in compressed sparse row form.
pub struct CollisionIndex {
    cell_m: f32,
    origin_x: f32,
    origin_z: f32,
    cells_x: u32,
    cells_z: u32,
    /// `cells + 1` offsets into [`Self::items`].
    starts: Vec<u32>,
    /// Box indices, cell-major.
    items: Vec<u32>,
    /// One stamp per box, so a query can visit each box once.
    stamps: Vec<Cell<u32>>,
    /// The current query stamp; bumped at the start of every traversal.
    stamp: Cell<u32>,
    /// Reused candidate buffer, so a traversal visits boxes in ascending index
    /// order exactly like the linear scan and allocates nothing per query.
    scratch: RefCell<Vec<u32>>,
    boxes: usize,
}

impl CollisionIndex {
    /// An index that visits every box of every query.
    ///
    /// This is the linear path with the same interface: it is what an empty
    /// level and a level whose boxes are all malformed use, so a query can
    /// always be written once.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            cell_m: MIN_CELL_M,
            origin_x: 0.0,
            origin_z: 0.0,
            cells_x: 0,
            cells_z: 0,
            starts: Vec::new(),
            items: Vec::new(),
            stamps: Vec::new(),
            stamp: Cell::new(0),
            scratch: RefCell::new(Vec::new()),
            boxes: 0,
        }
    }

    /// Builds the index over `boxes`.
    #[must_use]
    pub fn build(boxes: &[WallAabb]) -> Self {
        if boxes.is_empty() {
            return Self::empty();
        }
        let Some((min_x, max_x, min_z, max_z)) = finite_extent(boxes) else {
            // Every box was malformed; keep the linear behaviour rather than
            // pretending to index.
            return Self::empty();
        };
        let span_x = (max_x - min_x).max(0.0);
        let span_z = (max_z - min_z).max(0.0);
        #[allow(clippy::cast_precision_loss)]
        let count = boxes.len() as f32;
        let target = count.sqrt().max(1.0);
        let mut cell_m = (span_x.max(span_z) / target).clamp(MIN_CELL_M, MAX_CELL_M);
        let (mut cells_x, mut cells_z) = cell_counts(span_x, span_z, cell_m);
        // Double the cell edge until the grid fits the memory budget. The loop
        // is bounded: every step quarters the cell count.
        while u64::from(cells_x).saturating_mul(u64::from(cells_z)) > MAX_CELLS {
            cell_m *= 2.0;
            (cells_x, cells_z) = cell_counts(span_x, span_z, cell_m);
        }
        let grid = GridSpec {
            cell_m,
            cells_x,
            cells_z,
            origin_x: min_x,
            origin_z: min_z,
        };

        let slots = cell_count(cells_x, cells_z);
        let mut counts = vec![0_u32; slots];
        let mut ranges: Vec<(u32, u32, u32, u32)> = Vec::with_capacity(boxes.len());
        for wall in boxes {
            let range = grid.range(wall.min_x, wall.max_x, wall.min_z, wall.max_z);
            for z in range.1..=range.3 {
                for x in range.0..=range.2 {
                    if let Some(slot) = counts.get_mut(cell_slot(x, z, cells_x)) {
                        *slot = slot.saturating_add(1);
                    }
                }
            }
            ranges.push(range);
        }

        let mut starts = Vec::with_capacity(slots.saturating_add(1));
        let mut running = 0_u32;
        starts.push(0);
        for count in &counts {
            running = running.saturating_add(*count);
            starts.push(running);
        }
        let mut cursors: Vec<u32> = starts.iter().take(slots).copied().collect();
        let mut items = vec![u32::MAX; usize::try_from(running).unwrap_or(usize::MAX)];
        for (box_index, range) in ranges.iter().enumerate() {
            let box_index = u32::try_from(box_index).unwrap_or(u32::MAX);
            for z in range.1..=range.3 {
                for x in range.0..=range.2 {
                    let slot = cell_slot(x, z, cells_x);
                    let Some(cursor) = cursors.get_mut(slot) else {
                        continue;
                    };
                    let position = usize::try_from(*cursor).unwrap_or(usize::MAX);
                    if let Some(item) = items.get_mut(position) {
                        *item = box_index;
                    }
                    *cursor = cursor.saturating_add(1);
                }
            }
        }
        let boxes_len = boxes.len();
        Self {
            cell_m,
            origin_x: min_x,
            origin_z: min_z,
            cells_x,
            cells_z,
            starts,
            items,
            stamps: (0..boxes_len).map(|_| Cell::new(0)).collect(),
            stamp: Cell::new(0),
            scratch: RefCell::new(Vec::new()),
            boxes: boxes_len,
        }
    }

    /// The grid's fixed parameters, as a range-query helper.
    const fn grid(&self) -> GridSpec {
        GridSpec {
            cell_m: self.cell_m,
            cells_x: self.cells_x,
            cells_z: self.cells_z,
            origin_x: self.origin_x,
            origin_z: self.origin_z,
        }
    }

    /// True when the index carries no boxes; every query visits nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Boxes indexed.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.boxes
    }

    /// Cell edge, in metres (diagnostics and tests).
    #[must_use]
    pub const fn cell_metres(&self) -> f32 {
        self.cell_m
    }

    /// Cell counts, for diagnostics and tests.
    #[must_use]
    pub const fn cells(&self) -> (u32, u32) {
        (self.cells_x, self.cells_z)
    }

    /// Visits every box whose footprint touches the disc `(x, z, radius)`.
    ///
    /// A box appears at most once per call. The set is a superset of the boxes
    /// [`WallAabb::overlaps_disc`] accepts.
    pub fn for_each_disc(
        &self,
        x: f32,
        z: f32,
        radius: f32,
        boxes: &[WallAabb],
        mut visit: impl FnMut(&WallAabb),
    ) {
        if self.is_empty() {
            for wall in boxes {
                visit(wall);
            }
            return;
        }
        if !x.is_finite() || !z.is_finite() || !radius.is_finite() {
            self.visit_all(boxes, &mut visit);
            return;
        }
        let r = radius.max(0.0);
        self.visit_cells(
            self.grid().range(x - r, x + r, z - r, z + r),
            boxes,
            &mut visit,
        );
    }

    /// Visits every box whose footprint contains `(x, z)`.
    pub fn for_each_point(
        &self,
        x: f32,
        z: f32,
        boxes: &[WallAabb],
        mut visit: impl FnMut(&WallAabb),
    ) {
        if self.is_empty() {
            for wall in boxes {
                visit(wall);
            }
            return;
        }
        if !x.is_finite() || !z.is_finite() {
            self.visit_all(boxes, &mut visit);
            return;
        }
        let range = self.grid().range(x, x, z, z);
        self.visit_cells(range, boxes, &mut visit);
    }

    /// Visits every box whose footprint lies within one cell of the 2-D
    /// segment `origin + t * direction` for `t` in `0..=max_t`.
    ///
    /// Conservative by a one-cell margin, which is what keeps it a superset of
    /// the cells the segment truly crosses at cell corners. Boxes are visited
    /// in ascending index order, like the linear scan.
    #[allow(clippy::too_many_arguments)] // the ray and its inputs, one each
    pub fn for_each_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_t: f32,
        boxes: &[WallAabb],
        mut visit: impl FnMut(&WallAabb),
    ) {
        if self.is_empty() {
            for wall in boxes {
                visit(wall);
            }
            return;
        }
        if !origin.is_finite() || !direction.is_finite() || !max_t.is_finite() || max_t <= 0.0 {
            self.visit_all(boxes, &mut visit);
            return;
        }
        let step = (self.cell_m * 0.5).max(0.25);
        let samples = (max_t / step).ceil();
        if !samples.is_finite() || samples > MAX_RAY_SAMPLES {
            self.visit_all(boxes, &mut visit);
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let samples = samples.max(1.0) as u32;
        let stamp = self.begin_query();
        let grid = self.grid();
        // The scratch borrow is held across `visit`, so a visitor must not
        // issue another index query from inside the callback. Every caller in
        // this crate only inspects the box it is handed.
        let mut scratch = self.scratch.borrow_mut();
        scratch.clear();
        for index in 0..=samples {
            #[allow(clippy::cast_precision_loss)]
            let t = max_t * (index as f32 / samples as f32);
            let point_x = direction.x.mul_add(t, origin.x);
            let point_z = direction.z.mul_add(t, origin.z);
            let range = grid.range(point_x, point_x, point_z, point_z);
            self.collect_cells(
                (
                    range.0.saturating_sub(1),
                    range.1.saturating_sub(1),
                    range
                        .2
                        .saturating_add(1)
                        .min(self.cells_x.saturating_sub(1)),
                    range
                        .3
                        .saturating_add(1)
                        .min(self.cells_z.saturating_sub(1)),
                ),
                stamp,
                &mut scratch,
            );
        }
        scratch.sort_unstable();
        for &box_index in scratch.iter() {
            if let Some(wall) = boxes.get(usize::try_from(box_index).unwrap_or(usize::MAX)) {
                visit(wall);
            }
        }
    }

    /// Visits every indexed box once, in ascending index order.
    fn visit_all(&self, boxes: &[WallAabb], visit: &mut impl FnMut(&WallAabb)) {
        let stamp = self.begin_query();
        for (index, wall) in boxes.iter().enumerate() {
            if self.claim(index, stamp) {
                visit(wall);
            }
        }
    }

    /// Visits every box in the cell rectangle `(x0, z0)..=(x1, z1)`, in
    /// ascending box-index order (the linear scan's own order).
    fn visit_cells(
        &self,
        range: (u32, u32, u32, u32),
        boxes: &[WallAabb],
        visit: &mut impl FnMut(&WallAabb),
    ) {
        let stamp = self.begin_query();
        // Held across `visit`; see `for_each_ray`.
        let mut scratch = self.scratch.borrow_mut();
        scratch.clear();
        self.collect_cells(range, stamp, &mut scratch);
        scratch.sort_unstable();
        for &box_index in scratch.iter() {
            if let Some(wall) = boxes.get(usize::try_from(box_index).unwrap_or(usize::MAX)) {
                visit(wall);
            }
        }
    }

    /// Appends every not-yet-stamped box of the cell rectangle to `out`.
    fn collect_cells(&self, range: (u32, u32, u32, u32), stamp: u32, out: &mut Vec<u32>) {
        for z in range.1..=range.3 {
            for x in range.0..=range.2 {
                let slot = cell_slot(x, z, self.cells_x);
                let start = self.starts.get(slot).copied().unwrap_or(0);
                let end = self
                    .starts
                    .get(slot.saturating_add(1))
                    .copied()
                    .unwrap_or(start);
                for item in start..end {
                    let Some(&box_index) =
                        self.items.get(usize::try_from(item).unwrap_or(usize::MAX))
                    else {
                        continue;
                    };
                    let index = usize::try_from(box_index).unwrap_or(usize::MAX);
                    if self.claim(index, stamp) {
                        out.push(box_index);
                    }
                }
            }
        }
    }

    /// Starts a traversal and returns its stamp.
    fn begin_query(&self) -> u32 {
        let next = self.stamp.get().wrapping_add(1);
        if next == 0 {
            // Wrapped after four billion queries (over two years of frames at
            // 60 Hz); clear the table so a stale stamp can never match.
            for stamp in &self.stamps {
                stamp.set(0);
            }
            self.stamp.set(1);
            return 1;
        }
        self.stamp.set(next);
        next
    }

    /// True when `index` has not been visited in this traversal yet.
    fn claim(&self, index: usize, stamp: u32) -> bool {
        let Some(slot) = self.stamps.get(index) else {
            return false;
        };
        if slot.get() == stamp {
            return false;
        }
        slot.set(stamp);
        true
    }
}

/// The finite bounding extent of every box, or `None` when none is finite.
fn finite_extent(boxes: &[WallAabb]) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for wall in boxes {
        for value in [wall.min_x, wall.max_x] {
            if value.is_finite() {
                min_x = min_x.min(value);
                max_x = max_x.max(value);
            }
        }
        for value in [wall.min_z, wall.max_z] {
            if value.is_finite() {
                min_z = min_z.min(value);
                max_z = max_z.max(value);
            }
        }
    }
    (min_x.is_finite() && max_x.is_finite() && min_z.is_finite() && max_z.is_finite())
        .then_some((min_x, max_x, min_z, max_z))
}

/// The fixed parameters of one grid, bundled so a range query carries one
/// value rather than five loose numbers.
#[derive(Clone, Copy)]
struct GridSpec {
    cell_m: f32,
    cells_x: u32,
    cells_z: u32,
    origin_x: f32,
    origin_z: f32,
}

impl GridSpec {
    /// The clamped cell rectangle covering a world-space X/Z rectangle.
    fn range(self, min_x: f32, max_x: f32, min_z: f32, max_z: f32) -> (u32, u32, u32, u32) {
        let (x0, x1) = axis_range(min_x, max_x, self.origin_x, self.cell_m, self.cells_x);
        let (z0, z1) = axis_range(min_z, max_z, self.origin_z, self.cell_m, self.cells_z);
        (x0, z0, x1, z1)
    }
}

/// One axis of [`cell_range`], clamped into `0..=last`.
fn axis_range(lo: f32, hi: f32, origin: f32, cell_m: f32, cells: u32) -> (u32, u32) {
    let last = cells.saturating_sub(1);
    if !lo.is_finite() || !hi.is_finite() || !cell_m.is_finite() || cell_m <= 0.0 {
        return (0, last);
    }
    let lo_cell = clamp_cell((lo - origin) / cell_m, last);
    let hi_cell = clamp_cell((hi - origin) / cell_m, last);
    (lo_cell.min(hi_cell), lo_cell.max(hi_cell))
}

/// `value` (a cell coordinate) clamped into `0..=last`.
fn clamp_cell(value: f32, last: u32) -> u32 {
    if value <= 0.0 {
        return 0;
    }
    let upper = f32::from(u16::try_from(last).unwrap_or(u16::MAX));
    if value >= upper {
        return last;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `value` is finite, positive and below `last`, which fits a `u32`.
    let value = value as u32;
    value.min(last)
}

/// Cell counts for a span and edge, at least one cell per axis.
fn cell_counts(span_x: f32, span_z: f32, cell_m: f32) -> (u32, u32) {
    let axis = |span: f32| {
        if !span.is_finite() || !cell_m.is_finite() || cell_m <= 0.0 {
            return 1;
        }
        let count = (span / cell_m).ceil() + 1.0;
        if count <= 1.0 {
            return 1;
        }
        // Saturate rather than truncate; `build` then grows the cell until the
        // grid fits the budget.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = count.min(f32::from(u16::MAX)) as u32;
        count.max(1)
    };
    (axis(span_x), axis(span_z))
}

/// The flat slot index of a cell, in cell-major order.
fn cell_slot(x: u32, z: u32, cells_x: u32) -> usize {
    let row = usize::try_from(z).unwrap_or(usize::MAX);
    let column = usize::try_from(x).unwrap_or(usize::MAX);
    let stride = usize::try_from(cells_x).unwrap_or(usize::MAX);
    row.saturating_mul(stride).saturating_add(column)
}

/// The number of cells in a grid.
fn cell_count(cells_x: u32, cells_z: u32) -> usize {
    usize::try_from(cells_x)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(cells_z).unwrap_or(usize::MAX))
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    fn wall(x: f32, z: f32, width: f32, depth: f32) -> WallAabb {
        WallAabb::with_y(x, 0.0, z, width, 3.0, depth)
    }

    /// A comparable, order-free key for one box.
    fn key(wall: &WallAabb) -> [f32; 6] {
        [
            wall.min_x, wall.max_x, wall.min_y, wall.max_y, wall.min_z, wall.max_z,
        ]
    }

    /// The boxes a query visited, sorted by their key.
    fn visited(query: impl FnOnce(&mut dyn FnMut(&WallAabb))) -> Vec<WallAabb> {
        let mut hits: Vec<WallAabb> = Vec::new();
        let mut visit = |wall: &WallAabb| hits.push(*wall);
        query(&mut visit);
        hits.sort_by(|a, b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits
    }

    #[test]
    fn disc_queries_match_the_linear_scan() {
        let mut boxes = Vec::new();
        for row in 0..40 {
            for column in 0..40 {
                boxes.push(wall(
                    row as f32 * 7.0 - 100.0,
                    column as f32 * 5.0 - 100.0,
                    0.4,
                    (column % 3 + 1) as f32 * 2.5,
                ));
            }
        }
        let index = CollisionIndex::build(&boxes);
        let mut state = 0x1234_5678_u32;
        for _ in 0..400 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[allow(clippy::cast_precision_loss)]
            let x = ((state >> 8) % 30_000) as f32 / 100.0 - 150.0;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[allow(clippy::cast_precision_loss)]
            let z = ((state >> 8) % 30_000) as f32 / 100.0 - 150.0;
            let mut expected: Vec<WallAabb> = boxes
                .iter()
                .filter(|wall| wall.overlaps_disc(x, z, 0.3))
                .copied()
                .collect();
            expected.sort_by(|a, b| {
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let indexed: Vec<WallAabb> =
                visited(|visit| index.for_each_disc(x, z, 0.3, &boxes, visit))
                    .into_iter()
                    .filter(|wall| wall.overlaps_disc(x, z, 0.3))
                    .collect();
            assert_eq!(expected, indexed, "disc at ({x}, {z})");
        }
    }

    #[test]
    fn disc_queries_visit_each_box_at_most_once() {
        let boxes = vec![
            wall(0.0, 0.0, 30.0, 0.4),
            wall(0.0, 0.0, 0.4, 30.0),
            wall(10.0, 10.0, 1.0, 1.0),
        ];
        let index = CollisionIndex::build(&boxes);
        for _ in 0..3 {
            // Repeated calls must not be suppressed by a stale stamp, and a
            // box spanning several cells must still appear exactly once.
            let hits = visited(|visit| {
                index.for_each_disc(5.0, 5.0, 25.0, &boxes, visit);
            });
            assert_eq!(hits.len(), 3);
        }
    }

    #[test]
    fn point_queries_match_the_linear_scan() {
        let boxes = vec![
            wall(-5.0, -5.0, 10.0, 0.3),
            wall(-5.0, 4.7, 10.0, 0.3),
            wall(-5.0, -5.0, 0.3, 10.0),
            wall(4.7, -5.0, 0.3, 10.0),
            wall(0.0, 0.0, 2.0, 2.0),
        ];
        let index = CollisionIndex::build(&boxes);
        let mut state = 7_u32;
        for _ in 0..200 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[allow(clippy::cast_precision_loss)]
            let x = ((state >> 10) % 2000) as f32 / 100.0 - 10.0;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[allow(clippy::cast_precision_loss)]
            let z = ((state >> 10) % 2000) as f32 / 100.0 - 10.0;
            let mut expected: Vec<WallAabb> = boxes
                .iter()
                .filter(|wall| wall.supports_center(x, z))
                .copied()
                .collect();
            expected.sort_by(|a, b| {
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let indexed: Vec<WallAabb> = visited(|visit| {
                index.for_each_point(x, z, &boxes, visit);
            })
            .into_iter()
            .filter(|wall| wall.supports_center(x, z))
            .collect();
            assert_eq!(expected, indexed, "point at ({x}, {z})");
        }
    }

    #[test]
    fn ray_queries_are_a_superset_of_the_segment() {
        let boxes: Vec<WallAabb> = (0..50)
            .map(|index| {
                let offset = index as f32 * 4.0;
                wall(offset, 3.0, 6.0, 0.2)
            })
            .collect();
        let index = CollisionIndex::build(&boxes);
        for slot in 0..50 {
            let offset = slot as f32 * 4.0;
            let origin = Vec3::new(offset - 5.0, 1.0, 3.1);
            let mut hit = false;
            index.for_each_ray(origin, Vec3::X, 12.0, &boxes, |wall| {
                if wall.overlaps_disc(origin.x + 6.0, origin.z, 0.01) {
                    hit = true;
                }
            });
            assert!(hit, "ray {slot} lost its target");
        }
    }

    #[test]
    fn an_empty_index_visits_every_box() {
        let boxes = vec![wall(0.0, 0.0, 1.0, 1.0), wall(5.0, 5.0, 1.0, 1.0)];
        let index = CollisionIndex::empty();
        let mut seen = 0;
        index.for_each_disc(0.0, 0.0, 0.1, &boxes, |_| seen += 1);
        assert_eq!(seen, 2);
        let mut seen = 0;
        index.for_each_point(0.0, 0.0, &boxes, |_| seen += 1);
        assert_eq!(seen, 2);
        let mut seen = 0;
        index.for_each_ray(Vec3::ZERO, Vec3::X, 1.0, &boxes, |_| seen += 1);
        assert_eq!(seen, 2);
    }

    #[test]
    fn a_large_sparse_world_stays_within_the_cell_budget() {
        let boxes = vec![
            WallAabb::with_y(-9_000.0, 0.0, -9_000.0, 0.4, 3.0, 18_000.0),
            WallAabb::with_y(9_000.0, 0.0, -9_000.0, 0.4, 3.0, 18_000.0),
            WallAabb::with_y(-9_000.0, 0.0, -9_000.0, 18_000.0, 3.0, 0.4),
            WallAabb::with_y(-9_000.0, 0.0, 9_000.0, 18_000.0, 3.0, 0.4),
        ];
        let index = CollisionIndex::build(&boxes);
        let (cells_x, cells_z) = index.cells();
        assert!(u64::from(cells_x) * u64::from(cells_z) <= MAX_CELLS);
        let mut seen = 0;
        index.for_each_disc(0.0, 0.0, 0.3, &boxes, |_| seen += 1);
        assert_eq!(seen, 0, "the far walls must not be a disc candidate");
    }

    #[test]
    fn non_finite_boxes_do_not_panic_or_drop_real_ones() {
        let boxes = vec![
            WallAabb::with_y(f32::NAN, 0.0, 0.0, 1.0, 1.0, 1.0),
            WallAabb::with_y(f32::INFINITY, 0.0, 0.0, 1.0, 1.0, 1.0),
            wall(0.0, 0.0, 2.0, 2.0),
        ];
        let index = CollisionIndex::build(&boxes);
        let mut seen = 0;
        index.for_each_point(1.0, 1.0, &boxes, |wall| {
            if wall.supports_center(1.0, 1.0) {
                seen += 1;
            }
        });
        assert_eq!(seen, 1);
    }
}

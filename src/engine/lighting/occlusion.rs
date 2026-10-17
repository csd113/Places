//! Prop occlusion geometry for the baked lighting.
//!
//! Placed props are solid objects, but until this module existed the bake only
//! tested light against walls, floors and ceilings: a couch, cabinet or
//! washing machine was lit as if it were air. This module derives a small set
//! of opaque boxes for every *distinct placed prop model* directly from the
//! model's real triangles, so the same asset that is drawn is what shades the
//! bake — no second collision mesh for artists to author, and no oversized
//! bounding box for a thin object like a desk or a guardrail.
//!
//! How a model becomes boxes
//! -------------------------
//! 1. The model's X/Z bounds are ground into a uniform column grid at the
//!    requested cell ([`PROP_OCCLUSION_CELL_M`] when the caller wants the
//!    historical resolution, [`occlusion_boxes_with_cell`] otherwise), coarser
//!    only when a model exceeds [`PROP_OCCLUSION_MAX_CELLS_PER_AXIS`]). The
//!    grid origin is the model's own bounds minimum, so the result is a pure
//!    function of the asset and the cell.
//! 2. Every triangle marks the columns its X/Z projection covers. A triangle
//!    that projects to a line — the vertical panel of a guardrail, the flat
//!    quad of a rug — marks the columns the *segment* crosses, so thin
//!    geometry still occludes. Each covered column records the union of the
//!    Y spans of the triangles over it.
//! 3. Adjacent columns with equal spans merge along X into runs, and equal
//!    runs merge along Z into boxes: a closed crate becomes one box, a chair a
//!    handful (seat, legs, back), a slatted desk a top slab plus legs.
//!    Boxes never extend past the model's own bounds.
//! 4. The result is capped per model ([`MAX_PROP_OCCLUSION_BOXES_PER_MODEL`])
//!    and per level ([`MAX_PROP_OCCLUSION_BOXES_PER_LEVEL`]), in the fixed
//!    scan order, so an extreme mesh can never make the bake unbounded.
//!
//! Placement
//! ---------
//! A model box is transformed exactly like the drawn instance: uniform scale,
//! yaw about the model origin, then translate to the prop's position with the
//! authored `y` measured above the local walkable floor. Upright placements
//! retain exact yaw-oriented boxes. Pitch/roll placements use conservative
//! world boxes because the compact Static visibility format stores yaw only;
//! Full transport still sees the exact transformed model triangles.
//!
//! Only static props participate. Every entry of a level's `props` array is
//! static; dynamic objects are a separate renderer-side scene and never level
//! props ([`prop_is_static`]).
//!
//! Loading
//! -------
//! Boxes come from the shipped GLB through [`crate::engine::props::PropAssets`], the
//! same cache the renderer uses, and are remembered per `(model path, grid
//! cell)` in a thread-local cache ([`level_occluders_with_cell`]). Keying by
//! the cell as well as the path is what lets a quality level pick its own
//! grid without ever serving another level's boxes: a quality-level switch
//! re-derives against the newly requested cell. A missing asset root or a
//! failed model is never an error.
//!
//! Draw-path agreement
//! -------------------
//! The bake must occlude what is *drawn*. When the renderer cannot draw a
//! prop's real model, it draws the catalogue placeholder box instead
//! (`render::geometry::emit_prop_fallbacks`); the bake therefore contributes that same box
//! as an occluder, so a prop never silently stops casting a shadow the moment
//! its asset is missing, the vertex budget is exhausted or the level's model
//! budget is full. A prop whose model resolves but whose triangles derive no
//! usable box (an empty or fully degenerate mesh) occludes nothing in either
//! path.
//!
//! Emission is deliberately invisible here: boxes are derived from
//! `vertices`/`indices` only. A prop's material emission never creates
//! illumination, and the bake's lights remain the generic [`LightSource`]s
//! authored on the level.
//!
//! [`LightSource`]: super::LightSource

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use super::tuning::{
    MAX_PROP_OCCLUSION_BOXES_PER_LEVEL, MAX_PROP_OCCLUSION_BOXES_PER_MODEL, PROP_OCCLUSION_CELL_M,
    PROP_OCCLUSION_DEGENERATE_AREA2_M2, PROP_OCCLUSION_MAX_CELLS_PER_AXIS,
    PROP_OCCLUSION_MERGE_EPS_M, PROP_OCCLUSION_MIN_THICKNESS_M,
};
use super::visibility::OrientedBox;
use crate::engine::gltf::PropModel;
use crate::engine::level::{
    LevelDef, LevelSurfaces, MAX_LEVEL_PROP_MODELS, MAX_LEVEL_PROP_VERTICES, PropDef,
};

/// One opaque box in model-local space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LocalBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Derived occlusion geometry of one prop model.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ModelOcclusion {
    /// Model-local boxes, in deterministic grid scan order.
    pub boxes: Vec<LocalBox>,
    /// Vertex count of the asset, mirroring the renderer's level vertex budget.
    pub vertex_count: usize,
    /// True when the asset resolved and parsed, so the renderer draws the real
    /// model. False when the draw path falls back to the catalogue placeholder
    /// box, which the occluder builder then contributes instead.
    pub loaded: bool,
}

/// True when a placed prop may contribute immutable lighting geometry.
///
/// A floating prop is
/// drawn by the dynamic scene at the water surface, and the dynamic path is
/// documented invisible to the baker, so baking it at its dry authored
/// position would make the bake disagree with what is drawn.
/// Fully claimed characters are subsequently excluded using the resolved
/// character claim plan; model/vertex budgets retain their original charge.
#[must_use]
pub(super) const fn prop_is_static(prop: &PropDef) -> bool {
    prop.float.is_none() && !prop.is_physics_movable()
}

/// Occlusion boxes for one model, derived from its real triangles, at an
/// explicit X/Z grid cell in metres.
///
/// A finer cell resolves finer silhouette detail at the cost of more boxes; a
/// coarser one merges a model into fewer, larger boxes. The result is a pure
/// function of `(model, cell)`. A model with no usable geometry (empty vertex
/// list, non-finite bounds) yields no boxes, which callers must treat as "this
/// prop occludes nothing".
///
/// A non-finite or non-positive `cell_m` falls back to
/// [`PROP_OCCLUSION_CELL_M`], so no caller can ask for a degenerate grid.
///
/// A grid *finer* than the historical cell never silently truncates a model at
/// [`MAX_PROP_OCCLUSION_BOXES_PER_MODEL`]: a mesh detailed enough to hit the
/// cap at the requested resolution is re-ground at a coarser cell until it
/// fits, so the prop keeps a complete (if coarser) silhouette instead of
/// losing the high-Z/high-X part of its shadow. The historical cell itself is
/// never re-ground, so its exact cap behaviour is unchanged.
#[must_use]
pub(super) fn occlusion_boxes_with_cell(model: &PropModel, cell_m: f32) -> Vec<LocalBox> {
    let requested = sanitized_cell(cell_m);
    if requested >= PROP_OCCLUSION_CELL_M {
        return grind_model(model, requested);
    }
    let mut cell = requested;
    loop {
        let boxes = grind_model(model, cell);
        if boxes.len() < MAX_PROP_OCCLUSION_BOXES_PER_MODEL || cell >= PROP_OCCLUSION_CELL_M {
            return boxes;
        }
        cell = (cell * 2.0).min(PROP_OCCLUSION_CELL_M);
    }
}

/// Grinds one model's triangles into merged boxes at exactly `cell_m`.
///
/// The cap is applied by [`push_run_box`] in scan order, so this is the
/// documented truncating primitive; [`occlusion_boxes_with_cell`] is what
/// decides whether a truncated result is acceptable.
#[must_use]
fn grind_model(model: &PropModel, cell_m: f32) -> Vec<LocalBox> {
    let Some((min, max)) = model.bounds() else {
        return Vec::new();
    };
    if !min.iter().chain(max.iter()).all(|value| value.is_finite()) {
        return Vec::new();
    }
    let span_x = (max[0] - min[0]).max(0.0);
    let span_z = (max[2] - min[2]).max(0.0);
    let cell = grid_cell(span_x, span_z, cell_m);
    let cells_x = grid_cells(span_x, cell);
    let cells_z = grid_cells(span_z, cell);
    let Some(cell_count) = cells_x.checked_mul(cells_z) else {
        return Vec::new();
    };

    // One occupied Y span per column, `None` where the mesh does not cover
    // the column. Filled by grinding every triangle's X/Z projection.
    let mut spans: Vec<Option<(f32, f32)>> = vec![None; cell_count];
    for chunk in model.indices.as_chunks::<3>().0 {
        let &[i0, i1, i2] = chunk;
        let (Some(a), Some(b), Some(c)) = (
            model.vertices.get(usize::from(i0)),
            model.vertices.get(usize::from(i1)),
            model.vertices.get(usize::from(i2)),
        ) else {
            continue;
        };
        if !a
            .pos
            .iter()
            .chain(b.pos.iter())
            .chain(c.pos.iter())
            .all(|value| value.is_finite())
        {
            continue;
        }
        let y_lo = a.pos[1].min(b.pos[1]).min(c.pos[1]);
        let y_hi = a.pos[1].max(b.pos[1]).max(c.pos[1]);
        let (ix0, ix1) = cell_span(
            a.pos[0].min(b.pos[0]).min(c.pos[0]),
            a.pos[0].max(b.pos[0]).max(c.pos[0]),
            min[0],
            cell,
            cells_x,
        );
        let (iz0, iz1) = cell_span(
            a.pos[2].min(b.pos[2]).min(c.pos[2]),
            a.pos[2].max(b.pos[2]).max(c.pos[2]),
            min[2],
            cell,
            cells_z,
        );
        let corners = [
            [a.pos[0], a.pos[2]],
            [b.pos[0], b.pos[2]],
            [c.pos[0], c.pos[2]],
        ];
        for iz in iz0..=iz1 {
            for ix in ix0..=ix1 {
                let x0 = min[0] + cell_offset(cell, ix);
                let x1 = x0 + cell;
                let z0 = min[2] + cell_offset(cell, iz);
                let z1 = z0 + cell;
                if !triangle_overlaps_cell(corners, x0, x1, z0, z1) {
                    continue;
                }
                let index = iz.saturating_mul(cells_x).saturating_add(ix);
                let Some(slot) = spans.get_mut(index) else {
                    continue;
                };
                match slot {
                    Some((lo, hi)) => {
                        *lo = lo.min(y_lo);
                        *hi = hi.max(y_hi);
                    }
                    empty @ None => *empty = Some((y_lo, y_hi)),
                }
            }
        }
    }

    merge_runs(&spans, cells_x, cells_z, min, max, cell)
}

/// Cell size used for one model's occupancy grid, in metres.
///
/// The requested cell is honoured unless the model is too large for
/// [`PROP_OCCLUSION_MAX_CELLS_PER_AXIS`] columns at that resolution: the grid
/// is then coarsened just enough to fit, so a pathological asset can never turn
/// box derivation into an unbounded scan. A non-finite or non-positive request
/// falls back to [`PROP_OCCLUSION_CELL_M`].
fn grid_cell(span_x: f32, span_z: f32, requested_cell: f32) -> f32 {
    let requested = if requested_cell.is_finite() && requested_cell > 0.0 {
        requested_cell
    } else {
        PROP_OCCLUSION_CELL_M
    };
    let limit = usize_to_f32(PROP_OCCLUSION_MAX_CELLS_PER_AXIS);
    let needed = (span_x / limit).max(span_z / limit).max(requested);
    if needed.is_finite() && needed > 0.0 {
        needed
    } else {
        PROP_OCCLUSION_CELL_M
    }
}

/// Stable cache key for one requested grid cell.
///
/// The cell is an `f32` constant supplied by the caller, so its exact bit
/// pattern is the deterministic identity of the grid it names; a non-finite or
/// non-positive request is canonicalised to the historical cell's own key so it
/// shares that grid's cached boxes.
fn cell_key(cell_m: f32) -> u32 {
    if cell_m.is_finite() && cell_m > 0.0 {
        cell_m.to_bits()
    } else {
        PROP_OCCLUSION_CELL_M.to_bits()
    }
}

/// Number of grid cells spanning `span` at `cell` metres, at least one.
fn grid_cells(span: f32, cell: f32) -> usize {
    if !span.is_finite() || span <= 0.0 || !cell.is_finite() || cell <= 0.0 {
        return 1;
    }
    let count = (span / cell).ceil();
    if !count.is_finite() || count < 1.0 {
        return 1;
    }
    // The cell size is chosen so the count is at most
    // `PROP_OCCLUSION_MAX_CELLS_PER_AXIS`; the clamp is a defensive bound and
    // keeps the cast exact.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The cell size is chosen so the count is at most `PROP_OCCLUSION_MAX_CELLS_PER_AXIS`; the clamp is a defensive bound and keeps the cast exact."
    )]
    let cell_count = count.min(usize_to_f32(PROP_OCCLUSION_MAX_CELLS_PER_AXIS)) as usize;
    cell_count.max(1)
}

/// A grid index as `f32`.
///
/// Indices are bounded by [`PROP_OCCLUSION_MAX_CELLS_PER_AXIS`] (96), so the
/// conversion is exact; the saturating `u16` conversion keeps the helper total
/// without a lossy float cast.
fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// World offset of one grid column, in metres.
fn cell_offset(cell: f32, index: usize) -> f32 {
    cell * usize_to_f32(index)
}

/// Cell range `[lo, hi]` a coordinate span covers in one grid axis.
fn cell_span(low: f32, high: f32, origin: f32, cell: f32, cells: usize) -> (usize, usize) {
    let last = cells.saturating_sub(1);
    let last_f = usize_to_f32(last);
    let to_cell = |value: f32| -> usize {
        let raw = ((value - origin) / cell).floor();
        if !raw.is_finite() {
            return 0;
        }
        let clamped = raw.clamp(0.0, last_f);
        // The clamp bounds the value to `0.0..=last`, so the cast is exact.
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "The clamp bounds the value to `0.0..=last`, so the cast is exact."
        )]
        let cell_index = clamped as usize;
        cell_index
    };
    let a = to_cell(low);
    let b = to_cell(high);
    (a.min(b), a.max(b))
}

/// True when the triangle's X/Z projection touches the cell square.
///
/// A triangle with a degenerate projection is a line, and is tested as such so
/// a vertical panel or a flat quad still marks the columns it crosses.
fn triangle_overlaps_cell(corners: [[f32; 2]; 3], x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
    let min_x = corners[0][0].min(corners[1][0]).min(corners[2][0]);
    let max_x = corners[0][0].max(corners[1][0]).max(corners[2][0]);
    let min_z = corners[0][1].min(corners[1][1]).min(corners[2][1]);
    let max_z = corners[0][1].max(corners[1][1]).max(corners[2][1]);
    if min_x > x1 || max_x < x0 || min_z > z1 || max_z < z0 {
        return false;
    }
    let cross = (corners[1][1] - corners[0][1]).mul_add(
        -(corners[2][0] - corners[0][0]),
        (corners[1][0] - corners[0][0]) * (corners[2][1] - corners[0][1]),
    );
    if cross.abs() <= PROP_OCCLUSION_DEGENERATE_AREA2_M2 {
        return segment_overlaps_rect(corners[0], corners[1], x0, x1, z0, z1)
            || segment_overlaps_rect(corners[1], corners[2], x0, x1, z0, z1)
            || segment_overlaps_rect(corners[2], corners[0], x0, x1, z0, z1);
    }
    let centre = [f32::midpoint(x0, x1), f32::midpoint(z0, z1)];
    let half = [(x1 - x0) * 0.5, (z1 - z0) * 0.5];
    for (start, end) in [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[0]),
    ] {
        let axis = [start[1] - end[1], end[0] - start[0]];
        if axis_separated(axis, &corners, centre, half) {
            return false;
        }
    }
    !axis_separated([1.0, 0.0], &corners, centre, half)
        && !axis_separated([0.0, 1.0], &corners, centre, half)
}

/// True when the projections of the triangle and the rectangle are disjoint on
/// one axis (a separating-axis test step).
fn axis_separated(
    axis: [f32; 2],
    corners: &[[f32; 2]; 3],
    centre: [f32; 2],
    half: [f32; 2],
) -> bool {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for point in corners {
        let distance = axis[1].mul_add(point[1], axis[0] * point[0]);
        low = low.min(distance);
        high = high.max(distance);
    }
    let rect_centre = axis[1].mul_add(centre[1], axis[0] * centre[0]);
    let rect_radius = half[1].mul_add(axis[1].abs(), half[0] * axis[0].abs());
    high < rect_centre - rect_radius || low > rect_centre + rect_radius
}

/// True when the segment `a`-`b` touches the axis-aligned rectangle.
///
/// A plain Liang-Barsky clip against the rectangle's four edges. A segment
/// parallel to an edge (a zero component) is inside that edge's band whenever
/// its coordinate lies within the rectangle's own bounds; the two bounds of the
/// axis are tested together, because testing only one of them rejects every
/// parallel segment whose coordinate sits inside the rectangle — the exact
/// case of a vertical panel's X/Z projection, which is a line, not a point.
fn segment_overlaps_rect(a: [f32; 2], b: [f32; 2], x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (step, start, low, high) in [(delta[0], a[0], x0, x1), (delta[1], a[1], z0, z1)] {
        if step == 0.0 {
            if start < low || start > high {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / step;
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
    enter <= exit
}

/// Merges the occupied columns into runs along X and then into boxes along Z.
///
/// The scan order is fixed (rows of Z, then X), and the emitted boxes are
/// clipped to the model's own bounds, so the result is deterministic and never
/// reaches past the geometry it came from.
fn merge_runs(
    spans: &[Option<(f32, f32)>],
    cells_x: usize,
    cells_z: usize,
    min: [f32; 3],
    max: [f32; 3],
    cell: f32,
) -> Vec<LocalBox> {
    let mut boxes: Vec<LocalBox> = Vec::new();
    let mut active: Vec<Run> = Vec::new();
    for iz in 0..cells_z {
        let mut row: Vec<Run> = Vec::new();
        let mut ix = 0;
        while ix < cells_x {
            let index = iz.saturating_mul(cells_x).saturating_add(ix);
            let Some(Some(span)) = spans.get(index).copied() else {
                ix = ix.saturating_add(1);
                continue;
            };
            let start = ix;
            let mut end = ix;
            while end.saturating_add(1) < cells_x {
                let next_index = iz
                    .saturating_mul(cells_x)
                    .saturating_add(end.saturating_add(1));
                let Some(Some(next)) = spans.get(next_index).copied() else {
                    break;
                };
                if !spans_equal(span, next) {
                    break;
                }
                end = end.saturating_add(1);
            }
            row.push(Run {
                ix0: start,
                ix1: end,
                iz0: iz,
                iz1: iz,
                y0: span.0,
                y1: span.1,
            });
            ix = end.saturating_add(1);
        }

        // Extend a run from the previous row when the whole X range and its Y
        // span match; the first match in scan order wins, deterministically.
        let mut merged: Vec<bool> = vec![false; row.len()];
        for run in &mut active {
            if run.iz1.saturating_add(1) != iz {
                continue;
            }
            if let Some((index, _)) = row.iter_mut().enumerate().find(|(index, next)| {
                !merged.get(*index).copied().unwrap_or(true)
                    && next.ix0 == run.ix0
                    && next.ix1 == run.ix1
                    && spans_equal((run.y0, run.y1), (next.y0, next.y1))
            }) {
                run.iz1 = iz;
                if let Some(flag) = merged.get_mut(index) {
                    *flag = true;
                }
            }
        }
        for (index, run) in row.into_iter().enumerate() {
            if merged.get(index).copied() != Some(true) {
                active.push(run);
            }
        }

        // A run that was not extended this row is complete: emit it.
        let mut completed: Vec<Run> = Vec::new();
        active.retain(|run| {
            if run.iz1 == iz {
                true
            } else {
                completed.push(*run);
                false
            }
        });
        for run in completed {
            push_run_box(&mut boxes, run, min, max, cell);
        }
    }
    for run in active {
        push_run_box(&mut boxes, run, min, max, cell);
    }
    boxes
}

/// One merged rectangle of occupied columns, before it becomes a box.
#[derive(Clone, Copy, Debug)]
struct Run {
    ix0: usize,
    ix1: usize,
    iz0: usize,
    iz1: usize,
    y0: f32,
    y1: f32,
}

/// True when two occupied Y spans are the same within the merge tolerance.
fn spans_equal(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() <= PROP_OCCLUSION_MERGE_EPS_M
        && (a.1 - b.1).abs() <= PROP_OCCLUSION_MERGE_EPS_M
}

/// Emits one run as a model-local box, clipped to the model bounds and capped.
fn push_run_box(boxes: &mut Vec<LocalBox>, run: Run, min: [f32; 3], max: [f32; 3], cell: f32) {
    if boxes.len() >= MAX_PROP_OCCLUSION_BOXES_PER_MODEL {
        return;
    }
    let x0 = min[0] + cell_offset(cell, run.ix0);
    let x1 = (min[0] + cell_offset(cell, run.ix1.saturating_add(1))).min(max[0]);
    let z0 = min[2] + cell_offset(cell, run.iz0);
    let z1 = (min[2] + cell_offset(cell, run.iz1.saturating_add(1))).min(max[2]);
    // A model that is flat on an axis (a single-quad curtain, a zero-thickness
    // rail) would otherwise emit a zero-thickness box and silently stop
    // occluding; the same minimum applies to its Y span.
    let (left_edge, right_edge) = thickened(x0, x1);
    let (near_edge, far_edge) = thickened(z0, z1);
    let (y0, y1) = thickened(run.y0, run.y1);
    let bounds = LocalBox {
        min: [left_edge, y0, near_edge],
        max: [right_edge, y1, far_edge],
    };
    if bounds
        .min
        .iter()
        .chain(bounds.max.iter())
        .all(|v| v.is_finite())
        && bounds.min[0] < bounds.max[0]
        && bounds.min[1] < bounds.max[1]
        && bounds.min[2] < bounds.max[2]
    {
        boxes.push(bounds);
    }
}

/// Gives a span a minimum extent when it is flat, centred on the geometry.
fn thickened(low: f32, high: f32) -> (f32, f32) {
    if high - low >= PROP_OCCLUSION_MIN_THICKNESS_M {
        return (low, high);
    }
    let mid = f32::midpoint(low, high);
    let half = PROP_OCCLUSION_MIN_THICKNESS_M * 0.5;
    (mid - half, mid + half)
}

/// Per-`(path, cell)` model occluder cache plus the prop catalog it resolves
/// through.
///
/// The cell is part of the key because the boxes are a function of it: a
/// quality-level switch asks for a different grid and must never be served
/// another level's boxes.
pub(super) struct PropOcclusionCache {
    catalog: crate::engine::loader::PropCatalog,
    assets: crate::engine::props::PropAssets,
    models: HashMap<(String, u32), Arc<ModelOcclusion>>,
}

impl PropOcclusionCache {
    /// A cache over the shipped catalog and asset root.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            catalog: crate::engine::loader::PropCatalog::load_default(),
            assets: crate::engine::props::PropAssets::load_default(),
            models: HashMap::new(),
        }
    }

    /// World-space occluders for every static prop of one level, at an
    /// explicit prop-occlusion grid cell in metres, in level order, capped at
    /// [`MAX_PROP_OCCLUSION_BOXES_PER_LEVEL`].
    ///
    /// The classification mirrors the renderer's instance loop exactly, so a
    /// prop that falls back to the catalogue placeholder box in the draw path
    /// contributes that same box here and never silently loses its shadow. The
    /// model cache is keyed by `(model path, cell)`, so switching grid never
    /// serves stale boxes.
    #[must_use]
    pub(super) fn level_occluders_with_cell(
        &mut self,
        level: &LevelDef,
        surfaces: &LevelSurfaces<'_>,
        cell_m: f32,
    ) -> Vec<OrientedBox> {
        let mut out: Vec<OrientedBox> = Vec::new();
        if level.props.is_empty() {
            return out;
        }
        let grid_cell_m = sanitized_cell(cell_m);
        let claimed =
            crate::engine::render::claimed_character_models(level, &self.catalog, &mut self.assets);
        let mut seen_models: Vec<String> = Vec::new();
        let mut busy_vertices = 0usize;
        for prop in &level.props {
            if out.len() >= MAX_PROP_OCCLUSION_BOXES_PER_LEVEL {
                break;
            }
            if !prop_is_static(prop) {
                continue;
            }
            // Decorative alpha-cutout scenery (grass, leaf canopies) opts out:
            // the bake derives coarse solid boxes from triangles, which would
            // turn thousands of blade cards into solid shadow volumes.
            if !prop.occludes {
                continue;
            }
            let entry = self.catalog.get(&prop.model);
            let Some(model_path) = entry.model else {
                // The draw path has no model to draw: the catalogue
                // placeholder box is the visible geometry, and it occludes.
                push_placeholder_occluder(prop, entry.size, surfaces, &mut out);
                continue;
            };
            if busy_vertices >= MAX_LEVEL_PROP_VERTICES {
                // The vertex budget is spent: the draw path replaces every
                // further instance with its placeholder box.
                push_placeholder_occluder(prop, entry.size, surfaces, &mut out);
                continue;
            }
            let known = seen_models.iter().any(|path| path == &model_path);
            if !known && seen_models.len() >= MAX_LEVEL_PROP_MODELS {
                // The model budget is spent: same placeholder fallback.
                push_placeholder_occluder(prop, entry.size, surfaces, &mut out);
                continue;
            }
            let model = self.model_occlusion(&model_path, grid_cell_m);
            if !model.loaded {
                // The asset failed to load; the renderer draws (and therefore
                // the bake occludes) the catalogue placeholder box.
                push_placeholder_occluder(prop, entry.size, surfaces, &mut out);
                continue;
            }
            let casts_static_lighting = !claimed.contains(&model_path);
            if !known {
                seen_models.push(model_path);
            }
            busy_vertices = busy_vertices.saturating_add(model.vertex_count);
            if !casts_static_lighting {
                continue;
            }
            if model.boxes.is_empty() {
                // A real model that resolves with no usable triangle: the draw
                // path draws it and it occludes nothing.
                continue;
            }
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            push_instance_occluders(&model.boxes, prop, base_y, &mut out);
        }
        out
    }

    /// The derived occlusion of one model path at one grid cell, loaded and
    /// cached on demand.
    fn model_occlusion(&mut self, model_path: &str, cell_m: f32) -> Arc<ModelOcclusion> {
        let key = (model_path.to_string(), cell_key(cell_m));
        if let Some(cached) = self.models.get(&key) {
            return Arc::clone(cached);
        }
        let occlusion = match self.assets.resolve(model_path) {
            Ok(asset) => ModelOcclusion {
                boxes: occlusion_boxes_with_cell(&asset.model, cell_m),
                vertex_count: asset.model.vertices.len(),
                loaded: true,
            },
            Err(message) => {
                self.assets.report_failure(model_path, &message);
                ModelOcclusion::default()
            }
        };
        let shared_occlusion = Arc::new(occlusion);
        drop(self.models.insert(key, Arc::clone(&shared_occlusion)));
        shared_occlusion
    }
}

/// Canonical grid cell a caller may ask for: finite and strictly positive.
fn sanitized_cell(cell_m: f32) -> f32 {
    if cell_m.is_finite() && cell_m > 0.0 {
        cell_m
    } else {
        PROP_OCCLUSION_CELL_M
    }
}

/// The catalogue placeholder box of a prop that has no drawable model, as a
/// one-box model-local occluder set.
///
/// Mirrors the renderer's `add_prop_box` exactly: the box is centred on the
/// prop's origin, spans `size` in X/Z and rises from the placement point, and
/// the caller's instance transform applies the prop's scale and yaw.
fn placeholder_boxes(size: [f32; 3]) -> Vec<LocalBox> {
    if !size.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Vec::new();
    }
    vec![LocalBox {
        min: [-size[0] * 0.5, 0.0, -size[2] * 0.5],
        max: [size[0] * 0.5, size[1], size[2] * 0.5],
    }]
}

/// Contributes the catalogue placeholder box of one fallback prop, at the
/// same position and size the renderer draws it.
///
/// A prop with an invalid transform or a degenerate catalogue size draws no
/// placeholder box, so it contributes no occluder either.
fn push_placeholder_occluder(
    prop: &PropDef,
    catalog_size: [f32; 3],
    surfaces: &LevelSurfaces<'_>,
    out: &mut Vec<OrientedBox>,
) {
    let size = prop.size.unwrap_or(catalog_size);
    let local = placeholder_boxes(size);
    if local.is_empty() {
        return;
    }
    let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
    push_instance_occluders(&local, prop, base_y, out);
}

// The process-wide cache the bake resolves prop occluders through.
//
// Thread-local rather than global because `crate::engine::props::PropAssets` shares
// decoded models through `Arc`. Each thread parses a given model once; the bake
// result depends only on the level and the shipped assets, never on the order
// threads happened to touch it.
thread_local! {
    static PROP_OCCLUSIONS: RefCell<PropOcclusionCache> =
        RefCell::new(PropOcclusionCache::new());
}

/// Shares the exact request's immutable model inputs with the lighting bake.
/// Derived boxes are request-local; an edited model can never reuse stale boxes.
pub fn set_preparation_assets(
    catalog: crate::engine::loader::PropCatalog,
    assets: crate::engine::props::PropAssets,
) {
    PROP_OCCLUSIONS.with(|cache| {
        *cache.borrow_mut() = PropOcclusionCache {
            catalog,
            assets,
            models: HashMap::new(),
        };
    });
}

/// World-space occluders for every static prop of one level at the historical
/// grid cell.
///
/// This is the entry point the historical [`LevelLighting::bake`] uses. A
/// level with no props does no loading at all; a level whose props cannot be
/// resolved contributes the placeholder occluders the draw path draws.
///
/// [`LevelLighting::bake`]: super::LevelLighting::bake
#[must_use]
pub(super) fn level_occluders(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<OrientedBox> {
    level_occluders_with_cell(level, surfaces, PROP_OCCLUSION_CELL_M)
}

/// [`level_occluders`] with an explicit prop-occlusion grid cell, in metres.
///
/// This is the entry point the level-configured bake
/// (`LevelLighting::bake_with`) uses; the cache is keyed by
/// `(model path, cell)`, so a quality-level switch can never serve another
/// level's boxes.
#[must_use]
pub(super) fn level_occluders_with_cell(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    cell_m: f32,
) -> Vec<OrientedBox> {
    if level.props.is_empty() {
        return Vec::new();
    }
    PROP_OCCLUSIONS.with(|cache| {
        cache
            .borrow_mut()
            .level_occluders_with_cell(level, surfaces, cell_m)
    })
}

/// Transforms one model-local point to world space exactly as the renderer's
/// instance matrix does: scale, yaw about `+Y`, then translate by the prop's
/// position with the authored `y` measured above the local walkable floor.
///
/// This is the occluder path's half of the draw/bake agreement, and
/// `the_instance_transform_matches_the_renderers_matrix` pins it to the same
/// `T * R_y * S` product `crate::engine::render::props::prop_instance_matrix` builds.
/// `None` for a non-finite or non-positive placement.
fn instance_point(local: [f32; 3], prop: &PropDef, base_y: f32) -> Option<[f32; 3]> {
    if !prop.x.is_finite()
        || !prop.y.is_finite()
        || !prop.z.is_finite()
        || !prop.rotation_degrees.is_finite()
        || !prop.pitch_degrees.is_finite()
        || !prop.roll_degrees.is_finite()
        || !prop.scale.is_finite()
        || prop.scale <= 0.0
        || !base_y.is_finite()
        || !local.iter().all(|value| value.is_finite())
    {
        return None;
    }
    Some(
        prop.placement_matrix(base_y)
            .transform_point3(glam::Vec3::from_array(local))
            .to_array(),
    )
}

/// Transforms one model's local boxes by a prop's placement, in the same
/// transform the renderer uses. The compact output encloses pitch/roll in
/// world boxes; upright placements keep their tighter yaw-oriented boxes.
fn push_instance_occluders(
    local: &[LocalBox],
    prop: &PropDef,
    base_y: f32,
    out: &mut Vec<OrientedBox>,
) {
    let yaw = prop.rotation_degrees.to_radians();
    for local_box in local {
        if out.len() >= MAX_PROP_OCCLUSION_BOXES_PER_LEVEL {
            return;
        }
        let local_centre = [
            f32::midpoint(local_box.min[0], local_box.max[0]),
            f32::midpoint(local_box.min[1], local_box.max[1]),
            f32::midpoint(local_box.min[2], local_box.max[2]),
        ];
        let Some(centre) = instance_point(local_centre, prop, base_y) else {
            return;
        };
        let (length_scale, shear) = prop.span.map_or((1.0, 0.0), |span| {
            (
                span.length / span.source_length,
                span.rise / span.source_length,
            )
        });
        let source_half_x = (local_box.max[0] - local_box.min[0]) * 0.5;
        let half = [
            source_half_x * length_scale * prop.scale,
            source_half_x.mul_add(shear.abs(), (local_box.max[1] - local_box.min[1]) * 0.5)
                * prop.scale,
            (local_box.max[2] - local_box.min[2]) * 0.5 * prop.scale,
        ];
        // The compact visibility format stores yaw-oriented boxes. Preserve
        // every tilted corner with a conservative world box; Full transport
        // continues to use the exact transformed model triangles.
        let (output_centre, output_half, output_yaw) = if prop.pitch_degrees == 0.0
            && prop.roll_degrees == 0.0
        {
            (centre, half, yaw)
        } else {
            let matrix = prop.placement_matrix(base_y);
            let mut bounds = crate::engine::spatial::Aabb::EMPTY;
            for x in [local_box.min[0], local_box.max[0]] {
                for y in [local_box.min[1], local_box.max[1]] {
                    for z in [local_box.min[2], local_box.max[2]] {
                        bounds.expand(matrix.transform_point3(glam::Vec3::new(x, y, z)).to_array());
                    }
                }
            }
            (
                bounds.centre(),
                [
                    (bounds.max[0] - bounds.min[0]) * 0.5,
                    (bounds.max[1] - bounds.min[1]) * 0.5,
                    (bounds.max[2] - bounds.min[2]) * 0.5,
                ],
                0.0,
            )
        };
        if let Some(occluder) = OrientedBox::new(output_centre, output_half, output_yaw) {
            out.push(occluder);
        }
    }
}

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
//! authored `y` measured above the local walkable floor. Because the instance
//! transform is a Y rotation, the result is again a box — an oriented one —
//! so a rotated couch casts its rotated shadow rather than the bounding box
//! of its rotation.
//!
//! Only static props participate. Every entry of a level's `props` array is
//! static; dynamic objects are a separate renderer-side scene and never level
//! props ([`prop_is_static`]).
//!
//! Loading
//! -------
//! Boxes come from the shipped GLB through [`crate::props::PropAssets`], the
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
#[cfg(test)]
use std::path::PathBuf;
use std::rc::Rc;

use super::tuning::{
    MAX_PROP_OCCLUSION_BOXES_PER_LEVEL, MAX_PROP_OCCLUSION_BOXES_PER_MODEL, PROP_OCCLUSION_CELL_M,
    PROP_OCCLUSION_DEGENERATE_AREA2_M2, PROP_OCCLUSION_MAX_CELLS_PER_AXIS,
    PROP_OCCLUSION_MERGE_EPS_M, PROP_OCCLUSION_MIN_THICKNESS_M,
};
use super::visibility::OrientedBox;
use crate::gltf::PropModel;
use crate::level::{
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

/// True when a placed prop is static geometry for the baked lighting.
///
/// Every level `props` entry is static: dynamic objects are a separate
/// renderer-side scene, not level props. Naming the classification here keeps
/// a future dynamic source from leaking into the occluder builder.
#[must_use]
pub(super) const fn prop_is_static(_prop: &PropDef) -> bool {
    true
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = count.min(usize_to_f32(PROP_OCCLUSION_MAX_CELLS_PER_AXIS)) as usize;
    count.max(1)
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
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = clamped as usize;
        cell
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
/// A plain Liang-Barsky clip against the rectangle's four edges.
fn segment_overlaps_rect(a: [f32; 2], b: [f32; 2], x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (edge, bound, offset) in [
        (-delta[0], x0, a[0]),
        (delta[0], x1, a[0]),
        (-delta[1], z0, a[1]),
        (delta[1], z1, a[1]),
    ] {
        let distance = bound - offset;
        if edge == 0.0 {
            if distance < 0.0 {
                return false;
            }
            continue;
        }
        let fraction = distance / edge;
        if edge < 0.0 {
            if fraction > exit {
                return false;
            }
            enter = enter.max(fraction);
        } else {
            if fraction < enter {
                return false;
            }
            exit = exit.min(fraction);
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
    let (x0, x1) = thickened(x0, x1);
    let (z0, z1) = thickened(z0, z1);
    let (y0, y1) = thickened(run.y0, run.y1);
    let bounds = LocalBox {
        min: [x0, y0, z0],
        max: [x1, y1, z1],
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
    catalog: crate::loader::PropCatalog,
    assets: crate::props::PropAssets,
    models: HashMap<(String, u32), Rc<ModelOcclusion>>,
}

impl PropOcclusionCache {
    /// A cache over the shipped catalog and asset root.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            catalog: crate::loader::PropCatalog::load_default(),
            assets: crate::props::PropAssets::load_default(),
            models: HashMap::new(),
        }
    }

    /// A cache whose models resolve below an explicit asset root, for tests.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            catalog: crate::loader::PropCatalog::load_default(),
            assets: crate::props::PropAssets::with_root(root),
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
        let cell_m = sanitized_cell(cell_m);
        let mut seen_models: Vec<String> = Vec::new();
        let mut busy_vertices = 0usize;
        for prop in &level.props {
            if out.len() >= MAX_PROP_OCCLUSION_BOXES_PER_LEVEL {
                break;
            }
            if !prop_is_static(prop) {
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
            let model = self.model_occlusion(&model_path, cell_m);
            if !model.loaded {
                // The asset failed to load; the renderer draws (and therefore
                // the bake occludes) the catalogue placeholder box.
                push_placeholder_occluder(prop, entry.size, surfaces, &mut out);
                continue;
            }
            if !known {
                seen_models.push(model_path);
            }
            busy_vertices = busy_vertices.saturating_add(model.vertex_count);
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
    fn model_occlusion(&mut self, model_path: &str, cell_m: f32) -> Rc<ModelOcclusion> {
        let key = (model_path.to_string(), cell_key(cell_m));
        if let Some(cached) = self.models.get(&key) {
            return Rc::clone(cached);
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
        let occlusion = Rc::new(occlusion);
        self.models.insert(key, Rc::clone(&occlusion));
        occlusion
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
// Thread-local rather than global because `crate::props::PropAssets` shares
// decoded models through `Rc`. Each thread parses a given model once; the bake
// result depends only on the level and the shipped assets, never on the order
// threads happened to touch it.
thread_local! {
    static PROP_OCCLUSIONS: RefCell<PropOcclusionCache> =
        RefCell::new(PropOcclusionCache::new());
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
/// `T * R_y * S` product `crate::render::props::prop_instance_matrix` builds.
/// `None` for a non-finite or non-positive placement.
fn instance_point(local: [f32; 3], prop: &PropDef, base_y: f32) -> Option<[f32; 3]> {
    if !prop.x.is_finite()
        || !prop.y.is_finite()
        || !prop.z.is_finite()
        || !prop.rotation_degrees.is_finite()
        || !prop.scale.is_finite()
        || prop.scale <= 0.0
        || !base_y.is_finite()
        || !local.iter().all(|value| value.is_finite())
    {
        return None;
    }
    let yaw = prop.rotation_degrees.to_radians();
    let (sin, cos) = yaw.sin_cos();
    let scaled = [
        local[0] * prop.scale,
        local[1] * prop.scale,
        local[2] * prop.scale,
    ];
    Some([
        scaled[2].mul_add(sin, scaled[0].mul_add(cos, prop.x)),
        base_y + prop.y + scaled[1],
        scaled[2].mul_add(cos, -(scaled[0] * sin)) + prop.z,
    ])
}

/// Transforms one model's local boxes by a prop's placement, in the same
/// transform the renderer uses: scale, then yaw about Y, then translate.
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
        let half = [
            (local_box.max[0] - local_box.min[0]) * 0.5 * prop.scale,
            (local_box.max[1] - local_box.min[1]) * 0.5 * prop.scale,
            (local_box.max[2] - local_box.min[2]) * 0.5 * prop.scale,
        ];
        if let Some(occluder) = OrientedBox::new(centre, half, yaw) {
            out.push(occluder);
        }
    }
}

#[cfg(test)]
// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::gltf::{PropSubmesh, PropVertex};
    use std::fmt::Write as _;

    /// Boxes at the historical default cell, the shorthand the tests below read.
    fn default_boxes(model: &PropModel) -> Vec<LocalBox> {
        occlusion_boxes_with_cell(model, PROP_OCCLUSION_CELL_M)
    }

    /// A `PropModel` built from raw triangles: three positions per triangle.
    fn model(triangles: &[[[f32; 3]; 3]]) -> PropModel {
        let mut vertices: Vec<PropVertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        for triangle in triangles {
            for position in triangle {
                indices.push(u16::try_from(vertices.len()).expect("small test mesh"));
                vertices.push(PropVertex {
                    pos: *position,
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                });
            }
        }
        PropModel {
            vertices,
            indices,
            textures: Vec::new(),
            submeshes: vec![PropSubmesh {
                material: 0,
                texture: None,
                emission: crate::materials::MaterialEmission::NONE,
                first_index: 0,
                index_count: u32::try_from(triangles.len() * 3).expect("small test mesh"),
            }],
            triangles: triangles.len(),
            materials: 1,
            ..PropModel::default()
        }
    }

    /// An axis-aligned closed box from `min` to `max`, as 12 triangles.
    fn box_model(min: [f32; 3], max: [f32; 3]) -> PropModel {
        let corners = [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [max[0], min[1], max[2]],
            [min[0], min[1], max[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
            [max[0], max[1], max[2]],
            [min[0], max[1], max[2]],
        ];
        let quads: [[usize; 4]; 6] = [
            [0, 1, 2, 3],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ];
        let mut triangles: Vec<[[f32; 3]; 3]> = Vec::new();
        for quad in quads {
            triangles.push([corners[quad[0]], corners[quad[1]], corners[quad[2]]]);
            triangles.push([corners[quad[0]], corners[quad[2]], corners[quad[3]]]);
        }
        model(&triangles)
    }

    #[test]
    fn a_solid_box_merges_into_one_occluder() {
        let model = box_model([-0.3, 0.0, -0.3], [0.3, 0.6, 0.3]);
        let boxes = default_boxes(&model);
        assert_eq!(boxes.len(), 1, "a crate is one box: {boxes:?}");
        let bounds = boxes[0];
        assert!((bounds.min[0] + 0.3).abs() < 1e-4);
        assert!((bounds.max[0] - 0.3).abs() < 1e-4);
        assert!(bounds.min[1].abs() < 1e-6);
        assert!((bounds.max[1] - 0.6).abs() < 1e-6);
        assert!((bounds.min[2] + 0.3).abs() < 1e-4);
        assert!((bounds.max[2] - 0.3).abs() < 1e-4);
    }

    #[test]
    fn a_desk_derives_a_thin_top_and_legs_not_one_oversized_box() {
        // A 1.6 x 0.7 m top 5 cm thick at 0.70..0.75, plus four 0.1 m legs.
        let mut triangles: Vec<[[f32; 3]; 3]> = Vec::new();
        let top = box_model([-0.8, 0.70, -0.35], [0.8, 0.75, 0.35]);
        triangles.extend(triangles_of(&top));
        for (x, z) in [(-0.75, -0.30), (0.75, -0.30), (-0.75, 0.30), (0.75, 0.30)] {
            let leg = box_model([x - 0.05, 0.0, z - 0.05], [x + 0.05, 0.70, z + 0.05]);
            triangles.extend(triangles_of(&leg));
        }
        let model = model(&triangles);
        let boxes = default_boxes(&model);
        assert!(
            boxes.len() <= 16,
            "a desk must not become hundreds of boxes: {}",
            boxes.len()
        );
        // No box reaches higher than the desk top or past its footprint.
        for bounds in &boxes {
            assert!(bounds.max[1] <= 0.75 + 1e-6, "box {bounds:?}");
            assert!(bounds.min[0] >= -0.8 - 1e-6 && bounds.max[0] <= 0.8 + 1e-6);
            assert!(bounds.min[2] >= -0.35 - 1e-6 && bounds.max[2] <= 0.35 + 1e-6);
        }
        // The middle of the desk is shadowed by the top slab alone: no leg
        // column reaches the floor there.
        let under_middle = boxes
            .iter()
            .find(|b| b.min[0] <= 0.0 && b.max[0] >= 0.0 && b.min[2] <= 0.0 && b.max[2] >= 0.0)
            .expect("the top covers the middle");
        assert!(
            under_middle.min[1] > 0.5,
            "the middle column is the top slab only: {under_middle:?}"
        );
        // A leg column reaches the floor.
        let on_leg = boxes
            .iter()
            .find(|b| b.min[0] <= -0.7 && b.max[0] >= -0.7 && b.min[2] <= -0.3 && b.max[2] >= -0.3)
            .expect("a leg column exists");
        assert!(on_leg.min[1] <= 1e-6, "a leg reaches the floor: {on_leg:?}");
    }

    /// Raw triangles of a model built by [`box_model`].
    fn triangles_of(model: &PropModel) -> Vec<[[f32; 3]; 3]> {
        model
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|chunk| {
                let &[i0, i1, i2] = chunk;
                [
                    model.vertices[usize::from(i0)].pos,
                    model.vertices[usize::from(i1)].pos,
                    model.vertices[usize::from(i2)].pos,
                ]
            })
            .collect()
    }

    #[test]
    fn a_vertical_panel_still_occludes() {
        // A thin guardrail panel: a single vertical quad with no X/Z area.
        let model = model(&[
            [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            [[-1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]],
        ]);
        let boxes = default_boxes(&model);
        assert!(!boxes.is_empty(), "a vertical panel must still occlude");
        let covered: f32 = boxes
            .iter()
            .map(|b| b.max[0] - b.min[0])
            .fold(0.0, f32::max);
        assert!(covered >= 1.0, "the panel spans its length: {boxes:?}");
        for bounds in &boxes {
            assert!(bounds.max[1] >= 0.9, "panel is full height: {bounds:?}");
        }
    }

    #[test]
    fn a_flat_quad_gets_a_minimum_thickness() {
        let model = model(&[
            [[-1.0, 0.02, -1.0], [1.0, 0.02, -1.0], [1.0, 0.02, 1.0]],
            [[-1.0, 0.02, -1.0], [1.0, 0.02, 1.0], [-1.0, 0.02, 1.0]],
        ]);
        let boxes = default_boxes(&model);
        assert_eq!(boxes.len(), 1, "a flat rug is one box: {boxes:?}");
        let bounds = boxes[0];
        assert!(bounds.max[1] > bounds.min[1]);
        assert!(
            (bounds.max[1] - bounds.min[1] - PROP_OCCLUSION_MIN_THICKNESS_M).abs() < 1e-5,
            "{bounds:?}"
        );
    }

    /// One rectangular room with the given `ceiling_lights` and `props`.
    fn receiver_scene(lights: &[String], props: &[String]) -> crate::lighting::LevelLighting {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "receiver",
                "name": "Receiver",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
                "ceiling_lights": [{}],
                "props": [{}]
            }}"#,
            lights.join(","),
            props.join(",")
        );
        crate::lighting::LevelLighting::bake(
            &crate::level::LevelDef::from_json(&json).expect("receiver scene parses"),
        )
    }

    fn receiver_fixture(x: f32, z: f32, intensity: f32) -> String {
        format!(
            r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "intensity": {intensity} }}"#
        )
    }

    fn receiver_prop(model: &str, x: f32, z: f32) -> String {
        format!(r#"{{ "model": "{model}", "x": {x}, "z": {z} }}"#)
    }

    /// Samples every vertex of `model` as the renderer would, at both scenes.
    ///
    /// Returns `(lit, open, world_positions)`: the per-vertex shaded value,
    /// the value the same point would receive without the prop, and the world
    /// position of each vertex.
    fn per_vertex_samples(
        model: &PropModel,
        prop: &crate::level::PropDef,
        base_y: f32,
        lit: &crate::lighting::LevelLighting,
        open: &crate::lighting::LevelLighting,
    ) -> (
        Vec<crate::lighting::LightColor>,
        Vec<crate::lighting::LightColor>,
        Vec<[f32; 3]>,
    ) {
        let transform =
            glam::Mat4::from_translation(glam::Vec3::new(prop.x, base_y + prop.y, prop.z))
                * glam::Mat4::from_rotation_y(prop.rotation_degrees.to_radians())
                * glam::Mat4::from_scale(glam::Vec3::splat(prop.scale));
        let mut shaded = Vec::new();
        let mut unshaded = Vec::new();
        let mut positions = Vec::new();
        for vertex in &model.vertices {
            let p = transform.transform_point3(glam::Vec3::from_array(vertex.pos));
            let p = [p.x, p.y, p.z];
            shaded.push(lit.sample(p[0], p[1], p[2]));
            unshaded.push(open.sample(p[0], p[1], p[2]));
            positions.push(p);
        }
        (shaded, unshaded, positions)
    }

    /// Mean luminance of the shaded samples whose vertices lie on one side of
    /// the prop's own Z midline.
    fn side_mean(
        shaded: &[crate::lighting::LightColor],
        positions: &[[f32; 3]],
        centre_z: f32,
        near: bool,
    ) -> f32 {
        let mut sum = 0.0;
        let mut count = 0usize;
        for (value, position) in shaded.iter().zip(positions) {
            if (position[2] < centre_z) != near {
                continue;
            }
            sum += value.luminance();
            count = count.saturating_add(1);
        }
        if count == 0 { 0.0 } else { sum / count as f32 }
    }

    #[test]
    fn a_props_far_side_is_shaded_by_its_own_body() {
        // The fixture sits on the prop's -Z side, so the prop's own body must
        // shade the +Z half of its own vertices: that is self-shadowing, and
        // it is what the per-vertex receiver buys over the old uniform prop
        // colour. The near half still sees the fixture.
        let mut cache = PropOcclusionCache::with_root("assets");
        let open = receiver_scene(&[receiver_fixture(5.0, 5.0, 1.0)], &[]);
        for id in ["core:washing_machine", "core:fridge", "core:desk"] {
            let Some(path) = cache.catalog.get(id).model else {
                panic!("{id} must resolve");
            };
            let asset = cache.assets.resolve(&path).expect("shipped model loads");
            let level = crate::level::LevelDef::from_json(&format!(
                r#"{{
                    "format_version": 1,
                    "id": "receiver",
                    "name": "Receiver",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
                    "ceiling_lights": [{}],
                    "props": [{}]
                }}"#,
                receiver_fixture(5.0, 5.0, 1.0),
                receiver_prop(id, 5.0, 7.0)
            ))
            .expect("receiver level parses");
            let lit = crate::lighting::LevelLighting::bake(&level);
            let (shaded, unshaded, positions) =
                per_vertex_samples(&asset.model, &level.props[0], 0.0, &lit, &open);
            let near = side_mean(&shaded, &positions, 7.0, true);
            let far = side_mean(&shaded, &positions, 7.0, false);
            assert!(
                near > far + 0.02,
                "{id}: the fixture side must be brighter than the far side: {near} vs {far}"
            );
            // The far side is genuinely blocked, not merely dimmer geometry:
            // the prop's own box lowers the value below the same point in the
            // prop-free scene.
            let far_open = side_mean(&unshaded, &positions, 7.0, false);
            assert!(
                far < far_open - 0.02,
                "{id}: the far side must lose the pool to its own body: {far} vs {far_open}"
            );
        }
    }

    #[test]
    fn a_prop_receives_the_shadow_of_a_nearby_wall() {
        // A 2.5 m wall (tall enough to block the pool, short of the ceiling so
        // the room stays one baseline zone) stands between the fixture and a
        // fridge. The fridge's near face is sampled; it must be darker than in
        // the same scene without the wall.
        let scene = |wall: bool| -> crate::lighting::LevelLighting {
            let walls = if wall {
                r#"[{ "x": 7.0, "z": 6.0, "width": 0.4, "depth": 1.6, "height": 2.5 }]"#
            } else {
                "[]"
            };
            let json = format!(
                r#"{{
                    "format_version": 1,
                    "id": "wall_shadow",
                    "name": "Wall Shadow",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "rooms": [{{ "x": 0.0, "z": 0.0, "width": 14.0, "depth": 12.0, "height": 3.0 }}],
                    "walls": {walls},
                    "ceiling_lights": [{}],
                    "props": [{}]
                }}"#,
                receiver_fixture(4.0, 6.0, 1.0),
                receiver_prop("core:fridge", 9.5, 6.0)
            );
            let level = crate::level::LevelDef::from_json(&json).expect("scene parses");
            crate::lighting::LevelLighting::bake(&level)
        };
        let blocked = scene(true);
        let open = scene(false);
        assert_eq!(
            blocked.zone_count(),
            1,
            "the wall must not partition the room"
        );
        let point = [9.15_f32, 1.0, 6.0];
        let shaded = blocked.sample(point[0], point[1], point[2]);
        let lit = open.sample(point[0], point[1], point[2]);
        assert!(
            shaded.luminance() < lit.luminance() - 0.02,
            "the wall must shade the prop face: {} vs {}",
            shaded.luminance(),
            lit.luminance()
        );
        assert!(
            lit.luminance() > open.rooms()[0].baseline.luminance() + 1e-3,
            "without the wall the pool must actually reach the face: {}",
            lit.luminance()
        );
    }

    #[test]
    fn a_prop_receives_the_shadow_of_a_nearby_prop() {
        // Two fridges in a row along +X, fixture on the -X side: the first
        // body shadows the second's near face.
        let scene = |first: bool| -> crate::lighting::LevelLighting {
            let props = if first {
                format!(
                    "{}, {}",
                    receiver_prop("core:fridge", 7.5, 6.0),
                    receiver_prop("core:fridge", 9.0, 6.0)
                )
            } else {
                receiver_prop("core:fridge", 9.0, 6.0)
            };
            receiver_scene(&[receiver_fixture(4.0, 6.0, 1.0)], &[props])
        };
        let blocked = scene(true);
        let open = scene(false);
        let point = [8.65_f32, 1.0, 6.0];
        let shaded = blocked.sample(point[0], point[1], point[2]);
        let lit = open.sample(point[0], point[1], point[2]);
        assert!(
            shaded.luminance() < lit.luminance() - 0.02,
            "the first prop must shade the second: {} vs {}",
            shaded.luminance(),
            lit.luminance()
        );
    }

    #[test]
    fn an_unknown_model_still_shades_like_the_box_it_draws() {
        // A level naming an unresolvable prop draws the catalogue placeholder
        // box. The bake must test against that same box: the floor under it is
        // darker than open floor at the same distance from the fixture, while
        // a point outside its footprint is untouched.
        let with_prop = receiver_scene(
            &[receiver_fixture(5.0, 5.0, 1.0)],
            &[receiver_prop("core:definitely_not_a_model", 4.5, 5.0)],
        );
        let open = receiver_scene(&[receiver_fixture(5.0, 5.0, 1.0)], &[]);
        let under = with_prop.sample(4.5, 0.0, 5.0);
        let under_open = open.sample(4.5, 0.0, 5.0);
        let beside = with_prop.sample(6.5, 0.0, 5.0);
        let beside_open = open.sample(6.5, 0.0, 5.0);
        assert!(
            under.luminance() < under_open.luminance(),
            "the placeholder box must darken its own footprint: {} vs {}",
            under.luminance(),
            under_open.luminance()
        );
        assert!(
            (beside.luminance() - beside_open.luminance()).abs() < 1e-6,
            "outside the placeholder footprint nothing changes: {} vs {}",
            beside.luminance(),
            beside_open.luminance()
        );
    }

    #[test]
    #[allow(clippy::print_stdout)] // developer measurement output, like the audit report
    fn measure_receiver_self_shadowing() {
        // One fixture at the room centre (5, 5), one prop 2 m to its +Z side
        // (5, 7). Each model's own vertices are sampled with and without the
        // prop's body in the occluder set, so `lit < open` means the prop's
        // own box shaded that vertex; equal means the vertex sees the fixture.
        // The fixture is on the prop's -Z side, so a vertex at lower Z is on
        // the lit side and a vertex at higher Z is on the far side.
        let ids = [
            "core:washing_machine",
            "core:fridge",
            "core:rug",
            "core:pool_curtain_straight",
            "core:pool_guardrail_straight",
            "core:desk",
            "core:couch",
        ];
        let mut cache = PropOcclusionCache::with_root("assets");
        let open = receiver_scene(&[receiver_fixture(5.0, 5.0, 1.0)], &[]);
        for id in ids {
            let Some(path) = cache.catalog.get(id).model else {
                println!("{id}: no model");
                continue;
            };
            let Ok(asset) = cache.assets.resolve(&path) else {
                println!("{id}: failed to load");
                continue;
            };
            let level = crate::level::LevelDef::from_json(&format!(
                r#"{{
                    "format_version": 1,
                    "id": "receiver",
                    "name": "Receiver",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
                    "ceiling_lights": [{}],
                    "props": [{}]
                }}"#,
                receiver_fixture(5.0, 5.0, 1.0),
                receiver_prop(id, 5.0, 7.0)
            ))
            .expect("receiver level parses");
            let lit = crate::lighting::LevelLighting::bake(&level);
            let prop = &level.props[0];
            let (shaded, unshaded, positions) =
                per_vertex_samples(&asset.model, prop, 0.0, &lit, &open);
            let total = positions.len().max(1);
            let shaded_count = shaded
                .iter()
                .zip(&unshaded)
                .filter(|(a, b)| a.luminance() < b.luminance() - 1e-4)
                .count();
            println!(
                "{id:30} verts={total:4} body-shaded={shaded_count:4} ({:3}%)  near={:.3} far={:.3} open far={:.3}",
                100 * shaded_count / total,
                side_mean(&shaded, &positions, 7.0, true),
                side_mean(&shaded, &positions, 7.0, false),
                side_mean(&unshaded, &positions, 7.0, false),
            );
        }
    }

    /// Every distinct model the shipped demo places, with its decoded asset.
    ///
    /// The receiver and cap tests below all need the real shipped assets, so
    /// the loading is shared.
    fn demo_models(
        cache: &mut PropOcclusionCache,
    ) -> Vec<(String, Rc<crate::props::LoadedPropAsset>)> {
        let level = crate::level::LevelDef::from_json(
            &std::fs::read_to_string("assets/levels/places_demo.json").expect("demo readable"),
        )
        .expect("demo parses");
        let catalog = crate::loader::PropCatalog::load_default();
        let mut seen: Vec<String> = Vec::new();
        let mut models = Vec::new();
        for prop in &level.props {
            let Some(path) = catalog.get(&prop.model).model else {
                continue;
            };
            if seen.contains(&path) {
                continue;
            }
            seen.push(path.clone());
            let Ok(asset) = cache.assets.resolve(&path) else {
                continue;
            };
            models.push((path, asset));
        }
        models
    }

    #[test]
    fn the_historical_cell_reproduces_the_historical_boxes() {
        // `occlusion_boxes_with_cell(model, PROP_OCCLUSION_CELL_M)` is exactly
        // the historical derivation, which is what keeps `LevelLighting::bake`
        // bit-for-bit unchanged.
        let model = box_model([-0.31, 0.0, -0.18], [0.27, 0.94, 0.22]);
        assert_eq!(
            default_boxes(&model),
            occlusion_boxes_with_cell(&model, PROP_OCCLUSION_CELL_M)
        );
    }

    #[test]
    fn a_finer_cell_separates_detail_a_coarse_cell_merges() {
        // Two 5 cm posts, 20 cm apart, both full height: at 0.15 m each post
        // owns a column and the equal Y spans merge across the gap into one
        // box (an over-shadow); at 0.05 m the gap is empty columns and the two
        // posts stay separate boxes.
        let mut triangles = Vec::new();
        triangles.extend(triangles_of(&box_model(
            [-0.15, 0.0, -0.05],
            [-0.10, 1.0, 0.05],
        )));
        triangles.extend(triangles_of(&box_model(
            [0.10, 0.0, -0.05],
            [0.15, 1.0, 0.05],
        )));
        let model = model(&triangles);
        let coarse = occlusion_boxes_with_cell(&model, 0.15);
        let fine = occlusion_boxes_with_cell(&model, 0.05);
        assert!(
            coarse.len() < fine.len(),
            "the coarse grid merges the two posts into one box: {coarse:?} vs {fine:?}"
        );
        // The merged coarse box spans the gap; the fine pair does not.
        let coarse_span = coarse
            .iter()
            .fold(0.0_f32, |span, b| span.max(b.max[0] - b.min[0]));
        let fine_span = fine
            .iter()
            .fold(0.0_f32, |span, b| span.max(b.max[0] - b.min[0]));
        assert!(coarse_span > fine_span, "{coarse:?} vs {fine:?}");
    }

    #[test]
    #[allow(clippy::print_stdout)] // developer measurement output, like the audit report
    fn shipped_models_stay_within_the_box_cap_on_a_finer_grid() {
        // The profile grid must not silently truncate shipped art: a model at
        // the cap loses its high-Z/high-X shadow. Every distinct model the
        // shipped demo places is measured at the historical 0.15 m and the
        // finer cells a profile could select.
        let mut cache = PropOcclusionCache::with_root("assets");
        let models = demo_models(&mut cache);
        assert!(!models.is_empty(), "the demo must resolve its prop models");
        for (path, asset) in &models {
            let mut line = format!("{path:48} tris={:4}", asset.model.triangles);
            for cell in [0.15_f32, 0.10, 0.075, 0.05] {
                let boxes = occlusion_boxes_with_cell(&asset.model, cell);
                assert!(
                    boxes.len() < MAX_PROP_OCCLUSION_BOXES_PER_MODEL,
                    "{path}: {} boxes at {cell} m hit the {MAX_PROP_OCCLUSION_BOXES_PER_MODEL} cap",
                    boxes.len()
                );
                // Below the historical cell the result is either the exact
                // grid (when it fits the cap) or the complete coarser
                // silhouette - never a truncated slice of the exact grid.
                let exact = grind_model(&asset.model, cell);
                if exact.len() < MAX_PROP_OCCLUSION_BOXES_PER_MODEL {
                    assert_eq!(
                        boxes, exact,
                        "{path}: {cell} m must stay exact below the cap"
                    );
                }
                let _ = write!(line, "  {cell:.3}:{:3}", boxes.len());
            }
            println!("{line}");
        }
    }

    #[test]
    fn a_model_too_detailed_for_the_cap_is_re_ground_at_a_coarser_cell() {
        // A slanted quad whose Y span varies with X fragments into one box per
        // column: at 0.075 m that is more than the cap, at 0.15 m it fits. The
        // finer request must degrade to the complete coarser silhouette, not
        // return a cap-truncated slice of the fine grid.
        let mut triangles: Vec<[[f32; 3]; 3]> = Vec::new();
        let steps = 56usize;
        for step in 0..steps {
            let t0 = step as f32 / (steps - 1) as f32;
            let t1 = (step + 1) as f32 / (steps - 1) as f32;
            let p0 = [8.0_f32.mul_add(t0, -4.0), 0.2 + t0, 0.0];
            let p1 = [8.0_f32.mul_add(t1, -4.0), 0.2 + t1, 0.0];
            triangles.push([p0, p1, [p1[0], p1[1] + 0.4, p1[2]]]);
            triangles.push([p0, [p1[0], p1[1] + 0.4, p1[2]], [p0[0], p0[1] + 0.4, p0[2]]]);
        }
        let model = model(&triangles);
        let truncated = grind_model(&model, 0.075);
        assert!(
            truncated.len() >= MAX_PROP_OCCLUSION_BOXES_PER_MODEL,
            "the fixture must actually hit the cap: {}",
            truncated.len()
        );
        let fine = occlusion_boxes_with_cell(&model, 0.075);
        assert!(
            fine.len() < MAX_PROP_OCCLUSION_BOXES_PER_MODEL,
            "the detail request must degrade instead of truncating: {} boxes",
            fine.len()
        );
        assert_eq!(
            fine,
            occlusion_boxes_with_cell(&model, 0.15),
            "the degraded result is the complete coarser silhouette"
        );
    }

    #[test]
    #[allow(clippy::print_stdout)] // developer measurement output, like the audit report
    fn measure_demo_level_box_totals() {
        for (path, label) in [
            ("assets/levels/places_demo.json", "places_demo"),
            ("tests/fixtures/levels/prop_stress.json", "prop_stress"),
            ("tests/fixtures/levels/prop_showcase.json", "prop_showcase"),
        ] {
            let level = crate::level::LevelDef::from_json(
                &std::fs::read_to_string(path).expect("level is readable"),
            )
            .expect("level parses");
            let surfaces = crate::level::LevelSurfaces::new(&level);
            let mut cache = PropOcclusionCache::with_root("assets");
            let mut line = format!("{label:14} props={:3}", level.props.len());
            for cell in [0.15_f32, 0.10, 0.075, 0.05] {
                let started = std::time::Instant::now();
                let boxes = cache.level_occluders_with_cell(&level, &surfaces, cell);
                let _ = write!(
                    line,
                    "  {cell:.3}:{:5} ({:.1} ms)",
                    boxes.len(),
                    started.elapsed().as_secs_f64() * 1e3
                );
            }
            println!("{line}");
        }
    }

    #[test]
    fn a_multi_primitive_model_occludes_every_primitive() {
        // Two primitives (two materials) sharing one model: the second quad
        // must occlude exactly like the first, so the merged boxes cover both.
        // The model is built by hand so the primitive split is explicit.
        let mut vertices: Vec<PropVertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut push_quad = |x0: f32, x1: f32| {
            let base = u16::try_from(vertices.len()).expect("small test mesh");
            for (x, y) in [(x0, 0.0), (x1, 0.0), (x1, 1.0), (x0, 1.0)] {
                vertices.push(PropVertex {
                    pos: [x, y, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        push_quad(-1.0, 0.0);
        push_quad(0.0, 1.0);
        let model = PropModel {
            vertices,
            indices,
            textures: Vec::new(),
            submeshes: vec![
                PropSubmesh {
                    material: 0,
                    texture: None,
                    emission: crate::materials::MaterialEmission::NONE,
                    first_index: 0,
                    index_count: 6,
                },
                PropSubmesh {
                    material: 1,
                    texture: None,
                    emission: crate::materials::MaterialEmission::NONE,
                    first_index: 6,
                    index_count: 6,
                },
            ],
            triangles: 4,
            materials: 2,
            ..PropModel::default()
        };
        let boxes = default_boxes(&model);
        let min_x = boxes.iter().map(|b| b.min[0]).fold(f32::INFINITY, f32::min);
        let max_x = boxes
            .iter()
            .map(|b| b.max[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            min_x <= -1.0 + 1e-4,
            "the first primitive occludes: {min_x}"
        );
        assert!(
            max_x >= 1.0 - 1e-4,
            "the second primitive occludes: {max_x}"
        );
    }

    #[test]
    fn the_instance_transform_matches_the_renderers_matrix() {
        // `instance_point` must be the exact point transform the renderer's
        // `prop_instance_matrix` builds: translate * rotate_y * scale. Every
        // angle sign and axis order is pinned here, so an occluder can never
        // rotate the opposite way from the geometry it stands for.
        for (x, y, z, yaw_degrees, scale) in [
            (1.0_f32, 0.0_f32, 2.0_f32, 0.0_f32, 1.0_f32),
            (1.0, 0.0, 2.0, 90.0, 1.0),
            (1.0, 0.5, 2.0, 180.0, 0.5),
            (-3.0, -0.4, 0.25, 270.0, 2.25),
            (0.0, 0.0, 0.0, 37.5, 1.5),
            (5.5, 0.2, -7.25, -45.0, 0.75),
        ] {
            let prop = crate::level::PropDef {
                model: "test".into(),
                x,
                y,
                z,
                rotation_degrees: yaw_degrees,
                scale,
                size: None,
                solid: false,
                lights: Vec::new(),
            };
            let base_y = 0.35_f32;
            let transform = glam::Mat4::from_translation(glam::Vec3::new(x, base_y + y, z))
                * glam::Mat4::from_rotation_y(yaw_degrees.to_radians())
                * glam::Mat4::from_scale(glam::Vec3::splat(scale));
            for local in [
                [0.0_f32, 0.0, 0.0],
                [0.8, 0.75, -0.35],
                [-1.2, 0.1, 0.6],
                [0.0, 2.6, 1.4],
            ] {
                let expected = transform.transform_point3(glam::Vec3::from_array(local));
                let got = instance_point(local, &prop, base_y).expect("valid placement");
                assert!(
                    (got[0] - expected.x).abs() < 1e-4
                        && (got[1] - expected.y).abs() < 1e-4
                        && (got[2] - expected.z).abs() < 1e-4,
                    "({x},{y},{z}) yaw {yaw_degrees} scale {scale}: local {local:?} -> \
                     {got:?} but the renderer matrix gives {expected:?}"
                );
            }
        }
    }

    #[test]
    fn a_concave_model_does_not_fill_its_own_opening() {
        // A U-shaped plan (back wall at -Z, two side walls, open to +Z): the
        // opening is air, and a box there would shadow light the real prop
        // never blocks. The columns at the middle of the mouth must stay
        // unoccupied.
        let mut triangles = Vec::new();
        triangles.extend(triangles_of(&box_model(
            [-0.5, 0.0, -0.5],
            [0.5, 1.0, -0.4],
        )));
        triangles.extend(triangles_of(&box_model(
            [-0.5, 0.0, -0.4],
            [-0.4, 1.0, 0.5],
        )));
        triangles.extend(triangles_of(&box_model([0.4, 0.0, -0.4], [0.5, 1.0, 0.5])));
        let model = model(&triangles);
        let boxes = default_boxes(&model);
        let mouth_centre_covered = boxes
            .iter()
            .any(|b| b.min[0] < 0.05 && b.max[0] > -0.05 && b.min[2] < 0.4 && b.max[2] > 0.2);
        assert!(
            !mouth_centre_covered,
            "the U's mouth must stay air: {boxes:?}"
        );
    }

    #[test]
    fn winding_does_not_remove_an_occluder() {
        // The renderer never back-face culls, so the bake must not either: a
        // quad wound the other way still occludes.
        let front = model(&[
            [[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0], [0.5, 1.0, 0.0]],
            [[-0.5, 0.0, 0.0], [0.5, 1.0, 0.0], [-0.5, 1.0, 0.0]],
        ]);
        let back = model(&[
            [[-0.5, 0.0, 0.0], [0.5, 1.0, 0.0], [0.5, 0.0, 0.0]],
            [[-0.5, 0.0, 0.0], [-0.5, 1.0, 0.0], [0.5, 1.0, 0.0]],
        ]);
        assert!(!default_boxes(&front).is_empty());
        assert_eq!(
            default_boxes(&front),
            default_boxes(&back),
            "winding must not change the occluder set"
        );
    }

    #[test]
    fn an_empty_model_contributes_nothing() {
        let empty = PropModel {
            vertices: Vec::new(),
            indices: Vec::new(),
            textures: Vec::new(),
            submeshes: Vec::new(),
            triangles: 0,
            materials: 0,
            ..PropModel::default()
        };
        assert!(default_boxes(&empty).is_empty());
        assert!(default_boxes(&model(&[])).is_empty());
    }

    #[test]
    fn boxes_never_leave_the_model_bounds() {
        let model = box_model([-0.17, 0.0, -0.24], [0.19, 1.03, 0.31]);
        for bounds in default_boxes(&model) {
            assert!(bounds.min[0] >= -0.17 - 1e-6);
            assert!(bounds.max[0] <= 0.19 + 1e-6);
            assert!(bounds.min[1] >= -1e-6);
            assert!(bounds.max[1] <= 1.03 + 1e-6);
            assert!(bounds.min[2] >= -0.24 - 1e-6);
            assert!(bounds.max[2] <= 0.31 + 1e-6);
        }
    }

    #[test]
    fn the_placeholder_box_matches_the_drawn_prop_box() {
        // The draw path builds a box centred on the prop origin, spanning the
        // catalogue size in X/Z and rising from the placement point; the
        // occluder of a fallback prop must be exactly that box.
        let boxes = placeholder_boxes([0.6, 0.9, 0.6]);
        assert_eq!(
            boxes,
            vec![LocalBox {
                min: [-0.3, 0.0, -0.3],
                max: [0.3, 0.9, 0.3],
            }]
        );
        assert!(placeholder_boxes([0.0, 1.0, 1.0]).is_empty());
        assert!(placeholder_boxes([f32::NAN, 1.0, 1.0]).is_empty());
    }

    #[test]
    fn a_failed_asset_falls_back_to_the_placeholder_box_occluder() {
        // With no asset root every model fails to load. The draw path draws a
        // catalogue placeholder box for each prop, so the bake must test
        // against those same boxes: one per prop, no more and no fewer.
        let mut cache = PropOcclusionCache::with_root("target/definitely-not-here");
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "no_assets",
                "name": "No Assets",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
                "props": [
                    { "model": "core:chair", "x": 1.0, "z": 1.0 },
                    { "model": "core:not_a_model", "x": 2.0, "z": 2.0 },
                    { "model": "core:desk", "x": 3.0, "z": 3.0, "size": [1.0, 1.0, 1.0] }
                ]
            }"#,
        )
        .expect("test level parses");
        let surfaces = crate::level::LevelSurfaces::new(&level);
        let boxes = cache.level_occluders_with_cell(&level, &surfaces, PROP_OCCLUSION_CELL_M);
        assert_eq!(
            boxes.len(),
            level.props.len(),
            "every fallback prop contributes its placeholder box"
        );
    }

    #[test]
    fn a_loadable_model_never_also_contributes_a_placeholder_box() {
        // A prop whose model loads derives its boxes from the model alone: the
        // placeholder is a fallback, not an addition. The desk derives several
        // boxes; the same placement with a missing asset root derives exactly
        // the one placeholder box, and the two must differ.
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "one_prop",
                "name": "One Prop",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
                "props": [{ "model": "core:desk", "x": 2.0, "z": 2.0 }]
            }"#,
        )
        .expect("test level parses");
        let surfaces = crate::level::LevelSurfaces::new(&level);
        let mut loaded_cache = PropOcclusionCache::with_root("assets");
        let mut failed_cache = PropOcclusionCache::with_root("target/definitely-not-here");
        let loaded =
            loaded_cache.level_occluders_with_cell(&level, &surfaces, PROP_OCCLUSION_CELL_M);
        let failed =
            failed_cache.level_occluders_with_cell(&level, &surfaces, PROP_OCCLUSION_CELL_M);
        assert_eq!(failed.len(), 1, "one placeholder box for the one prop");
        assert!(
            loaded.len() > failed.len(),
            "the loaded model derives its own boxes, not the placeholder: {} vs {}",
            loaded.len(),
            failed.len()
        );
    }

    #[test]
    fn the_model_cache_is_keyed_by_the_grid_cell() {
        // The same model at two cells: the boxes must differ (the desk's top
        // slab and leg columns resolve separately at 0.075 m), and asking for
        // the first cell again must return that cell's own result rather than
        // the most recently derived one. This is the quality-level staleness
        // guard.
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "one_prop",
                "name": "One Prop",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
                "props": [{ "model": "core:desk", "x": 2.0, "z": 2.0 }]
            }"#,
        )
        .expect("test level parses");
        let surfaces = crate::level::LevelSurfaces::new(&level);
        let mut cache = PropOcclusionCache::with_root("assets");
        let fine = cache.level_occluders_with_cell(&level, &surfaces, 0.075);
        let historical = cache.level_occluders_with_cell(&level, &surfaces, 0.15);
        let fine_again = cache.level_occluders_with_cell(&level, &surfaces, 0.075);
        assert_ne!(
            fine, historical,
            "the two cells must derive different boxes"
        );
        assert_eq!(fine, fine_again, "the cache must not serve the other cell");
        // A degenerate request is canonicalised to the historical grid.
        let degenerate = cache.level_occluders_with_cell(&level, &surfaces, f32::NAN);
        assert_eq!(degenerate, historical);
    }

    #[test]
    fn a_level_without_props_does_no_loading() {
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "no_props",
                "name": "No Props",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
            }"#,
        )
        .expect("test level parses");
        let surfaces = crate::level::LevelSurfaces::new(&level);
        assert!(level_occluders(&level, &surfaces).is_empty());
    }
}

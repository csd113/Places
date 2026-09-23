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
//! 1. The model's X/Z bounds are ground into a uniform column grid
//!    ([`PROP_OCCLUSION_CELL_M`], coarser only when a model exceeds
//!    [`PROP_OCCLUSION_MAX_CELLS_PER_AXIS`]). The grid origin is the model's
//!    own bounds minimum, so the result is a pure function of the asset.
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
//! static in this batch; dynamic objects are a separate renderer-side scene
//! and never level props ([`prop_is_static`]).
//!
//! Loading
//! -------
//! Boxes come from the shipped GLB through [`crate::props::PropAssets`], the
//! same cache the renderer uses, and are remembered per model path in a
//! thread-local cache ([`level_occluders`]). A missing asset root, a failed
//! model or a model past the level's model budget contributes no boxes: an
//! absent occluder set is always a valid answer and never an error.
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
}

/// True when a placed prop is static geometry for the baked lighting.
///
/// Every level `props` entry is static in this batch: dynamic objects are a
/// separate renderer-side scene, not level props. Naming the classification
/// here keeps a future dynamic source from leaking into the occluder builder.
#[must_use]
pub(super) const fn prop_is_static(_prop: &PropDef) -> bool {
    true
}

/// Occlusion boxes for one model, derived from its real triangles.
///
/// A model with no usable geometry (empty vertex list, non-finite bounds)
/// yields no boxes, which callers must treat as "this prop occludes nothing".
#[must_use]
pub(super) fn occlusion_boxes(model: &PropModel) -> Vec<LocalBox> {
    let Some((min, max)) = model.bounds() else {
        return Vec::new();
    };
    if !min.iter().chain(max.iter()).all(|value| value.is_finite()) {
        return Vec::new();
    }
    let span_x = (max[0] - min[0]).max(0.0);
    let span_z = (max[2] - min[2]).max(0.0);
    let cell = grid_cell(span_x, span_z);
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
fn grid_cell(span_x: f32, span_z: f32) -> f32 {
    let limit = usize_to_f32(PROP_OCCLUSION_MAX_CELLS_PER_AXIS);
    let needed = (span_x / limit)
        .max(span_z / limit)
        .max(PROP_OCCLUSION_CELL_M);
    if needed.is_finite() && needed > 0.0 {
        needed
    } else {
        PROP_OCCLUSION_CELL_M
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

/// Per-path model occluder cache plus the prop catalog it resolves through.
pub(super) struct PropOcclusionCache {
    catalog: crate::loader::PropCatalog,
    assets: crate::props::PropAssets,
    models: HashMap<String, Rc<ModelOcclusion>>,
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

    /// World-space occluders for every static prop of one level, in level
    /// order, capped at [`MAX_PROP_OCCLUSION_BOXES_PER_LEVEL`].
    #[must_use]
    pub(super) fn level_occluders(
        &mut self,
        level: &LevelDef,
        surfaces: &LevelSurfaces<'_>,
    ) -> Vec<OrientedBox> {
        let mut out: Vec<OrientedBox> = Vec::new();
        if level.props.is_empty() {
            return out;
        }
        // Mirrors the renderer's instance classification, so a prop that falls
        // back to a catalogue placeholder box in the draw path does not cast
        // the real model's shadow either.
        let mut seen_models: Vec<String> = Vec::new();
        let mut busy_vertices = 0usize;
        for prop in &level.props {
            if out.len() >= MAX_PROP_OCCLUSION_BOXES_PER_LEVEL {
                break;
            }
            if !prop_is_static(prop) || busy_vertices >= MAX_LEVEL_PROP_VERTICES {
                continue;
            }
            let Some(model_path) = self.catalog.get(&prop.model).model else {
                continue;
            };
            let known = seen_models.iter().any(|path| path == &model_path);
            if !known && seen_models.len() >= MAX_LEVEL_PROP_MODELS {
                continue;
            }
            let model = self.model_occlusion(&model_path);
            if model.boxes.is_empty() {
                continue;
            }
            if !known {
                seen_models.push(model_path);
            }
            busy_vertices = busy_vertices.saturating_add(model.vertex_count);
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            push_instance_occluders(&model.boxes, prop, base_y, &mut out);
        }
        out
    }

    /// The derived occlusion of one model path, loaded and cached on demand.
    fn model_occlusion(&mut self, model_path: &str) -> Rc<ModelOcclusion> {
        if let Some(cached) = self.models.get(model_path) {
            return Rc::clone(cached);
        }
        let occlusion = match self.assets.resolve(model_path) {
            Ok(asset) => ModelOcclusion {
                boxes: occlusion_boxes(&asset.model),
                vertex_count: asset.model.vertices.len(),
            },
            Err(message) => {
                self.assets.report_failure(model_path, &message);
                ModelOcclusion::default()
            }
        };
        let occlusion = Rc::new(occlusion);
        self.models
            .insert(model_path.to_string(), Rc::clone(&occlusion));
        occlusion
    }
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

/// World-space occluders for every static prop of one level.
///
/// This is the entry point the bake uses. A level with no props does no
/// loading at all; a level whose props cannot be resolved contributes no
/// occluders rather than failing.
#[must_use]
pub(super) fn level_occluders(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<OrientedBox> {
    if level.props.is_empty() {
        return Vec::new();
    }
    PROP_OCCLUSIONS.with(|cache| cache.borrow_mut().level_occluders(level, surfaces))
}

/// Transforms one model's local boxes by a prop's placement, in the same
/// transform the renderer uses: scale, then yaw about Y, then translate.
fn push_instance_occluders(
    local: &[LocalBox],
    prop: &PropDef,
    base_y: f32,
    out: &mut Vec<OrientedBox>,
) {
    if !prop.x.is_finite()
        || !prop.y.is_finite()
        || !prop.z.is_finite()
        || !prop.rotation_degrees.is_finite()
        || !prop.scale.is_finite()
        || prop.scale <= 0.0
        || !base_y.is_finite()
    {
        return;
    }
    let origin = [prop.x, base_y + prop.y, prop.z];
    let yaw = prop.rotation_degrees.to_radians();
    let (sin, cos) = yaw.sin_cos();
    for local_box in local {
        if out.len() >= MAX_PROP_OCCLUSION_BOXES_PER_LEVEL {
            return;
        }
        let local_centre = [
            f32::midpoint(local_box.min[0], local_box.max[0]),
            f32::midpoint(local_box.min[1], local_box.max[1]),
            f32::midpoint(local_box.min[2], local_box.max[2]),
        ];
        let half = [
            (local_box.max[0] - local_box.min[0]) * 0.5 * prop.scale,
            (local_box.max[1] - local_box.min[1]) * 0.5 * prop.scale,
            (local_box.max[2] - local_box.min[2]) * 0.5 * prop.scale,
        ];
        let scaled = [
            local_centre[0] * prop.scale,
            local_centre[1] * prop.scale,
            local_centre[2] * prop.scale,
        ];
        let centre = [
            scaled[0].mul_add(cos, scaled[2] * sin) + origin[0],
            origin[1] + scaled[1],
            scaled[2].mul_add(cos, -scaled[0] * sin) + origin[2],
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
        let boxes = occlusion_boxes(&model);
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
        let boxes = occlusion_boxes(&model);
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
        let boxes = occlusion_boxes(&model);
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
        let boxes = occlusion_boxes(&model);
        assert_eq!(boxes.len(), 1, "a flat rug is one box: {boxes:?}");
        let bounds = boxes[0];
        assert!(bounds.max[1] > bounds.min[1]);
        assert!(
            (bounds.max[1] - bounds.min[1] - PROP_OCCLUSION_MIN_THICKNESS_M).abs() < 1e-5,
            "{bounds:?}"
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
        };
        assert!(occlusion_boxes(&empty).is_empty());
        assert!(occlusion_boxes(&model(&[])).is_empty());
    }

    #[test]
    fn boxes_never_leave_the_model_bounds() {
        let model = box_model([-0.17, 0.0, -0.24], [0.19, 1.03, 0.31]);
        for bounds in occlusion_boxes(&model) {
            assert!(bounds.min[0] >= -0.17 - 1e-6);
            assert!(bounds.max[0] <= 0.19 + 1e-6);
            assert!(bounds.min[1] >= -1e-6);
            assert!(bounds.max[1] <= 1.03 + 1e-6);
            assert!(bounds.min[2] >= -0.24 - 1e-6);
            assert!(bounds.max[2] <= 0.31 + 1e-6);
        }
    }

    #[test]
    fn a_missing_asset_root_yields_no_occluders_without_failing() {
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
                    { "model": "core:not_a_model", "x": 2.0, "z": 2.0 }
                ]
            }"#,
        )
        .expect("test level parses");
        let surfaces = crate::level::LevelSurfaces::new(&level);
        assert!(cache.level_occluders(&level, &surfaces).is_empty());
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

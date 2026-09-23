//! Generic architectural geometry: ramps, staircases, half walls, columns,
//! archways, guardrails, thresholds and baseboards.
//!
//! Every piece here is theme-independent architecture. The geometry is
//! generated from the level's own rectangles and slopes, and each face draws
//! whatever material the level named for it, so a Home skirting, an office
//! partition and an industrial kick plate are the same mesh with different
//! catalog ids. Nothing in this module carries a texture of its own.
//!
//! The emitters follow the same rules as the wall, floor and skirt emitters:
//!
//! * every face is a whole quad, wound so its front points out of the solid,
//!   with the project's directional face shading and the wall vocabulary's
//!   darker-bottom/brighter-top gradient on vertical faces;
//! * UVs are world-space at the material's `tile_metres`, exactly like a floor,
//!   a ceiling or a wall face, so trim and architecture share the tiling of the
//!   surfaces they stand against;
//! * static surfaces are stamped into the lightmap atlas through
//!   [`stamp_lightmap_quad`]; only the vertex-lit fallback samples the bake per
//!   corner, exactly like the wall emitter.
//!
//! The pieces that physically exist are also the pieces collision and the
//! lighting bake read ([`LevelDef::architecture_solids`]), so a wall that is
//! drawn is solid and occludes: the emitters must never invent an extra face
//! outside that volume, and must never skip a face inside it.
//!
//! [`LevelDef::architecture_solids`]: crate::level::LevelDef::architecture_solids

use super::geometry::{EmitContext, WALL_BOTTOM_GRADIENT, WALL_TOP_GRADIENT};
use super::{
    LitSurface, MaterialSlot, SurfaceKey, Vertex, WALL_FACE_EAST_MULT, WALL_FACE_NORTH_MULT,
    WALL_FACE_SOUTH_MULT, WALL_FACE_WEST_MULT, add_quad, emit_lit_surface_grid, lit_surface_grid,
    shade, stamp_lightmap_quad, tiled_uv,
};
use crate::level::{
    ArchwayDef, BaseboardDef, ColumnDef, GuardrailDef, HalfWallDef, LevelDef, LevelSurfaces,
    MaterialRef, RampDef, StairDef, ThresholdDef, WallAxis, axis_positions,
};
use crate::lighting::light_grid_cells;
use crate::lighting::lightmap::PatchKind;
use crate::spatial::SpatialBuckets;

/// Shade multiplier of a horizontal face looking up (a cap or a tread).
const FACE_UP_MULT: f32 = 1.0;
/// Shade multiplier of a horizontal face looking down.
const FACE_DOWN_MULT: f32 = 0.85;
/// Tolerance within which a cap is treated as meeting the ceiling exactly and
/// is therefore not emitted, in metres.
const FLUSH_EPS_M: f32 = 0.02;
/// Rise below which an archway soffit segment is treated as a horizontal flat
/// lintel rather than a sloped intrados, in metres. An `arch_rise` of `0.0`
/// produces exactly-flat segments; a real curve's segments climb by centimetres,
/// far above this.
const FLAT_SOFFIT_EPS_M: f32 = 1.0e-4;
/// How far outside a ramp or staircase the surrounding floor is sampled, in
/// metres. Small enough to sample the neighbour the piece actually meets, large
/// enough to clear the piece's own boundary.
const ADJACENT_PROBE_M: f32 = 0.05;

/// One planar architectural face, ready to emit.
#[derive(Clone, Copy)]
struct ArchitectureFace {
    /// Corners in winding order; the quad's front points along `normal`.
    points: [[f32; 3]; 4],
    /// Texture coordinates in the same corner order.
    uv: [[f32; 2]; 4],
    /// Which corners lie on the face's upper edge: a vertical face takes the
    /// wall gradient from these.
    top: [bool; 4],
    /// Outward normal of the face.
    normal: [f32; 3],
    /// True for a vertical face (directional shade + vertical gradient), false
    /// for a horizontal one.
    vertical: bool,
    /// Horizontal faces looking up take [`FACE_UP_MULT`], looking down
    /// [`FACE_DOWN_MULT`]. Ignored for vertical faces.
    up: bool,
    key: SurfaceKey,
    kind: PatchKind,
}

/// Applies a per-corner mapping over a quad's four values without indexing, so
/// the emitters' corner loops stay lint-clean and provably in bounds.
fn map_corners<T: Copy, U>(values: [T; 4], map: impl Fn(T) -> U) -> [U; 4] {
    let [a, b, c, d] = values;
    [map(a), map(b), map(c), map(d)]
}

/// Emits every architectural piece of the level, in authored order.
pub(super) fn emit_architecture(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    for ramp in &context.level.ramps {
        emit_ramp(context, buckets, scratch, ramp);
    }
    for stair in &context.level.stairs {
        emit_stair(context, buckets, scratch, stair);
    }
    for piece in &context.level.half_walls {
        emit_half_wall(context, buckets, scratch, piece);
    }
    for piece in &context.level.columns {
        emit_column(context, buckets, scratch, piece);
    }
    for piece in &context.level.archways {
        emit_archway(context, buckets, scratch, piece);
    }
    for rail in &context.level.guardrails {
        emit_guardrail(context, buckets, scratch, rail);
    }
    for strip in &context.level.thresholds {
        emit_threshold(context, buckets, scratch, strip);
    }
    for (index, board) in context.level.baseboards.iter().enumerate() {
        emit_baseboard(context, buckets, scratch, index, board);
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The cross product of two world vectors, as a plain `[f32; 3]`.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

/// The dot product of two world vectors.
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

/// The normal of a quad wound `p0 -> p1 -> p2`.
fn quad_normal(points: [[f32; 3]; 4]) -> [f32; 3] {
    let edge_a = [
        points[1][0] - points[0][0],
        points[1][1] - points[0][1],
        points[1][2] - points[0][2],
    ];
    let edge_b = [
        points[2][0] - points[0][0],
        points[2][1] - points[0][1],
        points[2][2] - points[0][2],
    ];
    cross(edge_a, edge_b)
}

/// Reverses a quad's winding when it points against `expected`, carrying every
/// per-corner attribute with it.
///
/// The archway, ramp and staircase quads are built from their own local axes, so
/// the winding a slope or a rotated run produces depends on the sign of the
/// slope; normalising it here keeps every emitter's geometry consistently wound
/// without a sign case in each one.
fn orient(
    points: &mut [[f32; 3]; 4],
    uv: &mut [[f32; 2]; 4],
    top: &mut [bool; 4],
    expected: [f32; 3],
) {
    if dot(quad_normal(*points), expected) >= 0.0 {
        return;
    }
    points.swap(1, 3);
    uv.swap(1, 3);
    top.swap(1, 3);
}

/// The squared distance between two world points, for the coincident-corner
/// test that decides whether a face is a triangle.
fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[2].mul_add(d[2], d[1].mul_add(d[1], d[0] * d[0]))
}

/// Wall-style directional shade for a vertical face, from its outward normal.
fn face_mult(normal: [f32; 3]) -> f32 {
    if normal[0].abs() >= normal[2].abs() {
        if normal[0] >= 0.0 {
            WALL_FACE_EAST_MULT
        } else {
            WALL_FACE_WEST_MULT
        }
    } else if normal[2] >= 0.0 {
        WALL_FACE_SOUTH_MULT
    } else {
        WALL_FACE_NORTH_MULT
    }
}

/// Emits one architectural face, clearing and flushing its quad on its own so
/// multi-material pieces stay simple.
fn emit_face(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    face: ArchitectureFace,
) {
    let mult = if face.vertical {
        face_mult(face.normal)
    } else if face.up {
        FACE_UP_MULT
    } else {
        FACE_DOWN_MULT
    };
    let tint = context.materials.tint(face.key);
    let scaled = |factor: f32| {
        [
            (tint[0] * mult * factor).min(1.0),
            (tint[1] * mult * factor).min(1.0),
            (tint[2] * mult * factor).min(1.0),
        ]
    };
    let bottom_shade = scaled(WALL_BOTTOM_GRADIENT);
    let top_shade = scaled(WALL_TOP_GRADIENT);
    let flat_shade = scaled(1.0);
    let base: [[f32; 3]; 4] = std::array::from_fn(|index| {
        if !face.vertical {
            return flat_shade;
        }
        if *face.top.get(index).unwrap_or(&false) {
            top_shade
        } else {
            bottom_shade
        }
    });
    let lightmapped = context.lightmapped();
    let colors: [[f32; 3]; 4] = if lightmapped {
        base
    } else {
        std::array::from_fn(|index| {
            let point = face.points.get(index).copied().unwrap_or_default();
            let base = base.get(index).copied().unwrap_or_default();
            shade(base, context.lighting.sample(point[0], point[1], point[2]))
        })
    };
    scratch.clear();
    // A face whose two consecutive corners coincide is really a triangle (a
    // ramp's side skirt that lands flush on a floor has no height at one end).
    // Reorder it into the quad form the mesh pipeline expects, with the
    // repeated corner last: the index pass then drops the zero-area second
    // triangle instead of storing it, and the lightmap chart stays a valid
    // frame over the triangle's own corners.
    let (points, colors, uv) = fold_triangle_to_quad(face.points, colors, face.uv);
    add_quad(
        scratch, points[0], colors[0], uv[0], points[1], colors[1], uv[1], points[2], colors[2],
        uv[2], points[3], colors[3], uv[3],
    );
    let room = if lightmapped {
        let centre = [
            (face.points[0][0] + face.points[1][0] + face.points[2][0] + face.points[3][0]) * 0.25,
            (face.points[0][1] + face.points[1][1] + face.points[2][1] + face.points[3][1]) * 0.25,
            (face.points[0][2] + face.points[1][2] + face.points[2][2] + face.points[3][2]) * 0.25,
        ];
        context
            .lighting
            .room_index_at_height(centre[0], centre[1], centre[2])
    } else {
        None
    };
    stamp_lightmap_quad(context.lightmap, scratch, 0, face.kind, points, room);
    buckets.add_quads(face.key, scratch);
}

/// Reorders one face's per-corner attributes so a coincident adjacent pair
/// lands on the last two corners.
///
/// A genuine quad is returned unchanged. For a triangle-shaped face the three
/// distinct corners keep their winding order and the third is repeated;
/// [`crate::spatial`]'s index pass recognises `corner[2] == corner[3]` and
/// emits a single triangle.
fn fold_triangle_to_quad<T: Copy + Default>(
    points: [[f32; 3]; 4],
    colors: [T; 4],
    uv: [[f32; 2]; 4],
) -> ([[f32; 3]; 4], [T; 4], [[f32; 2]; 4]) {
    for first in 0..4usize {
        let second = if first == 3 {
            0
        } else {
            first.saturating_add(1)
        };
        let a = points.get(first).copied().unwrap_or_default();
        let b = points.get(second).copied().unwrap_or_default();
        if distance_squared(a, b) > 1.0e-12 {
            continue;
        }
        // Drop the first corner of the coincident pair; the three survivors are
        // the triangle's corners in winding order, and the last repeats. The
        // order is built with `from_fn` so no index ever needs checking.
        let source = |slot: usize| {
            let slot = if slot >= 3 { 2 } else { slot };
            let sum = second.saturating_add(slot);
            if sum >= 4 { sum.saturating_sub(4) } else { sum }
        };
        return (
            std::array::from_fn(|slot| points.get(source(slot)).copied().unwrap_or_default()),
            std::array::from_fn(|slot| colors.get(source(slot)).copied().unwrap_or_default()),
            std::array::from_fn(|slot| uv.get(source(slot)).copied().unwrap_or_default()),
        );
    }
    (points, colors, uv)
}

/// The four corners of a horizontal rectangle at world Y `y`, wound so the quad
/// faces up, with its UVs in the same order.
fn horizontal_quad(
    axis: WallAxis,
    span: (f32, f32),
    across: (f32, f32),
    y: f32,
    uv: impl Fn([f32; 3]) -> [f32; 2],
) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
    let (a0, a1) = span;
    let (b0, b1) = across;
    let points: [[f32; 3]; 4] = match axis {
        WallAxis::X => [[a0, y, b1], [a1, y, b1], [a1, y, b0], [a0, y, b0]],
        WallAxis::Z => [[b1, y, a0], [b1, y, a1], [b0, y, a1], [b0, y, a0]],
    };
    let uvs = points.map(uv);
    (points, uvs)
}

/// Emits one horizontal quad (a cap or a tread) facing up.
#[allow(clippy::too_many_arguments)] // matches the other quad emitters in this module
fn emit_horizontal(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    key: SurfaceKey,
    kind: PatchKind,
    axis: WallAxis,
    span: (f32, f32),
    across: (f32, f32),
    y: f32,
    tile: f32,
) {
    let (points, uv) = horizontal_quad(axis, span, across, y, |point| match axis {
        WallAxis::X => tiled_uv(point[0], point[2], tile),
        WallAxis::Z => tiled_uv(point[2], point[0], tile),
    });
    emit_face(
        context,
        buckets,
        scratch,
        ArchitectureFace {
            points,
            uv,
            top: [false; 4],
            normal: [0.0, 1.0, 0.0],
            vertical: false,
            up: true,
            key,
            kind,
        },
    );
}

/// The material key for one surface reference, with a fallback to the level's
/// wall default.
fn wall_key(context: &EmitContext<'_, '_>, material: Option<MaterialRef<'_>>) -> SurfaceKey {
    context.materials.key(
        MaterialSlot::Wall,
        material.unwrap_or_else(|| context.level.defaults.wall_ref()),
    )
}

/// The material key for one floor surface reference, with a fallback to the
/// level's floor default.
fn floor_key(context: &EmitContext<'_, '_>, material: Option<MaterialRef<'_>>) -> SurfaceKey {
    context.materials.key(
        MaterialSlot::Floor,
        material.unwrap_or_else(|| context.level.defaults.floor_ref()),
    )
}

// ---------------------------------------------------------------------------
// Ramps
// ---------------------------------------------------------------------------

/// Emits one ramp: its sloped top surface, its two closed sides and, where the
/// author left the ends standing in the air, the end faces that close it.
fn emit_ramp(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    ramp: &RampDef,
) {
    let (x0, x1, z0, z1) = ramp.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    if !ramp.width.is_finite()
        || !ramp.depth.is_finite()
        || ramp.width <= 0.0
        || ramp.depth <= 0.0
        || !ramp.base_offset().is_finite()
        || !ramp.rise().is_finite()
    {
        return;
    }
    let Some(room_floor) = context.surfaces.room_floor_y_at(cx, cz) else {
        return;
    };
    let Some(room_index) = context.surfaces.room_index_at(cx, cz) else {
        return;
    };
    let key = floor_key(context, ramp.floor_ref());
    let tile = context.materials.tile_metres(key);
    emit_ramp_surface(
        context, buckets, scratch, ramp, room_floor, room_index, key, tile,
    );
    // The sides and the ends are closed down to whatever floor they actually
    // meet, so a ramp on a raised platform is never a floating slab.
    let edge_key = wall_key(context, ramp.edge_ref());
    let edge_tile = context.materials.tile_metres(edge_key);
    emit_ramp_sides(
        context, buckets, scratch, ramp, room_floor, edge_key, edge_tile,
    );
    emit_ramp_ends(
        context, buckets, scratch, ramp, room_floor, edge_key, edge_tile,
    );
}

/// The ramp's sloped top surface, sampled on the same height function the
/// walkable model answers with.
#[allow(clippy::too_many_arguments)] // one surface's frame and material
fn emit_ramp_surface(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    ramp: &RampDef,
    room_floor: f32,
    room_index: usize,
    key: SurfaceKey,
    tile: f32,
) {
    let (x0, x1, z0, z1) = ramp.bounds();
    let y_at = |x: f32, z: f32| room_floor + ramp.offset_at(x, z);
    let cells_x = light_grid_cells(ramp.width.abs());
    let cells_z = light_grid_cells(ramp.depth.abs());
    let xs = axis_positions(x0, x1 - x0, cells_x);
    let zs = axis_positions(z0, z1 - z0, cells_z);
    let colors = lit_surface_grid(
        context.lighting,
        room_index,
        &xs,
        &zs,
        y_at,
        Some(context.materials.tint(key)),
        context.lightmapped(),
    );
    scratch.clear();
    emit_lit_surface_grid(
        scratch,
        &xs,
        &zs,
        &colors,
        LitSurface {
            y_at,
            ceiling: false,
            region: None,
        },
        |x, z| tiled_uv(x, z, tile),
        PatchKind::Floor,
        Some(room_index),
        context.lightmap,
    );
    buckets.add_quads(key, scratch);
}

/// The ramp's two closed sides, each dropping from the sloped edge to the floor
/// just outside it.
fn emit_ramp_sides(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    ramp: &RampDef,
    room_floor: f32,
    edge_key: SurfaceKey,
    edge_tile: f32,
) {
    let (x0, x1, z0, z1) = ramp.bounds();
    let axis = ramp.axis();
    let (across_low, across_high) = match axis {
        WallAxis::X => (z0, z1),
        WallAxis::Z => (x0, x1),
    };
    let surface_at = |fraction: f32| room_floor + ramp.rise().mul_add(fraction, ramp.base_offset());
    for side in [-1.0f32, 1.0] {
        // The floor beside the ramp can step along the run: a ramp arriving at
        // a raised platform reads the platform's own height at its far end.
        // Sampling only that end used to collapse the whole skirt to nothing,
        // leaving the wedge open. Sample along the run and bottom the skirt at
        // the lowest floor it stands beside, so the face always closes down to
        // (or below) whatever it meets.
        let mut bottom = f32::INFINITY;
        for sample in 0..=4u8 {
            #[allow(clippy::cast_precision_loss)] // four samples, far below 2^24
            let fraction = f32::from(sample) / 4.0;
            let (px, pz) = ramp.side_probe(side, fraction, ADJACENT_PROBE_M);
            bottom = bottom.min(
                context
                    .surfaces
                    .floor_y_at(px, pz)
                    .unwrap_or_else(|| surface_at(fraction)),
            );
        }
        let top_low = surface_at(0.0);
        let top_high = surface_at(1.0);
        let bottom = bottom.min(top_low).min(top_high);
        if top_low - bottom <= 1e-4 && top_high - bottom <= 1e-4 {
            continue;
        }
        let at = if side < 0.0 { across_low } else { across_high };
        let expected = match axis {
            WallAxis::X => [0.0, 0.0, side],
            WallAxis::Z => [side, 0.0, 0.0],
        };
        let span = match axis {
            WallAxis::X => (x0, x1),
            WallAxis::Z => (z0, z1),
        };
        let mut points: [[f32; 3]; 4] = match axis {
            WallAxis::X => [
                [span.0, bottom, at],
                [span.1, bottom, at],
                [span.1, top_high, at],
                [span.0, top_low, at],
            ],
            WallAxis::Z => [
                [at, bottom, span.0],
                [at, bottom, span.1],
                [at, top_high, span.1],
                [at, top_low, span.0],
            ],
        };
        let along_of = |point: [f32; 3]| match axis {
            WallAxis::X => point[0],
            WallAxis::Z => point[2],
        };
        // A corner belongs to the face's upper edge when it sits on the sloped
        // line from `(span.0, top_low)` to `(span.1, top_high)` and that line
        // stands clear of the face's bottom. A ramp end that lands flush on a
        // floor collapses the upper edge onto the bottom there: that corner is
        // classified as a bottom corner, and its duplicate carries the same
        // flag. The winding normalisation may reverse the corner order and the
        // triangle fold drops one of the two coincident corners, so a flag that
        // differed between the duplicates could survive on the wrong corner and
        // rotate the wall gradient to run along the run instead of up the face
        // — a shade step exactly at the junction with the floor.
        let mut top_flags: [bool; 4] = map_corners(points, |point| {
            let along = along_of(point);
            let fraction = ((along - span.0) / (span.1 - span.0)).clamp(0.0, 1.0);
            let edge = (top_high - top_low).mul_add(fraction, top_low);
            edge - bottom > 1e-4 && (point[1] - edge).abs() <= 1e-4
        });
        let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
            tiled_uv(along_of(point), top_low.max(top_high) - point[1], edge_tile)
        });
        orient(&mut points, &mut uv, &mut top_flags, expected);
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: top_flags,
                normal: expected,
                vertical: true,
                up: false,
                key: edge_key,
                kind: PatchKind::Skirt,
            },
        );
    }
}

/// The ramp's two end faces: closed against the floors they meet, and skipped
/// where the ramp lands flush on one (the landing's own floor already owns that
/// plane, so a second face there would be two coplanar surfaces).
fn emit_ramp_ends(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    ramp: &RampDef,
    room_floor: f32,
    edge_key: SurfaceKey,
    edge_tile: f32,
) {
    let (x0, x1, z0, z1) = ramp.bounds();
    let axis = ramp.axis();
    let (across_low, across_high) = match axis {
        WallAxis::X => (z0, z1),
        WallAxis::Z => (x0, x1),
    };
    for end in [0.0f32, 1.0] {
        let end_y = room_floor + ramp.rise().mul_add(end, ramp.base_offset());
        // Probe outside the end along the run, so the sample lands on the
        // neighbouring floor rather than inside the ramp.
        let probe_fraction = if end < 0.5 {
            -ADJACENT_PROBE_M / ramp.length().max(1e-3)
        } else {
            1.0 + ADJACENT_PROBE_M / ramp.length().max(1e-3)
        };
        let (probe_x, probe_z) = span_point(axis, x0, x1, z0, z1, probe_fraction);
        let adjacent = context
            .surfaces
            .floor_y_at(probe_x, probe_z)
            .unwrap_or(end_y);
        if (adjacent - end_y).abs() <= FLUSH_EPS_M {
            continue;
        }
        let sign = if end > 0.5 { 1.0 } else { -1.0 };
        let expected = match axis {
            WallAxis::X => [sign, 0.0, 0.0],
            WallAxis::Z => [0.0, 0.0, sign],
        };
        let at = match axis {
            WallAxis::X => {
                if end > 0.5 {
                    x1
                } else {
                    x0
                }
            }
            WallAxis::Z => {
                if end > 0.5 {
                    z1
                } else {
                    z0
                }
            }
        };
        let span = (across_low, across_high);
        let (bottom, top) = (adjacent.min(end_y), adjacent.max(end_y));
        let mut points: [[f32; 3]; 4] = match axis {
            WallAxis::X => [
                [at, bottom, span.0],
                [at, bottom, span.1],
                [at, top, span.1],
                [at, top, span.0],
            ],
            WallAxis::Z => [
                [span.0, bottom, at],
                [span.1, bottom, at],
                [span.1, top, at],
                [span.0, top, at],
            ],
        };
        let mut top_flags = [false, false, true, true];
        let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
            let along = match axis {
                WallAxis::X => point[2],
                WallAxis::Z => point[0],
            };
            tiled_uv(along, top - point[1], edge_tile)
        });
        orient(&mut points, &mut uv, &mut top_flags, expected);
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: top_flags,
                normal: expected,
                vertical: true,
                up: false,
                key: edge_key,
                kind: PatchKind::Wall,
            },
        );
    }
}

/// A point at run fraction `fraction` (0 or 1 for the ends, or just outside)
/// of an axis-aligned footprint.
fn span_point(axis: WallAxis, x0: f32, x1: f32, z0: f32, z1: f32, fraction: f32) -> (f32, f32) {
    match axis {
        WallAxis::X => ((x1 - x0).mul_add(fraction, x0), f32::midpoint(z0, z1)),
        WallAxis::Z => (f32::midpoint(x0, x1), (z1 - z0).mul_add(fraction, z0)),
    }
}

// ---------------------------------------------------------------------------
// Staircases
// ---------------------------------------------------------------------------

/// The resolved constants of one staircase, shared by its sub-emitters.
struct StairFrame {
    axis: WallAxis,
    across_low: f32,
    across_high: f32,
    /// World Y of the walking surface at the foot of the flight.
    base_y: f32,
    /// Height of one riser, in metres.
    riser: f32,
    steps: u32,
    /// World Y of the floor just outside the foot: where the first riser
    /// starts when the flight stands above the room floor.
    foot_floor: f32,
    tread_key: SurfaceKey,
    tread_tile: f32,
    riser_key: SurfaceKey,
    riser_tile: f32,
    side_key: SurfaceKey,
    side_tile: f32,
}

impl StairFrame {
    /// World Y of tread `step`'s top surface.
    fn tread_y(&self, step: u32) -> f32 {
        self.riser
            .mul_add(u32_to_f32(step.saturating_add(1)), self.base_y)
    }
}

/// Emits one staircase: closed sides, risers, treads and a landing face.
fn emit_stair(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    stair: &StairDef,
) {
    let (x0, x1, z0, z1) = stair.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    if !stair.width.is_finite()
        || !stair.depth.is_finite()
        || stair.width <= 0.0
        || stair.depth <= 0.0
        || stair.step_count() == 0
        || stair.rise() <= 0.0
    {
        return;
    }
    let Some(room_floor) = context.surfaces.room_floor_y_at(cx, cz) else {
        return;
    };
    let axis = stair.axis();
    let steps = stair.step_count();
    let (head_x, head_z) = span_point(axis, x0, x1, z0, z1, 1.0);
    let probe = ADJACENT_PROBE_M;
    // Just outside the flight's ends: the floor a walker meets at the foot and
    // the floor the top tread lands on (a raised platform closes the flight's
    // own end face, so it is only emitted when nothing else does).
    let outside = |fraction: f32, before: bool| -> (f32, f32) {
        let (x, z) = span_point(axis, x0, x1, z0, z1, fraction);
        let offset = if before { -probe } else { probe };
        match axis {
            WallAxis::X => (x + offset, z),
            WallAxis::Z => (x, z + offset),
        }
    };
    let foot_floor = context
        .surfaces
        .floor_y_at(outside(0.0, true).0, outside(0.0, true).1)
        .unwrap_or_else(|| room_floor + stair.base_offset());
    let frame = StairFrame {
        axis,
        across_low: if axis == WallAxis::X { z0 } else { x0 },
        across_high: if axis == WallAxis::X { z1 } else { x1 },
        base_y: room_floor + stair.base_offset(),
        riser: stair.rise() / u32_to_f32(steps),
        steps,
        foot_floor,
        tread_key: floor_key(context, stair.tread_ref()),
        tread_tile: 0.0,
        riser_key: wall_key(context, stair.riser_ref()),
        riser_tile: 0.0,
        side_key: wall_key(context, stair.side_ref()),
        side_tile: 0.0,
    };
    let frame = StairFrame {
        tread_tile: context.materials.tile_metres(frame.tread_key),
        riser_tile: context.materials.tile_metres(frame.riser_key),
        side_tile: context.materials.tile_metres(frame.side_key),
        ..frame
    };
    for step in 0..steps {
        emit_stair_step(context, buckets, scratch, stair, &frame, step);
    }
    emit_stair_landing(context, buckets, scratch, &frame, head_x, head_z);
}

/// One tread of a staircase: its two closed side panels, its riser and its top.
fn emit_stair_step(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    stair: &StairDef,
    frame: &StairFrame,
    step: u32,
) {
    let tread_y = frame.tread_y(step);
    let (span_start, span_end) = stair.tread_span(step);

    for side in [-1.0f32, 1.0] {
        emit_stair_side(
            context,
            buckets,
            scratch,
            stair,
            frame,
            side,
            (span_start, span_end),
            tread_y,
        );
    }

    // The riser at this tread's front edge. The first one starts at the
    // surrounding floor (or the authored foot offset, whichever is lower), so a
    // flight standing above the room floor is closed at its foot too.
    let riser_bottom = if step == 0 {
        frame.base_y.min(frame.foot_floor)
    } else {
        frame.riser.mul_add(u32_to_f32(step), frame.base_y)
    };
    let expected = match frame.axis {
        WallAxis::X => [-1.0, 0.0, 0.0],
        WallAxis::Z => [0.0, 0.0, -1.0],
    };
    let mut points: [[f32; 3]; 4] = match frame.axis {
        WallAxis::X => [
            [span_start, riser_bottom, frame.across_low],
            [span_start, riser_bottom, frame.across_high],
            [span_start, tread_y, frame.across_high],
            [span_start, tread_y, frame.across_low],
        ],
        WallAxis::Z => [
            [frame.across_low, riser_bottom, span_start],
            [frame.across_high, riser_bottom, span_start],
            [frame.across_high, tread_y, span_start],
            [frame.across_low, tread_y, span_start],
        ],
    };
    let mut top_flags = [false, false, true, true];
    let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
        let along = match frame.axis {
            WallAxis::X => point[2],
            WallAxis::Z => point[0],
        };
        tiled_uv(along, tread_y - point[1], frame.riser_tile)
    });
    orient(&mut points, &mut uv, &mut top_flags, expected);
    emit_face(
        context,
        buckets,
        scratch,
        ArchitectureFace {
            points,
            uv,
            top: top_flags,
            normal: expected,
            vertical: true,
            up: false,
            key: frame.riser_key,
            kind: PatchKind::Wall,
        },
    );

    emit_horizontal(
        context,
        buckets,
        scratch,
        frame.tread_key,
        PatchKind::Floor,
        frame.axis,
        (span_start, span_end),
        (frame.across_low, frame.across_high),
        tread_y,
        frame.tread_tile,
    );
}

/// One closed side panel of a staircase step, dropping from the tread to the
/// surrounding floor line outside the flight.
///
/// Each side takes one flat base height, sampled just outside that side at both
/// ends of the flight: adjacent panels then share their bottom edge, the profile
/// reads as a closed stringer, and no panel can collapse into a zero-area quad
/// where the floor beside the flight rises to meet the treads.
#[allow(clippy::too_many_arguments)] // one step's frame, side and material
fn emit_stair_side(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    stair: &StairDef,
    frame: &StairFrame,
    side: f32,
    span: (f32, f32),
    tread_y: f32,
) {
    let (span_start, span_end) = span;
    let beside = |fraction: f32| -> f32 {
        let (x, z) = stair.side_probe(side, fraction, ADJACENT_PROBE_M);
        context.surfaces.floor_y_at(x, z).unwrap_or(frame.base_y)
    };
    let bottom = beside(0.02).min(beside(0.98));
    if tread_y - bottom <= 1e-4 {
        return;
    }
    let at = if side < 0.0 {
        frame.across_low
    } else {
        frame.across_high
    };
    let expected = match frame.axis {
        WallAxis::X => [0.0, 0.0, side],
        WallAxis::Z => [side, 0.0, 0.0],
    };
    let mut points: [[f32; 3]; 4] = match frame.axis {
        WallAxis::X => [
            [span_start, bottom, at],
            [span_end, bottom, at],
            [span_end, tread_y, at],
            [span_start, tread_y, at],
        ],
        WallAxis::Z => [
            [at, bottom, span_start],
            [at, bottom, span_end],
            [at, tread_y, span_end],
            [at, tread_y, span_start],
        ],
    };
    let mut top_flags = [false, false, true, true];
    let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
        let along = match frame.axis {
            WallAxis::X => point[0],
            WallAxis::Z => point[2],
        };
        tiled_uv(along, tread_y - point[1], frame.side_tile)
    });
    orient(&mut points, &mut uv, &mut top_flags, expected);
    emit_face(
        context,
        buckets,
        scratch,
        ArchitectureFace {
            points,
            uv,
            top: top_flags,
            normal: expected,
            vertical: true,
            up: false,
            key: frame.side_key,
            kind: PatchKind::Skirt,
        },
    );
}

/// The landing face at the top of a staircase, emitted only when the top tread
/// stands above the floor beyond it: a flight that lands on a platform of the
/// same height is already closed by that platform's own skirt.
fn emit_stair_landing(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    frame: &StairFrame,
    head_x: f32,
    head_z: f32,
) {
    let top_y = frame.tread_y(frame.steps.saturating_sub(1));
    let (beyond_x, beyond_z) = match frame.axis {
        WallAxis::X => (head_x + ADJACENT_PROBE_M, head_z),
        WallAxis::Z => (head_x, head_z + ADJACENT_PROBE_M),
    };
    let beyond = context
        .surfaces
        .floor_y_at(beyond_x, beyond_z)
        .unwrap_or(top_y);
    if (beyond - top_y).abs() <= FLUSH_EPS_M {
        return;
    }
    let expected = match frame.axis {
        WallAxis::X => [1.0, 0.0, 0.0],
        WallAxis::Z => [0.0, 0.0, 1.0],
    };
    let (bottom, top) = (beyond.min(top_y), beyond.max(top_y));
    let mut points: [[f32; 3]; 4] = match frame.axis {
        WallAxis::X => [
            [head_x, bottom, frame.across_low],
            [head_x, bottom, frame.across_high],
            [head_x, top, frame.across_high],
            [head_x, top, frame.across_low],
        ],
        WallAxis::Z => [
            [frame.across_low, bottom, head_z],
            [frame.across_high, bottom, head_z],
            [frame.across_high, top, head_z],
            [frame.across_low, top, head_z],
        ],
    };
    let mut top_flags = [false, false, true, true];
    let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
        let along = match frame.axis {
            WallAxis::X => point[2],
            WallAxis::Z => point[0],
        };
        tiled_uv(along, top - point[1], frame.side_tile)
    });
    orient(&mut points, &mut uv, &mut top_flags, expected);
    emit_face(
        context,
        buckets,
        scratch,
        ArchitectureFace {
            points,
            uv,
            top: top_flags,
            normal: expected,
            vertical: true,
            up: false,
            key: frame.side_key,
            kind: PatchKind::Wall,
        },
    );
}

/// `u32` to `f32` for the small counts (steps, segments) the level bounds.
#[allow(clippy::cast_precision_loss)] // bounded by level validation, far below 2^24
const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

// ---------------------------------------------------------------------------
// Half walls, columns and archways
// ---------------------------------------------------------------------------

/// Emits a solid box's visible faces: four sides, a top cap and (when the box
/// stands clear of the floor) a bottom cap.
///
/// `cap` is skipped when the box's top meets the local ceiling exactly, which
/// is what keeps a full-height column from z-fighting the ceiling plane; the
/// caller decides that by passing `None`.
fn emit_box(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    bounds: ([f32; 3], [f32; 3]),
    keys: BoxKeys,
) {
    let (min, max) = bounds;
    let (x0, y0, z0) = (min[0], min[1], min[2]);
    let (x1, y1, z1) = (max[0], max[1], max[2]);
    if !(x1 > x0 && y1 > y0 && z1 > z0) {
        return;
    }
    let tile = context.materials.tile_metres(keys.faces);
    // The four vertical faces. `faces` covers the two across-thickness sides,
    // `ends` the two short ends.
    let verticals: [([[f32; 3]; 4], [f32; 3], SurfaceKey); 4] = [
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [1.0, 0.0, 0.0],
            keys.ends,
        ),
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [-1.0, 0.0, 0.0],
            keys.ends,
        ),
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [0.0, 0.0, 1.0],
            keys.faces,
        ),
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [0.0, 0.0, -1.0],
            keys.faces,
        ),
    ];
    for (points, normal, key) in verticals {
        let face_tile = context.materials.tile_metres(key);
        let uv: [[f32; 2]; 4] = points.map(|point| match normal {
            [1.0 | -1.0, 0.0, 0.0] => tiled_uv(point[2], y1 - point[1], face_tile),
            _ => tiled_uv(point[0], y1 - point[1], face_tile),
        });
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: [false, false, true, true],
                normal,
                vertical: true,
                up: false,
                key,
                kind: PatchKind::Wall,
            },
        );
    }
    emit_box_caps(context, buckets, scratch, (min, max), keys, tile);
}

/// Emits a solid box's top and (when it stands clear of the floor) bottom caps.
fn emit_box_caps(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    bounds: ([f32; 3], [f32; 3]),
    keys: BoxKeys,
    tile: f32,
) {
    let (min, max) = bounds;
    let (x0, z0) = (min[0], min[2]);
    let (x1, z1) = (max[0], max[2]);
    let (y0, y1) = (min[1], max[1]);
    if let Some(cap) = keys.cap {
        emit_horizontal(
            context,
            buckets,
            scratch,
            cap,
            PatchKind::Wall,
            WallAxis::X,
            (x0, x1),
            (z0, z1),
            y1,
            context.materials.tile_metres(cap),
        );
    }
    if let Some(bottom) = keys.bottom {
        let (points, uv) = horizontal_quad(WallAxis::X, (x0, x1), (z0, z1), y0, |point| {
            tiled_uv(point[0], point[2], tile)
        });
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: [false; 4],
                normal: [0.0, -1.0, 0.0],
                vertical: false,
                up: false,
                key: bottom,
                kind: PatchKind::Wall,
            },
        );
    }
}

/// The resolved material keys of one solid box.
#[derive(Clone, Copy)]
struct BoxKeys {
    /// The two faces along the length axis.
    faces: SurfaceKey,
    /// The two short ends.
    ends: SurfaceKey,
    /// The top cap, or `None` when the top meets the ceiling exactly.
    cap: Option<SurfaceKey>,
    /// The bottom cap, or `None` when the box stands on the floor.
    bottom: Option<SurfaceKey>,
}

/// Resolves the keys of a box-like piece whose length faces, ends and cap have
/// their own material references.
fn box_keys(
    context: &EmitContext<'_, '_>,
    faces: Option<MaterialRef<'_>>,
    ends: Option<MaterialRef<'_>>,
    cap: Option<MaterialRef<'_>>,
    cap_flush: bool,
    bottom_visible: bool,
) -> BoxKeys {
    let face_key = wall_key(context, faces);
    BoxKeys {
        faces: face_key,
        ends: wall_key(context, ends),
        cap: if cap_flush {
            None
        } else {
            Some(wall_key(context, cap))
        },
        bottom: bottom_visible.then_some(face_key),
    }
}

/// Emits one half wall.
fn emit_half_wall(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    piece: &HalfWallDef,
) {
    let Some(boxed) = piece.solid_box(context.surfaces) else {
        return;
    };
    let (x0, x1, z0, z1) = piece.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    let floor = context.surfaces.floor_y_at(cx, cz).unwrap_or(boxed.min[1]);
    let ceiling = context.surfaces.ceiling_y_at(cx, cz);
    let keys = box_keys(
        context,
        piece.material_ref(),
        piece.end_ref(),
        piece.cap_ref(),
        (boxed.max[1] - ceiling).abs() <= FLUSH_EPS_M,
        boxed.min[1] - floor > 0.02,
    );
    emit_box(context, buckets, scratch, (boxed.min, boxed.max), keys);
}

/// Emits one column / post.
fn emit_column(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    piece: &ColumnDef,
) {
    let Some(boxed) = piece.solid_box(context.surfaces) else {
        return;
    };
    let (x0, x1, z0, z1) = piece.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    let floor = context.surfaces.floor_y_at(cx, cz).unwrap_or(boxed.min[1]);
    let ceiling = context.surfaces.ceiling_y_at(cx, cz);
    let keys = box_keys(
        context,
        piece.material_ref(),
        piece.material_ref(),
        piece.cap_ref(),
        (boxed.max[1] - ceiling).abs() <= FLUSH_EPS_M,
        boxed.min[1] - floor > 0.02,
    );
    emit_box(context, buckets, scratch, (boxed.min, boxed.max), keys);
}

/// The resolved frame of one archway, shared by its sub-emitters.
struct ArchwayFrame {
    axis: WallAxis,
    /// World coordinate of the block's run origin (its minimum corner along the
    /// length axis).
    origin: f32,
    /// The block's two across planes.
    t0: f32,
    t1: f32,
    base: f32,
    top: f32,
    /// World Y of the springing line: where the arch leaves the piers.
    spring: f32,
    length: f32,
    open_start: f32,
    open_end: f32,
    key: SurfaceKey,
    reveal_key: SurfaceKey,
    tile: f32,
    reveal_tile: f32,
}

impl ArchwayFrame {
    /// World point at (run offset, height, across offset).
    fn world(&self, along: f32, y: f32, across: f32) -> [f32; 3] {
        match self.axis {
            WallAxis::X => [self.origin + along, y, across],
            WallAxis::Z => [across, y, self.origin + along],
        }
    }

    /// World Y of the arch curve at run offset `along`.
    fn arch_y(&self, piece: &ArchwayDef, along: f32) -> f32 {
        self.base + piece.arch_height_at(along)
    }
}

/// Emits one archway: its two faces, the arched soffit, the jamb reveals and
/// the block's own ends and top cap.
fn emit_archway(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    piece: &ArchwayDef,
) {
    let (x0, x1, z0, z1) = piece.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    if !piece.width.is_finite()
        || !piece.depth.is_finite()
        || !piece.height.is_finite()
        || piece.width <= 0.0
        || piece.depth <= 0.0
        || piece.height <= 0.0
    {
        return;
    }
    let Some(base) = context.surfaces.floor_y_at(cx, cz) else {
        return;
    };
    let axis = piece.axis();
    let length = piece.length();
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    let (open_start, open_end) = piece.opening_span();
    let key = wall_key(context, piece.material_ref());
    let reveal_key = wall_key(context, piece.reveal_ref());
    let frame = ArchwayFrame {
        axis,
        origin: if axis == WallAxis::X { x0 } else { z0 },
        t0: if axis == WallAxis::X { z0 } else { x0 },
        t1: if axis == WallAxis::X { z1 } else { x1 },
        base,
        top: base + piece.height,
        spring: base + piece.spring_height(),
        length,
        open_start,
        open_end,
        key,
        reveal_key,
        tile: context.materials.tile_metres(key),
        reveal_tile: context.materials.tile_metres(reveal_key),
    };
    emit_archway_faces(context, buckets, scratch, piece, &frame);
    emit_archway_soffit(context, buckets, scratch, piece, &frame);
    emit_archway_reveals(context, buckets, scratch, &frame);
    let ceiling = context.surfaces.ceiling_y_at(cx, cz);
    if (frame.top - ceiling).abs() > FLUSH_EPS_M {
        emit_horizontal(
            context,
            buckets,
            scratch,
            frame.key,
            PatchKind::Wall,
            frame.axis,
            (0.0, length),
            (frame.t0, frame.t1),
            frame.top,
            frame.tile,
        );
    }
}

/// The archway's two faces: its piers and the spandrel above the arch.
fn emit_archway_faces(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    piece: &ArchwayDef,
    frame: &ArchwayFrame,
) {
    let segments = crate::level::ARCHWAY_SEGMENTS;
    for (across, expected) in [(frame.t0, -1.0f32), (frame.t1, 1.0f32)] {
        let normal = match frame.axis {
            WallAxis::X => [0.0, 0.0, expected],
            WallAxis::Z => [expected, 0.0, 0.0],
        };
        // Piers: rectangles either side of the opening, full height.
        for (start, end) in [(0.0, frame.open_start), (frame.open_end, frame.length)] {
            if end - start <= 1e-4 {
                continue;
            }
            let mut points = [
                frame.world(start, frame.base, across),
                frame.world(end, frame.base, across),
                frame.world(end, frame.top, across),
                frame.world(start, frame.top, across),
            ];
            let mut top_flags = [false, false, true, true];
            let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
                let along = world_along(frame.axis, point, frame.origin);
                tiled_uv(along, frame.top - point[1], frame.tile)
            });
            orient(&mut points, &mut uv, &mut top_flags, normal);
            emit_face(
                context,
                buckets,
                scratch,
                ArchitectureFace {
                    points,
                    uv,
                    top: top_flags,
                    normal,
                    vertical: true,
                    up: false,
                    key: frame.key,
                    kind: PatchKind::Wall,
                },
            );
        }
        // The spandrel above the arch, one quad per flat segment.
        for segment in 0..segments {
            let a0 = flat_segment_offset(frame.open_start, frame.open_end, segments, segment);
            let a1 = flat_segment_offset(
                frame.open_start,
                frame.open_end,
                segments,
                segment.saturating_add(1),
            );
            let y0 = frame.arch_y(piece, a0);
            let y1 = frame.arch_y(piece, a1);
            let mut points = [
                frame.world(a0, frame.top, across),
                frame.world(a1, frame.top, across),
                frame.world(a1, y1, across),
                frame.world(a0, y0, across),
            ];
            let mut top_flags = [true, true, false, false];
            let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
                let along = world_along(frame.axis, point, frame.origin);
                tiled_uv(along, frame.top - point[1], frame.tile)
            });
            orient(&mut points, &mut uv, &mut top_flags, normal);
            emit_face(
                context,
                buckets,
                scratch,
                ArchitectureFace {
                    points,
                    uv,
                    top: top_flags,
                    normal,
                    vertical: true,
                    up: false,
                    key: frame.key,
                    kind: PatchKind::Wall,
                },
            );
        }
    }
}

/// The arch's soffit: one quad per flat segment, spanning the wall thickness.
fn emit_archway_soffit(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    piece: &ArchwayDef,
    frame: &ArchwayFrame,
) {
    let segments = crate::level::ARCHWAY_SEGMENTS;
    for segment in 0..segments {
        let a0 = flat_segment_offset(frame.open_start, frame.open_end, segments, segment);
        let a1 = flat_segment_offset(
            frame.open_start,
            frame.open_end,
            segments,
            segment.saturating_add(1),
        );
        let y0 = frame.arch_y(piece, a0);
        let y1 = frame.arch_y(piece, a1);
        let mut points = [
            frame.world(a0, y0, frame.t0),
            frame.world(a1, y1, frame.t0),
            frame.world(a1, y1, frame.t1),
            frame.world(a0, y0, frame.t1),
        ];
        let normal = [0.0, -1.0, 0.0];
        // A segment with no rise is the flat lintel's underside: a horizontal
        // face looking down, which takes the same flat `FACE_DOWN_MULT` shade as
        // a box's bottom cap. Only a segment that actually climbs keeps the
        // vertical wall-style gradient of the curved intrados.
        let horizontal = (y1 - y0).abs() <= FLAT_SOFFIT_EPS_M;
        let mut top_flags = [false, false, true, true];
        let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
            let along = world_along(frame.axis, point, frame.origin);
            let across = world_across(frame.axis, point);
            tiled_uv(along, across, frame.reveal_tile)
        });
        orient(&mut points, &mut uv, &mut top_flags, normal);
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: if horizontal { [false; 4] } else { top_flags },
                normal,
                vertical: !horizontal,
                up: false,
                key: frame.reveal_key,
                kind: PatchKind::Wall,
            },
        );
    }
}

/// The archway's jamb reveals and its two block ends.
fn emit_archway_reveals(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    frame: &ArchwayFrame,
) {
    // Jamb reveals: the vertical faces inside the opening, up to the springing
    // line where the arch leaves the piers.
    let jamb_top = frame.spring.min(frame.top);
    for (along, expected) in [(frame.open_start, 1.0f32), (frame.open_end, -1.0f32)] {
        let normal = match frame.axis {
            WallAxis::X => [expected, 0.0, 0.0],
            WallAxis::Z => [0.0, 0.0, expected],
        };
        let mut points = [
            frame.world(along, frame.base, frame.t0),
            frame.world(along, frame.base, frame.t1),
            frame.world(along, jamb_top, frame.t1),
            frame.world(along, jamb_top, frame.t0),
        ];
        let mut top_flags = [false, false, true, true];
        let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
            let across = world_across(frame.axis, point);
            tiled_uv(across, jamb_top - point[1], frame.reveal_tile)
        });
        orient(&mut points, &mut uv, &mut top_flags, normal);
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: top_flags,
                normal,
                vertical: true,
                up: false,
                key: frame.reveal_key,
                kind: PatchKind::Wall,
            },
        );
    }

    // The block's own ends.
    for (along, expected) in [(0.0f32, -1.0f32), (frame.length, 1.0f32)] {
        let normal = match frame.axis {
            WallAxis::X => [expected, 0.0, 0.0],
            WallAxis::Z => [0.0, 0.0, expected],
        };
        let mut points = [
            frame.world(along, frame.base, frame.t0),
            frame.world(along, frame.base, frame.t1),
            frame.world(along, frame.top, frame.t1),
            frame.world(along, frame.top, frame.t0),
        ];
        let mut top_flags = [false, false, true, true];
        let mut uv: [[f32; 2]; 4] = map_corners(points, |point| {
            let across = world_across(frame.axis, point);
            tiled_uv(across, frame.top - point[1], frame.tile)
        });
        orient(&mut points, &mut uv, &mut top_flags, normal);
        emit_face(
            context,
            buckets,
            scratch,
            ArchitectureFace {
                points,
                uv,
                top: top_flags,
                normal,
                vertical: true,
                up: false,
                key: frame.key,
                kind: PatchKind::Wall,
            },
        );
    }
}

/// The world run coordinate of a point on an axis-aligned piece.
fn world_along(axis: WallAxis, point: [f32; 3], origin: f32) -> f32 {
    match axis {
        WallAxis::X => point[0] - origin,
        WallAxis::Z => point[2] - origin,
    }
}

/// The world across coordinate of a point on an axis-aligned piece.
const fn world_across(axis: WallAxis, point: [f32; 3]) -> f32 {
    match axis {
        WallAxis::X => point[2],
        WallAxis::Z => point[0],
    }
}

/// Run offset of flat arch segment `index`'s start, `index` in
/// `0..=segments`.
fn flat_segment_offset(open_start: f32, open_end: f32, segments: u32, index: u32) -> f32 {
    if segments == 0 {
        return open_start;
    }
    let fraction = u32_to_f32(index.min(segments)) / u32_to_f32(segments);
    (open_end - open_start).mul_add(fraction, open_start)
}

// ---------------------------------------------------------------------------
// Guardrails, thresholds and baseboards
// ---------------------------------------------------------------------------

/// A local frame for a rotated trim run: `along` runs from the piece's anchor
/// point and `across` perpendicular to it, both in the world's XZ plane.
///
/// The yaw convention matches decals and floor rotation: `rotation_degrees: 0`
/// runs east (+X), 90 north (-Z), 180 west and 270 south. Expressing every face
/// in this frame keeps the rotated pieces' corner order and UVs readable while
/// the world points are computed in one place.
struct RunFrame {
    origin: (f32, f32),
    along: (f32, f32),
    across: (f32, f32),
}

impl RunFrame {
    fn new(origin: (f32, f32), rotation_degrees: f32) -> Self {
        let radians = rotation_degrees.to_radians();
        Self {
            origin,
            along: (radians.cos(), -radians.sin()),
            across: (radians.sin(), radians.cos()),
        }
    }

    /// World point at run distance `along`, across offset `across` and height
    /// `y`.
    fn point(&self, along: f32, across: f32, y: f32) -> [f32; 3] {
        [
            self.along
                .0
                .mul_add(along, self.across.0.mul_add(across, self.origin.0)),
            y,
            self.along
                .1
                .mul_add(along, self.across.1.mul_add(across, self.origin.1)),
        ]
    }

    /// World direction of a face normal expressed in the run's own frame.
    fn normal(&self, along: f32, across: f32) -> [f32; 3] {
        [
            self.along.0.mul_add(along, self.across.0 * across),
            0.0,
            self.along.1.mul_add(along, self.across.1 * across),
        ]
    }
}

/// Emits one face of a rotated trim piece from local `(along, across, y)`
/// corners, with UVs supplied in the same corner order.
#[allow(clippy::too_many_arguments)] // one face's frame and material
fn emit_local_face(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    key: SurfaceKey,
    kind: PatchKind,
    frame: &RunFrame,
    locals: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    expected_normal: [f32; 3],
    vertical: bool,
    up: bool,
) {
    let mut points: [[f32; 3]; 4] = locals.map(|[along, across, y]| frame.point(along, across, y));
    let mut top_flags = [false, false, true, true];
    let mut uv = uv;
    orient(&mut points, &mut uv, &mut top_flags, expected_normal);
    emit_face(
        context,
        buckets,
        scratch,
        ArchitectureFace {
            points,
            uv,
            top: top_flags,
            normal: expected_normal,
            vertical,
            up,
            key,
            kind,
        },
    );
}

/// Emits one guardrail: its two rails and its posts.
fn emit_guardrail(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    rail: &GuardrailDef,
) {
    if !rail.length.is_finite()
        || rail.length <= 0.0
        || !rail.x.is_finite()
        || !rail.z.is_finite()
        || !rail.rotation_degrees.is_finite()
    {
        return;
    }
    let frame = RunFrame::new((rail.x, rail.z), rail.rotation_degrees);
    let base = rail.base_y(context.surfaces);
    let key = wall_key(context, rail.material_ref());
    let post_key = wall_key(context, rail.post_ref());
    let tile = context.materials.tile_metres(key);
    let post_tile = context.materials.tile_metres(post_key);
    let run = rail.length;
    let rise = rail.resolved_rise(context.surfaces);
    let base_at = |along: f32| rise.mul_add(along / run, base);
    let half_width = crate::level::GUARDRAIL_RAIL_WIDTH_M * 0.5;
    let top_height = rail.height();
    let top_thickness = crate::level::GUARDRAIL_RAIL_THICKNESS_M;

    // The top rail and the lower rail: the same run at two heights, both with
    // their own top, sides and ends. The undersides are never visible.
    for (rail_top, thickness) in [
        (top_height, top_thickness),
        (
            crate::level::GUARDRAIL_MIDRAIL_TOP_M,
            crate::level::GUARDRAIL_MIDRAIL_THICKNESS_M,
        ),
    ] {
        emit_rail_run(
            context, buckets, scratch, key, tile, &frame, &base_at, run, rail_top, thickness,
            half_width,
        );
    }

    // Posts, from the base line to the underside of the top rail.
    let spacing = rail.post_spacing();
    let post_half = crate::level::GUARDRAIL_POST_SIZE_M * 0.5;
    let post_top_at = |along: f32| base_at(along) + top_height - top_thickness;
    let mut previous = f32::NEG_INFINITY;
    for index in 0..=512u32 {
        let along = u32_to_f32(index) * spacing;
        if along > run + 1e-3 {
            break;
        }
        let at = along.min(run);
        if at - previous < post_half {
            continue;
        }
        previous = at;
        emit_post(
            context,
            buckets,
            scratch,
            post_key,
            post_tile,
            &frame,
            at,
            base_at(at),
            post_top_at(at),
            post_half,
        );
    }
    if run - previous > post_half {
        emit_post(
            context,
            buckets,
            scratch,
            post_key,
            post_tile,
            &frame,
            run,
            base_at(run),
            post_top_at(run),
            post_half,
        );
    }
}

/// Emits one rail of a guardrail run: its top, its two sides and its two ends.
#[allow(clippy::too_many_arguments)] // one rail's full frame
fn emit_rail_run(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    key: SurfaceKey,
    tile: f32,
    frame: &RunFrame,
    base_at: &impl Fn(f32) -> f32,
    run: f32,
    rail_top: f32,
    thickness: f32,
    half_width: f32,
) {
    let top_at = |along: f32| base_at(along) + rail_top;
    let bottom_at = |along: f32| base_at(along) + rail_top - thickness;
    if top_at(0.0) - bottom_at(0.0) <= 1e-4 && top_at(run) - bottom_at(run) <= 1e-4 {
        return;
    }
    // The top face: along the run, across the rail's width.
    let top_locals = [
        [0.0, -half_width, top_at(0.0)],
        [run, -half_width, top_at(run)],
        [run, half_width, top_at(run)],
        [0.0, half_width, top_at(0.0)],
    ];
    let top_uv = top_locals.map(|[along, across, _]| tiled_uv(along, across, tile));
    emit_local_face(
        context,
        buckets,
        scratch,
        key,
        PatchKind::Wall,
        frame,
        top_locals,
        top_uv,
        [0.0, 1.0, 0.0],
        false,
        true,
    );

    // The two sides and the two ends.
    for side in [-1.0f32, 1.0] {
        let locals = [
            [0.0, side * half_width, bottom_at(0.0)],
            [run, side * half_width, bottom_at(run)],
            [run, side * half_width, top_at(run)],
            [0.0, side * half_width, top_at(0.0)],
        ];
        let uv = locals.map(|[along, _, y]| tiled_uv(along, top_at(along) - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            frame,
            locals,
            uv,
            frame.normal(0.0, side),
            true,
            false,
        );
    }
    for (at, sign) in [(0.0f32, -1.0f32), (run, 1.0f32)] {
        let top = top_at(at);
        let locals = [
            [at, -half_width, bottom_at(at)],
            [at, half_width, bottom_at(at)],
            [at, half_width, top],
            [at, -half_width, top],
        ];
        let uv = locals.map(|[_, across, y]| tiled_uv(across, top - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            frame,
            locals,
            uv,
            frame.normal(sign, 0.0),
            true,
            false,
        );
    }
}

/// Emits one square guardrail post between two heights.
#[allow(clippy::too_many_arguments)] // one post's frame and material
fn emit_post(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    key: SurfaceKey,
    tile: f32,
    frame: &RunFrame,
    at: f32,
    bottom: f32,
    top: f32,
    half: f32,
) {
    if top - bottom <= 1e-3 {
        return;
    }
    // The face's own local sense decides the UV axis, never the world normal:
    // a rail rotated 90 degrees has a world normal whose X component is zero,
    // and choosing the UV axis from that collapsed the face onto a single texel
    // column. `along_facing` is true for the two faces whose normal runs along
    // the rail, whose horizontal spread is the `across` axis.
    let faces: [([[f32; 3]; 4], [f32; 3], bool); 4] = [
        (
            [
                [at + half, -half, bottom],
                [at + half, half, bottom],
                [at + half, half, top],
                [at + half, -half, top],
            ],
            frame.normal(1.0, 0.0),
            true,
        ),
        (
            [
                [at - half, half, bottom],
                [at - half, -half, bottom],
                [at - half, -half, top],
                [at - half, half, top],
            ],
            frame.normal(-1.0, 0.0),
            true,
        ),
        (
            [
                [at - half, half, bottom],
                [at + half, half, bottom],
                [at + half, half, top],
                [at - half, half, top],
            ],
            frame.normal(0.0, 1.0),
            false,
        ),
        (
            [
                [at + half, -half, bottom],
                [at - half, -half, bottom],
                [at - half, -half, top],
                [at + half, -half, top],
            ],
            frame.normal(0.0, -1.0),
            false,
        ),
    ];
    for (locals, normal, along_facing) in faces {
        let uv = locals.map(|[along, across, y]| {
            let horizontal = if along_facing { across } else { along };
            tiled_uv(horizontal, top - y, tile)
        });
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            frame,
            locals,
            uv,
            normal,
            true,
            false,
        );
    }
}

/// Emits one threshold strip: a raised, collision-free floor transition.
fn emit_threshold(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    strip: &ThresholdDef,
) {
    if !strip.length.is_finite()
        || strip.length <= 0.0
        || !strip.x.is_finite()
        || !strip.z.is_finite()
        || !strip.rotation_degrees.is_finite()
    {
        return;
    }
    // An authored `y` is absolute; otherwise the strip sits on the walkable
    // floor under its centre.
    let base = if strip.y.is_some() {
        strip.base_y(context.surfaces)
    } else {
        context
            .surfaces
            .floor_y_at(strip.x, strip.z)
            .unwrap_or_else(|| strip.base_y(context.surfaces))
    };
    // A strip is trim, not a floor: it batches as a wall-kind surface so its
    // vertical faces never join the floor batch, while its material (and the
    // level's floor default when it authors none) still decides its look.
    let key = context.materials.key(
        MaterialSlot::Wall,
        strip
            .material_ref()
            .unwrap_or_else(|| context.level.defaults.floor_ref()),
    );
    let tile = context.materials.tile_metres(key);
    let frame = RunFrame::new((strip.x, strip.z), strip.rotation_degrees);
    let top = base + strip.height();
    let half_length = strip.length * 0.5;
    let half_thickness = strip.thickness() * 0.5;

    // The top face carries the transition profile across the doorway.
    let top_locals = [
        [-half_length, -half_thickness, top],
        [half_length, -half_thickness, top],
        [half_length, half_thickness, top],
        [-half_length, half_thickness, top],
    ];
    let top_uv = top_locals.map(|[along, across, _]| tiled_uv(along, across, tile));
    emit_local_face(
        context,
        buckets,
        scratch,
        key,
        PatchKind::Wall,
        &frame,
        top_locals,
        top_uv,
        [0.0, 1.0, 0.0],
        false,
        true,
    );

    // The two long sides and the two ends; the underside is buried in the
    // floor the strip sits on.
    for side in [-1.0f32, 1.0] {
        let locals = [
            [-half_length, side * half_thickness, base],
            [half_length, side * half_thickness, base],
            [half_length, side * half_thickness, top],
            [-half_length, side * half_thickness, top],
        ];
        let uv = locals.map(|[along, _, y]| tiled_uv(along, top - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            &frame,
            locals,
            uv,
            frame.normal(0.0, side),
            true,
            false,
        );
    }
    for (at, sign) in [(-half_length, -1.0f32), (half_length, 1.0f32)] {
        let locals = [
            [at, -half_thickness, base],
            [at, half_thickness, base],
            [at, half_thickness, top],
            [at, -half_thickness, top],
        ];
        let uv = locals.map(|[_, across, y]| tiled_uv(across, top - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            &frame,
            locals,
            uv,
            frame.normal(sign, 0.0),
            true,
            false,
        );
    }
}

/// One half-plane in a baseboard run's local `(along, across)` frame.
///
/// The run keeps the side where `along * n_along + across * n_across >= offset`.
/// A joint with an earlier run subtracts that run's cap rectangle, which is the
/// intersection of four of these planes.
#[derive(Clone, Copy, Debug)]
struct BaseboardPlane {
    n_along: f32,
    n_across: f32,
    offset: f32,
}

impl BaseboardPlane {
    /// The signed side value of a local point; non-negative is inside.
    fn value(&self, along: f32, across: f32) -> f32 {
        along.mul_add(self.n_along, across * self.n_across) - self.offset
    }
}

/// One earlier run's cap rectangle expressed in `board`'s local frame.
///
/// The four planes bound the rectangle (`along` along the other run, `across`
/// across it, each `>= 0` inward).
fn other_cap_planes(other: &BaseboardDef, board: &BaseboardDef) -> [BaseboardPlane; 4] {
    let (dx, dz) = board.direction();
    let (ax, az) = board.across();
    let (odx, odz) = other.direction();
    let (oax, oaz) = other.across();
    let run = other.length;
    let thickness = other.thickness();
    // A world plane through `corner` with inward normal `n` becomes
    // `a*along + b*across >= c` with `a = dir.n`, `b = across.n` and
    // `c = (corner - board.origin).n`.
    let plane = |n: (f32, f32), corner: (f32, f32)| BaseboardPlane {
        n_along: n.0.mul_add(dx, n.1 * dz),
        n_across: n.0.mul_add(ax, n.1 * az),
        offset: n.0.mul_add(corner.0 - board.x, n.1 * (corner.1 - board.z)),
    };
    [
        plane((odx, odz), (other.x, other.z)),
        plane(
            (-odx, -odz),
            (odx.mul_add(run, other.x), odz.mul_add(run, other.z)),
        ),
        plane((oax, oaz), (other.x, other.z)),
        plane(
            (-oax, -oaz),
            (
                oax.mul_add(thickness, other.x),
                oaz.mul_add(thickness, other.z),
            ),
        ),
    ]
}

/// The joint information for one end of a baseboard run.
struct BaseboardJoint {
    /// True when another run's cap contains this end's back corner, so this
    /// run's end face sits inside the other run and is not drawn.
    joined: bool,
    /// Earlier runs whose cap rectangles this run's cap must subtract; the
    /// later-authored run gives up the overlap so the two caps never share a
    /// coplanar surface.
    trims: Vec<[BaseboardPlane; 4]>,
}

/// Every other baseboard run whose cap contains this run's end corner.
///
/// Only runs of the same cap height and base count: caps at different heights
/// never share a plane, so they cannot flicker. Parallel runs are skipped — a
/// collinear overlap is duplicate authoring, not a corner, and trimming by the
/// other's planes would cut the whole run.
fn baseboard_joint(
    level: &LevelDef,
    index: usize,
    board: &BaseboardDef,
    surfaces: &LevelSurfaces<'_>,
    start: bool,
) -> BaseboardJoint {
    let mut joint = BaseboardJoint {
        joined: false,
        trims: Vec::new(),
    };
    let (px, pz) = if start {
        (board.x, board.z)
    } else {
        board.point_at(1.0, 0.0)
    };
    if !px.is_finite() || !pz.is_finite() {
        return joint;
    }
    let (dx, dz) = board.direction();
    let base = board.base_y(surfaces);
    for (other_index, other) in level.baseboards.iter().enumerate() {
        if other_index == index {
            continue;
        }
        if (other.height() - board.height()).abs() > 1.0e-4
            || (other.base_y(surfaces) - base).abs() > 1.0e-4
        {
            continue;
        }
        let (odx, odz) = other.direction();
        if dx.mul_add(odx, dz * odz).abs() > 0.99 {
            continue;
        }
        let rel_x = px - other.x;
        let rel_z = pz - other.z;
        let along = odx.mul_add(rel_x, odz * rel_z);
        let (oax, oaz) = other.across();
        let across = oax.mul_add(rel_x, oaz * rel_z);
        let tol = 1.0e-3;
        if along < -tol
            || along > other.length + tol
            || across < -tol
            || across > other.thickness() + tol
        {
            continue;
        }
        joint.joined = true;
        if other_index < index {
            joint.trims.push(other_cap_planes(other, board));
        }
    }
    joint
}

/// Clips a convex polygon, keeping the side where `plane.value >= 0`.
fn clip_polygon_keep(polygon: &[[f32; 2]], plane: BaseboardPlane) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut previous = polygon.last().copied();
    let mut previous_value = previous.map(|[x, y]| plane.value(x, y));
    for [x, y] in polygon.iter().copied() {
        let value = plane.value(x, y);
        if value >= 0.0 {
            if let (Some([px, py]), Some(previous_value)) = (previous, previous_value)
                && previous_value < 0.0
            {
                let t = previous_value / (previous_value - value);
                out.push([(x - px).mul_add(t, px), (y - py).mul_add(t, py)]);
            }
            out.push([x, y]);
        } else if let (Some([px, py]), Some(previous_value)) = (previous, previous_value)
            && previous_value >= 0.0
        {
            let t = previous_value / (previous_value - value);
            out.push([(x - px).mul_add(t, px), (y - py).mul_add(t, py)]);
        }
        previous = Some([x, y]);
        previous_value = Some(value);
    }
    out
}

/// Subtracts a cap rectangle (four planes) from convex pieces.
///
/// Each piece is split by the plane; the outside part is kept as its own convex
/// piece and the inside part continues to the next plane. The final inside
/// remainder is the intersection and is dropped, so the union of the returned
/// pieces is exactly the input minus the rectangle.
fn subtract_cap_rectangle(
    mut pieces: Vec<Vec<[f32; 2]>>,
    planes: [BaseboardPlane; 4],
) -> Vec<Vec<[f32; 2]>> {
    let mut outside: Vec<Vec<[f32; 2]>> = Vec::new();
    for plane in planes {
        let mut next: Vec<Vec<[f32; 2]>> = Vec::new();
        for piece in pieces {
            let outer = clip_polygon_keep(
                &piece,
                BaseboardPlane {
                    n_along: -plane.n_along,
                    n_across: -plane.n_across,
                    offset: -plane.offset,
                },
            );
            if outer.len() >= 3 {
                outside.push(outer);
            }
            let inner = clip_polygon_keep(&piece, plane);
            if inner.len() >= 3 {
                next.push(inner);
            }
        }
        pieces = next;
        if pieces.is_empty() {
            break;
        }
    }
    outside
}

/// The `along` interval at `across = thickness` that a cap rectangle covers.
///
/// The front face is a line in plan; the rectangle's four planes each cut it to
/// one half-line, and their intersection is the interval the front face loses.
fn cap_front_gap(planes: &[BaseboardPlane; 4], thickness: f32) -> Option<(f32, f32)> {
    let mut low = f32::NEG_INFINITY;
    let mut high = f32::INFINITY;
    for plane in planes {
        let base = plane.n_across.mul_add(thickness, -plane.offset);
        if plane.n_along.abs() < 1.0e-6 {
            if base < 0.0 {
                return None;
            }
            continue;
        }
        let bound = -base / plane.n_along;
        if plane.n_along > 0.0 {
            low = low.max(bound);
        } else {
            high = high.min(bound);
        }
    }
    if low.is_finite() && high.is_finite() && high - low > 1.0e-4 {
        Some((low, high))
    } else {
        None
    }
}

/// Emits one baseboard run: its front face, its top edge and its two ends,
/// trimmed at each end that butts into an earlier run.
///
/// Two runs that both reach a corner overlap in the corner square, and their
/// caps would be two coplanar, overlapping surfaces — a flicker at every
/// 90-degree corner. The later-authored run gives up the overlap: its cap is
/// the set difference against the earlier run's cap rectangle, its front face
/// loses the covered length, and the end face hidden inside the earlier run is
/// skipped. The earlier run keeps its own boards whole, so the corner stays
/// closed with no gap and no coincident surface pair.
#[allow(clippy::too_many_lines)] // one trim-aware emitter with its three faces
fn emit_baseboard(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    index: usize,
    board: &BaseboardDef,
) {
    if !board.length.is_finite()
        || board.length <= 0.0
        || !board.x.is_finite()
        || !board.z.is_finite()
        || !board.rotation_degrees.is_finite()
    {
        return;
    }
    let base = if board.y.is_some() {
        board.base_y(context.surfaces)
    } else {
        context
            .surfaces
            .floor_y_at(board.x, board.z)
            .unwrap_or_else(|| board.base_y(context.surfaces))
    };
    let key = wall_key(context, board.material_ref());
    let tile = context.materials.tile_metres(key);
    let frame = RunFrame::new((board.x, board.z), board.rotation_degrees);
    let top = base + board.height();
    let thickness = board.thickness();
    let run = board.length;

    // Joints: other runs this board's ends butt into. An earlier run is kept
    // whole; this board subtracts its cap rectangle and skips the end face
    // hidden inside it.
    let start_joint = baseboard_joint(context.level, index, board, context.surfaces, true);
    let end_joint = baseboard_joint(context.level, index, board, context.surfaces, false);
    let mut cap_pieces: Vec<Vec<[f32; 2]>> = vec![vec![
        [0.0, 0.0],
        [run, 0.0],
        [run, thickness],
        [0.0, thickness],
    ]];
    let mut front_gaps: Vec<(f32, f32)> = Vec::new();
    let mut trim_start = start_joint.joined;
    let mut trim_end = end_joint.joined;
    for (is_start, joint) in [(true, &start_joint), (false, &end_joint)] {
        for planes in &joint.trims {
            let before: f32 = cap_pieces.iter().map(|piece| polygon_area(piece)).sum();
            cap_pieces = subtract_cap_rectangle(cap_pieces, *planes);
            let after: f32 = cap_pieces.iter().map(|piece| polygon_area(piece)).sum();
            if before - after > 1.0e-6 {
                if is_start {
                    trim_start = true;
                } else {
                    trim_end = true;
                }
            }
            if let Some(gap) = cap_front_gap(planes, thickness) {
                front_gaps.push(gap);
            }
        }
    }
    // Slivers below a square millimetre are float residue of the subtraction,
    // not trim: dropping them keeps the mesh free of near-degenerate triangles.
    cap_pieces.retain(|piece| polygon_area(piece) > 1.0e-6);

    // The front face: the run's full length minus every covered interval.
    let mut spans: Vec<(f32, f32)> = vec![(0.0, run)];
    for (gap_low, gap_high) in front_gaps {
        let mut next: Vec<(f32, f32)> = Vec::new();
        for (low, high) in spans {
            if gap_high <= low || gap_low >= high {
                next.push((low, high));
                continue;
            }
            if gap_low > low {
                next.push((low, gap_low.min(high)));
            }
            if gap_high < high {
                next.push((gap_high.max(low), high));
            }
        }
        spans = next;
    }
    for (low, high) in spans {
        if high - low <= 1.0e-4 {
            continue;
        }
        let front = [
            [low, thickness, base],
            [high, thickness, base],
            [high, thickness, top],
            [low, thickness, top],
        ];
        let front_uv = front.map(|[along, _, y]| tiled_uv(along, top - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            &frame,
            front,
            front_uv,
            frame.normal(0.0, 1.0),
            true,
            false,
        );
    }

    // The cap: each remaining convex piece as its own fan (a corner trim leaves
    // a triangle or a quad; a both-ends trim can leave two pieces).
    for piece in &cap_pieces {
        let mut iter = piece.iter();
        let Some(first) = iter.next().copied() else {
            continue;
        };
        let mut previous = first;
        for point in iter {
            if triangle_area_2d(first, previous, *point) > 1.0e-6 {
                let [fx, fz] = first;
                let [px, pz] = previous;
                let [qx, qz] = *point;
                let locals = [[fx, fz, top], [px, pz, top], [qx, qz, top], [qx, qz, top]];
                let uv = locals.map(|[along, across, _]| tiled_uv(along, across, tile));
                emit_local_face(
                    context,
                    buckets,
                    scratch,
                    key,
                    PatchKind::Wall,
                    &frame,
                    locals,
                    uv,
                    [0.0, 1.0, 0.0],
                    false,
                    true,
                );
            }
            previous = *point;
        }
    }

    // The ends, unless the joint's other run already closes them.
    for (at, sign, trimmed) in [(0.0f32, -1.0f32, trim_start), (run, 1.0f32, trim_end)] {
        if trimmed {
            continue;
        }
        let locals = [
            [at, 0.0, base],
            [at, thickness, base],
            [at, thickness, top],
            [at, 0.0, top],
        ];
        let uv = locals.map(|[_, across, y]| tiled_uv(across, top - y, tile));
        emit_local_face(
            context,
            buckets,
            scratch,
            key,
            PatchKind::Wall,
            &frame,
            locals,
            uv,
            frame.normal(sign, 0.0),
            true,
            false,
        );
    }
}

/// The area of a convex local-space polygon.
fn polygon_area(polygon: &[[f32; 2]]) -> f32 {
    let mut area = 0.0f32;
    let mut previous = polygon.last().copied();
    for [x, y] in polygon.iter().copied() {
        if let Some([px, py]) = previous {
            area += px.mul_add(y, -(x * py));
        }
        previous = Some([x, y]);
    }
    (area * 0.5).abs()
}

/// The signed area of a local-space triangle.
fn triangle_area_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    let [ax, ay] = a;
    let [bx, by] = b;
    let [cx, cy] = c;
    ((cx - ax).mul_add(-(by - ay), (bx - ax) * (cy - ay))).abs() * 0.5
}

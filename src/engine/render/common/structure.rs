//! Structural backing follows the same convex patches as rigid collision.
//! Existing architecture owns the walking surfaces and their tall transitions;
//! this adds the missing underside and the lowest structural edge band only.
use super::{
    MaterialSlot, SurfaceKey, Vertex,
    architecture::{ArchitectureFace, emit_face},
    geometry::EmitContext,
    subtract_rectangles, tiled_uv,
};
use crate::engine::{
    level::{MIN_STRUCTURE_M, WalkableFloor},
    lighting::lightmap::PatchKind,
    spatial::SpatialBuckets,
};
use glam::Vec3;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Edge {
    start: f32,
    end: f32,
    bottom: f32,
    positive: bool,
    key: SurfaceKey,
}

pub(super) fn emit(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    let patches = WalkableFloor::from_level(context.level).structural_patches();
    let mut groups: BTreeMap<(bool, u32), Vec<Edge>> = BTreeMap::new();
    for patch in &patches {
        let [a, b, c, d] = patch.corners;
        let centre_x = f32::midpoint(a.x, c.x);
        let centre_z = f32::midpoint(a.z, c.z);
        let reference = context
            .level
            .ramps
            .iter()
            .rev()
            .find(|r| r.contains(centre_x, centre_z))
            .and_then(crate::engine::level::RampDef::edge_ref)
            .or_else(|| {
                context
                    .level
                    .stairs
                    .iter()
                    .rev()
                    .find(|s| s.contains(centre_x, centre_z))
                    .and_then(crate::engine::level::StairDef::side_ref)
            })
            .or_else(|| {
                context
                    .level
                    .floor_regions
                    .iter()
                    .rev()
                    .find(|r| r.contains(centre_x, centre_z))
                    .and_then(crate::engine::level::FloorRegionDef::edge_ref)
            })
            .unwrap_or_else(|| context.level.defaults.wall_ref());
        let key = context.materials.key(MaterialSlot::Wall, reference);
        let covered: Vec<_> = context
            .level
            .room_iter()
            .filter(|room| room.ceiling.is_flat() && (room.eave_y() - patch.backing).abs() < 0.0001)
            .map(crate::engine::level::RoomDef::bounds)
            .collect();
        for (x0, x1, z0, z1) in subtract_rectangles((a.x, c.x, a.z, c.z), &covered) {
            face(
                context,
                buckets,
                scratch,
                [
                    [x0, patch.backing, z0],
                    [x1, patch.backing, z0],
                    [x1, patch.backing, z1],
                    [x0, patch.backing, z1],
                ],
                [0.0, -1.0, 0.0],
                key,
            );
        }
        for (axis, line, start, end, positive) in [
            (false, a.x, a.z, d.z, false),
            (false, b.x, b.z, c.z, true),
            (true, a.z, a.x, b.x, false),
            (true, d.z, d.x, c.x, true),
        ] {
            let coordinate = if line.abs() < f32::EPSILON { 0.0 } else { line };
            groups
                .entry((axis, coordinate.to_bits()))
                .or_default()
                .push(Edge {
                    start,
                    end,
                    bottom: patch.backing,
                    positive,
                    key,
                });
        }
    }
    emit_edges(context, buckets, scratch, groups);
}
fn emit_edges(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    groups: BTreeMap<(bool, u32), Vec<Edge>>,
) {
    for ((axis, line), edges) in groups {
        let at = f32::from_bits(line);
        for edge in &edges {
            let mut cuts = vec![edge.start, edge.end];
            for other in &edges {
                if other.positive != edge.positive {
                    for value in [other.start, other.end] {
                        if value > edge.start && value < edge.end {
                            cuts.push(value);
                        }
                    }
                }
            }
            cuts.sort_by(f32::total_cmp);
            cuts.dedup_by(|a, b| a.total_cmp(b).is_eq());
            for span in cuts.windows(2) {
                let &[start, end] = span else {
                    continue;
                };
                let mid = f32::midpoint(start, end);
                let covered = edges.iter().any(|other| {
                    other.positive != edge.positive
                        && other.start <= mid
                        && other.end >= mid
                        && (other.bottom - edge.bottom).abs() < 0.0001
                });
                if covered {
                    continue;
                }
                emit_band(context, buckets, scratch, *edge, (axis, at), (start, end));
            }
        }
    }
}

fn emit_band(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    edge: Edge,
    (axis, at): (bool, f32),
    (start, end): (f32, f32),
) {
    let low = edge.bottom;
    let high = low + MIN_STRUCTURE_M;
    let mut covered = Vec::new();
    for wall in context.coverages {
        for &(along_start, along_end, bottom, top) in &wall.solids {
            let (x0, x1, z0, z1) = match wall.axis {
                crate::engine::level::WallAxis::X => {
                    (along_start, along_end, wall.thickness.0, wall.thickness.1)
                }
                crate::engine::level::WallAxis::Z => {
                    (wall.thickness.0, wall.thickness.1, along_start, along_end)
                }
            };
            let (plane_low, plane_high, run_low, run_high) = if axis {
                (z0, z1, x0, x1)
            } else {
                (x0, x1, z0, z1)
            };
            if at >= plane_low - 0.001 && at <= plane_high + 0.001 {
                covered.push((run_low, run_high, bottom, top));
            }
        }
    }
    for (left, right, bottom, top) in subtract_rectangles((start, end, low, high), &covered) {
        let point = |run, y| if axis { [run, y, at] } else { [at, y, run] };
        let sign = if edge.positive { 1.0 } else { -1.0 };
        face(
            context,
            buckets,
            scratch,
            [
                point(left, bottom),
                point(right, bottom),
                point(right, top),
                point(left, top),
            ],
            if axis {
                [0.0, 0.0, sign]
            } else {
                [sign, 0.0, 0.0]
            },
            edge.key,
        );
    }
}

fn face(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    mut points: [[f32; 3]; 4],
    normal: [f32; 3],
    key: SurfaceKey,
) {
    let [a, b, c, _] = points.map(Vec3::from_array);
    if Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z)
        .cross(Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z))
        .dot(Vec3::from_array(normal))
        < 0.0
    {
        points.swap(1, 3);
    }
    let tile = context.materials.tile_metres(key);
    let uv = points.map(|p| {
        if normal[1].abs() > 0.5 {
            tiled_uv(p[0], p[2], tile)
        } else if normal[0].abs() > 0.5 {
            tiled_uv(p[2], p[1], tile)
        } else {
            tiled_uv(p[0], p[1], tile)
        }
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
            vertical: normal[1].abs() < 0.5,
            up: false,
            key,
            kind: PatchKind::Skirt,
        },
    );
}

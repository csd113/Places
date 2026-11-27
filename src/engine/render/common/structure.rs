//! Structural backing follows the same convex patches as rigid collision.
//! Existing architecture owns the walking surfaces and their tall transitions;
//! this adds the missing underside and the lowest structural edge band only.
use super::{
    MaterialSlot, SurfaceKey, Vertex,
    architecture::{ArchitectureFace, emit_face},
    geometry::EmitContext,
    subtract_rectangles, subtract_rectangles_with_tolerance, tiled_uv,
};
use crate::engine::{
    level::{MIN_STRUCTURE_M, WalkableFloor},
    lighting::lightmap::PatchKind,
    spatial::SpatialBuckets,
};
use glam::Vec3;
use std::collections::BTreeMap;

// Keep true slab reveals down to floating-point noise, independently of the
// broader tolerance used to hide nearly coincident architectural walls.
const SLAB_EDGE_EPS: f32 = 0.000_001;
type Band = (f32, f32, f32, f32);

struct BandClips<'a> {
    opposing: Vec<Band>,
    emitted_same_side: &'a mut Vec<Band>,
}

#[derive(Clone, Copy)]
struct Edge {
    start: f32,
    end: f32,
    bottom: f32,
    flat_slab: bool,
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
        let flat_slab = patch.belt.is_none()
            && patch
                .corners
                .iter()
                .all(|point| (point.y - patch.room_floor_y).abs() < 0.0001);
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
                    flat_slab,
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
        let mut emitted = [Vec::new(), Vec::new()];
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
                let high = edge.bottom + MIN_STRUCTURE_M;
                let mut covered = Vec::new();
                let fully_covered = edges
                    .iter()
                    .filter(|other| {
                        other.positive != edge.positive && other.start <= mid && other.end >= mid
                    })
                    .any(|other| {
                        if !(edge.flat_slab && other.flat_slab) {
                            // Ordinary terrain skirts still own the upper
                            // transition between a base and a modified floor.
                            return other.bottom <= edge.bottom + 0.0001;
                        }
                        if (other.bottom - edge.bottom).abs() <= SLAB_EDGE_EPS {
                            return true;
                        }
                        // An opposite edge on another storey is not an
                        // occluder unless its slab actually intersects ours.
                        let overlap_bottom = edge.bottom.max(other.bottom);
                        let overlap_top = high.min(other.bottom + MIN_STRUCTURE_M);
                        if overlap_top > overlap_bottom {
                            covered.push((start, end, overlap_bottom, overlap_top));
                        }
                        false
                    });
                if !fully_covered {
                    let [negative, positive] = &mut emitted;
                    let same_side = if edge.positive { positive } else { negative };
                    emit_band(
                        context,
                        buckets,
                        scratch,
                        *edge,
                        (axis, at),
                        (start, end),
                        BandClips {
                            opposing: covered,
                            emitted_same_side: same_side,
                        },
                    );
                }
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
    mut clips: BandClips<'_>,
) {
    let low = edge.bottom;
    let high = low + MIN_STRUCTURE_M;
    let mut wall_covered = Vec::new();
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
                wall_covered.push((run_low, run_high, bottom, top));
            }
        }
    }
    // Apply the historical wall tolerance before making any thin slab cuts.
    let wall_spans = subtract_rectangles((start, end, low, high), &wall_covered);
    if edge.flat_slab {
        // The first visible flat band owns only the area it actually emitted,
        // so later slabs retain their material everywhere else.
        clips
            .opposing
            .extend(clips.emitted_same_side.iter().copied().filter(
                |&(left, right, bottom, top)| {
                    left < end && right > start && bottom < high && top > low
                },
            ));
    }
    let spans = if clips.opposing.is_empty() {
        wall_spans
    } else {
        wall_spans
            .into_iter()
            .flat_map(|span| {
                subtract_rectangles_with_tolerance(span, &clips.opposing, SLAB_EDGE_EPS)
            })
            .collect()
    };
    for (left, right, bottom, top) in spans {
        if edge.flat_slab {
            clips.emitted_same_side.push((left, right, bottom, top));
        }
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

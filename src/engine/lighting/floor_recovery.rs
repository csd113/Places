//! Conservative recovery cap for verified open joins between real floor charts.
//!
//! Only the added recovery target uses this field. Physical transport, moment,
//! authored room identity, and probe recovery retain their original inputs.

use std::collections::BTreeMap;

use super::bake::{LevelLighting, RoomLighting};
use super::lightmap::{Chart, LightmapPatch, PatchKind};
use crate::engine::spatial::Aabb;

/// Physical width within which chart ownership cannot establish an isolated
/// recovery baseline. Away from a certified join use the usual room target.
const REACH_M: f32 = 0.75;
const AIR_HEIGHT_M: f32 = 1.6;
// Covers the rounding of the clipped quad's fourth corner. A visible floor
// gap or step cannot certify a join.
const CHART_CORNER_EPS_M: f32 = 0.000_02;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum Axis {
    X,
    Z,
}

impl Axis {
    const ALL: [Self; 2] = [Self::X, Self::Z];

    const fn separate(self, [x, _, z]: [f32; 3]) -> (f32, f32) {
        match self {
            Self::X => (x, z),
            Self::Z => (z, x),
        }
    }

    const fn point(self, across: f32, y: f32, along: f32) -> [f32; 3] {
        match self {
            Self::X => [across, y, along],
            Self::Z => [along, y, across],
        }
    }

    const fn room_span(self, room: &RoomLighting) -> [f32; 2] {
        match self {
            Self::X => [room.x0, room.x1],
            Self::Z => [room.z0, room.z1],
        }
    }
}

#[derive(Clone, Copy)]
struct Edge {
    room: usize,
    span: [f32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Line {
    axis: Axis,
    across: u32,
    floor: u32,
}

impl Line {
    const fn across(self) -> f32 {
        f32::from_bits(self.across)
    }

    const fn floor(self) -> f32 {
        f32::from_bits(self.floor)
    }
}

struct Join {
    line: Line,
    along: [f32; 2],
    other: usize,
}

/// The relevant open spans for each room, constructed only for floor recovery.
pub(super) struct FloorRecovery {
    per_room: BTreeMap<usize, Vec<Join>>,
}

impl FloorRecovery {
    pub(super) fn new(lighting: &LevelLighting, charts: &[(LightmapPatch, Chart)]) -> Self {
        let mut edges: BTreeMap<Line, [Vec<Edge>; 2]> = BTreeMap::new();
        for (patch, _) in charts {
            let Some(bounds) = floor_bounds(patch) else {
                continue;
            };
            let Some(room) = patch.room else {
                continue;
            };
            let Some(volume) = lighting.rooms().get(room) else {
                continue;
            };
            let floor = patch.origin[1] + 0.0;
            if floor.to_bits() != (volume.floor_y + 0.0).to_bits() {
                continue;
            }
            for axis in Axis::ALL {
                let (across_min, along_min) = axis.separate(bounds.min);
                let (across_max, along_max) = axis.separate(bounds.max);
                if across_max - across_min < REACH_M - CHART_CORNER_EPS_M {
                    continue;
                }
                let authored = axis.room_span(volume);
                for (end, (actual, room_edge)) in [across_min, across_max]
                    .into_iter()
                    .zip(authored)
                    .enumerate()
                {
                    let across = (room_edge + 0.0).to_bits();
                    if (actual - room_edge).abs() <= CHART_CORNER_EPS_M
                        && let Some(side) = edges
                            .entry(Line {
                                axis,
                                across,
                                floor: floor.to_bits(),
                            })
                            .or_default()
                            .get_mut(end)
                    {
                        side.push(Edge {
                            room,
                            span: [along_min, along_max],
                        });
                    }
                }
            }
        }

        let mut per_room = BTreeMap::<usize, Vec<Join>>::new();
        for (line, [lower_edges, upper_edges]) in edges {
            let (Some(negative), Some(positive)) = (coalesce(upper_edges), coalesce(lower_edges))
            else {
                continue;
            };
            let mut neg = negative.iter().peekable();
            let mut pos = positive.iter().peekable();
            while let (Some(&left), Some(&right)) = (neg.peek(), pos.peek()) {
                let along = [
                    left.span[0].max(right.span[0]),
                    left.span[1].min(right.span[1]),
                ];
                if left.room != right.room && along[0] < along[1] {
                    for span in uncovered(lighting, line, along, left.room, right.room) {
                        for (owner, other) in [(left.room, right.room), (right.room, left.room)] {
                            per_room.entry(owner).or_default().push(Join {
                                line,
                                along: span,
                                other,
                            });
                        }
                    }
                }
                if left.span[1] <= right.span[1] {
                    let _advanced = neg.next();
                } else {
                    let _advanced = pos.next();
                }
            }
        }
        Self { per_room }
    }

    /// Prepare only nearby spans once per chart, rather than scanning the level
    /// during every recovery query in the texel loop.
    pub(super) fn for_patch(&self, patch: &LightmapPatch) -> PatchRecovery<'_> {
        let Some(bounds) = floor_bounds(patch) else {
            return PatchRecovery(Vec::new());
        };
        let Some(joins) = patch.room.and_then(|room| self.per_room.get(&room)) else {
            return PatchRecovery(Vec::new());
        };
        PatchRecovery(
            joins
                .iter()
                .filter(|join| {
                    if join.line.floor != (patch.origin[1] + 0.0).to_bits() {
                        return false;
                    }
                    let (lo, start) = join.line.axis.separate(bounds.min);
                    let (hi, end) = join.line.axis.separate(bounds.max);
                    let edge = join.line.across();
                    lo < edge + REACH_M
                        && hi > edge - REACH_M
                        && start < join.along[1]
                        && end > join.along[0]
                })
                .collect(),
        )
    }
}

pub(super) struct PatchRecovery<'a>(Vec<&'a Join>);

impl PatchRecovery<'_> {
    pub(super) fn cap(
        &self,
        point: [f32; 3],
        query_target: impl Fn(usize, f32, f32) -> [f32; 3],
        own: [f32; 3],
    ) -> [f32; 3] {
        let mut result = own;
        for join in &self.0 {
            let (across, along) = join.line.axis.separate(point);
            let edge = join.line.across();
            let inset = across - edge;
            let normal = smooth_unit(1.0 - inset.abs() / REACH_M);
            let tangent = smooth_unit((along - join.along[0]).min(join.along[1] - along) / REACH_M);
            let weight = normal * tangent;
            if weight > 0.0 {
                let [x, _, z] = join.line.axis.point(edge - inset, point[1], along);
                let other = query_target(join.other, x, z);
                for ((output, original), opposite) in result.iter_mut().zip(own).zip(other) {
                    let reduction = (original - opposite).max(0.0);
                    *output = output.min((-reduction).mul_add(weight, original));
                }
            }
        }
        result
    }
}

fn smooth_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * 2.0_f32.mul_add(-t, 3.0)
}

fn floor_bounds(patch: &LightmapPatch) -> Option<Aabb> {
    let [ux, uy, uz] = patch.u_axis;
    let [vx, vy, vz] = patch.v_axis;
    // Only regular horizontal rectangles with their normal in +Y can
    // certify two sides of an open floor. Skip folds, ramps and uncertainty.
    let minor = |v: f32| v.abs() <= CHART_CORNER_EPS_M;
    let orthogonal =
        (ux.abs() > CHART_CORNER_EPS_M && vz.abs() > CHART_CORNER_EPS_M && minor(uz) && minor(vx))
            || (uz.abs() > CHART_CORNER_EPS_M
                && vx.abs() > CHART_CORNER_EPS_M
                && minor(ux)
                && minor(vz));
    if patch.kind != PatchKind::Floor
        || patch.is_triangular()
        || uy.abs() > 0.0
        || vy.abs() > 0.0
        || !orthogonal
        || uz.mul_add(vx, -ux * vz) <= 0.0
        || patch.diagonal_correction[1].abs() > 0.0
        || !patch.diagonal_correction.into_iter().all(minor)
    {
        return None;
    }
    let mut bounds = Aabb {
        min: patch.origin,
        max: patch.origin,
    };
    for (u, v) in [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        for ((low, high), coordinate) in bounds
            .min
            .iter_mut()
            .zip(&mut bounds.max)
            .zip(patch.point_at(u, v))
        {
            *low = low.min(coordinate);
            *high = high.max(coordinate);
        }
    }
    bounds
        .min
        .iter()
        .chain(&bounds.max)
        .all(|v| v.is_finite())
        .then_some(bounds)
}

/// Coalesce regular chart subdivision. A line with overlapping owners on one
/// side is ambiguous and cannot provide a physical join.
fn coalesce(mut edges: Vec<Edge>) -> Option<Vec<Edge>> {
    edges.sort_unstable_by(|a, b| a.span[0].total_cmp(&b.span[0]));
    let mut merged = Vec::<Edge>::new();
    for edge in edges {
        if let Some(last) = merged.last_mut() {
            if last.room == edge.room && last.span[1] + CHART_CORNER_EPS_M >= edge.span[0] {
                last.span[1] = last.span[1].max(edge.span[1]);
                continue;
            }
            if last.span[1] > edge.span[0] {
                return None;
            }
        }
        merged.push(edge);
    }
    Some(merged)
}

/// Cut the projected join at every opaque wall/prop overlapping its whole
/// influence strip. Even a partially closed opening retains ordinary recovery
/// there. No texel-local visibility switch or transitive room merge is used.
fn uncovered(
    lighting: &LevelLighting,
    line: Line,
    span: [f32; 2],
    left: usize,
    right: usize,
) -> Vec<[f32; 2]> {
    let edge = line.across();
    let floor = line.floor();
    let height = match (lighting.rooms().get(left), lighting.rooms().get(right)) {
        (Some(l), Some(r)) => l.height_m.min(r.height_m).min(AIR_HEIGHT_M),
        _ => return Vec::new(),
    };
    let bounds = Aabb {
        min: line.axis.point(edge - REACH_M, floor, span[0]),
        max: line.axis.point(edge + REACH_M, floor + height, span[1]),
    };
    let mut blocked = Vec::new();
    let mut block = |start: f32, end: f32| {
        let lo = start.max(span[0]);
        let hi = end.min(span[1]);
        if lo < hi {
            blocked.push([lo, hi]);
        }
    };
    // A third coplanar owner can cover this strip without having an edge
    // along the join. Use its authored footprint as a conservative bound.
    for (index, room) in lighting.rooms().iter().enumerate() {
        if index != left && index != right && (room.floor_y + 0.0).to_bits() == line.floor {
            let (lo, start) = line.axis.separate([room.x0, floor, room.z0]);
            let (hi, end) = line.axis.separate([room.x1, floor, room.z1]);
            if lo < edge + REACH_M && hi > edge - REACH_M {
                block(start, end);
            }
        }
    }
    lighting
        .recovery_occluders()
        .for_each_recovery_blocker(bounds, |solid| {
            let (_, start) = line.axis.separate(solid.min);
            let (_, end) = line.axis.separate(solid.max);
            block(start, end);
        });
    blocked.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]));
    let mut open = Vec::new();
    let mut start = span[0];
    for [lo, hi] in blocked {
        if lo > start {
            open.push([start, lo]);
        }
        start = start.max(hi);
    }
    if span[1] > start {
        open.push([start, span[1]]);
    }
    open
}

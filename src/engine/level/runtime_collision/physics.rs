//! Physical floor and ceiling patches derived from the controller's owned data.

use glam::Vec3;
use std::collections::BTreeMap;

use super::{CeilingProfileDef, WalkableCeiling, WalkableFloor, WallAxis};

/// One convex, vertical prism. Its upper or lower surface may be sloped.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalPatch {
    pub corners: [Vec3; 4],
    pub backing: f32,
    pub ceiling: bool,
    pub room_floor_y: f32,
}

#[derive(Clone, Copy)]
struct Edge {
    low: f32,
    high: f32,
    low_y: f32,
    high_y: f32,
    side: i8,
}

impl Edge {
    fn height(self, at: f32) -> f32 {
        (self.high_y - self.low_y).mul_add((at - self.low) / (self.high - self.low), self.low_y)
    }
}

const fn coordinate_key(coordinate: f32) -> u32 {
    if coordinate.abs() < f32::EPSILON {
        0
    } else {
        coordinate.to_bits()
    }
}

fn add_step_face(
    axis: WallAxis,
    line: f32,
    span: [f32; 2],
    first: [f32; 2],
    second: [f32; 2],
    output: &mut Vec<[Vec3; 4]>,
) {
    let [low, high] = span;
    let [a0, a1] = first;
    let [b0, b1] = second;
    if (a0 - b0).abs().max((a1 - b1).abs()) < 1.0e-4 {
        return;
    }
    let point = |along: f32, y: f32| match axis {
        WallAxis::X => Vec3::new(line, y, along),
        WallAxis::Z => Vec3::new(along, y, line),
    };
    output.push([
        point(low, a0.max(b0)),
        point(high, a1.max(b1)),
        point(high, a1.min(b1)),
        point(low, a0.min(b0)),
    ]);
}

/// Only shared, opposite-facing patch edges make internal step sides. Room
/// exteriors and overlapping storeys never fabricate a thick wall below a floor.
fn close_steps(patches: &[PhysicalPatch], output: &mut Vec<[Vec3; 4]>) {
    let mut groups: BTreeMap<(u8, u32), Vec<Edge>> = BTreeMap::new();
    for patch in patches {
        let [a, b, c, d] = patch.corners;
        for (axis, line, edge) in [
            (
                0_u8,
                a.x,
                Edge {
                    low: a.z,
                    high: d.z,
                    low_y: a.y,
                    high_y: d.y,
                    side: -1,
                },
            ),
            (
                0_u8,
                b.x,
                Edge {
                    low: b.z,
                    high: c.z,
                    low_y: b.y,
                    high_y: c.y,
                    side: 1,
                },
            ),
            (
                1_u8,
                a.z,
                Edge {
                    low: a.x,
                    high: b.x,
                    low_y: a.y,
                    high_y: b.y,
                    side: -1,
                },
            ),
            (
                1_u8,
                d.z,
                Edge {
                    low: d.x,
                    high: c.x,
                    low_y: d.y,
                    high_y: c.y,
                    side: 1,
                },
            ),
        ] {
            groups
                .entry((axis, coordinate_key(line)))
                .or_default()
                .push(edge);
        }
    }
    for ((axis_tag, line_key), edges) in groups {
        let axis = if axis_tag == 0 {
            WallAxis::X
        } else {
            WallAxis::Z
        };
        let line = f32::from_bits(line_key);
        for first in edges.iter().filter(|edge| edge.side < 0) {
            for second in edges.iter().filter(|edge| edge.side > 0) {
                let low = first.low.max(second.low);
                let high = first.high.min(second.high);
                if high <= low {
                    continue;
                }
                let a = [first.height(low), first.height(high)];
                let b = [second.height(low), second.height(high)];
                let [a0, a1] = a;
                let [b0, b1] = b;
                let delta0 = a0 - b0;
                let delta1 = a1 - b1;
                if delta0 * delta1 < 0.0 {
                    let crossing = (delta0 / (delta0 - delta1)).mul_add(high - low, low);
                    let height = first.height(crossing);
                    add_step_face(
                        axis,
                        line,
                        [low, crossing],
                        [a0, height],
                        [b0, height],
                        output,
                    );
                    add_step_face(
                        axis,
                        line,
                        [crossing, high],
                        [height, a1],
                        [height, b1],
                        output,
                    );
                } else {
                    add_step_face(axis, line, [low, high], a, b, output);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
}

impl Rect {
    const fn new(bounds: (f32, f32, f32, f32)) -> Self {
        Self {
            x0: bounds.0,
            x1: bounds.1,
            z0: bounds.2,
            z1: bounds.3,
        }
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let clipped = Self {
            x0: self.x0.max(other.x0),
            x1: self.x1.min(other.x1),
            z0: self.z0.max(other.z0),
            z1: self.z1.min(other.z1),
        };
        (clipped.x1 > clipped.x0 && clipped.z1 > clipped.z0).then_some(clipped)
    }

    fn subtract(self, cut: Self, output: &mut Vec<Self>) {
        let Some(overlap) = self.intersection(cut) else {
            output.push(self);
            return;
        };
        for candidate in [
            Self {
                x1: overlap.x0,
                ..self
            },
            Self {
                x0: overlap.x1,
                ..self
            },
            Self {
                x0: overlap.x0,
                x1: overlap.x1,
                z1: overlap.z0,
                ..self
            },
            Self {
                x0: overlap.x0,
                x1: overlap.x1,
                z0: overlap.z1,
                ..self
            },
        ] {
            if candidate.x1 > candidate.x0 && candidate.z1 > candidate.z0 {
                output.push(candidate);
            }
        }
    }

    fn corners(self, height: impl Fn(f32, f32) -> f32) -> [Vec3; 4] {
        [
            Vec3::new(self.x0, height(self.x0, self.z0), self.z0),
            Vec3::new(self.x1, height(self.x1, self.z0), self.z0),
            Vec3::new(self.x1, height(self.x1, self.z1), self.z1),
            Vec3::new(self.x0, height(self.x0, self.z1), self.z1),
        ]
    }
}

/// Cut only at authored physical boundaries, never at baked lighting cells.
fn consume_patch(
    remaining: &mut Vec<Rect>,
    bounds: Rect,
    height: impl Fn(f32, f32) -> f32,
    backing: f32,
    output: &mut Vec<PhysicalPatch>,
) {
    let mut next = Vec::with_capacity(remaining.len());
    for old in remaining.drain(..) {
        if let Some(visible) = old.intersection(bounds) {
            let corners = visible.corners(&height);
            let bottom = corners.iter().fold(backing, |low, point| low.min(point.y));
            output.push(PhysicalPatch {
                corners,
                backing: bottom - super::MIN_STRUCTURE_M,
                ceiling: false,
                room_floor_y: backing,
            });
        }
        old.subtract(bounds, &mut next);
    }
    *remaining = next;
}

impl WalkableFloor {
    /// Exact visible floors, with last-authored overlay precedence and real treads.
    pub(crate) fn physical_geometry(&self) -> (Vec<PhysicalPatch>, Vec<[Vec3; 4]>) {
        self.physical_geometry_inner(true)
    }

    pub(crate) fn structural_patches(&self) -> Vec<PhysicalPatch> {
        self.physical_geometry_inner(false).0
    }

    fn physical_geometry_inner(
        &self,
        barriers_needed: bool,
    ) -> (Vec<PhysicalPatch>, Vec<[Vec3; 4]>) {
        let mut output = Vec::new();
        let mut barriers = Vec::new();
        let mut prior_rooms: Vec<(Rect, f32)> = Vec::new();
        for room in &self.rooms {
            let patch_start = output.len();
            let bounds = Rect::new((room.x0, room.x1, room.z0, room.z1));
            let mut remaining = vec![bounds];
            // First-room ownership removes coplanar overlaps. Different
            // storeys retain their own physical landing surfaces.
            for &(prior, floor_y) in &prior_rooms {
                if (floor_y - room.floor_y).abs() > 1.0e-4 {
                    continue;
                }
                let mut clipped = Vec::new();
                for fragment in remaining {
                    fragment.subtract(prior, &mut clipped);
                }
                remaining = clipped;
            }
            prior_rooms.push((bounds, room.floor_y));
            // This is the same precedence as room_height_at: ramps, stairs,
            // then regions, with the latest authored entry in each class winning.
            for ramp in room.ramps.iter().rev() {
                consume_patch(
                    &mut remaining,
                    Rect::new(ramp.surface.bounds()),
                    |x, z| ramp.height_at(x, z),
                    room.floor_y,
                    &mut output,
                );
            }
            for stair in room.stairs.iter().rev() {
                let (x0, x1, z0, z1) = stair.surface.bounds();
                for tread in 0..stair.surface.step_count() {
                    let (start, end) = stair.surface.tread_span(tread);
                    let tread_bounds = match stair.surface.axis() {
                        WallAxis::X => (start, end, z0, z1),
                        WallAxis::Z => (x0, x1, start, end),
                    };
                    consume_patch(
                        &mut remaining,
                        Rect::new(tread_bounds),
                        |_, _| stair.tread_height(tread),
                        room.floor_y,
                        &mut output,
                    );
                }
            }
            for region in room.regions.iter().rev() {
                consume_patch(
                    &mut remaining,
                    Rect::new((region.x0, region.x1, region.z0, region.z1)),
                    |_, _| region.y,
                    room.floor_y,
                    &mut output,
                );
            }
            consume_patch(
                &mut remaining,
                bounds,
                |_, _| room.floor_y,
                room.floor_y,
                &mut output,
            );
            if barriers_needed && let Some(room_patches) = output.get(patch_start..) {
                close_steps(room_patches, &mut barriers);
            }
        }
        (output, barriers)
    }
}

/// A coplanar floor already owns the shared structural plane. Keeping the
/// ceiling's upward backing there fills the first 12 cm of the upper storey.
/// Clip only actual flat floor coverage; uncovered roofs retain their slab.
fn unshared_ceiling(bounds: Rect, height: f32, floors: &[PhysicalPatch]) -> Vec<Rect> {
    let mut remaining = vec![bounds];
    for floor in floors {
        if floor.corners.iter().any(|corner| {
            (corner.y - height).abs() > 1.0e-4
                && ((floor.backing - height).abs() > 1.0e-4 || corner.y < height)
        }) {
            continue;
        }
        let [a, _, c, _] = floor.corners;
        let cut = Rect::new((a.x, c.x, a.z, c.z));
        let mut next = Vec::new();
        for fragment in remaining {
            fragment.subtract(cut, &mut next);
        }
        remaining = next;
        if remaining.is_empty() {
            break;
        }
    }
    remaining
}

impl WalkableCeiling {
    /// Flat or pitched roof slabs, split once at a gable's ridge.
    pub(crate) fn physical_patches(&self, floors: &[PhysicalPatch]) -> Vec<PhysicalPatch> {
        let mut output = Vec::new();
        for room in &self.rooms {
            let bounds = Rect::new(room.bounds);
            let patches = match room.profile {
                CeilingProfileDef::Gable {
                    ridge: WallAxis::X,
                    ridge_rise: _,
                } => {
                    let ridge = f32::midpoint(bounds.z0, bounds.z1);
                    [
                        Rect {
                            z1: ridge,
                            ..bounds
                        },
                        Rect {
                            z0: ridge,
                            ..bounds
                        },
                    ]
                    .to_vec()
                }
                CeilingProfileDef::Gable {
                    ridge: WallAxis::Z,
                    ridge_rise: _,
                } => {
                    let ridge = f32::midpoint(bounds.x0, bounds.x1);
                    [
                        Rect {
                            x1: ridge,
                            ..bounds
                        },
                        Rect {
                            x0: ridge,
                            ..bounds
                        },
                    ]
                    .to_vec()
                }
                CeilingProfileDef::Flat => {
                    unshared_ceiling(bounds, room.ceiling_y_at(bounds.x0, bounds.z0), floors)
                }
                CeilingProfileDef::Open => Vec::new(),
            };
            for patch in patches {
                let corners = patch.corners(|x, z| room.ceiling_y_at(x, z));
                let backing = corners
                    .iter()
                    .fold(f32::NEG_INFINITY, |high, point| high.max(point.y))
                    + super::MIN_STRUCTURE_M;
                output.push(PhysicalPatch {
                    corners,
                    backing,
                    ceiling: true,
                    room_floor_y: room.floor_y,
                });
            }
        }
        output
    }
}

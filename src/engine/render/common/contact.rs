//! Exact convex contact subtraction before lightmap chart creation.
//! The original positions and UVs survive outside the intersecting solid.
use super::{LightmapEmit, Vertex, quad_normal, stamp_lightmap_quad};
use crate::engine::{
    level::{LevelDef, LevelSurfaces, WallAxis, round_point},
    lighting::lightmap::PatchKind,
    spatial::Aabb,
};
use glam::Vec3;

#[derive(Clone, Copy)]
struct Plane {
    normal: Vec3,
    offset: f32,
}
impl Plane {
    fn distance(self, point: [f32; 3]) -> f32 {
        self.normal.dot(Vec3::from_array(point)) - self.offset
    }
}
struct Solid {
    trim_owner: Option<usize>,
    covers_trim: bool,
    bounds: Aabb,
    planes: Vec<Plane>,
}
enum Node {
    Leaf {
        bounds: Aabb,
        solids: Vec<Solid>,
    },
    Branch {
        bounds: Aabb,
        left: Box<Self>,
        right: Box<Self>,
    },
}
impl Node {
    fn build(mut solids: Vec<Solid>) -> Self {
        let bounds = solids.iter().fold(Aabb::EMPTY, |a, b| a.union(&b.bounds));
        if solids.len() <= 8 {
            return Self::Leaf { bounds, solids };
        }
        let axis = if bounds.max[0] - bounds.min[0] >= bounds.max[2] - bounds.min[2] {
            0
        } else {
            2
        };
        solids.sort_by(|a, b| {
            a.bounds
                .centre()
                .get(axis)
                .unwrap_or(&0.0)
                .total_cmp(b.bounds.centre().get(axis).unwrap_or(&0.0))
        });
        let right = solids.split_off(solids.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(solids)),
            right: Box::new(Self::build(right)),
        }
    }
    const fn bounds(&self) -> Aabb {
        match self {
            Self::Leaf { bounds, solids: _ }
            | Self::Branch {
                bounds,
                left: _,
                right: _,
            } => *bounds,
        }
    }
    fn query<'a>(&'a self, bounds: Aabb, out: &mut Vec<&'a Solid>) {
        if !overlaps(self.bounds(), bounds) {
            return;
        }
        match self {
            Self::Leaf { solids, bounds: _ } => {
                out.extend(solids.iter().filter(|s| overlaps(s.bounds, bounds)));
            }
            Self::Branch {
                left,
                right,
                bounds: _,
            } => {
                left.query(bounds, out);
                right.query(bounds, out);
            }
        }
    }
}
fn overlaps(a: Aabb, b: Aabb) -> bool {
    a.min.iter().zip(a.max).zip(b.min.iter().zip(b.max)).all(
        |((low, high), (other_low, other_high))| {
            *low <= other_high + 0.00001 && high >= *other_low - 0.00001
        },
    )
}

/// Built once per mesh, with a bounded spatial tree instead of a scene scan per face.
pub struct Contacts {
    root: Node,
    fit_architecture: bool,
}
impl Contacts {
    pub(super) fn new(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Self {
        let mut solids = Vec::new();
        let fit_architecture = level.walls.iter().any(|p| p.trim.is_some())
            || level.columns.iter().any(|p| p.trim.is_some())
            || level.pillars.iter().any(|p| p.trim.is_some())
            || level.arc_walls.iter().any(|p| p.trim.is_some())
            || level.walls.iter().any(|w| {
                w.openings
                    .iter()
                    .any(|o| o.recess.is_some() || o.surround.is_some())
            })
            || level.rooms.iter().any(|r| !r.tunnels.is_empty());
        for piece in crate::engine::level::construction::pieces(level) {
            let [x0, y0, z0] = piece.bounds.min;
            let [x1, y1, z1] = piece.bounds.max;
            let mut volume = prism(&[(x0, z0), (x1, z0), (x1, z1), (x0, z1)], y0, y1);
            volume.covers_trim = true;
            solids.push(volume);
        }
        for wall in &level.walls {
            let (x, z) = wall.min_corner();
            for slice in surfaces.wall_solid_slices(wall) {
                let (x0, x1, z0, z1) = match wall.axis() {
                    WallAxis::X => (x + slice.start, x + slice.end, z, z + wall.depth.abs()),
                    WallAxis::Z => (x, x + wall.width.abs(), z + slice.start, z + slice.end),
                };
                // Profiled wall tops are not flat prisms. Existing wall coverage
                // still owns those; contact fitting is admitted on flat spans.
                if wall.height.is_some() || surfaces.wall_profile_breaks(wall).is_empty() {
                    solids.push(prism(
                        &[(x0, z0), (x1, z0), (x1, z1), (x0, z1)],
                        slice.bottom,
                        slice.top,
                    ));
                }
            }
        }
        for column in &level.columns {
            let (x0, x1, z0, z1) = column.bounds();
            solids.push(prism(
                &[(x0, z0), (x1, z0), (x1, z1), (x0, z1)],
                column.base_y(surfaces),
                column.top_y(surfaces),
            ));
        }
        for pillar in &level.pillars {
            solids.push(prism(
                &pillar.polygon_points(),
                pillar.base_y(surfaces),
                pillar.top_y(surfaces),
            ));
        }
        for arc in &level.arc_walls {
            let count = arc.resolved_segments();
            let divisor = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
            for step in 0..count {
                let fraction = f32::from(u16::try_from(step).unwrap_or(u16::MAX)) / divisor;
                let next =
                    f32::from(u16::try_from(step.saturating_add(1)).unwrap_or(u16::MAX)) / divisor;
                let a0 = arc.sweep_degrees.mul_add(fraction, arc.start_degrees);
                let a1 = arc.sweep_degrees.mul_add(next, arc.start_degrees);
                let points = [
                    round_point(arc.x, arc.z, arc.inner_radius(), a0),
                    round_point(arc.x, arc.z, arc.outer_radius(), a0),
                    round_point(arc.x, arc.z, arc.outer_radius(), a1),
                    round_point(arc.x, arc.z, arc.inner_radius(), a1),
                ];
                let top = points
                    .iter()
                    .map(|(x, z)| arc.top_y_at(surfaces, *x, *z))
                    .fold(f32::INFINITY, f32::min);
                solids.push(prism(&points, arc.base_y(surfaces), top));
            }
        }
        for (index, board) in level.baseboards.iter().enumerate() {
            let (sx, ex) = board.span_at(board.thickness());
            let (dx, dz) = board.direction();
            let (ax, az) = board.across();
            let point = |along: f32, across: f32| {
                (
                    dx.mul_add(along, ax.mul_add(across, board.x)),
                    dz.mul_add(along, az.mul_add(across, board.z)),
                )
            };
            let points = [
                (board.x, board.z),
                board.point_at(1.0, 0.0),
                point(ex, board.thickness()),
                point(sx, board.thickness()),
            ];
            let mut volume = prism(
                &points,
                board.base_y(surfaces),
                board.base_y(surfaces) + board.height(),
            );
            volume.trim_owner = Some(index);
            solids.push(volume);
        }
        Self {
            root: Node::build(solids),
            fit_architecture,
        }
    }

    fn clip(
        &self,
        polygon: Vec<Vertex>,
        normal: Vec3,
        owner: Option<usize>,
        carrier: bool,
    ) -> Vec<Vec<Vertex>> {
        if !carrier && !self.fit_architecture {
            return vec![polygon];
        }
        let bounds = polygon.iter().fold(Aabb::EMPTY, |a, v| a.expanded(v.pos));
        let mut candidates = Vec::new();
        self.root.query(bounds, &mut candidates);
        let mut pieces = vec![polygon];
        for solid in candidates {
            if owner.is_some() && solid.trim_owner == owner {
                continue;
            }
            let mut next = Vec::new();
            for piece in pieces {
                next.extend(
                    subtract(piece, solid, normal, owner)
                        .into_iter()
                        .map(clean_polygon)
                        .filter(|fragment| area(fragment) > 1.0e-9),
                );
            }
            pieces = next;
            if pieces.is_empty() {
                break;
            }
        }
        pieces
    }
}

fn prism(points: &[(f32, f32)], bottom: f32, top: f32) -> Solid {
    let mut bounds = Aabb::EMPTY;
    let divisor = f32::from(u16::try_from(points.len()).unwrap_or(1));
    let centre = points.iter().fold(Vec3::ZERO, |sum, (x, z)| {
        Vec3::new(sum.x + x / divisor, 0.0, sum.z + z / divisor)
    });
    let mut planes = vec![
        Plane {
            normal: Vec3::Y,
            offset: bottom,
        },
        Plane {
            normal: Vec3::NEG_Y,
            offset: -top,
        },
    ];
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        bounds.expand([a.0, bottom, a.1]);
        bounds.expand([a.0, top, a.1]);
        let origin = Vec3::new(a.0, 0.0, a.1);
        let mut normal = Vec3::new(a.1 - b.1, 0.0, b.0 - a.0).normalize_or_zero();
        if normal.dot(difference(centre, origin)) < 0.0 {
            normal = negate(normal);
        }
        planes.push(Plane {
            normal,
            offset: normal.dot(origin),
        });
    }
    Solid {
        bounds,
        planes,
        trim_owner: None,
        covers_trim: false,
    }
}

fn subtract(
    polygon: Vec<Vertex>,
    solid: &Solid,
    normal: Vec3,
    owner: Option<usize>,
) -> Vec<Vec<Vertex>> {
    // Boundary surfaces facing out of this volume remain. A cap facing into
    // its neighbour is covered, even when it is exactly on the contact plane.
    if solid.planes.iter().any(|plane| {
        polygon.iter().all(|v| plane.distance(v.pos) <= 0.00001)
            && plane.normal.dot(normal) <= 0.0
            // Fitted frames own coincident trim ends; retaining both exterior
            // faces leaves the old baseboard visibly fighting the new jamb.
            && !(solid.covers_trim && owner.is_some())
            && owner
                .zip(solid.trim_owner)
                .is_none_or(|(current, other)| other >= current)
    }) {
        return vec![polygon];
    }
    let mut inside = polygon;
    let mut outside = Vec::new();
    for plane in &solid.planes {
        // The ownership check above decides coplanar faces. Keeping this same
        // face in both half-spaces would duplicate a complete horizontal cap.
        if inside
            .iter()
            .all(|vertex| plane.distance(vertex.pos).abs() < 1.0e-6)
        {
            continue;
        }
        let outer = clip_half(
            &inside,
            Plane {
                normal: negate(plane.normal),
                offset: -plane.offset,
            },
        );
        if area(&outer) > 1.0e-9 {
            outside.push(outer);
        }
        inside = clip_half(&inside, *plane);
        if area(&inside) <= 1.0e-9 {
            break;
        }
    }
    outside
}
fn clip_half(polygon: &[Vertex], plane: Plane) -> Vec<Vertex> {
    let mut out = Vec::new();
    let Some(mut previous) = polygon.last().copied() else {
        return out;
    };
    let mut previous_distance = plane.distance(previous.pos);
    for vertex in polygon {
        let distance = plane.distance(vertex.pos);
        if (distance >= 0.0) != (previous_distance >= 0.0) {
            out.push(interpolate(
                previous,
                *vertex,
                previous_distance / (previous_distance - distance),
            ));
        }
        if distance >= 0.0 {
            out.push(*vertex);
        }
        previous = *vertex;
        previous_distance = distance;
    }
    out
}
fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let mix = |x: f32, y: f32| (y - x).mul_add(t, x);
    Vertex {
        pos: std::array::from_fn(|i| {
            mix(
                a.pos.get(i).copied().unwrap_or(0.0),
                b.pos.get(i).copied().unwrap_or(0.0),
            )
        }),
        color: std::array::from_fn(|i| {
            mix(
                a.color.get(i).copied().unwrap_or(0.0),
                b.color.get(i).copied().unwrap_or(0.0),
            )
        }),
        caustics_albedo: std::array::from_fn(|i| {
            mix(
                a.caustics_albedo.get(i).copied().unwrap_or(0.0),
                b.caustics_albedo.get(i).copied().unwrap_or(0.0),
            )
        }),
        uv: std::array::from_fn(|i| {
            mix(
                a.uv.get(i).copied().unwrap_or(0.0),
                b.uv.get(i).copied().unwrap_or(0.0),
            )
        }),
        ..a
    }
}
fn area(polygon: &[Vertex]) -> f32 {
    let Some(first) = polygon.first() else {
        return 0.0;
    };
    let origin = Vec3::from_array(first.pos);
    polygon
        .windows(2)
        .filter_map(|pair| {
            let &[a, b] = pair else { return None };
            Some(
                (difference(Vec3::from_array(a.pos), origin))
                    .cross(difference(Vec3::from_array(b.pos), origin))
                    .length()
                    * 0.5,
            )
        })
        .sum()
}

/// Replaces only the newly appended quad, then stamps its actual visible pieces.
pub(super) fn stamp(
    contacts: &Contacts,
    lightmap: Option<LightmapEmit<'_>>,
    vertices: &mut Vec<Vertex>,
    first: usize,
    kind: PatchKind,
    room: Option<usize>,
    owner: Option<usize>,
) {
    let Some(&[corner_a, corner_b, corner_c, _, _, corner_d]) =
        vertices.get(first..).and_then(|v| v.first_chunk::<6>())
    else {
        return;
    };
    let points = [corner_a.pos, corner_b.pos, corner_c.pos, corner_d.pos];
    let normal = Vec3::from_array(quad_normal(points));
    let mut polygon = vec![corner_a, corner_b, corner_c];
    if difference(
        Vec3::from_array(corner_d.pos),
        Vec3::from_array(corner_c.pos),
    )
    .length_squared()
        > 1.0e-18
    {
        polygon.push(corner_d);
    }
    let pieces = contacts.clip(
        polygon,
        normal,
        owner,
        matches!(kind, PatchKind::Floor | PatchKind::Ceiling),
    );
    if let [piece] = pieces.as_slice()
        && piece.as_slice() == [corner_a, corner_b, corner_c, corner_d]
    {
        stamp_lightmap_quad(lightmap, vertices, first, kind, points, room);
        return;
    }
    vertices.truncate(first);
    for piece in pieces {
        if area(&piece) <= 1.0e-9 {
            continue;
        }
        if let [anchor, edge_a, edge_b, edge_c] = piece.as_slice() {
            let start = vertices.len();
            vertices.extend([*anchor, *edge_a, *edge_b, *anchor, *edge_b, *edge_c]);
            stamp_lightmap_quad(
                lightmap,
                vertices,
                start,
                kind,
                [anchor.pos, edge_a.pos, edge_b.pos, edge_c.pos],
                room,
            );
        } else {
            let Some(anchor) = piece.first().copied() else {
                continue;
            };
            for pair in piece.get(1..).unwrap_or_default().windows(2) {
                let &[edge_a, edge_b] = pair else { continue };
                if (difference(Vec3::from_array(edge_a.pos), Vec3::from_array(anchor.pos)))
                    .cross(difference(
                        Vec3::from_array(edge_b.pos),
                        Vec3::from_array(anchor.pos),
                    ))
                    .length_squared()
                    <= 1.0e-18
                {
                    continue;
                }
                let start = vertices.len();
                vertices.extend([anchor, edge_a, edge_b, anchor, edge_b, edge_b]);
                stamp_lightmap_quad(
                    lightmap,
                    vertices,
                    start,
                    kind,
                    [anchor.pos, edge_a.pos, edge_b.pos, edge_b.pos],
                    room,
                );
            }
        }
    }
}

fn difference(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn negate(vector: Vec3) -> Vec3 {
    Vec3::new(-vector.x, -vector.y, -vector.z)
}

#[cfg(test)]
#[path = "../../tests/trim_contact.rs"]
mod tests;

// Intersection arithmetic can leave adjacent copies of one contact corner a
// few micrometres apart. Collapse only those copies and collinear
// residue before triangulation/chart admission, retaining the real boundary.
fn clean_polygon(mut polygon: Vec<Vertex>) -> Vec<Vertex> {
    let same = |a: Vertex, b: Vertex| {
        difference(Vec3::from_array(a.pos), Vec3::from_array(b.pos)).length_squared() <= 4.0e-10
    };
    polygon.dedup_by(|a, b| same(*a, *b));
    if polygon
        .first()
        .zip(polygon.last())
        .is_some_and(|(a, b)| same(*a, *b))
    {
        let _last = polygon.pop();
    }
    loop {
        if polygon.len() < 3 {
            break;
        }
        let redundant = (0..polygon.len()).find(|index| {
            let previous = if *index == 0 {
                polygon.last()
            } else {
                polygon.get(index.saturating_sub(1))
            };
            let next = polygon
                .get(index.saturating_add(1))
                .or_else(|| polygon.first());
            previous
                .zip(polygon.get(*index))
                .zip(next)
                .is_some_and(|((a, b), c)| {
                    difference(Vec3::from_array(b.pos), Vec3::from_array(a.pos))
                        .cross(difference(Vec3::from_array(c.pos), Vec3::from_array(b.pos)))
                        .length_squared()
                        <= 4.0e-18
                })
        });
        let Some(index) = redundant else { break };
        let _removed = polygon.remove(index);
    }
    polygon
}

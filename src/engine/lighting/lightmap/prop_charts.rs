//! Recover chart domains from one placed model primitive at a time.

use super::{LightmapPatch, LightmapPlan, PatchKind};
use crate::engine::render::Vertex;

type PropPositionKey = [u32; 3];
type PropEdgeKey = (PropPositionKey, PropPositionKey);

#[derive(Clone, Copy)]
struct SharedPropChart {
    partner: usize,
    corners: [[f32; 3]; 4],
}

/// Exact source corners, with signed zero canonicalized for adjacency only.
fn prop_position_key(position: [f32; 3]) -> PropPositionKey {
    position.map(|value| {
        let bits = value.to_bits();
        if bits == 0x8000_0000 { 0 } else { bits }
    })
}

fn prop_triangle_edges(triangle: &[Vertex; 3]) -> [PropEdgeKey; 3] {
    let [a, b, c] = triangle.map(|vertex| prop_position_key(vertex.pos));
    [(a, b), (b, c), (c, a)]
}

// glam's bounded floating-point vector operations cannot overflow an integer
// or panic; the existing patch guard rejects nonfinite/degenerate geometry.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded float normal frame math; native patch guards reject unusable domains"
)]
fn prop_triangle_normal(triangle: &[Vertex; 3]) -> glam::Vec3 {
    let [a, b, c] = triangle.map(|vertex| glam::Vec3::from_array(vertex.pos));
    (b - a).cross(c - a).normalize_or_zero()
}

/// Recover only a native-supported four-corner union. The common source edge
/// is always p0/p2, so a tapered quad follows exactly the original two triangles.
fn prop_quad_corners(first: &[Vertex; 3], second: &[Vertex; 3]) -> Option<[[f32; 3]; 4]> {
    use std::collections::BTreeMap;
    // Do not recover a previously rejected sliver merely because its neighbor
    // has a larger frame. Unsupported pairs keep the original triangle path.
    for triangle in [first, second] {
        let [a, b, c] = triangle.map(|vertex| vertex.pos);
        let _triangle_domain = LightmapPatch::from_quad(PatchKind::Prop, [a, b, c, c], None)?;
    }
    let common: Vec<_> = first
        .iter()
        .filter(|vertex| {
            second
                .iter()
                .any(|other| prop_position_key(vertex.pos) == prop_position_key(other.pos))
        })
        .collect();
    let [start, opposite] = common.as_slice() else {
        return None;
    };
    let positions: BTreeMap<_, _> = first
        .iter()
        .chain(second)
        .map(|vertex| (prop_position_key(vertex.pos), vertex.pos))
        .collect();
    if positions.len() != 4
        || prop_triangle_normal(first).dot(prop_triangle_normal(second)) < 0.999_999
    {
        return None;
    }
    for vertex in &common {
        let other = second
            .iter()
            .find(|other| prop_position_key(vertex.pos) == prop_position_key(other.pos))?;
        let alignment =
            glam::Vec3::from_array(vertex.normal).dot(glam::Vec3::from_array(other.normal));
        if !alignment.is_finite() || alignment < 8.0_f32.mul_add(-f32::EPSILON, 1.0) {
            return None;
        }
    }
    let common_keys = [
        prop_position_key(start.pos),
        prop_position_key(opposite.pos),
    ];
    let mut boundary = BTreeMap::new();
    for (from, to) in prop_triangle_edges(first)
        .into_iter()
        .chain(prop_triangle_edges(second))
    {
        if common_keys.contains(&from) && common_keys.contains(&to) {
            continue;
        }
        // Consistent winding gives exactly one outgoing boundary edge per
        // corner. Overlaps, reversed faces and bowties cannot form this loop.
        if boundary.insert(from, to).is_some() {
            return None;
        }
    }
    let mut key = prop_position_key(start.pos);
    let mut corners = [[0.0; 3]; 4];
    for corner in &mut corners {
        *corner = *positions.get(&key)?;
        key = *boundary.get(&key)?;
    }
    let [p0, _, p2, _] = corners;
    if key != prop_position_key(p0) || prop_position_key(p2) != prop_position_key(opposite.pos) {
        return None;
    }
    let patch = LightmapPatch::from_quad(PatchKind::Prop, corners, None)?;
    (!patch.is_triangular()).then_some(corners)
}

/// Pair manifold shared edges only within this placed instance and primitive.
/// Sorting by source triangle order keeps allocation and ambiguous pairing
/// deterministic without changing the source mesh's emission order.
fn shared_prop_charts(triangles: &[[Vertex; 3]]) -> Vec<Option<SharedPropChart>> {
    use std::collections::BTreeMap;
    let mut edges: BTreeMap<PropEdgeKey, Vec<usize>> = BTreeMap::new();
    for (index, triangle) in triangles.iter().enumerate() {
        for (a, b) in prop_triangle_edges(triangle) {
            edges.entry((a.min(b), a.max(b))).or_default().push(index);
        }
    }
    let mut candidates = Vec::new();
    for neighbors in edges.values() {
        let &[first, second] = neighbors.as_slice() else {
            continue;
        };
        let (Some(left), Some(right)) = (triangles.get(first), triangles.get(second)) else {
            continue;
        };
        if let Some(corners) = prop_quad_corners(left, right) {
            candidates.push((first, second, corners));
        }
    }
    candidates.sort_by_key(|(first, second, _)| (*first, *second));
    let mut pairs = vec![None; triangles.len()];
    for (first, second, corners) in candidates {
        if pairs.get(first).is_some_and(Option::is_none)
            && pairs.get(second).is_some_and(Option::is_none)
        {
            if let Some(slot) = pairs.get_mut(first) {
                *slot = Some(SharedPropChart {
                    partner: second,
                    corners,
                });
            }
            if let Some(slot) = pairs.get_mut(second) {
                *slot = Some(SharedPropChart {
                    partner: first,
                    corners,
                });
            }
        }
    }
    pairs
}

fn stamp_shared_prop_chart(
    triangles: &mut [[Vertex; 3]],
    first: usize,
    pair: SharedPropChart,
    plan: &mut LightmapPlan,
    density: f32,
) -> bool {
    if first >= pair.partner {
        return false;
    }
    let Some((earlier, later)) = triangles.split_at_mut_checked(pair.partner) else {
        return false;
    };
    let (Some(left), Some(right)) = (earlier.get_mut(first), later.first_mut()) else {
        return false;
    };
    plan.stamp_prop_quad(left, right, pair.corners, density)
}

/// Stamp prepared atlas receivers onto one placed primitive’s original triangles.
pub fn lightmap_prop_triangles(
    mut triangles: Vec<[Vertex; 3]>,
    plan: &mut LightmapPlan,
    density: f32,
) -> Vec<[Vertex; 3]> {
    let pairs = shared_prop_charts(&triangles);
    let mut stamped = vec![false; triangles.len()];
    let mut retained = Vec::with_capacity(triangles.len());
    for index in 0..triangles.len() {
        let already_stamped = stamped.get(index).copied().unwrap_or(false);
        let shared = !already_stamped
            && pairs.get(index).copied().flatten().is_some_and(|pair| {
                let accepted = stamp_shared_prop_chart(&mut triangles, index, pair, plan, density);
                if accepted && let Some(slot) = stamped.get_mut(pair.partner) {
                    *slot = true;
                }
                accepted
            });
        let Some(triangle) = triangles.get_mut(index) else {
            continue;
        };
        if already_stamped || shared || plan.prepare_prop_triangle(triangle, density) {
            retained.push(*triangle);
        }
    }
    retained
}

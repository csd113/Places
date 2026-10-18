//! Bounded spatial traversal before triangle clipping. Whole branches of one
//! owner or distant geometry never consume quadratic triangle-pair work.

use std::ops::Range;

use super::{Triangle, bounds_overlap};
use crate::engine::spatial::Aabb;

const LEAF_SIZE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Architecture,
    Prop(usize),
    Mixed,
}

struct Node {
    bounds: Aabb,
    range: Range<usize>,
    children: Option<[usize; 2]>,
    owner: Owner,
}

fn build(triangles: &mut [Triangle], start: usize, nodes: &mut Vec<Node>) -> usize {
    let bounds = triangles.iter().fold(Aabb::EMPTY, |bounds, triangle| {
        bounds.union(&triangle.bounds)
    });
    let owner = triangles.first().map_or(Owner::Mixed, |first| {
        if triangles.iter().all(|triangle| triangle.prop == first.prop) {
            first.prop.map_or(Owner::Architecture, Owner::Prop)
        } else {
            Owner::Mixed
        }
    });
    let children = if triangles.len() > LEAF_SIZE {
        let axis = (0..3)
            .max_by(|&a, &b| extent(bounds, a).total_cmp(&extent(bounds, b)))
            .unwrap_or(0);
        let middle = triangles.len() / 2;
        let _partition = triangles.select_nth_unstable_by(middle, |first, second| {
            centre(first.bounds, axis).total_cmp(&centre(second.bounds, axis))
        });
        let (left, right) = triangles.split_at_mut(middle);
        Some([
            build(left, start, nodes),
            build(right, start.saturating_add(middle), nodes),
        ])
    } else {
        None
    };
    let index = nodes.len();
    nodes.push(Node {
        bounds,
        range: start..start.saturating_add(triangles.len()),
        children,
        owner,
    });
    index
}

fn extent(bounds: Aabb, axis: usize) -> f32 {
    bounds.max.get(axis).copied().unwrap_or(0.0) - bounds.min.get(axis).copied().unwrap_or(0.0)
}

fn centre(bounds: Aabb, axis: usize) -> f32 {
    f32::midpoint(
        bounds.min.get(axis).copied().unwrap_or(0.0),
        bounds.max.get(axis).copied().unwrap_or(0.0),
    )
}

/// Visits every cross-owner overlapping pair unless the combined tree/pair
/// work budget runs out. The balanced tree bounds storage and stack depth.
/// Spatially adjacent subtrees are examined before their larger joins.
pub(super) fn visit(
    triangles: &mut [Triangle],
    budget: usize,
    mut candidate: impl FnMut(&Triangle, &Triangle),
) -> (usize, bool) {
    if triangles.is_empty() {
        return (0, true);
    }
    let mut nodes = Vec::new();
    let root = build(triangles, 0, &mut nodes);
    let mut stack = vec![(root, root)];
    let mut work = 0_usize;
    while let Some((a, b)) = stack.pop() {
        if work >= budget {
            return (work, false);
        }
        work = work.saturating_add(1);
        let (Some(first), Some(second)) = (nodes.get(a), nodes.get(b)) else {
            continue;
        };
        if !bounds_overlap(first.bounds, second.bounds)
            || (first.owner != Owner::Mixed && first.owner == second.owner)
        {
            continue;
        }
        if a == b {
            if let Some([left, right]) = first.children {
                stack.extend([(left, right), (right, right), (left, left)]);
            } else {
                for (slot, one) in triangles
                    .get(first.range.clone())
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                {
                    for two in triangles
                        .get(first.range.clone())
                        .unwrap_or_default()
                        .iter()
                        .skip(slot.saturating_add(1))
                    {
                        if !visit_pair(one, two, &mut work, budget, &mut candidate) {
                            return (work, false);
                        }
                    }
                }
            }
        } else if let Some([left, right]) = first.children
            && (second.children.is_none() || first.range.len() >= second.range.len())
        {
            stack.extend([(right, b), (left, b)]);
        } else if let Some([left, right]) = second.children {
            stack.extend([(a, right), (a, left)]);
        } else {
            for one in triangles.get(first.range.clone()).unwrap_or_default() {
                for two in triangles.get(second.range.clone()).unwrap_or_default() {
                    if !visit_pair(one, two, &mut work, budget, &mut candidate) {
                        return (work, false);
                    }
                }
            }
        }
    }
    (work, true)
}

fn visit_pair(
    first: &Triangle,
    second: &Triangle,
    work: &mut usize,
    budget: usize,
    candidate: &mut impl FnMut(&Triangle, &Triangle),
) -> bool {
    if *work >= budget {
        return false;
    }
    *work = work.saturating_add(1);
    if first.prop != second.prop && bounds_overlap(first.bounds, second.bounds) {
        candidate(first, second);
    }
    true
}

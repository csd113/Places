//! Binned surface-area hierarchy for the profiled ray-query hot path.
//! The original tree is retained for point queries and nearest-hit tie identity.
use super::{BVH_LEAF_TRIANGLES, BVH_MAX_DEPTH, BvhNode, TransportTriangle};

const BINS: usize = 16;

#[derive(Clone, Copy)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl Bounds {
    const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    fn include(self, other: Self) -> Self {
        Self {
            min: std::array::from_fn(|axis| self.min[axis].min(other.min[axis])),
            max: std::array::from_fn(|axis| self.max[axis].max(other.max[axis])),
        }
    }

    fn area(self) -> f64 {
        let extent: [f64; 3] = std::array::from_fn(|axis| {
            (f64::from(self.max[axis]) - f64::from(self.min[axis])).max(0.0)
        });
        2.0 * (extent[0] * extent[1] + extent[0] * extent[2] + extent[1] * extent[2])
    }
}

#[derive(Clone, Copy)]
struct Bin {
    bounds: Bounds,
    /// The validated scene contains fewer than u32::MAX triangles.
    count: u32,
}

impl Bin {
    const EMPTY: Self = Self {
        bounds: Bounds::EMPTY,
        count: 0,
    };
    fn include(self, other: Self) -> Self {
        Self {
            bounds: self.bounds.include(other.bounds),
            count: self.count.saturating_add(other.count),
        }
    }
}

pub(super) fn build(
    triangles: &[TransportTriangle],
    centroids: &[[f32; 3]],
) -> (Vec<BvhNode>, Vec<u32>) {
    let bounds: Vec<_> = triangles
        .iter()
        .map(|triangle| {
            [triangle.p0, triangle.p1, triangle.p2].into_iter().fold(
                Bounds::EMPTY,
                |bounds, point| {
                    bounds.include(Bounds {
                        min: point,
                        max: point,
                    })
                },
            )
        })
        .collect();
    let mut order: Vec<_> = (0..triangles.len())
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect();
    let mut nodes = Vec::with_capacity(triangles.len().saturating_mul(2));
    if !order.is_empty() {
        let _len_status = build_node(
            &bounds,
            centroids,
            &mut order,
            &mut nodes,
            0,
            triangles.len(),
            0,
        );
    }
    (nodes, order)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The fixed bin count is 16; normalization is clamped to 0..=15 before truncation, with native NaN-to-zero saturation retaining a valid bin."
)]
fn bin(value: f32, minimum: f32, extent: f32) -> usize {
    ((value - minimum) / extent * BINS as f32).clamp(0.0, (BINS - 1) as f32) as usize
}

fn split(
    bounds: &[Bounds],
    centroids: &[[f32; 3]],
    order: &[u32],
    range: Bounds,
) -> Option<(usize, usize)> {
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for axis in 0..3 {
        let extent = range.max[axis] - range.min[axis];
        if extent <= 0.0 || !extent.is_finite() {
            continue;
        }
        let mut bins = [Bin::EMPTY; BINS];
        for index in order {
            let triangle_index = usize::try_from(*index).unwrap_or(usize::MAX);
            let slot = bin(centroids[triangle_index][axis], range.min[axis], extent);
            bins[slot] = bins[slot].include(Bin {
                bounds: bounds[triangle_index],
                count: 1,
            });
        }
        let mut suffix = [Bin::EMPTY; BINS];
        let mut right = Bin::EMPTY;
        for slot in (0..BINS).rev() {
            right = right.include(bins[slot]);
            suffix[slot] = right;
        }
        let mut left = Bin::EMPTY;
        for (slot, (left_bin, right_bin)) in bins.iter().zip(suffix.iter().skip(1)).enumerate() {
            left = left.include(*left_bin);
            if left.count == 0 || right_bin.count == 0 {
                continue;
            }
            let cost = left.bounds.area() * f64::from(left.count)
                + right_bin.bounds.area() * f64::from(right_bin.count);
            if cost < best_cost {
                best = Some((axis, slot));
                best_cost = cost;
            }
        }
    }
    best
}

fn partition(
    order: &mut [u32],
    centroids: &[[f32; 3]],
    range: Bounds,
    axis: usize,
    split: usize,
) -> usize {
    let extent = range.max[axis] - range.min[axis];
    let mut left = 0usize;
    let mut right = order.len();
    while left < right {
        let index = usize::try_from(order[left]).unwrap_or(usize::MAX);
        if bin(centroids[index][axis], range.min[axis], extent) <= split {
            left = left.saturating_add(1);
        } else {
            right = right.saturating_sub(1);
            order.swap(left, right);
        }
    }
    left
}

fn build_node(
    bounds: &[Bounds],
    centroids: &[[f32; 3]],
    order: &mut [u32],
    nodes: &mut Vec<BvhNode>,
    start: usize,
    end: usize,
    depth: u32,
) -> u32 {
    let index = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
    let mut geometry = Bounds::EMPTY;
    let mut centres = Bounds::EMPTY;
    for entry in &order[start..end] {
        let triangle_index = usize::try_from(*entry).unwrap_or(usize::MAX);
        geometry = geometry.include(bounds[triangle_index]);
        centres = centres.include(Bounds {
            min: centroids[triangle_index],
            max: centroids[triangle_index],
        });
    }
    let count = end.saturating_sub(start);
    nodes.push(BvhNode {
        min: geometry.min,
        max: geometry.max,
        first: u32::try_from(start).unwrap_or(0),
        count: u32::try_from(count).unwrap_or(u32::MAX),
        right: 0,
    });
    if count <= BVH_LEAF_TRIANGLES || depth >= BVH_MAX_DEPTH {
        return index;
    }
    let offset = split(bounds, centroids, &order[start..end], centres)
        .map_or(count / 2, |(axis, cut)| {
            partition(&mut order[start..end], centroids, centres, axis, cut)
        });
    if offset == 0 || offset >= count {
        return index;
    }
    let mid = start.saturating_add(offset);
    let left = build_node(
        bounds,
        centroids,
        order,
        nodes,
        start,
        mid,
        depth.saturating_add(1),
    );
    let right = build_node(
        bounds,
        centroids,
        order,
        nodes,
        mid,
        end,
        depth.saturating_add(1),
    );
    if let Some(node) = nodes.get_mut(usize::try_from(index).unwrap_or(usize::MAX)) {
        node.first = left;
        node.right = right;
        node.count = 0;
    }
    index
}

//! Conservative rejection tree. Leaf tests retain the original oriented clip.
use super::visibility::directional_box_hit;

pub(super) trait DirectionalSolid {
    fn directional_bounds(&self) -> ([f32; 3], [f32; 3]);
    fn directional_hit(&self, origin: [f32; 3], direction: [f32; 3]) -> bool;
    fn directional_index_eligible(&self) -> bool {
        let (min, max) = self.directional_bounds();
        min.iter().chain(&max).all(|v| v.is_finite())
    }
    fn directional_center(&self, axis: usize) -> f32 {
        let (min, max) = self.directional_bounds();
        min.get(axis)
            .copied()
            .unwrap_or(0.0)
            .mul_add(0.5, max.get(axis).copied().unwrap_or(0.0) * 0.5)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct DirectionalIndex {
    nodes: Vec<Node>,
    order: Vec<usize>,
}

#[derive(Clone, Debug)]
struct Node {
    min: [f32; 3],
    max: [f32; 3],
    start: usize,
    end: usize,
    next: usize,
}

impl DirectionalIndex {
    pub fn build(props: &[impl DirectionalSolid]) -> Self {
        // At most ~8 MiB per solid class of derived acceleration; hostile giant records retain
        // the exact linear oracle instead of growing a second unbounded index.
        if props.len() > (1_usize << 18_u32)
            || props.iter().any(|prop| !prop.directional_index_eligible())
        {
            return Self::default();
        }
        let mut tree = Self {
            nodes: Vec::with_capacity(props.len().saturating_div(4).saturating_add(1)),
            order: (0..props.len()).collect(),
        };
        if !props.is_empty() {
            tree.partition(props, 0, props.len());
        }
        tree
    }

    fn partition(&mut self, props: &[impl DirectionalSolid], start: usize, end: usize) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for index in self.order.get(start..end).into_iter().flatten() {
            if let Some(prop) = props.get(*index) {
                let (low, high) = prop.directional_bounds();
                for ((lo, hi), (a, b)) in
                    min.iter_mut().zip(&mut max).zip(low.into_iter().zip(high))
                {
                    *lo = lo.min(a);
                    *hi = hi.max(b);
                }
            }
        }
        let slot = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            start,
            end,
            next: 0,
        });
        if end.saturating_sub(start) > 8 {
            let axis = (0..3)
                .max_by(|a, b| {
                    let extent = |axis| {
                        max.get(axis).copied().unwrap_or(0.0)
                            - min.get(axis).copied().unwrap_or(0.0)
                    };
                    extent(*a).total_cmp(&extent(*b))
                })
                .unwrap_or(0);
            if let Some(order) = self.order.get_mut(start..end) {
                order.sort_unstable_by(|a, b| {
                    let center = |index: usize| {
                        props
                            .get(index)
                            .map_or(0.0, |prop| prop.directional_center(axis))
                    };
                    center(*a).total_cmp(&center(*b)).then(a.cmp(b))
                });
            }
            let middle = start.saturating_add(end.saturating_sub(start) / 2);
            self.partition(props, start, middle);
            self.partition(props, middle, end);
        }
        let next = self.nodes.len();
        if let Some(node) = self.nodes.get_mut(slot) {
            node.next = next;
        }
    }

    pub fn occludes(
        &self,
        props: &[impl DirectionalSolid],
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> bool {
        if self.nodes.is_empty() {
            return props
                .iter()
                .any(|prop| prop.directional_hit(origin, direction));
        }
        let mut slot = 0;
        while let Some(node) = self.nodes.get(slot) {
            if !directional_box_hit(origin, direction, node.min, node.max) {
                slot = node.next;
            } else if node.end.saturating_sub(node.start) > 8 {
                slot = slot.saturating_add(1);
            } else {
                if self
                    .order
                    .get(node.start..node.end)
                    .into_iter()
                    .flatten()
                    .any(|index| {
                        props
                            .get(*index)
                            .is_some_and(|prop| prop.directional_hit(origin, direction))
                    })
                {
                    return true;
                }
                slot = node.next;
            }
        }
        false
    }

    pub const fn retained_heap_bytes(&self) -> usize {
        self.nodes
            .capacity()
            .saturating_mul(std::mem::size_of::<Node>())
            .saturating_add(
                self.order
                    .capacity()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
    }
}

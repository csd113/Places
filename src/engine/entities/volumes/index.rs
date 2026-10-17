//! Static balanced AABB tree over authored triggers. Swept actor bounds admit
//! candidates, plus previously occupied boxes so disappearance and refused exits
//! can be retried. Buffers are reserved at load. Empty-space queries traverse the
//! tree instead of every volume; arbitrary large boxes need no global fallback.

use std::collections::HashMap;

use glam::Vec3;

use super::{MAX_TRIGGER_ITEMS, MAX_TRIGGER_PATH_POINTS, TriggerItem};
use crate::engine::entities::components::{ComponentTable, Transform, TriggerVolume};
use crate::engine::entities::id::{EntityHandle, EntityNames};
use crate::engine::level::TriggerFilterDef;

const ITEM_MASK_WORDS: usize = MAX_TRIGGER_ITEMS.div_ceil(64);

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct TriggerQueryStats {
    pub volumes_sampled: usize,
    pub item_sweeps: usize,
    pub entity_sweeps: usize,
    pub spatial_candidates: usize,
    pub broadphase_nodes: usize,
}

#[derive(Clone, Copy, Default)]
pub(in crate::engine::entities) struct Candidate {
    pub player: bool,
    pub items: [u64; ITEM_MASK_WORDS],
    pub entity: Option<usize>,
}

impl Candidate {
    pub fn item_indices(self) -> impl Iterator<Item = usize> {
        self.items
            .into_iter()
            .enumerate()
            .flat_map(|(word_index, mut word)| {
                std::iter::from_fn(move || {
                    if word == 0 {
                        return None;
                    }
                    let bit = usize::try_from(word.trailing_zeros()).ok()?;
                    word &= word.wrapping_sub(1);
                    Some(word_index.saturating_mul(64).saturating_add(bit))
                })
            })
    }
}

struct WatchedEntity {
    id: String,
    previous: Option<(EntityHandle, Vec3)>,
    pub sample: Option<(EntityHandle, Vec3, Vec3)>,
    physical: bool,
}

#[derive(Default)]
pub(in crate::engine::entities) struct TriggerIndex {
    tree: Vec<Node>,
    bounds: Vec<Bounds>,
    order: Vec<usize>,
    traversal: Vec<usize>,
    pub handles: Vec<EntityHandle>,
    filters: Vec<TriggerFilterDef>,
    watchers: Vec<WatchedEntity>,
    watch_by_id: HashMap<String, usize>,
    watch_for_volume: Vec<Option<usize>>,
    pub candidates: Vec<usize>,
    pub actor_candidates: Vec<Candidate>,
    occupied: Vec<usize>,
    next_occupied: Vec<usize>,
    epoch: u64,
    stamps: Vec<u64>,

    query_slots: Vec<usize>,
    pub stats: TriggerQueryStats,
}

impl TriggerIndex {
    pub fn build(volumes: &ComponentTable<TriggerVolume>) -> Self {
        let count = volumes.len();
        let mut result = Self {
            handles: Vec::with_capacity(count),
            filters: Vec::with_capacity(count),
            watch_for_volume: Vec::with_capacity(count),
            candidates: Vec::with_capacity(count),
            actor_candidates: vec![Candidate::default(); count],
            occupied: Vec::with_capacity(count),
            next_occupied: Vec::with_capacity(count),
            stamps: vec![0; count],
            bounds: Vec::with_capacity(count),
            order: Vec::with_capacity(count),
            tree: Vec::with_capacity(count.saturating_mul(2)),
            traversal: Vec::with_capacity(count.saturating_mul(2)),
            query_slots: Vec::with_capacity(count),
            ..Self::default()
        };
        for (handle, volume) in volumes.iter() {
            let slot = result.handles.len();
            result.handles.push(handle);
            result.filters.push(volume.filter.clone());
            let watch = if let TriggerFilterDef::Entity { instance } = &volume.filter {
                let next = result.watchers.len();
                let watch_index =
                    *result
                        .watch_by_id
                        .entry(instance.clone())
                        .or_insert_with(|| {
                            result.watchers.push(WatchedEntity {
                                id: instance.clone(),
                                previous: None,
                                sample: None,
                                physical: false,
                            });
                            next
                        });
                Some(watch_index)
            } else {
                None
            };
            result.watch_for_volume.push(watch);
            result.bounds.push(Bounds {
                low: Vec3::new(volume.bounds[0], volume.bottom_y, volume.bounds[2]),
                high: Vec3::new(volume.bounds[1], volume.top_y, volume.bounds[3]),
            });
            result.order.push(slot);
        }
        if count > 0 {
            let _ = build_node(
                &mut result.tree,
                &mut result.order,
                &result.bounds,
                0,
                count,
            );
        }
        result
    }

    pub fn begin(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamps.fill(0);
            self.epoch = 1;
        }
        self.candidates.clear();
        self.next_occupied.clear();
        self.stats = TriggerQueryStats::default();
        for index in 0..self.occupied.len() {
            if let Some(slot) = self.occupied.get(index).copied() {
                self.claim(slot);
            }
        }
        for watch in &mut self.watchers {
            watch.physical = false;
        }
    }

    pub fn player(&mut self, from: Vec3, to: Vec3) {
        if !from.is_finite() || !to.is_finite() {
            return;
        }
        self.gather(from.min(to), from.max(to));
        for index in 0..self.query_slots.len() {
            let Some(slot) = self.query_slots.get(index).copied() else {
                continue;
            };
            if self
                .filters
                .get(slot)
                .is_some_and(TriggerFilterDef::accepts_player)
            {
                self.claim(slot);
                if let Some(candidate) = self.actor_candidates.get_mut(slot) {
                    candidate.player = true;
                }
            }
        }
    }

    pub fn item(&mut self, item: &TriggerItem<'_>, item_index: usize) {
        if let Some(watch) = self
            .watch_by_id
            .get(item.instance)
            .and_then(|index| self.watchers.get_mut(*index))
        {
            watch.physical = true;
        }
        let mut low = item.from.min(item.to);
        let mut high = item.from.max(item.to);
        if let Some(path) = item.path {
            if path.len() > MAX_TRIGGER_PATH_POINTS || path.iter().any(|point| !point.is_finite()) {
                return;
            }
            for point in path {
                low = low.min(*point);
                high = high.max(*point);
            }
        }
        self.gather(
            Vec3::new(
                low.x - item.half_extents.x,
                low.y - item.half_extents.y,
                low.z - item.half_extents.z,
            ),
            Vec3::new(
                high.x + item.half_extents.x,
                high.y + item.half_extents.y,
                high.z + item.half_extents.z,
            ),
        );
        for index in 0..self.query_slots.len() {
            let Some(slot) = self.query_slots.get(index).copied() else {
                continue;
            };
            if self
                .filters
                .get(slot)
                .is_some_and(|filter| filter.accepts_item(item))
            {
                self.claim(slot);
                if let Some(word) = self
                    .actor_candidates
                    .get_mut(slot)
                    .and_then(|candidate| candidate.items.get_mut(item_index / 64))
                {
                    *word |= 1_u64 << item_index.checked_rem(64).unwrap_or_default();
                }
            }
        }
    }

    pub fn entities(
        &mut self,
        names: &EntityNames,
        transforms: &ComponentTable<Transform>,
        seed: bool,
    ) {
        for watch_index in 0..self.watchers.len() {
            let Some(watch) = self.watchers.get_mut(watch_index) else {
                continue;
            };
            let current = names
                .get(&watch.id)
                .and_then(|actor| transforms.get(actor).map(|pose| (actor, pose.position)));
            let previous = watch.previous;
            watch.previous = current;
            watch.sample = current.map(|(actor, to)| {
                let from = if seed {
                    to
                } else {
                    previous
                        .filter(|(old_actor, _)| *old_actor == actor)
                        .map_or(to, |(_, position)| position)
                };
                (actor, from, to)
            });
            if watch.physical {
                continue;
            }
            let Some((_, from, to)) = watch.sample else {
                continue;
            };
            if !from.is_finite() || !to.is_finite() {
                continue;
            }
            self.gather(from.min(to), from.max(to));
            for index in 0..self.query_slots.len() {
                let Some(slot) = self.query_slots.get(index).copied() else {
                    continue;
                };
                if self.watch_for_volume.get(slot).copied().flatten() == Some(watch_index) {
                    self.claim(slot);
                    if let Some(candidate) = self.actor_candidates.get_mut(slot) {
                        candidate.entity = Some(watch_index);
                    }
                }
            }
        }
        self.candidates.sort_unstable();
    }

    pub fn entity_sample(&self, watch: usize) -> Option<(EntityHandle, Vec3, Vec3)> {
        self.watchers.get(watch)?.sample
    }

    pub fn finish_volume(&mut self, slot: usize, inside: bool) {
        self.stats.volumes_sampled = self.stats.volumes_sampled.saturating_add(1);
        if inside {
            self.next_occupied.push(slot);
        }
    }

    pub const fn finish(&mut self) {
        std::mem::swap(&mut self.occupied, &mut self.next_occupied);
    }

    fn claim(&mut self, slot: usize) {
        let Some(stamp) = self.stamps.get_mut(slot) else {
            return;
        };
        if *stamp != self.epoch {
            *stamp = self.epoch;
            self.candidates.push(slot);
            if let Some(candidate) = self.actor_candidates.get_mut(slot) {
                *candidate = Candidate::default();
            }
        }
    }

    fn gather(&mut self, low: Vec3, high: Vec3) {
        self.query_slots.clear();
        self.traversal.clear();
        if self.tree.is_empty() {
            return;
        }
        self.traversal.push(0);
        let query = Bounds { low, high };
        while let Some(index) = self.traversal.pop() {
            self.stats.broadphase_nodes = self.stats.broadphase_nodes.saturating_add(1);
            let Some(node) = self.tree.get(index) else {
                continue;
            };
            if !node.bounds.overlaps(query) {
                continue;
            }
            if let Some((left, right)) = node.children {
                self.traversal.push(right);
                self.traversal.push(left);
            } else if let Some(order) = self.order.get(node.start..node.end) {
                for &slot in order {
                    if self
                        .bounds
                        .get(slot)
                        .is_some_and(|bounds| bounds.overlaps(query))
                    {
                        self.query_slots.push(slot);
                    }
                }
            }
        }
        self.stats.spatial_candidates = self
            .stats
            .spatial_candidates
            .saturating_add(self.query_slots.len());
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    low: Vec3,
    high: Vec3,
}

impl Bounds {
    fn overlaps(self, other: Self) -> bool {
        self.low.cmple(other.high).all() && self.high.cmpge(other.low).all()
    }

    const fn center_axis(self, axis: usize) -> f32 {
        match axis {
            0 => f32::midpoint(self.low.x, self.high.x),
            1 => f32::midpoint(self.low.y, self.high.y),
            _ => f32::midpoint(self.low.z, self.high.z),
        }
    }
}

struct Node {
    bounds: Bounds,
    children: Option<(usize, usize)>,
    start: usize,
    end: usize,
}

fn build_node(
    nodes: &mut Vec<Node>,
    order: &mut [usize],
    bounds: &[Bounds],
    start: usize,
    end: usize,
) -> usize {
    let mut combined = Bounds {
        low: Vec3::splat(f32::INFINITY),
        high: Vec3::splat(f32::NEG_INFINITY),
    };
    if let Some(entries) = order.get(start..end) {
        for &slot in entries {
            if let Some(bound) = bounds.get(slot) {
                combined.low = combined.low.min(bound.low);
                combined.high = combined.high.max(bound.high);
            }
        }
    }
    let index = nodes.len();
    nodes.push(Node {
        bounds: combined,
        children: None,
        start,
        end,
    });
    if end.saturating_sub(start) > 8 {
        let span_x = combined.high.x - combined.low.x;
        let span_y = combined.high.y - combined.low.y;
        let span_z = combined.high.z - combined.low.z;
        let axis = if span_x >= span_z && span_x >= span_y {
            0
        } else if span_y >= span_z {
            1
        } else {
            2
        };
        if let Some(entries) = order.get_mut(start..end) {
            entries.sort_unstable_by(|a, b| {
                let left = bounds.get(*a).map_or(0.0, |bound| bound.center_axis(axis));
                let right = bounds.get(*b).map_or(0.0, |bound| bound.center_axis(axis));
                left.total_cmp(&right).then_with(|| a.cmp(b))
            });
        }
        let middle = start.saturating_add(end.saturating_sub(start) / 2);
        let left = build_node(nodes, order, bounds, start, middle);
        let right = build_node(nodes, order, bounds, middle, end);
        if let Some(node) = nodes.get_mut(index) {
            node.children = Some((left, right));
        }
    }
    index
}

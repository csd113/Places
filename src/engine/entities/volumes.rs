//! Swept, spatially indexed trigger occupancy shared by every authored action kind.

mod index;
mod publish;
pub(super) use index::TriggerIndex;
pub use index::TriggerQueryStats;
use publish::{CrossingBatch, VisitOwner};
pub(super) use publish::{TriggerVisit, VisitSet};

use glam::Vec3;

use super::EntityWorld;
use super::audio::CheckpointOccupant;
use super::components::TriggerVolume;
use super::id::{EntityHandle, EntityNames};
use crate::engine::level::{ItemReceiptDef, TriggerFilterDef, TriggerOccupancyDef};

/// Maximum physical snapshots consumed per update, matching the item world cap.
pub const MAX_TRIGGER_ITEMS: usize = crate::engine::physics::MAX_BODIES;

/// Frame start, up to twelve fixed substeps and the final held-item pose.
pub const MAX_TRIGGER_PATH_POINTS: usize = 14;

/// One physical item's movement over the current presentation frame.
///
/// Centers and conservative world AABB half extents come from the physics world.
/// Instance and asset ids are the owning entity's exact authored identities.
#[derive(Clone, Copy, Debug)]
pub struct TriggerItem<'a> {
    pub key: u64,
    pub instance: &'a str,
    pub asset: &'a str,
    pub from: Vec3,
    pub to: Vec3,
    pub half_extents: Vec3,
    pub held: bool,
    /// Actual dynamic solver sleep; false for dormant, held or kinematic bodies.
    pub sleeping: bool,
    /// Actual rigid-body local +Y projected on world +Y, for physical games.
    pub up_y: f32,
    /// Optional solver path; omitted preserves the endpoint-segment API.
    pub path: Option<&'a [Vec3]>,
    /// Physical shape-centre path; omitted uses the ordinary path/endpoints.
    pub centre_path: Option<&'a [Vec3]>,
    /// Normalized solver tick times shared by the origin and centre paths.
    pub path_times: Option<&'a [f64]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OccupantId {
    Player,
    Item(u64),
    Entity(EntityHandle),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VolumeOccupant {
    id: OccupantId,
    key: String,
    actor: Option<EntityHandle>,
    position: Vec3,
}

impl VolumeOccupant {
    fn checkpoint_id(&self) -> CheckpointOccupant {
        match self.id {
            OccupantId::Player => CheckpointOccupant::Player,
            OccupantId::Item(_) => CheckpointOccupant::Item(self.key.clone()),
            OccupantId::Entity(_) => CheckpointOccupant::Entity(self.key.clone()),
        }
    }

    fn matches_checkpoint(&self, occupant: &CheckpointOccupant) -> bool {
        match (&self.id, occupant) {
            (OccupantId::Player, CheckpointOccupant::Player) => true,
            (OccupantId::Item(_), CheckpointOccupant::Item(id))
            | (OccupantId::Entity(_), CheckpointOccupant::Entity(id)) => self.key == *id,
            _ => false,
        }
    }
}

impl TriggerVolume {
    pub(super) fn checkpoint_visit(&self, visit: &TriggerVisit) -> Option<CheckpointOccupant> {
        match self.visits.owner(visit)? {
            VisitOwner::Aggregate => self.inside.then_some(CheckpointOccupant::Aggregate),
            VisitOwner::Actor(actor) => self
                .occupants
                .iter()
                .find(|occupant| occupant.id == actor)
                .map(VolumeOccupant::checkpoint_id),
        }
    }

    pub(super) fn restore_trigger_visit(
        &mut self,
        saved: &CheckpointOccupant,
    ) -> Option<TriggerVisit> {
        if *saved == CheckpointOccupant::Aggregate {
            return if self.inside && self.occupancy == TriggerOccupancyDef::Aggregate {
                self.visits.restore(None)
            } else {
                None
            };
        }
        let mut matching = self
            .occupants
            .iter()
            .filter(|occupant| occupant.matches_checkpoint(saved));
        let actor = matching.next()?.id;
        if self.occupancy == TriggerOccupancyDef::PerActor && matching.next().is_none() {
            self.visits.restore(Some(actor))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub(super) struct Sweep {
    occupant: VolumeOccupant,
    intervals: SweepIntervals,
    at_end: bool,
    was_inside: bool,
}

#[derive(Clone, Copy, Debug)]
struct CrossingTime {
    frame: f64,
    // Multiple accepted positions can share one solver time. Keep the
    // traversal order of those instantaneous segments at that timestamp.
    instant_order: f64,
}

impl CrossingTime {
    const FRAME_START: Self = Self {
        frame: 0.0,
        instant_order: 0.0,
    };
    const FRAME_END: Self = Self {
        frame: 1.0,
        instant_order: 0.0,
    };

    fn segment(
        (start, end): (f64, f64),
        fraction: f32,
        progress: f64,
        times: &[f64],
        index: usize,
    ) -> Self {
        let at_start = fraction <= 0.0;
        let at_end = fraction >= 1.0;
        let frame = if at_start {
            start
        } else if at_end {
            end
        } else {
            (end - start).mul_add(f64::from(fraction), start)
        };
        let shares_instant = start.total_cmp(&end).is_eq()
            || (at_start
                && index
                    .checked_sub(1)
                    .and_then(|previous| times.get(previous))
                    == Some(&start))
            || (at_end && times.get(index.saturating_add(2)) == Some(&end));
        Self {
            frame,
            instant_order: if shares_instant { progress } else { 0.0 },
        }
    }

    fn cmp(self, other: Self) -> std::cmp::Ordering {
        self.frame
            .total_cmp(&other.frame)
            .then_with(|| self.instant_order.total_cmp(&other.instant_order))
    }
}

#[derive(Clone, Copy, Debug)]
struct CrossingPoint {
    time: CrossingTime,
    progress: f64,
    position: Vec3,
}

#[derive(Debug, Default)]
struct SweepIntervals {
    inline: [Option<(CrossingPoint, CrossingPoint)>; MAX_TRIGGER_PATH_POINTS],
    extra: Vec<(CrossingPoint, CrossingPoint)>,
    len: usize,
}

impl SweepIntervals {
    fn push(&mut self, enter: CrossingPoint, exit: CrossingPoint) {
        let previous = self.extra.last_mut().or_else(|| {
            self.len
                .checked_sub(1)
                .and_then(|index| self.inline.get_mut(index))
                .and_then(Option::as_mut)
        });
        if let Some((_, previous_exit)) = previous
            && enter.progress <= previous_exit.progress
        {
            if exit.progress > previous_exit.progress {
                *previous_exit = exit;
            }
            return;
        }
        if let Some(slot) = self.inline.get_mut(self.len) {
            *slot = Some((enter, exit));
        } else {
            self.extra.push((enter, exit));
        }
        self.len = self.len.saturating_add(1);
    }

    fn iter(&self) -> impl Iterator<Item = (CrossingPoint, CrossingPoint)> {
        self.inline
            .iter()
            .flatten()
            .chain(self.extra.iter())
            .copied()
    }
}

pub(super) struct Edge {
    time: CrossingTime,
    entered: bool,
    occupant: VolumeOccupant,
}

impl TriggerFilterDef {
    const fn accepts_player(&self) -> bool {
        matches!(self, Self::Player | Self::Any)
    }

    fn accepts_item(&self, item: &TriggerItem<'_>) -> bool {
        match self {
            Self::Player => false,
            Self::Any => true,
            Self::Item {
                asset,
                instance,
                held,
            } => {
                asset.as_deref().is_none_or(|id| id == item.asset)
                    && instance.as_deref().is_none_or(|id| id == item.instance)
                    && held.is_none_or(|required| required == item.held)
            }
            Self::Entity { instance } => instance == item.instance,
        }
    }
}

impl EntityWorld {
    /// Baselines every eligible occupant without emitting edges after a load or teleport.
    pub fn seed_volume_occupants(&mut self, feet: Vec3, items: &[TriggerItem<'_>]) {
        self.seed_volume_occupants_iter(feet, items.iter().copied());
    }

    /// Iterator form for a persistent bounded physics snapshot buffer.
    pub fn seed_volume_occupants_iter<'a>(
        &mut self,
        feet: Vec3,
        items: impl Iterator<Item = TriggerItem<'a>> + Clone,
    ) {
        self.sample_volume_occupants(&[feet], None, items, true);
        self.rebind_checkpoint_loops();
    }

    /// Backward-compatible player-only baseline; callers with items use the combined API.
    pub fn seed_volumes(&mut self, feet: Vec3) {
        self.seed_volume_occupants(feet, &[]);
    }

    /// Baselines the player after a stance/teleport change without fabricating a sweep.
    pub fn reseed_volumes(&mut self, feet: Vec3) {
        let broadphase = &mut self.trigger_index;
        broadphase.begin();
        broadphase.player(&[feet]);
        for candidate_index in 0..broadphase.candidates.len() {
            let Some(slot) = broadphase.candidates.get(candidate_index).copied() else {
                continue;
            };
            let Some(handle) = broadphase.handles.get(slot).copied() else {
                continue;
            };
            let Some(volume) = self.components.volumes.get_mut(handle) else {
                continue;
            };
            volume
                .occupants
                .retain(|occupant| occupant.id != OccupantId::Player);
            let player_inside =
                volume.filter.accepts_player() && volume.contains(feet.x, feet.z, feet.y);
            if player_inside {
                volume.occupants.push(player_occupant(feet));
            }
            volume.inside = !volume.occupants.is_empty();
            volume.visits.reseed_player(player_inside, volume.inside);
            broadphase.finish_volume(slot, volume.inside);
        }
        broadphase.finish();
    }

    /// Updates all volumes once after player movement and physical items have advanced.
    /// Complete fast crossings emit ordered entry and exit events in the same frame.
    /// Only the first [`MAX_TRIGGER_ITEMS`] snapshots are consumed; producers are bounded
    /// to the same cap. Missing items leave their previous volumes on this update.
    pub fn update_volume_occupants(&mut self, from: Vec3, to: Vec3, items: &[TriggerItem<'_>]) {
        self.update_volume_occupants_iter(from, to, items.iter().copied());
    }

    /// Iterator form; cloning the iterator does not allocate a temporary item list.
    pub fn update_volume_occupants_iter<'a>(
        &mut self,
        from: Vec3,
        to: Vec3,
        items: impl Iterator<Item = TriggerItem<'a>> + Clone,
    ) {
        self.update_volume_paths_iter(&[from, to], None, items);
    }

    /// Updates trigger occupancy from the player's accepted movement points.
    pub fn update_volume_paths_iter<'a>(
        &mut self,
        player_path: &[Vec3],
        player_times: Option<&[f64]>,
        items: impl Iterator<Item = TriggerItem<'a>>,
    ) {
        self.sample_volume_occupants(player_path, player_times, items, false);
    }

    /// Backward-compatible player-only update for consumers without physical items.
    pub fn update_volumes(&mut self, from: Vec3, to: Vec3) {
        self.update_volume_occupants(from, to, &[]);
    }

    fn sample_volume_occupants<'a>(
        &mut self,
        player_path: &[Vec3],
        player_times: Option<&[f64]>,
        items: impl Iterator<Item = TriggerItem<'a>>,
        seed: bool,
    ) {
        let snapshots = materialize_snapshots(items);
        let broadphase = &mut self.trigger_index;
        broadphase.begin();
        broadphase.player(player_path);
        for (index, item) in snapshots.iter().enumerate() {
            broadphase.item(item, index);
        }
        broadphase.entities(&self.names, &self.components.transforms, seed);
        let mut batch = CrossingBatch::new(self.events.remaining_capacity());
        let sweeps = &mut self.trigger_sweeps;
        let edges = &mut self.trigger_edges;
        for candidate_index in 0..broadphase.candidates.len() {
            let Some(slot) = broadphase.candidates.get(candidate_index).copied() else {
                continue;
            };
            let Some(handle) = broadphase.handles.get(slot).copied() else {
                continue;
            };
            let Some(candidate) = broadphase.actor_candidates.get(slot).copied() else {
                continue;
            };
            let Some(volume) = self.components.volumes.get_mut(handle) else {
                continue;
            };
            sweeps.clear();
            edges.clear();
            let initial_count = volume.occupants.len();
            if candidate.player
                && let Some(sweep) = player_sweep(volume, player_path, player_times)
            {
                sweeps.push(sweep);
            }
            if let Some(sample) = candidate
                .entity
                .and_then(|watch| broadphase.entity_sample(watch))
                && let Some(sweep) = entity_sweep(volume, sample)
            {
                broadphase.stats.entity_sweeps = broadphase.stats.entity_sweeps.saturating_add(1);
                sweeps.push(sweep);
            }
            for item_index in candidate.item_indices() {
                let Some(item) = snapshots.get(item_index).copied() else {
                    continue;
                };
                broadphase.stats.item_sweeps = broadphase.stats.item_sweeps.saturating_add(1);
                if !volume.filter.accepts_item(&item)
                    || sweeps
                        .iter()
                        .any(|sweep| sweep.occupant.id == OccupantId::Item(item.key))
                {
                    continue;
                }
                if let Some(sweep) = physical_sweep(volume, &item, &self.names) {
                    sweeps.push(sweep);
                }
            }
            if seed {
                volume.visits.clear();
            } else {
                occupancy_edges(volume, sweeps, edges, initial_count);
                for edge in edges.drain(..) {
                    let visit = volume.visits.transition(volume.occupancy, &edge);
                    batch.offer(self.generation, handle, edge, visit);
                }
                volume.visits.prune();
            }
            volume.occupants.clear();
            volume.occupants.extend(
                sweeps
                    .drain(..)
                    .filter(|sweep| sweep.at_end)
                    .map(|sweep| sweep.occupant),
            );
            volume.inside = !volume.occupants.is_empty();
            broadphase.finish_volume(slot, volume.inside);
        }
        broadphase.finish();
        if batch.publish(&mut self.events) {
            self.stop_dropped_trigger_loops();
        }
    }

    /// Last update's exact-work counts for explicit developer diagnostics.
    #[must_use]
    pub const fn trigger_query_stats(&self) -> TriggerQueryStats {
        self.trigger_index.stats
    }
}

fn materialize_snapshots<'a>(items: impl Iterator<Item = TriggerItem<'a>>) -> Vec<TriggerItem<'a>> {
    items
        .take(MAX_TRIGGER_ITEMS)
        .filter(|item| {
            item.from.is_finite()
                && item.to.is_finite()
                && item.half_extents.is_finite()
                && item.half_extents.cmpge(Vec3::ZERO).all()
        })
        .collect()
}

fn physical_sweep(
    volume: &mut TriggerVolume,
    item: &TriggerItem<'_>,
    names: &EntityNames,
) -> Option<Sweep> {
    let receipt = receipt_item(volume, item)?;
    let intervals = item_intervals(volume, &receipt)?;
    let existing = volume
        .occupants
        .iter()
        .position(|occupant| occupant.id == OccupantId::Item(item.key));
    let occupant = existing.map_or_else(
        || VolumeOccupant {
            id: OccupantId::Item(item.key),
            key: item.instance.to_string(),
            actor: names.get(item.instance),
            position: receipt.to,
        },
        |index| volume.occupants.swap_remove(index),
    );
    Some(Sweep {
        occupant: VolumeOccupant {
            position: receipt.to,
            ..occupant
        },
        intervals,
        at_end: endpoint_inside(volume, receipt.to, receipt.half_extents),
        was_inside: existing.is_some(),
    })
}

fn receipt_item<'a>(volume: &TriggerVolume, item: &TriggerItem<'a>) -> Option<TriggerItem<'a>> {
    if volume.item_receipt == ItemReceiptDef::Overlap {
        return Some(*item);
    }
    let path = item.centre_path.or(item.path);
    if path.is_some_and(|points| {
        points.len() > MAX_TRIGGER_PATH_POINTS || points.iter().any(|point| !point.is_finite())
    }) {
        return None;
    }
    let from = path
        .and_then(|points| points.first().copied())
        .unwrap_or(item.from);
    let to = path
        .and_then(|points| points.last().copied())
        .unwrap_or(item.to);
    if !from.is_finite() || !to.is_finite() {
        return None;
    }
    if volume.item_receipt == ItemReceiptDef::SettledCentre {
        // A receipt is a final rest state, never a transient crossing followed
        // by sleep outside. Waking or picking up produces the normal exit.
        if item.held || !item.sleeping || !endpoint_inside(volume, to, Vec3::ZERO) {
            return None;
        }
        return Some(TriggerItem {
            from: to,
            to,
            half_extents: Vec3::ZERO,
            path: None,
            path_times: None,
            ..*item
        });
    }
    Some(TriggerItem {
        from,
        to,
        half_extents: Vec3::ZERO,
        path,
        ..*item
    })
}

fn entity_sweep(
    volume: &mut TriggerVolume,
    (actor, path, times): (EntityHandle, &[Vec3], &[f64]),
) -> Option<Sweep> {
    let TriggerFilterDef::Entity { instance } = &volume.filter else {
        return None;
    };

    let position = *path.last()?;
    let intervals = path_intervals(volume, path, Some(times), Vec3::ZERO)?;
    let existing = volume
        .occupants
        .iter()
        .position(|occupant| occupant.id == OccupantId::Entity(actor));
    let occupant = if let Some(index) = existing {
        volume.occupants.swap_remove(index)
    } else {
        VolumeOccupant {
            id: OccupantId::Entity(actor),
            key: instance.clone(),
            actor: Some(actor),
            position,
        }
    };
    Some(Sweep {
        occupant: VolumeOccupant {
            position,
            ..occupant
        },
        intervals,
        at_end: endpoint_inside(volume, position, Vec3::ZERO),
        was_inside: existing.is_some(),
    })
}

fn player_sweep(volume: &mut TriggerVolume, path: &[Vec3], times: Option<&[f64]>) -> Option<Sweep> {
    if !volume.filter.accepts_player() {
        return None;
    }
    let to = *path.last()?;
    let intervals = path_intervals(volume, path, times, Vec3::ZERO)?;
    let existing = volume
        .occupants
        .iter()
        .position(|occupant| occupant.id == OccupantId::Player);
    Some(Sweep {
        occupant: existing.map_or_else(
            || player_occupant(to),
            |index| {
                let occupant = volume.occupants.swap_remove(index);
                VolumeOccupant {
                    position: to,
                    ..occupant
                }
            },
        ),
        intervals,
        at_end: volume.contains(to.x, to.z, to.y),
        was_inside: existing.is_some(),
    })
}

fn player_occupant(position: Vec3) -> VolumeOccupant {
    VolumeOccupant {
        id: OccupantId::Player,
        key: "player".to_string(),
        actor: Some(crate::engine::ai::PLAYER_TARGET_HANDLE),
        position,
    }
}

fn endpoint_inside(volume: &TriggerVolume, point: Vec3, half: Vec3) -> bool {
    point.x >= volume.bounds[0] - half.x
        && point.x <= volume.bounds[1] + half.x
        && point.y >= volume.bottom_y - half.y
        && point.y <= volume.top_y + half.y
        && point.z >= volume.bounds[2] - half.z
        && point.z <= volume.bounds[3] + half.z
}

/// Slab intersection over [0, 1], expanded by each item's physical AABB.
fn interval(volume: &TriggerVolume, from: Vec3, to: Vec3, half: Vec3) -> Option<(f32, f32)> {
    if !from.is_finite() || !to.is_finite() {
        return None;
    }
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (start, end, low, high) in [
        (
            from.x,
            to.x,
            volume.bounds[0] - half.x,
            volume.bounds[1] + half.x,
        ),
        (
            from.y,
            to.y,
            volume.bottom_y - half.y,
            volume.top_y + half.y,
        ),
        (
            from.z,
            to.z,
            volume.bounds[2] - half.z,
            volume.bounds[3] + half.z,
        ),
    ] {
        let delta = end - start;
        if delta.abs() <= f32::EPSILON {
            if start < low || start > high {
                return None;
            }
        } else {
            let near = (low - start) / delta;
            let far = (high - start) / delta;
            enter = enter.max(near.min(far));
            exit = exit.min(near.max(far));
            if enter > exit {
                return None;
            }
        }
    }
    Some((enter, exit))
}

fn item_intervals(volume: &TriggerVolume, item: &TriggerItem<'_>) -> Option<SweepIntervals> {
    if let Some(path) = item.path.filter(|path| path.len() >= 2) {
        if path.len() > MAX_TRIGGER_PATH_POINTS || path.iter().any(|point| !point.is_finite()) {
            return None;
        }
        path_intervals(volume, path, item.path_times, item.half_extents)
    } else {
        path_intervals(volume, &[item.from, item.to], None, item.half_extents)
    }
}

fn path_intervals(
    volume: &TriggerVolume,
    path: &[Vec3],
    times: Option<&[f64]>,
    half: Vec3,
) -> Option<SweepIntervals> {
    if times.is_some_and(|values| {
        values.len() != path.len()
            || values
                .iter()
                .any(|time| !(0.0_f64..=1.0_f64).contains(time))
            || !values.is_sorted()
    }) {
        return None;
    }
    let mut intervals = SweepIntervals::default();
    let first = *path.first()?;
    if path.len() == 1 {
        if endpoint_inside(volume, first, half) {
            intervals.push(
                CrossingPoint {
                    time: CrossingTime::FRAME_START,
                    progress: 0.0,
                    position: first,
                },
                CrossingPoint {
                    time: CrossingTime::FRAME_END,
                    progress: 1.0,
                    position: first,
                },
            );
        }
    } else {
        let segments = f64::from(u32::try_from(path.len().saturating_sub(1)).ok()?);
        for (index, pair) in path.windows(2).enumerate() {
            let (&from, &to) = (pair.first()?, pair.get(1)?);
            if let Some((enter, exit)) = interval(volume, from, to, half) {
                let offset = f64::from(u32::try_from(index).ok()?);
                let explicit_times = times.and_then(|values| {
                    values
                        .get(index)
                        .copied()
                        .zip(values.get(index.saturating_add(1)).copied())
                });
                let crossing = |fraction| {
                    let progress = offset + f64::from(fraction);
                    let time = explicit_times.map_or(
                        CrossingTime {
                            frame: progress / segments,
                            instant_order: 0.0,
                        },
                        |(start, end)| {
                            CrossingTime::segment(
                                (start, end),
                                fraction,
                                progress,
                                times.unwrap_or_default(),
                                index,
                            )
                        },
                    );
                    CrossingPoint {
                        time,
                        progress,
                        position: from.lerp(to, fraction),
                    }
                };
                intervals.push(crossing(enter), crossing(exit));
            }
        }
    }
    (intervals.len > 0).then_some(intervals)
}

fn occupancy_edges(
    volume: &TriggerVolume,
    sweeps: &[Sweep],
    edges: &mut Vec<Edge>,
    initial_count: usize,
) {
    for occupant in &volume.occupants {
        edges.push(Edge {
            time: CrossingTime::FRAME_START,
            entered: false,
            occupant: occupant.clone(),
        });
    }
    for sweep in sweeps {
        for (index, (enter, exit)) in sweep.intervals.iter().enumerate() {
            let starts_inside = index == 0 && sweep.was_inside;
            if starts_inside && enter.progress > 0.0_f64 {
                edges.push(Edge {
                    time: CrossingTime::FRAME_START,
                    entered: false,
                    occupant: sweep.occupant.clone(),
                });
            }
            if !starts_inside || enter.progress > 0.0_f64 {
                edges.push(Edge {
                    time: enter.time,
                    entered: true,
                    occupant: sweep.occupant.clone(),
                });
            }
            if !sweep.at_end || index.saturating_add(1) < sweep.intervals.len {
                edges.push(Edge {
                    time: exit.time,
                    entered: false,
                    occupant: VolumeOccupant {
                        position: exit.position,
                        ..sweep.occupant.clone()
                    },
                });
            }
        }
    }
    // Enter before exit at a shared boundary prevents an aggregate ambience
    // from stopping when one occupant hands occupancy directly to another.
    edges.sort_by(|a, b| a.time.cmp(b.time).then_with(|| b.entered.cmp(&a.entered)));
    if volume.occupancy == TriggerOccupancyDef::Aggregate {
        let mut count = initial_count;
        edges.retain(|edge| {
            let before = count;
            count = if edge.entered {
                count.saturating_add(1)
            } else {
                count.saturating_sub(1)
            };
            (before == 0) != (count == 0)
        });
    }
}

/// Read-only current-state preview, using the same filter and receipt policy
/// as volume occupancy. It does not fabricate crossing events or run actions.
pub(super) fn receipt_preview(volume: &TriggerVolume, item: &TriggerItem<'_>) -> bool {
    volume.filter.accepts_item(item)
        && receipt_item(volume, item)
            .is_some_and(|receipt| endpoint_inside(volume, receipt.to, receipt.half_extents))
}

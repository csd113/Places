//! Swept, bounded trigger occupancy shared by every authored action kind.

use glam::Vec3;

use super::EntityWorld;
use super::components::TriggerVolume;
use super::events::{EventKind, EventRecord};
use super::id::EntityHandle;
use crate::engine::level::{TriggerFilterDef, TriggerOccupancyDef};

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
    /// Optional solver path; omitted preserves the endpoint-segment API.
    pub path: Option<&'a [Vec3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OccupantId {
    Player,
    Item(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VolumeOccupant {
    id: OccupantId,
    key: String,
    actor: Option<EntityHandle>,
}

#[derive(Debug)]
pub(super) struct Sweep {
    occupant: VolumeOccupant,
    intervals: SweepIntervals,
    at_end: bool,
    was_inside: bool,
}

#[derive(Clone, Copy, Debug)]
struct SweepIntervals {
    values: [(f32, f32); MAX_TRIGGER_PATH_POINTS],
    len: usize,
}

impl SweepIntervals {
    const fn new() -> Self {
        Self {
            values: [(0.0, 0.0); MAX_TRIGGER_PATH_POINTS],
            len: 0,
        }
    }

    fn push(&mut self, enter: f32, exit: f32) {
        if let Some(previous) = self
            .len
            .checked_sub(1)
            .and_then(|index| self.values.get_mut(index))
            && enter <= previous.1
        {
            previous.1 = previous.1.max(exit);
            return;
        }
        if let Some(slot) = self.values.get_mut(self.len) {
            *slot = (enter, exit);
            self.len = self.len.saturating_add(1);
        }
    }

    fn iter(&self) -> impl Iterator<Item = (f32, f32)> {
        self.values.iter().take(self.len).copied()
    }
}

pub(super) struct Edge {
    time: f32,
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
        self.sample_volume_occupants(feet, feet, &items.take(MAX_TRIGGER_ITEMS), true);
    }

    /// Backward-compatible player-only baseline; callers with items use the combined API.
    pub fn seed_volumes(&mut self, feet: Vec3) {
        self.seed_volume_occupants(feet, &[]);
    }

    /// Baselines the player after a stance/teleport change without fabricating a sweep.
    pub fn reseed_volumes(&mut self, feet: Vec3) {
        for (_, volume) in self.components.volumes.iter_mut() {
            volume
                .occupants
                .retain(|occupant| occupant.id != OccupantId::Player);
            if volume.filter.accepts_player() && volume.contains(feet.x, feet.z, feet.y) {
                volume.occupants.push(player_occupant());
            }
            volume.inside = !volume.occupants.is_empty();
        }
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
        self.sample_volume_occupants(from, to, &items.take(MAX_TRIGGER_ITEMS), false);
    }

    /// Backward-compatible player-only update for consumers without physical items.
    pub fn update_volumes(&mut self, from: Vec3, to: Vec3) {
        self.update_volume_occupants(from, to, &[]);
    }

    fn sample_volume_occupants<'a>(
        &mut self,
        from: Vec3,
        to: Vec3,
        items: &(impl Iterator<Item = TriggerItem<'a>> + Clone),
        seed: bool,
    ) {
        let sweeps = &mut self.trigger_sweeps;
        let edges = &mut self.trigger_edges;
        for (handle, volume) in self.components.volumes.iter_mut() {
            sweeps.clear();
            edges.clear();
            let initial_count = volume.occupants.len();
            if let Some(sweep) = player_sweep(volume, from, to) {
                sweeps.push(sweep);
            }
            for item in (*items).clone().take(MAX_TRIGGER_ITEMS).filter(|item| {
                item.from.is_finite()
                    && item.to.is_finite()
                    && item.half_extents.is_finite()
                    && item.half_extents.cmpge(Vec3::ZERO).all()
            }) {
                if !volume.filter.accepts_item(&item)
                    || sweeps
                        .iter()
                        .any(|sweep| sweep.occupant.id == OccupantId::Item(item.key))
                {
                    continue;
                }
                if let Some(intervals) = item_intervals(volume, &item) {
                    let existing = volume
                        .occupants
                        .iter()
                        .position(|occupant| occupant.id == OccupantId::Item(item.key));
                    let occupant = existing.map_or_else(
                        || VolumeOccupant {
                            id: OccupantId::Item(item.key),
                            key: item.instance.to_string(),
                            actor: self
                                .names
                                .get(item.instance)
                                .filter(|actor| self.store.contains(*actor)),
                        },
                        |index| volume.occupants.swap_remove(index),
                    );
                    sweeps.push(Sweep {
                        occupant,
                        intervals,
                        at_end: endpoint_inside(volume, item.to, item.half_extents),
                        was_inside: existing.is_some(),
                    });
                }
            }
            if !seed {
                occupancy_edges(volume, sweeps, edges, initial_count);
            }
            if !seed && edges.len() > self.events.remaining_capacity() {
                self.events.refuse_batch(edges.len());
                // Restore the prior authoritative membership so a refused final
                // exit is retried next frame. Complete outside/outside pairs may
                // be refused together, but can never leave an orphaned loop.
                volume.occupants.extend(
                    sweeps
                        .drain(..)
                        .filter(|sweep| sweep.was_inside)
                        .map(|sweep| sweep.occupant),
                );
                edges.clear();
                continue;
            }
            volume.occupants.clear();
            volume.occupants.extend(
                sweeps
                    .drain(..)
                    .filter(|sweep| sweep.at_end)
                    .map(|sweep| sweep.occupant),
            );
            volume.inside = !volume.occupants.is_empty();
            for edge in edges.drain(..) {
                let _queued = self.events.push(EventRecord {
                    generation: self.generation,
                    kind: if edge.entered {
                        EventKind::EnterVolume
                    } else {
                        EventKind::ExitVolume
                    },
                    subject: handle,
                    key: edge.occupant.key,
                    actor: edge.occupant.actor,
                    depth: 0,
                });
            }
        }
    }
}

fn player_sweep(volume: &mut TriggerVolume, from: Vec3, to: Vec3) -> Option<Sweep> {
    if !volume.filter.accepts_player() {
        return None;
    }
    let (enter, exit) = interval(volume, from, to, Vec3::ZERO)?;
    let existing = volume
        .occupants
        .iter()
        .position(|occupant| occupant.id == OccupantId::Player);
    let mut intervals = SweepIntervals::new();
    intervals.push(enter, exit);
    Some(Sweep {
        occupant: existing
            .map_or_else(player_occupant, |index| volume.occupants.swap_remove(index)),
        intervals,
        at_end: volume.contains(to.x, to.z, to.y),
        was_inside: existing.is_some(),
    })
}

fn player_occupant() -> VolumeOccupant {
    VolumeOccupant {
        id: OccupantId::Player,
        key: "player".to_string(),
        actor: Some(crate::engine::ai::PLAYER_TARGET_HANDLE),
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
    let mut intervals = SweepIntervals::new();
    if let Some(path) = item.path.filter(|path| path.len() >= 2) {
        if path.len() > MAX_TRIGGER_PATH_POINTS || path.iter().any(|point| !point.is_finite()) {
            return None;
        }
        let segments = path.len().saturating_sub(1);
        let denominator = f32::from(u16::try_from(segments).ok()?);
        for (index, pair) in path.windows(2).enumerate() {
            let (&from, &to) = (pair.first()?, pair.get(1)?);
            if let Some((enter, exit)) = interval(volume, from, to, item.half_extents) {
                let offset = f32::from(u16::try_from(index).ok()?);
                intervals.push(
                    (offset + enter) / denominator,
                    (offset + exit) / denominator,
                );
            }
        }
    } else if let Some((enter, exit)) = interval(volume, item.from, item.to, item.half_extents) {
        intervals.push(enter, exit);
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
            time: 0.0,
            entered: false,
            occupant: occupant.clone(),
        });
    }
    for sweep in sweeps {
        for (index, (enter, exit)) in sweep.intervals.iter().enumerate() {
            let starts_inside = index == 0 && sweep.was_inside;
            if starts_inside && enter > 0.0 {
                edges.push(Edge {
                    time: 0.0,
                    entered: false,
                    occupant: sweep.occupant.clone(),
                });
            }
            if !starts_inside || enter > 0.0 {
                edges.push(Edge {
                    time: enter,
                    entered: true,
                    occupant: sweep.occupant.clone(),
                });
            }
            if !sweep.at_end || index.saturating_add(1) < sweep.intervals.len {
                edges.push(Edge {
                    time: exit,
                    entered: false,
                    occupant: sweep.occupant.clone(),
                });
            }
        }
    }
    // Enter before exit at a shared boundary prevents an aggregate ambience
    // from stopping when one occupant hands occupancy directly to another.
    edges.sort_by(|a, b| {
        a.time
            .total_cmp(&b.time)
            .then_with(|| b.entered.cmp(&a.entered))
    });
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

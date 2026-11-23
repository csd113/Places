//! Chronological admission of volume edges to the shared FIFO.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};

use super::{CrossingTime, Edge, OccupantId};
use crate::engine::entities::events::{EventKind, EventQueue, EventRecord};
use crate::engine::entities::id::EntityHandle;
use crate::engine::level::{ActionDef, EventBindingDef, EventKindName, TriggerOccupancyDef};

/// Shared by one occupancy interval, its queued entry, and any loop that entry
/// starts. A dropped exit invalidates only that interval's playback.
#[derive(Clone, Debug, Default)]
pub(in crate::engine::entities) struct TriggerVisit(Arc<AtomicBool>);

impl PartialEq for TriggerVisit {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl TriggerVisit {
    pub fn exit_dropped(&self) -> bool {
        self.0.load(AtomicOrdering::Relaxed)
    }
}

#[derive(Clone, Debug)]
struct PendingVisit(Weak<AtomicBool>);

impl PendingVisit {
    fn new(visit: &TriggerVisit) -> Self {
        Self(Arc::downgrade(&visit.0))
    }

    fn upgrade(&self) -> Option<TriggerVisit> {
        self.0.upgrade().map(TriggerVisit)
    }

    fn live(&self) -> bool {
        self.0.strong_count() != 0
    }

    fn belongs_to(&self, visit: &TriggerVisit) -> bool {
        std::ptr::eq(self.0.as_ptr(), Arc::as_ptr(&visit.0))
    }
}

impl PartialEq for PendingVisit {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

pub(super) enum VisitOwner {
    Aggregate,
    Actor(OccupantId),
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::engine::entities) struct VisitSet {
    tracks_audio: bool,
    aggregate: Option<PendingVisit>,
    actors: Vec<(OccupantId, PendingVisit)>,
}

impl VisitSet {
    pub fn new(bindings: &[EventBindingDef]) -> Self {
        Self {
            tracks_audio: bindings.iter().any(|binding| {
                binding.on == EventKindName::ExitVolume
                    && binding
                        .actions
                        .iter()
                        .any(|action| matches!(action, ActionDef::StopSound { target: _ }))
            }),
            aggregate: None,
            actors: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.aggregate = None;
        self.actors = Vec::new();
    }

    pub fn reseed_player(&mut self, player_inside: bool, inside: bool) {
        if !player_inside {
            self.actors
                .retain(|(actor, _)| *actor != OccupantId::Player);
        }
        if !inside {
            self.aggregate = None;
        }
    }

    /// Occupancy only needs to remember visits that still have a queued event
    /// or running audio. In particular refused entries cannot accumulate here.
    pub(super) fn prune(&mut self) {
        if self.aggregate.as_ref().is_some_and(|visit| !visit.live()) {
            self.aggregate = None;
        }
        self.actors.retain(|(_, visit)| visit.live());
        if self.actors.capacity() > self.actors.len().saturating_mul(2) {
            self.actors.shrink_to_fit();
        }
    }

    pub(super) fn owner(&self, visit: &TriggerVisit) -> Option<VisitOwner> {
        if self
            .aggregate
            .as_ref()
            .is_some_and(|pending| pending.belongs_to(visit))
        {
            Some(VisitOwner::Aggregate)
        } else {
            self.actors
                .iter()
                .find(|(_, pending)| pending.belongs_to(visit))
                .map(|(actor, _)| VisitOwner::Actor(*actor))
        }
    }

    pub(super) fn restore(&mut self, occupant: Option<OccupantId>) -> Option<TriggerVisit> {
        if !self.tracks_audio {
            return None;
        }
        if let Some(actor) = occupant {
            if let Some(visit) = self
                .actors
                .iter()
                .find(|(existing, _)| *existing == actor)
                .and_then(|(_, pending)| pending.upgrade())
            {
                return Some(visit);
            }
            let visit = TriggerVisit::default();
            self.actors.push((actor, PendingVisit::new(&visit)));
            Some(visit)
        } else if let Some(visit) = self.aggregate.as_ref().and_then(PendingVisit::upgrade) {
            Some(visit)
        } else {
            let visit = TriggerVisit::default();
            self.aggregate = Some(PendingVisit::new(&visit));
            Some(visit)
        }
    }

    pub(super) fn transition(
        &mut self,
        occupancy: TriggerOccupancyDef,
        edge: &Edge,
    ) -> Option<TriggerVisit> {
        if !self.tracks_audio {
            return None;
        }
        let actor = edge.occupant.id;
        if occupancy == TriggerOccupancyDef::Aggregate {
            return if edge.entered {
                let visit = TriggerVisit::default();
                self.aggregate = Some(PendingVisit::new(&visit));
                Some(visit)
            } else {
                self.aggregate.take()?.upgrade()
            };
        }
        if edge.entered {
            let visit = TriggerVisit::default();
            self.actors.push((actor, PendingVisit::new(&visit)));
            Some(visit)
        } else {
            let index = self.actors.iter().position(|(id, _)| *id == actor)?;
            self.actors.swap_remove(index).1.upgrade()
        }
    }
}

struct Crossing {
    time: CrossingTime,
    order: usize,
    record: EventRecord,
}

impl Ord for Crossing {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(other.time)
            .then_with(|| self.order.cmp(&other.order))
    }
}

impl PartialOrd for Crossing {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Crossing {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Crossing {}

pub(super) struct CrossingBatch {
    pending: BinaryHeap<Crossing>,
    capacity: usize,
    order: usize,
    dropped: usize,
    dropped_loop_exit: bool,
}

impl CrossingBatch {
    pub(super) const fn new(capacity: usize) -> Self {
        Self {
            pending: BinaryHeap::new(),
            capacity,
            order: 0,
            dropped: 0,
            dropped_loop_exit: false,
        }
    }

    pub(super) fn offer(
        &mut self,
        generation: u64,
        subject: EntityHandle,
        edge: Edge,
        trigger_visit: Option<TriggerVisit>,
    ) {
        let crossing = Crossing {
            time: edge.time,
            order: self.order,
            record: EventRecord {
                generation,
                subject,
                kind: if edge.entered {
                    EventKind::EnterVolume
                } else {
                    EventKind::ExitVolume
                },
                key: edge.occupant.key,
                actor: edge.occupant.actor,
                origin: Some(edge.occupant.position),
                depth: 0,
                trigger_visit,
            },
        };
        self.order = self.order.saturating_add(1);
        // The index visits volumes in authored order. Keep only the earliest
        // edges that can be admitted, even when a later volume crossed first.
        if self.pending.len() < self.capacity {
            self.pending.push(crossing);
            return;
        }
        let rejected = if let Some(mut latest) = self.pending.peek_mut()
            && crossing < *latest
        {
            std::mem::replace(&mut *latest, crossing)
        } else {
            crossing
        };
        self.dropped = self.dropped.saturating_add(1);
        if rejected.record.kind == EventKind::ExitVolume
            && let Some(visit) = rejected.record.trigger_visit
        {
            visit.0.store(true, AtomicOrdering::Relaxed);
            self.dropped_loop_exit = true;
        }
    }

    pub(super) fn publish(self, events: &mut EventQueue) -> bool {
        for crossing in self.pending.into_sorted_vec() {
            let _queued = events.push(crossing.record);
        }
        events.refuse_batch(self.dropped);
        self.dropped_loop_exit
    }
}

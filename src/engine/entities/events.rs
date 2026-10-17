//! The runtime event queue and typed condition evaluation.
//!
//! An authored binding listens for one event kind and runs only while every one
//! of its conditions holds. This module owns the runtime vocabulary of that
//! contract: [`EventKind`] mirrors the authored [`EventKindName`] as a plain
//! enum, [`EventRecord`] is one occurrence queued by a producer, [`EventQueue`]
//! is the bounded FIFO the dispatcher drains once per tick, and [`evaluate`]
//! answers a [`ConditionDef`] through a read-only [`ConditionView`].
//!
//! The queue never resolves a handle or touches the component tables; it is
//! deliberately dumb storage. Everything that needs world knowledge arrives
//! through the view, which keeps condition evaluation total: a target the world
//! does not know makes its condition false, never a panic. Equality between
//! state values is exact on the [`StateValue`] variant — `Int(1)` is not
//! `Float(1.0)` — so a condition can never compare across numeric
//! representations.

use std::collections::VecDeque;

use crate::engine::entities::components::StateValue;
use crate::engine::entities::id::EntityHandle;
use crate::engine::level::{ConditionDef, EventKindName};

/// An event kind an authored binding can listen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// The player interacted with the subject.
    Interact,
    /// An eligible occupant entered the subject volume.
    EnterVolume,
    /// An eligible occupant left the subject volume.
    ExitVolume,
    /// A timer on the subject elapsed.
    Timer,
    /// A typed state of the subject changed.
    ObjectState,
    /// A sequence running on the subject completed or stopped.
    SequenceComplete,
    /// The subject entity was spawned.
    Spawn,
    /// An animation on the subject completed.
    AnimationComplete,
    /// An AI agent changed state.
    AiState,
    /// An AI agent caught its prey.
    Caught,
}

impl EventKind {
    /// The stable serialized name, matching the authored `snake_case` tag.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Interact => "interact",
            Self::EnterVolume => "enter_volume",
            Self::ExitVolume => "exit_volume",
            Self::Timer => "timer",
            Self::ObjectState => "object_state",
            Self::SequenceComplete => "sequence_complete",
            Self::Spawn => "spawn",
            Self::AnimationComplete => "animation_complete",
            Self::AiState => "ai_state",
            Self::Caught => "caught",
        }
    }

    /// Parses a serialized name back to its kind; `None` when the name is not
    /// part of the runtime vocabulary.
    #[must_use]
    pub const fn from_name(name: &str) -> Option<Self> {
        match name.as_bytes() {
            b"interact" => Some(Self::Interact),
            b"enter_volume" => Some(Self::EnterVolume),
            b"exit_volume" => Some(Self::ExitVolume),
            b"timer" => Some(Self::Timer),
            b"object_state" => Some(Self::ObjectState),
            b"sequence_complete" => Some(Self::SequenceComplete),
            b"spawn" => Some(Self::Spawn),
            b"animation_complete" => Some(Self::AnimationComplete),
            b"ai_state" => Some(Self::AiState),
            b"caught" => Some(Self::Caught),
            _ => None,
        }
    }

    /// Maps an authored event kind to its runtime mirror.
    #[must_use]
    pub const fn parse(kind: EventKindName) -> Self {
        match kind {
            EventKindName::Interact => Self::Interact,
            EventKindName::EnterVolume => Self::EnterVolume,
            EventKindName::ExitVolume => Self::ExitVolume,
            EventKindName::Timer => Self::Timer,
            EventKindName::ObjectState => Self::ObjectState,
            EventKindName::SequenceComplete => Self::SequenceComplete,
            EventKindName::Spawn => Self::Spawn,
            EventKindName::AnimationComplete => Self::AnimationComplete,
            EventKindName::AiState => Self::AiState,
            EventKindName::Caught => Self::Caught,
        }
    }
}

/// One queued event occurrence.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    /// World position of the occupant that caused this occurrence, when
    /// known. Reusable actions can choose an away-facing door swing without
    /// depending on a player or editor singleton.
    pub origin: Option<glam::Vec3>,
    /// World generation the record was produced in. A record whose generation
    /// differs from the current world's is dropped by
    /// [`EventQueue::drain_stale`], never applied.
    pub generation: u64,
    /// What happened.
    pub kind: EventKind,
    /// The entity the event happened to.
    pub subject: EntityHandle,
    /// Kind-specific discriminator: the state name for an object-state change,
    /// the timer id for a timer, the clip for an animation completion; empty
    /// when the kind has none.
    pub key: String,
    /// The entity that caused the event, when the producer knows.
    pub actor: Option<EntityHandle>,
    /// Chain depth this record was produced at: 0 from an external producer
    /// (the interaction key, a volume edge, a timer), parent + 1 from an
    /// action or sequence step that ran inside another event's dispatch.
    ///
    /// The dispatcher enforces [`crate::engine::entities::MAX_CHAIN_DEPTH`]
    /// against this value, including when a bounded budget defers the record
    /// to another tick, so delayed chains cannot restart their depth count.
    pub depth: u8,
}

/// The bounded FIFO between event producers and the dispatcher.
///
/// Producers push in the order they detect an occurrence and the dispatcher
/// pops in that same order, so a tick's bindings run in a deterministic
/// sequence. A full queue refuses the push instead of allocating: the producer
/// keeps the authoritative count from the `false` return, and [`Self::dropped`]
/// records the total refusals for the once-per-run diagnostic.
#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    /// Queued occurrences, oldest first.
    records: VecDeque<EventRecord>,
    /// Pushes refused since the last [`Self::clear`].
    dropped: usize,
}

impl EventQueue {
    /// Most occurrences one queue holds before it refuses new pushes.
    ///
    /// A bound, not a tuning knob: one tick can never fan out into unbounded
    /// memory, and a producer that is refused names the burst in its
    /// diagnostic instead of silently dropping work.
    ///
    /// Raised to 4096 from 256 in the 2026 capacity pass. The level format
    /// admits up to [`crate::engine::level::MAX_LEVEL_AREA_TRIGGERS`] (4000) trigger
    /// volumes, and a tick in which every trigger gains its first occupant
    /// produces one occurrence per trigger: a 256-slot queue would refuse
    /// most of them. 4096 holds one occurrence per trigger plus a spawn
    /// burst's worth of headroom, and the record is a small plain-data
    /// struct, so the queue's worst-case footprint stays in the tens of
    /// kilobytes.
    pub const MAX_QUEUED_EVENTS: usize = 4096;

    /// An empty queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: VecDeque::new(),
            dropped: 0,
        }
    }

    /// Queues `record`, returning `false` when the queue is full.
    ///
    /// A refused push leaves the queue untouched and increments the drop
    /// counter; the caller reports and counts it.
    pub fn push(&mut self, record: EventRecord) -> bool {
        if self.records.len() >= Self::MAX_QUEUED_EVENTS {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.records.push_back(record);
        true
    }

    /// Removes and returns the oldest record.
    pub fn pop(&mut self) -> Option<EventRecord> {
        self.records.pop_front()
    }

    /// Space available for an atomic producer batch.
    #[must_use]
    pub fn remaining_capacity(&self) -> usize {
        Self::MAX_QUEUED_EVENTS.saturating_sub(self.records.len())
    }

    /// Records an atomically refused batch without inserting a partial prefix.
    pub(super) const fn refuse_batch(&mut self, count: usize) {
        self.dropped = self.dropped.saturating_add(count);
    }

    /// The oldest record without removing it.
    #[must_use]
    pub fn peek(&self) -> Option<&EventRecord> {
        self.records.front()
    }

    /// Number of queued records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when no record is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Drops every queued record and resets the drop counter.
    pub fn clear(&mut self) {
        self.records.clear();
        self.dropped = 0;
    }

    /// Pushes refused since the last [`Self::clear`].
    #[must_use]
    pub const fn dropped(&self) -> usize {
        self.dropped
    }

    /// Drops every record whose generation differs from `generation`.
    ///
    /// Called when the world is replaced: everything queued for the old world
    /// belongs to handles that no longer resolve, and is discarded as a batch.
    /// Records for `generation` keep their relative order. Returns the number
    /// of records dropped.
    pub fn drain_stale(&mut self, generation: u64) -> usize {
        let before = self.records.len();
        self.records
            .retain(|record| record.generation == generation);
        before.saturating_sub(self.records.len())
    }
}

/// The read-only window into the world that [`evaluate`] needs.
///
/// Every method answers `None` when the target does not exist or does not carry
/// the component the condition asks about. The world implements this over its
/// component tables; the queue and the condition vocabulary stay independent of
/// the storage.
pub trait ConditionView {
    /// The current value of state `name` on `target`.
    fn state(&self, target: &str, name: &str) -> Option<StateValue>;

    /// Whether `target` exists and its interaction/component is enabled.
    fn enabled(&self, target: &str) -> Option<bool>;

    /// Whether `target` exists and is locked.
    fn locked(&self, target: &str) -> Option<bool>;

    /// Whether `target` exists and is a door that is open.
    fn door_open(&self, target: &str) -> Option<bool>;

    /// Whether `target` exists and has a sequence running.
    fn sequence_running(&self, target: &str) -> Option<bool>;
}

/// Evaluates one authored condition against the world.
///
/// Total for every definition and every view: a target the view does not know
/// (or that lacks the component the condition asks about) makes the condition
/// false, never a panic. State equality is exact on the [`StateValue`] variant:
/// `Int(1)` and `Float(1.0)` are different values and can never satisfy the
/// same condition, and a missing state or a state of another variant is false
/// for `Enabled`-style conditions just as it is for `State`.
#[must_use]
pub fn evaluate(condition: &ConditionDef, view: &impl ConditionView) -> bool {
    match condition {
        ConditionDef::State {
            target,
            name,
            equals,
        } => view
            .state(target, name)
            .is_some_and(|value| value == *equals),
        ConditionDef::Enabled { target } => view.enabled(target) == Some(true),
        ConditionDef::Disabled { target } => view.enabled(target) == Some(false),
        ConditionDef::Locked { target } => view.locked(target) == Some(true),
        ConditionDef::Unlocked { target } => view.locked(target) == Some(false),
        ConditionDef::DoorOpen { target } => view.door_open(target) == Some(true),
        ConditionDef::DoorClosed { target } => view.door_open(target) == Some(false),
        ConditionDef::SequenceRunning { target } => view.sequence_running(target) == Some(true),
        ConditionDef::SequenceIdle { target } => view.sequence_running(target) == Some(false),
    }
}

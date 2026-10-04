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

use crate::entities::components::StateValue;
use crate::entities::id::EntityHandle;
use crate::level::{ConditionDef, EventKindName};

/// An event kind an authored binding can listen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// The player interacted with the subject.
    Interact,
    /// The player's feet entered the subject volume.
    EnterVolume,
    /// The player's feet left the subject volume.
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRecord {
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
    /// The dispatcher's per-tick wave counter is what enforces
    /// [`crate::entities::MAX_CHAIN_DEPTH`]; this field records the depth the
    /// producing action ran at, so a diagnostic can name how deep a chain got.
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
    /// admits up to [`crate::level::MAX_LEVEL_AREA_TRIGGERS`] (4000) trigger
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use std::collections::HashMap;

    use super::*;

    /// A tiny in-memory [`ConditionView`] for exercising every condition kind.
    #[derive(Default)]
    struct TestView {
        states: HashMap<(String, String), StateValue>,
        enabled: HashMap<String, bool>,
        locked: HashMap<String, bool>,
        doors: HashMap<String, bool>,
        sequences: HashMap<String, bool>,
    }

    impl TestView {
        fn with_state(mut self, target: &str, name: &str, value: StateValue) -> Self {
            drop(
                self.states
                    .insert((target.to_owned(), name.to_owned()), value),
            );
            self
        }

        fn with_enabled(mut self, target: &str, value: bool) -> Self {
            let _previous_value = self.enabled.insert(target.to_owned(), value);
            self
        }

        fn with_locked(mut self, target: &str, value: bool) -> Self {
            let _previous_value = self.locked.insert(target.to_owned(), value);
            self
        }

        fn with_door(mut self, target: &str, value: bool) -> Self {
            let _previous_value = self.doors.insert(target.to_owned(), value);
            self
        }

        fn with_sequence(mut self, target: &str, value: bool) -> Self {
            let _previous_value = self.sequences.insert(target.to_owned(), value);
            self
        }
    }

    impl ConditionView for TestView {
        fn state(&self, target: &str, name: &str) -> Option<StateValue> {
            self.states
                .get(&(target.to_owned(), name.to_owned()))
                .cloned()
        }

        fn enabled(&self, target: &str) -> Option<bool> {
            self.enabled.get(target).copied()
        }

        fn locked(&self, target: &str) -> Option<bool> {
            self.locked.get(target).copied()
        }

        fn door_open(&self, target: &str) -> Option<bool> {
            self.doors.get(target).copied()
        }

        fn sequence_running(&self, target: &str) -> Option<bool> {
            self.sequences.get(target).copied()
        }
    }

    fn record(generation: u64, kind: EventKind, index: u32, key: &str) -> EventRecord {
        EventRecord {
            generation,
            kind,
            subject: EntityHandle::from_parts(index, 0),
            key: key.to_owned(),
            actor: None,
            depth: 0,
        }
    }

    fn state(target: &str, name: &str, equals: StateValue) -> ConditionDef {
        ConditionDef::State {
            target: target.to_owned(),
            name: name.to_owned(),
            equals,
        }
    }

    #[test]
    fn queue_pops_in_push_order() {
        let mut queue = EventQueue::new();
        assert!(queue.is_empty());
        assert!(queue.peek().is_none());
        let mut first = record(1, EventKind::Interact, 0, "a");
        first.actor = Some(EntityHandle::from_parts(7, 0));
        assert!(queue.push(first));
        assert!(queue.push(record(1, EventKind::Timer, 1, "b")));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.peek().map(|item| item.key.as_str()), Some("a"));
        let popped = queue.pop().expect("first");
        assert_eq!(popped.kind, EventKind::Interact);
        assert_eq!(popped.subject.index(), 0);
        assert_eq!(popped.actor.map(EntityHandle::index), Some(7));
        let second = queue.pop().expect("second");
        assert_eq!(second.kind, EventKind::Timer);
        assert!(queue.pop().is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn a_full_queue_refuses_rather_than_grows() {
        let mut queue = EventQueue::new();
        for _ in 0..EventQueue::MAX_QUEUED_EVENTS {
            assert!(queue.push(record(1, EventKind::Timer, 0, "tick")));
        }
        assert_eq!(queue.len(), EventQueue::MAX_QUEUED_EVENTS);
        assert!(!queue.push(record(1, EventKind::Timer, 0, "overflow")));
        assert_eq!(queue.len(), EventQueue::MAX_QUEUED_EVENTS);
        assert_eq!(queue.dropped(), 1);
        // Popping one record frees exactly one slot.
        assert!(queue.pop().is_some());
        assert!(queue.push(record(1, EventKind::Timer, 0, "again")));
        assert_eq!(queue.dropped(), 1);
    }

    #[test]
    fn clear_resets_records_and_the_drop_counter() {
        let mut queue = EventQueue::new();
        for _ in 0..EventQueue::MAX_QUEUED_EVENTS {
            let _push_status = queue.push(record(1, EventKind::Interact, 0, "x"));
        }
        let _push_status_2 = queue.push(record(1, EventKind::Interact, 0, "x"));
        assert_eq!(queue.dropped(), 1);
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.dropped(), 0);
        assert!(queue.push(record(1, EventKind::Interact, 0, "fresh")));
    }

    #[test]
    fn drain_stale_keeps_only_the_current_generation_in_order() {
        let mut queue = EventQueue::new();
        let _push_status = queue.push(record(4, EventKind::Interact, 0, "old"));
        let _push_status_2 = queue.push(record(5, EventKind::Spawn, 1, "kept_a"));
        let _push_status_3 = queue.push(record(4, EventKind::Timer, 2, "old"));
        let _push_status_4 = queue.push(record(5, EventKind::ObjectState, 3, "kept_b"));
        assert_eq!(queue.drain_stale(5), 2);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop().map(|item| item.key), Some("kept_a".to_owned()));
        assert_eq!(queue.pop().map(|item| item.key), Some("kept_b".to_owned()));
        assert_eq!(queue.drain_stale(5), 0, "nothing stale remains");
        assert_eq!(queue.drain_stale(6), 0, "an empty queue drains nothing");
    }

    #[test]
    fn event_kind_names_round_trip() {
        let kinds = [
            EventKind::Interact,
            EventKind::EnterVolume,
            EventKind::ExitVolume,
            EventKind::Timer,
            EventKind::ObjectState,
            EventKind::SequenceComplete,
            EventKind::Spawn,
            EventKind::AnimationComplete,
            EventKind::AiState,
            EventKind::Caught,
        ];
        for kind in kinds {
            let name = kind.name();
            assert_eq!(EventKind::from_name(name), Some(kind), "{name} round-trips");
        }
        assert_eq!(EventKind::from_name("activate"), None);
        assert_eq!(
            EventKind::from_name("Interact"),
            None,
            "names are case sensitive"
        );
        assert_eq!(EventKind::from_name(""), None);
    }

    #[test]
    fn authored_event_kinds_map_to_their_runtime_mirror() {
        assert_eq!(
            EventKind::parse(EventKindName::Interact),
            EventKind::Interact
        );
        assert_eq!(
            EventKind::parse(EventKindName::EnterVolume),
            EventKind::EnterVolume
        );
        assert_eq!(
            EventKind::parse(EventKindName::ExitVolume),
            EventKind::ExitVolume
        );
        assert_eq!(EventKind::parse(EventKindName::Timer), EventKind::Timer);
        assert_eq!(
            EventKind::parse(EventKindName::ObjectState),
            EventKind::ObjectState
        );
        assert_eq!(
            EventKind::parse(EventKindName::SequenceComplete),
            EventKind::SequenceComplete
        );
        assert_eq!(EventKind::parse(EventKindName::Spawn), EventKind::Spawn);
        assert_eq!(
            EventKind::parse(EventKindName::AnimationComplete),
            EventKind::AnimationComplete
        );
        assert_eq!(EventKind::parse(EventKindName::AiState), EventKind::AiState);
        assert_eq!(EventKind::parse(EventKindName::Caught), EventKind::Caught);
    }

    #[test]
    fn state_conditions_compare_exactly_on_the_variant() {
        let view = TestView::default()
            .with_state("lamp", "on", StateValue::Bool(true))
            .with_state("counter", "count", StateValue::Int(1))
            .with_state("gauge", "level", StateValue::Float(1.0))
            .with_state("sign", "text", StateValue::Text("open".to_owned()));
        assert!(evaluate(
            &state("lamp", "on", StateValue::Bool(true)),
            &view
        ));
        assert!(!evaluate(
            &state("lamp", "on", StateValue::Bool(false)),
            &view
        ));
        assert!(evaluate(
            &state("counter", "count", StateValue::Int(1)),
            &view
        ));
        assert!(
            !evaluate(&state("counter", "count", StateValue::Float(1.0)), &view),
            "Int(1) is not Float(1.0): equality is exact on the variant"
        );
        assert!(evaluate(
            &state("gauge", "level", StateValue::Float(1.0)),
            &view
        ));
        assert!(
            !evaluate(&state("gauge", "level", StateValue::Int(1)), &view),
            "Float(1.0) is not Int(1)"
        );
        assert!(evaluate(
            &state("sign", "text", StateValue::Text("open".to_owned())),
            &view
        ));
        assert!(!evaluate(
            &state("sign", "text", StateValue::Text("closed".to_owned())),
            &view
        ));
        assert!(
            !evaluate(&state("sign", "text", StateValue::Bool(true)), &view),
            "a text state never equals a bool"
        );
        assert!(
            !evaluate(&state("lamp", "missing", StateValue::Bool(true)), &view),
            "a known entity with an unknown state is false"
        );
    }

    #[test]
    fn every_condition_is_false_for_an_unknown_target() {
        let view = TestView::default();
        assert!(!evaluate(
            &state("ghost", "on", StateValue::Bool(true)),
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::Enabled {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::Disabled {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::Locked {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::Unlocked {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::DoorOpen {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::DoorClosed {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::SequenceRunning {
                target: "ghost".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::SequenceIdle {
                target: "ghost".to_owned()
            },
            &view
        ));
    }

    #[test]
    fn flag_conditions_mirror_their_component() {
        let on = TestView::default()
            .with_enabled("switch", true)
            .with_locked("gate", true)
            .with_door("hatch", true)
            .with_sequence("monologue", true);
        let off = TestView::default()
            .with_enabled("switch", false)
            .with_locked("gate", false)
            .with_door("hatch", false)
            .with_sequence("monologue", false);

        let enabled = ConditionDef::Enabled {
            target: "switch".to_owned(),
        };
        let disabled = ConditionDef::Disabled {
            target: "switch".to_owned(),
        };
        assert!(evaluate(&enabled, &on));
        assert!(!evaluate(&disabled, &on));
        assert!(!evaluate(&enabled, &off));
        assert!(evaluate(&disabled, &off));

        let locked = ConditionDef::Locked {
            target: "gate".to_owned(),
        };
        let unlocked = ConditionDef::Unlocked {
            target: "gate".to_owned(),
        };
        assert!(evaluate(&locked, &on));
        assert!(!evaluate(&unlocked, &on));
        assert!(!evaluate(&locked, &off));
        assert!(evaluate(&unlocked, &off));

        let door_open = ConditionDef::DoorOpen {
            target: "hatch".to_owned(),
        };
        let door_closed = ConditionDef::DoorClosed {
            target: "hatch".to_owned(),
        };
        assert!(evaluate(&door_open, &on));
        assert!(!evaluate(&door_closed, &on));
        assert!(!evaluate(&door_open, &off));
        assert!(evaluate(&door_closed, &off));

        let running = ConditionDef::SequenceRunning {
            target: "monologue".to_owned(),
        };
        let idle = ConditionDef::SequenceIdle {
            target: "monologue".to_owned(),
        };
        assert!(evaluate(&running, &on));
        assert!(!evaluate(&idle, &on));
        assert!(!evaluate(&running, &off));
        assert!(evaluate(&idle, &off));
    }

    #[test]
    fn a_missing_flag_is_neither_true_nor_false() {
        // One entity that only has `enabled`; every other lookup is unknown.
        let view = TestView::default().with_enabled("switch", true);
        assert!(evaluate(
            &ConditionDef::Enabled {
                target: "switch".to_owned()
            },
            &view
        ));
        assert!(!evaluate(
            &ConditionDef::SequenceIdle {
                target: "switch".to_owned()
            },
            &view
        ));
    }
}

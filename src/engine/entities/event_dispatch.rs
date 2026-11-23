//! Bounded FIFO dispatch and in-flight terminal bindings.

use super::{
    DispatchReport, EntityWorld, EventRecord, MAX_CHAIN_DEPTH, MAX_EVENTS_PER_TICK, WorldTick,
    warn_once,
};
use crate::engine::level::{ActionDef, MAX_BINDINGS_PER_ENTITY};

/// The head event has left the FIFO. Its unused slot belongs to this dispatch
/// across a yield so a later arrival cannot take its first child's slot.
struct EventDispatch {
    event: EventRecord,
    // Owner-local positions survive compaction of other entities' bindings.
    matched: [Option<usize>; MAX_BINDINGS_PER_ENTITY],
    next_binding: usize,
    next_action: usize,
    child_slot: bool,
}

#[derive(Default)]
pub(super) struct TerminalDispatch {
    pending: Option<EventDispatch>,
    first: bool,
}

impl TerminalDispatch {
    pub(super) const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl EntityWorld {
    pub(super) fn clear_event_dispatch(&mut self) {
        self.events.clear();
        self.terminal_dispatch = TerminalDispatch::default();
    }

    /// Alternate who can spend the fresh budget first while a terminal binding
    /// and automatic modules are both able to produce work every frame.
    pub(super) fn schedule_pending_terminal_event(&mut self, tick: &mut WorldTick) {
        if self.terminal_dispatch.is_pending() {
            self.terminal_dispatch.first = !self.terminal_dispatch.first;
            if self.terminal_dispatch.first {
                self.resume_terminal_event(tick);
            }
        }
    }

    fn resume_terminal_event(&mut self, tick: &mut WorldTick) {
        let Some(mut dispatch) = self.terminal_dispatch.pending.take() else {
            return;
        };
        if !self.event_is_live(&dispatch.event) {
            self.events.hold_terminal_slot(false);
            return;
        }
        if tick.events_processed >= MAX_EVENTS_PER_TICK || !self.scripts.has_callback_budget() {
            self.terminal_dispatch.pending = Some(dispatch);
            return;
        }
        tick.events_processed = tick.events_processed.saturating_add(1);
        if !self.dispatch_event_bindings(&mut dispatch, tick, true) {
            self.terminal_dispatch.pending = Some(dispatch);
        }
    }

    pub(super) fn pump_events(&mut self, tick: &mut WorldTick) {
        self.resume_terminal_event(tick);
        while !self.terminal_dispatch.is_pending()
            && !tick.player_reset
            && tick.events_processed < MAX_EVENTS_PER_TICK
        {
            let Some((event, terminal)) = self.events.pop_dispatch() else {
                break;
            };
            tick.events_processed = tick.events_processed.saturating_add(1);
            if !self.event_is_live(&event) {
                continue;
            }
            if u32::from(event.depth) >= MAX_CHAIN_DEPTH {
                tick.events_dropped = tick.events_dropped.saturating_add(1);
                warn_once(
                    "event-chain-depth",
                    format!(
                        "[entities] event chain reached the depth limit ({MAX_CHAIN_DEPTH}); offending occurrence dropped"
                    ),
                );
                continue;
            }
            let mut dispatch = EventDispatch {
                matched: self.match_event_bindings(&event),
                event,
                next_binding: 0,
                next_action: 0,
                child_slot: terminal,
            };
            if !self.dispatch_event_bindings(&mut dispatch, tick, terminal) {
                self.terminal_dispatch.pending = Some(dispatch);
            }
        }
        if !tick.player_reset {
            self.compact_bindings();
        }
    }

    fn event_is_live(&self, event: &EventRecord) -> bool {
        event.generation == self.generation && self.store.contains(event.subject)
    }

    fn match_event_bindings(
        &self,
        event: &EventRecord,
    ) -> [Option<usize>; MAX_BINDINGS_PER_ENTITY] {
        // Snapshot conditions before any action can change them, including when
        // some of the matching bindings execute in a later frame.
        let mut matched = [None; MAX_BINDINGS_PER_ENTITY];
        let indices = self
            .bindings_by_owner
            .get(&event.subject)
            .into_iter()
            .flatten()
            .copied()
            .enumerate()
            .filter_map(|(owner_index, index)| {
                let binding = self.bindings.get(index)?;
                (binding.on == event.kind
                    && binding.key.as_ref().is_none_or(|key| key == &event.key)
                    && binding
                        .when
                        .iter()
                        .all(|condition| self.condition_holds(condition)))
                .then_some(owner_index)
            });
        for (slot, index) in matched.iter_mut().zip(indices) {
            *slot = Some(index);
        }
        matched
    }

    fn dispatch_event_bindings(
        &mut self,
        dispatch: &mut EventDispatch,
        tick: &mut WorldTick,
        terminal: bool,
    ) -> bool {
        if !self.scripts.is_empty() {
            self.script_event = Some(dispatch.event.clone());
        }
        let queued_before = self.events.len();
        if terminal {
            self.events.hold_terminal_slot(false);
        }
        let complete = self.advance_event_bindings(dispatch, tick, terminal);
        if terminal {
            dispatch.child_slot &= self.events.len() == queued_before;
            self.events
                .hold_terminal_slot(!complete && dispatch.child_slot);
        }
        self.script_event = None;
        complete
    }

    fn advance_event_bindings(
        &mut self,
        dispatch: &mut EventDispatch,
        tick: &mut WorldTick,
        terminal: bool,
    ) -> bool {
        while let Some(Some(owner_index)) = dispatch.matched.get(dispatch.next_binding).copied() {
            let Some((index, binding)) = self
                .bindings_by_owner
                .get(&dispatch.event.subject)
                .and_then(|indices| indices.get(owner_index))
                .and_then(|index| {
                    self.bindings
                        .get(*index)
                        .cloned()
                        .map(|binding| (*index, binding))
                })
            else {
                dispatch.next_binding = dispatch.next_binding.saturating_add(1);
                dispatch.next_action = 0;
                continue;
            };
            if dispatch.next_action == 0
                && ((binding.fired && binding.once) || binding.cooldown_remaining > 0.0)
            {
                dispatch.next_binding = dispatch.next_binding.saturating_add(1);
                continue;
            }
            let mut report = DispatchReport::default();
            let mut binding_completed = true;
            for action in binding.actions.iter().skip(dispatch.next_action) {
                if let ActionDef::RunScript { script, handler: _ } = action
                    && self.scripts.defer_linked_call(script, terminal)
                {
                    if terminal {
                        tick.record_dispatch(&report);
                        return false;
                    }
                    binding_completed = false;
                    continue;
                }
                self.run_action(
                    action,
                    Some(dispatch.event.subject),
                    dispatch.event.origin,
                    &mut report,
                    dispatch.event.depth,
                );
                dispatch.next_action = dispatch.next_action.saturating_add(1);
                if report.player_reset {
                    tick.record_dispatch(&report);
                    return true;
                }
            }
            tick.record_dispatch(&report);
            if binding_completed && let Some(entry) = self.bindings.get_mut(index) {
                entry.fired = true;
                entry.cooldown_remaining = binding.cooldown_seconds;
                if binding.cooldown_seconds > 0.0 {
                    self.cooling_bindings.push(index);
                }
            }
            dispatch.next_binding = dispatch.next_binding.saturating_add(1);
            dispatch.next_action = 0;
        }
        true
    }
}

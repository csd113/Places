//! Data-driven sequences: ordered steps that share the ordinary action
//! pipeline.
//!
//! A sequence is an authored resource (`id`, `looped`, `steps`), not a second
//! scripting language: every step is either an ordinary [`ActionDef`] or a
//! bounded primitive the action set cannot express on its own (a wait, a
//! collision-respecting move, a turn, an animation wait, an event emission, a
//! typed state write, or a stop).
//!
//! Ownership is explicit: a sequence always runs *on* one entity. Starting a
//! second sequence on an entity replaces the first (the owner is the
//! controller), despawning the owner cancels it, and a map unload drops every
//! runtime. Completion emits `sequence_complete` exactly once, carrying the
//! sequence id as the event key, so a binding can chain the next phase without
//! guessing.
//!
//! The step machine lives in [`crate::entities::EntityWorld`], which owns the
//! collision world, the animation state and the action dispatcher; this module
//! owns the authored definitions and the per-owner runtime state.

use serde::{Deserialize, Serialize};

use super::id::EntityHandle;
use crate::level::{ActionDef, EventKindName};

/// One authored sequence.
///
/// ```json
/// { "id": "sauna_warmup", "steps": [
///     { "step": "set_state", "name": "phase", "value": "heating" },
///     { "step": "wait", "seconds": 2.0 },
///     { "step": "emit", "on": "timer", "key": "warm" },
///     { "step": "wait_animation", "clip": "hisss", "timeout": 3.0 },
///     { "step": "set_state", "name": "phase", "value": "ready" }
/// ] }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SequenceDef {
    /// Stable sequence id, unique per level; `start_sequence` names it.
    pub id: String,
    /// Restart from step 0 after the last step instead of completing.
    #[serde(default)]
    pub looped: bool,
    /// The steps, in order. Must not be empty.
    pub steps: Vec<SequenceStepDef>,
}

/// One step of a sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum SequenceStepDef {
    /// Run one ordinary action, then advance.
    Action {
        /// The action to run.
        action: ActionDef,
    },
    /// Wait for simulation time, then advance.
    Wait {
        /// Seconds to wait; must be finite and non-negative.
        seconds: f32,
    },
    /// Walk to a world position, respecting collision and walkable floors.
    Move {
        /// Target world X.
        x: f32,
        /// Target world Y; omitted keeps the current floor height.
        #[serde(default)]
        y: Option<f32>,
        /// Target world Z.
        z: f32,
        /// Speed in m/s; must be finite and positive.
        speed: f32,
    },
    /// Turn to a yaw at the entity's turn rate.
    Face {
        /// Target yaw in degrees.
        yaw_degrees: f32,
    },
    /// Wait for a named clip to complete, or for `timeout` seconds.
    ///
    /// A missing clip is a validation error, so a shipped map cannot strand a
    /// sequence on an animation that can never finish; `timeout` bounds a clip
    /// that never reports completion.
    WaitAnimation {
        /// Clip name to wait for; omitted waits for whatever is playing.
        #[serde(default)]
        clip: Option<String>,
        /// Seconds after which the wait completes regardless.
        #[serde(default)]
        timeout: f32,
    },
    /// Emit an event from the sequence's entity, then advance.
    Emit {
        /// Event kind to emit.
        on: EventKindName,
        /// Optional event key.
        #[serde(default)]
        key: Option<String>,
    },
    /// Write a typed state value, then advance.
    SetState {
        /// State name.
        name: String,
        /// New value.
        value: crate::entities::components::StateValue,
    },
    /// End the sequence here, as if it completed.
    Stop,
}

impl SequenceStepDef {
    /// Stable diagnostic name of this step kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Action { action: _ } => "action",
            Self::Wait { seconds: _ } => "wait",
            Self::Move {
                x: _,
                y: _,
                z: _,
                speed: _,
            } => "move",
            Self::Face { yaw_degrees: _ } => "face",
            Self::WaitAnimation {
                clip: _,
                timeout: _,
            } => "wait_animation",
            Self::Emit { on: _, key: _ } => "emit",
            Self::SetState { name: _, value: _ } => "set_state",
            Self::Stop => "stop",
        }
    }

    /// True when this step completes only after simulation time passes (or an
    /// external completion arrives). A chain of steps that are all immediate
    /// can loop forever in one tick if it restarts; the compiler uses this to
    /// reject zero-delay cycles.
    #[must_use]
    pub const fn is_delayed(&self) -> bool {
        matches!(
            self,
            Self::Wait { seconds: _ }
                | Self::Move {
                    x: _,
                    y: _,
                    z: _,
                    speed: _
                }
                | Self::Face { yaw_degrees: _ }
                | Self::WaitAnimation {
                    clip: _,
                    timeout: _
                }
        )
    }

    /// Every action this step can run, for target validation.
    #[must_use]
    pub fn actions(&self) -> Vec<&ActionDef> {
        match self {
            Self::Action { action } => vec![action],
            Self::Wait { seconds: _ }
            | Self::Move {
                x: _,
                y: _,
                z: _,
                speed: _,
            }
            | Self::Face { yaw_degrees: _ }
            | Self::WaitAnimation {
                clip: _,
                timeout: _,
            }
            | Self::Emit { on: _, key: _ }
            | Self::SetState { name: _, value: _ }
            | Self::Stop => Vec::new(),
        }
    }
}

/// One sequence's runtime state, owned by the entity running it.
// The flags are independent state-machine facts (which step effect ran, which
// step waiter was satisfied), not interchangeable booleans.
#[expect(
    clippy::struct_excessive_bools,
    reason = "The flags are independent state-machine facts (which step effect ran, which step waiter was satisfied), not interchangeable booleans."
)]
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceRuntime {
    /// Id of the running sequence.
    pub sequence: String,
    /// The entity the sequence runs on.
    pub owner: EntityHandle,
    /// Index of the active step.
    pub step: usize,
    /// Seconds spent in the active step.
    pub step_time: f32,
    /// True when the active step has run its one-shot effect (an action, an
    /// emit or a state write) and is waiting to advance.
    pub step_ran: bool,
    /// The active `wait_animation` step observed its clip complete.
    pub animation_complete: bool,
    /// True when the authored sequence loops.
    pub looped: bool,
    /// Completed and waiting to be reaped this tick.
    pub finished: bool,
    /// Owned by a `stop_sequence` request or a cancelled owner.
    pub stopped: bool,
}

impl SequenceRuntime {
    /// Starts a fresh run of `def` on `owner`.
    #[must_use]
    pub fn new(def: &SequenceDef, owner: EntityHandle) -> Self {
        Self {
            sequence: def.id.clone(),
            owner,
            step: 0,
            step_time: 0.0,
            step_ran: false,
            animation_complete: false,
            looped: def.looped,
            finished: false,
            stopped: false,
        }
    }

    /// The active step of `def`, if the run is still inside it.
    #[must_use]
    pub fn current<'a>(&self, def: &'a SequenceDef) -> Option<&'a SequenceStepDef> {
        if self.finished || self.stopped {
            return None;
        }
        def.steps.get(self.step)
    }

    /// Advances to the next step; returns false when the sequence completed.
    ///
    /// A looping sequence wraps to step 0 (the only way a sequence runs
    /// forever); a non-looping one finishes.
    pub const fn advance(&mut self, def: &SequenceDef) -> bool {
        self.step = self.step.saturating_add(1);
        self.step_time = 0.0;
        self.step_ran = false;
        self.animation_complete = false;
        if self.step < def.steps.len() {
            return true;
        }
        if self.looped && !def.steps.is_empty() {
            self.step = 0;
            return true;
        }
        self.finished = true;
        false
    }

    /// Marks the run stopped by an explicit request.
    pub const fn stop(&mut self) {
        self.stopped = true;
    }
}

/// Every sequence authored by one level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sequences {
    definitions: Vec<SequenceDef>,
}

impl Sequences {
    /// An empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    /// Resolves the level's authored sequences.
    #[must_use]
    pub fn from_level(level: &crate::level::LevelDef) -> Self {
        Self {
            definitions: level
                .sequences
                .iter()
                .filter(|def| !def.id.trim().is_empty() && !def.steps.is_empty())
                .cloned()
                .collect(),
        }
    }

    /// Number of sequences.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.definitions.len()
    }

    /// True when the level authors none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// One sequence by authored id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SequenceDef> {
        self.definitions.iter().find(|def| def.id == id.trim())
    }

    /// Every sequence, in authored order.
    #[must_use]
    pub fn defs(&self) -> &[SequenceDef] {
        &self.definitions
    }
}

/// The maximum sequences one level may author.
///
/// Raised to 1024 from 256: sequences are authored records advanced by the
/// entity runtime; how many *run at once* is bounded separately by
/// [`MAX_ACTIVE_SEQUENCES`], so a large library of sequences is an authoring
/// convenience, not per-frame work.
pub const MAX_LEVEL_SEQUENCES: usize = 1024;

/// The maximum steps one sequence may declare.
///
/// Raised to 128 from 64: a step is one small record, and a sequence is
/// advanced by walking its step list, so the count is an authoring bound.
pub const MAX_SEQUENCE_STEPS: usize = 128;

/// The maximum sequences running at once across a level.
pub const MAX_ACTIVE_SEQUENCES: usize = 64;

/// Largest uncompressed `wait` a sequence step may author, in seconds.
pub const MAX_SEQUENCE_WAIT_S: f32 = 600.0;

/// Largest `wait_animation` timeout a sequence step may author, in seconds.
pub const MAX_SEQUENCE_ANIMATION_TIMEOUT_S: f32 = 120.0;

/// Terminal distance of a sequence `move` step, in metres.
pub const SEQUENCE_ARRIVE_EPS_M: f32 = 0.02;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::level::EventKindName;

    fn handle(index: u32) -> EntityHandle {
        EntityHandle::from_parts(index, 1)
    }

    fn def(id: &str, looped: bool, steps: Vec<SequenceStepDef>) -> SequenceDef {
        SequenceDef {
            id: id.into(),
            looped,
            steps,
        }
    }

    #[test]
    fn a_sequence_walks_its_steps_and_completes() {
        let def = def(
            "test",
            false,
            vec![
                SequenceStepDef::Wait { seconds: 0.5 },
                SequenceStepDef::Emit {
                    on: EventKindName::Timer,
                    key: Some("tick".into()),
                },
            ],
        );
        let mut runtime = SequenceRuntime::new(&def, handle(0));
        assert_eq!(
            runtime.current(&def).map(SequenceStepDef::kind),
            Some("wait")
        );
        assert!(runtime.advance(&def));
        assert_eq!(
            runtime.current(&def).map(SequenceStepDef::kind),
            Some("emit")
        );
        assert!(!runtime.advance(&def), "the last step completes the run");
        assert!(runtime.finished);
        assert!(runtime.current(&def).is_none());
    }

    #[test]
    fn a_looping_sequence_restarts_at_step_zero() {
        let def = def(
            "loop",
            true,
            vec![
                SequenceStepDef::Wait { seconds: 1.0 },
                SequenceStepDef::Stop,
            ],
        );
        let mut runtime = SequenceRuntime::new(&def, handle(0));
        assert!(runtime.advance(&def));
        assert!(runtime.advance(&def), "a looping run never completes");
        assert_eq!(runtime.step, 0);
        assert!(!runtime.finished);
    }

    #[test]
    fn stopping_a_run_ends_it_without_completing() {
        let def = def("stop", false, vec![SequenceStepDef::Wait { seconds: 10.0 }]);
        let mut runtime = SequenceRuntime::new(&def, handle(0));
        runtime.stop();
        assert!(runtime.stopped);
        assert!(runtime.current(&def).is_none());
    }

    #[test]
    fn only_delayed_steps_are_classified_as_delayed() {
        assert!(SequenceStepDef::Wait { seconds: 1.0 }.is_delayed());
        assert!(
            SequenceStepDef::Move {
                x: 0.0,
                y: None,
                z: 0.0,
                speed: 1.0
            }
            .is_delayed()
        );
        assert!(
            !SequenceStepDef::SetState {
                name: "x".into(),
                value: crate::entities::components::StateValue::Bool(true)
            }
            .is_delayed()
        );
    }
}

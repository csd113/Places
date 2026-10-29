//! Simulation-time timers.
//!
//! A timer is authored with an id, a positive period in seconds, a repeat flag
//! and an autostart flag. The runtime keeps one [`TimerRuntime`] per authored
//! timer and advances them only with simulation time: [`Timers::tick`]
//! subtracts the frame delta, fires due timers in authored order and returns how
//! many fired. A non-finite or negative delta does nothing, so a broken frame
//! can never fire every timer at once.
//!
//! Each timer fires at most once per tick. A one-shot timer stops after its
//! single fire; a repeating timer re-arms at its full effective period, so a
//! long frame cannot burst-fire a queue of overdue ticks — the next fire is
//! always a full period after the previous one.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::engine::level::LevelDef;

/// One authored timer's resolved definition.
///
/// `seconds` is required and must be finite and positive; [`Timers::from_level`]
/// skips a definition that is not (a loaded level is validated, so the skip is
/// a defensive second line, never a silent map edit).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerDef {
    /// Authored timer id, unique per level.
    pub id: String,
    /// Seconds from a start (or the previous fire) to the fire.
    pub seconds: f32,
    /// True re-arms after each fire instead of stopping.
    #[serde(default)]
    pub repeat: bool,
    /// True starts counting as soon as the world loads, and on every reset.
    #[serde(default)]
    pub autostart: bool,
    /// What the timer's fires do.
    #[serde(default)]
    pub bindings: Vec<crate::engine::level::EventBindingDef>,
}

/// One timer's runtime state.
#[derive(Clone, Debug, PartialEq)]
pub struct TimerRuntime {
    /// The authored definition.
    ///
    /// [`Timers::start`] overrides the effective period and repeat flag for the
    /// current and next arms without rewriting this; [`Timers::reset`] restores
    /// the authored values.
    pub def: TimerDef,
    /// Seconds left before the next fire.
    pub remaining: f32,
    /// True while the timer is counting down.
    pub running: bool,
    /// Fires since the last reset.
    pub fires: u64,
    /// Effective period in seconds for the current and next arm.
    period: f32,
    /// Effective repeat flag for the current and next arm.
    repeat: bool,
}

impl TimerRuntime {
    /// The effective period in seconds: the authored `seconds` unless
    /// [`Timers::start`] overrode it for this arm.
    #[must_use]
    pub const fn period(&self) -> f32 {
        self.period
    }

    /// The effective repeat flag: the authored `repeat` unless
    /// [`Timers::start`] overrode it for this arm.
    #[must_use]
    pub const fn repeat(&self) -> bool {
        self.repeat
    }
}

/// Every timer of one world, in authored order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timers {
    /// One runtime per usable authored definition, in authored order.
    timers: Vec<TimerRuntime>,
    /// Authored id -> index into `timers`.
    index: HashMap<String, usize>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    id: String,
    remaining: f32,
    running: bool,
    fires: u64,
    period: f32,
    repeat: bool,
}
impl Timers {
    #[must_use]
    pub fn checkpoint(&self) -> Vec<Checkpoint> {
        self.iter()
            .map(|timer| Checkpoint {
                id: timer.def.id.clone(),
                remaining: timer.remaining,
                running: timer.running,
                fires: timer.fires,
                period: timer.period,
                repeat: timer.repeat,
            })
            .collect()
    }
    /// # Errors
    /// Rejects mismatched IDs and invalid countdowns before the private world commits.
    pub fn restore_checkpoint(&mut self, records: &[Checkpoint]) -> Result<(), String> {
        if records.len() != self.len() {
            return Err("Checkpoint timer count mismatch".into());
        }
        for (timer, record) in self.timers.iter_mut().zip(records) {
            super::checkpoint::number(record.period, 10_000_000.0)?;
            super::checkpoint::number(record.remaining, record.period)?;
            if timer.def.id != record.id || record.period <= 0.0 || record.remaining < 0.0 {
                return Err("Invalid checkpoint timer".into());
            }
            timer.remaining = record.remaining;
            timer.running = record.running;
            timer.fires = record.fires;
            timer.period = record.period;
            timer.repeat = record.repeat;
        }
        Ok(())
    }
}

impl Timers {
    /// An empty timer set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the runtime state for a level's authored timers.
    ///
    /// A definition with a non-finite or non-positive `seconds` is skipped, and
    /// a duplicate id keeps its first definition: validation rejects both
    /// before load, so this only keeps a hand-built level from arming a timer
    /// that can never behave.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let mut timers = Self::new();
        for authored in &level.timers {
            timers.push_authored(TimerDef {
                id: authored.id.clone(),
                seconds: authored.seconds,
                repeat: authored.repeat,
                autostart: authored.autostart,
                bindings: authored.bindings.clone(),
            });
        }
        timers
    }

    /// Adds one authored definition, skipping unusable ones.
    fn push_authored(&mut self, def: TimerDef) {
        if !def.seconds.is_finite() || def.seconds <= 0.0 || self.index.contains_key(&def.id) {
            return;
        }
        let period = def.seconds;
        let repeat = def.repeat;
        let autostart = def.autostart;
        let index = self.timers.len();
        let _previous_value = self.index.insert(def.id.clone(), index);
        self.timers.push(TimerRuntime {
            def,
            remaining: period,
            running: autostart,
            fires: 0,
            period,
            repeat,
        });
    }

    /// Number of timers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.timers.len()
    }

    /// True when the world authored no usable timer.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    /// The timer with authored id `id`, if any.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&TimerRuntime> {
        self.index_of(id).and_then(|index| self.at(index))
    }

    /// Every timer, in authored order.
    pub fn iter(&self) -> std::slice::Iter<'_, TimerRuntime> {
        self.timers.iter()
    }

    /// The index of `id` into [`Self::iter`], if the id is known.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// The timer at `index` in authored order, if it exists.
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&TimerRuntime> {
        self.timers.get(index)
    }

    /// Arms `id` with an optional period and repeat override.
    ///
    /// Returns `false` without changing anything for an unknown id or a period
    /// that is not finite and positive. `seconds: None` keeps the authored
    /// period, `repeat: None` keeps the authored repeat flag; a running timer
    /// restarts from a full period.
    pub fn start(&mut self, id: &str, seconds: Option<f32>, repeat: Option<bool>) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let Some(timer) = self.timers.get_mut(index) else {
            return false;
        };
        let period = seconds.unwrap_or(timer.def.seconds);
        if !period.is_finite() || period <= 0.0 {
            return false;
        }
        timer.period = period;
        if let Some(repeat_mode) = repeat {
            timer.repeat = repeat_mode;
        }
        timer.remaining = period;
        timer.running = true;
        true
    }

    /// Stops `id`, returning whether it was running.
    ///
    /// The remaining time is kept for inspection; [`Self::start`] re-arms from
    /// a full period and [`Self::reset`] restores the authored state.
    pub fn stop(&mut self, id: &str) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let Some(timer) = self.timers.get_mut(index) else {
            return false;
        };
        let was_running = timer.running;
        timer.running = false;
        was_running
    }

    /// Restores every timer to its authored autostart state.
    ///
    /// Authored period and repeat flag, `remaining` at the authored period,
    /// running exactly when `autostart` is set, and the fire counter cleared:
    /// the state a freshly loaded world would hold.
    pub fn reset(&mut self) {
        for timer in &mut self.timers {
            timer.period = timer.def.seconds;
            timer.repeat = timer.def.repeat;
            timer.remaining = timer.def.seconds;
            timer.running = timer.def.autostart;
            timer.fires = 0;
        }
    }

    /// Advances every running timer by `delta_seconds` of simulation time and
    /// calls `fire(index)` for each timer that reaches zero.
    ///
    /// The callback runs after the timer's state has been updated and receives
    /// the index from [`Self::iter`]/[`Self::at`], so a tick that fires several
    /// timers reports them in authored order, deterministically. A repeating
    /// timer re-arms at its full effective period; a one-shot timer stops at
    /// zero; each timer fires at most once per tick. Returns the number of
    /// fires. A non-finite or negative delta does nothing.
    pub fn tick(&mut self, delta_seconds: f32, mut fire: impl FnMut(usize)) -> usize {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return 0;
        }
        let mut fires = 0_usize;
        for index in 0..self.timers.len() {
            let Some(timer) = self.timers.get_mut(index) else {
                continue;
            };
            if !timer.running {
                continue;
            }
            timer.remaining -= delta_seconds;
            if timer.remaining > 0.0 {
                continue;
            }
            timer.fires = timer.fires.saturating_add(1);
            if timer.repeat {
                timer.remaining = timer.period;
            } else {
                timer.remaining = 0.0;
                timer.running = false;
            }
            fire(index);
            fires = fires.saturating_add(1);
        }
        fires
    }
}

impl<'a> IntoIterator for &'a Timers {
    type Item = &'a TimerRuntime;
    type IntoIter = std::slice::Iter<'a, TimerRuntime>;

    fn into_iter(self) -> Self::IntoIter {
        self.timers.iter()
    }
}

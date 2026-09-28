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

use crate::level::LevelDef;

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
    pub bindings: Vec<crate::level::EventBindingDef>,
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
        self.index.insert(def.id.clone(), index);
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
        if let Some(repeat) = repeat {
            timer.repeat = repeat;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::float_cmp, clippy::indexing_slicing)]

    use super::*;

    fn def(id: &str, seconds: f32) -> TimerDef {
        TimerDef {
            id: id.to_owned(),
            seconds,
            repeat: false,
            autostart: false,
            bindings: Vec::new(),
        }
    }

    fn autostart(id: &str, seconds: f32, repeat: bool) -> TimerDef {
        TimerDef {
            repeat,
            autostart: true,
            ..def(id, seconds)
        }
    }

    #[test]
    fn a_one_shot_timer_fires_exactly_once_after_its_full_period() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("gate", 2.0, false));
        let mut fired = Vec::new();
        assert_eq!(timers.tick(1.0, |index| fired.push(index)), 0);
        assert_eq!(timers.tick(0.5, |index| fired.push(index)), 0);
        assert!(fired.is_empty());
        assert_eq!(timers.tick(0.5, |index| fired.push(index)), 1);
        assert_eq!(fired, vec![0]);
        let timer = timers.get("gate").expect("gate");
        assert!(!timer.running);
        assert_eq!(timer.remaining, 0.0);
        assert_eq!(timer.fires, 1);
        assert_eq!(timers.tick(100.0, |index| fired.push(index)), 0);
        assert_eq!(timers.get("gate").expect("gate").fires, 1);
    }

    #[test]
    fn a_large_delta_still_fires_a_one_shot_only_once() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("gate", 2.0, false));
        let mut fired = Vec::new();
        assert_eq!(timers.tick(60.0, |index| fired.push(index)), 1);
        assert_eq!(fired, vec![0]);
        let timer = timers.get("gate").expect("gate");
        assert!(!timer.running);
        assert_eq!(timer.remaining, 0.0);
        assert_eq!(timer.fires, 1);
    }

    #[test]
    fn a_repeating_timer_re_arms_and_fires_again() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("pulse", 1.0, true));
        let mut fired = Vec::new();
        assert_eq!(timers.tick(1.0, |index| fired.push(index)), 1);
        let timer = timers.get("pulse").expect("pulse");
        assert!(timer.running);
        assert_eq!(timer.remaining, 1.0);
        assert_eq!(timer.fires, 1);
        assert_eq!(timers.tick(0.75, |index| fired.push(index)), 0);
        assert_eq!(timers.tick(0.25, |index| fired.push(index)), 1);
        assert_eq!(fired, vec![0, 0]);
        assert_eq!(timers.get("pulse").expect("pulse").fires, 2);
    }

    #[test]
    fn a_large_delta_does_not_burst_fire_a_repeating_timer() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("pulse", 1.0, true));
        let mut fired = Vec::new();
        assert_eq!(timers.tick(10.0, |index| fired.push(index)), 1);
        assert_eq!(fired, vec![0]);
        let timer = timers.get("pulse").expect("pulse");
        assert!(timer.running);
        assert_eq!(timer.remaining, 1.0);
        assert_eq!(timer.fires, 1);
    }

    #[test]
    fn start_overrides_period_and_repeat() {
        let mut timers = Timers::new();
        timers.push_authored(def("door", 5.0));
        assert!(!timers.start("missing", None, None));
        assert!(
            !timers.start("door", Some(0.0), None),
            "zero period refused"
        );
        assert!(
            !timers.start("door", Some(f32::NAN), None),
            "NaN period refused"
        );
        assert!(!timers.start("door", Some(-2.0), None));
        assert!(!timers.start("door", Some(f32::INFINITY), None));
        assert!(
            !timers.get("door").expect("door").running,
            "refused starts change nothing"
        );
        assert!(timers.start("door", Some(0.5), Some(true)));
        let timer = timers.get("door").expect("door");
        assert!(timer.running);
        assert_eq!(timer.remaining, 0.5);
        assert_eq!(timer.period(), 0.5);
        assert!(timer.repeat());
        assert_eq!(timers.tick(0.5, |_| {}), 1);
        let timer = timers.get("door").expect("door");
        assert!(timer.running, "the repeat override re-arms it");
        assert_eq!(timer.remaining, 0.5);
    }

    #[test]
    fn stop_reports_whether_it_was_running_and_reset_restores_autostart() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("auto", 3.0, true));
        timers.push_authored(def("manual", 1.0));
        assert!(!timers.stop("missing"));
        assert!(timers.stop("auto"));
        assert!(!timers.stop("auto"), "already stopped");
        assert!(!timers.stop("manual"), "never started");
        assert_eq!(timers.tick(10.0, |_| {}), 0, "both are stopped");
        timers.reset();
        let auto = timers.get("auto").expect("auto");
        assert!(auto.running);
        assert_eq!(auto.remaining, 3.0);
        assert_eq!(auto.fires, 0);
        assert_eq!(auto.period(), 3.0);
        let manual = timers.get("manual").expect("manual");
        assert!(!manual.running);
        assert_eq!(manual.remaining, 1.0);
    }

    #[test]
    fn reset_discards_a_manual_override() {
        let mut timers = Timers::new();
        timers.push_authored(def("door", 5.0));
        assert!(timers.start("door", Some(0.25), Some(true)));
        timers.tick(0.25, |_| {});
        assert_eq!(timers.get("door").expect("door").fires, 1);
        timers.reset();
        let timer = timers.get("door").expect("door");
        assert!(!timer.running);
        assert_eq!(timer.remaining, 5.0);
        assert_eq!(timer.period(), 5.0);
        assert!(!timer.repeat());
    }

    #[test]
    fn a_non_finite_or_negative_delta_is_ignored() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("t", 1.0, false));
        assert_eq!(timers.tick(f32::NAN, |_| {}), 0);
        assert_eq!(timers.tick(f32::INFINITY, |_| {}), 0);
        assert_eq!(timers.tick(f32::NEG_INFINITY, |_| {}), 0);
        assert_eq!(timers.tick(-1.0, |_| {}), 0);
        let timer = timers.get("t").expect("t");
        assert!(timer.running);
        assert_eq!(timer.remaining, 1.0);
        assert_eq!(timer.fires, 0);
    }

    #[test]
    fn a_zero_delta_changes_nothing() {
        let mut timers = Timers::new();
        timers.push_authored(autostart("t", 1.0, false));
        assert_eq!(timers.tick(0.0, |_| {}), 0);
        assert_eq!(timers.get("t").expect("t").remaining, 1.0);
    }

    #[test]
    fn due_timers_fire_in_index_order() {
        let mut timers = Timers::new();
        timers.push_authored(def("a", 1.0));
        timers.push_authored(def("b", 1.0));
        timers.push_authored(def("c", 1.0));
        assert!(timers.start("a", None, None));
        assert!(timers.start("b", None, None));
        assert!(timers.start("c", None, None));
        let mut order = Vec::new();
        assert_eq!(timers.tick(1.0, |index| order.push(index)), 3);
        assert_eq!(order, vec![0, 1, 2]);
        assert_eq!(timers.index_of("b"), Some(1));
        assert_eq!(timers.index_of("missing"), None);
    }

    #[test]
    fn degenerate_definitions_never_arm() {
        let mut timers = Timers::new();
        timers.push_authored(def("nan", f32::NAN));
        timers.push_authored(def("infinite", f32::INFINITY));
        timers.push_authored(def("zero", 0.0));
        timers.push_authored(def("negative", -1.0));
        timers.push_authored(def("good", 1.0));
        timers.push_authored(def("good", 9.0));
        assert_eq!(timers.len(), 1);
        assert!(timers.get("good").is_some());
        assert_eq!(timers.get("good").expect("good").def.seconds, 1.0);
    }

    #[test]
    fn from_level_resolves_authored_timers_and_drops_degenerate_ones() {
        let level: LevelDef = serde_json::from_str(
            r#"{
                "format_version": 3,
                "id": "timers",
                "name": "Timers",
                "spawn": { "x": 0.0, "z": 0.0 },
                "timers": [
                    { "id": "auto_pulse", "seconds": 2.5, "repeat": true, "autostart": true },
                    { "id": "manual", "seconds": 4.0 },
                    { "id": "zero", "seconds": 0.0 },
                    { "id": "negative", "seconds": -3.0 },
                    { "id": "duplicate", "seconds": 1.0 },
                    { "id": "duplicate", "seconds": 9.0 }
                ]
            }"#,
        )
        .expect("level json parses");
        let timers = Timers::from_level(&level);
        assert_eq!(timers.len(), 3);
        let ids: Vec<&str> = timers.iter().map(|timer| timer.def.id.as_str()).collect();
        assert_eq!(ids, vec!["auto_pulse", "manual", "duplicate"]);
        let auto = timers.get("auto_pulse").expect("auto_pulse");
        assert!(auto.running);
        assert_eq!(auto.remaining, 2.5);
        assert_eq!(auto.period(), 2.5);
        assert!(auto.repeat());
        let manual = timers.get("manual").expect("manual");
        assert!(!manual.running, "autostart defaults to false");
        assert_eq!(manual.remaining, 4.0);
        assert_eq!(timers.get("zero"), None);
        assert_eq!(timers.get("negative"), None);
        assert_eq!(timers.at(99), None);
        assert_eq!(
            timers.at(2).expect("duplicate").def.seconds,
            1.0,
            "first wins"
        );
    }

    #[test]
    fn an_empty_world_has_no_timers() {
        let timers = Timers::new();
        assert!(timers.is_empty());
        assert_eq!(timers.len(), 0);
        assert_eq!(timers.iter().count(), 0);
        assert_eq!(timers.get("anything"), None);
        assert_eq!(timers.index_of("anything"), None);
        assert_eq!(timers.at(0), None);
    }
}

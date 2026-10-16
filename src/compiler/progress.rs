//! Opt-in compiler feedback. Engine preparation and tests pay only a disabled
//! atomic check; active workers publish counts, never terminal output.

use std::fmt::Write;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

static ACTIVE: Mutex<Option<Arc<State>>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
const TICK: Duration = Duration::from_millis(100);
const PLAIN_INTERVAL: Duration = Duration::from_secs(10);

/// CLI progress policy; all modes preserve stdout and diagnostic messages.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Redraw on a capable stderr terminal, otherwise emit plain stage lines.
    #[default]
    Auto,
    /// Emit plain stage lines and sparse work updates.
    Plain,
    /// Suppress progress, retaining errors and opt-in telemetry.
    Off,
}

struct Current {
    source: String,
    variant: String,
    stage: String,
    unit: &'static str,
    started: Instant,
    work: Option<Arc<Counter>>,
    last_plain: Instant,
    last_count: usize,
    last_interrupted: bool,
}

struct State {
    current: Mutex<Option<Current>>,
    cancelled: Arc<AtomicBool>,
    stopped: AtomicBool,
    mode: Mode,
    terminal: bool,
    observed: bool,
}

/// A point-in-time view of the current compiler pass, without terminal formatting.
pub struct Snapshot {
    /// Current lighting quality, or empty during shared preparation/publication.
    pub variant: String,
    /// The operation currently running.
    pub stage: String,
    /// Time since this source started compiling.
    pub elapsed: Duration,
    /// Completed work and its real denominator; absent for uncounted operations.
    pub work: Option<WorkCount>,
}

/// Completed work in one pass, never an estimate of whole-build progress.
pub struct WorkCount {
    /// Finished items, clamped to the pass total.
    pub completed: usize,
    /// Number of items in this pass.
    pub total: usize,
    /// Human-readable item name.
    pub unit: &'static str,
}

impl Current {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            variant: self.variant.clone(),
            stage: self.stage.clone(),
            elapsed: self.started.elapsed(),
            work: self.work.as_ref().map(|counter| WorkCount {
                completed: counter.done.load(Ordering::Relaxed).min(counter.total),
                total: counter.total,
                unit: counter.unit,
            }),
        }
    }
}

/// A CLI-owned session. Dropping it clears the line and joins its display job.
/// It never hides the cursor or changes terminal modes.
pub struct Session {
    state: Arc<State>,
    display: Option<JoinHandle<()>>,
}

impl Session {
    /// Starts feedback and installs cooperative Ctrl-C handling, once per CLI.
    /// Call after pinning process-wide environment, before starting a build.
    ///
    /// # Errors
    /// Returns an error if another session exists, signal handling cannot be
    /// installed, or the display thread cannot be started.
    pub fn start(mode: Mode) -> Result<Self, String> {
        Self::start_inner(mode, false)
    }

    /// Starts a session polled by a GUI, with no terminal display thread.
    ///
    /// # Errors
    /// Has the same session/signal-handler requirements as [`Self::start`].
    pub fn start_observed() -> Result<Self, String> {
        Self::start_inner(Mode::Off, true)
    }

    fn start_inner(mode: Mode, observed: bool) -> Result<Self, String> {
        let mut active = ACTIVE.lock().map_err(|error| error.to_string())?;
        if active.is_some() {
            return Err("a compiler progress session is already active".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal_flag = Arc::clone(&cancelled);
        ctrlc::set_handler(move || signal_flag.store(true, Ordering::Relaxed))
            .map_err(|error| format!("could not install Ctrl-C handling: {error}"))?;
        let state = Arc::new(State {
            current: Mutex::new(None),
            cancelled,
            stopped: AtomicBool::new(false),
            mode,
            observed,
            terminal: use_terminal(
                mode,
                io::stderr().is_terminal(),
                std::env::var("TERM").as_deref().ok(),
                std::env::var_os("CI").is_some(),
            ),
        });
        let display = if mode == Mode::Off {
            None
        } else {
            let display_state = Arc::clone(&state);
            Some(
                std::thread::Builder::new()
                    .name("compiler-progress".into())
                    .spawn(move || {
                        while !display_state.stopped.load(Ordering::Relaxed) {
                            display_state.refresh(false);
                            std::thread::park_timeout(TICK);
                        }
                    })
                    .map_err(|error| format!("could not start compiler progress: {error}"))?,
            )
        };
        *active = Some(Arc::clone(&state));
        drop(active);
        ENABLED.store(true, Ordering::Relaxed);
        Ok(Self { state, display })
    }

    /// True after the user presses Ctrl-C, including with progress disabled.
    #[must_use]
    pub fn interrupted(&self) -> bool {
        self.state.cancelled.load(Ordering::Relaxed)
    }

    /// Reads actual counters without touching compiler workers.
    #[must_use]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.state
            .current
            .lock()
            .ok()?
            .as_ref()
            .map(Current::snapshot)
    }

    /// Requests cooperative cancellation at the compiler's safe boundaries.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Relaxed);
    }

    /// Clears a prior cancellation. Call only after joining the previous build.
    pub fn reset_cancellation(&self) {
        self.state.cancelled.store(false, Ordering::Relaxed);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        ENABLED.store(false, Ordering::Relaxed);
        self.state.stopped.store(true, Ordering::Relaxed);
        if let Some(display) = self.display.take() {
            display.thread().unpark();
            let _joined = display.join();
        }
        if self.state.terminal {
            crate::logging::progress_line(None);
        }
        if let Ok(mut active) = ACTIVE.lock() {
            *active = None;
        }
    }
}

fn active() -> Option<Arc<State>> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    ACTIVE.lock().ok()?.as_ref().map(Arc::clone)
}

fn use_terminal(mode: Mode, tty: bool, term: Option<&str>, ci: bool) -> bool {
    mode == Mode::Auto && tty && !matches!(term, Some("dumb" | "")) && !ci
}

/// One source's lifetime, including early errors and cached-package returns.
pub(crate) struct BuildScope(Option<Arc<State>>);

impl BuildScope {
    pub(crate) fn start(source: &Path) -> Self {
        let state = active();
        if let Some(shared) = &state {
            if let Ok(mut current) = shared.current.lock() {
                let now = Instant::now();
                *current = Some(Current {
                    source: ascii(
                        &source
                            .file_name()
                            .unwrap_or(source.as_os_str())
                            .to_string_lossy(),
                    ),
                    variant: String::new(),
                    stage: "reading and validating source".into(),
                    unit: "receivers",
                    started: now,
                    work: None,
                    last_plain: now,
                    last_count: 0,
                    last_interrupted: false,
                });
            }
            shared.refresh(true);
        }
        Self(state)
    }

    pub(crate) fn finish(&self, result: &Result<super::BuildReport, String>) {
        if let Some(state) = &self.0
            && let Ok(mut current) = state.current.lock()
            && let Some(build) = current.take()
        {
            if state.terminal {
                crate::logging::progress_line(None);
            }
            if state.mode != Mode::Off {
                let status = if state.cancelled.load(Ordering::Relaxed) {
                    "interrupted"
                } else {
                    match result {
                        Ok(report) if !report.rebuilt => "current; verified package reused",
                        Ok(_) => "complete",
                        Err(_) => "failed",
                    }
                };
                crate::logging::warn(format_args!(
                    "[compile {}] {}: {status}",
                    elapsed(build.started.elapsed()),
                    build.source
                ));
            }
        }
    }
}

impl Drop for BuildScope {
    fn drop(&mut self) {
        // Unwinding still leaves a clean terminal and an honest final status.
        self.finish(&Err("build did not complete".into()));
    }
}

pub(crate) fn cancellation_flag() -> Arc<AtomicBool> {
    active().map_or_else(
        || Arc::new(AtomicBool::new(false)),
        |state| Arc::clone(&state.cancelled),
    )
}

pub(crate) fn check_cancelled() -> Result<(), String> {
    if active().is_some_and(|state| state.cancelled.load(Ordering::Relaxed)) {
        Err("build interrupted".into())
    } else {
        Ok(())
    }
}

pub(crate) fn variant(name: &str) {
    if let Some(state) = active()
        && let Ok(mut current) = state.current.lock()
        && let Some(build) = current.as_mut()
    {
        build.variant = if name == "off" { "static" } else { name }.into();
    }
}

/// Reports a named indeterminate stage. Never implies a whole-build percentage.
pub(crate) fn stage(label: impl AsRef<str>) {
    stage_with_unit(label.as_ref(), "receivers");
}

pub(crate) fn stage_with_unit(label: &str, unit: &'static str) {
    update(label, unit, None);
}

fn update(label: &str, unit: &'static str, counter: Option<Arc<Counter>>) {
    if let Some(state) = active() {
        if let Ok(mut current) = state.current.lock()
            && let Some(build) = current.as_mut()
        {
            build.stage = ascii(label);
            build.unit = unit;
            build.work = counter;
            build.last_count = 0;
        }
        state.refresh(true);
    }
}

struct Counter {
    done: AtomicUsize,
    total: usize,
    unit: &'static str,
}

/// Acquired once per parallel pass; updates are a relaxed atomic per batch.
pub(crate) struct Work(Option<Arc<Counter>>);

impl Work {
    pub(crate) fn current(total: usize) -> Self {
        let Some(state) = active() else {
            return Self(None);
        };
        if state.mode == Mode::Off && !state.observed {
            return Self(None);
        }
        let label = state.current.lock().ok().and_then(|current| {
            current
                .as_ref()
                .map(|build| (build.stage.clone(), build.unit))
        });
        label.map_or_else(
            || Self(None),
            |(stage, unit)| Self::start(&stage, total, unit),
        )
    }

    pub(crate) fn start(label: &str, total: usize, unit: &'static str) -> Self {
        if !ENABLED.load(Ordering::Relaxed) || total == 0 {
            stage(label);
            return Self(None);
        }
        let counter = Arc::new(Counter {
            done: AtomicUsize::new(0),
            total,
            unit,
        });
        update(label, unit, Some(Arc::clone(&counter)));
        Self(Some(counter))
    }

    pub(crate) fn advance(&self, count: usize) {
        if let Some(counter) = &self.0 {
            let _previous = counter.done.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub(crate) fn finish(&self) {
        // Force the final true count into plain logs even for fast passes.
        if self.0.is_some()
            && let Some(state) = active()
        {
            state.refresh(true);
        }
    }
}

impl State {
    fn refresh(&self, force: bool) {
        if self.mode == Mode::Off {
            return;
        }
        if let Ok(mut current) = self.current.lock()
            && let Some(build) = current.as_mut()
        {
            let done = build
                .work
                .as_ref()
                .map_or(0, |work| work.done.load(Ordering::Relaxed));
            let interrupted = self.cancelled.load(Ordering::Relaxed);
            if self.terminal {
                let width = terminal_size::terminal_size_of(io::stderr())
                    .map_or(80, |(terminal_size::Width(columns), _height)| {
                        usize::from(columns)
                    });
                crate::logging::progress_line(Some(frame(
                    build,
                    width.saturating_sub(1),
                    interrupted,
                )));
            } else if force
                || interrupted != build.last_interrupted
                || (build.last_plain.elapsed() >= PLAIN_INTERVAL && done != build.last_count)
            {
                crate::logging::warn(frame(build, usize::MAX, interrupted));
                build.last_plain = Instant::now();
                build.last_count = done;
                build.last_interrupted = interrupted;
            }
        }
    }
}

fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn ascii(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii() && !character.is_control() {
                character
            } else {
                '?'
            }
        })
        .collect()
}

fn clipped(text: &str, width: usize) -> String {
    if text.len() <= width {
        return text.into();
    }
    let mut shortened: String = text.chars().take(width.saturating_sub(1)).collect();
    if width > 0 {
        shortened.push('~');
    }
    shortened
}

fn frame(build: &Current, width: usize, interrupted: bool) -> String {
    let clock = if width == usize::MAX {
        format!("[compile {}]", elapsed(build.started.elapsed()))
    } else {
        format!("[{}]", elapsed(build.started.elapsed()))
    };
    let label = if interrupted {
        "interrupted; stopping current operation".into()
    } else if build.variant.is_empty() {
        build.stage.clone()
    } else {
        format!("{}: {}", build.variant, build.stage)
    };
    let mut tail = String::new();
    if !interrupted && let Some(work) = &build.work {
        let done = work.done.load(Ordering::Relaxed).min(work.total);
        let percent = usize::try_from(
            u128::try_from(done)
                .unwrap_or(u128::MAX)
                .saturating_mul(100)
                .checked_div(u128::try_from(work.total).unwrap_or(1))
                .unwrap_or(0),
        )
        .unwrap_or(100);
        tail = if width < 72 {
            format!(" {done}/{} ({percent}%)", work.total)
        } else {
            format!(" {done}/{} {} ({percent}%)", work.total, work.unit)
        };
        if (72..=240).contains(&width) {
            let filled = percent.saturating_mul(10) / 100;
            let _bar = write!(
                tail,
                " [{}{}]",
                "#".repeat(filled),
                "-".repeat(10_usize.saturating_sub(filled))
            );
        }
    }
    // Keep the stage/count visible before the filename at narrow widths; leave
    // one terminal column unused to avoid automatic line wrapping.
    let core = format!("{clock} {label}{tail}");
    let full = format!("{clock} {} | {label}{tail}", build.source);
    if full.len() <= width {
        full
    } else if core.len() <= width {
        core
    } else {
        let budget = width.saturating_sub(clock.len().saturating_add(tail.len()).saturating_add(1));
        clipped(&format!("{clock} {}{tail}", clipped(&label, budget)), width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure formatting and four tiny counter workers: no environment changes,
    // sleeping, GPU work or map bake. Protects honest counts, width and mode policy.
    #[test]
    fn stage_frames_have_real_denominators_and_fit_terminal_widths() {
        let now = Instant::now();
        let mut build = Current {
            source: ascii("a-long-\u{1b}[31m-雪-source.json"),
            variant: "full".into(),
            stage: "base direct visibility".into(),
            unit: "receivers",
            started: now,
            work: None,
            last_plain: now,
            last_count: 0,
            last_interrupted: false,
        };
        let indeterminate = frame(&build, usize::MAX, false);
        assert!(
            build.snapshot().work.is_none(),
            "GUI preparation must remain uncounted"
        );
        assert!(
            !indeterminate.contains('%'),
            "unknown work must have no percentage"
        );
        assert!(
            !indeterminate.contains('\u{1b}'),
            "source names cannot inject terminal escapes"
        );
        build.work = Some(Arc::new(Counter {
            done: AtomicUsize::new(25),
            total: 100,
            unit: "receivers",
        }));
        assert!(
            build
                .snapshot()
                .work
                .is_some_and(|work| work.completed == 25
                    && work.total == 100
                    && work.unit == "receivers"),
            "GUI counts must read the same worker counter as terminal progress"
        );
        assert!(
            frame(&build, 79, false).contains("25/100 receivers (25%)"),
            "counts describe the current pass"
        );
        assert!(
            frame(&build, 119, false).contains("[##--------]"),
            "a capable wide terminal gets a bar"
        );
        build.stage = "shared bounce 1/3".into();
        assert!(
            frame(&build, 59, false).contains("full: shared bounce 1/3 25/100 (25%)"),
            "narrow terminals must retain the stage and completed count"
        );
        assert!(
            !frame(&build, usize::MAX, false).contains("[##"),
            "plain output has no redraw bar"
        );
        for width in [0, 1, 12, 39, 59, 79, 119, 239] {
            for interrupted in [false, true] {
                let line = frame(&build, width, interrupted);
                assert!(
                    line.len() <= width && line.is_ascii(),
                    "ASCII frame must fit {width} columns"
                );
                if interrupted {
                    assert!(
                        !line.contains('%'),
                        "interruption cannot claim advancing work"
                    );
                }
            }
        }
        build.work = Some(Arc::new(Counter {
            done: AtomicUsize::new(usize::MAX),
            total: usize::MAX,
            unit: "items",
        }));
        assert!(
            frame(&build, usize::MAX, false).contains("(100%)"),
            "percentage arithmetic must not overflow"
        );
        assert_eq!(elapsed(Duration::from_secs(125)), "2:05");
    }

    #[test]
    fn workers_publish_completed_batches_without_lost_updates() {
        let work = Work(Some(Arc::new(Counter {
            done: AtomicUsize::new(0),
            total: 269,
            unit: "receivers",
        })));
        std::thread::scope(|scope| {
            for remainder in [1, 2, 3, 7] {
                let shared = &work;
                let _worker = scope.spawn(move || {
                    shared.advance(64);
                    shared.advance(remainder);
                });
            }
        });
        assert!(
            work.0
                .as_ref()
                .is_some_and(|counter| counter.done.load(Ordering::Relaxed) == 269),
            "all finished work must be visible across workers"
        );
    }

    #[test]
    fn redirected_dumb_ci_and_explicit_modes_do_not_animate() {
        assert!(use_terminal(
            Mode::Auto,
            true,
            Some("xterm-256color"),
            false
        ));
        for (mode, tty, term, ci) in [
            (Mode::Auto, false, Some("xterm"), false),
            (Mode::Auto, true, Some("dumb"), false),
            (Mode::Auto, true, Some("xterm"), true),
            (Mode::Plain, true, Some("xterm"), false),
            (Mode::Off, true, Some("xterm"), false),
        ] {
            assert!(
                !use_terminal(mode, tty, term, ci),
                "plain output must have no terminal controls"
            );
        }
    }
}

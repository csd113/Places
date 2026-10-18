//! Opt-in compiler feedback. Engine preparation and tests pay only a disabled
//! atomic check; active workers publish counts, never terminal output.

use std::fmt::Write;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::engine::progress::{Counter, Reporter, Subscription};
pub(crate) use crate::engine::progress::{Work, cancellation_flag, check_cancelled, stage};

static ACTIVE: Mutex<Option<Arc<State>>> = Mutex::new(None);
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
    /// Current source and explicit selected level label.
    pub source: String,
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
            source: self.source.clone(),
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
    reporting: Option<Subscription>,
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
        let reporter = Arc::clone(&state);
        let reporting = crate::engine::progress::subscribe(reporter)?;
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
        Ok(Self {
            state,
            display,
            reporting: Some(reporting),
        })
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

    pub(crate) fn cancellation_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.state.cancelled)
    }

    /// Clears a prior cancellation. Call only after joining the previous build.
    pub fn reset_cancellation(&self) {
        self.state.cancelled.store(false, Ordering::Relaxed);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.reporting.take());
        self.state.stopped.store(true, Ordering::Relaxed);
        if let Some(display) = self.display.take() {
            display.thread().unpark();
            let _joined = display.join();
        }
        if self.state.terminal {
            crate::engine::logging::progress_line(None);
        }
        if let Ok(mut active) = ACTIVE.lock() {
            *active = None;
        }
    }
}

fn active() -> Option<Arc<State>> {
    ACTIVE.lock().ok()?.as_ref().map(Arc::clone)
}

fn use_terminal(mode: Mode, tty: bool, term: Option<&str>, ci: bool) -> bool {
    mode == Mode::Auto && tty && !matches!(term, Some("dumb" | "")) && !ci
}

/// One source's lifetime, including early errors and cached-package returns.
pub(crate) struct BuildScope(Option<Arc<State>>);

impl BuildScope {
    pub(crate) fn start(source: &Path, selected: Option<&str>) -> Self {
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
            if let Some(level) = selected
                && let Ok(mut current) = shared.current.lock()
                && let Some(build) = current.as_mut()
            {
                build.source = format!("{} / {level}", build.source);
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
                crate::engine::logging::progress_line(None);
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
                crate::engine::logging::warn(format_args!(
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

pub(crate) fn variant(name: &str) {
    if let Some(state) = active()
        && let Ok(mut current) = state.current.lock()
        && let Some(build) = current.as_mut()
    {
        build.variant = if name == "off" { "static" } else { name }.into();
    }
}

/// Adapts neutral worker feedback to this compiler session's display policy.
impl Reporter for State {
    fn stage(&self, label: &str, unit: &'static str) {
        self.update(label, unit, None);
    }

    fn current_work(&self, total: usize) -> Option<Arc<Counter>> {
        if self.mode == Mode::Off && !self.observed {
            return None;
        }
        let label = self.current.lock().ok().and_then(|current| {
            current
                .as_ref()
                .map(|build| (build.stage.clone(), build.unit))
        });
        label.and_then(|(stage, unit)| self.start_work(&stage, total, unit))
    }

    fn start_work(&self, label: &str, total: usize, unit: &'static str) -> Option<Arc<Counter>> {
        if total == 0 {
            self.stage(label, "receivers");
            return None;
        }
        let counter = Arc::new(Counter {
            done: AtomicUsize::new(0),
            total,
            unit,
        });
        self.update(label, unit, Some(Arc::clone(&counter)));
        Some(counter)
    }

    fn refresh(&self) {
        self.refresh(true);
    }

    fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

impl State {
    fn update(&self, label: &str, unit: &'static str, counter: Option<Arc<Counter>>) {
        if let Ok(mut current) = self.current.lock()
            && let Some(build) = current.as_mut()
        {
            build.stage = ascii(label);
            build.unit = unit;
            build.work = counter;
            build.last_count = 0;
        }
        self.refresh(true);
    }

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
                crate::engine::logging::progress_line(Some(frame(
                    build,
                    width.saturating_sub(1),
                    interrupted,
                )));
            } else if force
                || interrupted != build.last_interrupted
                || (build.last_plain.elapsed() >= PLAIN_INTERVAL && done != build.last_count)
            {
                crate::engine::logging::warn(frame(build, usize::MAX, interrupted));
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

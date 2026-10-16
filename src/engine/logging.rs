//! Process-wide diagnostics with a quiet normal mode.
//!
//! A normal launch of Places prints nothing beyond genuine problems: the
//! startup telemetry (`[package]`, `[props]`, `[lighting]`, ...) and the
//! one-line progress reports are developer output, so they are printed only
//! when `PLACES_VERBOSE` is set to a true-ish value.
//!
//! Warnings and errors always print. A warning is deduplicated by key so a
//! per-material or per-file loop can never flood the terminal: the first
//! occurrence names the problem, later occurrences of the same key are silent.
//!
//! Nothing here changes simulation or rendering state; it is deliberately the
//! only module that is allowed to write to stdout/stderr.

use std::any::Any;
use std::collections::HashSet;
use std::fmt::Display;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static DIAGNOSTIC_STDERR: AtomicBool = AtomicBool::new(false);
static OUTPUT: Mutex<Output> = Mutex::new(Output { live: None });

/// Keeps compiler diagnostics off its result stream, including `--json`.
/// Call once from the CLI before dispatch; the player retains its usual output.
pub fn diagnostics_to_stderr() {
    DIAGNOSTIC_STDERR.store(true, Ordering::Relaxed);
}

struct Output {
    live: Option<String>,
}

impl Output {
    fn clear(&self, writer: &mut impl Write) -> io::Result<()> {
        if self.live.is_some() {
            write!(writer, "\r\x1b[2K")?;
        }
        Ok(())
    }

    fn line(&self, writer: &mut impl Write, message: impl Display) -> io::Result<()> {
        self.clear(writer)?;
        writeln!(writer, "{message}")?;
        if let Some(live) = &self.live {
            write!(writer, "\r{live}")?;
        }
        writer.flush()
    }

    fn progress(&mut self, writer: &mut impl Write, line: Option<String>) -> io::Result<()> {
        self.clear(writer)?;
        if let Some(text) = &line {
            write!(writer, "\r{text}")?;
        }
        self.live = line;
        writer.flush()
    }
}

/// Replaces the compiler's single live stderr line. `None` clears it.
/// Only the compiler's TTY display calls this; plain logs never use escapes.
pub(crate) fn progress_line(line: Option<String>) {
    if let Ok(mut output) = OUTPUT.lock() {
        // A broken diagnostic stream must not fail an otherwise valid build.
        if output.progress(&mut io::stderr().lock(), line).is_err() {
            output.live = None;
        }
    }
}

fn stderr_line(message: impl Display) {
    if let Ok(output) = OUTPUT.lock() {
        let _written = output.line(&mut io::stderr().lock(), message);
    }
}

/// Environment variable that turns developer telemetry on.
///
/// Any value other than empty, `0`, `false` or `off` enables it, matching the
/// other `PLACES_*` switches.
pub const VERBOSE_ENV: &str = "PLACES_VERBOSE";

/// Retains the standard panic messages when a worker join becomes an error.
/// Custom panic payloads have no required Display implementation.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

/// True when developer telemetry should print.
#[must_use]
pub fn verbose() -> bool {
    std::env::var(VERBOSE_ENV).is_ok_and(|value| truthy(&value))
}

/// Shared truthiness rule for the `PLACES_*` switches.
fn truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off"
    )
}

/// Prints one developer telemetry line when [`verbose`] is enabled.
#[expect(
    clippy::print_stdout,
    reason = "this is the central opt-in telemetry output"
)]
pub fn info(message: impl Display) {
    if verbose() {
        if DIAGNOSTIC_STDERR.load(Ordering::Relaxed) {
            stderr_line(message);
        } else {
            println!("{message}");
        }
    }
}

/// A low-frequency lifecycle event retained even during a quiet player launch.
pub(crate) fn event(message: impl Display) {
    crate::engine::diagnostics::record("event", &message);
    info(message);
}

/// Prints one warning, always.
///
/// Use for a genuine problem the player can act on (a level that failed to
/// load, a missing asset root, an unwritable settings file).
pub fn warn(message: impl Display) {
    crate::engine::diagnostics::record("warning", &message);
    stderr_line(message);
}

/// Prints one warning the first time `key` is seen in this process.
///
/// `key` identifies the problem class (usually the asset id or file path), so
/// repeated encounters collapse into one line. Use for problems that can occur
/// once per item in a loop.
pub fn warn_once(key: impl AsRef<str>, message: impl Display) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let first = seen
        .lock()
        .is_ok_and(|mut set| set.insert(key.as_ref().to_string()));
    if first {
        crate::engine::diagnostics::record("warning", &message);
        stderr_line(message);
    }
}

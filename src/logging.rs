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
use std::sync::{Mutex, OnceLock};

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
        println!("{message}");
    }
}

/// Prints one warning, always.
///
/// Use for a genuine problem the player can act on (a level that failed to
/// load, a missing asset root, an unwritable settings file).
#[expect(clippy::print_stderr, reason = "this is the central warning output")]
pub fn warn(message: impl Display) {
    eprintln!("{message}");
}

/// Prints one warning the first time `key` is seen in this process.
///
/// `key` identifies the problem class (usually the asset id or file path), so
/// repeated encounters collapse into one line. Use for problems that can occur
/// once per item in a loop.
#[expect(
    clippy::print_stderr,
    reason = "this is the central deduplicated warning output"
)]
pub fn warn_once(key: impl AsRef<str>, message: impl Display) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let first = seen
        .lock()
        .is_ok_and(|mut set| set.insert(key.as_ref().to_string()));
    if first {
        eprintln!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::{panic_message, truthy};

    #[test]
    fn worker_panic_messages_preserve_string_payloads() {
        assert_eq!(panic_message(&"worker failed"), "worker failed");
        assert_eq!(
            panic_message(&String::from("owned failure")),
            "owned failure"
        );
        assert_eq!(panic_message(&42_u32), "non-string panic payload");
    }

    #[test]
    fn only_an_explicit_truthy_value_enables_telemetry() {
        for value in ["1", "true", "TRUE", "yes", " on "] {
            assert!(truthy(value), "{value:?} should be truthy");
        }
        for value in ["", "0", "false", "FALSE", "off", " off "] {
            assert!(!truthy(value), "{value:?} should be false");
        }
    }
}

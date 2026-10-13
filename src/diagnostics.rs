//! Bounded player-session evidence. No signal handlers or panic recovery.
use std::fmt::{self, Display, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const MAX_LOG_BYTES: u64 = 256 * 1024;
const MAX_RECORD_BYTES: usize = 4096;
static JOURNAL: Mutex<Option<Journal>> = Mutex::new(None);

struct Journal {
    directory: PathBuf,
    file: Option<File>,
    bytes: u64,
    started: Instant,
    origin: String,
    map_request: String,
    context: String,
    // A permanent, locked inode serializes writers across player processes.
    // A second player uses stderr and never rotates the first player's files.
    _owner: File,
}

impl Journal {
    fn open(directory: &Path) -> io::Result<(Self, bool)> {
        fs::create_dir_all(directory)?;
        let owner = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("owner.lock"))?;
        owner.try_lock().map_err(io::Error::other)?;
        let interrupted = directory.join("session.active").exists();
        let mut journal = Self {
            directory: directory.to_path_buf(),
            file: None,
            bytes: 0,
            started: Instant::now(),
            origin: String::new(),
            map_request: String::new(),
            context: String::new(),
            _owner: owner,
        };
        journal.rotate()?;
        let mut marker = File::create(directory.join("session.active"))?;
        writeln!(
            marker,
            "pid={} started_unix_seconds={}",
            std::process::id(),
            unix_seconds()
        )?;
        marker.sync_data()?;
        Ok((journal, interrupted))
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let current = self.directory.join("player.log");
        let previous = self.directory.join("player.previous.log");
        remove_if_present(&previous)?;
        if current.exists() {
            fs::rename(current, previous)?;
        }
        self.file = Some(File::create(self.directory.join("player.log"))?);
        self.bytes = 0;
        if let Some(file) = &mut self.file {
            for header in [&self.origin, &self.map_request, &self.context] {
                if !header.is_empty() {
                    writeln!(file, "{header}")?;
                    self.bytes = self.bytes.saturating_add(
                        u64::try_from(header.len())
                            .unwrap_or(u64::MAX)
                            .saturating_add(1),
                    );
                }
            }
        }
        Ok(())
    }

    fn record(&mut self, kind: &str, message: impl Display) -> io::Result<()> {
        if self.file.is_none() {
            return Ok(());
        }
        let line = bounded(format_args!(
            "unix={} elapsed_ms={} {kind} {message}",
            unix_seconds(),
            self.started.elapsed().as_millis()
        ));
        if matches!(kind, "session" | "executable") {
            if !self.origin.is_empty() {
                self.origin.push('\n');
            }
            self.origin.push_str(&line);
        } else if matches!(kind, "runtime_context" | "world_committed") {
            self.context.clone_from(&line);
        } else if kind == "map_request" {
            self.map_request.clone_from(&line);
        }
        let length = u64::try_from(line.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        if self.bytes.saturating_add(length) > MAX_LOG_BYTES {
            self.rotate()?;
        }
        if let Some(file) = &mut self.file {
            writeln!(file, "{line}")?;
            self.bytes = self.bytes.saturating_add(length);
        }
        Ok(())
    }

    fn finish(&self) -> io::Result<()> {
        if let Some(file) = &self.file {
            file.sync_data()?;
        }
        remove_if_present(&self.directory.join("session.active"))
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

/// Limits formatting itself, including panic payloads, and keeps one record on
/// one line. No environment dump, frame buffers, or OS crash reports are copied.
fn bounded(message: impl Display) -> String {
    struct Limited(String);
    impl fmt::Write for Limited {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            for character in text.chars() {
                let replacement = if character.is_control() {
                    ' '
                } else {
                    character
                };
                if self.0.len().saturating_add(replacement.len_utf8()) > MAX_RECORD_BYTES {
                    return Err(fmt::Error);
                }
                self.0.push(replacement);
            }
            Ok(())
        }
    }
    let mut limited = Limited(String::with_capacity(MAX_RECORD_BYTES));
    let _format_result = write!(&mut limited, "{message}");
    limited.0
}

fn executable_identity() -> io::Result<String> {
    let path = std::env::current_exe()?;
    let mut file = File::open(&path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(
            buffer
                .get(..count)
                .ok_or_else(|| io::Error::other("invalid executable read"))?,
        );
    }
    Ok(format!(
        "path={} sha256={:x}",
        path.display(),
        digest.finalize()
    ))
}

pub fn start() -> bool {
    let directory = crate::assets::state_path("diagnostics");
    match Journal::open(&directory) {
        Ok((journal, interrupted)) => {
            if let Ok(mut slot) = JOURNAL.lock() {
                *slot = Some(journal);
            }
            record(
                "session",
                format_args!(
                    "version={} os={} arch={} debug_assertions={} pid={} prior_interrupted={interrupted}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    cfg!(debug_assertions),
                    std::process::id()
                ),
            );
            record(
                "executable",
                executable_identity()
                    .unwrap_or_else(|error| format!("identity unavailable: {error}")),
            );
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |information| {
                // A panic inside logging must never wait on its own mutex.
                record("panic", information);
                record("backtrace", std::backtrace::Backtrace::force_capture());
                previous_hook(information);
            }));
            interrupted
        }
        Err(error) => {
            crate::logging::warn(format_args!(
                "[diagnostics] persistent log unavailable at {}: {error}",
                directory.display()
            ));
            false
        }
    }
}

/// Best effort evidence only: failed I/O disables writes, never gameplay.
pub fn record(kind: &str, message: impl Display) {
    if let Ok(mut slot) = JOURNAL.try_lock()
        && let Some(journal) = slot.as_mut()
        && let Err(error) = journal.record(kind, message)
    {
        journal.file = None;
        // Avoid the logger here: it calls back into this module.
        #[expect(
            clippy::print_stderr,
            reason = "report diagnostic I/O failure without recursive logging"
        )]
        {
            eprintln!("Places diagnostic log stopped: {error}");
        }
    }
}

pub fn finish(error: Option<&str>) {
    record("exit", error.map_or("normal shutdown", |message| message));
    if let Ok(mut slot) = JOURNAL.lock() {
        if let Some(journal) = slot.as_mut()
            && let Err(failure) = journal.finish()
        {
            crate::logging::warn(format_args!(
                "[diagnostics] could not complete session: {failure}"
            ));
        }
        *slot = None;
    }
}

pub fn previous_session_notice(interrupted: bool) {
    if interrupted {
        show_notice(
            "Places stopped unexpectedly",
            "The previous Places session did not finish normally. This can follow a crash, forced quit, or interrupted shutdown. Its bounded log has been retained.",
        );
    }
}

pub fn show_notice(title: &str, message: &str) {
    crate::logging::warn(message);
    // Explicit benchmark runs remain unattended, with the same persisted evidence.
    if std::env::var("PLACES_BENCH").as_deref() == Ok("1") {
        return;
    }
    let body = format!(
        "{}\n\nDiagnostics: {}\n\nIf reporting this problem, include player.log and player.previous.log. On macOS, the Places report in Console > Crash Reports may explain a native crash.",
        bounded(message),
        crate::assets::state_path("diagnostics").display()
    );
    let result = sdl3::messagebox::show_simple_message_box(
        sdl3::messagebox::MessageBoxFlag::ERROR,
        title,
        &body,
        None,
    );
    if let Err(error) = result {
        crate::logging::warn(format_args!(
            "[diagnostics] could not show failure notice: {error}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_bound_unicode_and_control_characters() {
        // A few KiB protect UTF-8 boundaries and line injection; no runtime/GPU.
        let line = bounded(format_args!("\n\0{}", "é".repeat(MAX_RECORD_BYTES)));
        assert!(line.len() <= MAX_RECORD_BYTES);
        assert!(!line.chars().any(char::is_control));
    }

    #[test]
    fn sessions_rotate_bound_storage_and_distinguish_interruption() -> io::Result<()> {
        // Isolated <1 MiB file fixture protects rotation, clean/unclean shutdown
        // and competing-player ownership without starting SDL or baking a map.
        let directory = std::env::temp_dir().join(format!(
            "places-diagnostics-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |time| time.as_nanos())
        ));
        fs::create_dir_all(&directory)?;
        let (mut journal, interrupted) = Journal::open(&directory)?;
        assert!(!interrupted);
        journal.record("session", "version=test executable=test-hash")?;
        journal.record("runtime_context", "map=test quality=full")?;
        journal.record("map_request", "requested_manifest=test-identity")?;
        assert!(
            Journal::open(&directory).is_err(),
            "a second player must not rotate the active log"
        );
        for _ in 0_u32..150_u32 {
            journal.record("test", "x".repeat(MAX_RECORD_BYTES))?;
        }
        for name in ["player.log", "player.previous.log"] {
            assert!(fs::metadata(directory.join(name))?.len() <= MAX_LOG_BYTES);
            let log = fs::read_to_string(directory.join(name))?;
            assert!(log.contains("executable=test-hash") && log.contains("map=test"));
            assert!(log.contains("requested_manifest=test-identity"));
        }
        drop(journal);
        let (restarted, was_interrupted) = Journal::open(&directory)?;
        assert!(
            was_interrupted,
            "a missing exit retains evidence of an abrupt stop"
        );
        restarted.finish()?;
        drop(restarted);
        let (clean, clean_interrupted) = Journal::open(&directory)?;
        assert!(!clean_interrupted, "normal shutdown must not claim a crash");
        clean.finish()?;
        drop(clean);
        fs::remove_dir_all(directory)
    }
}

//! Small bounded pipe readers and child lifetime management.

use std::io::{Read, Write as _};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::map_compiler::BuildReport;
use crate::map_compiler::events::{Event, Outcome, PROTOCOL, Progress};

const DIAGNOSTIC_BYTES: usize = 16 * 1024;
const EVENT_BYTES: usize = 4 * 1024 * 1024;
const JSON_BYTES: usize = 1024 * 1024;
const CANCEL_GRACE: Duration = Duration::from_secs(8);
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

#[cfg(unix)]
#[path = "containment_unix.rs"]
mod containment;
#[cfg(windows)]
#[path = "containment_windows.rs"]
mod containment;

#[cfg(not(any(unix, windows)))]
#[path = "containment_unsupported.rs"]
mod containment;

struct InterruptiblePipe<R> {
    stream: R,
    stop: Arc<AtomicBool>,
}
impl<R: containment::Pipe> Read for InterruptiblePipe<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        containment::read(&mut self.stream, buffer, &self.stop)
    }
}

#[derive(Clone, Copy)]
pub(super) enum StopPolicy {
    Cooperative,
    Terminate,
}

#[derive(Clone, Default)]
pub(super) struct Capture {
    pub progress: Option<Progress>,
    pub terminal: Option<(Outcome, Option<Box<BuildReport>>, Option<String>)>,
    pub started: bool,
    pub protocol_error: Option<String>,
    pub diagnostics: String,
    pub output: Vec<u8>,
}

impl Capture {
    fn event(&mut self, line: &[u8]) {
        let event = match serde_json::from_slice::<Event>(line) {
            Ok(event) => event,
            Err(error) => {
                let _recorded = self
                    .protocol_error
                    .get_or_insert_with(|| format!("Invalid JSON event: {error}"));
                return;
            }
        };
        if self.terminal.is_some() {
            let _recorded = self
                .protocol_error
                .get_or_insert_with(|| "Received events after final result".into());
            return;
        }
        match event {
            Event::Started {
                protocol,
                source: _,
                output: _,
            } => {
                if self.started || protocol != PROTOCOL {
                    let _recorded = self.protocol_error.get_or_insert_with(|| {
                        "Duplicate start or unsupported event protocol".into()
                    });
                }
                self.started = true;
            }
            Event::Progress(progress) => {
                if !self.started {
                    let _recorded = self
                        .protocol_error
                        .get_or_insert_with(|| "Progress arrived before start".into());
                }
                self.progress = Some(progress);
            }
            Event::Finished {
                outcome,
                report,
                message,
            } => {
                self.terminal = Some((outcome, report, message));
            }
        }
    }

    fn diagnostic(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        self.diagnostics.extend(text.chars().map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        }));
        self.diagnostics.push('\n');
        let excess = self.diagnostics.len().saturating_sub(DIAGNOSTIC_BYTES);
        if excess > 0 {
            let boundary = self
                .diagnostics
                .char_indices()
                .map(|(index, _)| index)
                .find(|index| *index >= excess)
                .unwrap_or(self.diagnostics.len());
            drop(self.diagnostics.drain(..boundary));
        }
    }
}

#[derive(Clone, Copy)]
enum Output {
    Events,
    Diagnostics,
    Json,
}

pub(super) struct CapturedChild {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    capture: Arc<Mutex<Capture>>,
    readers: Vec<std::thread::JoinHandle<()>>,
    reader_stop: Arc<AtomicBool>,
    tree: Option<containment::Containment>,
    exited: Option<Instant>,
    terminated: Option<Instant>,
    policy: StopPolicy,
    stopped: Option<Instant>,
}

impl CapturedChild {
    pub fn spawn(
        command: &mut Command,
        policy: StopPolicy,
        protocol: bool,
    ) -> Result<Self, String> {
        Self::spawn_output(
            command,
            policy,
            if protocol {
                Output::Events
            } else {
                Output::Diagnostics
            },
        )
    }

    pub fn spawn_json(command: &mut Command) -> Result<Self, String> {
        Self::spawn_output(command, StopPolicy::Terminate, Output::Json)
    }

    fn spawn_output(
        command: &mut Command,
        policy: StopPolicy,
        output_kind: Output,
    ) -> Result<Self, String> {
        let _configured = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        containment::configure(command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start tool: {error}"))?;
        let tree = match containment::Containment::new(&child) {
            Ok(tree) => tree,
            Err(error) => {
                let _killed = child.kill();
                let _reaped = child.wait();
                return Err(format!("Could not contain tool: {error}"));
            }
        };
        let input = child.stdin.take();
        let output = child.stdout.take();
        let errors = child.stderr.take();
        let mut process = Self {
            child: Some(child),
            stdin: input,
            capture: Arc::new(Mutex::new(Capture::default())),
            readers: Vec::with_capacity(2),
            reader_stop: Arc::new(AtomicBool::new(false)),
            tree: Some(tree),
            exited: None,
            terminated: None,
            policy,
            stopped: None,
        };
        process.reader(output.ok_or("Tool stdout was not piped")?, output_kind)?;
        process.reader(
            errors.ok_or("Tool stderr was not piped")?,
            Output::Diagnostics,
        )?;
        Ok(process)
    }

    fn reader(
        &mut self,
        stream: impl containment::Pipe + Send + 'static,
        output: Output,
    ) -> Result<(), String> {
        containment::configure_pipe(&stream)
            .map_err(|error| format!("Could not configure tool output: {error}"))?;
        let interruptible = InterruptiblePipe {
            stream,
            stop: Arc::clone(&self.reader_stop),
        };
        let capture = Arc::clone(&self.capture);
        let reader = std::thread::Builder::new()
            .name("editor-tool-output".into())
            .spawn(move || match output {
                Output::Json => read_json(interruptible, &capture),
                Output::Events => read_stream(interruptible, true, &capture),
                Output::Diagnostics => read_stream(interruptible, false, &capture),
            })
            .map_err(|error| format!("Could not read tool output: {error}"))?;
        self.readers.push(reader);
        Ok(())
    }

    pub fn capture(&self) -> Capture {
        self.capture.lock().map_or_else(
            |_| Capture {
                protocol_error: Some("Tool feedback lock failed".into()),
                ..Capture::default()
            },
            |capture| capture.clone(),
        )
    }

    pub const fn stopping(&self) -> bool {
        self.stopped.is_some()
    }

    pub fn stop(&mut self) {
        if self.stopped.is_some() {
            return;
        }
        self.stopped = Some(Instant::now());
        if matches!(self.policy, StopPolicy::Cooperative) {
            if let Some(input) = self.stdin.as_mut() {
                let _sent = input.write_all(b"{\"command\":\"cancel\"}\n");
            }
            drop(self.stdin.take());
        } else {
            // Termination itself is nonblocking; poll drains and joins readers.
            let _terminated = self.terminate();
        }
    }

    fn terminate(&mut self) -> Result<(), String> {
        if self.terminated.is_none() {
            if let Some(tree) = &self.tree {
                tree.terminate()
                    .map_err(|error| format!("Could not terminate tool operation: {error}"))?;
            }
            self.terminated = Some(Instant::now());
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<ExitStatus>, String> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        if self.exited.is_none()
            && let Some(tree) = &self.tree
            && tree
                .exited(child)
                .map_err(|error| format!("Could not inspect tool: {error}"))?
        {
            self.exited = Some(Instant::now());
        }
        let readers_done = self
            .readers
            .iter()
            .all(std::thread::JoinHandle::is_finished);
        if self
            .stopped
            .is_some_and(|stopped| stopped.elapsed() >= CANCEL_GRACE)
            || self
                .exited
                .is_some_and(|exited| readers_done || exited.elapsed() >= OUTPUT_DRAIN_GRACE)
        {
            self.terminate()?;
        }
        if !readers_done
            && self
                .terminated
                .is_some_and(|terminated| terminated.elapsed() >= OUTPUT_DRAIN_GRACE)
        {
            if let Ok(mut capture) = self.capture.lock() {
                let _recorded = capture.protocol_error.get_or_insert_with(||
                    "Tool output remained open after operation termination; output forcibly closed".into());
                capture.diagnostic(b"Tool output forcibly closed after bounded drain; captured diagnostics may be incomplete");
            }
            self.reader_stop.store(true, Ordering::Release);
        }
        if self.exited.is_some() && readers_done {
            // Signal the still-anchored group even when pipes already closed:
            // descendants which redirected their output are owned too.
            self.terminate()?;
            let status = self
                .child
                .as_mut()
                .ok_or("Tool ownership lost")?
                .wait()
                .map_err(|error| format!("Could not reap tool: {error}"))?;
            drop(self.child.take());
            self.tree = None;
            drop(self.stdin.take());
            for reader in self.readers.drain(..) {
                reader
                    .join()
                    .map_err(|_panic| "Tool output reader panicked".to_string())?;
            }
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }
}

struct Cleanup {
    child: Child,
    tree: Option<containment::Containment>,
    readers: Vec<std::thread::JoinHandle<()>>,
    reader_stop: Arc<AtomicBool>,
    stopped: Instant,
}
impl Cleanup {
    fn retire(mut self) {
        // All reads are interruptible, and only this bounded reaper owns them.
        // The UI never joins a live reader or waits through cancellation grace.
        loop {
            let exited = self
                .tree
                .as_ref()
                .is_none_or(|tree| tree.exited(&mut self.child).unwrap_or(true));
            if exited || self.stopped.elapsed() >= CANCEL_GRACE {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some(tree) = &self.tree {
            let _terminated = tree.terminate();
        }
        // Direct kill is also a fallback for an OS containment failure.
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
        self.reader_stop.store(true, Ordering::Release);
        for reader in self.readers {
            let _joined = reader.join();
        }
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        self.stop();
        if let Some(child) = self.child.take() {
            let pending_cleanup = Cleanup {
                child,
                tree: self.tree.take(),
                readers: std::mem::take(&mut self.readers),
                reader_stop: Arc::clone(&self.reader_stop),
                stopped: self.stopped.unwrap_or_else(Instant::now),
            };
            // Preserve ownership if starting the bounded cleanup thread fails.
            let pending = Arc::new(Mutex::new(Some(pending_cleanup)));
            let background = Arc::clone(&pending);
            let reaper = std::thread::Builder::new()
                .name("editor-tool-reaper".into())
                .spawn(move || {
                    if let Ok(mut owned) = background.lock()
                        && let Some(cleanup) = owned.take()
                    {
                        cleanup.retire();
                    }
                });
            if reaper.is_err()
                && let Ok(mut owned) = pending.lock()
                && let Some(mut cleanup) = owned.take()
            {
                // Rare OS thread-start failure: force immediate cleanup rather
                // than wait through the normal cooperative grace on the UI.
                cleanup.stopped = Instant::now()
                    .checked_sub(CANCEL_GRACE)
                    .unwrap_or_else(Instant::now);
                cleanup.retire();
            }
        }
    }
}

fn read_json(mut stream: impl Read, capture: &Mutex<Capture>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if let Ok(mut state) = capture.lock() {
                    let remaining = JSON_BYTES.saturating_sub(state.output.len());
                    state
                        .output
                        .extend_from_slice(buffer.get(..count.min(remaining)).unwrap_or_default());
                    if count > remaining {
                        state.protocol_error = Some("Tool JSON exceeds 1 MiB limit".into());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                if let Ok(mut state) = capture.lock() {
                    state.protocol_error = Some(format!("Could not read tool JSON: {error}"));
                }
                break;
            }
        }
    }
}

pub(super) fn read_stream(mut stream: impl Read, protocol: bool, capture: &Mutex<Capture>) {
    let limit = if protocol {
        EVENT_BYTES
    } else {
        DIAGNOSTIC_BYTES
    };
    let mut buffer = [0_u8; 8192];
    let mut line = Vec::new();
    let mut overflow = false;
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                if let Ok(mut state) = capture.lock() {
                    state.diagnostic(format!("Could not read tool output: {error}").as_bytes());
                    if protocol {
                        state.protocol_error = Some("Event stream read failed".into());
                    }
                }
                break;
            }
        };
        for byte in buffer.get(..count).unwrap_or_default() {
            if *byte == b'\n' {
                consume_line(&line, overflow, protocol, capture);
                line.clear();
                overflow = false;
            } else if line.len() < limit {
                line.push(*byte);
            } else {
                overflow = true;
            }
        }
    }
    if !line.is_empty() || overflow {
        consume_line(&line, overflow, protocol, capture);
    }
}

fn consume_line(line: &[u8], overflow: bool, protocol: bool, capture: &Mutex<Capture>) {
    if let Ok(mut state) = capture.lock() {
        if protocol {
            if overflow {
                let _recorded = state
                    .protocol_error
                    .get_or_insert_with(|| "Compiler event exceeds size limit".into());
            } else {
                state.event(line);
            }
        } else {
            state.diagnostic(line);
        }
    }
}

#[cfg(test)]
#[path = "../tests/child_lifetime.rs"]
mod tests;

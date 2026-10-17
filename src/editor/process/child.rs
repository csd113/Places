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
const CANCEL_GRACE: Duration = Duration::from_secs(8);

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

pub(super) struct CapturedChild {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    capture: Arc<Mutex<Capture>>,
    stdout_done: Arc<AtomicBool>,
    stderr_done: Arc<AtomicBool>,
    policy: StopPolicy,
    stopped: Option<Instant>,
}

impl CapturedChild {
    pub fn spawn(
        command: &mut Command,
        policy: StopPolicy,
        protocol: bool,
    ) -> Result<Self, String> {
        let _configured = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start tool: {error}"))?;
        let input = child.stdin.take();
        let output = child.stdout.take();
        let errors = child.stderr.take();
        let process = Self {
            child: Some(child),
            stdin: input,
            capture: Arc::new(Mutex::new(Capture::default())),
            stdout_done: Arc::new(AtomicBool::new(false)),
            stderr_done: Arc::new(AtomicBool::new(false)),
            policy,
            stopped: None,
        };
        process.reader(
            output.ok_or("Tool stdout was not piped")?,
            protocol,
            Arc::clone(&process.stdout_done),
        )?;
        process.reader(
            errors.ok_or("Tool stderr was not piped")?,
            false,
            Arc::clone(&process.stderr_done),
        )?;
        Ok(process)
    }

    fn reader(
        &self,
        stream: impl Read + Send + 'static,
        protocol: bool,
        done: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let capture = Arc::clone(&self.capture);
        let _reader = std::thread::Builder::new()
            .name("editor-tool-output".into())
            .spawn(move || {
                read_stream(stream, protocol, &capture);
                done.store(true, Ordering::Release);
            })
            .map_err(|error| format!("Could not read tool output: {error}"))?;
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
        } else if let Some(child) = self.child.as_mut() {
            let _killed = child.kill();
        }
    }

    pub fn poll(&mut self) -> Result<Option<ExitStatus>, String> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        if self
            .stopped
            .is_some_and(|stopped| stopped.elapsed() >= CANCEL_GRACE)
        {
            let _killed = child.kill();
        }
        let status = child
            .try_wait()
            .map_err(|error| format!("Could not reap tool: {error}"))?;
        if status.is_some()
            && self.stdout_done.load(Ordering::Acquire)
            && self.stderr_done.load(Ordering::Acquire)
        {
            drop(self.child.take());
            drop(self.stdin.take());
            Ok(status)
        } else {
            Ok(None)
        }
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        self.stop();
        if let Some(child) = self.child.take() {
            let deadline = self.stopped.unwrap_or_else(Instant::now);
            // Reaping never blocks a rendered editor frame. Retain ownership
            // across a rare thread-start error so a child cannot escape.
            let pending = Arc::new(Mutex::new(Some(child)));
            let background = Arc::clone(&pending);
            let reaper = std::thread::Builder::new()
                .name("editor-tool-reaper".into())
                .spawn(move || {
                    if let Ok(mut owned) = background.lock()
                        && let Some(mut process) = owned.take()
                    {
                        loop {
                            match process.try_wait() {
                                Ok(Some(_)) => break,
                                Ok(None) if deadline.elapsed() < CANCEL_GRACE => {
                                    std::thread::sleep(Duration::from_millis(25));
                                }
                                Ok(None) | Err(_) => {
                                    let _killed = process.kill();
                                    let _reaped = process.wait();
                                    break;
                                }
                            }
                        }
                    }
                });
            if reaper.is_err()
                && let Ok(mut owned) = pending.lock()
                && let Some(mut process) = owned.take()
            {
                let _killed = process.kill();
                let _reaped = process.wait();
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

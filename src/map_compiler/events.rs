//! Versioned newline-delimited JSON for editor/compiler subprocesses.
//!
//! Stdout contains only events; diagnostics stay on stderr. Stdin accepts one
//! `{"command":"cancel"}` line. Closing stdin also requests safe cancellation.

use std::io::{self, BufRead as _, Read as _, Write as _};
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::progress::{Session, Snapshot};
use super::{BuildReport, BuildRequest};

pub const PROTOCOL: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Started {
        protocol: u32,
        source: String,
        output: String,
    },
    Progress(Progress),
    Finished {
        outcome: Outcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        report: Option<Box<BuildReport>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub variant: String,
    pub stage: String,
    pub elapsed_millis: u64,
    pub work: Option<Work>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub completed: usize,
    pub total: usize,
    pub unit: String,
}

impl From<Snapshot> for Progress {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            variant: snapshot.variant,
            stage: snapshot.stage,
            elapsed_millis: u64::try_from(snapshot.elapsed.as_millis()).unwrap_or(u64::MAX),
            work: snapshot.work.map(|work| Work {
                completed: work.completed,
                total: work.total,
                unit: work.unit.into(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failed,
    Cancelled,
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Cancel,
}

pub(super) enum Failure {
    Failed(String),
    Cancelled,
}

pub(super) fn emit(event: &Event) -> Result<(), String> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, event)
        .map_err(|error| format!("could not write compiler event: {error}"))?;
    output
        .write_all(b"\n")
        .and_then(|()| output.flush())
        .map_err(|error| format!("could not flush compiler event: {error}"))
}

pub(super) fn build(request: &BuildRequest) -> Result<BuildReport, Failure> {
    emit(&Event::Started {
        protocol: PROTOCOL,
        source: request.source.display().to_string(),
        output: request.out.display().to_string(),
    })
    .map_err(Failure::Failed)?;
    let session = Session::start_observed().map_err(Failure::Failed)?;
    let cancellation = session.cancellation_token();
    // Stdin has no other use in machine mode. This reader may remain blocked
    // until process exit; it owns only the cancellation token, never the build.
    let _control = std::thread::Builder::new()
        .name("compiler-control".into())
        .spawn(move || {
            let mut input = io::stdin().lock();
            loop {
                let mut line = Vec::new();
                let count = input.by_ref().take(1025).read_until(b'\n', &mut line);
                match count {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > 1024 => break,
                    Ok(_) => {
                        if matches!(
                            serde_json::from_slice::<Control>(&line),
                            Ok(Control::Cancel)
                        ) {
                            break;
                        }
                    }
                }
            }
            cancellation.store(true, Ordering::Relaxed);
        })
        .map_err(|error| Failure::Failed(format!("could not start compiler control: {error}")))?;
    let result = std::thread::scope(|scope| {
        let build = scope.spawn(|| super::build(request));
        let mut stream_error = None;
        while !build.is_finished() {
            if let Some(snapshot) = session.snapshot()
                && let Err(error) = emit(&Event::Progress(snapshot.into()))
            {
                session.cancel();
                stream_error = Some(error);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let completed = build
            .join()
            .unwrap_or_else(|_| Err("compiler worker stopped unexpectedly".into()));
        stream_error.map_or(completed, Err)
    });
    match result {
        // Publication may have completed immediately before cancellation. Its
        // successful report remains authoritative; never label valid output stale.
        Ok(report) => Ok(report),
        Err(_) if session.interrupted() => Err(Failure::Cancelled),
        Err(error) => Err(Failure::Failed(error)),
    }
}

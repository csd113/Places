//! Opt-in monotonic loading/event/presentation trace for the native harness.
use std::io::{BufWriter, Write};
use std::time::Instant;

pub struct LoadTrace {
    started: Instant,
    output: Option<BufWriter<std::fs::File>>,
}

impl LoadTrace {
    pub fn new() -> Self {
        let output = std::env::var_os("PLACES_LOAD_TRACE").and_then(|path| {
            match std::fs::File::create(path) {
                Ok(file) => Some(BufWriter::new(file)),
                Err(error) => {
                    crate::logging::warn(format!("cannot create loading trace: {error}"));
                    None
                }
            }
        });
        Self {
            started: Instant::now(),
            output,
        }
    }

    pub fn record(&mut self, event: &str, request: u64, detail: &str) {
        let Some(output) = &mut self.output else {
            return;
        };
        let value = serde_json::json!({
            "event": event, "request": request, "detail": detail,
            "elapsed_ms": self.started.elapsed().as_secs_f64() * 1_000.0_f64,
        });
        let failed = writeln!(output, "{value}").is_err()
            || (!matches!(
                event,
                "event_pump" | "present" | "upload_step_begin" | "upload_step_end"
            ) && output.flush().is_err());
        if failed {
            self.output = None;
            crate::logging::warn("loading trace write failed; tracing disabled");
        }
    }
}

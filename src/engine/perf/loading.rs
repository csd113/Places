//! Opt-in monotonic loading/event/presentation trace for the native harness.
use std::io::{BufWriter, Write};
use std::time::Instant;

pub struct LoadTrace {
    started: Instant,
    output: Option<BufWriter<std::fs::File>>,
}

impl LoadTrace {
    /// True only while an explicit native trace file is active.
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.output.is_some()
    }

    pub fn new() -> Self {
        let output = std::env::var_os("PLACES_LOAD_TRACE").and_then(|path| {
            match std::fs::File::create(path) {
                Ok(file) => Some(BufWriter::new(file)),
                Err(error) => {
                    crate::engine::logging::warn(format!("cannot create loading trace: {error}"));
                    None
                }
            }
        });
        Self {
            started: Instant::now(),
            output,
        }
    }

    /// Allows expensive callers to skip state nobody will consume.
    #[must_use]
    pub fn will_record(&self, event: &str) -> bool {
        self.output.is_some() || Self::journaled(event)
    }

    fn journaled(event: &str) -> bool {
        matches!(
            event,
            "entry"
                | "window_ready"
                | "request"
                | "phase"
                | "cpu_ready"
                | "gpu_ready"
                | "world_committed"
                | "runtime_context"
                | "failed"
                | "cancel"
                | "load_cancelled"
                | "load_recovered"
                | "graphics_cancelled"
                | "settings_change"
                | "settings_applied"
                | "window_size_observed"
                | "shutdown_requested"
                | "shutdown_complete"
                | "menu_ready"
                | "action"
        )
    }

    pub fn record(&mut self, event: &str, request: u64, detail: &str) {
        if Self::journaled(event) {
            crate::engine::diagnostics::record(event, format_args!("request={request} {detail}"));
        }
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
            crate::engine::logging::warn("loading trace write failed; tracing disabled");
        }
    }
}

#[cfg(test)]
#[path = "../tests/load_trace_filter.rs"]
mod tests;

/// Opt-in stage costs, including worker-side decode and main-thread installation.
/// No per-stage clock or output is used without an explicit loading trace.
pub struct StageTimer {
    scope: &'static str,
    started: Option<Instant>,
}

impl StageTimer {
    pub(crate) fn new(scope: &'static str) -> Self {
        Self {
            scope,
            started: std::env::var_os("PLACES_LOAD_TRACE").map(|_| Instant::now()),
        }
    }

    pub(crate) fn step(&mut self, stage: &'static str) {
        if let Some(started) = &mut self.started {
            let now = Instant::now();
            crate::engine::logging::info(format_args!(
                "[loading-stage] scope={} stage={stage} ms={:.3}",
                self.scope,
                now.duration_since(*started).as_secs_f64() * 1_000.0_f64,
            ));
            *started = now;
        }
    }
}

//! Opt-in native frame/pump evidence. Ordinary editor pacing stays unchanged.
use std::time::Instant;

use sdl3::video::Window;

use super::viewport::Viewport;
use crate::engine::perf::loading::LoadTrace;

pub(super) struct FrameTrace {
    trace: LoadTrace,
    previous: Instant,
    frame: Instant,
}

impl FrameTrace {
    pub(super) fn new(performance_test: bool, window: &Window) -> Self {
        let mut trace = LoadTrace::new();
        trace.record("entry", 0, if performance_test {
            "editor performance_test=true vsync=false sleep_ms=0; latency is distinct from FPS; verify display pacing"
        } else {
            "editor performance_test=false vsync=true sleep_ms=16"
        });
        if trace.is_recording() {
            let refresh = window
                .get_display()
                .ok()
                .and_then(|display| display.get_mode().ok())
                .map(|mode| mode.refresh_rate);
            trace.record(
                "runtime_context",
                0,
                &format!("display_refresh_hz={refresh:?}"),
            );
        }
        Self {
            trace,
            previous: Instant::now(),
            frame: Instant::now(),
        }
    }

    pub(super) fn pump(&mut self, viewport: &Viewport, window: &Window) {
        if !self.trace.is_recording() {
            return;
        }
        let now = Instant::now();
        self.trace.record(
            "event_pump",
            0,
            &format!(
                "gap_ms={:.3} preparing={} focus={}",
                now.duration_since(self.previous).as_secs_f64() * 1_000.0_f64,
                viewport.is_preparing(),
                window.has_input_focus()
            ),
        );
        self.previous = now;
        self.frame = now;
    }

    pub(super) fn present(&mut self, viewport: &Viewport, window: &Window, presented: bool) {
        if !self.trace.is_recording() {
            return;
        }
        let (width, height) = window.size_in_pixels();
        self.trace.record(
            "present",
            0,
            &format!(
                "frame_ms={:.3} mode={} drawable={}x{} preparing={} focus={} presented={}",
                self.frame.elapsed().as_secs_f64() * 1_000.0_f64,
                viewport.presentation_mode(),
                width,
                height,
                viewport.is_preparing(),
                window.has_input_focus(),
                presented
            ),
        );
    }
}

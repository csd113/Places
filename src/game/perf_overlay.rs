//! Cached player performance HUD built from shared engine counters.

use std::time::{Duration, Instant};

use crate::engine::perf::{CpuSampler, DeviceSampler, GpuUtilization, process_resident_bytes};
use crate::engine::render::{GpuTiming, Vertex};
use crate::game::ui::{add_rect, draw_text};

/// Update interval for the performance statistics (roughly twice per second).
pub const PERF_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// Frames per second over `seconds`, rounded to the nearest whole frame.
fn frames_per_second(frames: u32, seconds: f32) -> u32 {
    // `frames` counts the `update` calls since the last half-second metrics
    // refresh, so the widening cast is exact on any display (a few hundred
    // frames at most) and the narrowing `as` saturates exactly like the
    // historical cast did.
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`frames` counts the `update` calls since the last half-second metrics refresh, so the widening cast is exact on any display (a few hundred frames at most) and the narrowing `as` saturates exactly like the historical cast did."
    )]
    let fps = (frames as f32 / seconds).round() as u32;
    fps
}

/// Character count of `text` as `f32`.
///
/// The overlay line is a fixed-format diagnostic of a few dozen characters, so
/// the `u16` conversion is exact and the count is lossless in `f32`.
fn char_count(text: &str) -> f32 {
    f32::from(u16::try_from(text.chars().count()).unwrap_or(u16::MAX))
}

/// Rounded binary mebibytes without narrowing a resident-byte counter.
const fn resident_mebibytes(bytes: u64) -> u64 {
    const MEBIBYTE: u64 = 1_048_576;
    let whole = bytes / MEBIBYTE;
    if bytes % MEBIBYTE >= 524_288 {
        whole.saturating_add(1)
    } else {
        whole
    }
}

/// Pure metric formatting keeps unsupported counters and GPU scopes distinct.
fn overlay_text(
    cpu: Option<f32>,
    gpu: GpuUtilization,
    timing: GpuTiming,
    resident: Option<u64>,
    fps: u32,
) -> String {
    let cpu_text = cpu.map_or_else(
        || "CPU system N/A".to_string(),
        |pct| format!("CPU system {pct:.0}%"),
    );
    let gpu_text = match gpu {
        GpuUtilization::Measured(pct) => format!("GPU device {pct:.0}%"),
        GpuUtilization::Unavailable(_) => "GPU device N/A".to_string(),
    };
    let work_text = match timing {
        GpuTiming::Pending => "GPU elapsed pending".to_string(),
        GpuTiming::Measured(sample) => format!("GPU elapsed {:.2}ms", sample.milliseconds),
        GpuTiming::Unavailable(_) => "GPU elapsed N/A".to_string(),
    };
    let ram_text = resident.map_or_else(
        || "Game RAM RSS N/A".to_string(),
        |bytes| format!("Game RAM RSS {}MiB", resident_mebibytes(bytes)),
    );
    let mut text = format!("{cpu_text}  {gpu_text}\n{work_text}  {ram_text}  FPS {fps}");
    if let GpuUtilization::Unavailable(reason) = gpu {
        text.push_str("\nGPU device: ");
        text.push_str(reason);
    }
    if let GpuTiming::Unavailable(reason) = timing {
        text.push_str("\nGPU elapsed: ");
        text.push_str(reason.reason());
    }
    text
}

/// Performance statistics overlay (toggled with '-').
pub struct PerfOverlay {
    visible: bool,
    last_update: Instant,
    accumulated_frames: u32,
    accumulated_time: f32,
    cpu_sampler: CpuSampler,
    device_sampler: DeviceSampler,
    gpu_timing: GpuTiming,
    cached_text: String,
    cached_vertices: Vec<Vertex>,
}

impl Default for PerfOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfOverlay {
    #[must_use]
    pub fn new() -> Self {
        Self {
            visible: false, // Hidden by default
            last_update: Instant::now(),
            accumulated_frames: 0,
            accumulated_time: 0.0,
            cpu_sampler: CpuSampler::new(),
            device_sampler: DeviceSampler::default(),
            gpu_timing: GpuTiming::Pending,
            cached_text: String::new(),
            cached_vertices: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible && self.cached_vertices.is_empty() {
            // Immediate update on becoming visible so user doesn't see a blank frame
            self.refresh_metrics();
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if self.visible && self.cached_vertices.is_empty() {
            self.refresh_metrics();
        }
    }

    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    #[must_use]
    pub fn cached_vertices(&self) -> &[Vertex] {
        &self.cached_vertices
    }

    #[must_use]
    pub fn cached_text(&self) -> &str {
        &self.cached_text
    }

    /// Receives the renderer's latest completed hardware timestamp sample.
    pub const fn set_gpu_timing(&mut self, timing: GpuTiming) {
        self.gpu_timing = timing;
    }

    /// Records a rendered frame delta time.
    /// Updates CPU, GPU, FPS, and regenerates cached geometry roughly twice per second.
    pub fn update(&mut self, delta_seconds: f32) {
        self.accumulated_frames = self.accumulated_frames.saturating_add(1);
        self.accumulated_time += delta_seconds;

        // Update roughly twice per second (~500ms), not every frame
        if self.last_update.elapsed() >= PERF_UPDATE_INTERVAL {
            if self.visible {
                self.refresh_metrics();
            } else {
                // Keep time tracking fresh while hidden
                self.last_update = Instant::now();
                self.accumulated_frames = 0;
                self.accumulated_time = 0.0;
            }
        }
    }

    pub fn refresh_metrics(&mut self) {
        let elapsed = self.last_update.elapsed().as_secs_f32();
        let fps = if self.accumulated_time > 0.0 {
            frames_per_second(self.accumulated_frames, self.accumulated_time)
        } else if elapsed > 0.0 {
            frames_per_second(self.accumulated_frames, elapsed)
        } else {
            0
        };

        self.cached_text = overlay_text(
            self.cpu_sampler.sample(),
            self.device_sampler.sample_gpu(),
            self.gpu_timing,
            process_resident_bytes(),
            fps,
        );
        self.rebuild_cached_vertices();

        self.last_update = Instant::now();
        self.accumulated_frames = 0;
        self.accumulated_time = 0.0;
    }

    /// Rebuilds cached 2D vertices for the overlay at the 480x272 reference-space
    /// top-left (scaled to the drawable by `Renderer::render_ui`).
    pub fn rebuild_cached_vertices(&mut self) {
        self.cached_vertices.clear();
        if self.cached_text.is_empty() {
            return;
        }

        let characters = self
            .cached_text
            .lines()
            .map(char_count)
            .fold(0.0_f32, f32::max);
        let lines = f32::from(u16::try_from(self.cached_text.lines().count()).unwrap_or(u16::MAX));
        let pad_x = 4.0;
        let pad_y = 3.0;
        let text_w = characters * 8.0;
        let line_height = 11.0;
        let text_h = lines.mul_add(line_height, -3.0);

        let x0 = 4.0;
        let y0 = 4.0;
        let x1 = f32::mul_add(pad_x, 2.0, x0 + text_w);
        let y1 = f32::mul_add(pad_y, 2.0, y0 + text_h);

        // Dark background box to ensure readability over any 3D scene surface
        add_rect(
            &mut self.cached_vertices,
            x0,
            y0,
            x1,
            y1,
            [0.06, 0.06, 0.05],
        );

        // draw_text is a single-line primitive; position each metric line explicitly.
        for (index, line) in self.cached_text.lines().enumerate() {
            let row = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
            draw_text(
                &mut self.cached_vertices,
                line,
                x0 + pad_x,
                row.mul_add(line_height, y0 + pad_y),
                1.0,
                [0.92, 0.92, 0.88],
            );
        }
    }
}

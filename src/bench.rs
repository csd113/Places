//! Debug-only frame telemetry and hardware benchmark harness.
//!
//! Everything in this module is inert unless `PLACES_BENCH=1` is set in the
//! environment, so a normal release build keeps printing nothing and allocates
//! nothing per frame. Repeatable desktop measurements are written to stdout
//! and CSV without requiring interaction with the performance overlay.
//!
//! Timing is deliberately staged around presentation, because the whole
//! point of the first phase is to find out whether presentation actually
//! blocks on the display refresh or returns immediately:
//!
//! ```text
//! t_begin ─ events + game update ─ t_update ─ scene submit ─ t_render
//!        ─ UI submit ─ t_ui ─ present ─ t_swap ─ (next t_begin)
//! ```
//!
//! * `update_ms` — event pump, menu/gameplay update, movement.
//! * `render_ms` — `render_scene` + `render_ui`, i.e. the CPU cost of
//!   submitting the frame (not including presentation).
//! * `swap_ms` — time spent presenting. On a truly VSync-locked presentation
//!   this is where the frame waits for the display.
//! * `frame_ms` — `t_begin` to `t_swap`: the complete frame including the swap.
//! * `loop_ms` — `t_begin` of this frame to `t_begin` of the next: the real
//!   presentation cadence, which is what an FPS counter should be derived from.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use crate::render::RenderStats;

/// Environment switch that turns every other `PLACES_BENCH_*` option on.
const BENCH_ENV: &str = "PLACES_BENCH";
/// Optional CSV path; one row per recorded frame.
const BENCH_OUT_ENV: &str = "PLACES_BENCH_OUT";
/// Number of leading frames to discard before recording (pipeline warm-up).
const BENCH_WARMUP_ENV: &str = "PLACES_BENCH_WARMUP";
/// Stop the process after this many recorded frames (bounds a hardware run).
const BENCH_FRAMES_ENV: &str = "PLACES_BENCH_FRAMES";
/// A scripted live quality-profile switch, for the runtime verification matrix:
/// `PLACES_BENCH_QUALITY_CYCLE=<frame>:<profile>[,<frame>:<profile>...]`, e.g.
/// `3:low,6:full`. Each entry selects the profile when `frame_count` reaches
/// its frame, through the same `Settings::set_quality` path the menu uses, so
/// the renderer's normal graphics rebuild runs. Debug-only; inert unless the
/// benchmark is enabled.
const QUALITY_CYCLE_ENV: &str = "PLACES_BENCH_QUALITY_CYCLE";

/// A scripted live window-event sequence, for the lifecycle matrix:
/// `PLACES_BENCH_WINDOW_CYCLE=<frame>:<action>[,<frame>:<action>...]`, where
/// the actions are `resize:<width>x<height>`, `minimize` and `restore`, e.g.
/// `3:resize:800x450,6:minimize,9:restore,12:resize:640x360`. Each entry asks
/// the real SDL window to perform the action when `frame_count` reaches its
/// frame, so the platform's own resize/minimize events (and the drawable-size
/// changes they produce) flow through the normal frame loop — not through a
/// target-recreation helper call. Debug-only; inert unless the benchmark is
/// enabled.
const WINDOW_CYCLE_ENV: &str = "PLACES_BENCH_WINDOW_CYCLE";

/// Freeze the camera at `yaw_degrees[,pitch_degrees]` for a repeatable shot.
const BENCH_CAMERA_ENV: &str = "PLACES_CAMERA";
/// `on`/`off` override for the swap interval, used only to characterise `VSync`.
const BENCH_VSYNC_ENV: &str = "PLACES_VSYNC";
/// `1` waits for submitted GPU work before the swap, splitting renderer time
/// from presentation time unambiguously (diagnostic only).
const BENCH_FINISH_ENV: &str = "PLACES_BENCH_FINISH";
/// `1` skips scene/UI submission, leaving only the presentation path
/// (diagnostic only: the window shows a stale frame).
const BENCH_NORENDER_ENV: &str = "PLACES_BENCH_NORENDER";
/// `1` skips presentation (diagnostic only: nothing is presented).
const BENCH_NOSWAP_ENV: &str = "PLACES_BENCH_NOSWAP";
/// `1` submits every batch, so the same build can measure what culling is worth.
const BENCH_NOCULL_ENV: &str = "PLACES_BENCH_NOCULL";

/// Per-frame timings, all in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    pub update_ms: f32,
    pub render_ms: f32,
    pub swap_ms: f32,
    pub frame_ms: f32,
    pub loop_ms: f32,
}

/// Diagnostic submission switches parsed from the `PLACES_BENCH*` environment.
///
/// Each one changes exactly one renderer decision, so one release build measures
/// what culling, finishing and skipping are each worth while everything else
/// (level build, batching, draw order, shaders) is held fixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BenchSwitches(u8);

impl BenchSwitches {
    /// Wait for submitted GPU work immediately before the swap (diagnostic).
    pub const FINISH_BEFORE_SWAP: Self = Self(1 << 0);
    /// Skip scene/UI submission entirely (diagnostic).
    pub const SKIP_RENDER: Self = Self(1 << 1);
    /// Skip presentation entirely (diagnostic).
    pub const SKIP_SWAP: Self = Self(1 << 2);
    /// Submit every batch regardless of the frustum, to measure culling's worth.
    pub const NO_CULL: Self = Self(1 << 3);

    /// True when every switch in `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Adds `other` to the set.
    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// Hard cap on retained frame records for an unbounded benchmark session.
///
/// `PLACES_BENCH_FRAMES` normally ends a run; this only bounds the case where
/// the harness is left enabled for a whole interactive session. At 60 fps the
/// cap is a little over half an hour of recording.
pub const MAX_RECORDED_FRAMES: usize = 120_000;

/// Parsed `PLACES_BENCH*` environment configuration.
#[derive(Clone, Debug, Default)]
pub struct BenchConfig {
    pub enabled: bool,
    pub out_path: Option<PathBuf>,
    pub warmup_frames: u64,
    pub limit_frames: Option<u64>,
    pub camera: Option<(f32, f32)>,
    pub vsync_override: Option<bool>,
    /// Diagnostic switches that each change one submission decision.
    pub switches: BenchSwitches,
}

impl BenchConfig {
    /// Reads the `PLACES_BENCH*` environment. Malformed values fall back to the
    /// documented defaults rather than aborting a hardware run.
    pub fn from_env() -> Self {
        let enabled = env_flag(BENCH_ENV);
        if !enabled {
            return Self::default();
        }
        let out_path = non_empty_var(BENCH_OUT_ENV).map(PathBuf::from);
        let warmup_frames = non_empty_var(BENCH_WARMUP_ENV)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let limit_frames = non_empty_var(BENCH_FRAMES_ENV)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
        let camera = non_empty_var(BENCH_CAMERA_ENV).and_then(|value| parse_camera(&value));
        let vsync_override =
            non_empty_var(BENCH_VSYNC_ENV).and_then(|value| parse_vsync_override(&value));
        let mut switches = BenchSwitches::default();
        for (name, switch) in [
            (BENCH_FINISH_ENV, BenchSwitches::FINISH_BEFORE_SWAP),
            (BENCH_NORENDER_ENV, BenchSwitches::SKIP_RENDER),
            (BENCH_NOSWAP_ENV, BenchSwitches::SKIP_SWAP),
            (BENCH_NOCULL_ENV, BenchSwitches::NO_CULL),
        ] {
            if env_flag(name) {
                switches.insert(switch);
            }
        }
        Self {
            enabled,
            out_path,
            warmup_frames,
            limit_frames,
            camera,
            vsync_override,
            switches,
        }
    }
}

/// Trimmed, non-empty environment value.
fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// True when the value is a non-empty, non-`0`/`false`/`off` string.
fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        let value = value.trim().to_ascii_lowercase();
        !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
    })
}

/// A scripted live window action for the lifecycle matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    /// Resize the window to a logical size in points.
    Resize(u32, u32),
    /// Minimize the window (the platform's own iconify event).
    Minimize,
    /// Restore a minimized window.
    Restore,
}

/// Parses one `PLACES_BENCH_WINDOW_CYCLE` action.
///
/// Accepts `resize:<width>x<height>` (positive integers), `minimize` and
/// `restore`, case-insensitively. Anything else is `None` and is ignored by
/// the runner, like every other malformed benchmark value.
#[must_use]
pub fn parse_window_action(value: &str) -> Option<WindowAction> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("minimize") {
        return Some(WindowAction::Minimize);
    }
    if value.eq_ignore_ascii_case("restore") {
        return Some(WindowAction::Restore);
    }
    let (width, height) = value.strip_prefix("resize:")?.split_once('x')?;
    let width = width
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)?;
    let height = height
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)?;
    Some(WindowAction::Resize(width, height))
}

/// Parses `yaw_degrees` or `yaw_degrees,pitch_degrees`.
#[must_use]
pub fn parse_camera(value: &str) -> Option<(f32, f32)> {
    let mut parts = value.split(',').map(str::trim);
    let yaw = parts.next()?.parse::<f32>().ok()?;
    let pitch = match parts.next() {
        Some(text) => text.parse::<f32>().ok()?,
        None => 0.0,
    };
    if yaw.is_finite() && pitch.is_finite() {
        Some((yaw, pitch))
    } else {
        None
    }
}

/// Parses an explicit swap-interval override: `on`/`off`/`1`/`0`.
#[must_use]
pub fn parse_vsync_override(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" | "vsync" => Some(true),
        "0" | "off" | "false" | "no" | "immediate" => Some(false),
        _ => None,
    }
}

/// Distribution summary for one timing channel, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimingSummary {
    pub mean_ms: f32,
    pub median_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub min_ms: f32,
    pub max_ms: f32,
}

impl TimingSummary {
    /// Summarises a slice of millisecond samples. Returns all zeros for an
    /// empty slice and never divides by a zero count.
    pub fn from_samples(samples: &mut [f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let sum: f32 = samples.iter().copied().sum();
        Self {
            mean_ms: sum / sample_count_f32(samples.len()),
            median_ms: percentile(samples, 0.50),
            p95_ms: percentile(samples, 0.95),
            p99_ms: percentile(samples, 0.99),
            min_ms: samples.first().copied().unwrap_or_default(),
            max_ms: samples.last().copied().unwrap_or_default(),
        }
    }

    /// Frames per second implied by a millisecond frame time.
    #[must_use]
    pub fn fps_from_ms(ms: f32) -> f32 {
        if ms > 0.0 { 1000.0 / ms } else { 0.0 }
    }
}

/// `count` samples as `f32`.
///
/// One sample is recorded per frame, so a run long enough to round the count
/// (2^24 frames is six days at 30 fps) would need a gigabyte of records; the
/// cast is exact for every run this harness can produce.
#[allow(clippy::cast_precision_loss)]
const fn sample_count_f32(count: usize) -> f32 {
    count as f32
}

/// Nearest-rank percentile over an already sorted, non-empty slice.
fn percentile(sorted: &[f32], fraction: f32) -> f32 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    let index = (sample_count_f32(last) * fraction).round();
    // `usize::try_from(f32)` is unstable (`convert_float_to_int`), so the
    // rounded index narrows with `as`; `min(last)` below keeps it in bounds
    // and the cast's saturation matches the historical behaviour.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = index as usize;
    sorted.get(index.min(last)).copied().unwrap_or(0.0)
}

/// One recorded frame: timings plus the geometry counters submitted with it.
#[derive(Clone, Copy, Debug, Default)]
struct FrameRecord {
    timings: FrameTimings,
    stats: RenderStats,
}

/// Collects per-frame samples and writes the run summary.
pub struct Bench {
    config: BenchConfig,
    csv: Option<std::fs::File>,
    warmup_remaining: u64,
    limit_remaining: Option<u64>,
    recorded: u64,
    last_begin: Option<Instant>,
    frames: Vec<FrameRecord>,
    /// Swap interval actually in force, as reported by the presentation mode.
    reported_swap_interval: Option<i32>,
    /// Scripted live quality switches, in ascending frame order.
    quality_cycle: Vec<(u64, crate::quality::QualityProfile)>,
    /// Scripted live window actions, in ascending frame order.
    window_cycle: Vec<(u64, WindowAction)>,
}

impl Bench {
    /// Builds the harness from the environment. When benchmarking is disabled
    /// the returned value is a no-op and holds no file handle or buffers.
    #[must_use]
    pub fn new() -> Self {
        let config = BenchConfig::from_env();
        let mut csv = config
            .out_path
            .as_ref()
            .and_then(|path| std::fs::File::create(path).ok());
        if let Some(file) = csv.as_mut() {
            // Header row: column order must match `record_frame`.
            let _ = writeln!(
                file,
                "frame,update_ms,render_ms,swap_ms,frame_ms,loop_ms,total_vertices,visible_vertices,culled_vertices,total_batches,visible_batches,draw_calls,vbo_bytes,index_bytes,texture_binds,material_changes,reflection_passes"
            );
        }
        let mut quality_cycle: Vec<(u64, crate::quality::QualityProfile)> =
            std::env::var(QUALITY_CYCLE_ENV)
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .filter_map(|entry| {
                            let (frame, profile) = entry.split_once(':')?;
                            Some((
                                frame.trim().parse::<u64>().ok()?,
                                crate::quality::QualityProfile::parse(profile)?,
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
        quality_cycle.sort_by_key(|(frame, _)| *frame);
        let mut window_cycle: Vec<(u64, WindowAction)> = std::env::var(WINDOW_CYCLE_ENV)
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|entry| {
                        let (frame, action) = entry.split_once(':')?;
                        Some((
                            frame.trim().parse::<u64>().ok()?,
                            parse_window_action(action)?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        window_cycle.sort_by_key(|(frame, _)| *frame);
        Self {
            warmup_remaining: config.warmup_frames,
            limit_remaining: config.limit_frames,
            config,
            csv,
            recorded: 0,
            last_begin: None,
            frames: Vec::new(),
            reported_swap_interval: None,
            quality_cycle,
            window_cycle,
        }
    }

    /// The window action a scripted lifecycle selects at `frame`, if any.
    ///
    /// Called by the frame loop next to [`Self::quality_cycle_at`], so the
    /// action reaches the real SDL window and the normal event/drawable path.
    #[must_use]
    pub fn window_cycle_at(&self, frame: u64) -> Option<WindowAction> {
        self.window_cycle
            .iter()
            .find(|(at, _)| *at == frame)
            .map(|(_, action)| *action)
    }

    /// The profile a scripted quality cycle selects at `frame`, if any.
    ///
    /// Called by the frame loop before the pending-settings application, so a
    /// scheduled switch goes through exactly the path a menu change does.
    #[must_use]
    pub fn quality_cycle_at(&self, frame: u64) -> Option<crate::quality::QualityProfile> {
        self.quality_cycle
            .iter()
            .find(|(at, _)| *at == frame)
            .map(|(_, profile)| *profile)
    }

    /// True when `PLACES_BENCH=1` was set, i.e. the harness should be driven.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.config.enabled
    }

    /// Camera override (`yaw_degrees`, `pitch_degrees`) for repeatable shots.
    #[must_use]
    pub const fn camera_override(&self) -> Option<(f32, f32)> {
        self.config.camera
    }

    /// Explicit swap-interval request, when `PLACES_VSYNC` was set.
    #[must_use]
    pub const fn vsync_override(&self) -> Option<bool> {
        self.config.vsync_override
    }

    /// Whether to sync submitted GPU work before measuring presentation.
    #[must_use]
    pub const fn finish_before_swap(&self) -> bool {
        self.config
            .switches
            .contains(BenchSwitches::FINISH_BEFORE_SWAP)
    }

    /// Whether scene/UI submission should be skipped for this run.
    #[must_use]
    pub const fn skip_render(&self) -> bool {
        self.config.switches.contains(BenchSwitches::SKIP_RENDER)
    }

    /// Whether presentation should be skipped for this run.
    #[must_use]
    pub const fn skip_swap(&self) -> bool {
        self.config.switches.contains(BenchSwitches::SKIP_SWAP)
    }

    /// Whether frustum culling should be disabled for this run.
    #[must_use]
    pub const fn no_cull(&self) -> bool {
        self.config.switches.contains(BenchSwitches::NO_CULL)
    }

    /// Records the swap interval the platform reports after configuration.
    pub const fn set_reported_swap_interval(&mut self, interval: i32) {
        self.reported_swap_interval = Some(interval);
    }

    /// Records one frame. `begin` must be the instant captured at the top of the
    /// loop iteration and `swap_done` the instant presentation returned.
    pub fn record_frame(
        &mut self,
        begin: Instant,
        t_update: Instant,
        t_render: Instant,
        t_ui: Instant,
        t_swap: Instant,
        stats: RenderStats,
    ) {
        if !self.config.enabled {
            return;
        }
        // An unbounded run (`PLACES_BENCH=1` with no `PLACES_BENCH_FRAMES`)
        // keeps statistics for the whole session; stop retaining records once
        // the cap is reached so a long-running session cannot grow forever.
        if self.frames.len() >= MAX_RECORDED_FRAMES {
            return;
        }
        // The gap between consecutive frame starts is the real presentation
        // cadence: the only honest source for an FPS number when a swap may or
        // may not block.
        let loop_ms = self.last_begin.map_or(0.0, |previous| {
            millis(begin.saturating_duration_since(previous))
        });
        self.last_begin = Some(begin);

        if self.warmup_remaining > 0 {
            self.warmup_remaining = self.warmup_remaining.saturating_sub(1);
            return;
        }
        if let Some(remaining) = self.limit_remaining.as_mut() {
            if *remaining == 0 {
                return;
            }
            *remaining = remaining.saturating_sub(1);
        }

        let frame_ms = millis(t_swap.saturating_duration_since(begin));
        let timings = FrameTimings {
            update_ms: millis(t_update.saturating_duration_since(begin)),
            render_ms: millis(t_ui.saturating_duration_since(t_render))
                + millis(t_render.saturating_duration_since(t_update)),
            swap_ms: millis(t_swap.saturating_duration_since(t_ui)),
            frame_ms,
            loop_ms: if self.recorded == 0 {
                frame_ms
            } else {
                loop_ms
            },
        };
        self.recorded = self.recorded.saturating_add(1);

        if let Some(file) = self.csv.as_mut() {
            let _ = writeln!(
                file,
                "{},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{},{},{},{},{},{},{}",
                self.recorded,
                timings.update_ms,
                timings.render_ms,
                timings.swap_ms,
                timings.frame_ms,
                timings.loop_ms,
                stats.total_vertices,
                stats.visible_vertices,
                stats.culled_vertices,
                stats.total_batches,
                stats.visible_batches,
                stats.draw_calls,
                stats.vbo_bytes,
                stats.index_bytes,
                stats.texture_binds,
                stats.material_changes,
                stats.reflection_passes,
            );
        }
        self.frames.push(FrameRecord { timings, stats });
    }

    /// True once `PLACES_BENCH_FRAMES` frames have been recorded.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.config.enabled && self.limit_remaining == Some(0)
    }

    /// Writes the run summary to stdout as a single `BENCH_SUMMARY {...}` line.
    /// Called once, when the run ends.
    // The benchmark report is the harness's entire purpose on a headless
    // device (the on-screen overlay cannot be read over SSH) and has no logger
    // to route through.
    #[allow(clippy::print_stdout)]
    pub fn finish(&mut self) {
        if !self.config.enabled || self.frames.is_empty() {
            println!("BENCH_SUMMARY {{\"frames\":0}}");
            return;
        }
        let mut update: Vec<f32> = self.frames.iter().map(|f| f.timings.update_ms).collect();
        let mut render: Vec<f32> = self.frames.iter().map(|f| f.timings.render_ms).collect();
        let mut swap: Vec<f32> = self.frames.iter().map(|f| f.timings.swap_ms).collect();
        let mut frame: Vec<f32> = self.frames.iter().map(|f| f.timings.frame_ms).collect();
        let mut loop_ms: Vec<f32> = self.frames.iter().map(|f| f.timings.loop_ms).collect();
        let update = TimingSummary::from_samples(&mut update);
        let render = TimingSummary::from_samples(&mut render);
        let swap = TimingSummary::from_samples(&mut swap);
        let frame = TimingSummary::from_samples(&mut frame);
        let loop_ms = TimingSummary::from_samples(&mut loop_ms);

        let last = self.frames.last().copied().unwrap_or_default();
        let level = std::env::var("PLACES_LEVEL").unwrap_or_default();
        println!(
            "BENCH_SUMMARY {{\"level\":\"{level}\",\"frames\":{},\"swap_interval\":{},\"update_mean_ms\":{:.3},\"render_mean_ms\":{:.3},\"swap_mean_ms\":{:.3},\"frame_mean_ms\":{:.3},\"loop_mean_ms\":{:.3},\"frame_median_ms\":{:.3},\"loop_median_ms\":{:.3},\"frame_p95_ms\":{:.3},\"frame_p99_ms\":{:.3},\"loop_p95_ms\":{:.3},\"loop_p99_ms\":{:.3},\"frame_min_ms\":{:.3},\"frame_max_ms\":{:.3},\"loop_min_ms\":{:.3},\"loop_max_ms\":{:.3},\"fps_median\":{:.2},\"fps_p95\":{:.2},\"fps_p99\":{:.2},\"fps_1pct_low\":{:.2},\"fps_mean\":{:.2},\"measured_fps_mean\":{:.2},\"worst_fps\":{:.2},\"total_vertices\":{},\"visible_vertices\":{},\"culled_vertices\":{},\"total_batches\":{},\"visible_batches\":{},\"draw_calls\":{},\"vbo_bytes\":{},\"index_bytes\":{},\"texture_binds\":{},\"material_changes\":{},\"reflection_passes\":{}}}",
            self.frames.len(),
            self.reported_swap_interval
                .map_or_else(|| "null".to_string(), |value| value.to_string()),
            update.mean_ms,
            render.mean_ms,
            swap.mean_ms,
            frame.mean_ms,
            loop_ms.mean_ms,
            frame.median_ms,
            loop_ms.median_ms,
            frame.p95_ms,
            frame.p99_ms,
            loop_ms.p95_ms,
            loop_ms.p99_ms,
            frame.min_ms,
            frame.max_ms,
            loop_ms.min_ms,
            loop_ms.max_ms,
            TimingSummary::fps_from_ms(loop_ms.median_ms),
            TimingSummary::fps_from_ms(loop_ms.p95_ms),
            TimingSummary::fps_from_ms(loop_ms.p99_ms),
            TimingSummary::fps_from_ms(loop_ms.p99_ms),
            TimingSummary::fps_from_ms(loop_ms.mean_ms),
            TimingSummary::fps_from_ms(frame.mean_ms),
            TimingSummary::fps_from_ms(loop_ms.max_ms),
            last.stats.total_vertices,
            last.stats.visible_vertices,
            last.stats.culled_vertices,
            last.stats.total_batches,
            last.stats.visible_batches,
            last.stats.draw_calls,
            last.stats.vbo_bytes,
            last.stats.index_bytes,
            last.stats.texture_binds,
            last.stats.material_changes,
            last.stats.reflection_passes,
        );
        if let Some(file) = self.csv.as_mut() {
            let _ = file.flush();
        }
    }
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

/// Milliseconds as `f32`, saturating rather than wrapping on absurd durations.
fn millis(duration: std::time::Duration) -> f32 {
    // Frame timings are milliseconds-scale; the cast narrows a diagnostic
    // value and rounds rather than truncating.
    #[allow(clippy::cast_possible_truncation)]
    let millis = duration.as_secs_f64() as f32 * 1000.0;
    millis
}

#[cfg(test)]
mod tests;

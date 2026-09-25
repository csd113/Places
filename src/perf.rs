use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::render::Vertex;
use crate::ui::{add_rect, draw_text};

/// Update interval for the performance statistics (roughly twice per second).
pub const PERF_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// Snapshot of CPU counters for calculating real utilization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}

impl CpuSample {
    #[must_use]
    pub const fn new(total: u64, idle: u64) -> Self {
        Self { total, idle }
    }
}

/// Parses total and idle ticks from the first line of Linux /proc/stat.
#[must_use]
pub fn parse_proc_stat(stat_str: &str) -> Option<CpuSample> {
    for line in stat_str.lines() {
        if line.starts_with("cpu ") {
            let mut parts = line.split_whitespace();
            parts.next(); // Skip "cpu" label
            let mut total = 0u64;
            let mut idle = 0u64;
            for (i, val) in parts.enumerate() {
                if let Ok(num) = val.parse::<u64>() {
                    total = total.saturating_add(num);
                    if i == 3 || i == 4 {
                        // Index 3 is idle, index 4 is iowait
                        idle = idle.saturating_add(num);
                    }
                }
            }
            if total > 0 {
                return Some(CpuSample { total, idle });
            }
        }
    }
    None
}

/// Computes CPU utilization percentage between two samples.
#[must_use]
pub fn calculate_cpu_percentage(prev: &CpuSample, curr: &CpuSample) -> Option<f32> {
    let delta_total = curr.total.saturating_sub(prev.total);
    let delta_idle = curr.idle.saturating_sub(prev.idle);
    if delta_total > 0 {
        let delta_busy = delta_total.saturating_sub(delta_idle);
        // Only the ratio of the two 500 ms tick deltas matters, and the result
        // is a diagnostic percentage clamped to `0..=100`; `f32`'s 24-bit
        // mantissa is ample and `as` rounds rather than truncating.
        #[allow(clippy::cast_precision_loss)]
        let pct = (delta_busy as f32 / delta_total as f32) * 100.0;
        Some(pct.clamp(0.0, 100.0))
    } else {
        None
    }
}

/// Sampler for measuring overall CPU utilization.
pub struct CpuSampler {
    last_sample: Option<CpuSample>,
    stat_path: &'static str,
}

impl Default for CpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuSampler {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_sample: None,
            stat_path: "/proc/stat",
        }
    }

    #[must_use]
    pub const fn with_path(path: &'static str) -> Self {
        Self {
            last_sample: None,
            stat_path: path,
        }
    }

    pub fn sample(&mut self) -> Option<f32> {
        // 1. First attempt reading Linux /proc/stat
        if let Ok(content) = fs::read_to_string(self.stat_path)
            && let Some(curr) = parse_proc_stat(&content)
        {
            let pct = self
                .last_sample
                .and_then(|prev| calculate_cpu_percentage(&prev, &curr));
            self.last_sample = Some(curr);
            return pct;
        }

        // 2. macOS fallback via Mach kernel host_statistics64 (for local dev/testing)
        #[cfg(target_os = "macos")]
        {
            if let Some(curr) = sample_macos_cpu() {
                let pct = self
                    .last_sample
                    .and_then(|prev| calculate_cpu_percentage(&prev, &curr));
                self.last_sample = Some(curr);
                return pct;
            }
        }

        None
    }
}

#[cfg(target_os = "macos")]
fn sample_macos_cpu() -> Option<CpuSample> {
    #[repr(C)]
    struct HostCpuLoadInfo {
        cpu_ticks: [u32; 4],
    }
    unsafe extern "C" {
        fn mach_host_self() -> u32;
        fn host_statistics64(
            host_priv: u32,
            flavor: i32,
            host_info_out: *mut HostCpuLoadInfo,
            host_info_outCnt: *mut u32,
        ) -> i32;
    }
    const HOST_CPU_LOAD_INFO: i32 = 3;

    unsafe {
        let mut info = HostCpuLoadInfo { cpu_ticks: [0; 4] };
        let mut count = u32::try_from(
            std::mem::size_of::<HostCpuLoadInfo>()
                .checked_div(std::mem::size_of::<i32>())
                .unwrap_or(0),
        )
        .unwrap_or(u32::MAX);
        let host = mach_host_self();
        if host_statistics64(host, HOST_CPU_LOAD_INFO, &raw mut info, &raw mut count) == 0 {
            let user = u64::from(info.cpu_ticks[0]);
            let system = u64::from(info.cpu_ticks[1]);
            let idle = u64::from(info.cpu_ticks[2]);
            let nice = u64::from(info.cpu_ticks[3]);
            let total = user
                .saturating_add(system)
                .saturating_add(idle)
                .saturating_add(nice);
            Some(CpuSample { total, idle })
        } else {
            None
        }
    }
}

/// Parses devfreq load content from /sys/class/devfreq/*/load.
/// Supports standard formats:
/// - "45" or "45%"
/// - "45@500000000" (load%@frequency from vendor kernels)
/// - "`busy_time` `total_time`" or "`busy_time` / `total_time`"
#[must_use]
pub fn parse_devfreq_load(content: &str) -> Option<f32> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Format: "45@500000000"
    if let Some((load_part, _)) = trimmed.split_once('@')
        && let Ok(pct) = load_part.trim().trim_end_matches('%').parse::<f32>()
    {
        return Some(pct.clamp(0.0, 100.0));
    }

    // Format: "busy_time total_time" or "busy_time / total_time"
    let parts: Vec<&str> = trimmed
        .split(|c: char| c.is_whitespace() || c == '/')
        .filter(|s| !s.is_empty())
        .collect();
    if let [busy_text, total_text, ..] = parts.as_slice()
        && let (Ok(busy), Ok(total)) = (busy_text.parse::<f64>(), total_text.parse::<f64>())
        && total > 0.0
    {
        // The percentage is clamped to `0..=100` below, well inside `f32`'s
        // range; only the diagnostic precision narrows, never the value.
        #[allow(clippy::cast_possible_truncation)]
        let pct = (busy / total * 100.0) as f32;
        return Some(pct.clamp(0.0, 100.0));
    }

    // Format: "45" or "45%" or "45.0"
    if let Ok(pct) = trimmed.trim_end_matches('%').trim().parse::<f32>() {
        return Some(pct.clamp(0.0, 100.0));
    }

    None
}

/// Parses a debugfs utilization percentage.
/// Supports "42" or "utilization=42".
#[must_use]
pub fn parse_debugfs_utilization(content: &str) -> Option<f32> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=')
            && k.trim().eq_ignore_ascii_case("utilization")
            && let Ok(num) = v.trim().trim_end_matches('%').parse::<f32>()
        {
            return Some(num.clamp(0.0, 100.0));
        }
        if let Ok(num) = trimmed.trim_end_matches('%').parse::<f32>() {
            return Some(num.clamp(0.0, 100.0));
        }
    }
    None
}

/// Parses DRM GPU busy percentage from /sys/class/drm/card*/`device/gpu_busy_percent`.
#[must_use]
pub fn parse_drm_busy_percent(content: &str) -> Option<f32> {
    content
        .trim()
        .trim_end_matches('%')
        .parse::<f32>()
        .ok()
        .map(|p| p.clamp(0.0, 100.0))
}

/// Discovers and reads real GPU utilization from system interfaces.
/// Never substitutes clock frequency as utilization.
/// Returns None if meaningful utilization is unavailable.
#[must_use]
pub fn sample_gpu_utilization() -> Option<f32> {
    sample_gpu_from_paths(
        Path::new("/sys/class/devfreq"),
        Path::new("/sys/class/drm"),
        Path::new("/sys/kernel/debug"),
    )
}

/// Internal path-parameterized GPU utilization sampler for testability.
#[must_use]
pub fn sample_gpu_from_paths(
    devfreq_root: &Path,
    drm_root: &Path,
    debugfs_root: &Path,
) -> Option<f32> {
    // 1. Check generic GPU devfreq entries
    if let Ok(entries) = fs::read_dir(devfreq_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains("gpu") {
                let load_path = entry.path().join("load");
                if let Ok(content) = fs::read_to_string(&load_path)
                    && let Some(pct) = parse_devfreq_load(&content)
                {
                    return Some(pct);
                }
            }
        }
    }

    // 2. Check DRM busy percent paths
    let drm_paths = [
        drm_root.join("card0/device/gpu_busy_percent"),
        drm_root.join("card1/device/gpu_busy_percent"),
    ];
    for path in &drm_paths {
        if let Ok(content) = fs::read_to_string(path)
            && let Some(pct) = parse_drm_busy_percent(&content)
        {
            return Some(pct);
        }
    }

    // 3. Check debugfs utilization paths
    let debug_paths = [debugfs_root.join("dri/0/gpu_usage")];
    for path in &debug_paths {
        if let Ok(content) = fs::read_to_string(path)
            && let Some(pct) = parse_debugfs_utilization(&content)
        {
            return Some(pct);
        }
    }

    // Never substitute clock frequency as utilization!
    None
}

/// Frames per second over `seconds`, rounded to the nearest whole frame.
fn frames_per_second(frames: u32, seconds: f32) -> u32 {
    // `frames` counts the `update` calls since the last half-second metrics
    // refresh, so the widening cast is exact on any display (a few hundred
    // frames at most) and the narrowing `as` saturates exactly like the
    // historical cast did.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
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

/// Performance statistics overlay (toggled with '-').
pub struct PerfOverlay {
    visible: bool,
    last_update: Instant,
    accumulated_frames: u32,
    accumulated_time: f32,
    cpu_sampler: CpuSampler,
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

        let cpu_pct = self.cpu_sampler.sample();
        let gpu_pct = sample_gpu_utilization();

        let cpu_str = cpu_pct.map_or_else(|| "CPU N/A".to_string(), |pct| format!("CPU {pct:.0}%"));
        let gpu_str = gpu_pct.map_or_else(|| "GPU N/A".to_string(), |pct| format!("GPU {pct:.0}%"));

        // Example presentation: CPU 42%   GPU 61%   FPS 30
        self.cached_text = format!("{cpu_str}   {gpu_str}   FPS {fps}");
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

        let char_count = char_count(&self.cached_text);
        let pad_x = 4.0;
        let pad_y = 3.0;
        let text_w = char_count * 8.0;
        let text_h = 8.0;

        let x0 = 4.0;
        let y0 = 4.0;
        let x1 = x0 + text_w + pad_x * 2.0;
        let y1 = y0 + text_h + pad_y * 2.0;

        // Dark background box to ensure readability over any 3D scene surface
        add_rect(
            &mut self.cached_vertices,
            x0,
            y0,
            x1,
            y1,
            [0.06, 0.06, 0.05],
        );

        // Compact text at top-left
        draw_text(
            &mut self.cached_vertices,
            &self.cached_text,
            x0 + pad_x,
            y0 + pad_y,
            1.0,
            [0.92, 0.92, 0.88],
        );
    }
}

#[cfg(test)]
mod tests;

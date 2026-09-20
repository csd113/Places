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
    pub fn new(total: u64, idle: u64) -> Self {
        Self { total, idle }
    }
}

/// Parses total and idle ticks from the first line of Linux /proc/stat.
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
pub fn calculate_cpu_percentage(prev: &CpuSample, curr: &CpuSample) -> Option<f32> {
    let delta_total = curr.total.saturating_sub(prev.total);
    let delta_idle = curr.idle.saturating_sub(prev.idle);
    if delta_total > 0 {
        let delta_busy = delta_total.saturating_sub(delta_idle);
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
    pub fn new() -> Self {
        Self {
            last_sample: None,
            stat_path: "/proc/stat",
        }
    }

    pub fn with_path(path: &'static str) -> Self {
        Self {
            last_sample: None,
            stat_path: path,
        }
    }

    pub fn sample(&mut self) -> Option<f32> {
        // 1. First attempt reading Linux /proc/stat (PocketCHIP target platform)
        if let Ok(content) = fs::read_to_string(self.stat_path) {
            if let Some(curr) = parse_proc_stat(&content) {
                let pct = if let Some(prev) = self.last_sample {
                    calculate_cpu_percentage(&prev, &curr)
                } else {
                    None
                };
                self.last_sample = Some(curr);
                return pct;
            }
        }

        // 2. macOS fallback via Mach kernel host_statistics64 (for local dev/testing)
        #[cfg(target_os = "macos")]
        {
            if let Some(curr) = sample_macos_cpu() {
                let pct = if let Some(prev) = self.last_sample {
                    calculate_cpu_percentage(&prev, &curr)
                } else {
                    None
                };
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
    extern "C" {
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
        let mut count =
            (std::mem::size_of::<HostCpuLoadInfo>() / std::mem::size_of::<i32>()) as u32;
        let host = mach_host_self();
        if host_statistics64(host, HOST_CPU_LOAD_INFO, &mut info, &mut count) == 0 {
            let user = info.cpu_ticks[0] as u64;
            let system = info.cpu_ticks[1] as u64;
            let idle = info.cpu_ticks[2] as u64;
            let nice = info.cpu_ticks[3] as u64;
            let total = user + system + idle + nice;
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
/// - "busy_time total_time" or "busy_time / total_time"
pub fn parse_devfreq_load(content: &str) -> Option<f32> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Format: "45@500000000"
    if let Some((load_part, _)) = trimmed.split_once('@') {
        if let Ok(pct) = load_part.trim().trim_end_matches('%').parse::<f32>() {
            return Some(pct.clamp(0.0, 100.0));
        }
    }

    // Format: "busy_time total_time" or "busy_time / total_time"
    let parts: Vec<&str> = trimmed
        .split(|c: char| c.is_whitespace() || c == '/')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() >= 2 {
        if let (Ok(busy), Ok(total)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
            if total > 0.0 {
                let pct = (busy / total * 100.0) as f32;
                return Some(pct.clamp(0.0, 100.0));
            }
        }
    }

    // Format: "45" or "45%" or "45.0"
    if let Ok(pct) = trimmed.trim_end_matches('%').trim().parse::<f32>() {
        return Some(pct.clamp(0.0, 100.0));
    }

    None
}

/// Parses Mali driver utilization from /sys/class/misc/mali0/device/utilization or debugfs.
/// Supports "42" or "utilization=42".
pub fn parse_mali_utilization(content: &str) -> Option<f32> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim().eq_ignore_ascii_case("utilization") {
                if let Ok(num) = v.trim().trim_end_matches('%').parse::<f32>() {
                    return Some(num.clamp(0.0, 100.0));
                }
            }
        }
        if let Ok(num) = trimmed.trim_end_matches('%').parse::<f32>() {
            return Some(num.clamp(0.0, 100.0));
        }
    }
    None
}

/// Parses DRM GPU busy percentage from /sys/class/drm/card*/device/gpu_busy_percent.
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
pub fn sample_gpu_utilization() -> Option<f32> {
    sample_gpu_from_paths(
        Path::new("/sys/class/devfreq"),
        Path::new("/sys/class/misc"),
        Path::new("/sys/class/drm"),
        Path::new("/sys/kernel/debug"),
    )
}

/// Internal path-parameterized GPU utilization sampler for testability.
pub fn sample_gpu_from_paths(
    devfreq_root: &Path,
    misc_root: &Path,
    drm_root: &Path,
    debugfs_root: &Path,
) -> Option<f32> {
    // 1. Check devfreq directory for Lima/Mali GPU entries
    if let Ok(entries) = fs::read_dir(devfreq_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // Match GPU devfreq entries (e.g. 1c40000.gpu on Allwinner R8 / PocketCHIP, lima, mali)
            if name.contains("gpu")
                || name.contains("mali")
                || name.contains("lima")
                || name.contains("1c40000")
            {
                let load_path = entry.path().join("load");
                if let Ok(content) = fs::read_to_string(&load_path) {
                    if let Some(pct) = parse_devfreq_load(&content) {
                        return Some(pct);
                    }
                }
            }
        }
    }

    // 2. Check Mali DDK misc paths
    let mali_paths = [
        misc_root.join("mali0/device/utilization"),
        misc_root.join("mali/device/utilization"),
    ];
    for path in &mali_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(pct) = parse_mali_utilization(&content) {
                return Some(pct);
            }
        }
    }

    // 3. Check DRM busy percent paths
    let drm_paths = [
        drm_root.join("card0/device/gpu_busy_percent"),
        drm_root.join("card1/device/gpu_busy_percent"),
    ];
    for path in &drm_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(pct) = parse_drm_busy_percent(&content) {
                return Some(pct);
            }
        }
    }

    // 4. Check debugfs utilization paths
    let debug_paths = [
        debugfs_root.join("mali0/utilization"),
        debugfs_root.join("mali/utilization"),
        debugfs_root.join("dri/0/gpu_usage"),
    ];
    for path in &debug_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(pct) = parse_mali_utilization(&content) {
                return Some(pct);
            }
        }
    }

    // Never substitute clock frequency as utilization!
    None
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

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn cached_vertices(&self) -> &[Vertex] {
        &self.cached_vertices
    }

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
            (self.accumulated_frames as f32 / self.accumulated_time).round() as u32
        } else if elapsed > 0.0 {
            (self.accumulated_frames as f32 / elapsed).round() as u32
        } else {
            0
        };

        let cpu_pct = self.cpu_sampler.sample();
        let gpu_pct = sample_gpu_utilization();

        let cpu_str = match cpu_pct {
            Some(pct) => format!("CPU {:.0}%", pct),
            None => "CPU N/A".to_string(),
        };
        let gpu_str = match gpu_pct {
            Some(pct) => format!("GPU {:.0}%", pct),
            None => "GPU N/A".to_string(),
        };

        // Example presentation: CPU 42%   GPU 61%   FPS 30
        self.cached_text = format!("{cpu_str}   {gpu_str}   FPS {fps}");
        self.rebuild_cached_vertices();

        self.last_update = Instant::now();
        self.accumulated_frames = 0;
        self.accumulated_time = 0.0;
    }

    /// Rebuilds cached 2D vertices for the overlay at 480x272 top-left.
    pub fn rebuild_cached_vertices(&mut self) {
        self.cached_vertices.clear();
        if self.cached_text.is_empty() {
            return;
        }

        let char_count = self.cached_text.chars().count() as f32;
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
pub mod tests {
    use super::*;

    #[test]
    fn test_parse_proc_stat_valid() {
        let stat_content = "\
cpu  2255 34 2290 22625563 6290 127 456 0 0 0
cpu0 1132 17 1145 11312781 3145 63 228 0 0 0
intr 114930548 10 11 0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";
        let sample = parse_proc_stat(stat_content).expect("Failed to parse /proc/stat");
        assert_eq!(sample.idle, 22625563 + 6290);
        let expected_total: u64 = 2255 + 34 + 2290 + 22625563 + 6290 + 127 + 456;
        assert_eq!(sample.total, expected_total);
    }

    #[test]
    fn test_calculate_cpu_percentage() {
        let prev = CpuSample::new(1000, 800);
        let curr = CpuSample::new(1100, 850); // delta total: 100, delta idle: 50 -> busy 50%
        let pct = calculate_cpu_percentage(&prev, &curr).expect("Percentage should be calculated");
        assert!((pct - 50.0).abs() < 0.01);

        // delta total == 0 -> None
        assert_eq!(calculate_cpu_percentage(&prev, &prev), None);

        // 100% busy
        let curr_busy = CpuSample::new(1100, 800); // delta total: 100, delta idle: 0 -> busy 100%
        let pct_busy = calculate_cpu_percentage(&prev, &curr_busy).unwrap();
        assert!((pct_busy - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_devfreq_load() {
        // Plain integer percentage
        assert_eq!(parse_devfreq_load("45"), Some(45.0));
        assert_eq!(parse_devfreq_load(" 61% \n"), Some(61.0));

        // Rockchip/Allwinner kernel format: load%@freq
        assert_eq!(parse_devfreq_load("72@500000000"), Some(72.0));
        assert_eq!(parse_devfreq_load("85%@600000000"), Some(85.0));

        // busy_time total_time
        assert_eq!(parse_devfreq_load("250 1000"), Some(25.0));
        assert_eq!(parse_devfreq_load("500 / 1000"), Some(50.0));

        // Empty / invalid
        assert_eq!(parse_devfreq_load(""), None);
        assert_eq!(parse_devfreq_load("invalid"), None);
    }

    #[test]
    fn test_parse_mali_utilization() {
        assert_eq!(parse_mali_utilization("42"), Some(42.0));
        assert_eq!(parse_mali_utilization("utilization=65"), Some(65.0));
        assert_eq!(parse_mali_utilization("UTILIZATION = 80%"), Some(80.0));
        assert_eq!(parse_mali_utilization("not_a_number"), None);
    }

    #[test]
    fn test_parse_drm_busy_percent() {
        assert_eq!(parse_drm_busy_percent("90"), Some(90.0));
        assert_eq!(parse_drm_busy_percent("90%"), Some(90.0));
        assert_eq!(parse_drm_busy_percent("invalid"), None);
    }

    #[test]
    fn test_gpu_sampler_never_uses_clock_frequency() {
        // Confirm that frequency files (which would contain e.g. 500000000) are never treated as a percentage
        // and that devfreq parsing clamps to 0..100
        let freq_str = "500000000";
        // Directly parsing a frequency without load format produces clamped 100 or is not a load file.
        // But more importantly, sample_gpu_from_paths only looks for 'load' or 'utilization', never 'cur_freq'!
        let temp_dir = std::env::temp_dir().join("liminal_test_gpu_no_freq");
        let _ = fs::remove_dir_all(&temp_dir);
        let devfreq_dir = temp_dir.join("devfreq/1c40000.gpu");
        fs::create_dir_all(&devfreq_dir).unwrap();

        // Write cur_freq only (no load file)
        fs::write(devfreq_dir.join("cur_freq"), freq_str).unwrap();

        let sampled = sample_gpu_from_paths(
            &temp_dir.join("devfreq"),
            &temp_dir.join("misc"),
            &temp_dir.join("drm"),
            &temp_dir.join("debug"),
        );
        // cur_freq alone must result in None (GPU N/A)
        assert_eq!(
            sampled, None,
            "GPU sampler must never substitute clock frequency as utilization!"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sample_gpu_from_paths_with_mock_devfreq_load() {
        let temp_dir = std::env::temp_dir().join("liminal_test_gpu_mock");
        let _ = fs::remove_dir_all(&temp_dir);
        let devfreq_dir = temp_dir.join("devfreq/1c40000.gpu");
        fs::create_dir_all(&devfreq_dir).unwrap();

        fs::write(devfreq_dir.join("load"), "61\n").unwrap();

        let sampled = sample_gpu_from_paths(
            &temp_dir.join("devfreq"),
            &temp_dir.join("misc"),
            &temp_dir.join("drm"),
            &temp_dir.join("debug"),
        );
        assert_eq!(sampled, Some(61.0));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_perf_overlay_defaults_and_toggle() {
        let mut overlay = PerfOverlay::new();
        assert!(!overlay.is_visible(), "Overlay must be hidden by default");

        overlay.toggle();
        assert!(overlay.is_visible(), "Overlay must become visible on toggle");

        overlay.toggle();
        assert!(!overlay.is_visible(), "Overlay must become hidden on toggle");
    }

    #[test]
    fn test_perf_overlay_text_format_and_caching() {
        let mut overlay = PerfOverlay::new();
        overlay.set_visible(true);

        // Simulate frames with real delta times
        for _ in 0..15 {
            overlay.update(0.0333); // ~30 FPS frame time
        }

        // Force a metrics refresh
        overlay.refresh_metrics();

        let text = overlay.cached_text();
        assert!(
            text.starts_with("CPU "),
            "Text must begin with CPU: {text}"
        );
        assert!(text.contains("GPU "), "Text must contain GPU: {text}");
        assert!(text.contains("FPS "), "Text must contain FPS: {text}");

        // Cached vertices must be generated
        let verts = overlay.cached_vertices();
        assert!(!verts.is_empty(), "Cached vertices must not be empty");

        // The vertices count shouldn't change without another update interval
        let initial_count = verts.len();
        overlay.update(0.016);
        assert_eq!(
            overlay.cached_vertices().len(),
            initial_count,
            "Geometry must remain cached between update intervals"
        );
    }
}

//! Unit tests for the performance counters and overlay.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use super::*;

#[test]
fn test_parse_proc_stat_valid() {
    let stat_content = "\
cpu  2255 34 2290 22625563 6290 127 456 0 0 0
cpu0 1132 17 1145 11312781 3145 63 228 0 0 0
intr 114930548 10 11 0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";
    let sample = parse_proc_stat(stat_content).expect("Failed to parse /proc/stat");
    assert_eq!(sample.idle, 22_625_563 + 6_290);
    let expected_total: u64 = 2_255 + 34 + 2_290 + 22_625_563 + 6_290 + 127 + 456;
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

    // Vendor kernel format: load%@freq
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
    let temp_dir = std::env::temp_dir().join("places_test_gpu_no_freq");
    let _ = fs::remove_dir_all(&temp_dir);
    let devfreq_dir = temp_dir.join("devfreq/test.gpu");
    fs::create_dir_all(&devfreq_dir).unwrap();

    // Write cur_freq only (no load file)
    fs::write(devfreq_dir.join("cur_freq"), freq_str).unwrap();

    let sampled = sample_gpu_from_paths(
        &temp_dir.join("devfreq"),
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
    let temp_dir = std::env::temp_dir().join("places_test_gpu_mock");
    let _ = fs::remove_dir_all(&temp_dir);
    let devfreq_dir = temp_dir.join("devfreq/test.gpu");
    fs::create_dir_all(&devfreq_dir).unwrap();

    fs::write(devfreq_dir.join("load"), "61\n").unwrap();

    let sampled = sample_gpu_from_paths(
        &temp_dir.join("devfreq"),
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
    assert!(
        overlay.is_visible(),
        "Overlay must become visible on toggle"
    );

    overlay.toggle();
    assert!(
        !overlay.is_visible(),
        "Overlay must become hidden on toggle"
    );
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
    assert!(text.starts_with("CPU "), "Text must begin with CPU: {text}");
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

#[test]
fn test_parse_debugfs_utilization() {
    assert_eq!(parse_debugfs_utilization("42"), Some(42.0));
    assert_eq!(parse_debugfs_utilization("utilization=65"), Some(65.0));
    assert_eq!(parse_debugfs_utilization("UTILIZATION = 80%"), Some(80.0));
    assert_eq!(parse_debugfs_utilization("not_a_number"), None);
}

#[test]
fn test_startup_timeline_accumulates_deltas_and_reports_once() {
    let start = Instant::now();
    let mut timeline = StartupTimeline::begin_at(start);
    timeline.mark("first", start + Duration::from_millis(10));
    timeline.mark("second", start + Duration::from_millis(35));

    let lines = timeline.report_lines(start + Duration::from_millis(40));
    assert_eq!(lines[0], "[startup] total 40.0 ms");
    assert_eq!(lines[1], "[startup]   first: +10.0 ms (10.0 ms)");
    assert_eq!(lines[2], "[startup]   second: +25.0 ms (35.0 ms)");

    assert!(timeline.claim_report(), "the first report claims the trace");
    assert!(
        !timeline.claim_report(),
        "every later report must be a no-op"
    );
}

#[test]
fn test_startup_timeline_capacity_is_bounded() {
    let start = Instant::now();
    let mut timeline = StartupTimeline::begin_at(start);
    for _ in 0..(STARTUP_MARK_CAPACITY + 5) {
        timeline.mark("phase", start);
    }
    assert_eq!(timeline.marks.len(), STARTUP_MARK_CAPACITY);
    assert_eq!(
        timeline.report_lines(start).len(),
        STARTUP_MARK_CAPACITY + 1,
        "the total line plus one line per retained mark"
    );
}

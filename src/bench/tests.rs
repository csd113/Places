//! Unit tests for the benchmark harness.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::cast_precision_loss, clippy::float_cmp)]

use super::*;
use crate::test_support::assert_exact;

#[test]
fn parse_camera_accepts_yaw_and_optional_pitch() {
    assert_eq!(parse_camera("90"), Some((90.0, 0.0)));
    assert_eq!(parse_camera(" 90 , -15 "), Some((90.0, -15.0)));
    assert_eq!(parse_camera("abc"), None);
    assert_eq!(parse_camera("nan"), None);
    assert_eq!(parse_camera(""), None);
}

#[test]
fn parse_vsync_override_maps_human_words() {
    assert_eq!(parse_vsync_override("on"), Some(true));
    assert_eq!(parse_vsync_override("1"), Some(true));
    assert_eq!(parse_vsync_override("off"), Some(false));
    assert_eq!(parse_vsync_override("0"), Some(false));
    assert_eq!(parse_vsync_override("maybe"), None);
}

#[test]
fn parse_window_action_accepts_the_documented_set() {
    assert_eq!(
        parse_window_action("resize:800x450"),
        Some(WindowAction::Resize(800, 450))
    );
    assert_eq!(
        parse_window_action(" resize: 640 x 360 "),
        Some(WindowAction::Resize(640, 360))
    );
    assert_eq!(
        parse_window_action("Minimize"),
        Some(WindowAction::Minimize)
    );
    assert_eq!(parse_window_action("restore"), Some(WindowAction::Restore));
    for malformed in [
        "",
        "resize",
        "resize:0x10",
        "resize:10x0",
        "resize:axb",
        "maximize",
    ] {
        assert_eq!(parse_window_action(malformed), None, "{malformed}");
    }
}

#[test]
fn a_scripted_window_cycle_selects_actions_by_frame() {
    let mut bench = Bench::new();
    bench.window_cycle = vec![
        (3, WindowAction::Resize(800, 450)),
        (6, WindowAction::Minimize),
        (9, WindowAction::Restore),
    ];
    assert_eq!(
        bench.window_cycle_at(3),
        Some(WindowAction::Resize(800, 450))
    );
    assert_eq!(bench.window_cycle_at(6), Some(WindowAction::Minimize));
    assert_eq!(bench.window_cycle_at(9), Some(WindowAction::Restore));
    assert_eq!(bench.window_cycle_at(4), None);
}

#[test]
fn timing_summary_handles_empty_and_single_samples() {
    let empty = TimingSummary::from_samples(&mut []);
    assert_exact(empty.median_ms, 0.0);
    assert_exact(TimingSummary::fps_from_ms(0.0), 0.0);

    let single = TimingSummary::from_samples(&mut [16.0]);
    assert_exact(single.median_ms, 16.0);
    assert_exact(single.p95_ms, 16.0);
    assert_exact(single.min_ms, 16.0);
    assert_exact(single.max_ms, 16.0);
}

#[test]
fn timing_summary_percentiles_use_nearest_rank() {
    let mut samples: Vec<f32> = (1..=100).map(|value| value as f32).collect();
    let summary = TimingSummary::from_samples(&mut samples);
    assert!((summary.median_ms - 51.0).abs() < 0.01);
    assert!((summary.p95_ms - 95.0).abs() < 0.01);
    assert!((summary.p99_ms - 99.0).abs() < 0.01);
    assert_exact(summary.min_ms, 1.0);
    assert_exact(summary.max_ms, 100.0);
}

#[test]
fn disabled_bench_records_nothing_and_holds_no_file() {
    let saved = std::env::var(BENCH_ENV).ok();
    // SAFETY: no other test in this binary reads PLACES_BENCH.
    unsafe { std::env::remove_var(BENCH_ENV) };
    let mut bench = Bench::new();
    assert!(!bench.enabled());
    let now = Instant::now();
    bench.record_frame(now, now, now, now, now, RenderStats::default());
    assert!(bench.frames.is_empty());
    assert!(bench.csv.is_none());
    assert!(!bench.is_complete());
    if let Some(value) = saved {
        unsafe { std::env::set_var(BENCH_ENV, value) };
    }
}

#[test]
fn a_huge_warmup_counter_never_overflows_or_records() {
    let mut bench = Bench {
        config: BenchConfig {
            enabled: true,
            ..BenchConfig::default()
        },
        csv: None,
        warmup_remaining: u64::MAX,
        limit_remaining: None,
        recorded: 0,
        last_begin: None,
        frames: Vec::new(),
        reported_swap_interval: None,
        quality_cycle: Vec::new(),
        graphics_cycle: Vec::new(),
        window_cycle: Vec::new(),
    };
    let now = Instant::now();
    for _ in 0..3 {
        bench.record_frame(now, now, now, now, now, RenderStats::default());
    }
    assert_eq!(bench.recorded, 0);
    assert!(bench.frames.is_empty());
}

#[test]
fn frame_limits_and_completion_are_exact() {
    let mut bench = Bench {
        config: BenchConfig {
            enabled: true,
            ..BenchConfig::default()
        },
        csv: None,
        warmup_remaining: 1,
        limit_remaining: Some(2),
        recorded: 0,
        last_begin: None,
        frames: Vec::new(),
        reported_swap_interval: None,
        quality_cycle: Vec::new(),
        graphics_cycle: Vec::new(),
        window_cycle: Vec::new(),
    };
    let now = Instant::now();
    for _ in 0..5 {
        bench.record_frame(now, now, now, now, now, RenderStats::default());
    }
    assert_eq!(bench.frames.len(), 2);
    assert!(bench.is_complete());
}

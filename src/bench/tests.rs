//! Unit tests for the benchmark harness.

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
    // SAFETY: no other test in this binary reads LIMINAL_BENCH.
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
    };
    let now = Instant::now();
    for _ in 0..5 {
        bench.record_frame(now, now, now, now, now, RenderStats::default());
    }
    assert_eq!(bench.frames.len(), 2);
    assert!(bench.is_complete());
}

//! Unit tests for the benchmark harness.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::float_cmp,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

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
fn diagnostic_render_eye_requires_exactly_three_finite_coordinates() {
    assert_eq!(
        parse_camera_position(" -16.7 , 1.6, -30.8 "),
        Some([-16.7, 1.6, -30.8])
    );
    for invalid in [
        "", "1,2", "1,2,3,4", "1,2,3,", "nan,2,3", "1,inf,3", "1,2,-inf",
    ] {
        assert_eq!(parse_camera_position(invalid), None, "{invalid}");
    }
}

#[test]
fn deterministic_capture_delta_rejects_invalid_or_unbounded_steps() {
    assert_eq!(
        parse_fixed_delta_seconds(" 0.016666667 "),
        Some(0.016_666_668)
    );
    assert_eq!(parse_fixed_delta_seconds(".1"), Some(0.1));
    for invalid in ["", "0", "-0", "-.01", ".10001", "nan", "inf", "-inf", "1"] {
        assert_eq!(parse_fixed_delta_seconds(invalid), None, "{invalid}");
    }
}

#[test]
fn diagnostic_eye_and_fixed_time_are_inert_without_benchmark_enablement() {
    let mut bench = Bench::from_config(BenchConfig {
        camera_position: Some([1.0, 2.0, 3.0]),
        fixed_delta_seconds: Some(0.02),
        ..BenchConfig::default()
    });
    assert_eq!(bench.camera_position_override(), None);
    assert_eq!(bench.fixed_delta_seconds(), None);
    assert!(!bench.set_camera_override([4.0, 5.0, 6.0], 90.0, -15.0));
    assert_eq!(bench.camera_override(), None);
}

#[test]
fn independent_render_eye_changes_reject_invalid_updates_atomically() {
    let mut bench = Bench::from_config(BenchConfig {
        enabled: true,
        fixed_delta_seconds: Some(0.02),
        ..BenchConfig::default()
    });
    assert!(bench.set_camera_override([-16.7, 1.6, -30.8], 0.0, -20.0));
    for (position, yaw, pitch) in [
        ([f32::NAN, 1.6, -30.8], 0.0, -20.0),
        ([-16.7, 1.6, -30.8], f32::INFINITY, -20.0),
        ([-16.7, 1.6, -30.8], 0.0, f32::NEG_INFINITY),
    ] {
        assert!(!bench.set_camera_override(position, yaw, pitch));
        assert_eq!(bench.camera_position_override(), Some([-16.7, 1.6, -30.8]));
        assert_eq!(bench.camera_override(), Some((0.0, -20.0)));
    }
    assert_eq!(bench.fixed_delta_seconds(), Some(0.02));
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
    let mut samples: Vec<f32> = (1_i32..=100_i32)
        .map(|value| crate::test_support::exact_f32(value))
        .collect();
    let summary = TimingSummary::from_samples(&mut samples);
    assert!((summary.median_ms - 51.0).abs() < 0.01);
    assert!((summary.p95_ms - 95.0).abs() < 0.01);
    assert!((summary.p99_ms - 99.0).abs() < 0.01);
    assert_exact(summary.min_ms, 1.0);
    assert_exact(summary.max_ms, 100.0);
}

#[test]
fn disabled_bench_records_nothing_and_holds_no_file() {
    // Inject the disabled configuration rather than mutating the process
    // environment while other tests and their workers can read it.
    let mut bench = Bench::from_config(BenchConfig::default());
    assert!(!bench.enabled());
    let now = Instant::now();
    bench.record_frame(now, now, now, now, now, RenderStats::default());
    assert!(bench.frames.is_empty());
    assert!(bench.csv.is_none());
    assert!(!bench.is_complete());
}

#[test]
fn a_failed_csv_write_disables_the_stream_and_keeps_frame_samples() -> std::io::Result<()> {
    let mut bench = Bench::from_config(BenchConfig {
        enabled: true,
        ..BenchConfig::default()
    });
    // A read-only handle reliably fails writes without depending on filesystem
    // permissions, disk capacity, or a special platform device.
    bench.csv = Some(std::fs::File::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/Cargo.toml"
    ))?);
    let now = Instant::now();
    bench.record_frame(now, now, now, now, now, RenderStats::default());
    assert!(
        bench.csv.is_none(),
        "a failed stream must not retry every frame"
    );
    assert_eq!(
        bench.frames.len(),
        1,
        "in-memory samples survive a CSV failure"
    );
    Ok(())
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
        loop_ms: 0.0,
        frames: Vec::new(),
        reported_swap_interval: None,
        quality_cycle: Vec::new(),
        graphics_cycle: Vec::new(),
        window_cycle: Vec::new(),
        lighting_sequence: lighting_sequence::LightingSequence::default(),
    };
    let now = Instant::now();
    for _ in 0_i32..3_i32 {
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
        loop_ms: 0.0,
        frames: Vec::new(),
        reported_swap_interval: None,
        quality_cycle: Vec::new(),
        graphics_cycle: Vec::new(),
        window_cycle: Vec::new(),
        lighting_sequence: lighting_sequence::LightingSequence::default(),
    };
    let now = Instant::now();
    for _ in 0_i32..5_i32 {
        bench.record_frame(now, now, now, now, now, RenderStats::default());
    }
    assert_eq!(bench.frames.len(), 2);
    assert!(bench.is_complete());
}

/// AUD-006: excluded loading intervals advance the cadence baseline every
/// loop, so the first steady-state sample after a load reports the adjacent
/// loop instead of the whole gap.
#[test]
fn excluded_loading_intervals_do_not_become_one_long_cadence_sample() {
    let mut bench = Bench {
        config: BenchConfig {
            enabled: true,
            ..BenchConfig::default()
        },
        csv: None,
        warmup_remaining: 0,
        limit_remaining: None,
        recorded: 0,
        last_begin: None,
        loop_ms: 0.0,
        frames: Vec::new(),
        reported_swap_interval: None,
        quality_cycle: Vec::new(),
        graphics_cycle: Vec::new(),
        window_cycle: Vec::new(),
        lighting_sequence: lighting_sequence::LightingSequence::default(),
    };
    let t0 = Instant::now();
    let ms = std::time::Duration::from_millis;
    // One ready loop: sampled.
    bench.begin_frame(t0);
    bench.record_frame(t0, t0, t0, t0, t0 + ms(16), RenderStats::default());
    // Then loading loops keep advancing the cadence baseline, including a GPU
    // upload stall inside the last loading loop.
    for step in [32_u64, 48, 64, 3064] {
        bench.begin_frame(t0 + ms(step));
    }
    // The next ready loop is adjacent to the last loading loop: its cadence is
    // 16 ms, not the 3 s loading interval.
    bench.begin_frame(t0 + ms(3080));
    bench.record_frame(
        t0 + ms(3080),
        t0 + ms(3080),
        t0 + ms(3088),
        t0 + ms(3096),
        t0 + ms(3100),
        RenderStats::default(),
    );
    assert_eq!(bench.frames.len(), 2);
    let after_load = bench
        .frames
        .last()
        .map_or(0.0, |frame| frame.timings.loop_ms);
    assert_eq!(after_load, 16.0, "the sample reports the adjacent loop");
    assert!(
        after_load < 3000.0,
        "the loading gap must not become one cadence sample"
    );
}

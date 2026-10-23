//! Essential resource-measurement regression: streaming omits retained records,
//! preserves raw CSV and keeps the existing cap without stranding a finite run.
//! Two tiny rows and a seeded boundary counter cost no assets, GPU or bake.
use super::{Bench, BenchConfig, BenchSwitches, MAX_RECORDED_FRAMES};
use crate::engine::render::RenderStats;
use std::time::Instant;

#[test]
fn streaming_csv_preserves_record_cap_without_retained_frame_growth() -> Result<(), String> {
    let path =
        std::env::temp_dir().join(format!("places-stream-records-{}.csv", std::process::id()));
    let mut switches = BenchSwitches::default();
    switches.insert(BenchSwitches::STREAM_ONLY);
    let mut bench = Bench::from_config(BenchConfig {
        enabled: true,
        out_path: Some(path.clone()),
        switches,
        ..BenchConfig::default()
    });
    let at = Instant::now();
    bench.record_frame(at, at, at, at, at, RenderStats::default());
    assert_eq!(bench.recorded, 1, "CSV records still advance");
    assert!(
        bench.frames.is_empty(),
        "a resource run retains no duplicate frame records"
    );
    bench.recorded = u64::try_from(MAX_RECORDED_FRAMES)
        .map_err(|e| e.to_string())?
        .saturating_sub(1);
    bench.limit_remaining = Some(2);
    bench.record_frame(at, at, at, at, at, RenderStats::default());
    bench.record_frame(at, at, at, at, at, RenderStats::default());
    assert_eq!(
        bench.recorded,
        u64::try_from(MAX_RECORDED_FRAMES).map_err(|e| e.to_string())?,
        "the prior record cap remains enforced"
    );
    assert!(
        bench.frames.is_empty(),
        "the boundary does not allocate retained records"
    );
    assert_eq!(
        bench.limit_remaining,
        Some(0),
        "a longer finite run still reaches its requested endpoint"
    );
    drop(bench);
    let rows = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    assert_eq!(
        rows.lines().count(),
        3,
        "header plus the two admitted rows survive on disk"
    );
    Ok(())
}

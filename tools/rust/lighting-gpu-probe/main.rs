//! Explicit bounded GPU query economics; never called by baking, tests or CI.
mod adversarial;
mod device;
use places::engine::lighting::transport::diagnostics::gpu_oracle::{Oracle, Snapshot};
use serde_json::json;
use std::{fs, io::Write as _, path::PathBuf, time::Instant};

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn measure(
    oracle: &Oracle,
    backend: &mut Result<device::Gpu, String>,
    repeats: usize,
    fault: &str,
    cancel_after: Option<usize>,
) -> Result<(Vec<serde_json::Value>, bool), Box<dyn std::error::Error>> {
    let reference = oracle.batch(None, true, 12)?;
    if reference
        .iter()
        .zip(&oracle.snapshot().expected)
        .any(|((hit, _), expected)| u32::from(*hit) != *expected)
    {
        return Err("captured CPU reference is stale".into());
    }
    let mut rows = Vec::new();
    let mut cancelled = false;
    for repeat in 0..repeats {
        for mode in if repeat.is_multiple_of(2) {
            ["cpu", "gated", "verify-all"]
        } else {
            ["verify-all", "gated", "cpu"]
        } {
            let run_started = Instant::now();
            let mut transfer = json!(null);
            let mut failure = None;
            let mut candidates = None;
            if mode != "cpu" {
                match backend.as_ref() {
                    Ok(gpu) => match gpu.query(oracle.snapshot(), fault, cancel_after) {
                        Ok((values, times)) => {
                            candidates = Some(values);
                            transfer = times;
                        }
                        Err(error) if error == "cancelled" => {
                            cancelled = true;
                            break;
                        }
                        Err(error) => {
                            failure = Some(error);
                        }
                    },
                    Err(error) => failure = Some(error.clone()),
                }
            }
            if let Some(error) = &failure {
                *backend = Err(error.clone());
            }
            let verification_started = Instant::now();
            let values = oracle.batch(candidates.as_deref(), mode != "gated", 12)?;
            let verification_ms = ms(verification_started);
            let query_total_ms = ms(run_started);
            let mismatches: Vec<_> = values
                .iter()
                .zip(&reference)
                .enumerate()
                .filter_map(|(i, (a, b))| (a.0 != b.0).then_some(i))
                .collect();
            let raw_false_hits = candidates.as_ref().map_or(0, |v| {
                v.iter()
                    .zip(&reference)
                    .filter(|(a, b)| a.0.is_some() && !b.0)
                    .count()
            });
            let raw_false_misses = candidates.as_ref().map_or(0, |v| {
                v.iter()
                    .zip(&reference)
                    .filter(|(a, b)| a.0.is_none() && b.0)
                    .count()
            });
            let ambiguous = candidates
                .as_ref()
                .map_or(0, |entries| entries.iter().filter(|entry| entry.1).count());
            let exact_bytes: Vec<u8> = values.iter().map(|value| u8::from(value.0)).collect();
            let exact_sha256 = places::engine::package::hash::sha256_hex(&exact_bytes);
            let fallback_count = values.iter().filter(|v| v.1).count();
            rows.push(json!({"repeat":repeat,"mode":mode,"query_total_ms":query_total_ms,"certification_and_fallback_ms":verification_ms,"transfer":transfer,"full_cpu_fallbacks":fallback_count,"exact_candidate_certificates":values.len().saturating_sub(fallback_count),"ambiguous_rays":ambiguous,"raw_false_hits":raw_false_hits,"raw_false_misses":raw_false_misses,"exact_mismatches":mismatches,"exact_result_sha256":exact_sha256,"device_failure":failure}));
            if !mismatches.is_empty() {
                return Err("exact correctness gate failed".into());
            }
        }
        if cancelled {
            break;
        }
    }

    Ok((rows, cancelled))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let input = PathBuf::from(arguments.first().ok_or("snapshot path required")?);
    let output = PathBuf::from(arguments.get(1).ok_or("new report path required")?);
    let value = |name: &str| {
        arguments
            .iter()
            .position(|a| a == name)
            .and_then(|i| arguments.get(i.saturating_add(1)))
    };
    let tile = value("--tile").map_or(Ok(16_384_usize), |v| v.parse())?;
    let repeats = value("--repeats").map_or(Ok(5_usize), |v| v.parse())?;
    let fault = value("--fault").map_or("none", String::as_str);
    let cancel_after = value("--cancel-after")
        .map(|v| v.parse::<usize>())
        .transpose()?;
    if ![
        "none", "setup", "dispatch", "readback", "identity", "destroy",
    ]
    .contains(&fault)
        || !(1..=16_384).contains(&tile)
        || !(1..=20).contains(&repeats)
    {
        return Err("bounded tile/repeats or known fault required".into());
    }
    if input != std::path::Path::new("@adversarial")
        && fs::metadata(&input)?.len() > 64 * 1_024 * 1_024
    {
        return Err("snapshot exceeds 64 MiB".into());
    }
    let snapshot: Snapshot = if input == std::path::Path::new("@adversarial") {
        adversarial::snapshot()?
    } else {
        serde_json::from_slice(&fs::read(&input)?)?
    };
    let read_ms = ms(started);
    let oracle_started = Instant::now();
    let oracle = Oracle::new(snapshot)?;
    let oracle_ms = ms(oracle_started);
    let setup = Instant::now();
    let mut backend = if fault == "setup" {
        Err("injected unavailable device".into())
    } else {
        device::Gpu::new(oracle.snapshot(), tile)
    };
    let setup_ms = ms(setup);
    let metadata = backend.as_ref().map(device::Gpu::metadata).ok();
    let (rows, cancelled) = measure(&oracle, &mut backend, repeats, fault, cancel_after)?;
    let report = json!({"schema":1_u32,"input":input,"snapshot_read_ms":read_ms,"cpu_oracle_rebuild_ms":oracle_ms,"device_setup_geometry_pipeline_ms":setup_ms,"device":metadata,"tile_rays":tile,"rays":oracle.snapshot().rays.len(),"triangles":oracle.snapshot().triangles.len(),"excluded_nonopaque_triangles":oracle.snapshot().excluded_nonopaque_triangles,"fault":fault,"cancelled":cancelled,"rows":rows,"process_to_report_ms":ms(started),"scope":"filtered opaque any-hit only; exact CPU certificates for hits, full CPU for all misses/ambiguity/failure; no production solver integration or whole-bake acceleration claim"});
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    destination.write_all(&serde_json::to_vec_pretty(&report)?)?;
    destination.write_all(b"\n")?;
    std::io::stdout().write_all(&serde_json::to_vec(&report)?)?;
    Ok(())
}

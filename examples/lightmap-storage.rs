//! Explicit disk-codec pilot: existing META.json ATLAS.ktx2 OUTPUT.pllc.
//! Does not bake, alter exposure, or replace installed packages.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let meta_path = arguments.next().ok_or("missing META.json")?;
    let atlas_path = arguments.next().ok_or("missing ATLAS.ktx2")?;
    let output = arguments.next().ok_or("missing OUTPUT.pllc")?;
    if arguments.next().is_some() {
        return Err("unexpected argument".to_string());
    }
    let metadata = std::fs::read(&meta_path).map_err(|error| error.to_string())?;
    let meta: places::package::lightmaps::LightmapsMeta =
        serde_json::from_slice(&metadata).map_err(|error| error.to_string())?;
    let bytes = std::fs::read(&atlas_path).map_err(|error| error.to_string())?;
    let image = places::package::ktx2::read_rgba16f(&bytes)?;
    meta.validate(&image)?;
    let planes = image.levels.first().ok_or("missing atlas pixels")?;
    let groups = u32::try_from(meta.switchable_lights.len().saturating_add(1))
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let (encoded, stats) = places::package::compact_lightmaps::encode_with_stats(
        meta.page_edge,
        meta.page_count,
        groups,
        planes,
    )?;
    let encode_seconds = started.elapsed().as_secs_f64();
    let decode_started = Instant::now();
    let decoded = places::package::compact_lightmaps::decode(&encoded)?;
    let decode_seconds = decode_started.elapsed().as_secs_f64();
    if (decoded.edge, decoded.page_count, decoded.group_count)
        != (meta.page_edge, meta.page_count, groups)
    {
        return Err("codec shape changed".to_string());
    }
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(&output))
        .map_err(|error| error.to_string())?;
    destination
        .write_all(&encoded)
        .map_err(|error| error.to_string())?;
    let receipt = serde_json::json!({
        "input_bytes": planes.len(), "encoded_bytes": encoded.len(),
        "compact_blocks": stats.compact_blocks, "exact_blocks": stats.exact_blocks,
        "encode_seconds": encode_seconds, "decode_seconds": decode_seconds,
        "expanded_gpu_bytes": decoded.rgba16f_planes.len(),
    });
    writeln!(io::stdout().lock(), "{receipt}").map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _diagnostic = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}

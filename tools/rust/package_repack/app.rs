//! Investigation-only, exact-payload ZIP repacker using the project's codec.
//! Usage: cargo run --release --bin package-repack -- INPUT OUTPUT LEVEL
//! Decode-only timing: package-repack --read-only INPUT
//! This does not rebuild a map or change any decoded record or manifest.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use places::engine::package::{MAX_LIGHTMAP_ATLAS_BYTES, MAX_TOTAL_BYTES, PackageReader};

fn entry_limit(name: &str) -> u64 {
    if name == "manifest.json" {
        places::engine::package::MAX_MANIFEST_BYTES
    } else if name == "semantics.json" {
        places::engine::package::MAX_SEMANTICS_BYTES
    } else if name == "materials.json" {
        places::engine::package::MAX_MATERIALS_BYTES
    } else if name.ends_with(".lightmaps.ktx2") {
        MAX_LIGHTMAP_ATLAS_BYTES
    } else if Path::new(name).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("props") || extension.eq_ignore_ascii_case("mesh")
    }) {
        places::engine::package::MAX_BINARY_BYTES
    } else if name.ends_with(".lightmaps.json") {
        places::engine::package::MAX_LIGHTMAP_METADATA_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("collision"))
    {
        places::engine::package::MAX_COLLISION_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("navigation"))
    {
        places::engine::package::MAX_NAVIGATION_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        places::engine::package::MAX_TEXTURE_BYTES
    } else {
        places::engine::package::MAX_ENTRY_BYTES
    }
}

fn read_only(path: &Path) -> io::Result<()> {
    let mut reader = PackageReader::new(File::open(path)?)
        .map_err(|error| io::Error::other(format!("invalid input archive: {error}")))?;
    let names = reader.names().to_vec();
    let mut read_time = Duration::ZERO;
    let mut bytes = 0_u64;
    for name in names {
        let started = Instant::now();
        let payload = reader
            .read_entry(&name, entry_limit(&name))
            .map_err(io::Error::other)?;
        read_time = read_time.saturating_add(started.elapsed());
        bytes = bytes.saturating_add(u64::try_from(payload.len()).unwrap_or(u64::MAX));
    }
    let summary = serde_json::json!({"bytes": bytes, "read_seconds": read_time.as_secs_f64()});
    writeln!(io::stdout().lock(), "{summary}")?;
    Ok(())
}

fn execute() -> io::Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let input = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing INPUT"))?;
    if input == "--read-only" {
        let path = arguments
            .next()
            .ok_or_else(|| io::Error::other("missing INPUT"))?;
        if arguments.next().is_some() {
            return Err(io::Error::other("unexpected argument"));
        }
        return read_only(Path::new(&path));
    }
    let output = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing OUTPUT"))?;
    let level = arguments
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse::<i64>().ok()))
        .filter(|value| (1..=9).contains(value))
        .ok_or_else(|| io::Error::other("LEVEL must be 1 through 9"))?;
    if arguments.next().is_some() {
        return Err(io::Error::other("unexpected argument"));
    }
    let mut reader = PackageReader::new(File::open(&input)?)
        .map_err(|error| io::Error::other(format!("invalid input archive: {error}")))?;
    let names = reader.names().to_vec();
    // Exclusive creation protects the baseline and any earlier evidence.
    let output_file = File::options().write(true).create_new(true).open(&output)?;
    let mut writer = zip::ZipWriter::new(output_file);
    let mut read_time = Duration::ZERO;
    let mut pack_time = Duration::ZERO;
    for name in names {
        // This tool accepts already-validated experiment inputs. The existing
        // aggregate bound still applies; no archive-size bound is increased.
        let limit = entry_limit(&name);
        let read_start = Instant::now();
        let bytes = reader.read_entry(&name, limit).map_err(io::Error::other)?;
        read_time = read_time.saturating_add(read_start.elapsed());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(level))
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        let pack_start = Instant::now();
        writer.start_file(name, options)?;
        writer.write_all(&bytes)?;
        pack_time = pack_time.saturating_add(pack_start.elapsed());
    }
    let finish_start = Instant::now();
    let finished_file = writer.finish()?;
    pack_time = pack_time.saturating_add(finish_start.elapsed());
    finished_file.sync_all()?;
    let bytes = finished_file.metadata()?.len();
    let summary = serde_json::json!({
        "bytes": bytes, "deflate_level": level, "aggregate_cap": MAX_TOTAL_BYTES,
        "path": Path::new(&output), "read_seconds": read_time.as_secs_f64(),
        "pack_seconds": pack_time.as_secs_f64()
    });
    writeln!(io::stdout().lock(), "{summary}")?;
    Ok(())
}

pub fn run() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _result = writeln!(io::stderr().lock(), "package-repack: {error}");
            ExitCode::FAILURE
        }
    }
}

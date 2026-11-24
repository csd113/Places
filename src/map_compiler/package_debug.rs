//! Explicit bounded package reads and exact-record Deflate copies for tooling.
//! They never prepare a map or change input records.

use std::fs::File;
use std::io::{self, Write as _};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::engine::package::{MAX_LIGHTMAP_ATLAS_BYTES, MAX_TOTAL_BYTES, Manifest, PackageReader};

fn entry_limit(name: &str) -> u64 {
    if name == "manifest.json" {
        crate::engine::package::MAX_MANIFEST_BYTES
    } else if name == "semantics.json" {
        crate::engine::package::MAX_SEMANTICS_BYTES
    } else if name == "materials.json" {
        crate::engine::package::MAX_MATERIALS_BYTES
    } else if name.ends_with(".lightmaps.ktx2") || name.ends_with(".lightmaps.pllc") {
        MAX_LIGHTMAP_ATLAS_BYTES
    } else if Path::new(name).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("props") || extension.eq_ignore_ascii_case("mesh")
    }) {
        crate::engine::package::MAX_BINARY_BYTES
    } else if name.ends_with(".record") {
        // Project records use content-addressed physical names; typed limits
        // are checked by the regular package validator.
        crate::engine::package::MAX_BINARY_BYTES
    } else if name.ends_with(".lightmaps.json") {
        crate::engine::package::MAX_LIGHTMAP_METADATA_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("collision"))
    {
        crate::engine::package::MAX_COLLISION_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("navigation"))
    {
        crate::engine::package::MAX_NAVIGATION_BYTES
    } else if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        crate::engine::package::MAX_TEXTURE_BYTES
    } else {
        crate::engine::package::MAX_ENTRY_BYTES
    }
}

pub(super) fn read_only(path: &Path) -> io::Result<()> {
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

pub(super) fn repack(input: &Path, output: &Path, level: i64) -> io::Result<()> {
    let mut reader = PackageReader::new(File::open(input)?)
        .map_err(|error| io::Error::other(format!("invalid input archive: {error}")))?;
    let names = reader.names().to_vec();
    // Exclusive creation protects the baseline and any earlier evidence.
    let output_file = File::options().write(true).create_new(true).open(output)?;
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
        "path": output, "read_seconds": read_time.as_secs_f64(),
        "pack_seconds": pack_time.as_secs_f64()
    });
    writeln!(io::stdout().lock(), "{summary}")?;
    Ok(())
}

pub(super) fn manifest_identity(path: &Path, id: &str) -> io::Result<()> {
    use crate::engine::package::{MAX_MANIFEST_BYTES, hash};
    let mut reader = PackageReader::new(File::open(path)?)
        .map_err(|error| io::Error::other(format!("invalid input archive: {error}")))?;
    reader.select_level(Some(id)).map_err(io::Error::other)?;
    let bytes = reader
        .read_entry("manifest.json", MAX_MANIFEST_BYTES)
        .map_err(io::Error::other)?;
    let manifest = Manifest::from_json(&bytes, reader.names()).map_err(io::Error::other)?;
    if manifest.id != id {
        return Err(io::Error::other("manifest member identity differs"));
    }
    writeln!(io::stdout().lock(), "{}", hash::sha256_hex(&bytes))
}

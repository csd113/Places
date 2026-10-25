//! Export retention regression: source bytes and streamed prepared records with
//! the same identity must share one disk spool. A 128 KiB fixture crosses the
//! copy buffer boundary without assets, a world build or a large allocation.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::package::{PackageReader, archive, hash};

use super::super::project_spool::ProjectSpool;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn project_spool_streams_and_deduplicates_exact_source_and_prepared_bytes() -> Result<(), String> {
    let root = Fixture(std::env::temp_dir().join(format!(
        "places-project-spool-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&root.0).map_err(|error| error.to_string())?;
    let bytes = vec![b'S'; 131_071];
    let map = root.0.join("ordinary.placesmap");
    archive::write_archive(
        &map,
        vec![archive::PendingEntry {
            name: "prepared.bin".into(),
            bytes: bytes.clone(),
        }],
    )?;
    let mut reader = PackageReader::new(fs::File::open(&map).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let mut spool = ProjectSpool::new(&root.0)?;
    let source = spool.add_bytes(&bytes)?;
    let prepared = spool.add_entry(&mut reader, "prepared.bin", &|| Ok(()))?;
    assert_eq!(
        source, prepared,
        "identical source and prepared bytes share one physical record"
    );
    assert_eq!(
        spool.len(),
        1,
        "the duplicate must not consume a second record slot"
    );
    assert_eq!(
        spool.total(),
        131_071,
        "the duplicate must not consume aggregate budget"
    );
    assert!(
        !root.0.join("records/incoming").exists(),
        "deduplication removes its owned incoming file"
    );
    let entries = spool.into_entries();
    let entry = entries.first().ok_or("Missing spooled record")?;
    assert_eq!(
        entry.sha256,
        hash::sha256_hex(&bytes),
        "streamed hash matches the exact source"
    );
    assert_eq!(
        fs::read(&entry.path).map_err(|error| error.to_string())?,
        bytes,
        "disk spool preserves bytes across copy buffer boundaries"
    );
    Ok(())
}

// Protects compressed-copy integrity and cancellation without maps/assets. The
// 128 KiB deterministic payload crosses streaming boundaries; three codecs and
// tiny owned files keep runtime and memory proportional (well below one second).
fn raw_record(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut archive =
        zip::ZipArchive::new(fs::File::open(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let mut raw = archive.by_index_raw(0).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let _count = raw
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn corrupt_record(path: &std::path::Path) -> Result<(), String> {
    use std::io::{Read as _, Seek as _, Write as _};
    let mut archive =
        zip::ZipArchive::new(fs::File::open(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let raw = archive.by_index_raw(0).map_err(|error| error.to_string())?;
    let offset = raw
        .data_start()
        .ok_or("Missing raw offset")?
        .saturating_add(raw.compressed_size() / 2);
    drop(raw);
    drop(archive);
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let _position = file
        .seek(std::io::SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)
        .map_err(|error| error.to_string())?;
    for value in &mut byte {
        *value ^= 0xff;
    }
    let _write_position = file
        .seek(std::io::SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    file.write_all(&byte).map_err(|error| error.to_string())
}

#[test]
fn compressed_project_spools_preserve_streams_and_reject_corruption_and_cancel()
-> Result<(), String> {
    use std::io::Write as _;
    for method in [
        zip::CompressionMethod::Stored,
        zip::CompressionMethod::Deflated,
        zip::CompressionMethod::Zstd,
    ] {
        let root = Fixture(std::env::temp_dir().join(format!(
            "places-compressed-spool-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&root.0).map_err(|error| error.to_string())?;
        let bytes: Vec<u8> = (0_u32..32_768)
            .flat_map(|index| index.wrapping_mul(2_654_435_761).to_le_bytes())
            .collect();
        let name = format!("blobs/{}.record", hash::sha256_hex(&bytes));
        let source = root.0.join("source.placesmap");
        let mut writer =
            zip::ZipWriter::new(fs::File::create(&source).map_err(|error| error.to_string())?);
        writer
            .start_file(
                &name,
                zip::write::SimpleFileOptions::default().compression_method(method),
            )
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        let _finished = writer.finish().map_err(|error| error.to_string())?;
        let expected = raw_record(&source)?;
        let mut reader =
            PackageReader::new(fs::File::open(&source).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        let mut spool = ProjectSpool::new(&root.0)?;
        let physical = spool.add_compressed_entry(&mut reader, &name, &|| Ok(()))?;
        assert_eq!(
            physical, name,
            "verified decoded identity must survive raw staging"
        );
        let entries = spool.into_entries();
        let entry = entries.first().ok_or("Missing compressed spool")?;
        assert!(
            entry.compressed.is_some(),
            "export must retain the compressed stream"
        );
        let output = root.0.join("output.placesmap");
        archive::write_archive_files_with_level_guarded(&output, entries, || Ok(()), || Ok(()), 6)?;
        assert_eq!(
            raw_record(&output)?,
            expected,
            "all codecs retain exact compressed payloads"
        );
        let mut result =
            PackageReader::new(fs::File::open(&output).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        assert_eq!(
            result.read_blob(&name, 131_072)?,
            bytes,
            "raw publication remains fully decodable and hash verified"
        );
        drop(result);
        reject_changed_or_cancelled(&root, &mut reader, &name, &output)?;
        drop(reader);
        corrupt_record(&source)?;
        let stage = root.0.join("corrupt-input");
        fs::create_dir(&stage).map_err(|error| error.to_string())?;
        let mut corrupt_spool = ProjectSpool::new(&stage)?;
        let mut corrupt_reader =
            PackageReader::new(fs::File::open(&source).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        assert!(
            corrupt_spool
                .add_compressed_entry(&mut corrupt_reader, &name, &|| Ok(()))
                .is_err(),
            "raw staging must validate corrupted input through the bounded decoder"
        );
    }
    Ok(())
}

// Protects independent quality reuse at its key boundary. No world preparation,
// assets or map bake: a single tiny level is serialized for four selections.
#[test]
fn common_lighting_key_excludes_quality_addition_removal_and_order() -> Result<(), String> {
    use crate::engine::quality::LightmapQuality;
    let level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"quality_key","name":"Quality key","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
    let key = |qualities: &[LightmapQuality], revision| {
        super::super::lighting_fingerprint_with_revision(&level, qualities, &[], revision)
    };
    let expected = key(&[LightmapQuality::Full], 1)?;
    for qualities in [
        vec![LightmapQuality::Off],
        vec![LightmapQuality::Off, LightmapQuality::Full],
        vec![LightmapQuality::Full, LightmapQuality::Off],
    ] {
        assert_eq!(
            key(&qualities, 1)?,
            expected,
            "quality membership/order must not invalidate common preparation inputs"
        );
    }
    assert_ne!(
        key(&[LightmapQuality::Full], 2)?,
        expected,
        "geometry changes must still invalidate every quality"
    );
    Ok(())
}

fn reject_changed_or_cancelled(
    root: &Fixture,
    reader: &mut PackageReader<fs::File>,
    name: &str,
    output: &std::path::Path,
) -> Result<(), String> {
    for damage in [false, true] {
        let stage = root.0.join(if damage { "damage" } else { "cancel" });
        fs::create_dir(&stage).map_err(|error| error.to_string())?;
        let mut spool = ProjectSpool::new(&stage)?;
        let _physical = spool.add_compressed_entry(reader, name, &|| Ok(()))?;
        let entries = spool.into_entries();
        if damage {
            corrupt_record(&entries.first().ok_or("Missing spool")?.path)?;
        }
        let before = fs::read(output).map_err(|error| error.to_string())?;
        let partial = output.with_extension("placesmap.partial");
        let attempted = archive::write_archive_files_with_level_guarded(
            output,
            entries,
            || {
                if !damage
                    && partial
                        .metadata()
                        .is_ok_and(|metadata| metadata.len() > 32_768)
                {
                    Err("cancel compressed copy".into())
                } else {
                    Ok(())
                }
            },
            || Ok(()),
            6,
        );
        assert!(
            attempted.is_err(),
            "changed compressed bytes and mid-copy cancellation must reject publication"
        );
        assert_eq!(
            fs::read(output).map_err(|error| error.to_string())?,
            before,
            "failed raw publication must preserve accepted output"
        );
        assert!(!partial.exists(), "owned incomplete publication is removed");
    }
    Ok(())
}

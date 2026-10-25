//! Essential archive regressions for project-only capacity and streamed atomic
//! publication. Large boundaries are central-directory metadata and a virtual
//! sparse prefix: no large payload is allocated, baked, hashed or written.
//! Stream integrity/cancellation uses at most 128 KiB of ordinary fixture data.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom, Write as _};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::package::{
    MAX_PROJECT_TOTAL_BYTES, MAX_TOTAL_BYTES, PackageReader,
    archive::{self, SpoolEntry},
    hash, project,
};
use crate::engine::project::{ProjectManifest, ProjectPath};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Entry {
    name: String,
    bytes: Vec<u8>,
    declared: u32,
}

fn entry(name: String, bytes: Vec<u8>) -> Result<Entry, String> {
    Ok(Entry {
        name,
        declared: u32::try_from(bytes.len()).map_err(|error| error.to_string())?,
        bytes,
    })
}

fn project_entries(
    level_count: usize,
    prepared_bytes: Option<u32>,
    overflow_aliases: bool,
) -> Result<Vec<Entry>, String> {
    let levels: Vec<_> = (0..level_count)
        .map(|index| {
            serde_json::json!({"id":format!("map{index}"),"source":format!("levels/map{index}.json")})
        })
        .collect();
    let manifest_bytes = serde_json::to_vec(&serde_json::json!({
        "schema":"places-project","project_format":1_u32,"id":"capacity",
        "name":"Capacity","entry_level":"map0","levels":levels
    }))
    .map_err(|error| error.to_string())?;
    let manifest = ProjectManifest::from_json(&manifest_bytes)?;
    let manifest_name = hash::blob_name(&manifest_bytes, ".record");
    let mut index = project::ProjectPackage {
        schema: "places-project-package".into(),
        package_format: 1,
        project: manifest,
        levels: Vec::new(),
        sources: BTreeMap::from([(
            ProjectPath::new("places.project.json".into())?,
            manifest_name.clone(),
        )]),
    };
    let mut entries = vec![entry(manifest_name, manifest_bytes)?];
    for number in 0..level_count {
        let bytes = format!("undecodable prepared fixture {number}").into_bytes();
        let name = hash::blob_name(&bytes, ".record");
        let mut prepared = entry(name.clone(), bytes)?;
        if let Some(declared) = prepared_bytes {
            prepared.declared = declared;
        }
        entries.push(prepared);
        let mut records = BTreeMap::from([("manifest.json".into(), name.clone())]);
        if overflow_aliases && number == 0 {
            let _first = records.insert("alias-one".into(), name.clone());
            let _second = records.insert("alias-two".into(), name);
        }
        index.levels.push(project::ProjectMap {
            id: format!("map{number}"),
            records,
        });
        let source = serde_json::to_vec(&serde_json::json!({
            "schema":"places-level","format_version":1_u32,"id":format!("map{number}"),
            "name":format!("Map {number}"),"spawn":{"x":0_u32,"z":0_u32}
        }))
        .map_err(|error| error.to_string())?;
        let source_name = hash::blob_name(&source, ".record");
        let _old = index.sources.insert(
            ProjectPath::new(format!("levels/map{number}.json"))?,
            source_name.clone(),
        );
        entries.push(entry(source_name, source)?);
    }
    entries.push(entry(
        project::INDEX.into(),
        serde_json::to_vec(&index).map_err(|error| error.to_string())?,
    )?);
    Ok(entries)
}

fn zip_bytes(entries: &[Entry]) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for source in entries {
        writer
            .start_file(&source.name, zip::write::SimpleFileOptions::default())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&source.bytes)
            .map_err(|error| error.to_string())?;
    }
    let mut bytes = writer
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    let end = bytes.len().checked_sub(22).ok_or("Missing ZIP end")?;
    let mut cursor = usize::try_from(read_u32(&bytes, end.saturating_add(16))?)
        .map_err(|error| error.to_string())?;
    for source in entries {
        let length = source.name.len();
        put_u32(&mut bytes, cursor.saturating_add(24), source.declared)?;
        cursor = cursor.saturating_add(46).saturating_add(length);
    }
    Ok(bytes)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let encoded = bytes
        .get(offset..offset.saturating_add(4))
        .ok_or("Missing ZIP integer")?;
    Ok(u32::from_le_bytes(encoded.try_into().map_err(
        |error: std::array::TryFromSliceError| error.to_string(),
    )?))
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    bytes
        .get_mut(offset..offset.saturating_add(4))
        .ok_or("Missing ZIP integer")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[test]
fn project_capacity_requires_a_validated_index_and_bounded_virtual_members() -> Result<(), String> {
    let declared = 384_u32.saturating_mul(1024).saturating_mul(1024);
    let entries = project_entries(3, Some(declared), false)?;
    let bytes = zip_bytes(&entries)?;
    let mut reader = PackageReader::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    assert!(
        project::read_index(&mut reader)?.is_some(),
        "a valid project index may admit >1 GiB without decoding prepared records"
    );
    let mut ordinary = project_entries(3, Some(declared), false)?;
    ordinary.retain(|record| record.name != project::INDEX);
    assert!(
        PackageReader::new(Cursor::new(zip_bytes(&ordinary)?)).is_err(),
        "standalone metadata retains the 1 GiB aggregate cap"
    );
    let mut bogus = project_entries(3, Some(declared), false)?;
    let marker = bogus.last_mut().ok_or("Missing index fixture")?;
    marker.bytes = b"{}".to_vec();
    marker.declared = 2;
    assert!(
        PackageReader::new(Cursor::new(zip_bytes(&bogus)?)).is_err(),
        "a project.json marker alone must not grant a larger aggregate budget"
    );
    let aliases = project_entries(3, Some(declared), true)?;
    assert!(
        PackageReader::new(Cursor::new(zip_bytes(&aliases)?)).is_err(),
        "every virtual alias counts toward its member's original 1 GiB cap"
    );
    let too_large = project_entries(6, Some(declared), false)?;
    assert!(
        PackageReader::new(Cursor::new(zip_bytes(&too_large)?)).is_err(),
        "project metadata retains the 2 GiB outer cap"
    );
    assert_eq!(
        MAX_PROJECT_TOTAL_BYTES,
        MAX_TOTAL_BYTES.saturating_mul(2),
        "project allowance must not silently change the standalone budget"
    );
    Ok(())
}

/// A virtual sparse prefix tests physical transport sizes using only small ZIP
/// metadata in memory. Reads from the prefix return zeros without allocating it.
struct SparsePrefix {
    prefix: u64,
    bytes: Vec<u8>,
    position: u64,
}

impl SparsePrefix {
    fn new(mut bytes: Vec<u8>, prefix: u64) -> Result<Self, String> {
        let end = bytes.len().checked_sub(22).ok_or("Missing ZIP end")?;
        let directory = read_u32(&bytes, end.saturating_add(16))?;
        let mut cursor = usize::try_from(directory).map_err(|error| error.to_string())?;
        while cursor < end {
            let length_bytes = bytes
                .get(cursor.saturating_add(28)..cursor.saturating_add(30))
                .ok_or("Missing ZIP name length")?;
            let length =
                usize::from(u16::from_le_bytes(length_bytes.try_into().map_err(
                    |error: std::array::TryFromSliceError| error.to_string(),
                )?));
            let offset =
                u64::from(read_u32(&bytes, cursor.saturating_add(42))?).saturating_add(prefix);
            put_u32(
                &mut bytes,
                cursor.saturating_add(42),
                u32::try_from(offset).map_err(|error| error.to_string())?,
            )?;
            cursor = cursor.saturating_add(46).saturating_add(length);
        }
        put_u32(
            &mut bytes,
            end.saturating_add(16),
            u32::try_from(u64::from(directory).saturating_add(prefix))
                .map_err(|error| error.to_string())?,
        )?;
        Ok(Self {
            prefix,
            bytes,
            position: 0,
        })
    }

    fn len(&self) -> u64 {
        self.prefix
            .saturating_add(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX))
    }
}

impl Read for SparsePrefix {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = usize::try_from(self.len().saturating_sub(self.position))
            .unwrap_or(usize::MAX)
            .min(output.len());
        if count == 0 {
            return Ok(0);
        }
        let portion = output
            .get_mut(..count)
            .ok_or_else(|| std::io::Error::other("Sparse read outside buffer"))?;
        portion.fill(0);
        let end = self
            .position
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if end > self.prefix {
            let source_start = usize::try_from(self.position.saturating_sub(self.prefix))
                .map_err(std::io::Error::other)?;
            let source_end =
                usize::try_from(end.saturating_sub(self.prefix)).map_err(std::io::Error::other)?;
            let target_start = usize::try_from(self.prefix.saturating_sub(self.position))
                .map_err(std::io::Error::other)?;
            portion
                .get_mut(target_start..)
                .ok_or_else(|| std::io::Error::other("Sparse target outside buffer"))?
                .copy_from_slice(
                    self.bytes
                        .get(source_start..source_end)
                        .ok_or_else(|| std::io::Error::other("Sparse source outside buffer"))?,
                );
        }
        self.position = end;
        Ok(count)
    }
}

impl Seek for SparsePrefix {
    fn seek(&mut self, offset: SeekFrom) -> std::io::Result<u64> {
        self.position = match offset {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.len().checked_add_signed(delta),
        }
        .ok_or_else(|| std::io::Error::other("Invalid sparse seek"))?;
        Ok(self.position)
    }
}

#[test]
fn project_transport_allowance_cannot_be_claimed_by_an_ordinary_archive() -> Result<(), String> {
    let prefix = archive::MAX_TRANSPORT_BYTES;
    let ordinary = vec![entry("payload".into(), b"small".to_vec())?];
    assert!(
        PackageReader::new(SparsePrefix::new(zip_bytes(&ordinary)?, prefix)?).is_err(),
        "standalone transport retains 1 GiB +64 MiB despite a sparse physical prefix"
    );
    let entries = project_entries(1, None, false)?;
    assert!(
        PackageReader::new(SparsePrefix::new(zip_bytes(&entries)?, prefix)?).is_ok(),
        "the project transport allowance requires a valid readable index"
    );
    let mut bogus = entries;
    let marker = bogus.last_mut().ok_or("Missing index fixture")?;
    marker.bytes = b"{}".to_vec();
    marker.declared = 2;
    assert!(
        PackageReader::new(SparsePrefix::new(zip_bytes(&bogus)?, prefix)?).is_err(),
        "larger project transport cannot be admitted by a marker alone"
    );
    assert!(
        PackageReader::new(SparsePrefix::new(
            zip_bytes(&project_entries(1, None, false)?)?,
            archive::MAX_PROJECT_TRANSPORT_BYTES,
        )?)
        .is_err(),
        "project transport retains its 2 GiB +64 MiB outer cap"
    );
    Ok(())
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "places-archive-stream-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn spool(&self, entries: &[Entry]) -> Result<Vec<SpoolEntry>, String> {
        entries
            .iter()
            .enumerate()
            .map(|(number, source)| {
                let path = self.0.join(format!("spool{number}"));
                fs::write(&path, &source.bytes).map_err(|error| error.to_string())?;
                Ok(SpoolEntry {
                    compressed: None,
                    name: source.name.clone(),
                    path,
                    bytes: u64::try_from(source.bytes.len()).map_err(|error| error.to_string())?,
                    sha256: hash::sha256_hex(&source.bytes),
                })
            })
            .collect()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn streamed_project_publication_rejects_changed_spools_and_preserves_owned_temps()
-> Result<(), String> {
    let root = Directory::new()?;
    let path = root.0.join("output.placesmap");
    let reserved = root.0.join("output.placesmap.partial");
    let partial = root.0.join("output.placesmap.partial-1");
    fs::write(&path, b"previous accepted delivery").map_err(|error| error.to_string())?;
    fs::write(&reserved, b"unowned interrupted artifact").map_err(|error| error.to_string())?;
    let entries = project_entries(1, None, false)?;
    for failure in 0_u8..6 {
        let mut spool = root.spool(&entries)?;
        let first = spool.first_mut().ok_or("Missing spool fixture")?;
        match failure {
            0 => fs::write(&first.path, []).map_err(|error| error.to_string())?,
            1 => fs::OpenOptions::new()
                .append(true)
                .open(&first.path)
                .and_then(|mut file| file.write_all(b"growth"))
                .map_err(|error| error.to_string())?,
            2 => {
                let count = usize::try_from(first.bytes).map_err(|error| error.to_string())?;
                fs::write(&first.path, vec![b'x'; count]).map_err(|error| error.to_string())?;
            }
            3 | 4 => {}
            5 => {
                let marker = spool.last_mut().ok_or("Missing index fixture")?;
                fs::write(&marker.path, b"{}").map_err(|error| error.to_string())?;
                marker.bytes = 2;
                marker.sha256 = hash::sha256_hex(b"{}");
            }
            _ => return Err("Unknown stream fixture case".into()),
        }
        let checks = Cell::new(0_u32);
        let result = archive::write_project_archive_files_guarded(
            &path,
            spool,
            || {
                checks.set(checks.get().saturating_add(1));
                if failure == 3 && checks.get() >= 3 {
                    Err("cancelled during stream".into())
                } else {
                    Ok(())
                }
            },
            || {
                if failure == 4 {
                    Err("publication guard refused".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(
            result.is_err(),
            "fixture failure {failure} must refuse publication"
        );
        assert_eq!(
            fs::read(&path).map_err(|error| error.to_string())?,
            b"previous accepted delivery",
            "failed stream retains accepted output"
        );
        assert_eq!(
            fs::read(&reserved).map_err(|error| error.to_string())?,
            b"unowned interrupted artifact",
            "cleanup never removes an unowned partial"
        );
        assert!(
            !partial.exists(),
            "owned partial must be cleaned after failure {failure}"
        );
    }
    archive::write_project_archive_files_guarded(
        &path,
        root.spool(&entries)?,
        || Ok(()),
        || Ok(()),
    )?;
    assert!(
        project::inspect(&path)?.is_some(),
        "successful stream publishes a readable project"
    );
    Ok(())
}

#[test]
fn streamed_zip_copy_keeps_content_hashes_limits_and_chunk_cancellation() -> Result<(), String> {
    let payload = vec![b'a'; 128 * 1024];
    let name = hash::blob_name(&payload, ".mesh");
    let entries = vec![entry(name.clone(), payload.clone())?];
    let mut reader =
        PackageReader::new(Cursor::new(zip_bytes(&entries)?)).map_err(|error| error.to_string())?;
    let mut copied = Vec::new();
    let (bytes, digest) = reader.copy_entry_to(&name, 128 * 1024, &mut copied, &|| Ok(()))?;
    assert_eq!(bytes, 128 * 1024, "copy reports exact decoded length");
    assert_eq!(copied, payload, "copy preserves payload bytes");
    assert_eq!(
        digest,
        hash::sha256_hex(&payload),
        "copy reports its verified digest"
    );
    let mut discarded = Vec::new();
    assert!(
        reader
            .copy_entry_to(&name, 1, &mut discarded, &|| Ok(()))
            .is_err(),
        "declared size fails before copying"
    );
    assert!(discarded.is_empty(), "oversized entry writes no bytes");
    let checks = Cell::new(0_u32);
    assert!(
        reader
            .copy_entry_to(&name, 128 * 1024, &mut discarded, &|| {
                checks.set(checks.get().saturating_add(1));
                if checks.get() >= 2 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            })
            .is_err(),
        "cancellation is checked between chunks"
    );
    assert_eq!(
        discarded.len(),
        64 * 1024,
        "cancellation bounds copied data to one chunk"
    );
    let corrupt = vec![entry(name.clone(), vec![b'b'; 128 * 1024])?];
    let mut changed =
        PackageReader::new(Cursor::new(zip_bytes(&corrupt)?)).map_err(|error| error.to_string())?;
    assert!(
        changed
            .copy_entry_to(&name, 128 * 1024, &mut std::io::sink(), &|| Ok(()))
            .is_err(),
        "ordinary content-addressed records retain the original name's hash during rewrap"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn streamed_project_refuses_a_symbolic_spool_input() -> Result<(), String> {
    let root = Directory::new()?;
    let path = root.0.join("linked-input.placesmap");
    let entries = project_entries(1, None, false)?;
    let spool = root.spool(&entries)?;
    let source = spool.first().ok_or("Missing spool fixture")?;
    let target = root.0.join("real-spool");
    fs::rename(&source.path, &target).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&target, &source.path).map_err(|error| error.to_string())?;
    assert!(
        archive::write_project_archive_files_guarded(&path, spool, || Ok(()), || Ok(())).is_err(),
        "matching spool bytes do not authorize following a symbolic input"
    );
    assert!(!path.exists(), "a linked input never publishes a package");
    assert!(
        !root.0.join("linked-input.placesmap.partial").exists(),
        "a rejected link leaves no owned partial"
    );
    assert!(
        target.exists(),
        "input refusal never removes the link target"
    );
    Ok(())
}

// Standalone migration now streams owned files. This 128 KiB check uniquely
// protects its smaller aggregate cap plus digest/cancel atomicity; no bake.
#[test]
fn streamed_ordinary_migration_keeps_bounds_integrity_and_atomicity() -> Result<(), String> {
    let root = Directory::new()?;
    let output = root.0.join("ordinary.placesmap");
    fs::write(&output, b"accepted").map_err(|error| error.to_string())?;
    let entries = vec![Entry {
        name: "payload.bin".into(),
        bytes: vec![b'P'; 131_071],
        declared: 131_071,
    }];
    for failure in 0_u8..3 {
        let mut spool = root.spool(&entries)?;
        let first = spool.first_mut().ok_or("Missing ordinary spool")?;
        match failure {
            0 => {
                first.bytes = crate::engine::package::MAX_BINARY_BYTES;
                // Metadata-only aggregate overflow: each record is individually
                // legal, and admission must fail before opening these files.
                let extra = |name: &str| SpoolEntry {
                    compressed: None,
                    name: name.into(),
                    path: root.0.join("must-not-open"),
                    bytes: crate::engine::package::MAX_BINARY_BYTES,
                    sha256: hash::sha256_hex(&[]),
                };
                spool.extend([extra("extra-a.bin"), extra("extra-b.bin")]);
            }
            1 => {
                fs::write(&first.path, vec![b'Q'; 131_071]).map_err(|error| error.to_string())?;
            }
            2 => {}
            _ => return Err("Unknown ordinary spool failure".into()),
        }
        let checks = Cell::new(0_u32);
        assert!(
            archive::write_archive_files_with_level_guarded(
                &output,
                spool,
                || {
                    checks.set(checks.get().saturating_add(1));
                    if failure == 2 && checks.get() >= 3 {
                        Err("cancelled".into())
                    } else {
                        Ok(())
                    }
                },
                || Ok(()),
                9
            )
            .is_err(),
            "ordinary stream must reject failure {failure}"
        );
        assert_eq!(
            fs::read(&output).map_err(|error| error.to_string())?,
            b"accepted",
            "failure preserves accepted output"
        );
    }
    archive::write_archive_files_with_level_guarded(
        &output,
        root.spool(&entries)?,
        || Ok(()),
        || Ok(()),
        9,
    )?;
    let mut reader =
        PackageReader::new(fs::File::open(&output).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    assert_eq!(
        reader.read_entry("payload.bin", 131_071)?,
        vec![b'P'; 131_071],
        "streamed ordinary archive preserves bytes across the copy boundary"
    );
    Ok(())
}

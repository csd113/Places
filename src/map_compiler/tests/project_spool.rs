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

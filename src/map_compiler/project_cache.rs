//! Recover a previous project's ordinary map view into owned build staging.
//! This is only a cache seed: the compiler still checks exact source, asset,
//! tool, variant and prepared-stage signatures and every declared record.

use std::collections::BTreeMap;
use std::path::Path;

use crate::engine::package::{archive, hash, project};

pub(super) fn seed(previous: &Path, level: &str, staged: &Path) -> Result<(), String> {
    if !previous.exists() {
        return Ok(());
    }
    let mut reader = super::open_package(previous)?;
    let index = project::read_index(&mut reader)?.ok_or("Previous export is not a project")?;
    if !index.levels.iter().any(|record| record.id == level) {
        return Ok(());
    }
    project::select(&mut reader, Some(level))?;
    let manifest = super::read_manifest(&mut reader)?;
    let temporary = archive::create_temporary_directory(staged)?;
    let result = (|| {
        let mut spool = super::project_spool::ProjectSpool::new(&temporary)?;
        let mut records = BTreeMap::new();
        for entry in &manifest.entries {
            super::progress::check_cancelled()?;
            if reader.entry_size(&entry.name)? != entry.bytes {
                return Err("Previous seed record length changed".into());
            }
            let physical = spool.add_compressed_entry(
                &mut reader,
                &entry.name,
                &super::progress::check_cancelled,
            )?;
            if hash::sha256_from_blob_name(&physical).as_deref() != Some(entry.sha256.as_str()) {
                return Err("Previous seed record hash changed".into());
            }
            drop(records.insert(entry.name.clone(), physical));
        }
        // Bootstrap stays ordinary Deflate so older readers can report the
        // manifest's required capabilities before opening any binary record.
        let manifest_record = spool.add_bytes(&manifest.to_json()?)?;
        drop(records.insert("manifest.json".into(), manifest_record));
        let lookup: BTreeMap<_, _> = spool
            .into_entries()
            .into_iter()
            .map(|entry| (entry.name.clone(), entry))
            .collect();
        let entries = records
            .into_iter()
            .map(|(name, physical)| {
                let entry = lookup.get(&physical).ok_or("Seed spool disappeared")?;
                Ok(archive::SpoolEntry {
                    name,
                    path: entry.path.clone(),
                    bytes: entry.bytes,
                    sha256: entry.sha256.clone(),
                    compressed: entry.compressed.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        archive::write_archive_files_with_level_guarded(
            staged,
            entries,
            super::progress::check_cancelled,
            || Ok(()),
            6,
        )
    })();
    match (result, std::fs::remove_dir_all(&temporary)) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(format!(
            "Cache seeded; cleanup {}: {error}",
            temporary.display()
        )),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; seed cleanup {}: {cleanup}",
            temporary.display()
        )),
    }
}

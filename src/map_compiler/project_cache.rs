//! Recover a previous project's ordinary map view into owned build staging.
//! This is only a cache seed: the compiler still checks exact source, asset,
//! tool, variant and prepared-stage signatures and every declared record.

use std::path::Path;

use crate::engine::package::{archive, project};

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
    let mut entries = Vec::with_capacity(manifest.entries.len().saturating_add(1));
    for entry in &manifest.entries {
        super::progress::check_cancelled()?;
        entries.push(archive::PendingEntry {
            name: entry.name.clone(),
            bytes: super::read_declared_entry(&mut reader, entry)?,
        });
    }
    entries.push(archive::PendingEntry {
        name: "manifest.json".into(),
        bytes: manifest.to_json()?,
    });
    archive::write_archive_checked(staged, entries, super::progress::check_cancelled)
}

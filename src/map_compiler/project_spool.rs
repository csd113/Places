//! Owned, content-addressed disk staging for a complete project export.
//! Only record identities/sizes stay resident between ordinary map builds.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, Write as _};
use std::path::{Path, PathBuf};

use crate::engine::package::{MAX_BINARY_BYTES, MAX_ENTRIES, MAX_PROJECT_TOTAL_BYTES};
use crate::engine::package::{PackageReader, archive, hash};

pub(super) struct ProjectSpool {
    root: PathBuf,
    records: BTreeMap<String, archive::SpoolEntry>,
    total: u64,
}

impl ProjectSpool {
    pub(super) fn new(temporary: &Path) -> Result<Self, String> {
        let root = temporary.join("records");
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self {
            root,
            records: BTreeMap::new(),
            total: 0,
        })
    }

    pub(super) fn add_bytes(&mut self, bytes: &[u8]) -> Result<String, String> {
        let sha256 = hash::sha256_hex(bytes);
        let name = format!("blobs/{sha256}.record");
        if self.records.contains_key(&name) {
            return Ok(name);
        }
        let mut file = self.incoming()?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        drop(file);
        self.finish(
            u64::try_from(bytes.len()).map_err(|error| error.to_string())?,
            sha256,
        )
    }

    pub(super) fn add_entry<R: Read + Seek>(
        &mut self,
        reader: &mut PackageReader<R>,
        name: &str,
        check: &impl Fn() -> Result<(), String>,
    ) -> Result<String, String> {
        let mut file = self.incoming()?;
        let (bytes, sha256) = reader.copy_entry_to(name, MAX_BINARY_BYTES, &mut file, check)?;
        drop(file);
        self.finish(bytes, sha256)
    }

    fn incoming(&self) -> Result<File, String> {
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.root.join("incoming"))
            .map_err(|error| error.to_string())
    }

    fn finish(&mut self, bytes: u64, sha256: String) -> Result<String, String> {
        let name = format!("blobs/{sha256}.record");
        let incoming = self.root.join("incoming");
        if let Some(previous) = self.records.get(&name) {
            if previous.bytes != bytes {
                return Err("Project record identity has conflicting lengths".into());
            }
            fs::remove_file(incoming).map_err(|error| error.to_string())?;
            return Ok(name);
        }
        let total = self
            .total
            .checked_add(bytes)
            .ok_or("Project byte total overflows")?;
        // Reserve one archive entry for the bounded project index. Its bytes
        // are included by the final project archive writer's aggregate check.
        if total > MAX_PROJECT_TOTAL_BYTES || self.records.len() >= MAX_ENTRIES.saturating_sub(1) {
            return Err("Project exceeds package size/record limits".into());
        }
        let path = self.root.join(format!("{sha256}.record"));
        fs::rename(incoming, &path).map_err(|error| error.to_string())?;
        let _old = self.records.insert(
            name.clone(),
            archive::SpoolEntry {
                name: name.clone(),
                path,
                bytes,
                sha256,
            },
        );
        self.total = total;
        Ok(name)
    }

    pub(super) const fn total(&self) -> u64 {
        self.total
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    pub(super) fn into_entries(self) -> Vec<archive::SpoolEntry> {
        self.records.into_values().collect()
    }
}

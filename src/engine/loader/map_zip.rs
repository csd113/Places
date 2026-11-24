//! Local ZIP transport for compiled maps. Only validated package basenames reach
//! the existing transactional installer. No directory tree is ever extracted.

use std::io::{self, Read};
use std::path::Path;

use super::{ImportOutcome, ImportedPackage, LevelManager};
use crate::engine::package::{archive, is_package_path};

#[cfg(test)]
#[path = "../tests/map_zip_import.rs"]
mod tests;

const MAX_MEMBERS: usize = 256;
// The directory scan caps declared expansion at the project transport limit.
// Reserve one more byte in the actual budget for the final overrun probe.
const MAX_STREAMED_BYTES: u64 = archive::MAX_PROJECT_TRANSPORT_BYTES + 1;

#[derive(Default)]
pub struct ArchiveImport {
    rows: Vec<String>,
    imported: usize,
}

impl std::fmt::Display for ArchiveImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Imported {} package(s). Skipped {}.",
            self.imported,
            self.rows.len().saturating_sub(self.imported)
        )?;
        f.write_str(&self.rows.join("\n"))
    }
}

impl ArchiveImport {
    fn record(&mut self, name: &str, result: &Result<ImportedPackage, String>) {
        let row = match result {
            Ok(import) if import.outcome == ImportOutcome::InstalledOrUpdated => {
                self.imported = self.imported.saturating_add(1);
                format!("Imported {name} ({})", import.entry.id)
            }
            Ok(_) => format!("Skipped {name} - already installed and unchanged"),
            Err(error) => format!("Skipped {name} - {}", report_reason(error)),
        };
        self.rows.push(row);
    }
}

fn report_reason(error: &str) -> String {
    let mut chars = error.chars();
    let mut text: String = chars
        .by_ref()
        .take(512)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        text.push_str("...");
    }
    text
}

pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

impl LevelManager {
    pub(crate) fn import_zip(&mut self, source: &Path) -> Result<ArchiveImport, String> {
        let file =
            archive::open_file(source).map_err(|error| format!("Cannot open ZIP: {error}"))?;
        let mut zip = archive::open_map_zip(file, MAX_MEMBERS)
            .map_err(|error| format!("Invalid map ZIP: {error}"))?;
        let mut report = ArchiveImport::default();
        let mut remaining = MAX_STREAMED_BYTES;
        for index in 0..zip.len() {
            crate::engine::resources::checkpoint()?;
            let metadata = zip
                .by_index_data(index)
                .map_err(|error| format!("Cannot read ZIP directory: {error}"))?;
            // The bounded directory scan has already validated these raw bytes
            // as portable UTF-8 and checked that this index agrees with them.
            let indexed_name = std::str::from_utf8(metadata.name_raw())
                .map_err(|error| format!("Invalid ZIP member name: {error}"))?;
            if indexed_name.ends_with('/') || !is_package_path(Path::new(indexed_name)) {
                continue;
            }
            let name = indexed_name.to_string();
            let size = metadata.size();
            let result = if metadata
                .unix_mode()
                .is_some_and(|mode| !matches!(mode & 0o170_000, 0 | 0o100_000))
            {
                Err("ZIP member is not a regular file".into())
            } else if !matches!(
                metadata.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ) {
                Err("Unsupported ZIP compression. Use Stored or Deflate.".into())
            } else {
                let _budget = crate::engine::resources::batch_member_scope();
                self.import_member(&mut zip, index, &name, size, &mut remaining)
            };
            report.record(&name, &result);
        }
        if report.rows.is_empty() {
            return Err("The ZIP contains no compiled .placesmap files.".into());
        }
        Ok(report)
    }

    fn import_member(
        &mut self,
        zip: &mut zip::ZipArchive<std::fs::File>,
        index: usize,
        name: &str,
        size: u64,
        remaining: &mut u64,
    ) -> Result<ImportedPackage, String> {
        let filename = name.rsplit('/').next().ok_or("Missing ZIP member name")?;
        let member = zip
            .by_index(index)
            .map_err(|error| format!("Cannot open ZIP member: {error}"))?;
        self.import_zip_member(
            filename,
            size,
            &mut MemberRead::new(member, size, remaining)?,
        )
    }
}

/// Charge the shared byte budget even when decoding or validation fails.
struct MemberRead<'a, R> {
    input: io::Take<R>,
    remaining: &'a mut u64,
}

impl<'a, R: Read> MemberRead<'a, R> {
    fn new(input: R, declared: u64, remaining: &'a mut u64) -> Result<Self, String> {
        let limit = declared
            .checked_add(1)
            .filter(|limit| limit <= remaining)
            .ok_or("ZIP expanded-byte budget exhausted")?;
        Ok(Self {
            input: input.take(limit),
            remaining,
        })
    }
}

impl<R: Read> Read for MemberRead<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        crate::engine::resources::checkpoint().map_err(io::Error::other)?;
        let count = self.input.read(buffer)?;
        *self.remaining = self
            .remaining
            .saturating_sub(u64::try_from(count).unwrap_or(u64::MAX));
        Ok(count)
    }
}

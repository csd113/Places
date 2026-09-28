//! Archive access: bounded reading of one package and deterministic writing.

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use super::{MAX_ENTRIES, MAX_TOTAL_BYTES, OpenError, hash};

/// Read-only access to one opened package: a ZIP archive plus its validated
/// entry index.
///
/// The archive is read entry by entry; nothing is extracted to disk and no
/// entry is decompressed before the manifest has declared its expected size and
/// hash. Lookups are exact after normalization: duplicate normalized names are
/// rejected at open, so a name can never resolve ambiguously.
pub struct PackageReader<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
    names: Vec<String>,
}

impl<R: Read + Seek> PackageReader<R> {
    /// Opens an archive and rejects structural hazards before any payload is
    /// touched:
    ///
    /// * more entries than [`MAX_ENTRIES`];
    /// * absolute, drive, UNC or `..`-traversing names;
    /// * normalized-name collisions (the same entry reachable under two
    ///   spellings, which makes lookups ambiguous);
    /// * directory entries (this format has none);
    /// * a declared aggregate uncompressed size beyond [`MAX_TOTAL_BYTES`].
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn new(mut reader: R) -> Result<Self, OpenError> {
        // The raw central directory is validated before the ZIP crate builds
        // its name-keyed index: that index silently collapses exact duplicate
        // names, so a duplicate can only be seen here.
        let names = scan_central_directory(&mut reader)?;
        let archive = zip::ZipArchive::new(reader)
            .map_err(|error| OpenError::NotArchive(error.to_string()))?;
        if archive.len() != names.len() {
            return Err(OpenError::NotArchive(format!(
                "archive lists {} entries in its directory but {} are reachable",
                names.len(),
                archive.len()
            )));
        }
        Ok(Self { archive, names })
    }

    /// Number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.names.len()
    }

    /// True when the archive carries no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The sorted normalized entry names.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// True when `name` exists exactly as spelled.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names
            .binary_search_by(|entry| entry.as_str().cmp(name))
            .is_ok()
    }

    /// Reads one entry with a hard decompression cap.
    ///
    /// The declared uncompressed size is attacker-controlled and is only used
    /// as an allocation hint: the read is capped with [`Read::take`] and a
    /// stream that reaches the cap is an error, never a silent truncation.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn read_entry(&mut self, name: &str, limit: u64) -> Result<Vec<u8>, String> {
        let entry = self
            .archive
            .by_name(name)
            .map_err(|error| format!("package entry '{name}' is missing: {error}"))?;
        let declared = entry.size();
        if declared > limit {
            return Err(format!(
                "package entry '{name}' declares {declared} bytes (limit {limit})"
            ));
        }
        let capacity = usize::try_from(declared.min(limit)).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        let read = entry
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("package entry '{name}' failed to decompress: {error}"))?;
        if u64::try_from(read).unwrap_or(u64::MAX) > limit {
            return Err(format!(
                "package entry '{name}' exceeds the {limit}-byte limit"
            ));
        }
        Ok(bytes)
    }

    /// Reads and integrity-checks one content-addressed blob.
    ///
    /// The entry's name embeds its SHA-256; the digest is recomputed before
    /// the bytes are returned, so a corrupt or substituted payload is rejected
    /// before any decoder sees it.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn read_blob(&mut self, name: &str, limit: u64) -> Result<Vec<u8>, String> {
        let expected = hash::sha256_from_blob_name(name)
            .ok_or_else(|| format!("package blob '{name}' has no content hash in its name"))?;
        let bytes = self.read_entry(name, limit)?;
        let actual = hash::sha256_hex(&bytes);
        if actual != expected {
            return Err(format!(
                "package blob '{name}' has hash {actual}, expected {expected}"
            ));
        }
        Ok(bytes)
    }
}

/// Validates the archive's raw central directory before the ZIP crate sees it.
///
/// This is the one place duplicate entry names are detectable: the ZIP reader
/// stores entries in a name-keyed map, so two `manifest.json` entries collapse
/// into one before any later lookup. The scan also enforces the entry-count,
/// name-safety, directory-entry, symlink, ZIP64 and aggregate-size bounds
/// before the index is built.
#[allow(clippy::too_many_lines)] // one cohesive structural scan of the archive directory
fn scan_central_directory<R: Read + Seek>(reader: &mut R) -> Result<Vec<String>, OpenError> {
    /// End-of-central-directory record size without a comment.
    const EOCD_SIZE: u64 = 22;
    /// Largest accepted comment search window (max comment + EOCD).
    const EOCD_SEARCH: u64 = 65_557;
    /// Largest accepted central directory, in bytes.
    const MAX_CD_BYTES: u64 = 8 * 1024 * 1024;
    const EOCD_SIGNATURE: u32 = 0x0605_4b50;
    const CD_SIGNATURE: u32 = 0x0201_4b50;

    let file_len = reader
        .seek(std::io::SeekFrom::End(0))
        .map_err(|error| OpenError::NotArchive(error.to_string()))?;
    if file_len < EOCD_SIZE {
        return Err(OpenError::NotArchive(
            "file is too short to be an archive".to_string(),
        ));
    }
    let window = file_len.min(EOCD_SEARCH);
    let start = file_len.saturating_sub(window);
    reader
        .seek(std::io::SeekFrom::Start(start))
        .map_err(|error| OpenError::NotArchive(error.to_string()))?;
    let mut tail = vec![0_u8; usize::try_from(window).unwrap_or(0)];
    reader
        .read_exact(&mut tail)
        .map_err(|error| OpenError::NotArchive(error.to_string()))?;
    let mut eocd: Option<usize> = None;
    for index in (0..tail.len().saturating_sub(21)).rev() {
        if tail.get(index..index.saturating_add(4)) == Some(&EOCD_SIGNATURE.to_le_bytes()) {
            eocd = Some(index);
            break;
        }
    }
    let Some(eocd) = eocd else {
        return Err(OpenError::NotArchive(
            "file has no end-of-central-directory record".to_string(),
        ));
    };
    let field = |offset: usize| -> u32 {
        tail.get(eocd.saturating_add(offset)..eocd.saturating_add(offset).saturating_add(4))
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map_or(0, u32::from_le_bytes)
    };
    let field16 = |offset: usize| -> u16 {
        tail.get(eocd.saturating_add(offset)..eocd.saturating_add(offset).saturating_add(2))
            .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
            .map_or(0, u16::from_le_bytes)
    };
    if field16(4) != 0 || field16(6) != 0 {
        return Err(OpenError::NotArchive(
            "multi-disk archives are not supported".to_string(),
        ));
    }
    let entries = field16(10);
    if entries == u16::MAX || field(12) == u32::MAX || field(16) == u32::MAX {
        return Err(OpenError::NotArchive(
            "ZIP64 archives are not supported by this format".to_string(),
        ));
    }
    if usize::from(entries) > MAX_ENTRIES {
        return Err(OpenError::NotArchive(format!(
            "archive has {entries} entries (limit {MAX_ENTRIES})"
        )));
    }
    let cd_size = u64::from(field(12));
    let cd_offset = u64::from(field(16));
    if cd_size > MAX_CD_BYTES || cd_offset.saturating_add(cd_size) > file_len {
        return Err(OpenError::NotArchive(
            "central directory bounds are invalid".to_string(),
        ));
    }
    reader
        .seek(std::io::SeekFrom::Start(cd_offset))
        .map_err(|error| OpenError::NotArchive(error.to_string()))?;
    let mut directory = vec![0_u8; usize::try_from(cd_size).unwrap_or(0)];
    reader
        .read_exact(&mut directory)
        .map_err(|error| OpenError::NotArchive(error.to_string()))?;

    let mut names = Vec::with_capacity(usize::from(entries));
    let mut total: u64 = 0;
    let mut cursor = 0_usize;
    for _ in 0..entries {
        let header = directory
            .get(cursor..cursor.saturating_add(46))
            .ok_or_else(|| OpenError::NotArchive("central directory is truncated".to_string()))?;
        let header: [u8; 46] = header
            .try_into()
            .map_err(|_| OpenError::NotArchive("central directory is malformed".to_string()))?;
        if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != CD_SIGNATURE {
            return Err(OpenError::NotArchive(
                "central directory entry has a bad signature".to_string(),
            ));
        }
        let flags = u16::from_le_bytes([header[8], header[9]]);
        let compressed = u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
        let uncompressed = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        let name_len = usize::from(u16::from_le_bytes([header[28], header[29]]));
        let extra_len = usize::from(u16::from_le_bytes([header[30], header[31]]));
        let comment_len = usize::from(u16::from_le_bytes([header[32], header[33]]));
        if compressed == u32::MAX || uncompressed == u32::MAX {
            return Err(OpenError::NotArchive(
                "ZIP64 entries are not supported by this format".to_string(),
            ));
        }
        let name_start = cursor.saturating_add(46);
        let name_end = name_start.saturating_add(name_len);
        let raw_name = directory
            .get(name_start..name_end)
            .ok_or_else(|| OpenError::NotArchive("central directory is truncated".to_string()))?;
        if raw_name.is_empty() {
            return Err(OpenError::NotArchive(
                "archive has an empty entry name".to_string(),
            ));
        }
        // Names are UTF-8 by contract. The flag is informational (the writer
        // omits it for plain ASCII), so validity of the bytes is what counts.
        let _ = flags;
        let raw_name = std::str::from_utf8(raw_name)
            .map_err(|_| OpenError::NotArchive("entry name is not valid UTF-8".to_string()))?;
        if raw_name.ends_with('/') {
            return Err(OpenError::NotArchive(format!(
                "archive contains a directory entry '{raw_name}'"
            )));
        }
        let external = u32::from_le_bytes([header[38], header[39], header[40], header[41]]);
        let unix_mode = external >> 16;
        if unix_mode & 0o170_000 == 0o120_000 {
            return Err(OpenError::NotArchive(format!(
                "archive contains a symbolic link '{raw_name}'"
            )));
        }
        let name = normalize_entry_name(raw_name)
            .ok_or_else(|| OpenError::NotArchive(format!("unsafe entry name '{raw_name}'")))?;
        total = total.saturating_add(u64::from(uncompressed));
        names.push(name);
        cursor = name_end
            .saturating_add(extra_len)
            .saturating_add(comment_len);
    }
    if total > MAX_TOTAL_BYTES {
        return Err(OpenError::NotArchive(format!(
            "archive declares {total} uncompressed bytes (limit {MAX_TOTAL_BYTES})"
        )));
    }
    names.sort();
    if let Some(pair) = names.windows(2).find(|pair| pair.first() == pair.get(1)) {
        return Err(OpenError::NotArchive(format!(
            "archive contains duplicate entry '{}'",
            pair.first().map_or("", String::as_str)
        )));
    }
    Ok(names)
}

/// Normalizes an archive entry name to a safe relative form.
///
/// Returns `None` for absolute paths, Windows drive or UNC names, backslash
/// separators and any `..`, `.` or empty component. A returned name never
/// starts or ends with `/`.
#[must_use]
pub fn normalize_entry_name(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.starts_with('/') || raw.starts_with('\\') || raw.contains('\\') {
        return None;
    }
    if raw.len() >= 2 && raw.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        parts.push(part);
    }
    Some(parts.join("/"))
}

/// One entry to publish: its relative name and its exact bytes.
pub struct PendingEntry {
    /// Relative archive entry name (already normalized).
    pub name: String,
    /// Uncompressed entry bytes.
    pub bytes: Vec<u8>,
}

/// Writes a complete package archive atomically.
///
/// Entries are sorted by name so two builds of the same content produce the
/// same bytes, and every entry is written with a fixed timestamp and
/// permissions. The archive is written to a temporary sibling, flushed, and
/// renamed over the destination, so a failed build leaves the last valid
/// package untouched.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_archive(path: &Path, mut entries: Vec<PendingEntry>) -> Result<(), String> {
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "package has {} entries (limit {MAX_ENTRIES})",
            entries.len()
        ));
    }
    let total = entries.iter().fold(0_u64, |sum, entry| {
        sum.saturating_add(u64::try_from(entry.bytes.len()).unwrap_or(u64::MAX))
    });
    if total > MAX_TOTAL_BYTES {
        return Err(format!(
            "package would hold {total} uncompressed bytes (limit {MAX_TOTAL_BYTES})"
        ));
    }
    let temporary = temporary_path(path);
    let result = write_archive_to(&temporary, &entries);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("could not publish {}: {error}", path.display())
    })
}

/// Writes a sorted entry list to `path` without atomic publication.
fn write_archive_to(path: &Path, entries: &[PendingEntry]) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    for entry in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        writer
            .start_file(entry.name.as_str(), options)
            .map_err(|error| format!("could not start entry '{}': {error}", entry.name))?;
        writer
            .write_all(&entry.bytes)
            .map_err(|error| format!("could not write entry '{}': {error}", entry.name))?;
    }
    let mut file = writer
        .finish()
        .map_err(|error| format!("could not finish package: {error}"))?;
    file.flush()
        .map_err(|error| format!("could not flush package: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("could not sync package: {error}"))?;
    Ok(())
}

/// A same-directory temporary path for atomic publication.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || "package".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    name.push_str(".partial");
    path.with_file_name(name)
}

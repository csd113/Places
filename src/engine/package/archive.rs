//! Archive access: bounded reading of one package and deterministic writing.

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::{MAX_ENTRIES, MAX_PROJECT_TOTAL_BYTES, MAX_TOTAL_BYTES, OpenError, hash, project};

pub(crate) const MAX_TRANSPORT_BYTES: u64 = MAX_TOTAL_BYTES + 64 * 1024 * 1024;
pub(crate) const MAX_PROJECT_TRANSPORT_BYTES: u64 = MAX_PROJECT_TOTAL_BYTES + 64 * 1024 * 1024;

/// Reject special files before opening, then validate the opened handle too.
/// Unix nonblocking mode prevents a regular-file/FIFO swap from blocking open.
pub(crate) fn open_file(path: &Path) -> Result<std::fs::File, String> {
    let check = |metadata: &std::fs::Metadata| {
        if metadata.is_file() && metadata.len() <= MAX_PROJECT_TRANSPORT_BYTES {
            Ok(())
        } else {
            Err(format!(
                "Package {} must be a regular file within the {MAX_PROJECT_TRANSPORT_BYTES}-byte transport limit",
                path.display()
            ))
        }
    };
    check(&std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?)?;
    let mut options = std::fs::OpenOptions::new();
    let _read = options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _flags = options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    check(&file.metadata().map_err(|e| e.to_string())?)?;
    Ok(file)
}

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
    view: Option<std::collections::BTreeMap<String, String>>,
    project_ids: Option<std::collections::BTreeSet<String>>,
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
    /// * a declared aggregate uncompressed size beyond [`MAX_TOTAL_BYTES`],
    ///   except a bounded, validated project may use [`MAX_PROJECT_TOTAL_BYTES`].
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn new(mut reader: R) -> Result<Self, OpenError> {
        // The raw central directory is validated before the ZIP crate builds
        // its name-keyed index: that index silently collapses exact duplicate
        // names, so a duplicate can only be seen here.
        let directory = scan_central_directory(&mut reader)?;
        let archive = zip::ZipArchive::new(reader)
            .map_err(|error| OpenError::NotArchive(error.to_string()))?;
        if archive.len() != directory.names.len() {
            return Err(OpenError::NotArchive(format!(
                "archive lists {} entries in its directory but {} are reachable",
                directory.names.len(),
                archive.len()
            )));
        }
        let mut opened = Self {
            archive,
            names: directory.names,
            view: None,
            project_ids: None,
        };
        if directory.requires_project_validation {
            let _validated = project::read_index(&mut opened)
                .map_err(OpenError::NotArchive)?
                .ok_or_else(|| OpenError::NotArchive("Missing project index".into()))?;
        }
        Ok(opened)
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

    pub(crate) fn select_view(
        &mut self,
        aliases: std::collections::BTreeMap<String, String>,
        ids: std::collections::BTreeSet<String>,
    ) {
        self.names = aliases.keys().cloned().collect();
        self.view = Some(aliases);
        self.project_ids = Some(ids);
    }

    pub(crate) fn permits_project_level(&self, id: &str) -> bool {
        self.project_ids
            .as_ref()
            .is_some_and(|ids| ids.contains(id))
    }

    pub(crate) const fn has_view(&self) -> bool {
        self.view.is_some()
    }

    /// True when `name` exists exactly as spelled.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names
            .binary_search_by(|entry| entry.as_str().cmp(name))
            .is_ok()
    }

    /// The ZIP index's uncompressed size, without decoding the entry.
    ///
    /// # Errors
    /// Returns an error when the entry cannot be opened.
    pub(crate) fn entry_size(&mut self, name: &str) -> Result<u64, String> {
        let actual = self
            .view
            .as_ref()
            .and_then(|view| view.get(name))
            .map_or(name, String::as_str);
        self.archive
            .by_name(actual)
            .map(|entry| entry.size())
            .map_err(|error| format!("package entry '{name}' is missing: {error}"))
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
        let actual = self
            .view
            .as_ref()
            .and_then(|view| view.get(name))
            .map_or(name, String::as_str);
        let entry = self
            .archive
            .by_name(actual)
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
        if u64::try_from(read).unwrap_or(u64::MAX) != declared {
            return Err(format!(
                "package entry '{name}' declares {declared} bytes but decompressed {read}"
            ));
        }
        if actual != name && hash::sha256_from_blob_name(actual) != Some(hash::sha256_hex(&bytes)) {
            return Err(format!("project record hash mismatch: {actual}"));
        }
        Ok(bytes)
    }

    /// Copies one bounded entry without reserving its entire decoded payload.
    /// The caller owns any partial destination and must discard it on error.
    pub(crate) fn copy_entry_to(
        &mut self,
        name: &str,
        limit: u64,
        writer: &mut impl Write,
        check: &impl Fn() -> Result<(), String>,
    ) -> Result<(u64, String), String> {
        let actual = self
            .view
            .as_ref()
            .and_then(|view| view.get(name))
            .map_or(name, String::as_str);
        let entry = self
            .archive
            .by_name(actual)
            .map_err(|error| format!("package entry '{name}' is missing: {error}"))?;
        let declared = entry.size();
        let copied = copy_bounded(entry, writer, declared, limit, check)
            .map_err(|error| format!("package entry '{name}': {error}"))?;
        if hash::sha256_from_blob_name(actual).is_some_and(|expected| expected != copied.1) {
            return Err(format!("project record hash mismatch: {actual}"));
        }
        Ok(copied)
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
#[expect(
    clippy::too_many_lines,
    reason = "one cohesive structural scan of the archive directory"
)] // one cohesive structural scan of the archive directory
fn scan_central_directory<R: Read + Seek>(reader: &mut R) -> Result<DirectoryIndex, OpenError> {
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
    if file_len > MAX_PROJECT_TRANSPORT_BYTES {
        return Err(OpenError::NotArchive(
            "Package exceeds compressed transport limit".into(),
        ));
    }
    if file_len < EOCD_SIZE {
        return Err(OpenError::NotArchive(
            "file is too short to be an archive".to_string(),
        ));
    }
    let window = file_len.min(EOCD_SEARCH);
    let start = file_len.saturating_sub(window);
    let _seek_status = reader
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
    let Some(directory_end) = eocd else {
        return Err(OpenError::NotArchive(
            "file has no end-of-central-directory record".to_string(),
        ));
    };
    let field = |offset: usize| -> u32 {
        tail.get(
            directory_end.saturating_add(offset)
                ..directory_end.saturating_add(offset).saturating_add(4),
        )
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map_or(0, u32::from_le_bytes)
    };
    let field16 = |offset: usize| -> u16 {
        tail.get(
            directory_end.saturating_add(offset)
                ..directory_end.saturating_add(offset).saturating_add(2),
        )
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map_or(0, u16::from_le_bytes)
    };
    if field16(4) != 0 || field16(6) != 0 || field16(8) != field16(10) {
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
    let eocd_offset = start.saturating_add(u64::try_from(directory_end).unwrap_or(u64::MAX));
    if eocd_offset
        .saturating_add(EOCD_SIZE)
        .saturating_add(u64::from(field16(20)))
        != file_len
        || cd_size > MAX_CD_BYTES
        || cd_offset.saturating_add(cd_size) != eocd_offset
    {
        return Err(OpenError::NotArchive(
            "central directory bounds are invalid".to_string(),
        ));
    }
    let _seek_status_2 = reader
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
        let header_bytes: [u8; 46] = header.try_into().map_err(|error| {
            OpenError::NotArchive(format!("central directory is malformed: {error}"))
        })?;
        if u32::from_le_bytes([
            header_bytes[0],
            header_bytes[1],
            header_bytes[2],
            header_bytes[3],
        ]) != CD_SIGNATURE
        {
            return Err(OpenError::NotArchive(
                "central directory entry has a bad signature".to_string(),
            ));
        }
        let flags = u16::from_le_bytes([header_bytes[8], header_bytes[9]]);
        if flags & 0x0041 != 0 {
            return Err(OpenError::NotArchive(
                "Encrypted archive entries are not supported".into(),
            ));
        }
        let compressed = u32::from_le_bytes([
            header_bytes[20],
            header_bytes[21],
            header_bytes[22],
            header_bytes[23],
        ]);
        let uncompressed = u32::from_le_bytes([
            header_bytes[24],
            header_bytes[25],
            header_bytes[26],
            header_bytes[27],
        ]);
        let name_len = usize::from(u16::from_le_bytes([header_bytes[28], header_bytes[29]]));
        let extra_len = usize::from(u16::from_le_bytes([header_bytes[30], header_bytes[31]]));
        let comment_len = usize::from(u16::from_le_bytes([header_bytes[32], header_bytes[33]]));
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
        let entry_name = std::str::from_utf8(raw_name).map_err(|error| {
            OpenError::NotArchive(format!("entry name is not valid UTF-8: {error}"))
        })?;
        if entry_name.ends_with('/') {
            return Err(OpenError::NotArchive(format!(
                "archive contains a directory entry '{entry_name}'"
            )));
        }
        let external = u32::from_le_bytes([
            header_bytes[38],
            header_bytes[39],
            header_bytes[40],
            header_bytes[41],
        ]);
        let unix_mode = external >> 16_i32;
        if unix_mode & 0o170_000 == 0o120_000 {
            return Err(OpenError::NotArchive(format!(
                "archive contains a symbolic link '{entry_name}'"
            )));
        }
        let name = normalize_entry_name(entry_name)
            .ok_or_else(|| OpenError::NotArchive(format!("unsafe entry name '{entry_name}'")))?;
        total = total.saturating_add(u64::from(uncompressed));
        names.push(name);
        cursor = name_end
            .saturating_add(extra_len)
            .saturating_add(comment_len);
    }
    if cursor != directory.len() {
        return Err(OpenError::NotArchive(
            "Central directory has unaccounted bytes".into(),
        ));
    }
    names.sort();
    if let Some(pair) = names.windows(2).find(|pair| pair.first() == pair.get(1)) {
        return Err(OpenError::NotArchive(format!(
            "archive contains duplicate entry '{}'",
            pair.first().map_or("", String::as_str)
        )));
    }
    let is_project = names
        .binary_search_by(|name| name.as_str().cmp(project::INDEX))
        .is_ok();
    let total_limit = if is_project {
        MAX_PROJECT_TOTAL_BYTES
    } else {
        MAX_TOTAL_BYTES
    };
    if total > total_limit {
        return Err(OpenError::NotArchive(format!(
            "archive declares {total} uncompressed bytes (limit {total_limit})"
        )));
    }
    if !is_project && file_len > MAX_TRANSPORT_BYTES {
        return Err(OpenError::NotArchive(
            "Package exceeds compressed transport limit".into(),
        ));
    }
    Ok(DirectoryIndex {
        names,
        requires_project_validation: is_project
            && (total > MAX_TOTAL_BYTES || file_len > MAX_TRANSPORT_BYTES),
    })
}

struct DirectoryIndex {
    names: Vec<String>,
    requires_project_validation: bool,
}

/// Normalizes an archive entry name to a safe relative form.
///
/// Returns `None` for absolute paths, Windows drive or UNC names, backslash
/// separators and any `..`, `.` or empty component. A returned name never
/// starts or ends with `/`.
#[must_use]
pub fn normalize_entry_name(raw: &str) -> Option<String> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.contains('\\')
        || raw.chars().any(char::is_control)
    {
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

/// One owned compiler spool record, pinned by its decoded length and digest.
pub(crate) struct SpoolEntry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
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
pub fn write_archive(path: &Path, entries: Vec<PendingEntry>) -> Result<(), String> {
    write_archive_checked(path, entries, || Ok(()))
}

/// Compiler cancellation boundaries; encoding and atomic publication are the
/// same as `write_archive`. Never publish after the final check fails.
pub(crate) fn write_archive_checked(
    path: &Path,
    entries: Vec<PendingEntry>,
    check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    write_archive_guarded(path, entries, check, || Ok(()))
}

pub(crate) fn write_archive_guarded(
    path: &Path,
    entries: Vec<PendingEntry>,
    check: impl Fn() -> Result<(), String>,
    publish_check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    write_archive_with_level_guarded(path, entries, check, publish_check, 6)
}

pub(crate) fn write_archive_with_level_guarded(
    path: &Path,
    mut entries: Vec<PendingEntry>,
    check: impl Fn() -> Result<(), String>,
    publish_check: impl Fn() -> Result<(), String>,
    compression: i64,
) -> Result<(), String> {
    if !(1..=9).contains(&compression) {
        return Err("Invalid archive compression level".into());
    }
    check()?;
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    if entries
        .iter()
        .any(|entry| normalize_entry_name(&entry.name).as_deref() != Some(entry.name.as_str()))
        || entries.windows(2).any(|pair| {
            pair.first().map(|entry| &entry.name) == pair.get(1).map(|entry| &entry.name)
        })
    {
        return Err("Package entry names must be safe and unique".into());
    }
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
    // Own the temporary before writing or cleaning it up. Exclusive creation
    // never truncates a pre-existing file or follows a pre-existing symlink.
    let (temporary, file) = create_temporary(path)?;
    let result = write_archive_to(file, &entries, &check, compression)
        .and_then(|()| check())
        .and_then(|()| publish_check());
    if let Err(error) = result {
        return Err(cleanup_failed_write(&temporary, error));
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        cleanup_failed_write(
            &temporary,
            format!("could not publish {}: {error}", path.display()),
        )
    })
}

/// Streams a complete project from owned spool files into an atomic package.
/// Standalone writers keep their original aggregate bound. No larger candidate
/// is published before bounded index and virtual-member admission succeeds.
pub(crate) fn write_project_archive_files_guarded(
    path: &Path,
    entries: Vec<SpoolEntry>,
    check: impl Fn() -> Result<(), String>,
    publish_check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    write_project_archive_files_with_level_guarded(path, entries, check, publish_check, 6)
}

pub(crate) fn write_project_archive_files_with_level_guarded(
    path: &Path,
    mut entries: Vec<SpoolEntry>,
    check: impl Fn() -> Result<(), String>,
    publish_check: impl Fn() -> Result<(), String>,
    compression: i64,
) -> Result<(), String> {
    if !(1..=9).contains(&compression) {
        return Err("Invalid archive compression level".into());
    }
    check()?;
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    validate_spool_entries(&entries)?;
    let (temporary, file) = create_temporary(path)?;
    let result = write_project_archive_to(file, &entries, &check, compression)
        .and_then(|()| check())
        .and_then(|()| {
            let mut reader =
                PackageReader::new(open_file(&temporary)?).map_err(|error| error.to_string())?;
            let _index = project::read_index(&mut reader)?.ok_or("Missing project index")?;
            Ok(())
        })
        .and_then(|()| check())
        .and_then(|()| publish_check());
    if let Err(error) = result {
        return Err(cleanup_failed_write(&temporary, error));
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        cleanup_failed_write(
            &temporary,
            format!("could not publish {}: {error}", path.display()),
        )
    })
}

fn validate_spool_entries(entries: &[SpoolEntry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "package has {} entries (limit {MAX_ENTRIES})",
            entries.len()
        ));
    }
    if entries
        .iter()
        .any(|entry| normalize_entry_name(&entry.name).as_deref() != Some(entry.name.as_str()))
        || entries.windows(2).any(|pair| {
            pair.first().map(|entry| &entry.name) == pair.get(1).map(|entry| &entry.name)
        })
    {
        return Err("Package entry names must be safe and unique".into());
    }
    let mut total = 0_u64;
    let mut has_index = false;
    for entry in entries {
        total = total.saturating_add(entry.bytes);
        if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("Invalid spool digest for '{}'", entry.name));
        }
        if entry.name == project::INDEX {
            has_index = true;
            if entry.bytes > super::MAX_MANIFEST_BYTES {
                return Err("Project index exceeds manifest limit".into());
            }
        } else if entry.bytes > super::MAX_BINARY_BYTES
            || hash::sha256_from_blob_name(&entry.name).as_deref() != Some(entry.sha256.as_str())
        {
            return Err(format!("Invalid project spool record '{}'", entry.name));
        }
    }
    if !has_index {
        return Err("Project archive requires project.json".into());
    }
    if total > MAX_PROJECT_TOTAL_BYTES {
        return Err(format!(
            "package would hold {total} uncompressed bytes (limit {MAX_PROJECT_TOTAL_BYTES})"
        ));
    }
    Ok(())
}

fn write_project_archive_to(
    file: std::fs::File,
    entries: &[SpoolEntry],
    check: &impl Fn() -> Result<(), String>,
    compression: i64,
) -> Result<(), String> {
    let mut writer = zip::ZipWriter::new(file);
    for entry in entries {
        check()?;
        let mut input = open_spool_file(entry)?;
        writer
            .start_file(entry.name.as_str(), file_options(compression))
            .map_err(|error| format!("could not start entry '{}': {error}", entry.name))?;
        let (_, digest) = copy_bounded(&mut input, &mut writer, entry.bytes, entry.bytes, check)
            .map_err(|error| format!("could not write entry '{}': {error}", entry.name))?;
        if digest != entry.sha256
            || input.metadata().map_err(|error| error.to_string())?.len() != entry.bytes
        {
            return Err(format!(
                "Project spool record changed: {}",
                entry.path.display()
            ));
        }
    }
    finish_archive(writer)
}

/// Reject links/special files before opening and check the opened handle too.
/// Nonblocking/no-follow flags close the regular-file/FIFO and symlink races.
fn open_spool_file(entry: &SpoolEntry) -> Result<std::fs::File, String> {
    let check = |metadata: &std::fs::Metadata| {
        if metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == entry.bytes
        {
            Ok(())
        } else {
            Err(format!(
                "Invalid or changed project spool file: {}",
                entry.path.display()
            ))
        }
    };
    check(&std::fs::symlink_metadata(&entry.path).map_err(|error| error.to_string())?)?;
    let mut options = std::fs::OpenOptions::new();
    let _read = options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _flags = options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // Open the reparse point itself so handle validation can reject links.
        let _flags = options.custom_flags(0x0020_0000);
    }
    let input = options
        .open(&entry.path)
        .map_err(|error| error.to_string())?;
    check(&input.metadata().map_err(|error| error.to_string())?)?;
    Ok(input)
}

fn copy_bounded(
    input: impl Read,
    output: &mut impl Write,
    declared: u64,
    limit: u64,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(u64, String), String> {
    if declared > limit {
        return Err(format!("declares {declared} bytes (limit {limit})"));
    }
    let mut bounded_input = input.take(declared.saturating_add(1));
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    loop {
        check()?;
        let count = bounded_input
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if total > declared {
            return Err(format!("exceeds declared {declared}-byte size"));
        }
        let bytes = buffer.get(..count).ok_or("Read exceeds copy buffer")?;
        output.write_all(bytes).map_err(|error| error.to_string())?;
        hasher.update(bytes);
    }
    if total != declared {
        return Err(format!("declares {declared} bytes but read {total}"));
    }
    check()?;
    Ok((total, hash::digest_hex(&hasher.finalize())))
}

/// Retains the original failure and reports an owned temporary that could not
/// be removed. Missing temporaries require no cleanup.
pub(crate) fn cleanup_failed_write(path: &Path, error: String) -> String {
    if let Err(cleanup_error) = std::fs::remove_file(path)
        && cleanup_error.kind() != std::io::ErrorKind::NotFound
    {
        format!(
            "{error}; could not remove partial package {}: {cleanup_error}",
            path.display()
        )
    } else {
        error
    }
}

/// Writes a sorted entry list to an exclusively owned file before publication.
fn write_archive_to(
    file: std::fs::File,
    entries: &[PendingEntry],
    check: &impl Fn() -> Result<(), String>,
    compression: i64,
) -> Result<(), String> {
    let mut writer = zip::ZipWriter::new(file);
    for entry in entries {
        check()?;
        writer
            .start_file(entry.name.as_str(), file_options(compression))
            .map_err(|error| format!("could not start entry '{}': {error}", entry.name))?;
        writer
            .write_all(&entry.bytes)
            .map_err(|error| format!("could not write entry '{}': {error}", entry.name))?;
    }
    finish_archive(writer)
}

fn file_options(compression: i64) -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(compression))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644)
}

fn finish_archive(writer: zip::ZipWriter<std::fs::File>) -> Result<(), String> {
    let mut finished_file = writer
        .finish()
        .map_err(|error| format!("could not finish package: {error}"))?;
    finished_file
        .flush()
        .map_err(|error| format!("could not flush package: {error}"))?;
    finished_file
        .sync_all()
        .map_err(|error| format!("could not sync package: {error}"))?;
    Ok(())
}

/// A same-directory temporary path for atomic publication.
fn temporary_path(path: &Path, attempt: u8) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || std::ffi::OsString::from("package"),
        std::ffi::OsStr::to_os_string,
    );
    name.push(".partial");
    if attempt != 0 {
        name.push(format!("-{attempt}"));
    }
    path.with_file_name(name)
}

/// Skips existing interrupted artifacts without modifying files we do not own.
pub(crate) fn create_temporary(path: &Path) -> Result<(PathBuf, std::fs::File), String> {
    for attempt in 0_u8..16 {
        let temporary = temporary_path(path, attempt);
        // Project export already owns a same-named spool directory. Windows
        // reports AccessDenied for CreateNew on a directory, not AlreadyExists.
        // Preserve every occupied entry; exclusive creation still handles races.
        match std::fs::symlink_metadata(&temporary) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "could not inspect {}: {error}",
                    temporary.display()
                ));
            }
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!("could not create {}: {error}", temporary.display()));
            }
        }
    }
    Err(format!(
        "could not create a temporary for {}: all 16 sibling names exist",
        path.display()
    ))
}

/// Exclusively reserve an owned staging directory without a file/remove/mkdir gap.
pub(crate) fn create_temporary_directory(path: &Path) -> Result<PathBuf, String> {
    for attempt in 0_u8..16 {
        let temporary = temporary_path(path, attempt);
        match std::fs::create_dir(&temporary) {
            Ok(()) => return Ok(temporary),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("could not create {}: {error}", temporary.display())),
        }
    }
    Err(format!(
        "could not reserve staging for {}: all 16 sibling names exist",
        path.display()
    ))
}

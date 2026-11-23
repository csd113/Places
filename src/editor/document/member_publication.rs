//! Serializes manifest mutations by cooperating Places editors.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read as _, Seek as _, Write as _};
use std::path::Path;

use super::{Document, persistence};
use crate::engine::game_mode::{GameMode, MAX_MODE_BYTES};
use crate::engine::level::{LevelDef, MAX_LEVEL_JSON_BYTES};
use crate::engine::package::hash::sha256_hex;
use crate::engine::project::{MAX_MANIFEST_BYTES, ProjectFolder, ProjectLevel, read_bounded_file};

pub(super) fn lock(root: &Path) -> Result<File, String> {
    if fs::canonicalize(root).map_err(|error| error.to_string())? != root {
        return Err("Project root changed; reopen before editing membership".into());
    }
    // Keep the lock name stable across writers. Removing it on release would
    // let another process open a different object while a waiter still uses it.
    let path = root.join(".places-editor-project.lock");
    let file = confined_open()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            format!(
                "Could not open project writer lock {}: {error}",
                path.display()
            )
        })?;
    check_regular(&file, &path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => {
            Err("Another editor is publishing this project; try again".into())
        }
        Err(TryLockError::Error(error)) => {
            Err(format!("Could not lock the project for writing: {error}"))
        }
    }
}

fn confined_open() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _flags = options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // Permit readers and the created writer handle. Keep the checked
        // pathname from being renamed or deleted until the commit finishes.
        const SHARE_READ_WRITE: u32 = 3;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let _flags = options
            .share_mode(SHARE_READ_WRITE)
            .custom_flags(OPEN_REPARSE_POINT);
    }
    options
}

fn check_regular(file: &File, path: &Path) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(format!(
                "Project file {} must not be a reparse point",
                path.display()
            ));
        }
    }
    if !metadata.is_file() {
        return Err(format!(
            "Expected a regular project file at {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/member_recovery.rs"]
mod tests;

const PENDING_NAME: &str = ".places-editor-pending-member";
const RECORD_VERSION: u8 = 1;
const RECORD_START: u8 = b'{';
const CANCELED_START: u8 = b'!';

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PendingMember {
    version: u8,
    previous_manifest: String,
    next_manifest: String,
    source_digest: String,
    member: ProjectLevel,
}

/// Store exact source bytes beside the intent so an interrupted import no
/// longer depends on the original path or a completed source publication.
pub(super) fn stage(
    root: &Path,
    before: &[u8],
    after: &[u8],
    member: &ProjectLevel,
    source: &[u8],
) -> Result<(), String> {
    let record = PendingMember {
        version: RECORD_VERSION,
        previous_manifest: sha256_hex(before),
        next_manifest: sha256_hex(after),
        source_digest: sha256_hex(source),
        member: member.clone(),
    };
    let header = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    persistence::atomic_create_by_rename(&root.join(PENDING_NAME), &[&header, b"\n", source])
}

pub(super) fn finish(root: &Path) -> Result<(), String> {
    let path = root.join(PENDING_NAME);
    fs::remove_file(&path).map_err(|error| format!("Could not clear {}: {error}", path.display()))
}

pub(super) fn cancel(root: &Path, problem: String) -> String {
    let path = root.join(PENDING_NAME);
    if let Err(error) = mark_canceled(&path) {
        return format!(
            "Add/Import outcome is uncertain. Reopening may complete the operation. {problem}; could not durably cancel {}: {error}",
            path.display()
        );
    }
    match finish(root) {
        Ok(()) => problem,
        Err(error) => format!("{problem}; pending Add/Import canceled. {error}"),
    }
}

fn mark_canceled(path: &Path) -> Result<(), String> {
    let mut file = confined_open()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    check_regular(&file, path)?;
    let mut start = [0_u8];
    file.read_exact(&mut start)
        .map_err(|error| error.to_string())?;
    if start != [RECORD_START] {
        return Err("Pending Add/Import header changed".into());
    }
    // Persist the state before deleting the record or reporting a failure.
    // Changing its first byte leaves all staged source bytes intact.
    file.rewind()
        .and_then(|()| file.write_all(&[CANCELED_START]))
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())
}

pub(super) fn recover(document: &mut Document) -> Option<String> {
    let path = document.root().join(PENDING_NAME);
    if matches!(path.symlink_metadata(), Err(error) if error.kind() == io::ErrorKind::NotFound) {
        return None;
    }
    match recover_pending(document, &path) {
        Ok(true) => Some("Recovered a pending Add/Import".into()),
        Ok(false) => None,
        Err(error) => Some(format!(
            "Pending Add/Import preserved at {}: {error}",
            path.display()
        )),
    }
}

fn recover_pending(document: &mut Document, path: &Path) -> Result<bool, String> {
    let root = document.root().to_path_buf();
    let _writer = lock(&root)?;
    let mut record = read_record(
        path,
        MAX_MANIFEST_BYTES.saturating_add(MAX_LEVEL_JSON_BYTES),
    )?;
    let canceled = record.first() == Some(&CANCELED_START);
    if canceled && let Some(start) = record.first_mut() {
        *start = RECORD_START;
    }
    let (intent, source) = decode_record(&record)?;
    if canceled {
        finish(&root).map_err(|error| format!("Add/Import was canceled. {error}"))?;
        return Ok(false);
    }

    document.check_manifest()?;
    let current = sha256_hex(&document.manifest_bytes);
    let committed = current == intent.next_manifest
        && document.manifest().levels.last() == Some(&intent.member);
    if !committed && current != intent.previous_manifest {
        return Err("The project manifest changed since the operation began".into());
    }

    let member = &intent.member;
    let level = standalone_level(source)?;
    if member.id != level.id
        || member.source.as_str() != format!("levels/{}.json", level.id)
        || !member.scripts.is_empty()
        || member.thumbnail.is_some()
    {
        return Err("The pending level does not match its project membership".into());
    }
    check_levels_directory(&root)?;
    if committed {
        // The manifest can be installed even though a member no longer matches.
        verify_published(&document.project, member, source, &level)?;
        finish(&root)?;
        return Ok(false);
    }

    let mut manifest = document.manifest().clone();
    manifest.levels.push(member.clone());
    for destination in &level.transitions {
        let _declared = manifest.level(Some(destination))?;
    }
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    if sha256_hex(&bytes) != intent.next_manifest {
        return Err("The proposed manifest no longer matches the pending operation".into());
    }
    let size = u64::try_from(source.len()).map_err(|error| error.to_string())?;
    let candidate = document
        .project
        .with_manifest(&bytes, Some((&member.source, size)))?;
    let destination = root.join(member.source.as_str());
    let source_exists = check_source(&destination, source)?;

    if level.game_mode.is_some() {
        check_mode_path(&root, &level)?;
        super::game_modes::publish_copy(&root, &level)?;
    }
    if !source_exists {
        persistence::atomic_create_by_rename(&destination, &[source])?;
    }
    verify_published(&candidate, member, source, &level)?;
    document.commit_manifest(candidate, bytes)?;
    finish(&root).map_err(|error| format!("Level recovered, but {error}"))?;
    Ok(true)
}

fn verify_published(
    project: &ProjectFolder,
    member: &ProjectLevel,
    source: &[u8],
    level: &LevelDef,
) -> Result<(), String> {
    if level.game_mode.is_some() {
        let root = project.root();
        check_directory(&root.join("game_modes"))?;
        let mode = GameMode::from_level(level);
        let mode_path = root.join(mode.project_path()?.as_str());
        let published_mode = read_record(&mode_path, MAX_MODE_BYTES).map_err(|error| {
            format!(
                "Could not verify published game mode at {}: {error}",
                mode_path.display()
            )
        })?;
        if published_mode != mode.bytes()? {
            return Err(format!(
                "Published game mode bytes differ: {}",
                mode_path.display()
            ));
        }
    }
    let (path, published, _) = project.read_level(Some(&member.id))?;
    if published != source {
        return Err(format!(
            "Published level source differs: {}",
            path.display()
        ));
    }
    Ok(())
}

pub(super) fn standalone_level(bytes: &[u8]) -> Result<LevelDef, String> {
    super::source::check_editable_source(bytes)?;
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let id = raw
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or("Imported map needs an explicit stable ID")?;
    let (level, _byte_count) = persistence::validate_raw(&raw, id)?;
    if !level.scripts.is_empty() || !level.script_sources.is_empty() {
        return Err("Standalone JSON import cannot resolve script dependencies; recover the owning project instead".into());
    }
    Ok(level)
}

fn decode_record(record: &[u8]) -> Result<(PendingMember, &[u8]), String> {
    let mut sections = record.splitn(2, |byte| *byte == b'\n');
    let header = sections.next().ok_or("Missing pending member header")?;
    let source = sections.next().ok_or("Missing pending member source")?;
    if u64::try_from(header.len()).unwrap_or(u64::MAX) >= MAX_MANIFEST_BYTES
        || u64::try_from(source.len()).unwrap_or(u64::MAX) > MAX_LEVEL_JSON_BYTES
    {
        return Err("Pending Add/Import exceeds its size limit".into());
    }
    let intent: PendingMember = serde_json::from_slice(header)
        .map_err(|error| format!("Invalid pending member: {error}"))?;
    if intent.version != RECORD_VERSION || sha256_hex(source) != intent.source_digest {
        return Err("Pending Add/Import version or source digest does not match".into());
    }
    Ok((intent, source))
}

fn read_record(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = path.symlink_metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err(format!(
            "Expected a regular project file at {}",
            path.display()
        ));
    }
    read_bounded_file(path, limit)
}

fn check_source(path: &Path, source: &[u8]) -> Result<bool, String> {
    match path.symlink_metadata() {
        Ok(_) if read_record(path, MAX_LEVEL_JSON_BYTES)? == source => Ok(true),
        Ok(_) => Err(format!("Existing level source differs: {}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect {}: {error}", path.display())),
    }
}

pub(super) fn check_levels_directory(root: &Path) -> Result<(), String> {
    check_directory(&root.join("levels"))
}

fn check_directory(path: &Path) -> Result<(), String> {
    let metadata = path.symlink_metadata().map_err(|error| error.to_string())?;
    if !metadata.is_dir()
        || is_reparse_point(&metadata)
        || fs::canonicalize(path).map_err(|error| error.to_string())? != path
    {
        return Err(format!(
            "Expected a real directory inside this project at {}",
            path.display()
        ));
    }
    Ok(())
}

fn check_mode_path(root: &Path, level: &LevelDef) -> Result<(), String> {
    let directory = root.join("game_modes");
    match directory.symlink_metadata() {
        Ok(_) => check_directory(&directory)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    let mode = GameMode::from_level(level);
    let path = root.join(mode.project_path()?.as_str());
    match path.symlink_metadata() {
        Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => Ok(()),
        Ok(_) => Err(format!(
            "Expected a regular game mode file at {}",
            path.display()
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

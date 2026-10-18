//! Complete project distribution: shared prepared blobs and exact editable sources.
//! No extraction occurs while playing. A reader selects one virtual ordinary map.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, Write as _};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{PackageReader, hash};
use crate::engine::project::{MANIFEST_NAME, ProjectFolder, ProjectManifest, ProjectPath};

pub const INDEX: &str = "project.json";
pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_THUMBNAIL_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMap {
    pub id: String,
    /// Virtual ordinary package name -> shared content-addressed record.
    pub records: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPackage {
    pub schema: String,
    pub package_format: u32,
    pub project: ProjectManifest,
    pub levels: Vec<ProjectMap>,
    /// Exact source file bytes. Only declared levels/scripts/thumbnails are included.
    pub sources: BTreeMap<ProjectPath, String>,
}

/// Reads the bounded index and verifies its complete archive namespace.
/// # Errors
/// Rejects unsupported versions, missing/extra/unsafe records or membership conflicts.
pub fn read_index<R: Read + Seek>(
    reader: &mut PackageReader<R>,
) -> Result<Option<ProjectPackage>, String> {
    if reader.has_view() || !reader.contains(INDEX) {
        return Ok(None);
    }
    let bytes = reader.read_entry(INDEX, super::MAX_MANIFEST_BYTES)?;
    crate::engine::project::check_editable_source(&bytes)?;
    let index: ProjectPackage = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if index.schema != "places-project-package" || index.package_format != super::FORMAT_VERSION {
        return Err(
            "Unsupported project package schema/version; no supported conversion exists".into(),
        );
    }
    index.project.validate()?;
    if index.levels.len() != index.project.levels.len()
        || index.sources.len() > 1024
        || index
            .sources
            .keys()
            .filter(|path| path.as_str().starts_with("scripts/"))
            .count()
            > 256
    {
        return Err("Project package membership/source count is invalid".into());
    }
    let mut expected = BTreeSet::from([INDEX.to_string()]);
    for (compiled, authored) in index.levels.iter().zip(&index.project.levels) {
        if compiled.id != authored.id
            || compiled.records.len() > 512
            || !compiled.records.contains_key("manifest.json")
        {
            return Err("Project package level identity/record count is invalid".into());
        }
        for (name, record) in &compiled.records {
            if super::normalize_entry_name(name).as_deref() != Some(name.as_str()) {
                return Err("Unsafe virtual map entry".into());
            }
            check_record(reader, record)?;
            let _inserted = expected.insert(record.clone());
        }
    }
    let mut source_bytes = 0_u64;
    let mut paths = BTreeSet::new();
    for (path, record) in &index.sources {
        if !paths.insert(path.as_str().to_ascii_lowercase()) {
            return Err("Case-conflicting project source paths".into());
        }
        let limit = source_limit(path)?;
        let size = reader.entry_size(record)?;
        source_bytes = source_bytes.saturating_add(size);
        if size > limit || source_bytes > MAX_SOURCE_BYTES {
            return Err("Project source size limits exceeded".into());
        }
        check_record(reader, record)?;
        let _inserted = expected.insert(record.clone());
    }
    if expected.iter().ne(reader.names().iter())
        || !index.sources.keys().any(|p| p.as_str() == MANIFEST_NAME)
    {
        return Err("Project package contains undeclared or missing records".into());
    }
    Ok(Some(index))
}

fn check_record<R: Read + Seek>(reader: &mut PackageReader<R>, name: &str) -> Result<(), String> {
    if hash::sha256_from_blob_name(name).is_none() || !reader.contains(name) {
        return Err(format!("Invalid project blob: {name}"));
    }
    if reader.entry_size(name)? > super::MAX_BINARY_BYTES {
        return Err(format!("Project blob exceeds entry limit: {name}"));
    }
    Ok(())
}

pub(crate) fn select<R: Read + Seek>(
    reader: &mut PackageReader<R>,
    id: Option<&str>,
) -> Result<(), String> {
    if let Some(index) = read_index(reader)? {
        let selected = id.unwrap_or(&index.project.entry_level);
        let level = index
            .levels
            .iter()
            .find(|level| level.id == selected)
            .ok_or_else(|| format!("Project package has no level '{selected}'"))?;
        reader.select_view(
            level.records.clone(),
            index
                .project
                .levels
                .iter()
                .map(|record| record.id.clone())
                .collect(),
        );
    }
    Ok(())
}

/// Cheap discovery reads metadata only; it never loads other levels' semantics or geometry.
/// # Errors
/// Returns archive/index validation errors.
pub fn inspect(path: &Path) -> Result<Option<ProjectPackage>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = PackageReader::new(file).map_err(|e| e.to_string())?;
    read_index(&mut reader)
}

/// Reads only the project index and selected thumbnail blob; never prepares a world.
///
/// Entry-level image takes precedence over the package image. Other maps never
/// inherit the entry/package icon, keeping bundle menu associations explicit.
/// # Errors
/// Rejects unsafe/missing/corrupt blobs and invalid PNGs; callers may show fallback.
pub fn menu_thumbnail(
    path: &Path,
    level_id: &str,
) -> Result<Option<crate::engine::materials::RawImage>, String> {
    let mut reader = PackageReader::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let Some(index) = read_index(&mut reader)? else {
        return Ok(None);
    };
    let level = index.project.level(Some(level_id))?;
    let selected = level.thumbnail.as_ref().or_else(|| {
        (level_id == index.project.entry_level)
            .then_some(index.project.thumbnail.as_ref())
            .flatten()
    });
    let Some(source) = selected else {
        return Ok(None);
    };
    source.require_role("thumbnails", "png")?;
    let blob = index
        .sources
        .get(source)
        .ok_or("Missing menu thumbnail source")?;
    let bytes = reader.read_blob(blob, MAX_THUMBNAIL_BYTES)?;
    decode_thumbnail(&bytes).map(Some)
}

/// Checks every record hash and each level's semantics without loading prepared worlds.
/// # Errors
/// Returns corrupt record, script, dependency or index errors.
pub fn validate(path: &Path, check: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let mut reader = PackageReader::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if let Some(index) = read_index(&mut reader)? {
        validate_sources(&mut reader, &index, path, check)?;
        for name in reader.names().to_vec() {
            check()?;
            if name != INDEX {
                drop(reader.read_blob(&name, super::MAX_BINARY_BYTES)?);
            }
        }
    }
    Ok(())
}

fn validate_sources<R: Read + Seek>(
    reader: &mut PackageReader<R>,
    index: &ProjectPackage,
    package_path: &Path,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let mut files = BTreeMap::new();
    for (path, blob) in &index.sources {
        check()?;
        let bytes = reader.read_blob(blob, source_limit(path)?)?;
        if path.as_str().starts_with("thumbnails/") {
            validate_thumbnail(&bytes)?;
        }
        let _old = files.insert(path.as_str(), bytes);
    }
    let manifest_bytes = files
        .get(MANIFEST_NAME)
        .ok_or("Missing editable manifest")?;
    if ProjectManifest::from_json(manifest_bytes)? != index.project {
        return Err("Editable manifest disagrees with project index".into());
    }
    let mut expected = BTreeSet::from([MANIFEST_NAME.to_string()]);
    expected.extend(
        index
            .project
            .scripts
            .iter()
            .map(|path| path.as_str().to_string()),
    );
    expected.extend(
        index
            .project
            .thumbnail
            .iter()
            .map(|path| path.as_str().to_string()),
    );
    for record in &index.project.levels {
        check()?;
        let bytes = files
            .get(record.source.as_str())
            .ok_or("Missing editable level")?;
        crate::engine::project::check_editable_source(bytes)?;
        let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        crate::engine::project::metadata::validate_raw(&raw)?;
        crate::engine::project::metadata::validate_drafts(&raw)?;
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let mut level =
            crate::engine::level::LevelDef::from_json(text).map_err(|e| e.to_string())?;
        crate::engine::loader::validate_level(&level)?;
        if level.id != record.id || !level.script_sources.is_empty() {
            return Err("Invalid editable level identity or inline script snapshots".into());
        }
        for destination in &level.transitions {
            let _declared = index.project.level(Some(destination))?;
        }
        let _inserted = expected.insert(record.source.as_str().to_string());
        expected.extend(
            record
                .thumbnail
                .iter()
                .map(|path| path.as_str().to_string()),
        );
        expected.extend(record.scripts.iter().map(|path| path.as_str().to_string()));
        let mut modules = BTreeSet::new();
        for path in index
            .project
            .scripts
            .iter()
            .chain(&record.scripts)
            .chain(&level.scripts)
        {
            let _module_inserted = expected.insert(path.as_str().to_string());
            if modules.insert(path.as_str()) {
                let source =
                    std::str::from_utf8(files.get(path.as_str()).ok_or("Missing editable script")?)
                        .map_err(|e| e.to_string())?;
                level
                    .script_sources
                    .push(crate::engine::scripting::ScriptSource {
                        path: path.clone(),
                        source: source.into(),
                    });
            }
        }
        crate::engine::scripting::check_level(&level)?;
        let compiled = super::world::open_level(package_path, &record.id)?;
        if compiled.level.script_sources != level.script_sources
            || compiled.level.scripts != level.scripts
            || compiled.level.transitions != level.transitions
            || compiled.level.name != level.name
            || compiled.level.author != level.author
        {
            return Err(format!(
                "Editable/runtime identity, script or transition mismatch in '{}'",
                record.id
            ));
        }
    }
    if expected
        .iter()
        .map(String::as_str)
        .ne(files.keys().copied())
    {
        return Err("Missing or undeclared editable source files".into());
    }
    Ok(())
}

fn source_limit(path: &ProjectPath) -> Result<u64, String> {
    if path.as_str() == MANIFEST_NAME {
        return Ok(crate::engine::project::MAX_MANIFEST_BYTES);
    }
    for (folder, extension, limit) in [
        ("levels", "json", crate::engine::level::MAX_LEVEL_JSON_BYTES),
        (
            "scripts",
            "luau",
            crate::engine::scripting::MAX_SOURCE_BYTES,
        ),
        ("thumbnails", "png", MAX_THUMBNAIL_BYTES),
    ] {
        if path.require_role(folder, extension).is_ok() {
            return Ok(limit);
        }
    }
    Err(format!(
        "Unsupported project source role: {}",
        path.as_str()
    ))
}

/// PNG thumbnail contract: landscape 16:9, RGB/RGBA8, at most 1280x720; no animation.
/// # Errors
/// Rejects malformed image data before any project is published.
pub fn validate_thumbnail(bytes: &[u8]) -> Result<(), String> {
    decode_thumbnail(bytes).map(|_image| ())
}

/// Decodes the thumbnail-specific PNG contract, independently of world texture limits.
/// # Errors
/// Rejects malformed/oversized pixels, unsafe formats and animation before use.
pub fn decode_thumbnail(bytes: &[u8]) -> Result<crate::engine::materials::RawImage, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_THUMBNAIL_BYTES {
        return Err("Thumbnail exceeds 4 MiB".into());
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 8 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("Invalid thumbnail PNG: {e}"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 1280
        || info.height > 720
        || info.width.saturating_mul(9) != info.height.saturating_mul(16)
        || info.bit_depth != png::BitDepth::Eight
        || !matches!(info.color_type, png::ColorType::Rgb | png::ColorType::Rgba)
        || info.animation_control.is_some()
    {
        return Err("Thumbnail must be a 16:9 RGB/RGBA8 PNG, maximum 1280x720".into());
    }
    let (width, height) = (info.width, info.height);
    let mut pixels = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("Thumbnail decode size overflow")?
    ];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|e| format!("Invalid thumbnail pixels: {e}"))?;
    pixels.truncate(frame.buffer_size());
    let rgba = if frame.color_type == png::ColorType::Rgba {
        pixels
    } else {
        pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| pixel.iter().copied().chain(std::iter::once(255)))
            .collect()
    };
    reader
        .finish()
        .map_err(|e| format!("Invalid thumbnail end: {e}"))?;
    Ok(crate::engine::materials::RawImage::new(width, height, rgba))
}

/// Validates and snapshots the complete declared editable file set without compiling.
/// # Errors
/// Returns confined source, script, transition or thumbnail errors.
pub fn snapshot(
    project: &ProjectFolder,
    check: &impl Fn() -> Result<(), String>,
) -> Result<BTreeMap<ProjectPath, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut paths = BTreeSet::from([ProjectPath::new(MANIFEST_NAME.into())?]);
    paths.extend(project.manifest().scripts.iter().cloned());
    paths.extend(project.manifest().thumbnail.iter().cloned());
    for record in &project.manifest().levels {
        check()?;
        let (_, bytes, mut level) = project.read_level(Some(&record.id))?;
        crate::engine::project::check_editable_source(&bytes)?;
        let _inserted = paths.insert(record.source.clone());
        paths.extend(record.scripts.iter().cloned());
        paths.extend(record.thumbnail.iter().cloned());
        paths.extend(level.scripts.iter().cloned());
        project.attach_scripts(&mut level)?;
        crate::engine::scripting::check_level(&level)?;
    }
    if paths.len() > 1024
        || paths
            .iter()
            .filter(|path| path.as_str().starts_with("scripts/"))
            .count()
            > 256
    {
        return Err("Project exceeds source/module file count limits".into());
    }
    let mut total = 0_u64;
    let mut portable = BTreeSet::new();
    for path in paths {
        check()?;
        if !portable.insert(path.as_str().to_ascii_lowercase()) {
            return Err("Case-conflicting project source paths".into());
        }
        let bytes = crate::engine::project::read_bounded_file(
            &project.resolve(&path)?,
            source_limit(&path)?,
        )?;
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if total > MAX_SOURCE_BYTES {
            return Err("Editable project sources exceed 64 MiB".into());
        }
        if path.as_str().starts_with("thumbnails/") {
            validate_thumbnail(&bytes)?;
        }
        let _previous = files.insert(path, bytes);
    }
    let manifest_path = ProjectPath::new(MANIFEST_NAME.into())?;
    let manifest_bytes = files
        .get(&manifest_path)
        .ok_or("Missing project manifest snapshot")?;
    if ProjectManifest::from_json(manifest_bytes)? != *project.manifest() {
        return Err("Project manifest changed during validation; reopen and retry".into());
    }
    Ok(files)
}

/// Recovers exact authored bytes into a newly reserved directory. Existing paths are refused.
/// Each file is confined, bounded and hash checked; failures remove only this operation's staging.
/// # Errors
/// Returns invalid archive/source errors, cancellation or destination conflicts.
pub fn recover(
    source: &Path,
    target: &Path,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    crate::engine::progress::stage("validating complete project archive");
    validate(source, check)?;
    let mut reader = PackageReader::new(File::open(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let index = read_index(&mut reader)?
        .ok_or("This is a compiled single map; editable project sources are unavailable")?;
    if target.try_exists().map_err(|e| e.to_string())? {
        return Err("Recovery destination already exists; choose a new folder".into());
    }
    let (temporary, reservation) = super::archive::create_temporary(target)?;
    drop(reservation);
    fs::remove_file(&temporary).map_err(|e| e.to_string())?;
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        crate::engine::progress::stage("recovering editable source files");
        let mut total = 0_u64;
        for (path, blob) in &index.sources {
            check()?;
            let bytes = reader.read_blob(blob, source_limit(path)?)?;
            total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            if total > MAX_SOURCE_BYTES {
                return Err("Editable project sources exceed 64 MiB".into());
            }
            let destination = temporary.join(path.as_str());
            fs::create_dir_all(destination.parent().ok_or("Source has no parent")?)
                .map_err(|e| e.to_string())?;
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
        }
        let project = ProjectFolder::open(&temporary)?;
        if project.manifest() != &index.project {
            return Err("Recovery manifest disagrees with project index".into());
        }
        let files = snapshot(&project, check)?;
        if files.keys().ne(index.sources.keys()) {
            return Err("Recovery has missing or undeclared source files".into());
        }
        check()?;
        // Never replace even an empty directory that appeared while validating.
        if target.symlink_metadata().is_ok() {
            return Err("Recovery destination appeared while importing".into());
        }
        crate::engine::progress::stage("publishing recovered source project");
        publish_directory(&temporary, target)
    })();
    if result.is_err() {
        fs::remove_dir_all(&temporary).map_err(|e| {
            format!(
                "Recovery failed; staging cleanup {}: {e}",
                temporary.display()
            )
        })?;
    }
    result
}

/// Atomic directory publication with no replacement, including a concurrently created empty folder.
fn publish_directory(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        let from =
            std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let to =
            std::ffi::CString::new(target.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        // SAFETY: both paths are owned NUL-terminated C strings valid for the call;
        // RENAME_EXCL requests atomic refusal when any destination exists.
        let status = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(target_os = "linux")]
        // SAFETY: owned NUL-terminated paths and valid AT_FDCWD constants; the kernel
        // performs an atomic no-replace rename without retaining either pointer.
        let status = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Err("Atomic project recovery is supported on macOS, Linux and Windows".into());
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if status == 0 {
            Ok(())
        } else {
            Err(format!(
                "Could not publish recovered project: {}",
                std::io::Error::last_os_error()
            ))
        }
    }
    #[cfg(not(unix))]
    {
        // Windows directory rename refuses an existing destination, including empty folders.
        fs::rename(source, target).map_err(|e| e.to_string())
    }
}

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::{MANIFEST_NAME, MAX_MANIFEST_BYTES, ProjectManifest, ProjectPath};
use crate::engine::level::LevelDef;

const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 4 * 1024 * 1024;

/// Validated project metadata and its canonical root. No global current project
/// or working-directory changes: callers explicitly carry this value.
#[derive(Clone, Debug)]
pub struct ProjectFolder {
    root: PathBuf,
    manifest: ProjectManifest,
}

impl ProjectFolder {
    /// Opens a project directory or its named manifest; validates every declared
    /// file's confinement, type and size. Does not compile maps or execute code.
    ///
    /// # Errors
    /// Reports missing/unsupported metadata, unsafe symbolic links or bad files.
    pub fn open(input: &Path) -> Result<Self, String> {
        let path = if input.is_dir() {
            input.join(MANIFEST_NAME)
        } else {
            input.to_path_buf()
        };
        if path.file_name().is_none_or(|name| name != MANIFEST_NAME) {
            return Err(format!("project manifest must be named {MANIFEST_NAME}"));
        }
        let root = fs::canonicalize(
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )
        .map_err(|error| format!("project root {}: {error}", path.display()))?;
        let canonical =
            fs::canonicalize(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if !canonical.starts_with(&root) {
            return Err("project manifest symlink escapes project root".into());
        }
        let manifest =
            ProjectManifest::from_json(&read_bounded_file(&canonical, MAX_MANIFEST_BYTES)?)?;
        let project = Self { root, manifest };
        project.validate_files()?;
        Ok(project)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    /// Resolves an existing regular file while rejecting symbolic-link escapes.
    ///
    /// # Errors
    /// Returns a missing, nonregular or escaped file error with its path.
    pub fn resolve(&self, path: &ProjectPath) -> Result<PathBuf, String> {
        let resolved = fs::canonicalize(self.root.join(path.as_str()))
            .map_err(|error| format!("project file '{}': {error}", path.as_str()))?;
        if !resolved.starts_with(&self.root) || !resolved.is_file() {
            return Err(format!(
                "project file '{}' escapes root or is not a regular file",
                path.as_str()
            ));
        }
        Ok(resolved)
    }

    /// Reads a selected source through the engine's source parser and validator.
    ///
    /// # Errors
    /// Rejects unsupported/invalid maps, path errors and mismatching level IDs.
    pub fn read_level(&self, id: Option<&str>) -> Result<(PathBuf, Vec<u8>, LevelDef), String> {
        let selected = self.manifest.level(id)?;
        let path = self.resolve(&selected.source)?;
        let bytes = read_bounded_file(&path, crate::engine::level::MAX_LEVEL_JSON_BYTES)?;
        let text =
            std::str::from_utf8(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
        let level =
            LevelDef::from_json(text).map_err(|error| format!("{}: {error}", path.display()))?;
        crate::engine::loader::validate_level(&level)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if level.id != selected.id {
            return Err(format!(
                "project level '{}' has source ID '{}'",
                selected.id, level.id
            ));
        }
        Ok((path, bytes, level))
    }

    fn validate_files(&self) -> Result<(), String> {
        let mut files = Vec::new();
        for level in &self.manifest.levels {
            files.push((&level.source, crate::engine::level::MAX_LEVEL_JSON_BYTES));
            files.extend(level.scripts.iter().map(|path| (path, MAX_SCRIPT_BYTES)));
            files.extend(
                level
                    .thumbnail
                    .iter()
                    .map(|path| (path, MAX_THUMBNAIL_BYTES)),
            );
        }
        files.extend(
            self.manifest
                .scripts
                .iter()
                .map(|path| (path, MAX_SCRIPT_BYTES)),
        );
        files.extend(
            self.manifest
                .thumbnail
                .iter()
                .map(|path| (path, MAX_THUMBNAIL_BYTES)),
        );
        for (path, limit) in files {
            let resolved = self.resolve(path)?;
            let metadata = fs::metadata(&resolved).map_err(|error| error.to_string())?;
            if metadata.len() == 0 || metadata.len() > limit {
                return Err(format!(
                    "project file '{}' must contain 1..={limit} bytes",
                    path.as_str()
                ));
            }
        }
        Ok(())
    }
}

/// Reads a regular file through a hard byte cap, including if it grows after
/// opening. Used for both standalone map sources and project metadata.
///
/// # Errors
/// Reports file/type/read errors or exceeded limits without an unbounded read.
pub fn read_bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let before = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !before.is_file() || before.len() > limit {
        return Err(format!(
            "{} must be a regular file of at most {limit} bytes",
            path.display()
        ));
    }
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "{} must be a regular file of at most {limit} bytes",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    let _read = file
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(bytes)
}

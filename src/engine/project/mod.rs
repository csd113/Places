//! Shared authoring-project contract. No UI, importers, script VM or user-data
//! discovery lives here. The player continues to load compiled map packages.

mod assets;
mod folder;
mod json;
pub(crate) mod metadata;
mod path;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use assets::{BuiltinAssetKind, BuiltinAssetRef, validate_level_assets};
pub use folder::{ProjectFolder, read_bounded_file};
pub use json::check_editable_source;
pub use path::ProjectPath;

pub const MANIFEST_NAME: &str = "places.project.json";
pub const PROJECT_SCHEMA: &str = "places-project";
pub const PROJECT_FORMAT_VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: u64 = 262_144;
pub const MAX_PROJECT_LEVELS: usize = 256;
pub const MAX_SCRIPT_REFERENCES: usize = 256;

/// One source level. Its ID must also match `LevelDef.id`; a level never gets
/// a different runtime identity merely because it belongs to a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectLevel {
    pub id: String,
    pub source: ProjectPath,
    /// Optional menu title, read without loading inactive world semantics.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<ProjectPath>,
    /// Shared or selected-level Luau references snapshotted by project builds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ProjectPath>,
}

/// The only supported authoring-project manifest. Paths are project-relative,
/// portable and confined; maps retain the engine's one `LevelDef` schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema: String,
    pub project_format: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<ProjectPath>,
    pub entry_level: String,
    /// Show only the entry map in Explore while retaining every internal member.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub menu_entry_only: bool,
    pub levels: Vec<ProjectLevel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ProjectPath>,
}

impl ProjectManifest {
    /// Parses bounded metadata without touching the filesystem.
    ///
    /// # Errors
    /// Rejects unsupported schemas, unknown fields, unsafe paths and invalid
    /// identities, references or collection limits.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_MANIFEST_BYTES {
            return Err("project manifest exceeds 256 KiB".into());
        }
        check_editable_source(bytes)?;
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid project manifest: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates even values constructed directly by an editor.
    ///
    /// # Errors
    /// Returns the first unsupported schema, invalid field or reference.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PROJECT_SCHEMA || self.project_format != PROJECT_FORMAT_VERSION {
            return Err(format!(
                "unsupported project schema/format; expected {PROJECT_SCHEMA} version {PROJECT_FORMAT_VERSION}"
            ));
        }
        validate_id(&self.id)?;
        validate_text(&self.name, 256, false)?;
        validate_text(&self.author, 256, true)?;
        validate_text(&self.description, 4096, true)?;
        validate_thumbnail(self.thumbnail.as_ref())?;
        if self.levels.is_empty() || self.levels.len() > MAX_PROJECT_LEVELS {
            return Err("project requires 1..=256 levels".into());
        }
        let mut ids = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let mut script_count = 0_usize;
        validate_scripts(&self.scripts, &mut script_count)?;
        for level in &self.levels {
            validate_id(&level.id)?;
            level.source.require_role("levels", "json")?;
            // Conservative portable uniqueness also prevents case-folded output
            // collisions on the default macOS and Windows filesystems.
            if !ids.insert(level.id.to_ascii_lowercase())
                || !sources.insert(level.source.as_str().to_ascii_lowercase())
            {
                return Err(format!(
                    "duplicate project level ID or source: {}",
                    level.id
                ));
            }
            validate_text(&level.display_name, 256, true)?;
            validate_text(&level.description, 4096, true)?;
            validate_thumbnail(level.thumbnail.as_ref())?;
            validate_scripts(&level.scripts, &mut script_count)?;
        }
        if !self.levels.iter().any(|level| level.id == self.entry_level) {
            return Err(format!(
                "entry_level '{}' is not a declared level",
                self.entry_level
            ));
        }
        // An editor-created value must also be readable after serialization.
        // Count through serde without allocating a second metadata buffer.
        serde_json::to_writer(ManifestByteBudget(MAX_MANIFEST_BYTES), self)
            .map_err(|error| format!("project manifest exceeds serialization limits: {error}"))?;
        Ok(())
    }

    /// Finds a level by exact stable ID, using the entry level when omitted.
    ///
    /// # Errors
    /// Returns an error for a missing ID; it never silently selects another map.
    pub fn level(&self, id: Option<&str>) -> Result<&ProjectLevel, String> {
        let selected = id.unwrap_or(&self.entry_level);
        self.levels
            .iter()
            .find(|level| level.id == selected)
            .ok_or_else(|| format!("project has no level '{selected}'"))
    }
}

struct ManifestByteBudget(u64);

impl std::io::Write for ManifestByteBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let length = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
        self.0 = self
            .0
            .checked_sub(length)
            .ok_or_else(|| std::io::Error::other("project manifest exceeds 256 KiB"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 96
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(format!(
            "invalid project/level ID '{id}': use 1..=96 lowercase letters, digits, '_' or '-'"
        ));
    }
    // File output uses this ID. Apply the same portable basename rules.
    drop(ProjectPath::new(format!("levels/{id}.json"))?);
    Ok(())
}

fn validate_text(text: &str, limit: usize, empty: bool) -> Result<(), String> {
    if text.len() > limit
        || (!empty && text.trim().is_empty())
        || text.chars().any(char::is_control)
    {
        return Err(format!(
            "project text must contain at most {limit} bytes without control characters"
        ));
    }
    Ok(())
}

fn validate_thumbnail(path: Option<&ProjectPath>) -> Result<(), String> {
    path.map_or(Ok(()), |value| value.require_role("thumbnails", "png"))
}

fn validate_scripts(paths: &[ProjectPath], count: &mut usize) -> Result<(), String> {
    *count = count.saturating_add(paths.len());
    if *count > MAX_SCRIPT_REFERENCES {
        return Err("project exceeds 256 script references".into());
    }
    let mut unique = BTreeSet::new();
    for path in paths {
        path.require_role("scripts", "luau")?;
        if !unique.insert(path.as_str().to_ascii_lowercase()) {
            return Err(format!("duplicate script reference: {}", path.as_str()));
        }
    }
    Ok(())
}

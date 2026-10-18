//! Project-to-map build planning. Each output remains an ordinary single-level
//! `.placesmap`; complete project orchestration lives in `distribution`.

use std::path::{Path, PathBuf};

use crate::engine::level::LevelDef;
use crate::engine::project::{MANIFEST_NAME, ProjectFolder, read_bounded_file};

/// One validated input snapshot. Project metadata participates in compiler
/// identity, so changing membership/references cannot silently reuse an output.
pub(super) struct SourceSnapshot {
    pub level: LevelDef,
    pub identity: Vec<u8>,
}

/// Reads either a standalone map or an explicitly selected project level.
pub(super) fn read_source(input: &Path, selected: Option<&str>) -> Result<SourceSnapshot, String> {
    if is_project(input) {
        let project = ProjectFolder::open(input)?;
        let (_, bytes, mut level) = project.read_level(selected)?;
        project.attach_scripts(&mut level)?;
        crate::engine::scripting::check_level(&level)?;
        let mut identity =
            serde_json::to_vec(project.manifest()).map_err(|error| error.to_string())?;
        // JSON cannot contain a literal NUL, making the boundary unambiguous.
        identity.push(0);
        identity.extend(bytes);
        identity.push(0);
        identity
            .extend(serde_json::to_vec(&level.script_sources).map_err(|error| error.to_string())?);
        Ok(SourceSnapshot { level, identity })
    } else {
        if selected.is_some() {
            return Err("--level requires a project folder or places.project.json".into());
        }
        let bytes = read_bounded_file(input, crate::engine::level::MAX_LEVEL_JSON_BYTES)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| format!("level source is not valid UTF-8: {error}"))?;
        let level =
            LevelDef::from_json(text).map_err(|error| format!("invalid level source: {error}"))?;
        if !level.scripts.is_empty() || !level.script_sources.is_empty() {
            return Err("Scripted maps must be built through their owning project".into());
        }
        crate::engine::loader::validate_level(&level)?;
        Ok(SourceSnapshot {
            level,
            identity: bytes,
        })
    }
}

fn is_project(input: &Path) -> bool {
    input.is_dir() || input.file_name().is_some_and(|name| name == MANIFEST_NAME)
}

/// Resolves the default single-map output without creating directories. Project
/// outputs are siblings of their source maps; explicit `--out` still wins.
///
/// # Errors
/// Rejects invalid projects, undeclared levels and unsafe project paths.
pub fn output_path(input: &Path, selected: Option<&str>) -> Result<PathBuf, String> {
    if is_project(input) {
        let project = ProjectFolder::open(input)?;
        let source = &project.manifest().level(selected)?.source;
        Ok(super::package_path_for(&project.resolve(source)?))
    } else if selected.is_some() {
        Err("--level requires a project folder or places.project.json".into())
    } else {
        Ok(super::package_path_for(input))
    }
}

/// Plans one package per manifest level in declared order.
///
/// Planning validates every source before any output is written. The output directory is explicit
/// and must exist; no install or discovery policy is implied.
///
/// # Errors
/// Returns project/source errors or a nonexistent output directory.
pub fn build_plan(template: &super::BuildRequest) -> Result<Vec<super::BuildRequest>, String> {
    if !template.out.is_dir() {
        return Err(format!(
            "project output directory does not exist: {}",
            template.out.display()
        ));
    }
    let project = ProjectFolder::open(&template.source)?;
    let mut requests = Vec::with_capacity(project.manifest().levels.len());
    for level in &project.manifest().levels {
        drop(project.read_level(Some(&level.id))?);
        let mut request = template.clone();
        request.project_level = Some(level.id.clone());
        request.out = template.out.join(format!("{}.placesmap", level.id));
        requests.push(request);
    }
    Ok(requests)
}

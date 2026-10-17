use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use super::History;
use crate::engine::level::{LevelDef, MAX_LEVEL_JSON_BYTES};
use crate::engine::project::{
    MANIFEST_NAME, MAX_MANIFEST_BYTES, ProjectFolder, ProjectManifest, read_bounded_file,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct LevelDocument {
    pub(super) raw: Value,
    pub(super) level: LevelDef,
    pub(super) byte_count: usize,
    pub(super) history: History,
    path: PathBuf,
    saved: Value,
    disk_bytes: Vec<u8>,
}

impl LevelDocument {
    pub(super) fn open(project: &ProjectFolder, id: Option<&str>) -> Result<Self, String> {
        let (path, disk_bytes, level) = project.read_level(id)?;
        super::source::check_editable_source(&disk_bytes)?;
        let raw: Value = serde_json::from_slice(&disk_bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        // The typed view is read through ProjectFolder, but never serialized.
        let byte_count = disk_bytes.len();
        Ok(Self {
            saved: raw.clone(),
            raw,
            level,
            byte_count,
            history: History::default(),
            path,
            disk_bytes,
        })
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.raw != self.saved
    }

    pub(super) fn preflight(&self, project: &ProjectFolder) -> Result<(), String> {
        let source = &project.manifest().level(Some(&self.level.id))?.source;
        let resolved = project.resolve(source)?;
        if resolved != self.path {
            return Err(format!(
                "Source path changed outside the editor: {}",
                self.path.display()
            ));
        }
        if read_bounded_file(&resolved, MAX_LEVEL_JSON_BYTES)? != self.disk_bytes {
            return Err(format!(
                "Source changed outside the editor; reopen before saving: {}",
                self.path.display()
            ));
        }
        drop(validate_raw(&self.raw, &self.level.id)?);
        Ok(())
    }

    pub(super) fn save(&mut self, project: &ProjectFolder) -> Result<(), String> {
        if !self.is_dirty() {
            return Ok(());
        }
        self.preflight(project)?;
        let mut bytes = serde_json::to_vec_pretty(&self.raw).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LEVEL_JSON_BYTES {
            return Err("Formatted level source exceeds the engine source limit".into());
        }
        atomic_replace(&self.path, &bytes)?;
        self.disk_bytes = bytes;
        self.saved = self.raw.clone();
        Ok(())
    }
}

pub(super) fn read_manifest(project: &ProjectFolder) -> Result<Vec<u8>, String> {
    let manifest = fs::canonicalize(project.root().join(MANIFEST_NAME))
        .map_err(|error| format!("Project manifest: {error}"))?;
    if !manifest.starts_with(project.root()) {
        return Err("Project manifest now escapes the project root".into());
    }
    read_bounded_file(&manifest, MAX_MANIFEST_BYTES)
}

pub(super) fn validate_raw(raw: &Value, expected_id: &str) -> Result<(LevelDef, usize), String> {
    let bytes = serde_json::to_vec(raw).map_err(|error| format!("Level JSON: {error}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LEVEL_JSON_BYTES {
        return Err("Level source exceeds the engine source limit".into());
    }
    let level: LevelDef = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Unsupported or invalid scene: {error}"))?;
    crate::engine::loader::validate_level(&level)?;
    if level.id != expected_id {
        return Err("The scene ID must match its project level ID".into());
    }
    Ok((level, bytes.len()))
}

pub(super) fn create_project(path: &Path, id: &str, name: &str) -> Result<(), String> {
    let manifest = serde_json::json!({
        "schema": "places-project", "project_format": 1_u32,
        "id": id, "name": name, "entry_level": id,
        "levels": [{"id": id, "source": format!("levels/{id}.json")}]
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    drop(ProjectManifest::from_json(&manifest_bytes)?);
    let scene = starter_scene(id, name);
    drop(validate_raw(&scene, id)?);
    let source_bytes = serde_json::to_vec_pretty(&scene).map_err(|error| error.to_string())?;
    fs::create_dir(path).map_err(|error| format!("New project {}: {error}", path.display()))?;
    let levels = path.join("levels");
    let source = levels.join(format!("{id}.json"));
    let manifest_path = path.join(MANIFEST_NAME);
    let mut created_files = Vec::new();
    let mut created_levels = false;
    let result = (|| {
        fs::create_dir(&levels).map_err(|error| error.to_string())?;
        created_levels = true;
        for (destination, bytes) in [(&source, &source_bytes), (&manifest_path, &manifest_bytes)] {
            let mut file = open_new(destination)?;
            created_files.push(destination);
            write_contents(&mut file, destination, bytes)?;
        }
        Ok(())
    })();
    if result.is_err() {
        // Remove only files this operation created. Never recursively remove
        // a chosen path or anything concurrently added by the user.
        for owned in created_files {
            let _cleanup = fs::remove_file(owned);
        }
        if created_levels {
            let _cleanup_levels = fs::remove_dir(&levels);
        }
        let _cleanup_root = fs::remove_dir(path);
    }
    result
}

fn starter_scene(id: &str, name: &str) -> Value {
    serde_json::json!({
        "schema": "places-level", "format_version": 1_u32,
        "id": id, "name": name,
        "spawn": {"x": 3.0_f32, "z": 4.5_f32, "yaw_degrees": 0.0_f32},
        "rooms": [{"x": 0.0_f32, "z": 0.0_f32, "width": 6.0_f32, "depth": 6.0_f32, "height": 2.7_f32}],
        "walls": [], "props": [],
        "ceiling_lights": [{"id": "editor_light", "fixture": "core:fluorescent_panel_01", "x": 3.0_f32, "z": 3.0_f32}]
    })
}

fn open_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn write_contents(file: &mut File, path: &Path, bytes: &[u8]) -> Result<(), String> {
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Source has no parent folder")?;
    let (temporary, mut file) = reserve_temporary(parent)?;
    let result = (|| {
        write_contents(&mut file, &temporary, bytes)?;
        let permissions = fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions();
        fs::set_permissions(&temporary, permissions).map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not publish {} atomically: {error}", path.display()))
    })();
    if temporary.exists() {
        let _cleanup = fs::remove_file(&temporary);
    }
    result
}

fn reserve_temporary(parent: &Path) -> Result<(PathBuf, File), String> {
    for _attempt in 0_u8..16 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".places-editor-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("{}: {error}", temporary.display())),
        }
    }
    Err("Could not reserve an atomic-save temporary file".into())
}

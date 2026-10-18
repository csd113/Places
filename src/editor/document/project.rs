//! Explicit project metadata operations, guarded by ordinary saves/conflict checks.
use super::{Document, persistence};
use crate::engine::project::{MANIFEST_NAME, ProjectFolder, ProjectLevel, ProjectPath};
use std::io::Write as _;

impl Document {
    /// Imports one standalone JSON map under its existing stable ID. Original
    /// bytes, unknown fields and per-placement intent remain unchanged. The
    /// source is declared only after complete validated, no-replace publication.
    ///
    /// # Errors
    /// Refuses duplicate IDs/files, invalid source, undeclared destinations and
    /// external script dependencies. It never guesses IDs or copies scripts.
    pub fn import_level(&mut self, source: &std::path::Path) -> Result<String, String> {
        let bytes = crate::engine::project::read_bounded_file(
            source,
            crate::engine::level::MAX_LEVEL_JSON_BYTES,
        )?;
        super::source::check_editable_source(&bytes)?;
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let id = raw
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or("Imported map needs an explicit stable ID")?;
        let (level, _byte_count) = persistence::validate_raw(&raw, id)?;
        if !level.scripts.is_empty() || !level.script_sources.is_empty() {
            return Err("Standalone JSON import cannot resolve script dependencies; recover the owning project instead".into());
        }
        let mut manifest = self.manifest().clone();
        let target = ProjectPath::new(format!("levels/{id}.json"))?;
        manifest.levels.push(ProjectLevel {
            id: id.into(),
            source: target.clone(),
            display_name: level.name.clone(),
            description: String::new(),
            thumbnail: None,
            scripts: Vec::new(),
        });
        manifest.validate()?;
        for destination in &level.transitions {
            let _declared = manifest.level(Some(destination))?;
        }
        let levels = self.root().join("levels");
        if std::fs::symlink_metadata(&levels)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
            || std::fs::canonicalize(&levels).map_err(|error| error.to_string())? != levels
        {
            return Err("Levels directory must be a real directory inside this project".into());
        }
        self.save_all()?;
        let path = self.root().join(target.as_str());
        persistence::atomic_create(&path, &bytes)?;
        if let Err(error) = self.publish_manifest(&manifest) {
            // Delete only the exact source this operation published. A source
            // changed concurrently stays available for explicit recovery.
            if crate::engine::project::read_bounded_file(
                &path,
                crate::engine::level::MAX_LEVEL_JSON_BYTES,
            )
            .is_ok_and(|current| current == bytes)
            {
                std::fs::remove_file(&path).map_err(|cleanup| {
                    format!("{error}; imported source cleanup failed: {cleanup}")
                })?;
            }
            return Err(error);
        }
        self.select_level(id)?;
        Ok(id.into())
    }

    /// Updates explicit permitted destinations in the ordinary per-level history.
    /// # Errors
    /// Refuses unknown or duplicate IDs and breaking existing action references.
    pub fn set_transitions(&mut self, value: &str) -> Result<(), String> {
        let targets: Vec<_> = value
            .split(',')
            .map(str::trim)
            .filter(|candidate| !candidate.is_empty())
            .collect();
        for target in &targets {
            let _declared = self.manifest().level(Some(target))?;
        }
        let mut raw = self.raw().clone();
        drop(
            raw.as_object_mut()
                .ok_or("Invalid level")?
                .insert("transitions".into(), serde_json::json!(targets)),
        );
        self.accept_edit(raw)
    }

    /// Copies a validated landscape PNG into the project's thumbnail folder and links it.
    /// # Errors
    /// Rejects malformed pixels or existing conflicting files without overwriting them.
    pub fn attach_thumbnail(&mut self, source: &std::path::Path) -> Result<(), String> {
        self.save_all()?;
        let bytes = crate::engine::project::read_bounded_file(
            source,
            crate::engine::package::project::MAX_THUMBNAIL_BYTES,
        )?;
        crate::engine::package::project::validate_thumbnail(&bytes)?;
        let name = format!(
            "thumbnails/{}.png",
            crate::engine::package::hash::sha256_hex(&bytes)
        );
        let directory = self.root().join("thumbnails");
        if directory
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err("Thumbnail directory must not be a symlink".into());
        }
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let destination = self.root().join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(mut file) => file
                .write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if destination
                    .symlink_metadata()
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                    || crate::engine::project::read_bounded_file(
                        &destination,
                        crate::engine::package::project::MAX_THUMBNAIL_BYTES,
                    )? != bytes
                {
                    return Err("Thumbnail destination conflict".into());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
        self.update_manifest("thumbnail", &name)
    }
    /// Removes the selected map image and its entry-only package fallback.
    /// Existing PNG files remain available to other map references and recovery.
    /// # Errors
    /// Refuses conflicts or invalid project metadata before publication.
    pub fn remove_thumbnail(&mut self) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        manifest
            .levels
            .iter_mut()
            .find(|level| level.id == self.selected.level.id)
            .ok_or("Missing selected level")?
            .thumbnail = None;
        if self.selected.level.id == manifest.entry_level {
            manifest.thumbnail = None;
        }
        self.publish_manifest(&manifest)
    }

    /// Saves edits and atomically changes supported project metadata.
    /// # Errors
    /// Refuses conflicts or invalid metadata without changing the manifest.
    pub fn update_manifest(&mut self, field: &str, value: &str) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        match field {
            "name" => manifest.name = value.into(),
            "display_name" => {
                manifest
                    .levels
                    .iter_mut()
                    .find(|level| level.id == self.selected.level.id)
                    .ok_or("Missing selected level")?
                    .display_name = value.into();
            }
            "author" => manifest.author = value.into(),
            "description" => manifest.description = value.into(),
            "entry_level" => manifest.entry_level = value.into(),
            "thumbnail" => {
                let path = if value.is_empty() {
                    None
                } else {
                    Some(ProjectPath::new(value.into())?)
                };
                if let Some(thumbnail_path) = &path {
                    thumbnail_path.require_role("thumbnails", "png")?;
                    let bytes = crate::engine::project::read_bounded_file(
                        &self.project.resolve(thumbnail_path)?,
                        crate::engine::package::project::MAX_THUMBNAIL_BYTES,
                    )?;
                    crate::engine::package::project::validate_thumbnail(&bytes)?;
                }
                manifest
                    .levels
                    .iter_mut()
                    .find(|level| level.id == self.selected.level.id)
                    .ok_or("Missing selected level")?
                    .thumbnail = path;
            }
            _ => return Err("Unsupported project metadata field".into()),
        }
        self.publish_manifest(&manifest)
    }

    /// Saves edits and changes only the project's visible menu membership.
    /// # Errors
    /// Refuses external manifest changes or invalid metadata before publication.
    pub fn set_menu_entry_only(&mut self, entry_only: bool) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        manifest.menu_entry_only = entry_only;
        self.publish_manifest(&manifest)
    }

    /// Creates one new starter level with an explicit stable ID.
    /// # Errors
    /// Refuses existing files/IDs and retains the old project on failure.
    pub fn add_level(&mut self, id: &str) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        let source = ProjectPath::new(format!("levels/{id}.json"))?;
        manifest.levels.push(ProjectLevel {
            id: id.into(),
            source: source.clone(),
            display_name: String::new(),
            description: String::new(),
            thumbnail: None,
            scripts: Vec::new(),
        });
        manifest.validate()?;
        let levels_directory = self.root().join("levels");
        if levels_directory
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Levels directory must not be a symlink".into());
        }
        let path = self.root().join(source.as_str());
        let bytes = serde_json::to_vec_pretty(&persistence::starter_scene(id, id))
            .map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let result = file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())
            .and_then(|()| self.publish_manifest(&manifest));
        drop(file);
        if result.is_err() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        result?;
        self.select_level(id)
    }

    fn publish_manifest(
        &mut self,
        manifest: &crate::engine::project::ProjectManifest,
    ) -> Result<(), String> {
        manifest.validate()?;
        let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
        if persistence::read_manifest(&self.project)? != self.manifest_bytes {
            return Err("Manifest changed outside the editor; reopen first".into());
        }
        persistence::atomic_replace(&self.root().join(MANIFEST_NAME), &bytes)?;
        self.project = ProjectFolder::open(self.root())?;
        self.manifest_bytes = bytes;
        Ok(())
    }
}

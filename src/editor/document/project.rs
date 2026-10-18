//! Explicit project metadata operations, guarded by ordinary saves/conflict checks.
use super::{Document, persistence};
use crate::engine::project::{MANIFEST_NAME, ProjectFolder, ProjectLevel, ProjectPath};
use std::io::Write as _;

impl Document {
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
    /// Saves edits and atomically changes supported project metadata.
    /// # Errors
    /// Refuses conflicts or invalid metadata without changing the manifest.
    pub fn update_manifest(&mut self, field: &str, value: &str) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        match field {
            "name" => manifest.name = value.into(),
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

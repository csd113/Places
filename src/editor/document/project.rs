//! Explicit project metadata operations, guarded by ordinary saves/conflict checks.
use super::{Document, member_publication, persistence};
use crate::engine::project::{
    MANIFEST_NAME, ProjectFolder, ProjectLevel, ProjectManifest, ProjectPath,
};
use std::io::Write as _;

impl Document {
    /// Imports one standalone JSON map under its existing stable ID. Original
    /// bytes, unknown fields and per-placement intent remain unchanged. The
    /// source is declared only after complete validated, no-replace publication.
    ///
    /// # Errors
    /// Refuses duplicate IDs/files, invalid source, undeclared destinations and
    /// external script dependencies. A selection error can follow a successful commit.
    pub fn import_level(&mut self, source: &std::path::Path) -> Result<String, String> {
        let bytes = crate::engine::project::read_bounded_file(
            source,
            crate::engine::level::MAX_LEVEL_JSON_BYTES,
        )?;
        let level = member_publication::standalone_level(&bytes)?;
        let id = &level.id;
        let mut manifest = self.manifest().clone();
        let target = ProjectPath::new(format!("levels/{id}.json"))?;
        manifest.levels.push(ProjectLevel {
            id: id.to_owned(),
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
        self.save_all()?;
        self.publish_member(&manifest, &target, &bytes, Some(&level))?;
        self.select_published_level(id)?;
        Ok(level.id)
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
            "package_version" => manifest.package_version = value.to_string().try_into()?,
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
    /// Refuses existing files/IDs. A late failure preserves the unlisted source.
    pub fn add_level(&mut self, id: &str) -> Result<(), String> {
        self.save_all()?;
        let mut manifest = self.manifest().clone();
        let source = ProjectPath::new(format!("levels/{id}.json"))?;
        manifest.levels.push(ProjectLevel {
            id: id.to_owned(),
            source: source.clone(),
            display_name: String::new(),
            description: String::new(),
            thumbnail: None,
            scripts: Vec::new(),
        });
        let starter = persistence::starter_scene(id, id);
        drop(persistence::validate_raw(&starter, id)?);
        let bytes = serde_json::to_vec_pretty(&starter).map_err(|error| error.to_string())?;
        self.publish_member(&manifest, &source, &bytes, None)?;
        self.select_published_level(id)
    }

    fn publish_member(
        &mut self,
        manifest: &ProjectManifest,
        source: &ProjectPath,
        bytes: &[u8],
        imported: Option<&crate::engine::level::LevelDef>,
    ) -> Result<(), String> {
        let root = self.root().to_path_buf();
        let _writer = member_publication::lock(&root)?;
        member_publication::check_levels_directory(&root)?;
        let size = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
        let (candidate, manifest_bytes) = self.prepare_manifest(manifest, Some((source, size)))?;
        let path = root.join(source.as_str());
        match path.symlink_metadata() {
            Ok(_) => return Err(format!("Level source already exists: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect {}: {error}", path.display())),
        }
        let member = manifest.levels.last().ok_or("Missing new project member")?;
        member_publication::stage(&root, &self.manifest_bytes, &manifest_bytes, member, bytes)?;
        let publication = (|| {
            if let Some(level) = imported {
                super::game_modes::publish_copy(&root, level)?;
                persistence::atomic_create(&path, bytes)?;
            } else {
                persistence::write_unlisted_member(&path, bytes)?;
            }
            // Preserve the source when the current session cannot finish.
            self.commit_manifest(candidate, manifest_bytes).map_err(|error| {
                format!("{error}; level was not added to the project. Inspect the unlisted source at {} before retrying", path.display())
            })
        })();
        let cleanup = member_publication::finish(&root);
        match publication {
            Ok(()) => {
                // An already-committed record can be retired on the next open.
                self.recovery_notice = cleanup.err();
                Ok(())
            }
            Err(error) => match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!("{error}; {cleanup_error}")),
            },
        }
    }

    fn select_published_level(&mut self, id: &str) -> Result<(), String> {
        self.select_level(id).map_err(|error| {
            format!("Level '{id}' was published, but could not be selected: {error}")
        })
    }

    fn publish_manifest(&mut self, manifest: &ProjectManifest) -> Result<(), String> {
        let _writer = member_publication::lock(self.root())?;
        let (candidate, bytes) = self.prepare_manifest(manifest, None)?;
        self.commit_manifest(candidate, bytes)
    }

    fn prepare_manifest(
        &self,
        manifest: &ProjectManifest,
        new_level: Option<(&ProjectPath, u64)>,
    ) -> Result<(ProjectFolder, Vec<u8>), String> {
        self.check_manifest()?;
        let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
        let candidate = self.project.with_manifest(&bytes, new_level)?;
        Ok((candidate, bytes))
    }

    pub(super) fn check_manifest(&self) -> Result<(), String> {
        if !self
            .root()
            .join(MANIFEST_NAME)
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Project manifest must be a regular file".into());
        }
        if persistence::read_manifest(&self.project)? != self.manifest_bytes {
            return Err("Manifest changed outside the editor; reopen first".into());
        }
        Ok(())
    }

    pub(super) fn commit_manifest(
        &mut self,
        candidate: ProjectFolder,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        candidate.validate_files(None)?;
        self.check_manifest()?;
        persistence::atomic_replace(&self.root().join(MANIFEST_NAME), &bytes)?;
        self.project = candidate;
        self.manifest_bytes = bytes;
        Ok(())
    }
}

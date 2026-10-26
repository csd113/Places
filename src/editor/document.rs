//! Authoring documents keep raw JSON authoritative. Typed engine data is a
//! validated view, never the serialization source: future/unsupported fields
//! therefore survive ordinary edits and saves.

pub(crate) mod behavior;
pub(crate) mod components;
mod dragging;
pub(crate) mod flicker;
mod fog;
mod gameplay;
mod geometry;
mod history;
mod local_floor;
pub(crate) mod room_resize;
pub(super) use objects::array_mut as record_array_mut;
pub(crate) mod intent;
mod mounting;
mod objects;
mod persistence;
mod project;
mod scripts;
mod source;
mod templates;

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::engine::assets::AssetCatalog;
use crate::engine::level::LevelDef;
use crate::engine::project::{
    BuiltinAssetKind, BuiltinAssetRef, ProjectFolder, ProjectManifest, validate_level_assets,
};

pub use gameplay::GameplaySetting;
pub use objects::{ObjectId, ObjectInfo, ObjectKind, Transform};

use history::History;
use persistence::LevelDocument;

/// One open project, retaining an independent document/history for each visited
/// level. Merely switching levels never writes or discards edits.
pub struct Document {
    project: ProjectFolder,
    scripts: BTreeMap<String, scripts::ScriptDocument>,
    manifest_bytes: Vec<u8>,
    selected: LevelDocument,
    other_levels: BTreeMap<String, LevelDocument>,
}

const MAX_DOCUMENT_HEAP_CHARGE: usize = 256 * 1024 * 1024;

impl Document {
    fn heap_charge(&self) -> usize {
        std::iter::once(&self.selected)
            .chain(self.other_levels.values())
            .map(LevelDocument::heap_charge)
            .chain(
                self.scripts
                    .values()
                    .map(scripts::ScriptDocument::heap_charge),
            )
            .fold(0_usize, usize::saturating_add)
    }

    fn admit_document_bytes(&mut self, additional: usize) -> Result<(), String> {
        while self.heap_charge().saturating_add(additional) > MAX_DOCUMENT_HEAP_CHARGE {
            let clean = self
                .other_levels
                .iter()
                .find(|(_, document)| !document.is_dirty() && document.history.usage().0 == 0)
                .map(|(id, _)| id.clone());
            let Some(id) = clean else {
                return Err("Editor document memory budget reached (256 MiB charged heap). Unsaved maps and undo/redo are preserved; save and reopen the project to release history before opening or editing more content.".into());
            };
            drop(self.other_levels.remove(&id));
        }
        Ok(())
    }
    /// Opens the manifest or project folder using the shared confined API.
    ///
    /// # Errors
    /// Rejects unsupported schemas, malformed scenes and unsafe project paths.
    pub fn open(path: &Path) -> Result<Self, String> {
        let project = ProjectFolder::open(path)?;
        let manifest_bytes = persistence::read_manifest(&project)?;
        let selected = LevelDocument::open(&project, None)?;
        Ok(Self {
            project,
            scripts: BTreeMap::new(),
            manifest_bytes,
            selected,
            other_levels: BTreeMap::new(),
        })
    }

    /// Creates a new folder with one small floor/ceiling and a built-in light.
    /// The parent must exist; an existing destination is never overwritten.
    ///
    /// # Errors
    /// Rejects invalid IDs, existing destinations and filesystem failures.
    pub fn create(path: &Path, id: &str, name: &str) -> Result<Self, String> {
        persistence::create_project(path, id, name)?;
        Self::open(path)
    }

    #[must_use]
    pub const fn project(&self) -> &ProjectFolder {
        &self.project
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        self.project.root()
    }

    #[must_use]
    pub const fn manifest(&self) -> &ProjectManifest {
        self.project.manifest()
    }

    #[must_use]
    pub fn level_id(&self) -> &str {
        &self.selected.level.id
    }

    pub fn level_ids(&self) -> impl Iterator<Item = &str> {
        self.project
            .manifest()
            .levels
            .iter()
            .map(|level| level.id.as_str())
    }

    /// Selects a declared level, retaining every visited level's unsaved edits.
    ///
    /// # Errors
    /// Leaves the current level intact if the new source cannot be read.
    pub fn select_level(&mut self, id: &str) -> Result<(), String> {
        if id == self.level_id() {
            return Ok(());
        }
        let next = if let Some(cached) = self.other_levels.remove(id) {
            cached
        } else {
            let source = &self.project.manifest().level(Some(id))?.source;
            let bytes = std::fs::metadata(self.project.resolve(source)?)
                .map_err(|error| error.to_string())?
                .len();
            self.admit_document_bytes(
                usize::try_from(bytes)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(8),
            )?;
            let opened = LevelDocument::open(&self.project, Some(id))?;
            self.admit_document_bytes(opened.heap_charge())?;
            opened
        };
        let previous = std::mem::replace(&mut self.selected, next);
        drop(
            self.other_levels
                .insert(previous.level.id.clone(), previous),
        );
        Ok(())
    }

    pub(super) fn resource_snapshot(&self) -> serde_json::Value {
        let (documents, revisions, history_bytes) = std::iter::once(&self.selected)
            .chain(self.other_levels.values())
            .fold(
                (0_usize, 0_usize, 0_usize),
                |(documents, revisions, bytes), d| {
                    let (count, size) = d.history.usage();
                    (
                        documents.saturating_add(1),
                        revisions.saturating_add(count),
                        bytes.saturating_add(size),
                    )
                },
            );
        serde_json::json!({"documents":documents,"history_revisions":revisions,"history_serialized_bytes":history_bytes,"document_heap_charge":self.heap_charge(),"document_heap_budget":MAX_DOCUMENT_HEAP_CHARGE,"history_policy":"64 revisions / 32 MiB per stack; 256 MiB aggregate document admission; unsaved work and histories pinned"})
    }

    #[must_use]
    pub const fn level(&self) -> &LevelDef {
        &self.selected.level
    }

    #[must_use]
    pub const fn raw(&self) -> &Value {
        &self.selected.raw
    }

    #[must_use]
    pub fn objects(&self) -> &[ObjectInfo] {
        &self.selected.objects
    }

    /// Changes this map's crosshair default through ordinary undo/save history.
    /// # Errors
    /// Rejects invalid resulting level data.
    pub fn set_crosshair(&mut self, enabled: bool) -> Result<(), String> {
        let mut edited = self.selected.raw.clone();
        let object = edited
            .as_object_mut()
            .ok_or("Level source must be an object")?;
        drop(object.insert("crosshair".into(), Value::Bool(enabled)));
        self.accept_edit(edited)
    }

    /// Returns authored coordinates. Prop Y remains relative to its floor;
    /// prop size is its uniform scale repeated on all three axes.
    ///
    /// # Errors
    /// Rejects stale selections after deletion or undo.
    pub fn transform(&self, id: ObjectId) -> Result<Transform, String> {
        objects::transform(self.level(), id)
    }

    /// Edits only the supported scalar fields and creates one undo step.
    /// Axis-aligned geometry accepts quarter-turn footprint rotations only.
    ///
    /// # Errors
    /// Rejects invalid numbers, unsupported rotations and invalid scene edits.
    pub fn set_transform(&mut self, id: ObjectId, transform: Transform) -> Result<(), String> {
        let mut edited = self.selected.raw.clone();
        objects::set_transform(&mut edited, self.level(), id, transform)?;
        self.accept_edit(edited)
    }

    /// Validates a complete proposed transform without changing source or history.
    /// The preview uses the same scene admission as an ordinary edit.
    ///
    /// # Errors
    /// Rejects unsupported transforms and invalid resulting scene geometry.
    pub fn preview_transform(
        &self,
        id: ObjectId,
        transform: Transform,
    ) -> Result<LevelDef, String> {
        let mut edited = self.selected.raw.clone();
        objects::set_transform(&mut edited, self.level(), id, transform)?;
        persistence::validate_raw(&edited, self.level_id()).map(|(level, _bytes)| level)
    }

    /// Adds one room, wall or built-in prop at authored coordinates.
    /// `asset` optionally selects the floor/wall material or prop model.
    ///
    /// # Errors
    /// Rejects invalid coordinates or an invalid resulting scene.
    pub fn add(
        &mut self,
        kind: ObjectKind,
        position: [f32; 3],
        asset: Option<&str>,
    ) -> Result<ObjectId, String> {
        let mut edited = self.selected.raw.clone();
        let id = objects::add(&mut edited, self.level(), kind, position, asset)?;
        self.accept_edit(edited)?;
        Ok(id)
    }

    /// Adds a validated built-in asset and captures a prop's catalog collision
    /// size/solid setting in the same undo step. Runtime physics stays opt-in.
    ///
    /// # Errors
    /// Rejects incompatible assets, missing prop dimensions or invalid edits.
    pub fn add_with_catalog(
        &mut self,
        kind: ObjectKind,
        position: [f32; 3],
        asset: Option<&str>,
        catalog: &AssetCatalog,
    ) -> Result<ObjectId, String> {
        let model = asset.unwrap_or(if kind == ObjectKind::Entity {
            "mannequin"
        } else {
            "core:chair"
        });
        if matches!(kind, ObjectKind::Prop | ObjectKind::Entity)
            || (asset.is_some()
                && matches!(
                    kind,
                    ObjectKind::Room
                        | ObjectKind::Wall
                        | ObjectKind::Ramp
                        | ObjectKind::FloorRegion
                ))
        {
            BuiltinAssetRef {
                kind: if matches!(kind, ObjectKind::Prop | ObjectKind::Entity) {
                    BuiltinAssetKind::Model
                } else {
                    BuiltinAssetKind::Material
                },
                id: model.into(),
            }
            .validate(catalog, None)?;
        }
        let mut edited = self.selected.raw.clone();
        let id = objects::add(&mut edited, self.level(), kind, position, asset)?;
        if matches!(kind, ObjectKind::Prop | ObjectKind::Entity) {
            let entry = catalog.placeable(model).ok_or("Choose a built-in prop")?;
            let size = entry
                .size
                .ok_or("The selected prop has no catalog dimensions")?;
            objects::set_prop_collision(&mut edited, id, size, entry.solid)?;
        }
        self.accept_edit(edited)?;
        Ok(id)
    }

    /// Deletes the selected object; semantic references must remain valid.
    ///
    /// # Errors
    /// Rejects stale selections or deletion that breaks scene references.
    pub fn delete(&mut self, id: ObjectId) -> Result<(), String> {
        let mut edited = self.selected.raw.clone();
        objects::delete(&mut edited, self.level(), id)?;
        self.accept_edit(edited)
    }

    /// Copies a basic object with a small offset. Passive props receive a new
    /// explicit identity; active gameplay instances require later tooling.
    ///
    /// # Errors
    /// Rejects stale selections, active instances or invalid duplicates.
    pub fn duplicate(&mut self, id: ObjectId) -> Result<ObjectId, String> {
        let mut edited = self.selected.raw.clone();
        let copy = objects::duplicate(&mut edited, self.level(), id)?;
        self.accept_edit(edited)?;
        Ok(copy)
    }

    /// Changes a room's floor material or a wall's default face material.
    ///
    /// # Errors
    /// Rejects props, stale selections or invalid material identities.
    pub fn set_material(&mut self, id: ObjectId, material: &str) -> Result<(), String> {
        if !matches!(
            id,
            ObjectId::Room(_)
                | ObjectId::Wall(_)
                | ObjectId::Ramp(_)
                | ObjectId::FloorRegion(_)
                | ObjectId::Door(_)
        ) {
            return Err("Select a room or wall to change its material".into());
        }
        self.set_object_string(id, "material", material)
    }

    pub(super) fn set_door_style(&mut self, id: ObjectId, style: &str) -> Result<(), String> {
        if !matches!(id, ObjectId::Door(_)) {
            return Err("Select a door first".into());
        }
        self.set_object_string(id, "kind", style)
    }
    pub(super) fn set_ceiling_material(
        &mut self,
        id: ObjectId,
        material: &str,
    ) -> Result<(), String> {
        if !matches!(id, ObjectId::Room(_)) {
            return Err("Select a room ceiling first".into());
        }
        self.set_object_string(id, "ceiling_material", material)
    }

    /// Changes the model of a passive prop without replacing its other data.
    ///
    /// # Errors
    /// Rejects non-props, active instances or invalid scene changes.
    pub fn set_model(&mut self, id: ObjectId, model: &str) -> Result<(), String> {
        let ObjectId::Prop(index) = id else {
            return Err("Select a prop to change its model".into());
        };
        objects::require_passive_prop(self.level(), index)?;
        let mut edited = self.selected.raw.clone();
        objects::pin_prop_ids(&mut edited, self.level())?;
        objects::set_string(&mut edited, id, "model", model)?;
        self.accept_edit(edited)
    }

    fn set_object_string(&mut self, id: ObjectId, key: &str, value: &str) -> Result<(), String> {
        let mut edited = self.selected.raw.clone();
        objects::set_string(&mut edited, id, key, value)?;
        self.accept_edit(edited)
    }

    fn accept_edit(&mut self, edited: Value) -> Result<(), String> {
        if edited == self.selected.raw {
            return Ok(());
        }
        self.admit_document_bytes(
            history::value_heap_charge(&edited)
                .saturating_add(self.selected.byte_count.saturating_mul(2)),
        )?;
        let (level, byte_count) = persistence::validate_raw(&edited, self.level_id())?;
        let old = std::mem::replace(&mut self.selected.raw, edited);
        self.selected.history.edit(old, self.selected.byte_count);
        self.selected.byte_count = byte_count;
        self.selected.level = level;
        self.selected.update_view();
        Ok(())
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.selected.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.selected.history.can_redo()
    }

    /// Restores one validated revision; no filesystem access is performed.
    pub fn undo(&mut self) -> bool {
        self.restore_history(false)
    }

    /// Reapplies one undone revision; no filesystem access is performed.
    pub fn redo(&mut self) -> bool {
        self.restore_history(true)
    }

    fn restore_history(&mut self, redo: bool) -> bool {
        let Some(candidate) = self.selected.history.candidate(redo) else {
            return false;
        };
        // Validate before moving either history cursor so even an unexpected
        // validation failure cannot consume an undo/redo revision.
        let Ok((level, _)) = persistence::validate_raw(candidate, self.level_id()) else {
            return false;
        };
        let Some((raw, byte_count)) =
            self.selected
                .history
                .restore(redo, &self.selected.raw, self.selected.byte_count)
        else {
            return false;
        };
        self.selected.raw = raw;
        self.selected.level = level;
        self.selected.update_view();
        self.selected.byte_count = byte_count;
        true
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.selected.is_dirty()
            || self.other_levels.values().any(LevelDocument::is_dirty)
            || self.scripts.values().any(scripts::ScriptDocument::is_dirty)
    }

    #[must_use]
    pub const fn selected_is_dirty(&self) -> bool {
        self.selected.is_dirty()
    }

    /// Runs the shared built-in asset checks before save/build. The application
    /// supplies its installed catalog rather than creating another resolver.
    ///
    /// # Errors
    /// Names the first absent or incompatible asset in a visited level.
    pub fn validate_assets(&self, catalog: &AssetCatalog) -> Result<(), String> {
        for document in std::iter::once(&self.selected).chain(self.other_levels.values()) {
            validate_level_assets(&document.level, catalog)?;
        }
        Ok(())
    }

    /// Atomically saves the selected source, preserving the manifest and all
    /// unknown JSON fields. Source conflicts leave disk and history untouched.
    ///
    /// # Errors
    /// Rejects validation failures, external changes, unsafe paths and I/O errors.
    pub fn save(&mut self) -> Result<(), String> {
        self.preflight()?;
        self.selected.save(&self.project)?;
        self.save_scripts()
    }

    /// Saves every dirty visited level. All sources are preflighted before any
    /// publication; each source is atomic, while the group is not a transaction.
    /// Successfully saved levels retain their savepoints if a later I/O fails.
    ///
    /// # Errors
    /// Rejects validation failures, external changes, unsafe paths and I/O errors.
    pub fn save_all(&mut self) -> Result<(), String> {
        self.preflight()?;
        self.selected.save(&self.project)?;
        for document in self.other_levels.values_mut() {
            document.save(&self.project)?;
        }
        self.save_scripts()
    }

    fn preflight(&self) -> Result<(), String> {
        self.preflight_scripts()?;
        self.project.manifest().validate()?;
        if persistence::read_manifest(&self.project)? != self.manifest_bytes {
            return Err("Project manifest changed outside the editor; reopen before saving".into());
        }
        for document in std::iter::once(&self.selected).chain(self.other_levels.values()) {
            document.preflight(&self.project)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/document_memory.rs"]
mod memory_tests;

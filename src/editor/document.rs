//! Authoring documents keep raw JSON authoritative. Typed engine data is a
//! validated view, never the serialization source: future/unsupported fields
//! therefore survive ordinary edits and saves.

pub(crate) mod behavior;
pub(crate) mod cinematics;
pub(crate) mod circuits;
pub(crate) mod components;
mod dragging;
pub(crate) mod flicker;
mod fog;
mod game_modes;
mod gameplay;
mod geometry;
mod history;
mod local_floor;
mod member_publication;
pub(crate) mod room_resize;
pub(crate) mod wall_edit;
pub(super) use objects::array_mut as record_array_mut;
#[cfg(test)]
#[path = "tests/construction_generators.rs"]
mod construction_generator_tests;
pub(crate) mod intent;
pub(crate) mod mounting;
pub(super) mod objects;
pub(super) mod persistence;
mod project;
mod scripts;
pub(crate) mod signs;
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
    recovery_notice: Option<String>,
    selected: LevelDocument,
    other_levels: BTreeMap<String, LevelDocument>,
    heap_budget: usize,
    scene_stamp: super::agent_api::SceneStamp,
}

const MAX_DOCUMENT_HEAP_CHARGE: usize = 256 * 1024 * 1024;

fn document_memory_error() -> String {
    "Editor document memory budget reached (256 MiB charged heap). Unsaved maps and undo/redo are preserved; save and reopen the project to release history before opening or editing more content.".into()
}

impl Document {
    pub(super) fn add_escalator(
        &mut self,
        fit: crate::engine::escalator::Fit,
    ) -> Result<ObjectId, String> {
        let mut raw = self.raw().clone();
        let id = Self::stage_escalator(&mut raw, self.level(), fit)?;
        self.accept_edit(raw)?;
        Ok(id)
    }

    pub(super) fn stage_escalator(
        raw: &mut Value,
        level: &LevelDef,
        fit: crate::engine::escalator::Fit,
    ) -> Result<ObjectId, String> {
        use crate::engine::escalator::MODEL;
        let [x, world_y, z] = fit.origin.to_array();
        let floor_y = crate::engine::level::LevelSurfaces::new(level)
            .floor_y_at(x, z)
            .ok_or("Escalator base must be inside the room")?;
        let id = objects::add(
            raw,
            level,
            ObjectKind::Prop,
            [x, world_y - floor_y, z],
            Some(MODEL),
        )?;
        objects::apply_mechanism_preset(raw, id, MODEL)?;
        let prop = objects::object_mut(raw, id)?;
        objects::set_number(prop, "rotation_degrees", fit.yaw);
        drop(prop.insert(
            "escalator".into(),
            serde_json::json!({"rise":fit.dimensions.rise,"placement":fit.placement}),
        ));
        Ok(id)
    }

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
        while self.heap_charge().saturating_add(additional) > self.heap_budget {
            let clean = self
                .other_levels
                .iter()
                .find(|(_, document)| !document.is_dirty() && document.history.usage().0 == 0)
                .map(|(id, _)| id.clone());
            let Some(id) = clean else {
                return Err(document_memory_error());
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
        Self::open_with_budget(path, MAX_DOCUMENT_HEAP_CHARGE)
    }

    pub(super) fn open_with_recovery(path: &Path) -> Result<Self, String> {
        let mut document = Self::open(path)?;
        document.recovery_notice = member_publication::recover(&mut document);
        Ok(document)
    }

    fn open_with_budget(path: &Path, heap_budget: usize) -> Result<Self, String> {
        let (project, manifest_bytes) = ProjectFolder::open_snapshot(path)?;
        let selected = LevelDocument::open(&project, None, |charge| {
            if charge > heap_budget {
                Err(document_memory_error())
            } else {
                Ok(())
            }
        })?;
        Ok(Self {
            project,
            scripts: BTreeMap::new(),
            manifest_bytes,
            recovery_notice: None,
            selected,
            other_levels: BTreeMap::new(),
            heap_budget,
            scene_stamp: super::agent_api::SceneStamp::new(),
        })
    }

    pub(super) fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
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
            let project = self.project.clone();
            LevelDocument::open(&project, Some(id), |charge| {
                self.admit_document_bytes(charge)
            })?
        };
        let previous = std::mem::replace(&mut self.selected, next);
        self.scene_stamp = super::agent_api::SceneStamp::new();
        drop(
            self.other_levels
                .insert(previous.level.id.clone(), previous),
        );
        Ok(())
    }

    pub(super) const fn scene_stamp(&self) -> &super::agent_api::SceneStamp {
        &self.scene_stamp
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
        serde_json::json!({"documents":documents,"history_revisions":revisions,"history_serialized_bytes":history_bytes,"document_heap_charge":self.heap_charge(),"document_heap_budget":self.heap_budget,"history_policy":"64 revisions / 32 MiB per stack; 256 MiB aggregate document admission; unsaved work and histories pinned"})
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
    pub fn set_saving(&mut self, enabled: bool) -> Result<(), String> {
        let mut edited = self.raw().clone();
        drop(
            edited
                .as_object_mut()
                .ok_or("Invalid level")?
                .insert("saving".into(), Value::Bool(enabled)),
        );
        self.accept_edit(edited)
    }

    /// Changes this map's crosshair default through ordinary history.
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
        let (edited, _) = self.transform_candidate(id, transform)?;
        self.accept_edit(edited)
    }

    /// Validates the complete attachment edit without changing source/history.
    /// # Errors
    /// Rejects unsupported transforms and invalid resulting scene geometry.
    pub fn preview_transform(
        &self,
        id: ObjectId,
        transform: Transform,
    ) -> Result<LevelDef, String> {
        self.transform_candidate(id, transform)
            .map(|(_, level)| level)
    }

    pub(in crate::editor) fn transform_candidate(
        &self,
        id: ObjectId,
        transform: Transform,
    ) -> Result<(Value, LevelDef), String> {
        let mut edited = if let ObjectId::Wall(index) = id {
            wall_edit::edit_raw(self.raw(), self.level(), index, transform)?
        } else {
            self.selected.raw.clone()
        };
        if !matches!(id, ObjectId::Wall(_)) {
            objects::set_transform(&mut edited, self.level(), id, transform)?;
        }
        self.validated_preview(edited)
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

    /// Adds a validated built-in asset and captures catalog collision dimensions,
    /// solidity and any opted-in placement physics in the same undo step.
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
        let (edited, id) = self.catalog_add_candidate(kind, position, asset, catalog)?;
        self.accept_edit(edited)?;
        Ok(id)
    }

    pub(super) fn add_snapped_prop(
        &mut self,
        position: [f32; 3],
        asset: &str,
        catalog: &AssetCatalog,
        step: f32,
    ) -> Result<(ObjectId, Option<String>), String> {
        let (edited, id) =
            self.catalog_add_candidate(ObjectKind::Prop, position, Some(asset), catalog)?;
        let level = self.validate_candidate(&edited)?;
        let mounted = mounting::candidate_with_step(&level, &edited, id, Some(step))
            .and_then(|candidate| self.accept_edit(candidate));
        let mount_error = if let Err(error) = mounted {
            self.accept_edit(edited)?;
            Some(error)
        } else {
            None
        };
        Ok((id, mount_error))
    }

    fn catalog_add_candidate(
        &self,
        kind: ObjectKind,
        position: [f32; 3],
        asset: Option<&str>,
        catalog: &AssetCatalog,
    ) -> Result<(Value, ObjectId), String> {
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
            if let Some(profile) = &entry.editor_placement_physics {
                let physics = serde_json::to_value(profile).map_err(|error| error.to_string())?;
                drop(objects::object_mut(&mut edited, id)?.insert("physics".into(), physics));
            }
            objects::apply_mechanism_preset(&mut edited, id, model)?;
        }
        Ok((edited, id))
    }

    /// Deletes the selected object; semantic references must remain valid.
    ///
    /// # Errors
    /// Rejects stale selections or deletion that breaks scene references.
    pub fn delete(&mut self, id: ObjectId) -> Result<(), String> {
        self.delete_with_circuits(id)
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
            ObjectId::Column(_)
                | ObjectId::Pillar(_)
                | ObjectId::ArcWall(_)
                | ObjectId::Room(_)
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

    pub(crate) fn check_locked_edit(&self, edited: &Value) -> Result<(), String> {
        Self::check_locked_scene(self.raw(), self.objects(), edited)
    }

    pub(super) fn check_locked_scene(
        before: &Value,
        objects: &[ObjectInfo],
        edited: &Value,
    ) -> Result<(), String> {
        for object in objects {
            let (key, i) = object.id.collection();
            let Some(old) = before
                .get(key)
                .and_then(|a| a.get(i))
                .filter(|r| r.get("editor_locked").and_then(Value::as_bool) == Some(true))
            else {
                continue;
            };
            let retained = edited
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|a| a.contains(old));
            if retained {
                continue;
            }
            let mut unlocked = old.clone();
            if let Some(r) = unlocked.as_object_mut() {
                drop(r.insert("editor_locked".into(), Value::Bool(false)));
            }
            if edited.get(key).and_then(|a| a.get(i)) != Some(&unlocked) {
                return Err(format!(
                    "{} is locked. Unlock it in Layers before editing it or its attachments.",
                    object.label
                ));
            }
        }
        Ok(())
    }
    fn edit_charge(&self, edited: &Value) -> usize {
        history::value_heap_charge(edited)
            .saturating_add(self.selected.byte_count.saturating_mul(2))
    }

    #[cfg(windows)]
    pub(super) const fn agent_scene_size(&self) -> usize {
        self.selected.byte_count
    }

    // Like native admission, only clean inactive levels without history can
    // make room. API preview and rejected batches never evict any levels.
    #[cfg(windows)]
    fn agent_evictions(&self, edited: &Value) -> Result<Vec<String>, String> {
        let mut retained = self.heap_charge();
        let additional = self.edit_charge(edited);
        let mut evictions = Vec::new();
        for (id, cached) in &self.other_levels {
            if retained.saturating_add(additional) <= self.heap_budget {
                break;
            }
            if !cached.is_dirty() && cached.history.usage().0 == 0 {
                retained = retained.saturating_sub(cached.heap_charge());
                evictions.push(id.clone());
            }
        }
        if retained.saturating_add(additional) > self.heap_budget {
            return Err(document_memory_error());
        }
        Ok(evictions)
    }

    #[cfg(windows)]
    pub(super) fn prepare_agent_stage(
        &self,
        before: &Value,
        objects: &[ObjectInfo],
        edited: &Value,
    ) -> Result<(LevelDef, usize), String> {
        Self::check_locked_scene(before, objects, edited)?;
        drop(self.agent_evictions(edited)?);
        persistence::validate_raw(edited, self.level_id())
    }

    #[cfg(windows)]
    pub(super) fn install_agent_batch(
        &mut self,
        expected: &super::agent_api::SceneStamp,
        edited: Value,
        level: LevelDef,
        byte_count: usize,
        still_admitted: impl FnOnce() -> bool,
    ) -> Result<(), String> {
        if !self.scene_stamp.matches(expected) {
            return Err("Scene changed; refresh and preview again".into());
        }
        self.check_locked_edit(&edited)?;
        let evictions = self.agent_evictions(&edited)?;
        if edited == self.selected.raw {
            return Err("Batch would not change the scene".into());
        }
        if !still_admitted() {
            return Err("The visible editor grant was lost before commit".into());
        }
        for id in evictions {
            drop(self.other_levels.remove(&id));
        }
        self.install_edit(edited, level, byte_count);
        Ok(())
    }

    fn prepare_edit(&mut self, edited: &Value) -> Result<(LevelDef, usize), String> {
        let mut timing = crate::engine::perf::loading::StageTimer::new("editor-edit");
        self.check_locked_edit(edited)?;
        timing.step("equality-locks");
        self.admit_document_bytes(self.edit_charge(edited))?;
        timing.step("admission");
        let validated = persistence::validate_raw(edited, self.level_id())?;
        timing.step("validation");
        Ok(validated)
    }

    // Every fallible check precedes installation (and script-file publication).
    fn install_edit(&mut self, edited: Value, level: LevelDef, byte_count: usize) {
        let mut timing = crate::engine::perf::loading::StageTimer::new("editor-edit-install");
        let old = std::mem::replace(&mut self.selected.raw, edited);
        self.selected.history.edit(old, self.selected.byte_count);
        self.selected.byte_count = byte_count;
        self.selected.level = level;
        timing.step("history-install");
        self.selected.update_view();
        self.scene_stamp.advance();
        timing.step("dirty-enumerate");
    }

    pub(super) fn accept_edit(&mut self, edited: Value) -> Result<(), String> {
        if edited != self.selected.raw {
            let (level, byte_count) = self.prepare_edit(&edited)?;
            self.install_edit(edited, level, byte_count);
        }
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
        let mut timing = crate::engine::perf::loading::StageTimer::new("editor-history");
        let Some(candidate) = self.selected.history.candidate(redo) else {
            return false;
        };
        // Validate before moving either history cursor so even an unexpected
        // validation failure cannot consume an undo/redo revision.
        let Ok((level, _)) = persistence::validate_raw(candidate, self.level_id()) else {
            return false;
        };
        timing.step("validation");
        let Some(byte_count) =
            self.selected
                .history
                .restore(redo, &mut self.selected.raw, self.selected.byte_count)
        else {
            return false;
        };
        timing.step("restore");
        self.selected.level = level;
        self.selected.update_view();
        self.selected.byte_count = byte_count;
        self.scene_stamp.advance();
        timing.step("dirty-enumerate-install");
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

#[cfg(test)]
#[path = "tests/script_publication.rs"]
mod script_publication_tests;

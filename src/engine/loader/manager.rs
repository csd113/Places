//! Installed-level discovery, material resolution and transactional import.

use std::cell::RefCell;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    LevelEntry, LevelSourceType, LoadedLevel, PropCatalog, ResolvedFixtureSheet,
    resolve_fixture_sheets,
};
use crate::engine::level::LevelDef;
use crate::engine::materials::{MaterialTable, PackMaterials, TextureCache, resolve_materials};
use crate::engine::package::archive::{cleanup_failed_write, create_temporary};
use crate::engine::package::world::OpenedPackage;

/// Import transport budget: the project package budget plus generous ZIP
/// metadata and incompressible-deflate overhead. This bounds staging disk use
/// even when the source grows while copying or carries unused archive padding.
const MAX_IMPORT_BYTES: u64 = crate::engine::package::archive::MAX_PROJECT_TRANSPORT_BYTES;

/// Menu precedence of a source: bundled packages before installed ones.
const fn source_rank(source_type: LevelSourceType) -> u8 {
    match source_type {
        LevelSourceType::Bundled => 0,
        LevelSourceType::Installed => 1,
    }
}

type MenuImageCache = std::collections::BTreeMap<(PathBuf, String), Option<Arc<super::RawImage>>>;

/// Unified level loader and package manager.
pub struct LevelManager {
    assets_dir: PathBuf,
    levels_dir: PathBuf,
    import_dir: PathBuf,
    entries: Vec<LevelEntry>,
    menu_entries: Vec<LevelEntry>,
    menu_images: RefCell<MenuImageCache>,
    prop_catalog: PropCatalog,
    /// Decoded texture images shared across level loads (one decode per
    /// logical texture per session).
    texture_cache: RefCell<TextureCache>,
}

impl LevelManager {
    /// # Errors
    /// Returns the shared resource/storage configuration error.
    pub fn new() -> Result<Self, String> {
        let paths = crate::engine::paths::user()?;
        Ok(Self::with_paths(
            crate::engine::paths::assets()?.join("levels"),
            paths.levels(),
            paths.imports(),
        ))
    }

    #[must_use]
    pub fn with_paths(assets_dir: PathBuf, levels_dir: PathBuf, import_dir: PathBuf) -> Self {
        let mut manager = Self {
            assets_dir,
            levels_dir,
            import_dir,
            entries: Vec::new(),
            menu_entries: Vec::new(),
            menu_images: RefCell::new(std::collections::BTreeMap::new()),
            prop_catalog: PropCatalog::load_default(),
            texture_cache: RefCell::new(TextureCache::new()),
        };
        manager.refresh();
        manager
    }

    /// Every installed member, including internal maps hidden from Explore.
    #[must_use]
    pub fn entries(&self) -> &[LevelEntry] {
        &self.entries
    }

    /// Playable menu rows; internal project members remain in [`Self::entries`].
    #[must_use]
    pub fn menu_entries(&self) -> &[LevelEntry] {
        &self.menu_entries
    }

    #[must_use]
    pub fn get_menu_entry(&self, index: usize) -> Option<&LevelEntry> {
        self.menu_entries.get(index)
    }

    /// Bounded visible-row image cache, independent of prepared level loading.
    #[must_use]
    pub fn menu_thumbnail(&self, index: usize) -> Option<Arc<super::RawImage>> {
        let entry = self.get_menu_entry(index)?;
        let key = (entry.path.clone(), entry.id.clone());
        let mut cache = self.menu_images.borrow_mut();
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
        let image = match crate::engine::package::project::menu_thumbnail(&entry.path, &entry.id) {
            Ok(image) => image.map(Arc::new),
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("thumbnail:{}:{}", entry.path.display(), entry.id),
                    format!("[menu] {} thumbnail: {error}; using fallback", entry.id),
                );
                None
            }
        };
        if cache.len() >= 8 {
            cache.clear();
        }
        drop(cache.insert(key, image.clone()));
        image
    }

    /// Metadata only; reads no prepared world or texture records.
    pub fn menu_description(&self, index: usize) -> String {
        let Some(entry) = self.get_menu_entry(index) else {
            return String::new();
        };
        let description = crate::engine::package::project::inspect(&entry.path)
            .ok()
            .flatten()
            .and_then(|package| {
                package.project.level(Some(&entry.id)).ok().map(|level| {
                    if level.description.trim().is_empty()
                        && entry.id == package.project.entry_level
                    {
                        package.project.description.clone()
                    } else {
                        level.description.clone()
                    }
                })
            })
            .unwrap_or_default();
        if description.trim().is_empty() {
            format!(
                "{}\n\nA place to explore at your own pace.\n\nBy {}",
                entry.name,
                if entry.author.is_empty() {
                    "an independent creator"
                } else {
                    &entry.author
                }
            )
        } else {
            format!("{}\n\n{}\n\nBy {}", entry.name, description, entry.author)
        }
    }

    #[cfg(feature = "visual-diagnostics")]
    pub(crate) fn diagnostic_cached_images(&self) -> serde_json::Value {
        self.texture_cache.borrow().diagnostic_retention()
    }

    /// Drops optional decoded map-image ownership when returning to the menu.
    pub(crate) fn release_cached_images(&self) {
        *self.texture_cache.borrow_mut() = TextureCache::new();
    }

    /// Refreshes authoring metadata before a new explicit disk load.
    pub(crate) fn refresh_catalog(&mut self) {
        self.prop_catalog = PropCatalog::load_default();
    }

    /// Prop catalog used to resolve placed props.
    #[must_use]
    pub const fn prop_catalog(&self) -> &PropCatalog {
        &self.prop_catalog
    }

    #[must_use]
    pub fn get_entry(&self, idx: usize) -> Option<&LevelEntry> {
        self.entries.get(idx)
    }

    /// Re-scans directories for installed compiled packages.
    ///
    /// Only `.placesmap` files are playable. Authoring sources (`*.json`,
    /// `*.zip`) are **silently skipped**: the canonical repository layout keeps
    /// every source beside its compiled package, the player never compiles and
    /// never reads a source, so a source is expected content rather than a
    /// problem to report. Opening a source explicitly (the Import action or the
    /// `import_file` API) still fails with the compiler command to run; that is
    /// an explicit request and stays actionable. A package that does not open
    /// or validate is skipped with one warning naming the file and the reason.
    ///
    /// Precedence: a bundled package in `assets/levels/` wins over an installed
    /// package with the same level id, and within one directory the
    /// deterministic `(name, id)` order decides. A duplicate id is reported,
    /// never silently replacing a row.
    pub fn refresh(&mut self) {
        self.menu_images.get_mut().clear();
        let mut discovered = Vec::new();
        Self::scan_package_dir(&self.assets_dir, LevelSourceType::Bundled, &mut discovered);
        Self::scan_package_dir(
            &self.levels_dir,
            LevelSourceType::Installed,
            &mut discovered,
        );
        discovered.sort_by(|a, b| {
            source_rank(a.0.source_type)
                .cmp(&source_rank(b.0.source_type))
                .then_with(|| a.0.name.cmp(&b.0.name))
                .then_with(|| a.0.id.cmp(&b.0.id))
                .then_with(|| a.0.path.cmp(&b.0.path))
        });
        let mut unique: Vec<LevelEntry> = Vec::with_capacity(discovered.len());
        self.menu_entries.clear();
        for (entry, visible) in discovered {
            if let Some(existing) = unique.iter().find(|existing| existing.id == entry.id) {
                crate::engine::logging::warn_once(
                    format!("level-duplicate:{}", entry.path.display()),
                    format!(
                        "[levels] {} has the same id '{}' as {}; keeping {}",
                        entry.path.display(),
                        entry.id,
                        existing.path.display(),
                        existing.path.display()
                    ),
                );
                continue;
            }
            if visible {
                self.menu_entries.push(entry.clone());
            }
            unique.push(entry);
        }

        self.entries = unique;
    }

    /// Adds every `*.placesmap` directly below `dir` to `discovered`.
    fn scan_package_dir(
        dir: &Path,
        source_type: LevelSourceType,
        discovered: &mut Vec<(LevelEntry, bool)>,
    ) {
        let read_dir = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                Self::report_skipped_level(dir, &format!("Cannot read map directory: {error}"));
                return;
            }
        };
        for result in read_dir {
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    Self::report_skipped_level(dir, &format!("Cannot enumerate maps: {error}"));
                    continue;
                }
            };
            let path = entry.path();
            if Self::is_authoring_source(&path) {
                // Expected content, not a problem: see `is_authoring_source`.
                continue;
            }
            if !crate::engine::package::is_package_path(&path) {
                continue;
            }
            match Self::probe_package_file(&path, source_type) {
                Ok(meta) => match crate::engine::package::project::inspect(&path) {
                    Ok(Some(index)) => {
                        for level in &index.project.levels {
                            let mut item = meta.clone();
                            item.id.clone_from(&level.id);
                            item.name = if level.id == index.project.entry_level {
                                index.project.name.clone()
                            } else {
                                format!(
                                    "{} / {}",
                                    index.project.name,
                                    if level.display_name.is_empty() {
                                        &level.id
                                    } else {
                                        &level.display_name
                                    }
                                )
                            };
                            discovered.push((
                                item,
                                !index.project.menu_entry_only
                                    || level.id == index.project.entry_level,
                            ));
                        }
                    }
                    Ok(None) => discovered.push((meta, true)),
                    Err(error) => Self::report_skipped_level(&path, &error),
                },
                Err(error) => Self::report_skipped_level(&path, &error),
            }
        }
    }

    /// True for an authoring source the player never plays and never reports.
    ///
    /// A level directory intentionally holds `.json` sources (and the occasional
    /// `.zip` authoring bundle) beside the `.placesmap` packages compiled from
    /// them. Discovery enumerates playable packages only and skips these
    /// silently; an explicit open of one is rejected with the compiler command
    /// by [`Self::import_file`].
    #[must_use]
    pub(crate) fn is_authoring_source(path: &Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext == "json" || ext == "zip")
    }

    /// Reports one unreadable level file once per path.
    fn report_skipped_level(path: &Path, error: &str) {
        crate::engine::logging::warn_once(
            format!("level-skipped:{}", path.display()),
            format!("[levels] skipping {}: {error}", path.display()),
        );
    }

    /// Reads one package's manifest and semantics for discovery.
    fn probe_package_file(path: &Path, source_type: LevelSourceType) -> Result<LevelEntry, String> {
        let opened = crate::engine::package::world::open(path)?;
        Ok(LevelEntry {
            id: opened.level.id,
            name: opened.level.name,
            author: opened.level.author,
            source_type,
            path: path.to_path_buf(),
        })
    }

    /// Loads the first visible menu entry in deterministic discovery order.
    ///
    /// # Errors
    ///
    /// Returns the package's load error, or installation guidance when empty.
    pub fn load_default(&self) -> Result<LoadedLevel, String> {
        let entry = self.menu_entries.first().ok_or_else(|| {
            format!(
                "No maps installed. Put compiled .placesmap files in {} and choose Import levels.",
                self.import_dir.display()
            )
        })?;
        self.load_level(entry)
    }

    /// Resolves materials and fixture sheets for a validated package's level.
    ///
    /// The package carries the authoring-prepared semantics and the prepared
    /// world records; only the texture pixels come from the installed asset
    /// bundle, which the manifest identifies by content hash.
    ///
    /// # Errors
    ///
    /// Returns the named aggregate texture-budget rejection when the level's
    /// distinct decoded images exceed
    /// [`crate::engine::level::MAX_LEVEL_TEXTURE_BYTES`]. The check happens here,
    /// after resolution and before any GPU upload; the material *count* is
    /// already bounded by `validate_level`.
    fn assembled_level(
        &self,
        entry: LevelEntry,
        opened: crate::engine::package::world::OpenedPackage,
    ) -> Result<LoadedLevel, String> {
        let package_identity = Some(crate::engine::package::world::manifest_identity(
            &opened.manifest,
        )?);
        let level = opened.level;
        let materials = self.resolve_level_materials(&level, None)?;
        crate::engine::materials::check_texture_budget(&materials)
            .map_err(|error| format!("level `{}`: {error}", level.id))?;
        let light_sheets = self.resolve_level_fixture_sheets(&level, None, &materials)?;
        Ok(LoadedLevel {
            catalog: Arc::new(self.prop_catalog.clone()),
            level,
            materials,
            light_sheets,
            entry,
            package_identity,
        })
    }

    /// Resolves a level's surface materials through the catalog and an optional
    /// pack, reporting every problem once with its level and material context.
    fn resolve_level_materials(
        &self,
        level: &LevelDef,
        pack: Option<&PackMaterials>,
    ) -> Result<MaterialTable, String> {
        let root = crate::engine::assets::resolve_asset_root();
        let mut cache = self.texture_cache.borrow_mut();
        cache.begin_level();
        let table = resolve_materials(
            level,
            self.prop_catalog.assets(),
            pack,
            root.as_deref(),
            &mut cache,
        )
        .map_err(|error| format!("level `{}`: {error}", level.id))?;
        if root.is_none() && !table.errors().is_empty() {
            // Without an asset root the missing-root report above already said
            // why every one of these failed; do not repeat it per material.
            crate::engine::logging::warn_once(
                format!("materials-no-root:{}", level.id),
                format!(
                    "[materials] {}: {} material(s) unresolved (no asset root; see above)",
                    level.id,
                    table.errors().len()
                ),
            );
            return Ok(table);
        }
        for error in table.errors() {
            crate::engine::logging::warn_once(
                format!("material:{}:{error}", level.id),
                format!("[materials] {}: {error}", level.id),
            );
        }
        Ok(table)
    }

    /// Resolves a level's fixture sheets through the catalog and an optional
    /// pack, logging every problem once with its fixture id in it.
    fn resolve_level_fixture_sheets(
        &self,
        level: &LevelDef,
        pack: Option<&PackMaterials>,
        materials: &MaterialTable,
    ) -> Result<Vec<ResolvedFixtureSheet>, String> {
        let mut cache = self.texture_cache.borrow_mut();
        resolve_fixture_sheets(
            level,
            self.prop_catalog.assets(),
            pack,
            &mut cache,
            materials,
        )
    }

    /// Loads a validated compiled level package.
    ///
    /// Declared asset dependencies must match the package's recorded size and
    /// hash; missing or changed dependencies reject the load. Unresolved
    /// semantic texture references without a declared dependency use the
    /// diagnostic material and a logged error.
    ///
    /// # Errors
    ///
    /// Returns a message when the package cannot be opened, its manifest or
    /// semantics fail validation, or the entry names a missing file.
    pub fn load_level(&self, entry: &LevelEntry) -> Result<LoadedLevel, String> {
        let result = match entry.source_type {
            LevelSourceType::Bundled | LevelSourceType::Installed => {
                crate::engine::package::world::open_level(&entry.path, &entry.id)
                    .and_then(|opened| self.assembled_level(entry.clone(), opened))
            }
        };
        // Resolution has finished (or failed); only bounded optional cache
        // ownership remains. A successful level keeps its own image Arcs.
        self.texture_cache.borrow_mut().begin_level();
        result
    }

    /// Resolves pixels from the same validated semantic snapshot used by the
    /// request's variant reader. No path/mtime or cross-request validity cache.
    pub(crate) fn load_opened_level(
        &self,
        entry: &LevelEntry,
        opened: crate::engine::package::world::OpenedPackage,
    ) -> Result<LoadedLevel, String> {
        let result = if opened.level.id == entry.id {
            self.assembled_level(entry.clone(), opened)
        } else {
            Err("Opened package does not describe the selected level".into())
        };
        self.texture_cache.borrow_mut().begin_level();
        result
    }

    /// Imports an external `.placesmap` package into the installed levels
    /// directory.
    ///
    /// Raw authoring sources are rejected with the explicit compiler command:
    /// the player never compiles a map.
    ///
    /// # Errors
    ///
    /// Returns a message when the source file does not exist, is not a package,
    /// exceeds the import byte budget, cannot be validated, or cannot be
    /// staged and atomically published in the installed levels directory.
    /// A failed import preserves the previous installed file.
    pub fn import_file(&mut self, source_path: &Path) -> Result<LevelEntry, String> {
        if !source_path.exists() {
            return Err(format!(
                "Source file does not exist: {}",
                source_path.display()
            ));
        }
        let extension = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !crate::engine::package::is_package_path(source_path) {
            if extension == "json" || extension == "zip" {
                return Err(format!(
                    "{} is an authoring source. Compile it first with \
                     `places-compile build {}` and import the resulting .placesmap",
                    source_path.display(),
                    source_path.display()
                ));
            }
            return Err(format!(
                "Unsupported file format '.{extension}'. Supported format: .placesmap"
            ));
        }
        let file_name = source_path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;
        let portable_name = file_name.to_str().ok_or("Import file name must be UTF-8")?;
        let _validated = crate::engine::project::ProjectPath::new(portable_name.to_string())?;
        fs::create_dir_all(&self.levels_dir)
            .map_err(|error| format!("Failed to create levels directory: {error}"))?;
        let target_path = self.levels_dir.join(file_name);
        for result in fs::read_dir(&self.levels_dir).map_err(|error| error.to_string())? {
            let candidate = result.map_err(|error| error.to_string())?;
            if candidate.file_name() != file_name
                && candidate
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(portable_name))
            {
                return Err(format!(
                    "Import name conflicts with {}; rename the source file",
                    candidate.path().display()
                ));
            }
        }
        self.refresh();
        // Always stage, including aliases of the installed file: opening an
        // independent temporary never truncates the source's underlying inode.
        // Validation describes the exact bytes published, not an earlier read
        // of a source another process may edit while import is running.
        let opened = import_package(source_path, &target_path, |opened, staged_path| {
            self.accept_import(opened, staged_path, &target_path)
        })?;
        self.refresh();
        Ok(LevelEntry {
            id: opened.level.id,
            name: opened.level.name,
            author: opened.level.author,
            source_type: LevelSourceType::Installed,
            path: target_path,
        })
    }

    fn accept_import(
        &self,
        opened: &OpenedPackage,
        staged_path: &Path,
        target_path: &Path,
    ) -> Result<(), String> {
        let incoming_project = crate::engine::package::project::inspect(staged_path)?;
        if let Some(index) = &incoming_project {
            self.check_project_identity(&index.project.id, target_path)?;
        }
        let incoming_ids: Vec<&str> = incoming_project.as_ref().map_or_else(
            || vec![opened.level.id.as_str()],
            |index| {
                index
                    .project
                    .levels
                    .iter()
                    .map(|level| level.id.as_str())
                    .collect()
            },
        );
        if let Some(existing) = self.entries.iter().find(|entry| {
            incoming_ids
                .iter()
                .any(|id| entry.id.eq_ignore_ascii_case(id))
                && entry.path != target_path
        }) {
            return Err(format!(
                "Map id '{}' is already installed at {}; update that file's name instead",
                existing.id,
                existing.path.display()
            ));
        }
        match fs::symlink_metadata(target_path) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    return Err(format!(
                        "Refusing to replace non-regular import destination {}",
                        target_path.display()
                    ));
                }
                let previous =
                    crate::engine::package::world::open(target_path).map_err(|error| {
                        format!(
                            "Refusing to replace unreadable map {}: {error}",
                            target_path.display()
                        )
                    })?;
                let previous_project = crate::engine::package::project::inspect(target_path)?;
                let same_identity = match (&previous_project, &incoming_project) {
                    (Some(old), Some(new)) => old.project.id == new.project.id,
                    (None, None) => previous.level.id == opened.level.id,
                    (Some(_), None) | (None, Some(_)) => false,
                };
                if !same_identity {
                    return Err(format!(
                        "Refusing to overwrite map '{}' with '{}' at {}; rename the incoming file",
                        previous.level.id,
                        opened.level.id,
                        target_path.display()
                    ));
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Cannot inspect import destination {}: {error}",
                target_path.display()
            )),
        }
    }

    fn check_project_identity(&self, id: &str, target: &Path) -> Result<(), String> {
        let paths: std::collections::BTreeSet<_> = self
            .entries
            .iter()
            .map(|entry| &entry.path)
            .filter(|path| *path != target)
            .collect();
        for path in paths {
            if let Some(index) = crate::engine::package::project::inspect(path)?
                && index.project.id == id
            {
                return Err(format!(
                    "Project id '{id}' is already installed at {}; update that filename instead",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    /// Imports every unimported `.placesmap` from `import/` (and the nested
    /// `levels/import/`).
    ///
    /// Raw authoring sources found there are reported with their compiler
    /// command and skipped.
    ///
    /// # Errors
    ///
    /// Returns a message when a candidate package is malformed; files that
    /// import cleanly are reported through the returned count.
    pub fn import_available(&mut self) -> Result<usize, String> {
        fs::create_dir_all(&self.import_dir).map_err(|error| {
            format!(
                "Failed to create import directory {}: {error}",
                self.import_dir.display()
            )
        })?;
        fs::create_dir_all(&self.levels_dir).map_err(|error| {
            format!(
                "Failed to create levels directory {}: {error}",
                self.levels_dir.display()
            )
        })?;
        let mut imported_count: usize = 0;
        let mut first_error = None;
        let nested_import = self.levels_dir.join("import");
        let candidate_dirs = [self.import_dir.clone(), nested_import];
        for dir in &candidate_dirs {
            let entries = match fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(format!(
                        "Cannot read import directory {}: {error}",
                        dir.display()
                    ));
                }
            };
            let mut paths: Vec<PathBuf> = entries
                .map(|result| result.map(|entry| entry.path()))
                .collect::<Result<_, _>>()
                .map_err(|error| format!("Cannot enumerate {}: {error}", dir.display()))?;
            paths.sort();
            for path in paths {
                if crate::engine::package::is_package_path(&path) {
                    match self.import_file(&path) {
                        Ok(_) => imported_count = imported_count.saturating_add(1),
                        Err(error) => {
                            Self::report_skipped_level(&path, &error);
                            if first_error.is_none() {
                                first_error = Some(error);
                            }
                        }
                    }
                } else if path
                    .extension()
                    .is_some_and(|ext| ext == "json" || ext == "zip")
                {
                    crate::engine::logging::warn_once(
                        format!("import-source:{}", path.display()),
                        format!(
                            "[levels] {} is an authoring source; compile it with \
                             `places-compile build {}` and import the .placesmap",
                            path.display(),
                            path.display()
                        ),
                    );
                }
            }
        }
        // Also discover packages dropped directly into levels/ while running,
        // including when import/ is empty or a different candidate failed.
        self.refresh();
        first_error.map_or(Ok(imported_count), |error| {
            Err(format!(
                "Imported {imported_count} map(s); skipped an invalid package: {error}"
            ))
        })
    }
}

/// Copies into an exclusively owned sibling and publishes only validated bytes.
fn import_package(
    source: &Path,
    target: &Path,
    accept: impl FnOnce(&OpenedPackage, &Path) -> Result<(), String>,
) -> Result<OpenedPackage, String> {
    // Reuse nonblocking regular-file admission so a regular-file/FIFO swap
    // cannot block between path inspection and opened-handle validation.
    let mut input = crate::engine::package::archive::open_file(source)?;
    let (temporary, output) = create_temporary(target)?;
    let staged = stage_import(&mut input, output, &temporary)
        .map_err(|error| cleanup_failed_write(&temporary, error))?;
    accept(&staged, &temporary).map_err(|error| cleanup_failed_write(&temporary, error))?;
    drop(input);
    fs::rename(&temporary, target).map_err(|error| {
        cleanup_failed_write(
            &temporary,
            format!(
                "Failed to publish imported map {}: {error}",
                target.display()
            ),
        )
    })?;
    Ok(staged)
}

/// Owns and closes the write handle before validation or temporary cleanup.
fn stage_import(
    input: &mut fs::File,
    mut output: fs::File,
    temporary: &Path,
) -> Result<OpenedPackage, String> {
    let copied = std::io::copy(
        &mut input.take(MAX_IMPORT_BYTES.saturating_add(1)),
        &mut output,
    )
    .map_err(|error| {
        format!(
            "Failed to stage imported map {}: {error}",
            temporary.display()
        )
    })?;
    if copied > MAX_IMPORT_BYTES {
        return Err(format!(
            "Import source grew beyond the byte limit ({MAX_IMPORT_BYTES})"
        ));
    }
    output.flush().map_err(|error| {
        format!(
            "Failed to flush imported map {}: {error}",
            temporary.display()
        )
    })?;
    output.sync_all().map_err(|error| {
        format!(
            "Failed to sync imported map {}: {error}",
            temporary.display()
        )
    })?;
    drop(output);
    crate::engine::package::project::validate(temporary, &|| Ok(()))?;
    crate::engine::package::world::open(temporary)
        .map_err(|error| format!("Imported map validation failed: {error}"))
}

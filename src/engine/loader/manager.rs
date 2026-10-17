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

/// Import transport budget: the decompressed package budget plus generous ZIP
/// metadata and incompressible-deflate overhead. This bounds staging disk use
/// even when the source grows while copying or carries unused archive padding.
const MAX_IMPORT_BYTES: u64 = crate::engine::package::MAX_TOTAL_BYTES + 64 * 1024 * 1024;

/// Menu precedence of a source: bundled packages before installed ones.
const fn source_rank(source_type: LevelSourceType) -> u8 {
    match source_type {
        LevelSourceType::Bundled => 0,
        LevelSourceType::Installed => 1,
    }
}

/// Unified level loader and package manager.
pub struct LevelManager {
    assets_dir: PathBuf,
    levels_dir: PathBuf,
    import_dir: PathBuf,
    entries: Vec<LevelEntry>,
    prop_catalog: PropCatalog,
    /// Decoded texture images shared across level loads (one decode per
    /// logical texture per session).
    texture_cache: RefCell<TextureCache>,
}

impl Default for LevelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelManager {
    #[must_use]
    pub fn new() -> Self {
        let assets_dir = crate::engine::assets::resolve_asset_root().map_or_else(
            || PathBuf::from("assets/levels"),
            |root| root.join("levels"),
        );
        let mut manager = Self {
            assets_dir,
            levels_dir: crate::engine::assets::state_path("levels"),
            import_dir: crate::engine::assets::state_path("import"),
            entries: Vec::new(),
            prop_catalog: PropCatalog::load_default(),
            texture_cache: RefCell::new(TextureCache::new()),
        };
        manager.ensure_directories();
        manager.refresh();
        manager
    }

    /// Creates the writable directories a fresh install needs.
    ///
    /// A first launch must not require the player to construct `levels/` or
    /// `import/` by hand, and a read-only installation must still boot: a
    /// failure is reported once and the level list simply comes from the
    /// shipped `assets/levels/` folder.
    pub fn ensure_directories(&self) {
        for dir in [&self.levels_dir, &self.import_dir] {
            if let Err(error) = fs::create_dir_all(dir) {
                crate::engine::logging::warn_once(
                    format!("state-dir:{}", dir.display()),
                    format!(
                        "[levels] cannot create {}: {error}; custom levels may be unavailable",
                        dir.display()
                    ),
                );
            }
        }
    }

    #[must_use]
    pub fn with_paths(assets_dir: PathBuf, levels_dir: PathBuf, import_dir: PathBuf) -> Self {
        let mut manager = Self {
            assets_dir,
            levels_dir,
            import_dir,
            entries: Vec::new(),
            prop_catalog: PropCatalog::load_default(),
            texture_cache: RefCell::new(TextureCache::new()),
        };
        manager.refresh();
        manager
    }

    #[must_use]
    pub fn entries(&self) -> &[LevelEntry] {
        &self.entries
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
        let mut discovered = Vec::new();
        Self::scan_package_dir(&self.assets_dir, LevelSourceType::Bundled, &mut discovered);
        Self::scan_package_dir(
            &self.levels_dir,
            LevelSourceType::Installed,
            &mut discovered,
        );
        discovered.sort_by(|a, b| {
            source_rank(a.source_type)
                .cmp(&source_rank(b.source_type))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.path.cmp(&b.path))
        });
        let mut unique: Vec<LevelEntry> = Vec::with_capacity(discovered.len());
        for entry in discovered {
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
            unique.push(entry);
        }

        self.entries = unique;
    }

    /// Adds every `*.placesmap` directly below `dir` to `discovered`.
    fn scan_package_dir(
        dir: &Path,
        source_type: LevelSourceType,
        discovered: &mut Vec<LevelEntry>,
    ) {
        let Ok(read_dir) = fs::read_dir(dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if Self::is_authoring_source(&path) {
                // Expected content, not a problem: see `is_authoring_source`.
                continue;
            }
            if !crate::engine::package::is_package_path(&path) {
                continue;
            }
            match Self::probe_package_file(&path, source_type) {
                Ok(meta) => discovered.push(meta),
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

    /// Loads the first installed package in deterministic discovery order.
    ///
    /// # Errors
    ///
    /// Returns the package's load error, or installation guidance when empty.
    pub fn load_default(&self) -> Result<LoadedLevel, String> {
        let entry = self.entries.first().ok_or_else(|| {
            "No maps installed. Add a .placesmap to levels/ or use Import levels.".to_owned()
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
                crate::engine::package::world::open(&entry.path)
                    .and_then(|opened| self.assembled_level(entry.clone(), opened))
            }
        };
        // Resolution has finished (or failed); only bounded optional cache
        // ownership remains. A successful level keeps its own image Arcs.
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
        fs::create_dir_all(&self.levels_dir)
            .map_err(|error| format!("Failed to create levels directory: {error}"))?;
        let target_path = self.levels_dir.join(file_name);
        // Always stage, including aliases of the installed file: opening an
        // independent temporary never truncates the source's underlying inode.
        // Validation describes the exact bytes published, not an earlier read
        // of a source another process may edit while import is running.
        let opened = import_package(source_path, &target_path)?;
        self.refresh();
        Ok(LevelEntry {
            id: opened.level.id,
            name: opened.level.name,
            author: opened.level.author,
            source_type: LevelSourceType::Installed,
            path: target_path,
        })
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
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
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
fn import_package(source: &Path, target: &Path) -> Result<OpenedPackage, String> {
    // Reject ordinary named pipes/devices before open, which could otherwise
    // wait for a writer indefinitely. Recheck the opened handle below too.
    let source_metadata = fs::metadata(source)
        .map_err(|error| format!("Failed to inspect {} for import: {error}", source.display()))?;
    if !source_metadata.is_file() {
        return Err(format!(
            "Import source is not a regular file: {}",
            source.display()
        ));
    }
    let mut input = fs::File::open(source)
        .map_err(|error| format!("Failed to open {} for import: {error}", source.display()))?;
    let metadata = input
        .metadata()
        .map_err(|error| format!("Failed to inspect {} for import: {error}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "Import source is not a regular file: {}",
            source.display()
        ));
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "Import source {} is {} bytes (limit {MAX_IMPORT_BYTES})",
            source.display(),
            metadata.len()
        ));
    }
    let (temporary, output) = create_temporary(target)?;
    let staged = stage_import(&mut input, output, &temporary)
        .map_err(|error| cleanup_failed_write(&temporary, error))?;
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
    crate::engine::package::world::open(temporary)
        .map_err(|error| format!("Imported map validation failed: {error}"))
}

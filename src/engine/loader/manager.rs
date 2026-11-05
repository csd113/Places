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

fn ambiguous_installed_paths(
    discovered: &[(LevelEntry, bool)],
    owners: &DiscoveryOwners,
) -> std::collections::BTreeSet<PathBuf> {
    // Conflicts exclude complete community paths. Bundled rows retain their
    // existing precedence, without exposing another release's hidden members.
    let mut level_owners =
        std::collections::BTreeMap::<String, std::collections::BTreeSet<PathBuf>>::new();
    for (entry, _) in discovered {
        let _inserted = level_owners
            .entry(entry.id.to_ascii_lowercase())
            .or_default()
            .insert(entry.path.clone());
    }
    owners
        .values()
        .chain(level_owners.values())
        .filter(|paths| paths.len() > 1)
        .flat_map(|paths| paths.iter().cloned())
        .collect()
}

type MenuImageCache =
    std::collections::BTreeMap<(PathBuf, String, String), Option<Arc<super::RawImage>>>;

/// One refresh's metadata snapshot, shared with the existing loading worker.
/// No semantic worlds, source files, thumbnail pixels or prepared records live here.
#[derive(serde::Serialize)]
struct MetadataReceipt {
    identity: String,
    version: crate::engine::package::release::ReleaseVersion,
    project: Option<crate::engine::project::ProjectManifest>,
}

const MAX_METADATA_RECEIPTS: usize = 64;
const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
type MetadataCache = std::collections::BTreeMap<PathBuf, Arc<MetadataReceipt>>;
type DiscoveryOwners =
    std::collections::BTreeMap<(String, Option<String>), std::collections::BTreeSet<PathBuf>>;

/// Unified level loader and package manager.
pub struct LevelManager {
    assets_dir: PathBuf,
    levels_dir: PathBuf,
    import_dir: PathBuf,
    entries: Vec<LevelEntry>,
    menu_entries: Vec<LevelEntry>,
    menu_images: RefCell<MenuImageCache>,
    metadata: RefCell<MetadataCache>,
    prop_catalog: Arc<PropCatalog>,
    /// Decoded texture images shared across level loads (one decode per
    /// logical texture per session).
    texture_cache: RefCell<TextureCache>,
}

impl LevelManager {
    /// # Errors
    /// Returns the shared resource/storage configuration error.
    pub fn new() -> Result<Self, String> {
        let paths = crate::engine::paths::user()?;
        crate::engine::resources::checkpoint()?;
        let manager = Self::with_paths(
            crate::engine::paths::assets()?.join("levels"),
            paths.levels(),
            paths.imports(),
        );
        crate::engine::resources::checkpoint()?;
        Ok(manager)
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
            metadata: RefCell::new(MetadataCache::new()),
            prop_catalog: Arc::new(PropCatalog::load_default()),
            texture_cache: RefCell::new(TextureCache::new()),
        };
        manager.refresh();
        manager
    }

    /// Independent mutable discovery/image caches over one immutable catalog.
    /// Bootstrap needs UI and worker handles, never a second catalog read.
    pub(crate) fn fork_for_loading(&self) -> Self {
        Self {
            assets_dir: self.assets_dir.clone(),
            levels_dir: self.levels_dir.clone(),
            import_dir: self.import_dir.clone(),
            entries: self.entries.clone(),
            menu_entries: self.menu_entries.clone(),
            menu_images: RefCell::new(std::collections::BTreeMap::new()),
            metadata: RefCell::new(self.metadata.borrow().clone()),
            prop_catalog: Arc::clone(&self.prop_catalog),
            texture_cache: RefCell::new(TextureCache::new()),
        }
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
        let identity = crate::engine::package::world::discovery_identity(&entry.path).ok()?;
        let key = (entry.path.clone(), entry.id.clone(), identity);
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
        let description = self
            .current_metadata(&entry.path)
            .and_then(|receipt| {
                receipt.project.as_ref().and_then(|project| {
                    project.level(Some(&entry.id)).ok().map(|level| {
                        if level.description.trim().is_empty() && entry.id == project.entry_level {
                            project.description.clone()
                        } else {
                            level.description.clone()
                        }
                    })
                })
            })
            .unwrap_or_default();
        let version = self
            .current_metadata(&entry.path)
            .map(|receipt| format!("\nPackage version {}", receipt.version))
            .unwrap_or_default();
        if description.trim().is_empty() {
            format!(
                "{}\n\nA place to explore at your own pace.\n\nBy {}{version}",
                entry.name,
                if entry.author.is_empty() {
                    "an independent creator"
                } else {
                    &entry.author
                }
            )
        } else {
            format!(
                "{}\n\n{}\n\nBy {}{version}",
                entry.name, description, entry.author
            )
        }
    }

    /// Names from the just-discovered snapshot; startup does not reopen each
    /// project for menu naming. Import/refresh replaces this snapshot together.
    pub(crate) fn menu_names(&self) -> Vec<String> {
        let receipts = self.metadata.borrow().clone();
        self.menu_entries
            .iter()
            .map(|entry| {
                let receipt = receipts
                    .get(&entry.path)
                    .cloned()
                    .or_else(|| self.current_metadata(&entry.path));
                let prefix = receipt
                    .as_ref()
                    .and_then(|metadata| metadata.project.as_ref())
                    .map(|project| format!("{} / ", project.name));
                prefix
                    .as_deref()
                    .and_then(|project_prefix| entry.name.strip_prefix(project_prefix))
                    .unwrap_or(&entry.name)
                    .to_string()
            })
            .collect()
    }

    fn current_metadata(&self, path: &Path) -> Option<Arc<MetadataReceipt>> {
        let fresh_identity = crate::engine::package::world::discovery_identity(path).ok()?;
        if let Some(receipt) = self
            .metadata
            .borrow()
            .get(path)
            .filter(|receipt| receipt.identity == fresh_identity)
        {
            return Some(Arc::clone(receipt));
        }
        let (opened, project, identity) = crate::engine::package::world::discover(path).ok()?;
        let receipt = Arc::new(MetadataReceipt {
            identity,
            version: opened.manifest.package_version,
            project: project.map(|index| index.project),
        });
        Self::retain_metadata(&mut self.metadata.borrow_mut(), path, &receipt);
        Some(receipt)
    }

    fn retain_metadata(cache: &mut MetadataCache, path: &Path, receipt: &Arc<MetadataReceipt>) {
        let charge = |item: &MetadataReceipt| {
            serde_json::to_vec(item).map_or(MAX_METADATA_BYTES, |bytes| bytes.len())
        };
        let bytes = cache
            .iter()
            .filter(|(cached, _)| cached.as_path() != path)
            .fold(charge(receipt), |total, (_, cached)| {
                total.saturating_add(charge(cached))
            });
        drop(cache.remove(path));
        if cache.len() < MAX_METADATA_RECEIPTS && bytes <= MAX_METADATA_BYTES {
            drop(cache.insert(path.to_path_buf(), Arc::clone(receipt)));
        }
    }

    #[cfg(feature = "visual-diagnostics")]
    pub(crate) fn diagnostic_cached_images(&self) -> serde_json::Value {
        let mut diagnostics = self.texture_cache.borrow().diagnostic_retention();
        let receipts = self.metadata.borrow();
        let encoded_bytes = receipts.values().fold(0_usize, |total, receipt| {
            total.saturating_add(
                serde_json::to_vec(receipt.as_ref())
                    .map_or(MAX_METADATA_BYTES, |bytes| bytes.len()),
            )
        });
        if let Some(object) = diagnostics.as_object_mut() {
            drop(object.insert("metadata_receipts".into(), serde_json::json!({
                "entries": receipts.len(), "encoded_bytes": encoded_bytes,
                "entry_bound": MAX_METADATA_RECEIPTS, "encoded_byte_bound": MAX_METADATA_BYTES,
                "scope": "Immutable metadata receipts share Arcs across UI/loader snapshots; encoded charge excludes path/map/Arc overhead and is not process RSS or an exact heap count. No worlds or decoded images are retained by these receipts."
            })));
        }
        diagnostics
    }

    /// Drops optional decoded map-image ownership when returning to the menu.
    pub(crate) fn release_cached_images(&self) {
        *self.texture_cache.borrow_mut() = TextureCache::new();
    }

    /// Refreshes authoring metadata before a new explicit disk load.
    pub(crate) fn refresh_catalog(&mut self) {
        self.prop_catalog = Arc::new(PropCatalog::load_default());
    }

    /// Prop catalog used to resolve placed props.
    #[must_use]
    pub fn prop_catalog(&self) -> &PropCatalog {
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
        if let Err(error) = self.refresh_checked() {
            crate::engine::logging::info(format!("[levels] discovery stopped: {error}"));
        }
    }

    fn refresh_checked(&mut self) -> Result<(), String> {
        crate::engine::resources::checkpoint()?;
        let mut discovered = Vec::new();
        let mut receipts = MetadataCache::new();
        let mut owners = DiscoveryOwners::new();
        Self::scan_package_dir(
            &self.assets_dir,
            LevelSourceType::Bundled,
            &mut discovered,
            &mut receipts,
            &mut owners,
        )?;
        Self::scan_package_dir(
            &self.levels_dir,
            LevelSourceType::Installed,
            &mut discovered,
            &mut receipts,
            &mut owners,
        )?;
        crate::engine::resources::checkpoint()?;
        discovered.sort_by(|a, b| {
            source_rank(a.0.source_type)
                .cmp(&source_rank(b.0.source_type))
                .then_with(|| a.0.name.cmp(&b.0.name))
                .then_with(|| a.0.id.cmp(&b.0.id))
                .then_with(|| a.0.path.cmp(&b.0.path))
        });
        let mut unique: Vec<LevelEntry> = Vec::with_capacity(discovered.len());
        let ambiguous_paths = ambiguous_installed_paths(&discovered, &owners);
        self.menu_entries.clear();
        for (entry, visible) in discovered {
            if entry.source_type == LevelSourceType::Installed
                && ambiguous_paths.contains(&entry.path)
            {
                Self::report_skipped_level(
                    &entry.path,
                    "Ambiguous duplicate installed map ID; remove the duplicate files before loading or updating",
                );
                continue;
            }
            if let Some(existing) = unique
                .iter()
                .find(|existing| existing.id.eq_ignore_ascii_case(&entry.id))
            {
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
        *self.metadata.get_mut() = receipts;
        self.menu_images.get_mut().clear();
        Ok(())
    }

    /// Adds every `*.placesmap` directly below `dir` to `discovered`.
    fn scan_package_dir(
        dir: &Path,
        source_type: LevelSourceType,
        discovered: &mut Vec<(LevelEntry, bool)>,
        receipts: &mut MetadataCache,
        owners: &mut DiscoveryOwners,
    ) -> Result<(), String> {
        crate::engine::resources::checkpoint()?;
        let read_dir = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                Self::report_skipped_level(dir, &format!("Cannot read map directory: {error}"));
                return Ok(());
            }
        };
        for result in read_dir {
            crate::engine::resources::checkpoint()?;
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
            match crate::engine::package::world::discover(&path) {
                Ok((opened, project, identity)) => {
                    let scope = (
                        opened.manifest.package_id.to_ascii_lowercase(),
                        project
                            .is_none()
                            .then(|| opened.manifest.id.to_ascii_lowercase()),
                    );
                    let _inserted = owners.entry(scope).or_default().insert(path.clone());
                    let meta = LevelEntry {
                        id: opened.level.id,
                        name: opened.level.name,
                        author: opened.level.author,
                        source_type,
                        path: path.clone(),
                    };
                    let receipt = Arc::new(MetadataReceipt {
                        identity,
                        version: opened.manifest.package_version,
                        project: project.as_ref().map(|index| index.project.clone()),
                    });
                    Self::retain_metadata(receipts, &path, &receipt);
                    if let Some(index) = project {
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
                    } else {
                        discovered.push((meta, true));
                    }
                }
                Err(error) => {
                    crate::engine::resources::checkpoint()?;
                    Self::report_skipped_level(&path, &error);
                }
            }
        }
        Ok(())
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
            project_id: opened.project_id,
            catalog: Arc::clone(&self.prop_catalog),
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
        // One directory-wide OS lock protects stable IDs even when incoming names
        // differ. The kernel releases it on crash; never unlink the lock inode.
        let _installation = InstallationLock::acquire(&self.levels_dir)?;
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
        // Admission resolves the stable package ID, independent of an incoming rename.
        let (opened, installed_path) = import_package(source_path, &target_path, |staged_path| {
            self.accept_import(staged_path, &target_path)
        })?;
        self.refresh();
        Ok(LevelEntry {
            id: opened.level.id,
            name: opened.level.name,
            author: opened.level.author,
            source_type: LevelSourceType::Installed,
            path: installed_path,
        })
    }

    fn accept_import(&self, staged_path: &Path, named_target: &Path) -> Result<ImportPlan, String> {
        use crate::engine::package::release::{self, UpdateAdmission};
        let incoming = release::inspect(staged_path)?;
        let mut matching = Vec::new();
        for (directory, bundled) in [(&self.assets_dir, true), (&self.levels_dir, false)] {
            let paths = match fs::read_dir(directory) {
                Ok(paths) => paths,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            for item in paths {
                let path = item.map_err(|error| error.to_string())?.path();
                if !crate::engine::package::is_package_path(&path) {
                    continue;
                }
                let Ok(existing) = release::inspect_installed(&path) else {
                    // Unreadable packages cannot be played. A named destination
                    // is still explicitly checked below and never overwritten.
                    continue;
                };
                let same_id = existing
                    .package_id
                    .eq_ignore_ascii_case(&incoming.package_id)
                    && (existing.project
                        || incoming.project
                        || existing.member_ids == incoming.member_ids);
                let overlap = incoming.member_ids.iter().find(|id| {
                    existing
                        .member_ids
                        .iter()
                        .any(|other| other.eq_ignore_ascii_case(id))
                });
                if bundled && (same_id || overlap.is_some()) {
                    return Err(format!(
                        "Map id '{}' belongs to bundled package {}; community imports cannot replace bundled content",
                        overlap.unwrap_or(&incoming.package_id),
                        path.display()
                    ));
                }
                if same_id {
                    matching.push((path, existing));
                } else if let Some(id) = overlap {
                    return Err(format!(
                        "Map id '{id}' is already owned by another package at {}",
                        path.display()
                    ));
                }
            }
        }
        if matching.len() > 1 {
            return Err(format!(
                "Package '{}' has multiple installed copies; resolve the duplicate files before updating",
                incoming.package_id
            ));
        }
        let (target, unchanged) = if let Some((path, previous)) = matching.pop() {
            let unchanged = previous.admit_update(&incoming)? == UpdateAdmission::Unchanged;
            (path, unchanged)
        } else {
            (named_target.to_path_buf(), false)
        };
        if unchanged {
            validate_unchanged_installation(&target)?;
        }
        let previous_hash = match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    return Err(format!(
                        "Refusing to replace non-regular import destination {}",
                        target.display()
                    ));
                }
                let previous = release::inspect_installed(&target).map_err(|error| {
                    format!(
                        "Refusing to replace unreadable map {}: {error}",
                        target.display()
                    )
                })?;
                let _admission = previous.admit_update(&incoming)?;
                Some(crate::engine::package::hash::sha256_file(&target)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        // A rename update retains the existing filename; never leave a second copy.
        if target != named_target
            && named_target
                .try_exists()
                .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "Incoming filename also names another installed file: {}",
                named_target.display()
            ));
        }
        Ok(ImportPlan {
            target,
            unchanged,
            previous_hash,
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
struct ImportPlan {
    target: PathBuf,
    unchanged: bool,
    previous_hash: Option<String>,
}

fn validate_unchanged_installation(path: &Path) -> Result<(), String> {
    // An equal manifest hash is a content claim, not proof that installed
    // payloads remain intact. Never report corrupt installed bytes as a no-op.
    crate::engine::package::project::validate(path, &|| Ok(()))
        .and_then(|()| crate::engine::package::world::validate_import(path))
        .map_err(|error| format!("Installed package {} failed integrity validation: {error}; retain it for diagnosis and explicitly remove it before reinstalling", path.display()))
}

fn import_package(
    source: &Path,
    named_target: &Path,
    accept: impl FnOnce(&Path) -> Result<ImportPlan, String>,
) -> Result<(OpenedPackage, PathBuf), String> {
    let mut input = crate::engine::package::archive::open_file(source)?;
    let (temporary, output) = create_temporary(named_target)?;
    let staged = stage_import(&mut input, output, &temporary)
        .map_err(|error| cleanup_failed_write(&temporary, error))?;
    let plan = accept(&temporary).map_err(|error| cleanup_failed_write(&temporary, error))?;
    drop(input);
    let current_hash = match fs::symlink_metadata(&plan.target) {
        Ok(metadata) if metadata.file_type().is_file() => Some(
            crate::engine::package::hash::sha256_file(&plan.target)
                .map_err(|error| cleanup_failed_write(&temporary, error))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Ok(_) | Err(_) => {
            return Err(cleanup_failed_write(
                &temporary,
                "Import destination changed during admission".into(),
            ));
        }
    };
    if current_hash != plan.previous_hash {
        return Err(cleanup_failed_write(
            &temporary,
            "Import destination changed during admission; previous data retained".into(),
        ));
    }
    if plan.unchanged {
        fs::remove_file(&temporary).map_err(|error| error.to_string())?;
    } else {
        fs::rename(&temporary, &plan.target).map_err(|error| {
            cleanup_failed_write(
                &temporary,
                format!(
                    "Failed to publish imported map {}: {error}",
                    plan.target.display()
                ),
            )
        })?;
    }
    Ok((staged, plan.target))
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
    crate::engine::package::world::validate_import(temporary)?;
    crate::engine::package::world::open(temporary)
        .map_err(|error| format!("Imported map validation failed: {error}"))
}

/// Serializes admission through publication across threads and processes.
struct InstallationLock(fs::File);
impl InstallationLock {
    fn acquire(directory: &Path) -> Result<Self, String> {
        let path = directory.join(".places-import.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| {
                format!(
                    "Cannot open package installation lock {}: {error}",
                    path.display()
                )
            })?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Package installation lock is not a regular file".into());
        }
        // Fail promptly instead of blocking the menu behind another process.
        file.try_lock().map_err(|error| format!("Another package import is active, or the installation lock is unavailable: {error}. Retry after it finishes."))?;
        Ok(Self(file))
    }
}
impl Drop for InstallationLock {
    fn drop(&mut self) {
        // Closing also releases the OS lock, including during unwinding/crash.
        let _released = self.0.unlock();
    }
}

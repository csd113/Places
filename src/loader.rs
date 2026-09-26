use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use zip::ZipArchive;

use crate::assets::AssetCatalog;
use crate::level::{
    BASEBOARD_DEFAULT_HEIGHT_M, BASEBOARD_DEFAULT_THICKNESS_M, BaseboardDef, LevelDef,
    LevelSurfaces, MAX_LEVEL_FLOOR_AREA_M2, MAX_LEVEL_VERTICES, WALL_SLICE_EPS, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use crate::materials::{MaterialTable, PackMaterials, load_png_relative, resolve_materials};

/// Re-exported so the rest of the crate keeps its historical import paths.
pub use crate::materials::{RawImage, TextureCache, decode_png, encode_png, parse_materials_json};

/// The official demo, embedded so the game still boots when no level files are
/// installed on disk. `Places Demo` is the only level shipped with the game.
const FALLBACK_DEMO_JSON: &str = include_str!("../assets/levels/places_demo.json");

/// Stable id of the one official level, used by discovery and the runtime.
pub const DEMO_LEVEL_ID: &str = "places_demo";

const MAX_ZIP_ENTRIES: usize = 500;
const MAX_ZIP_ENTRY_SIZE: u64 = 10 * 1024 * 1024; // 10 MB per file
const MAX_ZIP_TOTAL_SIZE: u64 = 50 * 1024 * 1024; // 50 MB total uncompressed

/// Reads one ZIP entry with a hard output cap.
///
/// The ZIP header's declared uncompressed size is attacker-controlled and is
/// never trusted: a small deflate stream can declare `size = 1024` and expand
/// to gigabytes. The reader is capped with [`Read::take`] instead, and a read
/// that reaches the cap is an error. The declared size is still used for the
/// capacity hint, clamped to the cap.
fn read_zip_entry_capped<R: Read>(
    reader: &mut R,
    declared_size: u64,
    limit: u64,
    name: &str,
) -> Result<Vec<u8>, String> {
    let capacity = usize::try_from(declared_size.min(limit)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let read = reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {name}: {e}"))?;
    if u64::try_from(read).unwrap_or(u64::MAX) > limit {
        return Err(format!(
            "ZIP entry {name} exceeds the {}MB decompression limit",
            limit / (1024 * 1024)
        ));
    }
    Ok(bytes)
}

/// Source type of an installed level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceType {
    Official,
    CustomJson,
    PackZip,
    /// The demo compiled into the executable, used when no installed copy of
    /// Places Demo exists on disk. Selecting it loads the embedded JSON, so the
    /// demo is always offered no matter what is installed.
    Embedded,
}

/// Discovered level entry for level selection menu.
#[derive(Clone, Debug)]
pub struct LevelEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub source_type: LevelSourceType,
    pub path: PathBuf,
}

/// A validated, fully loaded level ready for gameplay.
///
/// `materials` is the level's resolved surface material table: every material
/// id the level references, with its decoded PNG, tiling and tint.
/// `light_sheets` is the same idea for the fixtures the level places: the
/// decoded visible-face PNG of each fixture family it uses, indexed by
/// [`crate::lighting::FixtureKind::index`], with a family missing from the list
/// drawing the shared untextured white sheet.
#[derive(Clone, Debug)]
pub struct LoadedLevel {
    pub level: LevelDef,
    pub materials: MaterialTable,
    pub light_sheets: Vec<ResolvedFixtureSheet>,
    pub entry: LevelEntry,
}

/// One fixture family's visible face, decoded and ready to upload.
///
/// A fixture's mesh is generated in code, but what that mesh shows is ordinary
/// external artwork: a catalogued fixture names its own PNG sheet, and a level
/// pack may ship one for a `pack:` fixture id. Attribution is per family, so a
/// level that mixes an office panel with a pool downlight resolves two sheets.
#[derive(Clone, Debug)]
pub struct ResolvedFixtureSheet {
    /// Family the sheet draws.
    pub kind: crate::lighting::FixtureKind,
    /// Session-unique decode/dedupe key: the catalog PNG path, or the pack's own
    /// `pack:<namespace>:<path>` key.
    pub key: String,
    /// Where the sheet came from; decides its GPU lifetime.
    pub origin: crate::materials::TextureOrigin,
    /// Decoded pixels, shared with the session cache.
    pub image: Rc<RawImage>,
}

/// Raw contents extracted safely from a ZIP level pack.
///
/// Texture bytes are reference-counted so that the several alias keys a pack
/// may use (`textures/x.png`, `x.png`, ...) share a single physical buffer
/// instead of duplicating it.
#[derive(Default, Debug)]
pub struct RawPackContents {
    pub level_json: String,
    pub materials_json: Option<String>,
    pub textures: HashMap<String, Rc<[u8]>>,
}

/// Reads and parses only the `level.json` entry from a ZIP pack without
/// decompressing any textures or other assets.
///
/// Used for cheap level discovery: probing a pack must not extract its full
/// contents just to learn its id/name/author.
/// # Errors
///
/// Returns a message when the archive is not a readable ZIP, has no
/// `level.json`, or its `level.json` is oversized or not valid UTF-8.
pub fn read_zip_level_json<R: Read + Seek>(reader: R) -> Result<String, String> {
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive exceeds maximum entry count of {MAX_ZIP_ENTRIES}"
        ));
    }

    // Match `extract_zip` semantics: normalize separators and let the last
    // level.json entry win if a pack contains more than one.
    let mut target: Option<usize> = None;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Corrupt ZIP entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let file_name = name.rsplit('/').next().unwrap_or(&name);
        if file_name.eq_ignore_ascii_case("level.json") {
            target = Some(i);
        }
    }

    let index = target.ok_or_else(|| "ZIP level pack missing required 'level.json'".to_string())?;
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("Corrupt ZIP entry {index}: {e}"))?;
    let declared = entry.size();
    let bytes = read_zip_entry_capped(&mut entry, declared, MAX_ZIP_ENTRY_SIZE, "level.json")?;
    String::from_utf8(bytes).map_err(|e| format!("level.json is not valid UTF-8: {e}"))
}

/// Safely extracts a ZIP level pack with path traversal and size limits enforcement.
/// # Errors
///
/// Returns a message when the archive is not a readable ZIP, an entry escapes
/// the extraction boundary, an entry or the pack exceeds the size limits, or the
/// pack has no `level.json`.
pub fn extract_zip<R: Read + Seek>(reader: R) -> Result<RawPackContents, String> {
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive exceeds maximum entry count of {MAX_ZIP_ENTRIES}"
        ));
    }

    let mut pack = RawPackContents::default();
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Corrupt ZIP entry {i}: {e}"))?;
        if file.is_dir() {
            continue;
        }

        // Security: Path traversal validation
        let raw_name = file.name().to_string();
        if raw_name.contains("..") || raw_name.starts_with('/') || raw_name.starts_with('\\') {
            return Err(format!(
                "Unsafe path traversal detected in ZIP entry: {raw_name}"
            ));
        }
        if file.enclosed_name().is_none() {
            return Err(format!(
                "Path in ZIP is outside extraction boundary: {raw_name}"
            ));
        }

        // Security: exclude executable or script extensions. File extensions
        // are compared case-insensitively, so `PATCH.EXE` is rejected too.
        if Path::new(&raw_name).extension().is_some_and(|extension| {
            ["exe", "sh", "bat", "so", "dylib", "dll", "bin", "wasm"]
                .iter()
                .any(|blocked| extension.eq_ignore_ascii_case(blocked))
        }) {
            continue;
        }

        let declared = file.size();
        let bytes = read_zip_entry_capped(&mut file, declared, MAX_ZIP_ENTRY_SIZE, &raw_name)?;
        total_uncompressed =
            total_uncompressed.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if total_uncompressed > MAX_ZIP_TOTAL_SIZE {
            return Err("Total uncompressed size of ZIP exceeds 50MB limit".into());
        }

        let normalized = raw_name.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);

        if file_name.eq_ignore_ascii_case("level.json") {
            pack.level_json = String::from_utf8(bytes)
                .map_err(|e| format!("level.json is not valid UTF-8: {e}"))?;
        } else if file_name.eq_ignore_ascii_case("materials.json") {
            pack.materials_json = Some(
                String::from_utf8(bytes)
                    .map_err(|e| format!("materials.json is not valid UTF-8: {e}"))?,
            );
        } else if normalized.contains("textures/")
            || Path::new(&normalized)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
            // Share one physical buffer across every alias key.
            let blob: Rc<[u8]> = Rc::from(bytes);
            pack.textures.insert(normalized.clone(), Rc::clone(&blob));
            if let Some(tex_sub) = normalized.split("textures/").nth(1) {
                pack.textures
                    .insert(format!("textures/{tex_sub}"), Rc::clone(&blob));
                pack.textures.insert(tex_sub.to_string(), Rc::clone(&blob));
            }
            pack.textures.insert(file_name.to_string(), blob);
        }
    }

    if pack.level_json.is_empty() {
        return Err("ZIP level pack missing required 'level.json'".into());
    }

    Ok(pack)
}

/// Neutral fallback colour used for unknown prop models, `#8a8a8a`.
const PROP_FALLBACK_COLOR_HEX: &str = "#8a8a8a";

/// Neutral grey used for unknown prop models.
fn prop_fallback_color() -> [f32; 3] {
    parse_hex_color(PROP_FALLBACK_COLOR_HEX).unwrap_or([0.541, 0.541, 0.541])
}

/// Parses `#rrggbb` (or bare `rrggbb`) into 0..1 RGB components.
///
/// The parser lives with the catalog that stores those colours
/// ([`crate::assets`]); this re-export keeps the level loader's existing call
/// sites and tests unchanged.
pub use crate::assets::parse_hex_color;

/// One catalog entry describing a placeable prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropCatalogEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub model: Option<String>,
    pub solid: bool,
}

/// Catalog of placeable assets (environment props and entities) available to
/// levels.
///
/// This is the placement view of the authoritative [`crate::assets::AssetCatalog`]:
/// levels reference a logical id such as `core:chair` or `spooner-man`, and the
/// catalog maps it to the canonical model resource under the asset root.
/// Entities resolve through exactly the same lookup, so `spooner-man` keeps
/// working unchanged.
///
/// Lookups always succeed: unknown or non-placeable ids resolve to a generated
/// fallback entry using [`crate::level::PROP_FALLBACK_SIZE`] and a neutral
/// colour, so a level referencing a missing prop still loads with an obvious
/// placeholder instead of failing or rendering the wrong model.
#[derive(Debug, Clone, Default)]
pub struct PropCatalog {
    assets: crate::assets::AssetCatalog,
}

impl PropCatalog {
    /// Empty catalog; every lookup falls back.
    #[must_use]
    pub fn builtin() -> Self {
        Self::default()
    }

    /// Parses an asset catalog document into the placement view.
    ///
    /// Accepts the generalized `assets` shape and the legacy `props` shape.
    /// Entries without an `id` are skipped in the legacy shape; duplicate
    /// logical ids are rejected.
    /// # Errors
    ///
    /// Returns a message when the document is not valid JSON, when an id is
    /// duplicated or malformed, or when an entry declares a malformed
    /// class/theme/type/source/resource path.
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        Ok(Self {
            assets: crate::assets::AssetCatalog::from_json_str(json)?,
        })
    }

    /// Loads a catalog from `path`, returning `None` when the file is missing
    /// or invalid. Never panics.
    #[must_use]
    pub fn load_from_path(path: &Path) -> Option<Self> {
        Some(Self {
            assets: crate::assets::AssetCatalog::load_from_path(path)?,
        })
    }

    /// Loads the shipped asset catalog, falling back to an empty catalog when
    /// no `assets/catalog.json` can be found.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            assets: crate::assets::AssetCatalog::load_default(),
        }
    }

    /// The authoritative catalog behind the placement view.
    #[must_use]
    pub const fn assets(&self) -> &crate::assets::AssetCatalog {
        &self.assets
    }

    /// The declared environment themes, in catalog order.
    #[must_use]
    pub fn themes(&self) -> &[crate::assets::AssetThemeDef] {
        self.assets.themes()
    }

    /// Resolves a logical asset id, or a generated fallback entry for unknown
    /// or non-placeable ids.
    #[must_use]
    pub fn get(&self, model: &str) -> PropCatalogEntry {
        if let Some(entry) = self.assets.placeable(model) {
            return placeable_entry(entry);
        }
        PropCatalogEntry {
            id: model.to_string(),
            name: model.to_string(),
            category: "Other".into(),
            size: crate::level::PROP_FALLBACK_SIZE,
            color: prop_fallback_color(),
            model: None,
            solid: false,
        }
    }

    /// Number of placeable catalog entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.assets.placeable_entries().len()
    }

    /// Every placeable entry, ordered by id so validation and reports are stable.
    #[must_use]
    pub fn entries(&self) -> Vec<PropCatalogEntry> {
        self.assets
            .placeable_entries()
            .into_iter()
            .map(placeable_entry)
            .collect()
    }

    /// True when the catalog defines this exact placeable id.
    #[must_use]
    pub fn contains(&self, model: &str) -> bool {
        self.assets.placeable(model).is_some()
    }

    /// True when the catalog has no placeable entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Converts a catalog entry into the runtime's placeable view, applying the
/// neutral fallbacks for missing optional metadata.
fn placeable_entry(entry: &crate::assets::AssetEntry) -> PropCatalogEntry {
    PropCatalogEntry {
        id: entry.id.clone(),
        name: entry.display_name.clone(),
        category: entry
            .category
            .clone()
            .unwrap_or_else(|| "Other".to_string()),
        size: entry
            .size
            .filter(|size| size.iter().all(|value| value.is_finite() && *value > 0.0))
            .unwrap_or(crate::level::PROP_FALLBACK_SIZE),
        color: entry.color.unwrap_or_else(prop_fallback_color),
        model: entry.model.clone(),
        solid: entry.solid,
    }
}

/// Validates level schema, format version, and physical dimensions.
///
/// Preserves intentional overlapping/intersecting geometry without snapping or
/// rejecting. The checks run in their historical order, so the first reported
/// problem is unchanged.
/// # Errors
///
/// Returns the first problem found: an unsupported format version, a missing or
/// duplicate id, non-finite or out-of-range dimensions, a prop outside its
/// budgets, or malformed openings and patches.
pub fn validate_level(level: &LevelDef) -> Result<(), String> {
    validate_header(level)?;
    validate_element_limits(level)?;
    validate_rooms(level)?;
    validate_surface_shine(level)?;
    validate_floor_regions(level)?;
    validate_water(level)?;
    validate_walls(level)?;
    validate_architecture(level)?;
    validate_ceiling_lights(level)?;
    validate_props(level)?;
    validate_prop_lights(level)?;
    validate_decals(level)?;
    validate_decal_surfaces(level)?;
    validate_animated_emissions(level)?;
    validate_geometry_budget(level)
}

/// Animated emissions: a known effect, a finite rate and a bounded depth.
///
/// A malformed animation is a level error rather than a silent no-op: a sign
/// that was meant to breathe and does not is a bug the author has to see.
fn validate_animated_emissions(level: &LevelDef) -> Result<(), String> {
    for (i, animation) in level.animated_emissions.iter().enumerate() {
        let id = animation.material.trim();
        if id.is_empty() {
            return Err(format!("Animated emission {i} names no material"));
        }
        let effect = animation
            .effect
            .as_deref()
            .map(str::trim)
            .filter(|effect| !effect.is_empty());
        if let Some(effect) = effect
            && crate::render::AnimationEffect::parse(effect).is_none()
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) has an unknown effect `{effect}`; \
                 expected `pulse` or `flicker`"
            ));
        }
        if let Some(hz) = animation.hz
            && (!hz.is_finite() || hz <= 0.0 || hz > crate::render::MAX_FLICKER_HZ)
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) must have a rate between 0 and {} Hz",
                crate::render::MAX_FLICKER_HZ
            ));
        }
        if let Some(depth) = animation.depth
            && (!depth.is_finite() || depth <= 0.0 || depth > crate::render::MAX_ANIMATION_DEPTH)
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) must have a depth between 0 and {}",
                crate::render::MAX_ANIMATION_DEPTH
            ));
        }
        if let Some(phase) = animation.phase
            && !phase.is_finite()
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) has a non-finite phase"
            ));
        }
    }
    Ok(())
}

/// Format version, identity and spawn point.
fn validate_header(level: &LevelDef) -> Result<(), String> {
    // 1. Format version
    if level.format_version != 1 {
        return Err(format!(
            "Unsupported level format_version: {} (expected 1)",
            level.format_version
        ));
    }

    // 2. Identity
    if level.id.trim().is_empty() {
        return Err("Level 'id' cannot be empty".into());
    }
    if level.name.trim().is_empty() {
        return Err("Level 'name' cannot be empty".into());
    }

    // 3. Spawn point sanity
    if !level.spawn.x.is_finite()
        || !level.spawn.z.is_finite()
        || !level.spawn.yaw_degrees.is_finite()
    {
        return Err("Player spawn coordinates or orientation contain non-finite numbers".into());
    }

    Ok(())
}

/// Per-element count budgets.
fn validate_element_limits(level: &LevelDef) -> Result<(), String> {
    let room_count = level.room_iter().count();
    if room_count > 500 {
        return Err(format!(
            "Level contains too many rooms: {room_count} (limit: 500)"
        ));
    }
    if level.walls.len() > 5000 {
        return Err(format!(
            "Level contains too many walls: {} (limit: 5000)",
            level.walls.len()
        ));
    }
    if level.ceiling_lights.len() > 5000 {
        return Err(format!(
            "Level contains too many ceiling lights: {} (limit: 5000)",
            level.ceiling_lights.len()
        ));
    }
    if level.props.len() > 5000 {
        return Err(format!(
            "Level contains too many props: {} (limit: 5000)",
            level.props.len()
        ));
    }
    if u64::try_from(level.decals.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_DECALS {
        return Err(format!(
            "Level contains too many decals: {} (limit: {})",
            level.decals.len(),
            crate::level::MAX_LEVEL_DECALS
        ));
    }
    if u64::try_from(level.floor_patches.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_FLOOR_PATCHES
    {
        return Err(format!(
            "Level contains too many floor patches: {} (limit: {})",
            level.floor_patches.len(),
            crate::level::MAX_LEVEL_FLOOR_PATCHES
        ));
    }
    Ok(())
}

/// Per-room dimensions, floor elevation and ceiling profile.
fn validate_rooms(level: &LevelDef) -> Result<(), String> {
    for (i, r) in level.room_iter().enumerate() {
        if !r.width.is_finite() || !r.depth.is_finite() || !r.height.is_finite() {
            return Err(format!("Room {i} dimensions must be finite numbers"));
        }
        if r.width <= 0.0 || r.depth <= 0.0 || r.height <= 0.0 {
            return Err(format!(
                "Room {i} width, depth, and height must be positive"
            ));
        }
        if r.width > 2000.0 || r.depth > 2000.0 || r.height > 50.0 {
            return Err(format!(
                "Room {i} dimensions exceed maximum limits (max 2000x2000x50m)"
            ));
        }
        // Vertical geometry: the room's floor elevation and ceiling profile.
        // A non-finite elevation would push every derived surface out of the
        // world, and a ridge at or below the eave is not a gable at all.
        if !r.floor_y.is_finite() || !(r.floor_y + r.height).is_finite() {
            return Err(format!("Room {i} floor elevation must be a finite number"));
        }
        if let crate::level::CeilingProfileDef::Gable { ridge_rise, .. } = r.ceiling {
            if !ridge_rise.is_finite() {
                return Err(format!(
                    "Room {i} ceiling ridge rise must be a finite number"
                ));
            }
            if ridge_rise <= 0.0 {
                return Err(format!(
                    "Room {i} ceiling ridge rise must be above the eave (got {ridge_rise} m)"
                ));
            }
            if ridge_rise > 50.0 {
                return Err(format!(
                    "Room {i} ceiling ridge rise exceeds the maximum limit (max 50 m)"
                ));
            }
            if !(r.floor_y + r.height + ridge_rise).is_finite() {
                return Err(format!(
                    "Room {i} ceiling ridge height must be a finite number"
                ));
            }
        }
    }
    Ok(())
}

/// Every per-surface `shine` override a level authors must be a unit value.
///
/// Shine is the author-facing glossiness (`0.0` matte .. `1.0` extremely
/// glossy). A malformed value is a level error rather than a silent clamp: a
/// surface that was meant to be matte and renders glossy (or the reverse) is
/// exactly the kind of mistake the loader exists to surface. A level that
/// authors no shine passes unchanged.
fn validate_surface_shine(level: &LevelDef) -> Result<(), String> {
    let check = |label: &str, shine: Option<f32>| -> Result<(), String> {
        let Some(shine) = shine else {
            return Ok(());
        };
        if !shine.is_finite() || !(0.0..=1.0).contains(&shine) {
            return Err(format!(
                "{label} shine must be a finite number between 0.0 and 1.0, found {shine:?}"
            ));
        }
        Ok(())
    };
    check("Level default wall", level.defaults.wall_shine)?;
    check("Level default floor", level.defaults.floor_shine)?;
    check("Level default ceiling", level.defaults.ceiling_shine)?;
    for (i, room) in level.room_iter().enumerate() {
        check(&format!("Room {i} floor"), room.shine)?;
        check(&format!("Room {i} ceiling"), room.ceiling_shine)?;
    }
    for (i, wall) in level.walls.iter().enumerate() {
        check(&format!("Wall {i}"), wall.shine)?;
        for (face, shine) in &wall.face_shine {
            check(&format!("Wall {i} face `{face}`"), Some(*shine))?;
        }
        for (j, opening) in wall.openings.iter().enumerate() {
            check(&format!("Wall {i} opening {j} glass"), opening.glass_shine)?;
        }
    }
    for (i, patch) in level.floor_patches.iter().enumerate() {
        check(&format!("Floor patch {i}"), patch.shine)?;
    }
    for (i, region) in level.floor_regions.iter().enumerate() {
        check(&format!("Floor region {i} floor"), region.shine)?;
        check(&format!("Floor region {i} edge"), region.edge_shine)?;
    }
    Ok(())
}

/// Local floor regions: position, size, materials and room containment.
fn validate_floor_regions(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.floor_regions.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_FLOOR_REGIONS
    {
        return Err(format!(
            "Level contains too many floor regions: {} (limit: {})",
            level.floor_regions.len(),
            crate::level::MAX_LEVEL_FLOOR_REGIONS
        ));
    }
    for (i, region) in level.floor_regions.iter().enumerate() {
        if !region.x.is_finite()
            || !region.z.is_finite()
            || !region.width.is_finite()
            || !region.depth.is_finite()
            || !region.offset_y.is_finite()
        {
            return Err(format!(
                "Floor region {i} position, size, and offset must be finite numbers"
            ));
        }
        if region.width <= 0.0 || region.depth <= 0.0 {
            return Err(format!("Floor region {i} width and depth must be positive"));
        }
        if region
            .material
            .as_deref()
            .is_some_and(|material| material.trim().is_empty())
            || region
                .edge_material
                .as_deref()
                .is_some_and(|material| material.trim().is_empty())
        {
            return Err(format!(
                "Floor region {i} materials must be non-empty ids when specified"
            ));
        }

        // A region has to describe a floor inside a room: one that overlaps
        // nothing is a typo, and one whose surface is at or above the room's
        // ceiling has no interior volume to stand in.
        let (rx0, rx1, rz0, rz1) = region.bounds();
        let mut overlaps_room = false;
        for (room_index, room) in level.room_iter().enumerate() {
            let (x0, x1, z0, z1) = room.bounds();
            if rx1 <= x0 || rx0 >= x1 || rz1 <= z0 || rz0 >= z1 {
                continue;
            }
            overlaps_room = true;
            let floor = room.floor_y + region.offset();
            if !floor.is_finite() || floor >= room.eave_y() {
                return Err(format!(
                    "Floor region {i} sits at or above the ceiling of room {room_index} \
                     ({floor:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
        }
        if !overlaps_room {
            return Err(format!("Floor region {i} lies outside every room section"));
        }
    }
    Ok(())
}

/// Rectangular water volumes: position, size, surface, material and depth.
///
/// A volume whose surface sits at or below the floor beneath it is a typo the
/// author has to see (the water would be hidden inside the geometry), so the
/// walkable floor is sampled inside the footprint and compared against the
/// authored surface. A volume that overlaps no room is rejected like a floor
/// region that overlaps none.
fn validate_water(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.water.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_WATER_VOLUMES
    {
        return Err(format!(
            "Level contains too many water volumes: {} (limit: {})",
            level.water.len(),
            crate::level::MAX_LEVEL_WATER_VOLUMES
        ));
    }
    let surfaces = crate::level::LevelSurfaces::new(level);
    for (i, volume) in level.water.iter().enumerate() {
        if !volume.x.is_finite()
            || !volume.z.is_finite()
            || !volume.width.is_finite()
            || !volume.depth.is_finite()
            || !volume.surface_y.is_finite()
        {
            return Err(format!(
                "Water volume {i} position, size, and surface must be finite numbers"
            ));
        }
        if volume.width <= 0.0 || volume.depth <= 0.0 {
            return Err(format!("Water volume {i} width and depth must be positive"));
        }
        if volume
            .material
            .as_deref()
            .is_some_and(|material| material.trim().is_empty())
        {
            return Err(format!(
                "Water volume {i} material must be a non-empty id when specified"
            ));
        }
        if let Some(opacity) = volume.opacity
            && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
        {
            return Err(format!(
                "Water volume {i} opacity must be a finite number between 0.0 and 1.0"
            ));
        }
        if let Some(bottom) = volume.bottom_y
            && (!bottom.is_finite() || bottom >= volume.surface_y)
        {
            return Err(format!(
                "Water volume {i} bottom_y must be finite and below its surface_y"
            ));
        }

        let (x0, x1, z0, z1) = volume.bounds();
        let mut overlaps_room = false;
        let mut samples: [(f32, f32); 5] = [
            (f32::midpoint(x0, x1), f32::midpoint(z0, z1)),
            (x0, z0),
            (x1, z0),
            (x1, z1),
            (x0, z1),
        ];
        // Inset the corner samples so a volume that shares an edge with a room
        // boundary is not rejected by floating-point noise on the seam.
        for (x, z) in &mut samples {
            *x = x.clamp(x0 + 1.0e-3, x1 - 1.0e-3);
            *z = z.clamp(z0 + 1.0e-3, z1 - 1.0e-3);
        }
        for (x, z) in samples {
            let Some(floor) = surfaces.floor_y_at(x, z) else {
                continue;
            };
            overlaps_room = true;
            if floor > volume.surface_y + 1.0e-2 {
                return Err(format!(
                    "Water volume {i} surface ({:.2} m) is below the floor at ({x:.2}, {z:.2}) \
                     ({floor:.2} m); raise surface_y above the floor it covers",
                    volume.surface_y
                ));
            }
        }
        if !overlaps_room {
            return Err(format!("Water volume {i} lies outside every room section"));
        }
    }
    Ok(())
}

/// The generic architectural pieces: ramps, staircases, half walls, columns,
/// archways, guardrails, thresholds and baseboards.
///
/// Every piece is validated on the same contract its geometry is built from:
/// finite dimensions, materials that are non-empty when authored, an overlap
/// with a room where the piece is a walking surface, and — for ramps and
/// staircases — a slope or riser the player controller can actually climb.
/// Invalid dimensions are named errors, never silently clamped geometry.
fn validate_architecture(level: &LevelDef) -> Result<(), String> {
    validate_ramps(level)?;
    validate_stairs(level)?;
    validate_half_walls(level)?;
    validate_columns(level)?;
    validate_archways(level)?;
    validate_guardrails(level)?;
    validate_thresholds(level)?;
    validate_baseboards(level)?;
    validate_architecture_overlaps(level)
}

/// True when an axis-aligned rectangle overlaps any room's footprint.
fn rect_overlaps_room(level: &LevelDef, bounds: (f32, f32, f32, f32)) -> bool {
    let (x0, x1, z0, z1) = bounds;
    level.room_iter().any(|room| {
        let (rx0, rx1, rz0, rz1) = room.bounds();
        x1 > rx0 && x0 < rx1 && z1 > rz0 && z0 < rz1
    })
}

/// True when two axis-aligned rectangles overlap by a real area.
fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.1 > b.0 && a.0 < b.1 && a.3 > b.2 && a.2 < b.3
}

/// True when an optional material id is present but blank.
fn blank_material(material: Option<&str>) -> bool {
    material.is_some_and(|id| id.trim().is_empty())
}

/// Ramps: a walkable slope inside a room, shallow enough to climb.
fn validate_ramps(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.ramps.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_RAMPS {
        return Err(format!(
            "Level contains too many ramps: {} (limit: {})",
            level.ramps.len(),
            crate::level::MAX_LEVEL_RAMPS
        ));
    }
    for (i, ramp) in level.ramps.iter().enumerate() {
        if !ramp.x.is_finite()
            || !ramp.z.is_finite()
            || !ramp.width.is_finite()
            || !ramp.depth.is_finite()
            || !ramp.offset_y.is_finite()
            || !ramp.rise.is_finite()
        {
            return Err(format!(
                "Ramp {i} position, size, offset and rise must be finite numbers"
            ));
        }
        if ramp.width <= 0.0 || ramp.depth <= 0.0 {
            return Err(format!("Ramp {i} width and depth must be positive"));
        }
        if ramp.rise.abs() <= 1e-3 {
            return Err(format!(
                "Ramp {i} has no rise; use a floor region for a flat material change"
            ));
        }
        if ramp.rise.abs() > crate::level::MAX_RAMP_RISE_M {
            return Err(format!(
                "Ramp {i} rise exceeds the maximum of {} m",
                crate::level::MAX_RAMP_RISE_M
            ));
        }
        let length = ramp.length();
        if ramp.rise.abs() > crate::level::MAX_RAMP_SLOPE * length {
            return Err(format!(
                "Ramp {i} is too steep to walk: {:.2} m of rise over {:.2} m of run \
                 (limit {} m per metre)",
                ramp.rise.abs(),
                length,
                crate::level::MAX_RAMP_SLOPE
            ));
        }
        if blank_material(ramp.material.as_deref()) || blank_material(ramp.edge_material.as_deref())
        {
            return Err(format!(
                "Ramp {i} materials must be non-empty ids when specified"
            ));
        }
        if !rect_overlaps_room(level, ramp.bounds()) {
            return Err(format!("Ramp {i} lies outside every room section"));
        }
        // The ramp's high end has to stay under the ceiling of every room it
        // crosses, or the walking surface would pass through the ceiling. Every
        // room it crosses must also share one floor plane: the mesh and the
        // lightmap are generated once, from the room under the ramp's centre,
        // while the walkable surface resolves each room's own floor.
        let mut ramp_floor: Option<f32> = None;
        for (room_index, room) in level.room_iter().enumerate() {
            if !rects_overlap(ramp.bounds(), room.bounds()) {
                continue;
            }
            let high = room.floor_y + ramp.high_offset();
            if !high.is_finite() || high >= room.eave_y() {
                return Err(format!(
                    "Ramp {i} rises to or above the ceiling of room {room_index} \
                     ({high:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
            match ramp_floor {
                None => ramp_floor = Some(room.floor_y),
                Some(floor) if (floor - room.floor_y).abs() > 1.0e-4 => {
                    return Err(format!(
                        "Ramp {i} spans rooms with different floors ({floor:.2} m vs \
                         {:.2} m in room {room_index}); the ramp is drawn on one floor \
                         plane, so every room it crosses must share it",
                        room.floor_y
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Staircases: climeable risers, usable treads, and a top tread that stays
/// under the ceiling.
fn validate_stairs(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.stairs.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_STAIRS {
        return Err(format!(
            "Level contains too many staircases: {} (limit: {})",
            level.stairs.len(),
            crate::level::MAX_LEVEL_STAIRS
        ));
    }
    for (i, stair) in level.stairs.iter().enumerate() {
        if !stair.x.is_finite()
            || !stair.z.is_finite()
            || !stair.width.is_finite()
            || !stair.depth.is_finite()
            || !stair.offset_y.is_finite()
            || !stair.rise.is_finite()
        {
            return Err(format!(
                "Staircase {i} position, size, offset and rise must be finite numbers"
            ));
        }
        if stair.width <= 0.0 || stair.depth <= 0.0 {
            return Err(format!("Staircase {i} width and depth must be positive"));
        }
        if stair.step_count() < 2 {
            return Err(format!(
                "Staircase {i} needs at least 2 steps (found {})",
                stair.step_count()
            ));
        }
        if stair.rise() <= 0.0 {
            return Err(format!("Staircase {i} rise must be positive"));
        }
        if stair.rise() > crate::level::MAX_RAMP_RISE_M {
            return Err(format!(
                "Staircase {i} rise exceeds the maximum of {} m",
                crate::level::MAX_RAMP_RISE_M
            ));
        }
        let riser = stair.riser_height();
        if riser > crate::level::MAX_STAIR_RISER_M + 1e-4 {
            return Err(format!(
                "Staircase {i} riser is {riser:.2} m, taller than the {:.2} m walkable step; \
                 add steps or reduce the rise",
                crate::level::MAX_STAIR_RISER_M
            ));
        }
        let tread = stair.tread_depth();
        if tread < crate::level::MIN_STAIR_TREAD_M {
            return Err(format!(
                "Staircase {i} tread is {tread:.2} m, shallower than the {} m minimum",
                crate::level::MIN_STAIR_TREAD_M
            ));
        }
        if blank_material(stair.material.as_deref())
            || blank_material(stair.riser_material.as_deref())
            || blank_material(stair.side_material.as_deref())
        {
            return Err(format!(
                "Staircase {i} materials must be non-empty ids when specified"
            ));
        }
        if !rect_overlaps_room(level, stair.bounds()) {
            return Err(format!("Staircase {i} lies outside every room section"));
        }
        // Every room the flight crosses must share one floor plane: the mesh is
        // generated once from the room under the flight's centre, while the
        // walkable surface resolves each room's own floor.
        let mut stair_floor: Option<f32> = None;
        for (room_index, room) in level.room_iter().enumerate() {
            if !rects_overlap(stair.bounds(), room.bounds()) {
                continue;
            }
            let top = room.floor_y + stair.top_offset();
            if !top.is_finite() || top >= room.eave_y() {
                return Err(format!(
                    "Staircase {i} climbs to or above the ceiling of room {room_index} \
                     ({top:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
            match stair_floor {
                None => stair_floor = Some(room.floor_y),
                Some(floor) if (floor - room.floor_y).abs() > 1.0e-4 => {
                    return Err(format!(
                        "Staircase {i} spans rooms with different floors ({floor:.2} m vs \
                         {:.2} m in room {room_index}); the flight is drawn on one floor \
                         plane, so every room it crosses must share it",
                        room.floor_y
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Half walls: an authored height and usable footprint.
fn validate_half_walls(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.half_walls.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_HALF_WALLS
    {
        return Err(format!(
            "Level contains too many half walls: {} (limit: {})",
            level.half_walls.len(),
            crate::level::MAX_LEVEL_HALF_WALLS
        ));
    }
    for (i, piece) in level.half_walls.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
            || !piece.height.is_finite()
        {
            return Err(format!(
                "Half wall {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 || piece.height <= 0.0 {
            return Err(format!(
                "Half wall {i} width, depth and height must be positive"
            ));
        }
        if piece.height > 50.0 {
            return Err(format!("Half wall {i} height exceeds the maximum of 50 m"));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Half wall {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.end_material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Half wall {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Columns: a solid post with an optional authored height.
fn validate_columns(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.columns.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_COLUMNS {
        return Err(format!(
            "Level contains too many columns: {} (limit: {})",
            level.columns.len(),
            crate::level::MAX_LEVEL_COLUMNS
        ));
    }
    for (i, piece) in level.columns.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
        {
            return Err(format!(
                "Column {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 {
            return Err(format!("Column {i} width and depth must be positive"));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Column {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Column {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Column {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Archways: an opening that fits inside its block, with a crown above the
/// springing line.
fn validate_archways(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.archways.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_ARCHWAYS {
        return Err(format!(
            "Level contains too many archways: {} (limit: {})",
            level.archways.len(),
            crate::level::MAX_LEVEL_ARCHWAYS
        ));
    }
    for (i, piece) in level.archways.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
            || !piece.height.is_finite()
            || !piece.opening_width.is_finite()
            || !piece.opening_height.is_finite()
            || !piece.arch_rise.is_finite()
        {
            return Err(format!(
                "Archway {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 || piece.height <= 0.0 {
            return Err(format!(
                "Archway {i} width, depth and height must be positive"
            ));
        }
        if piece.opening_width <= 0.0 || piece.opening_height <= 0.0 {
            return Err(format!(
                "Archway {i} opening width and height must be positive"
            ));
        }
        if piece.arch_rise < 0.0 {
            return Err(format!("Archway {i} arch rise cannot be negative"));
        }
        if piece.arch_rise >= piece.opening_height {
            return Err(format!(
                "Archway {i} arch rise ({:.2} m) must be lower than its opening height \
                 ({:.2} m); a flat lintel is `arch_rise: 0`",
                piece.arch_rise, piece.opening_height
            ));
        }
        if piece.height < piece.opening_height {
            return Err(format!(
                "Archway {i} block is shorter than its opening ({:.2} m vs {:.2} m)",
                piece.height, piece.opening_height
            ));
        }
        if piece.height > 50.0 {
            return Err(format!("Archway {i} height exceeds the maximum of 50 m"));
        }
        let length = piece.length();
        let minimum_pier = crate::level::ARCHWAY_MIN_PIER_M;
        if piece.opening_width > (-2.0f32).mul_add(minimum_pier, length) {
            return Err(format!(
                "Archway {i} opening is too wide for its block: {:.2} m opening in a {:.2} m \
                 block (each pier needs at least {:.2} m)",
                piece.opening_width, length, minimum_pier
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Archway {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.reveal_material.as_deref())
        {
            return Err(format!(
                "Archway {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Guardrails: a sane rail height, post spacing and slope.
fn validate_guardrails(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.guardrails.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_GUARDRAILS
    {
        return Err(format!(
            "Level contains too many guardrails: {} (limit: {})",
            level.guardrails.len(),
            crate::level::MAX_LEVEL_GUARDRAILS
        ));
    }
    for (i, rail) in level.guardrails.iter().enumerate() {
        if !rail.x.is_finite()
            || !rail.z.is_finite()
            || !rail.length.is_finite()
            || !rail.rotation_degrees.is_finite()
            || !rail.height.is_finite()
            || rail.rise.is_some_and(|rise| !rise.is_finite())
            || !rail.post_spacing.is_finite()
        {
            return Err(format!(
                "Guardrail {i} position and dimensions must be finite numbers"
            ));
        }
        if rail.length <= 0.0 {
            return Err(format!("Guardrail {i} length must be positive"));
        }
        if !(0.2..=2.0).contains(&rail.height) {
            return Err(format!(
                "Guardrail {i} height must be between 0.2 and 2.0 m (got {:.2} m)",
                rail.height
            ));
        }
        if !(0.2..=3.0).contains(&rail.post_spacing) {
            return Err(format!(
                "Guardrail {i} post spacing must be between 0.2 and 3.0 m (got {:.2} m)",
                rail.post_spacing
            ));
        }
        if rail.rise().abs() > crate::level::MAX_RAMP_SLOPE * rail.length {
            return Err(format!(
                "Guardrail {i} slopes too steeply: {:.2} m of rise over {:.2} m of run",
                rail.rise().abs(),
                rail.length
            ));
        }
        if rail.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Guardrail {i} base height must be a finite number"));
        }
        if blank_material(rail.material.as_deref()) || blank_material(rail.post_material.as_deref())
        {
            return Err(format!(
                "Guardrail {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Threshold strips: a small, floor-hugging trim piece over a level floor.
fn validate_thresholds(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.thresholds.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_THRESHOLDS
    {
        return Err(format!(
            "Level contains too many thresholds: {} (limit: {})",
            level.thresholds.len(),
            crate::level::MAX_LEVEL_THRESHOLDS
        ));
    }
    let surfaces = crate::level::LevelSurfaces::new(level);
    for (i, strip) in level.thresholds.iter().enumerate() {
        if !strip.x.is_finite()
            || !strip.z.is_finite()
            || !strip.length.is_finite()
            || !strip.thickness.is_finite()
            || !strip.height.is_finite()
            || !strip.rotation_degrees.is_finite()
        {
            return Err(format!(
                "Threshold {i} position and dimensions must be finite numbers"
            ));
        }
        if strip.length <= 0.0 {
            return Err(format!("Threshold {i} length must be positive"));
        }
        if !(0.02..=0.5).contains(&strip.thickness) {
            return Err(format!(
                "Threshold {i} thickness must be between 0.02 and 0.5 m (got {:.3} m)",
                strip.thickness
            ));
        }
        if !(0.002..=0.05).contains(&strip.height) {
            return Err(format!(
                "Threshold {i} height must be between 0.002 and 0.05 m (got {:.3} m)",
                strip.height
            ));
        }
        if strip.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Threshold {i} base height must be a finite number"));
        }
        if blank_material(strip.material.as_deref()) {
            return Err(format!(
                "Threshold {i} materials must be non-empty ids when specified"
            ));
        }
        // The strip sits on a floor: it must resolve one, and its ends must
        // stand at (essentially) the same height, or half of it floats.
        let Some(centre) = surfaces.floor_y_at(strip.x, strip.z) else {
            return Err(format!("Threshold {i} lies outside every room section"));
        };
        let half_length = strip.length * 0.5;
        let half_thickness = strip.thickness() * 0.5;
        let mut low = centre;
        let mut high = centre;
        for (along, across) in [
            (-half_length, -half_thickness),
            (half_length, -half_thickness),
            (-half_length, half_thickness),
            (half_length, half_thickness),
        ] {
            let (px, pz) = strip.point_at_offset(along, across);
            let Some(y) = surfaces.floor_y_at(px, pz) else {
                return Err(format!(
                    "Threshold {i} spans the point ({px:.2}, {pz:.2}) outside every room section"
                ));
            };
            low = low.min(y);
            high = high.max(y);
        }
        if high - low > 0.05 {
            return Err(format!(
                "Threshold {i} spans a floor height change of {:.2} m; threshold strips \
                 belong on a level floor",
                high - low
            ));
        }
        // A strip buried in a wall's solid (not in one of its openings) is
        // invisible, so it is rejected like a buried baseboard.
        let mid_y = strip.height().mul_add(0.5, centre);
        if let Some(wall) = point_buried_in_wall(level, strip.x, strip.z, mid_y) {
            return Err(format!(
                "Threshold {i} is buried inside wall {wall}: place the strip in the \
                 opening it crosses, not inside the wall solid"
            ));
        }
    }
    Ok(())
}

/// Baseboards: a thin trim run with usable proportions, placed where its front
/// face can actually be seen.
fn validate_baseboards(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.baseboards.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_BASEBOARDS
    {
        return Err(format!(
            "Level contains too many baseboards: {} (limit: {})",
            level.baseboards.len(),
            crate::level::MAX_LEVEL_BASEBOARDS
        ));
    }
    for (i, board) in level.baseboards.iter().enumerate() {
        if !board.x.is_finite()
            || !board.z.is_finite()
            || !board.length.is_finite()
            || !board.rotation_degrees.is_finite()
            || !board.height.is_finite()
            || !board.thickness.is_finite()
        {
            return Err(format!(
                "Baseboard {i} position and dimensions must be finite numbers"
            ));
        }
        if board.length <= 0.0 {
            return Err(format!("Baseboard {i} length must be positive"));
        }
        if !(0.01..=1.0).contains(&board.height) {
            return Err(format!(
                "Baseboard {i} height must be between 0.01 and 1.0 m (got {:.3} m)",
                board.height
            ));
        }
        if !(0.004..=0.2).contains(&board.thickness) {
            return Err(format!(
                "Baseboard {i} thickness must be between 0.004 and 0.2 m (got {:.3} m)",
                board.thickness
            ));
        }
        if board.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Baseboard {i} base height must be a finite number"));
        }
        if blank_material(board.material.as_deref()) {
            return Err(format!(
                "Baseboard {i} materials must be non-empty ids when specified"
            ));
        }
        // A board whose whole cross-section lies inside a wall is invisible.
        // The room boundary is the *centre* of the wall that straddles it, so
        // the natural "place the run at x = 0" lands the board inside the wall;
        // the run's back plane belongs on the wall's inner face.
        let (mx, mz) = board.point_at(0.5, board.thickness() * 0.5);
        let mid_y = board
            .height()
            .mul_add(0.5, board.base_y(&LevelSurfaces::new(level)));
        if let Some(wall) = point_buried_in_wall(level, mx, mz, mid_y) {
            return Err(format!(
                "Baseboard {i} is buried inside wall {wall}: place the run so its back \
                 plane lies on the wall's face (the room edge is the wall's centre plane)"
            ));
        }
    }
    Ok(())
}

/// The index of a wall whose solid contains `(x, z, y)`, openings respected.
///
/// A point exactly on a wall face is *not* inside: every wall boundary is
/// exclusive by [`WALL_SLICE_EPS`], so a surface mounted flush on the face
/// counts as visible. Openings are cut first, so trim floating in a doorway is
/// left alone: it is visible through the hole.
fn point_buried_in_wall(level: &LevelDef, x: f32, z: f32, y: f32) -> Option<usize> {
    if !x.is_finite() || !z.is_finite() || !y.is_finite() {
        return None;
    }
    let surfaces = LevelSurfaces::new(level);
    for (index, wall) in level.walls.iter().enumerate() {
        let (x0, x1) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (z0, z1) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        if x <= x0 + WALL_SLICE_EPS
            || x >= x1 - WALL_SLICE_EPS
            || z <= z0 + WALL_SLICE_EPS
            || z >= z1 - WALL_SLICE_EPS
        {
            continue;
        }
        let breaks = surfaces.wall_profile_breaks(wall);
        let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
        let (origin_x, origin_z) = wall.length_origin();
        let offset = match wall.axis() {
            WallAxis::X => x - origin_x,
            WallAxis::Z => z - origin_z,
        };
        for slice in wall_solid_slices_profiled(wall, clear, &breaks) {
            if offset > slice.start + WALL_SLICE_EPS
                && offset < slice.end - WALL_SLICE_EPS
                && y > slice.bottom + WALL_SLICE_EPS
                && y < slice.top - WALL_SLICE_EPS
            {
                return Some(index);
            }
        }
    }
    None
}

/// Cross-piece checks that no single piece can answer on its own: two walking
/// surfaces may not overlap, and a floor region may not be authored inside a
/// ramp or staircase.
fn validate_architecture_overlaps(level: &LevelDef) -> Result<(), String> {
    for (ri, ramp) in level.ramps.iter().enumerate() {
        for (si, stair) in level.stairs.iter().enumerate() {
            if rects_overlap(ramp.bounds(), stair.bounds()) {
                return Err(format!(
                    "Ramp {ri} overlaps staircase {si}; a space has one walking surface"
                ));
            }
        }
        for (fi, region) in level.floor_regions.iter().enumerate() {
            if rects_overlap(ramp.bounds(), region.bounds()) {
                return Err(format!(
                    "Floor region {fi} overlaps ramp {ri}; a ramp is a floor surface itself \
                     and the two cannot share a footprint"
                ));
            }
        }
    }
    for (si, stair) in level.stairs.iter().enumerate() {
        for (fi, region) in level.floor_regions.iter().enumerate() {
            if rects_overlap(stair.bounds(), region.bounds()) {
                return Err(format!(
                    "Floor region {fi} overlaps staircase {si}; a staircase is a floor surface \
                     itself and the two cannot share a footprint"
                ));
            }
        }
    }
    Ok(())
}

/// Wall dimensions and opening cutouts.
fn validate_walls(level: &LevelDef) -> Result<(), String> {
    for (i, w) in level.walls.iter().enumerate() {
        if !w.x.is_finite()
            || !w.y.is_finite()
            || !w.z.is_finite()
            || !w.width.is_finite()
            || !w.depth.is_finite()
        {
            return Err(format!("Wall {i} dimensions must be finite numbers"));
        }
        if let Some(h) = w.height {
            if !h.is_finite() {
                return Err(format!("Wall {i} dimensions must be finite numbers"));
            }
            if h <= 0.0 {
                return Err(format!(
                    "Wall {i} width, depth, and height must be positive"
                ));
            }
        }
        if w.width <= 0.0 || w.depth <= 0.0 {
            return Err(format!(
                "Wall {i} width, depth, and height must be positive"
            ));
        }

        // Openings are optional cutouts; openings that do not overlap the
        // wall's vertical range are allowed (they simply produce no cut).
        if w.openings.len() > crate::level::MAX_WALL_OPENINGS {
            return Err(format!(
                "Wall {i} has too many openings: {} (limit: {})",
                w.openings.len(),
                crate::level::MAX_WALL_OPENINGS
            ));
        }
        for (j, opening) in w.openings.iter().enumerate() {
            if !opening.offset.is_finite()
                || !opening.width.is_finite()
                || !opening.height.is_finite()
                || !opening.sill.is_finite()
            {
                return Err(format!("Wall {i} opening {j} contains non-finite numbers"));
            }
            if opening.width <= 0.0 || opening.height <= 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} must have a positive width and height"
                ));
            }
            if opening.sill < 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} cannot have a negative sill height"
                ));
            }
            if opening.offset < 0.0 {
                return Err(format!("Wall {i} opening {j} starts before the wall"));
            }
            if opening.end() > w.length() + 1e-3 {
                let prefix = if opening.is_door() {
                    "Door opening"
                } else if opening.kind == "window" {
                    "Window opening"
                } else {
                    "Opening"
                };
                return Err(format!(
                    "{prefix} extends beyond this wall (wall {i}: opening ends at {:.2} m, wall is {:.2} m long)",
                    opening.end(),
                    w.length()
                ));
            }
        }
    }
    Ok(())
}

/// Ceiling/wall fixture position, intensity, colour and mounting height.
fn validate_ceiling_lights(level: &LevelDef) -> Result<(), String> {
    for (i, light) in level.ceiling_lights.iter().enumerate() {
        if !light.x.is_finite() || !light.z.is_finite() || !light.rotation_degrees.is_finite() {
            return Err(format!(
                "Ceiling light {i} position and rotation must be finite numbers"
            ));
        }
        // The optional fixture intensity (`brightness`, also accepted as
        // `intensity`). Omitted means the standard 1.0 fixture; negative or
        // non-finite values are rejected, high values are clamped while baking.
        if let Some(brightness) = light.brightness {
            if !brightness.is_finite() {
                return Err(format!(
                    "Ceiling light {i} intensity must be a finite number"
                ));
            }
            if brightness < 0.0 {
                return Err(format!("Ceiling light {i} intensity cannot be negative"));
            }
        }
        // The optional emitted colour. Omitted means the standard warm
        // fixture; channels outside `[0, 1]` or non-finite values are
        // malformed data, exactly like a negative intensity.
        if let Some(color) = light.color
            && !color.is_valid()
        {
            return Err(format!(
                "Ceiling light {i} colour channels must be finite numbers between 0 and {}",
                crate::lighting::MAX_LIGHT_COLOR
            ));
        }
        // A wall fixture is authored at its own world height. A ceiling
        // fixture normally derives its height from the ceiling, but may author
        // a world `y` to mount at a chosen height (a stacked building uses it
        // to pick a storey); either way an authored height must be finite.
        if light.mount == crate::level::LightMount::Wall {
            match light.y {
                Some(y) if y.is_finite() => {}
                Some(_) => {
                    return Err(format!(
                        "Wall light {i} height (`y`) must be a finite number"
                    ));
                }
                None => {
                    return Err(format!(
                        "Wall light {i} needs a world height (`y`); a wall fixture cannot \
                         derive one from the ceiling"
                    ));
                }
            }
        }
        if let Some(y) = light.y
            && !y.is_finite()
        {
            return Err(format!(
                "Ceiling light {i} height (`y`) must be a finite number when authored"
            ));
        }
        // Optional range/falloff: a fixture may shape its own pool, but
        // malformed numbers are rejected rather than silently clamped, exactly
        // like an intensity.
        if let Some(range) = light.range
            && !(range.is_finite() && range > 0.0)
        {
            return Err(format!(
                "Ceiling light {i} range must be a positive finite number of metres"
            ));
        }
        // The optional independent emissive strength of the visible face.
        if let Some(emission) = light.emission
            && !(emission.is_finite() && emission >= 0.0)
        {
            return Err(format!(
                "Ceiling light {i} emission must be a finite number that is not negative"
            ));
        }
    }
    Ok(())
}

/// Validate the generic light sources a placed object owns.
///
/// These are the engine-level lights of [`crate::lighting::LightSource`]: a
/// shape, a local offset, a colour and a pool. A malformed light is a level
/// error — unlike an unknown prop model, which degrades to a placeholder box —
/// because a light is authored data the engine can check completely.
fn validate_prop_lights(level: &LevelDef) -> Result<(), String> {
    for (i, prop) in level.props.iter().enumerate() {
        if prop.lights.len() > crate::level::MAX_PROP_LIGHTS {
            return Err(format!(
                "Prop {i} declares {} attached lights; the limit is {}",
                prop.lights.len(),
                crate::level::MAX_PROP_LIGHTS
            ));
        }
        for (j, light) in prop.lights.iter().enumerate() {
            if !light.offset.iter().all(|value| value.is_finite())
                || !light.rotation_degrees.is_finite()
            {
                return Err(format!(
                    "Prop {i} light {j} offset and rotation must be finite numbers"
                ));
            }
            if let Some(intensity) = light.intensity
                && !(intensity.is_finite() && intensity >= 0.0)
            {
                return Err(format!(
                    "Prop {i} light {j} intensity must be a finite number that is not negative"
                ));
            }
            if let Some(color) = light.color
                && !color.is_valid()
            {
                return Err(format!(
                    "Prop {i} light {j} colour channels must be finite numbers between 0 and {}",
                    crate::lighting::MAX_LIGHT_COLOR
                ));
            }
            if let Some(range) = light.range
                && !(range.is_finite() && range > 0.0)
            {
                return Err(format!(
                    "Prop {i} light {j} range must be a positive finite number of metres"
                ));
            }
            if !light.shape().is_valid() {
                return Err(format!(
                    "Prop {i} light {j} has malformed {} dimensions; every extent must be \
                     finite, positive and within the engine caps",
                    light.shape().name()
                ));
            }
        }
    }
    Ok(())
}

/// Placed prop ids, transforms and sizes.
fn validate_props(level: &LevelDef) -> Result<(), String> {
    for (i, prop) in level.props.iter().enumerate() {
        if prop.model.trim().is_empty() {
            return Err(format!("Prop {i} must reference a non-empty model id"));
        }
        if !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.scale.is_finite()
        {
            return Err(format!(
                "Prop {i} position, rotation, and scale must be finite numbers"
            ));
        }
        if prop.scale <= 0.0 {
            return Err(format!("Prop {i} scale must be positive"));
        }
        if let Some(size) = prop.size
            && !size.iter().all(|v| v.is_finite() && *v > 0.0)
        {
            return Err(format!(
                "Prop {i} size must contain positive finite numbers"
            ));
        }
    }
    Ok(())
}

/// Decal transforms, sizes and material ids.
fn validate_decals(level: &LevelDef) -> Result<(), String> {
    for (i, decal) in level.decals.iter().enumerate() {
        if !decal.x.is_finite()
            || !decal.y.is_finite()
            || !decal.z.is_finite()
            || !decal.rotation_degrees.is_finite()
        {
            return Err(format!(
                "Decal {i} position and rotation must be finite numbers"
            ));
        }
        if !decal.width.is_finite() || !decal.height.is_finite() {
            return Err(format!("Decal {i} size must be finite numbers"));
        }
        if decal.width <= 0.0 || decal.height <= 0.0 {
            return Err(format!("Decal {i} width and height must be positive"));
        }
        if decal.width > crate::level::MAX_DECAL_SIZE_M
            || decal.height > crate::level::MAX_DECAL_SIZE_M
        {
            return Err(format!(
                "Decal {i} is larger than the {} m limit ({} x {})",
                crate::level::MAX_DECAL_SIZE_M,
                decal.width,
                decal.height
            ));
        }
        if decal.material.trim().is_empty() {
            return Err(format!("Decal {i} must reference a non-empty material id"));
        }
    }
    Ok(())
}

/// Decals on horizontal surfaces are snapped to the real surface height, so
/// they follow an elevated room or a recessed region.
///
/// A gable ceiling is a sloped surface and cannot carry a single planar decal,
/// and a decal whose footprint straddles a height change (a recess edge, a room
/// boundary at a different elevation) cannot be projected onto one plane
/// either: both are rejected clearly instead of being drawn at a nonsense
/// height.
fn validate_decal_surfaces(level: &LevelDef) -> Result<(), String> {
    let surfaces = crate::level::LevelSurfaces::new(level);
    for (i, decal) in level.decals.iter().enumerate() {
        if decal.surface.is_ceiling() && !surfaces.ceiling_is_flat_at(decal.x, decal.z) {
            return Err(format!(
                "Ceiling decal {i} targets a gable ceiling; sloped ceiling decals are not supported"
            ));
        }
        if !decal.surface.is_horizontal() {
            continue;
        }
        let Some(corners) = crate::render::decal_quad_points(decal) else {
            continue;
        };
        let heights = corners.map(|point| match decal.surface {
            crate::level::DecalSurface::Floor => {
                surfaces.floor_y_at(point[0], point[2]).unwrap_or(point[1])
            }
            crate::level::DecalSurface::Ceiling
            | crate::level::DecalSurface::WallNorth
            | crate::level::DecalSurface::WallSouth
            | crate::level::DecalSurface::WallWest
            | crate::level::DecalSurface::WallEast => surfaces.ceiling_y_at(point[0], point[2]),
        });
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(low, high), y| {
            (low.min(*y), high.max(*y))
        });
        if high - low > 0.05 {
            return Err(format!(
                "Decal {i} spans a floor or ceiling height change ({low:.2} m to {high:.2} m); \
                 place it entirely on one surface"
            ));
        }
    }
    Ok(())
}

/// 5. Generated-geometry complexity budget, checked after per-element
///    validation so dimension errors take precedence. This bounds the vertex
///    buffer built at load time, protecting the process from levels that would
///    otherwise exhaust memory. Overlapping/intersecting geometry is explicitly
///    allowed and is not validated here.
fn validate_geometry_budget(level: &LevelDef) -> Result<(), String> {
    let estimate = level.estimate_geometry();
    if estimate.floor_area_m2 > MAX_LEVEL_FLOOR_AREA_M2 {
        return Err(format!(
            "Level floor area is too large: {} m^2 (limit: {MAX_LEVEL_FLOOR_AREA_M2} m^2). \
             Use smaller or fewer room sections.",
            estimate.floor_area_m2
        ));
    }
    if estimate.total_vertices > MAX_LEVEL_VERTICES {
        return Err(format!(
            "Level geometry is too complex: ~{} vertices (limit: {MAX_LEVEL_VERTICES}). \
             Reduce rooms, walls or ceiling lights.",
            estimate.total_vertices
        ));
    }
    Ok(())
}

/// Resolves the visible-face sheets of the fixture families a level places.
///
/// Built-in fixtures resolve the PNG their catalog entry names, exactly like an
/// external decal sheet: the catalog owns the file, this decodes it through the
/// shared session cache and the standard PNG loader. A `pack:` fixture id uses
/// the pack's own sheet for its family instead, so a level pack can still
/// restyle a built-in fixture without touching the catalog.
///
/// The list is indexed by [`crate::lighting::FixtureKind::index`] and holds at
/// most one sheet per family: the first light of a family decides. A family with
/// no resolvable sheet is simply absent, and the fixture draws the shared white
/// sheet with its neutral face emission; a sheet that was named but cannot be
/// read or decoded is logged with the fixture id in it and degrades the same
/// way.
#[must_use]
pub fn resolve_fixture_sheets(
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
    pack: Option<&PackMaterials>,
    cache: &mut TextureCache,
) -> Vec<ResolvedFixtureSheet> {
    let root = crate::assets::resolve_asset_root();
    let mut sheets: Vec<ResolvedFixtureSheet> = Vec::new();
    for light in &level.ceiling_lights {
        let kind = crate::lighting::fixture_profile(&light.fixture).kind;
        if sheets.iter().any(|sheet| sheet.kind == kind) {
            continue;
        }
        match resolve_fixture_sheet(&light.fixture, kind, catalog, pack, root.as_deref(), cache) {
            Ok(Some(sheet)) => sheets.push(sheet),
            Ok(None) => {}
            Err(error) => {
                crate::logging::warn_once(
                    format!("fixture-sheet:{}:{error}", light.fixture),
                    format!("[fixtures] {error}; drawing the untextured sheet instead"),
                );
            }
        }
    }
    sheets
}

/// One fixture family's sheet: the pack's own artwork for a `pack:` id, else
/// the catalog PNG the light entry names, else nothing.
///
/// `Ok(None)` is "this fixture has no sheet to draw" (an unknown id, a pack
/// fixture the pack does not carry): a state to degrade from quietly. `Err` is a
/// sheet that exists but cannot be decoded, which is an authoring mistake and is
/// reported.
fn resolve_fixture_sheet(
    fixture_id: &str,
    kind: crate::lighting::FixtureKind,
    catalog: &crate::assets::AssetCatalog,
    pack: Option<&PackMaterials>,
    asset_root: Option<&Path>,
    cache: &mut TextureCache,
) -> Result<Option<ResolvedFixtureSheet>, String> {
    if fixture_id.starts_with("pack:") {
        let Some(pack) = pack else {
            return Ok(None);
        };
        let Some(path) = pack.texture_for(fixture_id) else {
            return Ok(None);
        };
        let key = pack.cache_key(&path);
        let image = pack
            .decode_texture(cache, &path)
            .map_err(|error| format!("fixture `{fixture_id}`: {error}"))?;
        return Ok(Some(ResolvedFixtureSheet {
            kind,
            key,
            origin: crate::materials::TextureOrigin::Pack,
            image,
        }));
    }

    let Some(path) = catalog.fixture_sheet_path(fixture_id) else {
        return Ok(None);
    };
    if let Some(image) = cache.get(path) {
        return Ok(Some(ResolvedFixtureSheet {
            kind,
            key: path.to_string(),
            origin: crate::materials::TextureOrigin::Catalog,
            image,
        }));
    }
    let Some(root) = asset_root else {
        return Err(format!(
            "fixture `{fixture_id}` sheet `{path}`: the asset root is missing"
        ));
    };
    let image = load_png_relative(root, path)
        .map_err(|error| format!("fixture `{fixture_id}` sheet `{path}`: {error}"))?;
    let image = cache.insert(path.to_string(), image);
    Ok(Some(ResolvedFixtureSheet {
        kind,
        key: path.to_string(),
        origin: crate::materials::TextureOrigin::Catalog,
        image,
    }))
}

// ---------------------------------------------------------------------------
// Level preparation
// ---------------------------------------------------------------------------

/// Distance a baseboard floor probe stands off the wall's face, in metres.
const BASEBOARD_FLOOR_PROBE_M: f32 = 0.05;
/// How far floor samples along one face may differ and still be one floor.
const BASEBOARD_FLOOR_AGREEMENT_M: f32 = 0.05;
/// How far a wall's own base may sit from the floor it fronts, in metres.
const BASEBOARD_BASE_AGREEMENT_M: f32 = 0.05;
/// Fractions along a face at which its walkable floor is sampled: both ends
/// inset, the middle and the quarter points, so a floor change anywhere along
/// the run is seen.
const BASEBOARD_FLOOR_FRACTIONS: [f32; 5] = [0.02, 0.25, 0.5, 0.75, 0.98];
/// Plane separation under which two baseboard runs count as the same plane.
const BASEBOARD_SAME_PLANE_M: f32 = 1.0e-3;
/// Run overlap under which two collinear runs do not suppress each other.
const BASEBOARD_OVERLAP_M: f32 = 1.0e-3;

/// One wall length face a baseboard run can sit on: the face's `faces` name,
/// the run's start point on the face, and the run's yaw.
///
/// The direction and across vectors follow [`BaseboardDef::direction`] and
/// [`BaseboardDef::across`] exactly, so a generated run and the emitter agree
/// on which way the board faces.
#[derive(Clone, Copy)]
struct BaseboardFace {
    name: &'static str,
    start: (f32, f32),
    rotation_degrees: f32,
}

impl BaseboardFace {
    /// Run direction as an `(x, z)` unit vector; 0 runs +X.
    fn direction(self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.cos(), -radians.sin())
    }

    /// Across-the-board direction as an `(x, z)` unit vector, pointing out of
    /// the wall's face into the room.
    fn across(self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.sin(), radians.cos())
    }
}

/// The one load-time preparation pass, run after validation and before the
/// decoded material table is built.
///
/// It applies every catalog-driven adjustment that the bake, mesh, collision
/// and fixture probe must all see:
///
/// * grid-aligns fluorescent panels onto their ceiling material's world tile
///   grid ([`LevelDef::align_ceiling_fixtures`]); and
/// * generates the automatic baseboard runs a wall material declares through
///   the catalog's `baseboard` field.
///
/// `materials` starts as a **logical** table: alignment only needs
/// `tile_metres`, which is already final before images decode, and the
/// baseboard pass only needs the catalog. The caller resolves the full decoded
/// table *after* this pass, so a generated run's trim material is part of the
/// renderer's material table even when no authored trim uses it.
pub(crate) fn prepare_level(
    level: &mut LevelDef,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) {
    let materials = MaterialTable::logical(level, catalog, pack);
    level.align_ceiling_fixtures(&materials);
    generate_automatic_baseboards(level, catalog);
}

/// Generates the baseboard runs every wall face's resolved material declares
/// through its catalog `baseboard`, appending them after the authored runs so
/// the existing joint trimming lets authored trim win. Returns how many runs
/// were generated.
///
/// A face qualifies when it is finished with a material whose catalog entry
/// declares `baseboard`, it fronts one walkable floor along its whole length
/// (samples just outside the face at both inset ends, the middle and the
/// quarter points all exist and agree within
/// [`BASEBOARD_FLOOR_AGREEMENT_M`]), and the wall's own base meets that floor
/// (within [`BASEBOARD_BASE_AGREEMENT_M`]). Each run is split around every
/// opening whose sill reaches the board's height, pinned to the floor it
/// fronts, and skipped when it would be buried in a crossing wall or when an
/// authored run already covers the same plane and span.
fn generate_automatic_baseboards(level: &mut LevelDef, catalog: &AssetCatalog) -> usize {
    let authored_count = level.baseboards.len();
    let mut generated: Vec<BaseboardDef> = Vec::new();
    let mut remaining = crate::level::MAX_LEVEL_BASEBOARDS
        .saturating_sub(u64::try_from(authored_count).unwrap_or(u64::MAX));
    {
        let surfaces = LevelSurfaces::new(level);
        // `authored_count` was captured from this same array, so the prefix is
        // always present; the empty fallback only satisfies the no-slicing rule.
        let authored = level.baseboards.get(..authored_count).unwrap_or_default();
        for wall in &level.walls {
            if remaining == 0 {
                break;
            }
            generate_wall_baseboards(
                level,
                &surfaces,
                wall,
                catalog,
                authored,
                &mut generated,
                &mut remaining,
            );
        }
    }
    let count = generated.len();
    level.baseboards.append(&mut generated);
    count
}

/// Generates the runs for one wall's two length faces: the per-wall half of
/// [`generate_automatic_baseboards`], which owns the shared budget and output.
fn generate_wall_baseboards(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    wall: &WallDef,
    catalog: &AssetCatalog,
    authored: &[BaseboardDef],
    generated: &mut Vec<BaseboardDef>,
    remaining: &mut u64,
) {
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let length = wall.length();
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    // Deterministic order: the + face then the - face. An X-axis wall's + face
    // is south (normal +Z), a Z-axis wall's is east (+X).
    let faces: [BaseboardFace; 2] = match wall.axis() {
        WallAxis::X => [
            BaseboardFace {
                name: "south",
                start: (x0, z1),
                rotation_degrees: 0.0,
            },
            BaseboardFace {
                name: "north",
                start: (x1, z0),
                rotation_degrees: 180.0,
            },
        ],
        WallAxis::Z => [
            BaseboardFace {
                name: "east",
                start: (x1, z1),
                rotation_degrees: 90.0,
            },
            BaseboardFace {
                name: "west",
                start: (x0, z0),
                rotation_degrees: 270.0,
            },
        ],
    };
    for face in faces {
        if *remaining == 0 {
            return;
        }
        let material = wall
            .face_ref(face.name)
            .map_or(level.defaults.wall.as_str(), |reference| reference.id);
        let Some(baseboard) = catalog
            .material(material)
            .and_then(|entry| entry.baseboard.as_deref())
        else {
            continue;
        };
        let Some(floor) = baseboard_face_floor(surfaces, wall, face, length) else {
            continue;
        };
        let (dx, dz) = face.direction();
        let along_positive = {
            let (ax, az) = match wall.axis() {
                WallAxis::X => (1.0, 0.0),
                WallAxis::Z => (0.0, 1.0),
            };
            dx.mul_add(ax, dz * az) > 0.0
        };
        for (low, high) in baseboard_segments(wall, length, along_positive) {
            let run = BaseboardDef {
                x: dx.mul_add(low, face.start.0),
                z: dz.mul_add(low, face.start.1),
                length: high - low,
                rotation_degrees: face.rotation_degrees,
                height: BASEBOARD_DEFAULT_HEIGHT_M,
                thickness: BASEBOARD_DEFAULT_THICKNESS_M,
                y: Some(floor),
                material: Some(baseboard.to_string()),
                shine: None,
            };
            if baseboard_run_is_hidden(level, &run) {
                continue;
            }
            if authored_run_suppresses(surfaces, authored, &run) {
                continue;
            }
            generated.push(run);
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                return;
            }
        }
    }
}

/// The walkable floor one face fronts, when the whole run fronts a single
/// floor that the wall's own base meets.
fn baseboard_face_floor(
    surfaces: &LevelSurfaces<'_>,
    wall: &WallDef,
    face: BaseboardFace,
    length: f32,
) -> Option<f32> {
    let (dx, dz) = face.direction();
    let (ax, az) = face.across();
    let mut floor: Option<f32> = None;
    for fraction in BASEBOARD_FLOOR_FRACTIONS {
        let along = length * fraction;
        let x = dx.mul_add(along, ax.mul_add(BASEBOARD_FLOOR_PROBE_M, face.start.0));
        let z = dz.mul_add(along, az.mul_add(BASEBOARD_FLOOR_PROBE_M, face.start.1));
        let sample = surfaces.floor_y_at(x, z)?;
        if floor.is_some_and(|previous| (sample - previous).abs() > BASEBOARD_FLOOR_AGREEMENT_M) {
            return None;
        }
        floor = Some(sample);
    }
    let floor = floor?;
    // The wall's own base is its absolute world `y`; a non-finite value falls
    // back to the room floor under the footprint centre.
    let base = if wall.y.is_finite() {
        wall.y
    } else {
        surfaces
            .floor_y_at(
                f32::midpoint(wall.x, wall.x + wall.width),
                f32::midpoint(wall.z, wall.z + wall.depth),
            )
            .unwrap_or(0.0)
    };
    ((base - floor).abs() <= BASEBOARD_BASE_AGREEMENT_M).then_some(floor)
}

/// The run spans of one face with every floor-reaching opening removed.
///
/// `along_positive` says whether the face's run direction is the wall's own
/// positive length axis; the other faces measure their span from the far end.
/// Spans ascend along the run.
fn baseboard_segments(wall: &WallDef, length: f32, along_positive: bool) -> Vec<(f32, f32)> {
    let mut spans = vec![(0.0, length)];
    for opening in &wall.openings {
        if !opening.sill.is_finite() || opening.sill > BASEBOARD_DEFAULT_HEIGHT_M + 1.0e-3 {
            continue;
        }
        let (low, high) = if along_positive {
            (opening.offset, opening.end())
        } else {
            (length - opening.end(), length - opening.offset)
        };
        let low = low.max(0.0);
        let high = high.min(length);
        if high <= low {
            continue;
        }
        let mut next: Vec<(f32, f32)> = Vec::new();
        for (span_low, span_high) in spans {
            if high <= span_low || low >= span_high {
                next.push((span_low, span_high));
                continue;
            }
            if low > span_low {
                next.push((span_low, low));
            }
            if high < span_high {
                next.push((high, span_high));
            }
        }
        spans = next;
    }
    spans.retain(|(low, high)| high - low > BASEBOARD_OVERLAP_M);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    spans
}

/// True when a run's midpoint lies inside a wall solid: a perpendicular wall
/// crossing the face would hide the board there, so the segment is skipped.
fn baseboard_run_is_hidden(level: &LevelDef, run: &BaseboardDef) -> bool {
    let (x, z) = run.point_at(0.5, run.thickness() * 0.5);
    let y = run
        .height()
        .mul_add(0.5, run.base_y(&LevelSurfaces::new(level)));
    point_buried_in_wall(level, x, z, y).is_some()
}

/// True when an authored run already lies on this run's plane and overlaps its
/// span, in which case the authored board owns the face and the generated run
/// is skipped.
fn authored_run_suppresses(
    surfaces: &LevelSurfaces<'_>,
    authored: &[BaseboardDef],
    run: &BaseboardDef,
) -> bool {
    let (rdx, rdz) = run.direction();
    let (rax, raz) = run.across();
    let base = run.base_y(surfaces);
    for other in authored {
        let (odx, odz) = other.direction();
        if rdx.mul_add(odx, rdz * odz).abs() < 0.999 {
            continue;
        }
        if (other.base_y(surfaces) - base).abs() > BASEBOARD_SAME_PLANE_M {
            continue;
        }
        let rel_x = run.x - other.x;
        let rel_z = run.z - other.z;
        if rax.mul_add(rel_x, raz * rel_z).abs() > BASEBOARD_SAME_PLANE_M {
            continue;
        }
        // Project the generated run's interval onto the authored run's own axis
        // so the two orientations compare in one frame.
        let start = odx.mul_add(rel_x, odz * rel_z);
        let end = run.length.mul_add(rdx.mul_add(odx, rdz * odz), start);
        let (run_low, run_high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let overlap = run_high.min(other.length) - run_low.max(0.0);
        if overlap > BASEBOARD_OVERLAP_M {
            return true;
        }
    }
    false
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
        let assets_dir = crate::assets::resolve_asset_root().map_or_else(
            || PathBuf::from("assets/levels"),
            |root| root.join("levels"),
        );
        let mut manager = Self {
            assets_dir,
            levels_dir: crate::assets::state_path("levels"),
            import_dir: crate::assets::state_path("import"),
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
                crate::logging::warn_once(
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

    /// Prop catalog used to resolve placed props.
    #[must_use]
    pub const fn prop_catalog(&self) -> &PropCatalog {
        &self.prop_catalog
    }

    #[must_use]
    pub fn get_entry(&self, idx: usize) -> Option<&LevelEntry> {
        self.entries.get(idx)
    }

    /// Re-scans directories for installed levels.
    ///
    /// A file that does not parse or does not validate is skipped with one
    /// warning naming the file and the reason, so a malformed drop-in level is
    /// diagnosable instead of silently absent.
    pub fn refresh(&mut self) {
        let mut discovered = Vec::new();

        // 1. Official levels in assets_dir
        if let Ok(dir) = fs::read_dir(&self.assets_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json") {
                    match Self::probe_level_file(&p, LevelSourceType::Official) {
                        Ok(meta) => discovered.push(meta),
                        Err(error) => Self::report_skipped_level(&p, &error),
                    }
                }
            }
        }

        // 2. Installed community / custom levels in levels_dir
        if let Ok(dir) = fs::read_dir(&self.levels_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json") {
                    match Self::probe_level_file(&p, LevelSourceType::CustomJson) {
                        Ok(meta) => discovered.push(meta),
                        Err(error) => Self::report_skipped_level(&p, &error),
                    }
                } else if p.extension().is_some_and(|ext| ext == "zip") {
                    match Self::probe_zip_file(&p) {
                        Ok(meta) => discovered.push(meta),
                        Err(error) => Self::report_skipped_level(&p, &error),
                    }
                }
            }
        }

        // Deterministic menu order: `read_dir` order is filesystem-dependent.
        discovered.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

        // Places Demo is always offered. When no installed `places_demo.json`
        // was discovered (no asset tree, or the installed copy is malformed),
        // the embedded copy is exposed as an ordinary entry so the Level Select
        // menu and `PLACES_LEVEL=places_demo` both keep working.
        if !discovered.iter().any(|entry| entry.id == DEMO_LEVEL_ID) {
            discovered.push(LevelEntry {
                id: DEMO_LEVEL_ID.to_string(),
                name: "Places Demo".to_string(),
                author: "Places Team".to_string(),
                source_type: LevelSourceType::Embedded,
                path: self.assets_dir.join("places_demo.json"),
            });
        }

        self.entries = discovered;
    }

    /// Reports one unreadable level file once per path.
    fn report_skipped_level(path: &Path, error: &str) {
        crate::logging::warn_once(
            format!("level-skipped:{}", path.display()),
            format!("[levels] skipping {}: {error}", path.display()),
        );
    }

    /// Reads a standalone level JSON with a hard byte cap before parsing.
    fn read_standalone_level(path: &Path) -> Result<String, String> {
        let metadata =
            fs::metadata(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if metadata.len() > crate::level::MAX_LEVEL_JSON_BYTES {
            return Err(format!(
                "{} is {} bytes, over the {} byte level limit",
                path.display(),
                metadata.len(),
                crate::level::MAX_LEVEL_JSON_BYTES
            ));
        }
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))
    }

    fn probe_level_file(path: &Path, source_type: LevelSourceType) -> Result<LevelEntry, String> {
        let content = Self::read_standalone_level(path)?;
        let level = LevelDef::from_json(&content).map_err(|e| e.to_string())?;
        validate_level(&level)?;
        Ok(LevelEntry {
            id: level.id,
            name: level.name,
            author: level.author,
            source_type,
            path: path.to_path_buf(),
        })
    }

    fn probe_zip_file(path: &Path) -> Result<LevelEntry, String> {
        // Lightweight probe: read only level.json, no texture extraction.
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let level_json = read_zip_level_json(file)?;
        let level = LevelDef::from_json(&level_json).map_err(|e| e.to_string())?;
        validate_level(&level)?;
        Ok(LevelEntry {
            id: level.id,
            name: level.name,
            author: level.author,
            source_type: LevelSourceType::PackZip,
            path: path.to_path_buf(),
        })
    }

    /// Loads the official demo, or the embedded copy when it is not installed.
    ///
    /// `Places Demo` is the only level bundled with the game, so it is also the
    /// default level the game boots into. External/user levels are unaffected:
    /// they are discovered alongside it and can be selected from the menu.
    /// # Errors
    ///
    /// Returns a message when the demo cannot be loaded, or when neither an
    /// installed nor an embedded demo parses and validates.
    pub fn load_default(&self) -> Result<LoadedLevel, String> {
        if let Some(entry) = self.entries.iter().find(|e| e.id == DEMO_LEVEL_ID) {
            self.load_level(entry)
        } else {
            // Direct fallback: the embedded demo JSON still resolves its
            // materials through the shipped catalog (and degrades loudly to the
            // diagnostic texture when no assets are installed at all).
            let mut level = LevelDef::from_json(FALLBACK_DEMO_JSON)
                .map_err(|e| format!("Failed to parse embedded Places Demo: {e}"))?;
            validate_level(&level)?;
            prepare_level(&mut level, self.prop_catalog.assets(), None);
            let materials = self.resolve_level_materials(&level, None);
            let light_sheets = self.resolve_level_fixture_sheets(&level, None);
            Ok(LoadedLevel {
                level,
                materials,
                light_sheets,
                entry: LevelEntry {
                    id: DEMO_LEVEL_ID.into(),
                    name: "Places Demo".into(),
                    author: "Places Team".into(),
                    source_type: LevelSourceType::Official,
                    path: self.assets_dir.join("places_demo.json"),
                },
            })
        }
    }

    /// Resolves a level's surface materials through the catalog and an optional
    /// pack, reporting every problem once with its level and material context.
    fn resolve_level_materials(
        &self,
        level: &LevelDef,
        pack: Option<&PackMaterials>,
    ) -> MaterialTable {
        let root = crate::assets::resolve_asset_root();
        let mut cache = self.texture_cache.borrow_mut();
        let table = resolve_materials(
            level,
            self.prop_catalog.assets(),
            pack,
            root.as_deref(),
            &mut cache,
        );
        if root.is_none() && !table.errors().is_empty() {
            // Without an asset root the missing-root report above already said
            // why every one of these failed; do not repeat it per material.
            crate::logging::warn_once(
                format!("materials-no-root:{}", level.id),
                format!(
                    "[materials] {}: {} material(s) unresolved (no asset root; see above)",
                    level.id,
                    table.errors().len()
                ),
            );
            return table;
        }
        for error in table.errors() {
            crate::logging::warn_once(
                format!("material:{}:{error}", level.id),
                format!("[materials] {}: {error}", level.id),
            );
        }
        table
    }

    /// Resolves a level's fixture sheets through the catalog and an optional
    /// pack, logging every problem once with its fixture id in it.
    fn resolve_level_fixture_sheets(
        &self,
        level: &LevelDef,
        pack: Option<&PackMaterials>,
    ) -> Vec<ResolvedFixtureSheet> {
        let mut cache = self.texture_cache.borrow_mut();
        resolve_fixture_sheets(level, self.prop_catalog.assets(), pack, &mut cache)
    }

    /// Unified level loader loading any standalone JSON or packaged ZIP level.
    ///
    /// Missing or corrupt texture files resolve to the diagnostic material and
    /// a logged error; a level never fails to load because of one bad PNG.
    /// # Errors
    ///
    /// Returns a message when the level file or pack cannot be read or
    /// validated.
    pub fn load_level(&self, entry: &LevelEntry) -> Result<LoadedLevel, String> {
        match entry.source_type {
            LevelSourceType::Official | LevelSourceType::CustomJson => {
                let content = Self::read_standalone_level(&entry.path)?;

                let mut level = LevelDef::from_json(&content)
                    .map_err(|e| format!("JSON parse error in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;
                prepare_level(&mut level, self.prop_catalog.assets(), None);
                let materials = self.resolve_level_materials(&level, None);
                let light_sheets = self.resolve_level_fixture_sheets(&level, None);

                Ok(LoadedLevel {
                    level,
                    materials,
                    light_sheets,
                    entry: entry.clone(),
                })
            }
            LevelSourceType::Embedded => {
                let mut level = LevelDef::from_json(FALLBACK_DEMO_JSON)
                    .map_err(|e| format!("Failed to parse the embedded Places Demo: {e}"))?;
                validate_level(&level)?;
                prepare_level(&mut level, self.prop_catalog.assets(), None);
                let materials = self.resolve_level_materials(&level, None);
                let light_sheets = self.resolve_level_fixture_sheets(&level, None);

                Ok(LoadedLevel {
                    level,
                    materials,
                    light_sheets,
                    entry: entry.clone(),
                })
            }
            LevelSourceType::PackZip => {
                let file = fs::File::open(&entry.path)
                    .map_err(|e| format!("Failed to open {}: {e}", entry.path.display()))?;
                let pack = extract_zip(file)?;
                let mut level = LevelDef::from_json(&pack.level_json)
                    .map_err(|e| format!("Invalid level.json in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;

                let pack_materials = PackMaterials::new(
                    entry.path.to_string_lossy().to_string(),
                    pack.materials_json.as_deref(),
                    pack.textures,
                );
                prepare_level(
                    &mut level,
                    self.prop_catalog.assets(),
                    Some(&pack_materials),
                );
                let materials = self.resolve_level_materials(&level, Some(&pack_materials));
                let light_sheets = self.resolve_level_fixture_sheets(&level, Some(&pack_materials));

                Ok(LoadedLevel {
                    level,
                    materials,
                    light_sheets,
                    entry: entry.clone(),
                })
            }
        }
    }

    /// Imports an external .json or .zip file into the installed levels directory.
    /// # Errors
    ///
    /// Returns a message when the source file does not exist, is neither a
    /// `.json` level nor a `.zip` pack, or cannot be validated and copied into
    /// the installed levels directory.
    pub fn import_file(&mut self, source_path: &Path) -> Result<LevelEntry, String> {
        if !source_path.exists() {
            return Err(format!(
                "Source file does not exist: {}",
                source_path.display()
            ));
        }

        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 1. Dry-run validate before copying
        let file_name = source_path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;

        let (level_id, level_name, author, source_type) = if ext == "json" {
            let content = Self::read_standalone_level(source_path)?;
            let level =
                LevelDef::from_json(&content).map_err(|e| format!("Invalid level JSON: {e}"))?;
            validate_level(&level)?;
            (
                level.id,
                level.name,
                level.author,
                LevelSourceType::CustomJson,
            )
        } else if ext == "zip" {
            // Validate cheaply from level.json only; textures are not needed to
            // decide whether the pack is acceptable.
            let file = fs::File::open(source_path)
                .map_err(|e| format!("Failed to open {}: {e}", source_path.display()))?;
            let level_json = read_zip_level_json(file)?;
            let level = LevelDef::from_json(&level_json)
                .map_err(|e| format!("Invalid level.json in ZIP: {e}"))?;
            validate_level(&level)?;
            (level.id, level.name, level.author, LevelSourceType::PackZip)
        } else {
            return Err(format!(
                "Unsupported file format '.{ext}'. Supported formats: .json, .zip"
            ));
        };

        // 2. Copy file to levels_dir
        fs::create_dir_all(&self.levels_dir)
            .map_err(|e| format!("Failed to create levels directory: {e}"))?;
        let target_path = self.levels_dir.join(file_name);
        if source_path != target_path {
            fs::copy(source_path, &target_path)
                .map_err(|e| format!("Failed to copy file to {}: {e}", target_path.display()))?;
        }

        self.refresh();

        Ok(LevelEntry {
            id: level_id,
            name: level_name,
            author,
            source_type,
            path: target_path,
        })
    }

    /// Scans `import_dir` and candidate locations for unimported .json or .zip files and imports them.
    /// # Errors
    ///
    /// Returns a message when a candidate file in `import/` is invalid; files
    /// that import cleanly are reported through the returned count.
    pub fn import_available(&mut self) -> Result<usize, String> {
        let _ = fs::create_dir_all(&self.import_dir);
        let _ = fs::create_dir_all(&self.levels_dir);

        let mut imported_count: usize = 0;
        let nested_import = self.levels_dir.join("import");
        let candidate_dirs = [self.import_dir.clone(), nested_import];

        for dir in &candidate_dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if (ext == "json" || ext == "zip") && self.import_file(&path).is_ok() {
                        imported_count = imported_count.saturating_add(1);
                    }
                }
            }
        }

        Ok(imported_count)
    }
}

#[cfg(test)]
mod tests;

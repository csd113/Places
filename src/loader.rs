use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use zip::ZipArchive;

use crate::level::{LevelDef, MAX_LEVEL_FLOOR_AREA_M2, MAX_LEVEL_VERTICES};
use crate::materials::{MaterialTable, PackMaterials, load_png_relative, resolve_materials};

/// Re-exported so the rest of the crate keeps its historical import paths.
pub use crate::materials::{RawImage, TextureCache, decode_png, encode_png, parse_materials_json};

/// The official demo, embedded so the game still boots when no level files are
/// installed on disk. `Places Demo` is the only level shipped with the game.
const FALLBACK_DEMO_JSON: &str = include_str!("../assets/levels/places_demo.json");

const MAX_ZIP_ENTRIES: usize = 500;
const MAX_ZIP_ENTRY_SIZE: u64 = 10 * 1024 * 1024; // 10 MB per file
const MAX_ZIP_TOTAL_SIZE: u64 = 50 * 1024 * 1024; // 50 MB total uncompressed

/// Source type of an installed level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceType {
    Official,
    CustomJson,
    PackZip,
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
    if entry.size() > MAX_ZIP_ENTRY_SIZE {
        return Err("level.json exceeds the maximum decompression limit".into());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read level.json: {e}"))?;
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

        let size = file.size();
        if size > MAX_ZIP_ENTRY_SIZE {
            return Err(format!(
                "ZIP entry {raw_name} exceeds 10MB decompression limit"
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(size);
        if total_uncompressed > MAX_ZIP_TOTAL_SIZE {
            return Err("Total uncompressed size of ZIP exceeds 50MB limit".into());
        }

        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        file.read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read {raw_name}: {e}"))?;

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
    validate_floor_regions(level)?;
    validate_walls(level)?;
    validate_ceiling_lights(level)?;
    validate_props(level)?;
    validate_decals(level)?;
    validate_decal_surfaces(level)?;
    validate_geometry_budget(level)
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
        // A wall fixture is authored at its own world height; a ceiling
        // fixture derives its height, so an authored `y` there is ignored.
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
///    buffer built at load time, protecting the ~512 MB `PocketCHIP` from
///    levels that would otherwise exhaust memory. Overlapping/intersecting
///    geometry is explicitly allowed and is not validated here.
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
/// sheet (its flat authored glow, exactly as before); a sheet that was named but
/// cannot be read or decoded is logged with the fixture id in it and degrades
/// the same way.
#[must_use]
// A broken fixture sheet is a chatty one-line diagnostic and the loader has no
// logger to route through (see `resolve_level_materials`).
#[allow(clippy::print_stderr)]
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
                eprintln!("[fixtures] {error}; drawing the untextured sheet instead");
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
        let mut manager = Self {
            assets_dir: PathBuf::from("assets/levels"),
            levels_dir: PathBuf::from("levels"),
            import_dir: PathBuf::from("import"),
            entries: Vec::new(),
            prop_catalog: PropCatalog::load_default(),
            texture_cache: RefCell::new(TextureCache::new()),
        };
        manager.refresh();
        manager
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

    /// The authoritative asset catalog (materials, textures, props, themes).
    #[must_use]
    pub const fn asset_catalog(&self) -> &crate::assets::AssetCatalog {
        self.prop_catalog.assets()
    }

    /// The session texture cache, for diagnostics and tests.
    #[must_use]
    pub fn texture_cache(&self) -> std::cell::RefMut<'_, TextureCache> {
        self.texture_cache.borrow_mut()
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
    pub fn refresh(&mut self) {
        let mut discovered = Vec::new();

        // 1. Official levels in assets_dir
        if let Ok(dir) = fs::read_dir(&self.assets_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json")
                    && let Ok(meta) = Self::probe_level_file(&p, LevelSourceType::Official)
                {
                    discovered.push(meta);
                }
            }
        }

        // 2. Installed community / custom levels in levels_dir
        if let Ok(dir) = fs::read_dir(&self.levels_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(meta) = Self::probe_level_file(&p, LevelSourceType::CustomJson) {
                        discovered.push(meta);
                    }
                } else if p.extension().is_some_and(|ext| ext == "zip")
                    && let Ok(meta) = Self::probe_zip_file(&p)
                {
                    discovered.push(meta);
                }
            }
        }

        self.entries = discovered;
    }

    fn probe_level_file(path: &Path, source_type: LevelSourceType) -> Result<LevelEntry, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
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
        if let Some(entry) = self.entries.iter().find(|e| e.id == "places_demo") {
            self.load_level(entry)
        } else {
            // Direct fallback: the embedded demo JSON still resolves its
            // materials through the shipped catalog (and degrades loudly to the
            // diagnostic texture when no assets are installed at all).
            let level = LevelDef::from_json(FALLBACK_DEMO_JSON)
                .map_err(|e| format!("Failed to parse embedded Places Demo: {e}"))?;
            validate_level(&level)?;
            let materials = self.resolve_level_materials(&level, None);
            let light_sheets = self.resolve_level_fixture_sheets(&level, None);
            Ok(LoadedLevel {
                level,
                materials,
                light_sheets,
                entry: LevelEntry {
                    id: "places_demo".into(),
                    name: "Places Demo".into(),
                    author: "Liminal Team".into(),
                    source_type: LevelSourceType::Official,
                    path: self.assets_dir.join("places_demo.json"),
                },
            })
        }
    }

    /// Resolves a level's surface materials through the catalog and an optional
    /// pack, logging every problem once with its level and material context.
    // A bad material is reported here or nowhere: the loader has no logger to
    // route the diagnostic through.
    #[allow(clippy::print_stderr)]
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
        for error in table.errors() {
            eprintln!("[materials] {}: {error}", level.id);
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
                let content = fs::read_to_string(&entry.path)
                    .map_err(|e| format!("Failed to read {}: {e}", entry.path.display()))?;

                let level = LevelDef::from_json(&content)
                    .map_err(|e| format!("JSON parse error in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;
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
                let level = LevelDef::from_json(&pack.level_json)
                    .map_err(|e| format!("Invalid level.json in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;

                let pack_materials = PackMaterials::new(
                    entry.path.to_string_lossy().to_string(),
                    pack.materials_json.as_deref(),
                    pack.textures,
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
            let content = fs::read_to_string(source_path)
                .map_err(|e| format!("Failed to read {}: {e}", source_path.display()))?;
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
        let candidate_dirs = [self.import_dir.clone(), PathBuf::from("levels/import")];

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

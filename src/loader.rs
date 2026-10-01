use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::Vec3;

use crate::assets::AssetCatalog;
use crate::entities::sequences::{
    MAX_LEVEL_SEQUENCES, MAX_SEQUENCE_ANIMATION_TIMEOUT_S, MAX_SEQUENCE_STEPS, MAX_SEQUENCE_WAIT_S,
    SequenceDef, SequenceStepDef,
};
use crate::entities::spawn::{
    MAX_LEVEL_SPAWN_GROUPS, MAX_LEVEL_SPAWN_POINTS, MAX_LEVEL_SPAWN_TEMPLATES,
};
use crate::level::{
    ActionDef, BASEBOARD_DEFAULT_HEIGHT_M, BASEBOARD_DEFAULT_THICKNESS_M, BaseboardDef,
    ComponentDef, ConditionDef, EventBindingDef, EventKindName, LevelDef, LevelSurfaces,
    MAX_ACTIONS_PER_SOURCE, MAX_BINDINGS_PER_ENTITY, MAX_LEVEL_FLOOR_AREA_M2, MAX_LEVEL_VERTICES,
    TriggerVolumeDef, WALL_SLICE_EPS, WallAxis, WallDef, wall_solid_slices_profiled,
};
use crate::materials::{MaterialTable, PackMaterials, resolve_materials};

/// Re-exported so the rest of the crate keeps its historical import paths.
pub use crate::materials::{RawImage, TextureCache, decode_png, encode_png, parse_materials_json};

/// The official demo, embedded as a compiled package so the game still boots
/// when no level files are installed on disk. `Places Demo` is the only level
/// shipped with the game; the embedded copy is the same package the repository
/// ships, so the fallback cannot drift from the installed content.
const FALLBACK_DEMO_PACKAGE: &[u8] = include_bytes!("../assets/levels/places_demo.placesmap");

/// Stable id of the one official level, used by discovery and the runtime.
pub const DEMO_LEVEL_ID: &str = "places_demo";

/// The compiled demo package embedded in the executable.
///
/// Exposed so the loading worker can decode the fallback world's records from
/// the same bytes discovery probes, with no second copy of the package.
#[must_use]
pub const fn embedded_demo_package() -> &'static [u8] {
    FALLBACK_DEMO_PACKAGE
}

/// Source type of an installed level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceType {
    /// A compiled package below the read-only bundled `assets/levels/`
    /// directory.
    Bundled,
    /// A compiled package installed in the writable drop-in `levels/`
    /// directory (or copied there by the Import action).
    Installed,
    /// The demo compiled into the executable, used when no installed copy of
    /// Places Demo exists on disk, so the demo is always offered no matter what
    /// is installed.
    Embedded,
}

impl LevelSourceType {
    /// Stable lower-case label, used by `--list-levels` and the diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::Installed => "installed",
            Self::Embedded => "embedded",
        }
    }
}

/// Menu precedence of a source: bundled packages first, then installed ones,
/// then the embedded fallback.
const fn source_rank(source_type: LevelSourceType) -> u8 {
    match source_type {
        LevelSourceType::Bundled => 0,
        LevelSourceType::Installed => 1,
        LevelSourceType::Embedded => 2,
    }
}

/// Discovered level entry for level selection menu.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub catalog: Arc<PropCatalog>,
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
    /// Fixture-face material index this entry fills.
    ///
    /// A family sheet fills its family's slot; a switchable fixture's own face
    /// fills its private slot after
    /// [`crate::level::FIXTURE_SWITCHABLE_MATERIAL_BASE`].
    pub slot: u32,
    /// Session-unique decode/dedupe key: the catalog PNG path, or the pack's own
    /// `pack:<namespace>:<path>` key.
    pub key: String,
    /// Where the sheet came from; decides its GPU lifetime.
    pub origin: crate::materials::TextureOrigin,
    /// Decoded pixels, shared with the session cache.
    pub image: Arc<RawImage>,
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
    /// The document's one entry collection is `assets`; duplicate logical ids
    /// are rejected.
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
    validate_materials(level)?;
    validate_sky(level)?;
    validate_rooms(level)?;
    validate_surface_shine(level)?;
    validate_floor_regions(level)?;
    validate_water(level)?;
    validate_ladders(level)?;
    validate_walls(level)?;
    validate_architecture(level)?;
    validate_ceiling_lights(level)?;
    validate_props(level)?;
    validate_prop_lights(level)?;
    validate_doors(level)?;
    validate_effects(level)?;
    let mut index = validate_instance_ids(level)?;
    index.sequence_owners = collect_sequence_owners(level);
    validate_volumes(level)?;
    validate_timers(level)?;
    validate_spawns(level, &index)?;
    validate_bindings(level, &index)?;
    validate_sequences(level, &index)?;
    validate_zero_delay_cycles(level, &index)?;
    validate_routes(level)?;
    validate_floats(level)?;
    validate_decals(level)?;
    validate_decal_surfaces(level)?;
    validate_animated_emissions(level)?;
    validate_fog_regions(level)?;
    validate_void_walls(level)?;
    validate_geometry_budget(level)
}

/// Regional fog volumes: identity, finite ordered bounds, bounded density,
/// colour and falloff, and the count cap.
///
/// A malformed region is a level error rather than a silent clamp: a layer
/// the author meant to sit over a yard and that instead covers the whole map
/// (or none of it) has to be visible at build time. The check is a pure
/// schema check; resolution against the global atmosphere happens at install.
fn validate_fog_regions(level: &LevelDef) -> Result<(), String> {
    let count = level.fog_regions.len();
    if u64::try_from(count).unwrap_or(u64::MAX)
        > u64::try_from(crate::level::MAX_FOG_REGIONS).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "Level contains too many fog regions: {count} (limit: {})",
            crate::level::MAX_FOG_REGIONS
        ));
    }
    let mut ids: Vec<&str> = Vec::with_capacity(count);
    for (i, region) in level.fog_regions.iter().enumerate() {
        let id = region.id.trim();
        if id.is_empty() {
            return Err(format!("fog region {i} names no id"));
        }
        if id.chars().count() > crate::level::MAX_FOG_REGION_ID_CHARS {
            return Err(format!(
                "fog region {i} ('{id}') has an id longer than {} characters",
                crate::level::MAX_FOG_REGION_ID_CHARS
            ));
        }
        if ids.contains(&id) {
            return Err(format!("fog region {i} ('{id}') repeats an id"));
        }
        ids.push(id);
        if !region
            .min
            .iter()
            .chain(region.max.iter())
            .all(|value| value.is_finite())
        {
            return Err(format!(
                "fog region {i} ('{id}') bounds must be finite numbers"
            ));
        }
        if !(region.min[0] < region.max[0]
            && region.min[1] < region.max[1]
            && region.min[2] < region.max[2])
        {
            return Err(format!(
                "fog region {i} ('{id}') must have min below max on every axis"
            ));
        }
        if !region.density.is_finite()
            || !(0.0..=crate::level::MAX_FOG_REGION_DENSITY).contains(&region.density)
        {
            return Err(format!(
                "fog region {i} ('{id}') has density {} (limit {})",
                region.density,
                crate::level::MAX_FOG_REGION_DENSITY
            ));
        }
        if let Some(color) = region.color
            && !color
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(format!(
                "fog region {i} ('{id}') colour components must be between 0.0 and 1.0"
            ));
        }
        if let Some(falloff) = region.falloff_m
            && (!falloff.is_finite() || falloff < 0.0)
        {
            return Err(format!(
                "fog region {i} ('{id}') has a falloff_m of {falloff} \
                 (must be a finite value at or above 0.0)"
            ));
        }
        if region.ground_y.is_some_and(|value| !value.is_finite())
            || region.top_y.is_some_and(|value| !value.is_finite())
        {
            return Err(format!(
                "fog region {i} ('{id}') ground_y and top_y must be finite when authored"
            ));
        }
    }
    Ok(())
}

/// Void walls: finite ordered boxes, a non-empty well-formed material id, the
/// count cap, and unique ids when present.
fn validate_void_walls(level: &LevelDef) -> Result<(), String> {
    let count = level.void_walls.len();
    if u64::try_from(count).unwrap_or(u64::MAX)
        > u64::try_from(crate::level::MAX_VOID_WALLS).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "Level contains too many void walls: {count} (limit: {})",
            crate::level::MAX_VOID_WALLS
        ));
    }
    let mut ids: Vec<&str> = Vec::with_capacity(count);
    for (i, piece) in level.void_walls.iter().enumerate() {
        if let Some(authored) = &piece.id {
            let id = authored.trim();
            if id.is_empty() {
                return Err(format!("void wall {i} has an empty id"));
            }
            if id.chars().count() > crate::level::MAX_VOID_WALL_ID_CHARS {
                return Err(format!(
                    "void wall {i} ('{id}') has an id longer than {} characters",
                    crate::level::MAX_VOID_WALL_ID_CHARS
                ));
            }
            if ids.contains(&id) {
                return Err(format!("void wall {i} ('{id}') repeats an id"));
            }
            ids.push(id);
        }
        let label = piece
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(|| i.to_string(), |id| format!("{i} ('{id}')"));
        if !piece
            .min
            .iter()
            .chain(piece.max.iter())
            .all(|value| value.is_finite())
        {
            return Err(format!("void wall {label} bounds must be finite numbers"));
        }
        if !(piece.min[0] < piece.max[0]
            && piece.min[1] < piece.max[1]
            && piece.min[2] < piece.max[2])
        {
            return Err(format!(
                "void wall {label} must have min below max on every axis"
            ));
        }
        let material = piece.material.trim();
        if material.is_empty() {
            return Err(format!("void wall {label} names no material"));
        }
        if !crate::assets::is_valid_asset_id(material) {
            return Err(format!(
                "void wall {label} material `{material}` is not a well-formed logical id"
            ));
        }
    }
    Ok(())
}

/// The optional sky: a well-formed texture id, a bounded brightness and a
/// bounded ambient radiance.
///
/// The id is checked for shape only; whether the catalog actually declares it
/// is a load-time resolution decision (an unknown id warns and draws no sky,
/// like every other unresolved reference).
fn validate_sky(level: &LevelDef) -> Result<(), String> {
    let Some(sky) = &level.sky else {
        return Ok(());
    };
    let id = sky.texture.trim();
    if id.is_empty() {
        return Err("sky names no texture".to_string());
    }
    if !crate::assets::is_valid_asset_id(id) {
        return Err(format!(
            "sky texture `{id}` is not a well-formed logical id"
        ));
    }
    if !sky.brightness.is_finite() || sky.brightness < 0.0 {
        return Err(format!(
            "sky brightness must be a finite value at or above 0.0, found {}",
            sky.brightness
        ));
    }
    if sky.brightness > crate::level::MAX_SKY_BRIGHTNESS {
        return Err(format!(
            "sky brightness must be at most {}, found {}",
            crate::level::MAX_SKY_BRIGHTNESS,
            sky.brightness
        ));
    }
    if !sky.ambient.is_finite() || !(0.0..=crate::level::MAX_SKY_AMBIENT).contains(&sky.ambient) {
        return Err(format!(
            "sky ambient must be between 0.0 and {}, found {}",
            crate::level::MAX_SKY_AMBIENT,
            sky.ambient
        ));
    }
    Ok(())
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
    if level.format_version != crate::level::LEVEL_FORMAT_VERSION {
        return Err(format!(
            "Unsupported level format_version: {} (expected {})",
            level.format_version,
            crate::level::LEVEL_FORMAT_VERSION
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
///
/// Each of these is an authoring bound, not a format limit: the engine's data
/// structures are `Vec`/`HashMap`-backed and the per-frame queries go through
/// the collision index, so the caps exist to refuse a pathological or
/// accidentally huge file before it becomes resident, not to define what fits.
/// The values are sized from the extended capacity fixture
/// (`tools/levels/build_capacity_fixtures.py`, which authors one past the
/// *former* cap in every category and is measured on the release build); a
/// level above them is genuinely outside the verified envelope rather than
/// merely large.
fn validate_element_limits(level: &LevelDef) -> Result<(), String> {
    let room_count = level.room_iter().count();
    if u64::try_from(room_count).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_ROOMS {
        return Err(format!(
            "Level contains too many rooms: {room_count} (limit: {})",
            crate::level::MAX_LEVEL_ROOMS
        ));
    }
    if u64::try_from(level.walls.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_WALLS {
        return Err(format!(
            "Level contains too many walls: {} (limit: {})",
            level.walls.len(),
            crate::level::MAX_LEVEL_WALLS
        ));
    }
    if u64::try_from(level.ceiling_lights.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_CEILING_LIGHTS
    {
        return Err(format!(
            "Level contains too many ceiling lights: {} (limit: {})",
            level.ceiling_lights.len(),
            crate::level::MAX_LEVEL_CEILING_LIGHTS
        ));
    }
    if u64::try_from(level.props.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_PROPS {
        return Err(format!(
            "Level contains too many props: {} (limit: {})",
            level.props.len(),
            crate::level::MAX_LEVEL_PROPS
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

/// The level's distinct material budget.
///
/// The count is exactly the id set [`crate::materials::referenced_material_ids`]
/// resolves — the set the renderer builds its
/// [`crate::materials::MaterialTable`] from — so this validator, the loader and
/// the draw path agree on what "a material" is by construction. The check runs
/// before any image is decoded or uploaded, so an over-budget file is refused
/// by name instead of saturating a material index.
fn validate_materials(level: &LevelDef) -> Result<(), String> {
    let count =
        u64::try_from(crate::materials::referenced_material_ids(level).len()).unwrap_or(u64::MAX);
    if count > crate::level::MAX_LEVEL_MATERIALS {
        return Err(format!(
            "Level declares too many distinct materials: {count} (limit {})",
            crate::level::MAX_LEVEL_MATERIALS
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
        if r.width > crate::level::MAX_ROOM_EXTENT_M
            || r.depth > crate::level::MAX_ROOM_EXTENT_M
            || r.height > crate::level::MAX_ROOM_HEIGHT_M
        {
            return Err(format!(
                "Room {i} dimensions exceed maximum limits (max {}x{}x{}m)",
                crate::level::MAX_ROOM_EXTENT_M,
                crate::level::MAX_ROOM_EXTENT_M,
                crate::level::MAX_ROOM_HEIGHT_M
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
        // The optional ceiling tile frame: a non-finite origin or rotation
        // would make every ceiling UV and decal snap on that room undefined.
        if let Some([tile_x, tile_z]) = r.ceiling_tile_origin
            && (!tile_x.is_finite() || !tile_z.is_finite())
        {
            return Err(format!(
                "Room {i} ceiling tile origin must be finite world coordinates"
            ));
        }
        if r.ceiling_tile_rotation_degrees
            .is_some_and(|rotation| !rotation.is_finite())
        {
            return Err(format!(
                "Room {i} ceiling tile rotation must be a finite number of degrees"
            ));
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
    for (i, piece) in level.arc_walls.iter().enumerate() {
        check(&format!("Arc wall {i}"), piece.shine)?;
        check(&format!("Arc wall {i} inner"), piece.inner_shine)?;
        check(&format!("Arc wall {i} outer"), piece.outer_shine)?;
        check(&format!("Arc wall {i} cap"), piece.cap_shine)?;
        check(&format!("Arc wall {i} end"), piece.end_shine)?;
    }
    for (i, piece) in level.pillars.iter().enumerate() {
        check(&format!("Pillar {i}"), piece.shine)?;
        check(&format!("Pillar {i} cap"), piece.cap_shine)?;
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

/// Water volumes: position, shape, size/radius, surface, material and depth.
///
/// A rectangle requires `width`/`depth`; a circle requires `radius` and may
/// author `width`/`depth` only as its own bounding box (`2 * radius`), so a
/// contradictory record can never make the drawn disc and the membership test
/// disagree. A volume whose surface sits at or below the floor beneath it is a
/// typo the author has to see (the water would be hidden inside the geometry),
/// so the walkable floor is sampled inside the volume's *own* footprint — the
/// rectangle, or the disc's interior rather than its bounding box corners —
/// and compared against the authored surface. A volume that overlaps no room
/// is rejected like a floor region that overlaps none.
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
        validate_water_shape(i, volume)?;
        validate_water_contract(i, volume)?;
        validate_water_footprint(i, volume, &surfaces)?;
    }
    Ok(())
}

/// One water volume's shape contract: finite position and surface, and the
/// matching `width`/`depth` or `radius` for its shape.
fn validate_water_shape(i: usize, volume: &crate::level::WaterVolumeDef) -> Result<(), String> {
    use crate::level::WaterShape;

    if !volume.x.is_finite() || !volume.z.is_finite() || !volume.surface_y.is_finite() {
        return Err(format!(
            "Water volume {i} position and surface must be finite numbers"
        ));
    }
    match volume.shape {
        WaterShape::Rect => {
            let Some(width) = volume.width else {
                return Err(format!(
                    "Water volume {i} is a rectangle and must author width"
                ));
            };
            let Some(depth) = volume.depth else {
                return Err(format!(
                    "Water volume {i} is a rectangle and must author depth"
                ));
            };
            if !width.is_finite() || width <= 0.0 || !depth.is_finite() || depth <= 0.0 {
                return Err(format!(
                    "Water volume {i} width and depth must be finite and positive"
                ));
            }
            if volume.radius.is_some() {
                return Err(format!(
                    "Water volume {i} authors radius on a rectangle; use \
                     \"shape\": \"circle\" for a circular pool"
                ));
            }
        }
        WaterShape::Circle => {
            let Some(radius) = volume.radius else {
                return Err(format!(
                    "Water volume {i} is a circle and must author radius"
                ));
            };
            if !radius.is_finite() || radius <= 0.0 {
                return Err(format!(
                    "Water volume {i} radius must be a finite number greater than 0"
                ));
            }
            // The bounding box is a derivation, never a second source of
            // truth: a circle may author `width`/`depth` only as its own
            // diameter (a generator that stamps the box verbatim), and a
            // mismatch is rejected by name.
            let diameter = 2.0 * radius;
            for (name, value) in [("width", volume.width), ("depth", volume.depth)] {
                if let Some(value) = value
                    && (!value.is_finite() || (value - diameter).abs() > 1.0e-4)
                {
                    return Err(format!(
                        "Water volume {i} {name} ({value:?}) must be absent or equal \
                         2 * radius ({diameter:?}); a circle's bounding box is derived"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// One water volume's material/opacity/depth contract.
fn validate_water_contract(i: usize, volume: &crate::level::WaterVolumeDef) -> Result<(), String> {
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
    Ok(())
}

/// One water volume's non-empty footprint and its overlap with a room.
///
/// The floor samples are the volume's own footprint: a rectangle's centre and
/// corners (inset off a room seam), or a circle's centre and four interior
/// points, so a bounding box corner the disc does not cover never decides
/// anything.
fn validate_water_footprint(
    i: usize,
    volume: &crate::level::WaterVolumeDef,
    surfaces: &crate::level::LevelSurfaces<'_>,
) -> Result<(), String> {
    use crate::level::WaterShape;

    let (x0, x1, z0, z1) = volume.bounds();
    if x1 <= x0 || z1 <= z0 {
        return Err(format!(
            "Water volume {i} has an empty footprint; its width/depth or radius \
             must describe a positive extent"
        ));
    }
    let mut overlaps_room = false;
    let mut samples: [(f32, f32); 5] = match volume.shape {
        WaterShape::Rect => [
            (f32::midpoint(x0, x1), f32::midpoint(z0, z1)),
            (x0, z0),
            (x1, z0),
            (x1, z1),
            (x0, z1),
        ],
        WaterShape::Circle => {
            let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
            // The centre and four interior points at half the radius: the
            // disc's own footprint, not the bounding box corners a square
            // would wrongly include.
            let offset = volume.radius.unwrap_or(0.0) * 0.353_553_4;
            [
                (cx, cz),
                (cx - offset, cz - offset),
                (cx + offset, cz - offset),
                (cx + offset, cz + offset),
                (cx - offset, cz + offset),
            ]
        }
    };
    if volume.shape == WaterShape::Rect {
        // Inset the corner samples so a volume that shares an edge with a
        // room boundary is not rejected by floating-point noise on the seam.
        // The inset is scaled down for a very small footprint so the clamp
        // bounds can never invert (which would panic).
        let inset_x = 1.0e-3_f32.min((x1 - x0) * 0.25);
        let inset_z = 1.0e-3_f32.min((z1 - z0) * 0.25);
        for (x, z) in &mut samples {
            *x = x.clamp(x0 + inset_x, x1 - inset_x);
            *z = z.clamp(z0 + inset_z, z1 - inset_z);
        }
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
    Ok(())
}

/// Climbable ladder volumes: finite footprint and reach, positive size, top
/// above bottom, and a footprint that overlaps a room section.
///
/// A ladder is the space the player climbs through, not the prop that draws the
/// rails, so the checks mirror the water volumes: the footprint must be real
/// geometry the player can reach, and a top at or below the bottom is a typo
/// the author has to see rather than an inert climb volume.
fn validate_ladders(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.ladders.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_LADDERS {
        return Err(format!(
            "Level contains too many ladders: {} (limit: {})",
            level.ladders.len(),
            crate::level::MAX_LEVEL_LADDERS
        ));
    }
    let surfaces = crate::level::LevelSurfaces::new(level);
    for (i, ladder) in level.ladders.iter().enumerate() {
        if !ladder.x.is_finite()
            || !ladder.z.is_finite()
            || !ladder.width.is_finite()
            || !ladder.depth.is_finite()
            || !ladder.bottom_y.is_finite()
            || !ladder.top_y.is_finite()
            || !ladder.facing_degrees.is_finite()
        {
            return Err(format!(
                "Ladder {i} position, size, reach and facing must be finite numbers"
            ));
        }
        if ladder.width <= 0.0 || ladder.depth <= 0.0 {
            return Err(format!("Ladder {i} width and depth must be positive"));
        }
        if ladder.top_y <= ladder.bottom_y {
            return Err(format!(
                "Ladder {i} top_y ({:.2} m) must be above its bottom_y ({:.2} m)",
                ladder.top_y, ladder.bottom_y
            ));
        }
        let (x0, x1, z0, z1) = ladder.bounds();
        let mut overlaps_room = false;
        let mut samples: [(f32, f32); 5] = [
            (f32::midpoint(x0, x1), f32::midpoint(z0, z1)),
            (x0, z0),
            (x1, z0),
            (x1, z1),
            (x0, z1),
        ];
        // The inset is scaled down for a very small footprint so the clamp
        // bounds can never invert (which would panic).
        let inset_x = 1.0e-3_f32.min((x1 - x0) * 0.25);
        let inset_z = 1.0e-3_f32.min((z1 - z0) * 0.25);
        for (x, z) in &mut samples {
            *x = x.clamp(x0 + inset_x, x1 - inset_x);
            *z = z.clamp(z0 + inset_z, z1 - inset_z);
        }
        for (x, z) in samples {
            if surfaces.floor_y_at(x, z).is_some() {
                overlaps_room = true;
                break;
            }
        }
        if !overlaps_room {
            return Err(format!("Ladder {i} lies outside every room section"));
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
    validate_arc_walls(level)?;
    validate_pillars(level)?;
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

/// Arc walls: a curved solid slab whose radii, sweep and tessellation are all
/// usable, with named diagnostics for every degenerate dimension.
fn validate_arc_walls(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.arc_walls.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_ARC_WALLS
    {
        return Err(format!(
            "Level contains too many arc walls: {} (limit: {})",
            level.arc_walls.len(),
            crate::level::MAX_LEVEL_ARC_WALLS
        ));
    }
    for (i, piece) in level.arc_walls.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.radius.is_finite()
            || !piece.thickness.is_finite()
        {
            return Err(format!(
                "Arc wall {i} centre, radius and thickness must be finite numbers"
            ));
        }
        if piece.radius <= 0.0 {
            return Err(format!(
                "Arc wall {i} radius must be positive (found {})",
                piece.radius
            ));
        }
        if piece.thickness <= 0.0 || piece.thickness >= piece.radius * 2.0 {
            return Err(format!(
                "Arc wall {i} thickness must be positive and thinner than twice its radius \
                 (radius {}, thickness {})",
                piece.radius, piece.thickness
            ));
        }
        if !piece.start_degrees.is_finite() || !piece.sweep_degrees.is_finite() {
            return Err(format!(
                "Arc wall {i} start and sweep angles must be finite numbers"
            ));
        }
        if piece.sweep_degrees.abs() <= 1.0e-3 || piece.sweep_degrees.abs() > 360.0 + 1.0e-3 {
            return Err(format!(
                "Arc wall {i} sweep must be a non-zero angle up to 360 degrees (found {})",
                piece.sweep_degrees
            ));
        }
        if let Some(segments) = piece.segments
            && !(crate::level::ROUND_SEGMENTS_MIN..=crate::level::ROUND_SEGMENTS_MAX)
                .contains(&segments)
        {
            return Err(format!(
                "Arc wall {i} segments must be between {} and {}, found {segments}",
                crate::level::ROUND_SEGMENTS_MIN,
                crate::level::ROUND_SEGMENTS_MAX
            ));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Arc wall {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!(
                "Arc wall {i} base height must be a finite number when authored"
            ));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.inner_material.as_deref())
            || blank_material(piece.outer_material.as_deref())
            || blank_material(piece.cap_material.as_deref())
            || blank_material(piece.end_material.as_deref())
        {
            return Err(format!(
                "Arc wall {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Circular pillars: a solid round post with a usable radius, height and
/// tessellation.
fn validate_pillars(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.pillars.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_PILLARS {
        return Err(format!(
            "Level contains too many pillars: {} (limit: {})",
            level.pillars.len(),
            crate::level::MAX_LEVEL_PILLARS
        ));
    }
    for (i, piece) in level.pillars.iter().enumerate() {
        if !piece.x.is_finite() || !piece.z.is_finite() || !piece.radius.is_finite() {
            return Err(format!(
                "Pillar {i} centre and radius must be finite numbers"
            ));
        }
        if piece.radius <= 0.0 {
            return Err(format!(
                "Pillar {i} radius must be positive (found {})",
                piece.radius
            ));
        }
        if let Some(segments) = piece.segments
            && !(crate::level::ROUND_SEGMENTS_MIN..=crate::level::ROUND_SEGMENTS_MAX)
                .contains(&segments)
        {
            return Err(format!(
                "Pillar {i} segments must be between {} and {}, found {segments}",
                crate::level::ROUND_SEGMENTS_MIN,
                crate::level::ROUND_SEGMENTS_MAX
            ));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Pillar {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!(
                "Pillar {i} base height must be a finite number when authored"
            ));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Pillar {i} materials must be non-empty ids when specified"
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

/// Largest authored lifetime a `lifetime` component may declare, in seconds.
///
/// A bound, not a tuning knob: a lifetime exists to remove a temporary or
/// spawned entity, and one that can outlive an hour-long session is a typo
/// (or an entity that never expires) that would pin its spawn slot for the
/// whole run. The runtime reaps the entity at expiry.
pub const MAX_LIFETIME_SECONDS: f32 = 3600.0;

/// Every authored id an action or condition can address, with the capability
/// facts those checks need.
///
/// One instance namespace covers props, doors, ceiling fixtures, trigger
/// volumes, timers and spawn points, exactly as the runtime addresses them;
/// sequences, spawn templates and spawn groups are separate resource
/// namespaces. See [`validate_instance_ids`].
#[derive(Default)]
struct LevelIndex<'a> {
    /// Entity instance ids, in authored order, mapped to their capability
    /// facts.
    entities: HashMap<String, EntityFacts<'a>>,
    /// Spawn template ids and the facts a template-born instance carries.
    spawn_templates: HashMap<String, EntityFacts<'a>>,
    /// Authored sequence ids.
    sequences: HashSet<String>,
    /// Authored spawn group ids.
    spawn_groups: HashSet<String>,
    /// Authored spawn point ids.
    spawn_points: HashSet<String>,
    /// Sequence id -> every entity the sequence can run on, discovered from
    /// the authored `start_sequence` sites.
    sequence_owners: HashMap<String, Vec<String>>,
}

impl<'a> LevelIndex<'a> {
    /// The facts of one addressable entity: a placed record, or the instances
    /// a spawn template creates.
    fn facts_of(&self, id: &str) -> Option<&EntityFacts<'a>> {
        self.entities
            .get(id)
            .or_else(|| self.spawn_templates.get(id))
    }
}

/// What one authored record can do, derived from its kind and components.
///
/// The action, condition and binding checks read only these facts, so a target
/// that resolves always has a known capability set and every failure names the
/// record and the missing capability.
#[allow(clippy::struct_excessive_bools)] // independent capability bits, not mutually exclusive states
#[derive(Default)]
struct EntityFacts<'a> {
    /// Diagnostic kind name, e.g. `prop` or `trigger volume`.
    kind: &'static str,
    /// True for a placed prop (the only record with a toggleable label).
    is_prop: bool,
    /// True for a door leaf.
    is_door: bool,
    /// True for a ceiling/wall fixture record.
    is_light_fixture: bool,
    /// True for an authored timer.
    is_timer: bool,
    /// True for an authored trigger volume.
    is_volume: bool,
    /// True for an authored spawn point.
    is_spawn_point: bool,
    /// True for a spawn template (its instances carry the template's facts).
    is_spawn_template: bool,
    /// The entity carries an `interactable` component.
    has_interactable: bool,
    /// The authored `interactable` starts enabled.
    interactable_enabled: bool,
    /// The entity carries an `animation` component.
    has_animation: bool,
    /// The entity carries a `light` component.
    has_light: bool,
    /// The entity's light (or fixture) can be switched at runtime.
    light_switchable: bool,
    /// The entity carries a `material` component.
    has_material: bool,
    /// The entity carries an `audio` component.
    has_audio: bool,
    /// State names the entity authors.
    state_names: HashSet<&'a str>,
    /// Variant names of the entity's `material` component, in authored order.
    material_variants: Vec<&'a str>,
}

impl EntityFacts<'_> {
    /// A record of `kind` with no authored capabilities.
    fn of(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// True when an action can turn this entity's light on and off.
    const fn light_capable(&self) -> bool {
        self.is_light_fixture || self.has_light
    }
}

/// Stable instance identities, component values and resource namespaces.
///
/// Every placed record shares one instance-id namespace per level: props,
/// doors, ceiling fixtures, trigger volumes, timers and spawn points. An id is
/// authored or deterministically defaulted (see
/// [`crate::level::LevelDef::prop_instance_ids`]); duplicates and malformed
/// values are named errors. Sequence, spawn-template and spawn-group ids are
/// separate resource namespaces with the same uniqueness contract within
/// their own kind.
///
/// The same pass validates every authored component on the records that carry
/// them and builds the capability index the binding checks read.
#[allow(clippy::too_many_lines)] // one cohesive id + capability pass
fn validate_instance_ids(level: &LevelDef) -> Result<LevelIndex<'_>, String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut first: HashMap<&str, String> = HashMap::new();
    let mut index = LevelIndex::default();

    let prop_ids = level.prop_instance_ids();
    for (i, id) in prop_ids.iter().enumerate() {
        validate_instance_id(&mut seen, &mut first, id, &format!("Prop {i}"), "instance")?;
    }
    let light_ids = level.light_instance_ids();
    for (i, id) in light_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Ceiling light {i}"),
            "instance",
        )?;
    }
    let door_ids = level.door_instance_ids();
    for (i, id) in door_ids.iter().enumerate() {
        validate_instance_id(&mut seen, &mut first, id, &format!("Door {i}"), "instance")?;
    }
    let volume_ids = volume_instance_ids(level);
    for (i, id) in volume_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Trigger volume {i}"),
            "instance",
        )?;
    }
    for (i, timer) in level.timers.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            timer.id.as_str(),
            &format!("Timer {i}"),
            "instance",
        )?;
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            point.id.as_str(),
            &format!("Spawn point {i}"),
            "instance",
        )?;
    }
    // Water volumes and effect emitters become entities with synthesized ids:
    // an authored id that shadows one would make the volume/emitter
    // unreachable to `enable`/`disable`.
    let water_ids = level.water_instance_ids();
    for (i, id) in water_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Water volume {i}"),
            "instance",
        )?;
    }
    let effect_ids: Vec<String> = level
        .effects
        .iter()
        .enumerate()
        .map(|(i, effect)| {
            effect
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map_or_else(|| format!("effect_{}", i.saturating_add(1)), str::to_string)
        })
        .collect();
    for (i, id) in effect_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Effect {i}"),
            "instance",
        )?;
    }

    // Resource namespaces: sequences, spawn templates and spawn groups are
    // addressed by resource name, so duplicates are errors within each kind
    // (an id shared across kinds is unambiguous).
    let mut sequence_seen: HashSet<&str> = HashSet::new();
    let mut sequence_first: HashMap<&str, String> = HashMap::new();
    for (i, sequence) in level.sequences.iter().enumerate() {
        validate_instance_id(
            &mut sequence_seen,
            &mut sequence_first,
            sequence.id.as_str(),
            &format!("Sequence {i}"),
            "sequence",
        )?;
        index.sequences.insert(sequence.id.trim().to_string());
    }
    let mut template_seen: HashSet<&str> = HashSet::new();
    let mut template_first: HashMap<&str, String> = HashMap::new();
    for (i, template) in level.spawn_templates.iter().enumerate() {
        let context = format!("Spawn template {i}");
        validate_instance_id(
            &mut template_seen,
            &mut template_first,
            template.id.as_str(),
            &context,
            "spawn template",
        )?;
        let mut facts = EntityFacts::of("spawn template");
        facts.is_spawn_template = true;
        validate_components(&context, &template.components, &mut facts)?;
        index
            .spawn_templates
            .insert(template.id.trim().to_string(), facts);
    }
    let mut group_seen: HashSet<&str> = HashSet::new();
    let mut group_first: HashMap<&str, String> = HashMap::new();
    for (i, group) in level.spawn_groups.iter().enumerate() {
        validate_instance_id(
            &mut group_seen,
            &mut group_first,
            group.id.as_str(),
            &format!("Spawn group {i}"),
            "spawn group",
        )?;
        index.spawn_groups.insert(group.id.trim().to_string());
    }

    // Capability facts for every placed record, in authored order.
    for (i, prop) in level.props.iter().enumerate() {
        let Some(id) = prop_ids.get(i) else {
            continue;
        };
        let context = format!("Prop {i} (`{id}`)");
        if let Some(name) = prop.display_name.as_deref()
            && name.trim().is_empty()
        {
            return Err(format!(
                "{context} display_name must not be blank when specified"
            ));
        }
        let mut facts = EntityFacts::of("prop");
        facts.is_prop = true;
        validate_components(&context, &prop.components, &mut facts)?;
        index.entities.insert(id.clone(), facts);
    }
    for (i, door) in level.doors.iter().enumerate() {
        let Some(id) = door_ids.get(i) else {
            continue;
        };
        let context = format!("Door {i} (`{id}`)");
        let mut facts = EntityFacts::of("door");
        facts.is_door = true;
        validate_components(&context, &door.components, &mut facts)?;
        index.entities.insert(id.clone(), facts);
    }
    for (i, fixture) in level.ceiling_lights.iter().enumerate() {
        let Some(id) = light_ids.get(i) else {
            continue;
        };
        let mut facts = EntityFacts::of("ceiling light");
        facts.is_light_fixture = true;
        facts.light_switchable = fixture.switchable;
        index.entities.insert(id.clone(), facts);
    }
    for (i, _volume) in level.volumes.iter().enumerate() {
        let Some(id) = volume_ids.get(i) else {
            continue;
        };
        let mut facts = EntityFacts::of("trigger volume");
        facts.is_volume = true;
        index.entities.insert(id.clone(), facts);
    }
    // Water volumes and effect emitters become entities too (they carry no
    // components), so an `enable`/`disable` action can name one: the same
    // resolution the runtime performs when it creates their controllers.
    for id in &water_ids {
        index
            .entities
            .insert(id.clone(), EntityFacts::of("water volume"));
    }
    for id in &effect_ids {
        index.entities.insert(id.clone(), EntityFacts::of("effect"));
    }
    for timer in &level.timers {
        let id = timer.id.trim();
        let mut facts = EntityFacts::of("timer");
        facts.is_timer = true;
        index.entities.insert(id.to_string(), facts);
    }
    for point in &level.spawn_points {
        let id = point.id.trim();
        index.spawn_points.insert(id.to_string());
        let mut facts = EntityFacts::of("spawn point");
        facts.is_spawn_point = true;
        index.entities.insert(id.to_string(), facts);
    }
    Ok(index)
}

/// Validates one authored id and records it for duplicate detection.
///
/// `namespace` names the uniqueness contract the id belongs to (`instance`,
/// `sequence`, `spawn template`, `spawn group`), so a duplicate error says
/// exactly which set of names it collided with.
fn validate_instance_id<'a>(
    seen: &mut HashSet<&'a str>,
    first: &mut HashMap<&'a str, String>,
    id: &'a str,
    context: &str,
    namespace: &str,
) -> Result<(), String> {
    let trimmed = id.trim();
    if !crate::assets::is_valid_asset_id(trimmed) {
        return Err(format!(
            "{context} id `{id}` must be a non-empty, well-formed identifier"
        ));
    }
    if !seen.insert(trimmed) {
        let first = first
            .get(trimmed)
            .map_or_else(|| "an earlier record".to_string(), String::clone);
        return Err(format!(
            "{context} id `{trimmed}` duplicates `{first}`; {namespace} ids must be unique \
             per level"
        ));
    }
    first.insert(trimmed, context.to_string());
    Ok(())
}

/// The stable instance id of one authored trigger volume: its own id when
/// authored, else `trigger_<n>` with `n` the 1-based authored position.
fn volume_id(index: usize, volume: &TriggerVolumeDef) -> String {
    volume
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map_or_else(
            || format!("trigger_{}", index.saturating_add(1)),
            str::to_string,
        )
}

/// Every trigger volume's stable instance id, in authored order.
fn volume_instance_ids(level: &LevelDef) -> Vec<String> {
    level
        .volumes
        .iter()
        .enumerate()
        .map(|(index, volume)| volume_id(index, volume))
        .collect()
}

/// Validates one record's components and records the capabilities they grant.
///
/// A component kind that only makes sense once per entity may appear at most
/// once; `state` is the one repeatable kind, and then only with a distinct,
/// non-empty name. Every authored value is checked here, so a component that
/// could never behave is a named error instead of a runtime no-op.
fn validate_components<'a>(
    context: &str,
    components: &'a [ComponentDef],
    facts: &mut EntityFacts<'a>,
) -> Result<(), String> {
    let mut singles: HashSet<&'static str> = HashSet::new();
    let mut state_names: HashSet<&'a str> = HashSet::new();
    for (i, component) in components.iter().enumerate() {
        match component {
            ComponentDef::State { name, .. } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(format!(
                        "{context} `state` component {i} must name a non-empty state"
                    ));
                }
                if !state_names.insert(name) {
                    return Err(format!(
                        "{context} declares two `state` components named `{name}`; state \
                         names must be unique per entity"
                    ));
                }
            }
            other @ (ComponentDef::Interactable { .. }
            | ComponentDef::Animation { .. }
            | ComponentDef::Audio { .. }
            | ComponentDef::Light { .. }
            | ComponentDef::Material { .. }
            | ComponentDef::Lifetime { .. }
            | ComponentDef::Steam { .. }
            | ComponentDef::Water { .. }
            | ComponentDef::NavAgent { .. }
            | ComponentDef::NavObstacle { .. }
            | ComponentDef::Fade(_)
            | ComponentDef::Glow(_)
            | ComponentDef::Ai(_)) => {
                let kind = other.kind();
                if !singles.insert(kind) {
                    return Err(format!(
                        "{context} declares more than one `{kind}` component; only `state` \
                         components may repeat, with distinct names"
                    ));
                }
            }
        }
        validate_component_value(context, i, component, facts)?;
    }
    let has_ai = components
        .iter()
        .any(|component| matches!(component, ComponentDef::Ai(_)));
    let has_body = components
        .iter()
        .any(|component| matches!(component, ComponentDef::NavAgent { .. }));
    if has_ai && !has_body {
        return Err(format!(
            "{context} authors an `ai` component without a `nav_agent` body; an agent needs \
             the physical profile its baked navigation class is selected by"
        ));
    }
    facts.state_names = state_names;
    Ok(())
}

/// One component's authored values, and the capability it grants.
#[allow(clippy::too_many_lines)] // one match arm per component kind
fn validate_component_value<'a>(
    context: &str,
    i: usize,
    component: &'a ComponentDef,
    facts: &mut EntityFacts<'a>,
) -> Result<(), String> {
    match component {
        ComponentDef::Interactable {
            prompt,
            reach,
            enabled,
            label,
        } => {
            if prompt
                .as_deref()
                .is_some_and(|prompt| prompt.trim().is_empty())
            {
                return Err(format!(
                    "{context} `interactable` component {i} prompt must not be blank when \
                     specified"
                ));
            }
            if label
                .as_deref()
                .is_some_and(|label| label.trim().is_empty())
            {
                return Err(format!(
                    "{context} `interactable` component {i} label must not be blank when \
                     specified"
                ));
            }
            if let Some(reach) = reach
                && !(reach.is_finite()
                    && *reach > 0.0
                    && *reach <= crate::interact::MAX_INTERACTION_REACH_M)
            {
                return Err(format!(
                    "{context} `interactable` component {i} reach ({reach:?} m) must be \
                     between 0 and {} metres",
                    crate::interact::MAX_INTERACTION_REACH_M
                ));
            }
            facts.has_interactable = true;
            facts.interactable_enabled = *enabled;
        }
        ComponentDef::Animation { clip, .. } => {
            if clip.trim().is_empty() {
                return Err(format!(
                    "{context} `animation` component {i} clip must not be blank"
                ));
            }
            facts.has_animation = true;
        }
        ComponentDef::Audio { sound, .. } => {
            if sound.trim().is_empty() {
                return Err(format!(
                    "{context} `audio` component {i} sound must not be blank"
                ));
            }
            facts.has_audio = true;
        }
        ComponentDef::Light {
            emission_scale,
            switchable,
            ..
        } => {
            if !emission_scale.is_finite() || *emission_scale < 0.0 {
                return Err(format!(
                    "{context} `light` component {i} emission_scale ({emission_scale:?}) \
                     must be a finite number that is not negative"
                ));
            }
            if *switchable {
                return Err(format!(
                    "{context} `light` component {i} sets switchable on a prop: only a \
                     ceiling fixture has prepared switchable lightmap layers, so a prop \
                     light is a static emitter (author `enabled` for its initial state)"
                ));
            }
            facts.has_light = true;
        }
        ComponentDef::Material { variants, current } => {
            if variants.is_empty() {
                return Err(format!(
                    "{context} `material` component {i} must declare at least one variant"
                ));
            }
            let mut names: HashSet<&str> = HashSet::new();
            for (j, variant) in variants.iter().enumerate() {
                let name = variant.name.trim();
                if name.is_empty() {
                    return Err(format!(
                        "{context} `material` component {i} variant {j} must name a \
                         non-empty variant"
                    ));
                }
                if !names.insert(name) {
                    return Err(format!(
                        "{context} `material` component {i} declares variant `{name}` twice; \
                         variant names must be unique"
                    ));
                }
                if !variant.emission_scale.is_finite() || variant.emission_scale < 0.0 {
                    return Err(format!(
                        "{context} `material` component {i} variant `{name}` emission_scale \
                         ({:?}) must be a finite number that is not negative",
                        variant.emission_scale
                    ));
                }
                facts.material_variants.push(name);
            }
            if let Some(current) = current {
                let current = current.trim();
                if current.is_empty() {
                    return Err(format!(
                        "{context} `material` component {i} current must not be blank when \
                         specified"
                    ));
                }
                if !names.contains(current) {
                    return Err(format!(
                        "{context} `material` component {i} current `{current}` is not one \
                         of its variants"
                    ));
                }
            }
            facts.has_material = true;
        }
        ComponentDef::Lifetime { seconds } => {
            if !seconds.is_finite() || *seconds <= 0.0 || *seconds > MAX_LIFETIME_SECONDS {
                return Err(format!(
                    "{context} `lifetime` component {i} seconds ({seconds:?}) must be a \
                     finite number between 0 and {MAX_LIFETIME_SECONDS}"
                ));
            }
        }
        ComponentDef::NavAgent {
            radius,
            speed_mps,
            height,
            step_height,
            max_slope,
        } => {
            let valid = radius.is_finite()
                && *radius > 0.0
                && speed_mps.is_finite()
                && *speed_mps > 0.0
                && height.is_finite()
                && *height > 0.0
                && step_height.is_finite()
                && *step_height > 0.0
                && max_slope.is_finite()
                && *max_slope > 0.0;
            if !valid {
                return Err(format!(
                    "{context} `nav_agent` component {i} radius, speed_mps, height, \
                     step_height and max_slope must be finite and positive"
                ));
            }
        }
        ComponentDef::Ai(def) => {
            if !def.is_valid() {
                return Err(format!(
                    "{context} `ai` component {i} has an invalid behavior, speed, range or tag"
                ));
            }
        }
        ComponentDef::Fade(def) => {
            if !def.period_seconds.is_finite()
                || def.period_seconds <= 0.0
                || def.period_seconds > crate::level::MAX_FADE_PERIOD_SECONDS
            {
                return Err(format!(
                    "{context} `fade` component {i} period_seconds ({:?}) must be a finite \
                     number between 0 and {}",
                    def.period_seconds,
                    crate::level::MAX_FADE_PERIOD_SECONDS
                ));
            }
            if let Some(phase) = def.phase
                && !(phase.is_finite() && (0.0..=1.0).contains(&phase))
            {
                return Err(format!(
                    "{context} `fade` component {i} phase ({phase:?}) must be a finite number \
                     between 0 and 1"
                ));
            }
            for (name, value) in [
                ("min_opacity", def.min_opacity),
                ("max_opacity", def.max_opacity),
            ] {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err(format!(
                        "{context} `fade` component {i} {name} ({value:?}) must be a finite \
                         number between 0 and 1"
                    ));
                }
            }
            if def.min_opacity > def.max_opacity {
                return Err(format!(
                    "{context} `fade` component {i} min_opacity ({:?}) must not exceed \
                     max_opacity ({:?})",
                    def.min_opacity, def.max_opacity
                ));
            }
            // The proximity contract is all-or-nothing: both radii, in order,
            // and the two fade times only alongside them.
            match (def.near_radius, def.far_radius) {
                (None, None) => {
                    if def.fade_out_seconds.is_some() || def.fade_in_seconds.is_some() {
                        return Err(format!(
                            "{context} `fade` component {i} authors fade_out_seconds/\
                             fade_in_seconds without near_radius and far_radius"
                        ));
                    }
                }
                (Some(near), Some(far)) => {
                    if !near.is_finite() || near <= 0.0 {
                        return Err(format!(
                            "{context} `fade` component {i} near_radius ({near:?}) must be a \
                             finite number greater than 0"
                        ));
                    }
                    if !far.is_finite() || far <= near {
                        return Err(format!(
                            "{context} `fade` component {i} far_radius ({far:?}) must be a \
                             finite number greater than near_radius ({near:?})"
                        ));
                    }
                    for (name, value) in [
                        ("fade_out_seconds", def.fade_out_seconds),
                        ("fade_in_seconds", def.fade_in_seconds),
                    ] {
                        if let Some(value) = value
                            && (!value.is_finite()
                                || value <= 0.0
                                || value > crate::level::MAX_FADE_SECONDS)
                        {
                            return Err(format!(
                                "{context} `fade` component {i} {name} ({value:?}) must be a \
                                 finite number between 0 and {}",
                                crate::level::MAX_FADE_SECONDS
                            ));
                        }
                    }
                }
                (Some(_), None) => {
                    return Err(format!(
                        "{context} `fade` component {i} authors near_radius without far_radius"
                    ));
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "{context} `fade` component {i} authors far_radius without near_radius"
                    ));
                }
            }
        }
        ComponentDef::Glow(def) => {
            if !def
                .color
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
            {
                return Err(format!(
                    "{context} `glow` component {i} color ({:?}) must be three finite channels \
                     between 0 and 1",
                    def.color
                ));
            }
            if !def.intensity.is_finite()
                || !(0.0..=crate::level::MAX_GLOW_INTENSITY).contains(&def.intensity)
            {
                return Err(format!(
                    "{context} `glow` component {i} intensity ({:?}) must be a finite number \
                     between 0 and {}",
                    def.intensity,
                    crate::level::MAX_GLOW_INTENSITY
                ));
            }
            if !def.range.is_finite()
                || !(crate::level::MIN_GLOW_RANGE_M..=crate::level::MAX_GLOW_RANGE_M)
                    .contains(&def.range)
            {
                return Err(format!(
                    "{context} `glow` component {i} range ({:?}) must be a finite number \
                     between {} and {} metres",
                    def.range,
                    crate::level::MIN_GLOW_RANGE_M,
                    crate::level::MAX_GLOW_RANGE_M
                ));
            }
            if def
                .socket
                .as_deref()
                .is_some_and(|socket| socket.trim().is_empty())
            {
                return Err(format!(
                    "{context} `glow` component {i} socket must not be blank when specified"
                ));
            }
            if !def
                .offset
                .iter()
                .all(|axis| axis.is_finite() && axis.abs() <= crate::level::MAX_GLOW_OFFSET_M)
            {
                return Err(format!(
                    "{context} `glow` component {i} offset ({:?}) must be three finite numbers \
                     within +/-{} metres",
                    def.offset,
                    crate::level::MAX_GLOW_OFFSET_M
                ));
            }
        }
        ComponentDef::State { .. }
        | ComponentDef::Steam { .. }
        | ComponentDef::Water { .. }
        | ComponentDef::NavObstacle { .. } => {}
    }
    Ok(())
}

/// One record's authored bindings and the label its diagnostics use.
struct AuthoredBindings<'a> {
    /// Stable instance id the record's own actions act on (a spawn template id
    /// for template-born instances).
    id: String,
    /// Authored record label, e.g. `Prop 3`.
    label: String,
    /// The bindings, in authored order.
    bindings: &'a [EventBindingDef],
}

/// Every record that carries bindings, in authored order: props, doors,
/// fixtures, volumes, timers, spawn points and spawn templates. Effects are
/// validated separately because an effect id is optional and not part of the
/// instance namespace.
fn authored_bindings(level: &LevelDef) -> Vec<AuthoredBindings<'_>> {
    let mut records = Vec::new();
    let prop_ids = level.prop_instance_ids();
    for (i, prop) in level.props.iter().enumerate() {
        if prop.bindings.is_empty() {
            continue;
        }
        if let Some(id) = prop_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Prop {i}"),
                bindings: &prop.bindings,
            });
        }
    }
    let light_ids = level.light_instance_ids();
    for (i, fixture) in level.ceiling_lights.iter().enumerate() {
        if fixture.bindings.is_empty() {
            continue;
        }
        if let Some(id) = light_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Ceiling light {i}"),
                bindings: &fixture.bindings,
            });
        }
    }
    let door_ids = level.door_instance_ids();
    for (i, door) in level.doors.iter().enumerate() {
        if door.bindings.is_empty() {
            continue;
        }
        if let Some(id) = door_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Door {i}"),
                bindings: &door.bindings,
            });
        }
    }
    let volume_ids = volume_instance_ids(level);
    for (i, volume) in level.volumes.iter().enumerate() {
        if volume.bindings.is_empty() {
            continue;
        }
        if let Some(id) = volume_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Trigger volume {i}"),
                bindings: &volume.bindings,
            });
        }
    }
    for (i, timer) in level.timers.iter().enumerate() {
        if timer.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: timer.id.trim().to_string(),
            label: format!("Timer {i}"),
            bindings: &timer.bindings,
        });
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        if point.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: point.id.trim().to_string(),
            label: format!("Spawn point {i}"),
            bindings: &point.bindings,
        });
    }
    for (i, template) in level.spawn_templates.iter().enumerate() {
        if template.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: template.id.trim().to_string(),
            label: format!("Spawn template {i}"),
            bindings: &template.bindings,
        });
    }
    records
}

/// Every authored binding, on every record that carries one.
fn validate_bindings(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    for record in authored_bindings(level) {
        let Some(facts) = index.facts_of(&record.id) else {
            continue;
        };
        let context = format!("{} (`{}`)", record.label, record.id);
        validate_entity_bindings(&context, &record.id, record.bindings, facts, index)?;
    }
    for (i, effect) in level.effects.iter().enumerate() {
        if effect.bindings.is_empty() {
            continue;
        }
        let facts = EntityFacts::of("effect");
        let label = effect
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(
                || format!("Effect {i}"),
                |id| format!("Effect {i} (`{id}`)"),
            );
        validate_entity_bindings(&label, "the effect", &effect.bindings, &facts, index)?;
    }
    Ok(())
}

/// One record's bindings: bounded, and every one can actually fire on it.
fn validate_entity_bindings(
    context: &str,
    actor: &str,
    bindings: &[EventBindingDef],
    facts: &EntityFacts<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    if bindings.len() > MAX_BINDINGS_PER_ENTITY {
        return Err(format!(
            "{context} declares {} bindings; the limit is {MAX_BINDINGS_PER_ENTITY}",
            bindings.len()
        ));
    }
    for (i, binding) in bindings.iter().enumerate() {
        if let Some(id) = binding.id.as_deref()
            && id.trim().is_empty()
        {
            return Err(format!(
                "{context} binding {i} id must not be blank when specified"
            ));
        }
        let binding_context = binding
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(
                || format!("{context} binding {i}"),
                |id| format!("{context} binding {i} (`{id}`)"),
            );
        if !binding.cooldown_seconds.is_finite() || binding.cooldown_seconds < 0.0 {
            return Err(format!(
                "{binding_context} cooldown_seconds must be a finite number that is not \
                 negative (found {:?})",
                binding.cooldown_seconds
            ));
        }
        match binding.on {
            EventKindName::EnterVolume | EventKindName::ExitVolume => {
                if !facts.is_volume {
                    return Err(format!(
                        "{binding_context} listens for `{}`, but {context} is not an \
                         authored trigger volume",
                        binding.on.name()
                    ));
                }
            }
            EventKindName::AnimationComplete => {
                if !facts.has_animation {
                    return Err(format!(
                        "{binding_context} listens for `animation_complete`, but {context} \
                         has no `animation` component"
                    ));
                }
            }
            EventKindName::Interact => {
                if !facts.has_interactable || !facts.interactable_enabled {
                    return Err(format!(
                        "{binding_context} listens for `interact`, but {context} has no \
                         enabled `interactable` component; a disabled interactable can \
                         never emit the event"
                    ));
                }
            }
            // A `timer` event may come from an authored timer *or* from a
            // sequence's `emit` step, which uses the same vocabulary as one
            // generic cue channel; any record may listen for it. The same
            // holds for the remaining kinds, which an `emit` step can publish.
            EventKindName::Timer
            | EventKindName::ObjectState
            | EventKindName::SequenceComplete
            | EventKindName::Spawn
            | EventKindName::AiState
            | EventKindName::Caught => {}
        }
        validate_conditions(&binding_context, &binding.when, index)?;
        validate_action_list(
            &binding_context,
            &binding.actions,
            &ImplicitTarget::Actor(actor, facts),
            index,
        )?;
    }
    Ok(())
}

/// Every condition in one binding: resolvable, and of the right kind.
fn validate_conditions(
    context: &str,
    conditions: &[ConditionDef],
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    for (i, condition) in conditions.iter().enumerate() {
        let condition_context = format!("{context} condition {i} (`{}`)", condition.kind());
        let target = condition.target().trim();
        if target.is_empty() {
            return Err(format!("{condition_context} target must not be blank"));
        }
        let Some(facts) = index.entities.get(target) else {
            return Err(format!(
                "{condition_context} targets unknown entity `{target}`"
            ));
        };
        match condition {
            ConditionDef::State { name, .. } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(format!("{condition_context} names no state"));
                }
                if !facts.state_names.contains(name) {
                    return Err(format!(
                        "{condition_context} reads state `{name}` on `{target}`, which does \
                         not author that state"
                    ));
                }
            }
            ConditionDef::Locked { .. }
            | ConditionDef::Unlocked { .. }
            | ConditionDef::DoorOpen { .. }
            | ConditionDef::DoorClosed { .. } => {
                if !facts.is_door {
                    return Err(format!(
                        "{condition_context} targets `{target}`, a {}; locked/unlocked and \
                         door state conditions need a door",
                        facts.kind
                    ));
                }
            }
            ConditionDef::SequenceRunning { .. }
            | ConditionDef::SequenceIdle { .. }
            | ConditionDef::Enabled { .. }
            | ConditionDef::Disabled { .. } => {}
        }
    }
    Ok(())
}

/// The implicit target an action acts on when it omits `target`.
enum ImplicitTarget<'a> {
    /// A binding's own record; a spawn template binding passes the template's
    /// component facts, which are the facts its instances are born with.
    Actor(&'a str, &'a EntityFacts<'a>),
    /// Every entity a sequence step can run on, discovered statically; empty
    /// when no `start_sequence` names the sequence on an authored entity.
    Owners(&'a [String]),
}

/// One action's diagnostic identity: where it was authored and which action it
/// is.
struct ActionContext<'a> {
    /// Owning record context, e.g. ``Prop 3 (`switch`) binding 0``.
    context: &'a str,
    /// Position of the action in its binding or step list.
    index: usize,
    /// Serialized action tag, e.g. `open`.
    kind: &'a str,
}

/// The `context` action `index` (`kind`) prefix every action error starts with.
fn action_context_label(action_context: &ActionContext<'_>) -> String {
    format!(
        "{} action {} (`{}`)",
        action_context.context, action_context.index, action_context.kind
    )
}

/// One source's action list: bounded, non-empty and composed only of
/// implemented actions with resolvable, capability-compatible targets.
fn validate_action_list(
    context: &str,
    actions: &[ActionDef],
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    if actions.is_empty() {
        return Err(format!("{context} must declare at least one action"));
    }
    if actions.len() > MAX_ACTIONS_PER_SOURCE {
        return Err(format!(
            "{context} declares {} actions; the limit is {MAX_ACTIONS_PER_SOURCE}",
            actions.len()
        ));
    }
    for (i, action) in actions.iter().enumerate() {
        let action_context = ActionContext {
            context,
            index: i,
            kind: action.kind(),
        };
        validate_action(&action_context, action, implicit, index)?;
    }
    Ok(())
}

/// One action: every target resolves and the action fits the target's
/// capabilities.
#[allow(clippy::too_many_lines)] // one match arm per action kind
fn validate_action(
    action_context: &ActionContext<'_>,
    action: &ActionDef,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    match action {
        ActionDef::ResetToStart => {}
        ActionDef::Open { target }
        | ActionDef::Close { target }
        | ActionDef::Lock { target }
        | ActionDef::Unlock { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a door target",
            |facts| facts.is_door,
        )?,
        ActionDef::Toggle { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a door, or a switchable ceiling fixture",
            |facts| facts.is_door || (facts.light_capable() && facts.light_switchable),
        )?,
        ActionDef::Enable { target } | ActionDef::Disable { target } => {
            validate_explicit_target(action_context, target.as_deref(), index)?;
        }
        ActionDef::SetLight { target, .. } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a switchable light (a ceiling fixture with `switchable: true`); a \
             non-switchable light is baked once and cannot change",
            |facts| facts.light_capable() && facts.light_switchable,
        )?,
        ActionDef::PlayAnimation { target, clip, .. }
        | ActionDef::ToggleAnimation { target, clip } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a target with an `animation` component",
                |facts| facts.has_animation,
            )?;
            if clip.as_deref().is_some_and(|clip| clip.trim().is_empty()) {
                return Err(format!(
                    "{} clip must not be blank when specified",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::PlaySound { target, sound, .. } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a target with an `audio` component",
                |facts| facts.has_audio,
            )?;
            if sound.as_ref().is_some_and(|sound| sound.trim().is_empty()) {
                return Err(format!(
                    "{} `sound` must not be blank when specified",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::StopSound { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a target with an `audio` component",
            |facts| facts.has_audio,
        )?,
        ActionDef::ChangeMaterial { target, variant } => {
            let variant = variant.trim();
            if variant.is_empty() {
                return Err(format!(
                    "{} needs a non-empty material variant name",
                    action_context_label(action_context)
                ));
            }
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a runtime instance of a spawn template with a `material` component (a \
                 baked static prop's material is prepared geometry and cannot change)",
                |facts| facts.is_spawn_template && facts.has_material,
            )?;
            check_action_variant(action_context, target.as_deref(), implicit, index, variant)?;
        }
        ActionDef::MoveObject {
            target,
            x,
            y,
            z,
            speed,
        } => {
            if !x.is_finite() || !z.is_finite() || y.is_some_and(|y| !y.is_finite()) {
                return Err(format!(
                    "{} coordinates must be finite numbers",
                    action_context_label(action_context)
                ));
            }
            if let Some(speed) = speed
                && (!speed.is_finite() || *speed <= 0.0)
            {
                return Err(format!(
                    "{} speed must be a finite positive number of metres per second",
                    action_context_label(action_context)
                ));
            }
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a runtime instance of a spawn template (a baked static prop and a door \
                 cannot move; drive a door with `open`/`close`/`toggle`)",
                |facts| facts.is_spawn_template,
            )?;
        }
        ActionDef::SetState { target, name, .. } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(format!(
                    "{} needs a non-empty state name",
                    action_context_label(action_context)
                ));
            }
            let requirement = format!(
                "an authored state named `{name}` (or a timer/trigger-volume target: the \
                 runtime owns those states); a `set_state` may only change a state the \
                 target already authors"
            );
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                &requirement,
                |facts| facts.state_names.contains(name) || facts.is_timer || facts.is_volume,
            )?;
        }
        ActionDef::ToggleLabel { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a placed prop target (only a placed prop shows a label)",
            |facts| facts.is_prop,
        )?,
        ActionDef::StartSequence { sequence, target } => {
            let sequence = sequence.trim();
            if sequence.is_empty() {
                return Err(format!(
                    "{} needs a non-empty sequence id",
                    action_context_label(action_context)
                ));
            }
            if !index.sequences.contains(sequence) {
                return Err(format!(
                    "{} references unknown sequence `{sequence}`",
                    action_context_label(action_context)
                ));
            }
            for_each_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                |_subject, _facts| Ok(()),
            )?;
        }
        ActionDef::StopSequence { target } => {
            for_each_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                |_subject, _facts| Ok(()),
            )?;
        }
        ActionDef::StartTimer {
            target, seconds, ..
        } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a timer target",
                |facts| facts.is_timer,
            )?;
            if let Some(seconds) = seconds
                && (!seconds.is_finite() || *seconds <= 0.0)
            {
                return Err(format!(
                    "{} seconds override must be a finite positive number of seconds",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::StopTimer { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a timer target",
            |facts| facts.is_timer,
        )?,
        ActionDef::SpawnEntity {
            template,
            point,
            group,
            name,
        } => {
            let spawn = SpawnAction {
                template: template.as_deref(),
                point: point.as_deref(),
                group: group.as_deref(),
                name: name.as_deref(),
            };
            validate_spawn_action(action_context, &spawn, implicit, index)?;
        }
        ActionDef::DespawnEntity { target } => {
            let target = target.trim();
            if target.is_empty() {
                return Err(format!(
                    "{} target must not be blank",
                    action_context_label(action_context)
                ));
            }
            if !index.entities.contains_key(target) && !index.spawn_groups.contains(target) {
                return Err(format!(
                    "{} targets `{target}`; `despawn_entity` needs an authored entity id \
                     or spawn group id",
                    action_context_label(action_context)
                ));
            }
        }
    }
    Ok(())
}

/// Resolves one action's optional target and runs `check` on every entity it
/// can act on.
///
/// An explicit target must resolve to an authored record or spawn template; an
/// omitted target is the acting record for a binding, or every owner a
/// sequence can run on for a sequence step. A sequence step whose sequence is
/// never started on any authored entity has nothing to act on and is rejected.
fn for_each_action_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    mut check: impl FnMut(&str, &EntityFacts<'_>) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(raw) = target {
        let resolved = raw.trim();
        if resolved.is_empty() {
            return Err(format!(
                "{} target must not be blank",
                action_context_label(action_context)
            ));
        }
        let Some(facts) = index.facts_of(resolved) else {
            return Err(format!(
                "{} targets unknown entity `{resolved}`",
                action_context_label(action_context)
            ));
        };
        return check(resolved, facts);
    }
    match implicit {
        ImplicitTarget::Actor(actor, facts) => check(actor, facts),
        ImplicitTarget::Owners(owners) => {
            if owners.is_empty() {
                return Err(format!(
                    "{} needs a `target`: the sequence is not started on any authored entity",
                    action_context_label(action_context)
                ));
            }
            for owner in *owners {
                let Some(facts) = index.facts_of(owner) else {
                    return Err(format!(
                        "{} runs on unknown entity `{owner}`",
                        action_context_label(action_context)
                    ));
                };
                check(owner, facts)?;
            }
            Ok(())
        }
    }
}

/// One action's target capability: every resolved target must satisfy
/// `satisfies`, and the failure names the record and what the action needs.
fn check_action_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    requirement: &str,
    satisfies: impl Fn(&EntityFacts<'_>) -> bool,
) -> Result<(), String> {
    for_each_action_target(action_context, target, implicit, index, |subject, facts| {
        if satisfies(facts) {
            return Ok(());
        }
        Err(format!(
            "{} targets `{subject}`, a {}; `{}` requires {requirement}",
            action_context_label(action_context),
            facts.kind,
            action_context.kind
        ))
    })
}

/// One `change_material` target's variant: the named variant must be one the
/// target's `material` component declares, or the runtime selection no-ops.
fn check_action_variant(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    variant: &str,
) -> Result<(), String> {
    for_each_action_target(action_context, target, implicit, index, |subject, facts| {
        if facts.material_variants.contains(&variant) {
            return Ok(());
        }
        Err(format!(
            "{} targets `{subject}`, a {}; its `material` component declares no variant \
             `{variant}`",
            action_context_label(action_context),
            facts.kind
        ))
    })
}

/// An action whose omitted target is always legal: an explicit target, when
/// present, must still resolve.
fn validate_explicit_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    let Some(raw) = target else {
        return Ok(());
    };
    let resolved = raw.trim();
    if resolved.is_empty() {
        return Err(format!(
            "{} target must not be blank",
            action_context_label(action_context)
        ));
    }
    if !index.entities.contains_key(resolved) && !index.spawn_templates.contains_key(resolved) {
        return Err(format!(
            "{} targets unknown entity `{resolved}`",
            action_context_label(action_context)
        ));
    }
    Ok(())
}

/// The authored fields of one `spawn_entity` action.
struct SpawnAction<'a> {
    /// Spawn template id override.
    template: Option<&'a str>,
    /// Spawn point to instantiate at.
    point: Option<&'a str>,
    /// Spawn group override.
    group: Option<&'a str>,
    /// Runtime name for the spawned instance.
    name: Option<&'a str>,
}

/// A `spawn_entity` action: the point resolves, the template exists and any
/// named group exists.
///
/// A template without a point can never resolve, because a template alone has
/// no world position; only the spawn point carries one. With neither field the
/// acting record must itself be a spawn point, whose authored template is
/// used.
fn validate_spawn_action(
    action_context: &ActionContext<'_>,
    spawn: &SpawnAction<'_>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    let label = action_context_label(action_context);
    let template = spawn.template.map(str::trim).filter(|id| !id.is_empty());
    if spawn.template.is_some() && template.is_none() {
        return Err(format!(
            "{label} `template` must not be blank when specified"
        ));
    }
    if let Some(template) = template
        && !index.spawn_templates.contains_key(template)
    {
        return Err(format!(
            "{label} references unknown spawn template `{template}`"
        ));
    }
    if let Some(group) = spawn.group {
        let group = group.trim();
        if group.is_empty() {
            return Err(format!("{label} `group` must not be blank when specified"));
        }
        if !index.spawn_groups.contains(group) {
            return Err(format!("{label} references unknown spawn group `{group}`"));
        }
    }
    if let Some(name) = spawn.name
        && name.trim().is_empty()
    {
        return Err(format!(
            "{label} runtime `name` must not be blank when specified"
        ));
    }
    let point = spawn.point.map(str::trim).filter(|id| !id.is_empty());
    if spawn.point.is_some() && point.is_none() {
        return Err(format!("{label} `point` must not be blank when specified"));
    }
    if let Some(point) = point {
        if !index.spawn_points.contains(point) {
            return Err(format!("{label} targets unknown spawn point `{point}`"));
        }
        return Ok(());
    }
    if template.is_some() {
        return Err(format!(
            "{label} names a `template` without a `point`; a template alone has no world \
             position, so name the authored spawn point to spawn at"
        ));
    }
    for_each_action_target(action_context, None, implicit, index, |subject, facts| {
        if facts.is_spawn_point {
            return Ok(());
        }
        Err(format!(
            "{label} targets `{subject}`, a {}; a spawn without a `point` may only come \
             from a spawn point",
            facts.kind
        ))
    })
}

/// Adds `owner` to `sequence`'s owner list when both are known.
///
/// Returns true when the list changed, so the fixpoint can stop early.
fn record_sequence_owner(
    owners: &mut HashMap<String, Vec<String>>,
    sequence: &str,
    owner: &str,
) -> bool {
    let sequence = sequence.trim();
    let Some(list) = owners.get_mut(sequence) else {
        return false; // an unknown sequence is a named error elsewhere
    };
    if list.iter().any(|existing| existing == owner) {
        return false;
    }
    list.push(owner.to_string());
    true
}

/// Every entity each sequence can run on, gathered from the authored
/// `start_sequence` sites.
///
/// A binding starts a sequence on its own record when the action omits the
/// target; a sequence step starts one on the sequence's own owner. The sites
/// form a bounded fixpoint, so a sequence that starts another with an omitted
/// target propagates its own owners.
fn collect_sequence_owners(level: &LevelDef) -> HashMap<String, Vec<String>> {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for sequence in &level.sequences {
        owners.entry(sequence.id.trim().to_string()).or_default();
    }
    let mut deferred: Vec<(String, String)> = Vec::new();
    for record in authored_bindings(level) {
        for binding in record.bindings {
            for action in &binding.actions {
                if let ActionDef::StartSequence { sequence, target } = action {
                    match target.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
                        Some(target) => {
                            record_sequence_owner(&mut owners, sequence, target);
                        }
                        None => {
                            record_sequence_owner(&mut owners, sequence, &record.id);
                        }
                    }
                }
            }
        }
    }
    for sequence in &level.sequences {
        let from = sequence.id.trim().to_string();
        for step in &sequence.steps {
            if let SequenceStepDef::Action {
                action:
                    ActionDef::StartSequence {
                        sequence: to,
                        target,
                    },
            } = step
            {
                match target.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
                    Some(target) => {
                        record_sequence_owner(&mut owners, to, target);
                    }
                    None => {
                        deferred.push((from.clone(), to.trim().to_string()));
                    }
                }
            }
        }
    }
    let mut passes = 0usize;
    while !deferred.is_empty() && passes <= owners.len() {
        let mut changed = false;
        for (from, to) in &deferred {
            let source = owners.get(from).cloned().unwrap_or_default();
            for owner in source {
                changed |= record_sequence_owner(&mut owners, to, &owner);
            }
        }
        if !changed {
            break;
        }
        passes = passes.saturating_add(1);
    }
    owners
}

/// Authored sequences: bounded steps, finite and ranged delays, and every step
/// action resolvable against the sequence's owners.
///
/// A `wait_animation` step's clip is only checked to be a non-empty name: a
/// model's clip set is not known to the loader, so the step's `timeout` is the
/// runtime bound that keeps a missing clip from stranding the sequence.
#[allow(clippy::too_many_lines)] // one match arm per step kind
fn validate_sequences(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    if level.sequences.len() > MAX_LEVEL_SEQUENCES {
        return Err(format!(
            "Level contains too many sequences: {} (limit: {MAX_LEVEL_SEQUENCES})",
            level.sequences.len()
        ));
    }
    for (i, sequence) in level.sequences.iter().enumerate() {
        let id = sequence.id.trim();
        let context = format!("Sequence {i} (`{id}`)");
        if sequence.steps.is_empty() {
            return Err(format!("{context} declares no steps"));
        }
        if sequence.steps.len() > MAX_SEQUENCE_STEPS {
            return Err(format!(
                "{context} declares {} steps; the limit is {MAX_SEQUENCE_STEPS}",
                sequence.steps.len()
            ));
        }
        let owners: &[String] = index.sequence_owners.get(id).map_or(&[], Vec::as_slice);
        for (j, step) in sequence.steps.iter().enumerate() {
            let step_context = format!("{context} step {j} (`{}`)", step.kind());
            match step {
                SequenceStepDef::Action { action } => {
                    let action_context = ActionContext {
                        context: &step_context,
                        index: 0,
                        kind: action.kind(),
                    };
                    validate_action(
                        &action_context,
                        action,
                        &ImplicitTarget::Owners(owners),
                        index,
                    )?;
                }
                SequenceStepDef::Wait { seconds } => {
                    if !seconds.is_finite() || *seconds < 0.0 || *seconds > MAX_SEQUENCE_WAIT_S {
                        return Err(format!(
                            "{step_context} seconds ({seconds:?}) must be a finite number \
                             between 0 and {MAX_SEQUENCE_WAIT_S}"
                        ));
                    }
                }
                SequenceStepDef::Move { x, y, z, speed } => {
                    if !x.is_finite()
                        || !z.is_finite()
                        || y.is_some_and(|y| !y.is_finite())
                        || !speed.is_finite()
                        || *speed <= 0.0
                    {
                        return Err(format!(
                            "{step_context} position and speed must be finite, and speed \
                             positive"
                        ));
                    }
                }
                SequenceStepDef::Face { yaw_degrees } => {
                    if !yaw_degrees.is_finite() {
                        return Err(format!("{step_context} yaw must be a finite number"));
                    }
                }
                SequenceStepDef::WaitAnimation { clip, timeout } => {
                    if clip.as_deref().is_some_and(|clip| clip.trim().is_empty()) {
                        return Err(format!(
                            "{step_context} clip must not be blank when specified"
                        ));
                    }
                    if !timeout.is_finite()
                        || *timeout < 0.0
                        || *timeout > MAX_SEQUENCE_ANIMATION_TIMEOUT_S
                    {
                        return Err(format!(
                            "{step_context} timeout ({timeout:?}) must be a finite number \
                             between 0 and {MAX_SEQUENCE_ANIMATION_TIMEOUT_S}"
                        ));
                    }
                }
                SequenceStepDef::Emit { key, .. } => {
                    if key.as_deref().is_some_and(|key| key.trim().is_empty()) {
                        return Err(format!(
                            "{step_context} key must not be blank when specified"
                        ));
                    }
                }
                SequenceStepDef::SetState { name, .. } => {
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(format!("{step_context} must name a non-empty state"));
                    }
                    for owner in owners {
                        if let Some(facts) = index.facts_of(owner)
                            && !facts.state_names.contains(name)
                            && !facts.is_timer
                            && !facts.is_volume
                        {
                            return Err(format!(
                                "{step_context} writes state `{name}` on `{owner}`, a {}, \
                                 which does not author it; a sequence state write may only \
                                 change a state its owner already authors",
                                facts.kind
                            ));
                        }
                    }
                }
                SequenceStepDef::Stop => {}
            }
        }
    }
    Ok(())
}

/// Authored spawn templates, points and groups: resolvable, finite and
/// bounded.
///
/// An `at_most_one_active` group is deliberately allowed to be shared by
/// several spawn points: that is the encounter mechanism (several candidate
/// spots, one live member), and the runtime refuses a second live member, so
/// no check is needed here.
fn validate_spawns(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    if level.spawn_templates.len() > MAX_LEVEL_SPAWN_TEMPLATES {
        return Err(format!(
            "Level contains too many spawn templates: {} (limit: {MAX_LEVEL_SPAWN_TEMPLATES})",
            level.spawn_templates.len()
        ));
    }
    if level.spawn_points.len() > MAX_LEVEL_SPAWN_POINTS {
        return Err(format!(
            "Level contains too many spawn points: {} (limit: {MAX_LEVEL_SPAWN_POINTS})",
            level.spawn_points.len()
        ));
    }
    if level.spawn_groups.len() > MAX_LEVEL_SPAWN_GROUPS {
        return Err(format!(
            "Level contains too many spawn groups: {} (limit: {MAX_LEVEL_SPAWN_GROUPS})",
            level.spawn_groups.len()
        ));
    }
    for (i, template) in level.spawn_templates.iter().enumerate() {
        let id = template.id.trim();
        let context = format!("Spawn template {i} (`{id}`)");
        if template.model.trim().is_empty() {
            return Err(format!("{context} must reference a non-empty model id"));
        }
        if !template.scale.is_finite() || template.scale <= 0.0 {
            return Err(format!("{context} scale must be a finite positive number"));
        }
        if let Some(seconds) = template.lifetime_seconds
            && (!seconds.is_finite() || seconds <= 0.0)
        {
            return Err(format!(
                "{context} lifetime_seconds must be a finite positive number when specified"
            ));
        }
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        let id = point.id.trim();
        let context = format!("Spawn point {i} (`{id}`)");
        if !point.x.is_finite()
            || !point.z.is_finite()
            || !point.yaw_degrees.is_finite()
            || point.y.is_some_and(|y| !y.is_finite())
        {
            return Err(format!("{context} position and yaw must be finite numbers"));
        }
        let template = point.template.trim();
        if template.is_empty() {
            return Err(format!(
                "{context} must reference a non-empty spawn template id"
            ));
        }
        if !index.spawn_templates.contains_key(template) {
            return Err(format!(
                "{context} references unknown spawn template `{template}`"
            ));
        }
        if let Some(group) = point.group.as_deref() {
            let group = group.trim();
            if group.is_empty() {
                return Err(format!("{context} group must not be blank when specified"));
            }
            if !index.spawn_groups.contains(group) {
                return Err(format!(
                    "{context} references unknown spawn group `{group}`"
                ));
            }
        }
    }
    Ok(())
}

/// Authored timers: a positive, finite period.
fn validate_timers(level: &LevelDef) -> Result<(), String> {
    for (i, timer) in level.timers.iter().enumerate() {
        let id = timer.id.trim();
        let context = format!("Timer {i} (`{id}`)");
        if !timer.seconds.is_finite() || timer.seconds <= 0.0 {
            return Err(format!(
                "{context} seconds must be a finite positive number of seconds"
            ));
        }
    }
    Ok(())
}

/// Upper bound on the depth of the zero-delay cycle search.
///
/// A deeper chain is left to the runtime's chain budget rather than rejecting
/// a map the compiler cannot see whole.
const MAX_CYCLE_DEPTH: usize = 26;
/// Upper bound on the nodes one zero-delay cycle search may visit.
const MAX_CYCLE_VISITS: usize = 100_000;

/// One node of the zero-delay causality graph.
#[derive(Clone, PartialEq, Eq, Hash)]
enum CycleNode {
    /// One binding on one owner: its position in the record's list and the
    /// owner's instance id.
    Binding { owner: String, index: usize },
    /// An authored sequence.
    Sequence { id: String },
}

impl CycleNode {
    /// Diagnostic label naming the node.
    fn label(&self) -> String {
        match self {
            Self::Binding { owner, index } => format!("binding {index} on `{owner}`"),
            Self::Sequence { id } => format!("sequence `{id}`"),
        }
    }
}

/// One binding node's cycle-relevant fields.
struct CycleBinding<'a> {
    /// Instance id the binding lives on.
    owner: String,
    /// Position of the binding in its record's list.
    index: usize,
    /// Event kind the binding listens for.
    on: EventKindName,
    /// True when the binding itself may re-enter; a `once`, a cooldown or a
    /// condition can stop a repetition, so the search does not traverse it.
    traversable: bool,
    /// The binding's actions, in order.
    actions: &'a [ActionDef],
}

/// True when a sequence can complete without a delayed step and therefore
/// emits `sequence_complete` in the same immediate chain.
fn sequence_completes_immediately(sequence: &SequenceDef) -> bool {
    for step in &sequence.steps {
        if step.is_delayed() {
            return false;
        }
        if matches!(step, SequenceStepDef::Stop) {
            return true;
        }
    }
    !sequence.looped
}

/// Rejects the obvious zero-delay cycles: a binding that starts a sequence
/// whose first steps (before any wait, move, face, animation wait or timer)
/// start a sequence again, or complete and re-enter the same binding.
///
/// The search is deliberately conservative: a binding with a `when` condition,
/// `once` or a cooldown is not traversed (each of those can stop the
/// repetition), `emit`, `start_timer` and automatic sequence completion are
/// treated as consuming time, and the walk is bounded by [`MAX_CYCLE_DEPTH`]
/// and [`MAX_CYCLE_VISITS`], so a chain the compiler cannot see whole is left
/// to the runtime's own chain budget instead of rejecting a legitimate map.
#[allow(clippy::too_many_lines)] // one linear graph construction pass
fn validate_zero_delay_cycles(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    let sequence_by_id: HashMap<&str, &SequenceDef> = level
        .sequences
        .iter()
        .map(|sequence| (sequence.id.trim(), sequence))
        .collect();
    let records = authored_bindings(level);
    let mut infos: Vec<CycleBinding<'_>> = Vec::new();
    for record in &records {
        for (i, binding) in record.bindings.iter().enumerate() {
            infos.push(CycleBinding {
                owner: record.id.clone(),
                index: i,
                on: binding.on,
                traversable: !binding.once
                    && binding.cooldown_seconds <= 0.0
                    && binding.when.is_empty(),
                actions: &binding.actions,
            });
        }
    }
    let mut completion_bindings: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        if info.on == EventKindName::SequenceComplete {
            completion_bindings
                .entry(info.owner.as_str())
                .or_default()
                .push(i);
        }
    }
    let mut edges: HashMap<CycleNode, Vec<CycleNode>> = HashMap::new();
    let mut order: Vec<CycleNode> = Vec::new();
    for info in &infos {
        let node = CycleNode::Binding {
            owner: info.owner.clone(),
            index: info.index,
        };
        order.push(node.clone());
        let mut out = Vec::new();
        if info.traversable {
            for action in info.actions {
                if let ActionDef::StartSequence { sequence, .. } = action {
                    let sequence = sequence.trim();
                    if sequence_by_id.contains_key(sequence) {
                        out.push(CycleNode::Sequence {
                            id: sequence.to_string(),
                        });
                    }
                }
            }
        }
        edges.insert(node, out);
    }
    for sequence in &level.sequences {
        let id = sequence.id.trim();
        let node = CycleNode::Sequence { id: id.to_string() };
        order.push(node.clone());
        let mut out = Vec::new();
        for step in &sequence.steps {
            if step.is_delayed() || matches!(step, SequenceStepDef::Stop) {
                break;
            }
            if let SequenceStepDef::Action {
                action: ActionDef::StartSequence { sequence: to, .. },
            } = step
            {
                let to = to.trim();
                if sequence_by_id.contains_key(to) {
                    out.push(CycleNode::Sequence { id: to.to_string() });
                }
            }
        }
        if sequence_completes_immediately(sequence)
            && let Some(owners) = index.sequence_owners.get(id)
        {
            for owner in owners {
                if let Some(list) = completion_bindings.get(owner.as_str()) {
                    for binding_index in list {
                        if let Some(info) = infos.get(*binding_index)
                            && info.traversable
                        {
                            out.push(CycleNode::Binding {
                                owner: info.owner.clone(),
                                index: info.index,
                            });
                        }
                    }
                }
            }
        }
        edges.insert(node, out);
    }
    let mut state: HashMap<CycleNode, u8> = HashMap::new();
    let mut visits = MAX_CYCLE_VISITS;
    for node in &order {
        if state.get(node).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<CycleNode> = Vec::new();
        if let Some(cycle) = cycle_dfs(node, 0, &edges, &mut state, &mut stack, &mut visits) {
            let path = cycle
                .iter()
                .map(CycleNode::label)
                .collect::<Vec<String>>()
                .join(" -> ");
            return Err(format!(
                "Zero-delay cycle: {path}. Each edge runs without consuming a wait, timer \
                 or animation, so the chain would recurse without end; add a delayed step, \
                 a timer, a `once`, a cooldown or a `when` condition."
            ));
        }
    }
    Ok(())
}

/// Depth-first cycle search over the zero-delay graph.
///
/// White nodes are unvisited, grey nodes are on the current path and black
/// nodes are finished. A grey neighbour is a cycle; the path from its first
/// occurrence to the current node is returned. The search stops without a
/// verdict once the depth or visit budget runs out, so it never rejects a map
/// it could not see whole.
fn cycle_dfs(
    node: &CycleNode,
    depth: usize,
    edges: &HashMap<CycleNode, Vec<CycleNode>>,
    state: &mut HashMap<CycleNode, u8>,
    stack: &mut Vec<CycleNode>,
    visits: &mut usize,
) -> Option<Vec<CycleNode>> {
    if *visits == 0 || depth > MAX_CYCLE_DEPTH {
        return None;
    }
    *visits = visits.saturating_sub(1);
    state.insert(node.clone(), 1);
    stack.push(node.clone());
    if let Some(neighbours) = edges.get(node) {
        for next in neighbours {
            match state.get(next).copied().unwrap_or(0) {
                1 => {
                    let position = stack.iter().position(|item| item == next).unwrap_or(0);
                    let mut cycle = stack.get(position..).unwrap_or_default().to_vec();
                    if let Some(first) = cycle.first().cloned() {
                        cycle.push(first);
                    }
                    return Some(cycle);
                }
                0 => {
                    if let Some(cycle) =
                        cycle_dfs(next, depth.saturating_add(1), edges, state, stack, visits)
                    {
                        return Some(cycle);
                    }
                }
                _ => {}
            }
        }
    }
    stack.pop();
    state.insert(node.clone(), 2);
    None
}

/// Floating props: a floating hull cannot be solid, its authored motion must
/// be finite and bounded, it cannot also be routed, and its whole swept
/// footprint must sit inside one water volume.
///
/// The containment rule is what keeps a float off the rim: the level proves at
/// load that even at the heel's extreme the silhouette (half-diagonal plus the
/// heel's horizontal excursion) fits inside the basin rectangle, so the
/// runtime update has no horizontal freedom to misuse.
fn validate_floats(level: &LevelDef) -> Result<(), String> {
    let float_count = level
        .props
        .iter()
        .filter(|prop| prop.float.is_some())
        .count();
    if float_count > crate::level::MAX_LEVEL_FLOAT_PROPS {
        return Err(format!(
            "Level declares {float_count} floating props; the limit is {}",
            crate::level::MAX_LEVEL_FLOAT_PROPS
        ));
    }
    if float_count == 0 {
        return Ok(());
    }
    let water = crate::level::WaterVolumes::from_level(level);
    let ids = level.prop_instance_ids();
    for (index, prop) in level.props.iter().enumerate() {
        let Some(float) = prop.float.as_ref() else {
            continue;
        };
        let context = format!("Float prop {index} (`{}`)", prop.model);
        let (width, height, depth) = validate_float_fields(&context, prop, float)?;
        if let Some(id) = ids.get(index)
            && level.routes.iter().any(|route| route.id.trim() == id)
        {
            return Err(format!(
                "{context} is addressed by a route; a floating prop cannot be routed"
            ));
        }
        let half_diagonal = 0.5 * width.hypot(depth);
        let heel_excursion = 0.5 * height * float.heel_degrees.to_radians().sin();
        let radius = half_diagonal + heel_excursion;
        if !water.contains_disc(prop.x, prop.z, radius) {
            return Err(format!(
                "{context} must be fully inside a water volume: its swept footprint \
                 (radius {radius:.3} m) is not contained at ({}, {})",
                prop.x, prop.z
            ));
        }
    }
    Ok(())
}

/// One float prop's own fields: solid/footprint/motion bounds.
///
/// Returns the scaled `(width, height, depth)` the containment rule needs.
fn validate_float_fields(
    context: &str,
    prop: &crate::level::PropDef,
    float: &crate::level::PropFloatDef,
) -> Result<(f32, f32, f32), String> {
    if prop.solid {
        return Err(format!(
            "{context} must set `solid: false`: a floating hull cannot leave a static collider"
        ));
    }
    if !prop.x.is_finite() || !prop.z.is_finite() || !prop.scale.is_finite() || prop.scale <= 0.0 {
        return Err(format!(
            "{context} position and scale must be finite and its scale positive"
        ));
    }
    let Some(size) = prop.size else {
        return Err(format!(
            "{context} must author `size`: the float contract is validated against its footprint"
        ));
    };
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(format!(
            "{context} size must be three positive finite numbers"
        ));
    }
    let width = size[0] * prop.scale;
    let height = size[1] * prop.scale;
    let depth = size[2] * prop.scale;
    if !float.draft.is_finite() || float.draft <= 0.0 || float.draft >= height {
        return Err(format!(
            "{context} draft must be a finite number above 0 and below its height ({height} m)"
        ));
    }
    if !float.bob.is_finite() || float.bob < 0.0 || float.bob > 0.5 * height {
        return Err(format!(
            "{context} bob must be finite, 0 or positive, and at most half its height"
        ));
    }
    if !float.bob_seconds.is_finite() || float.bob_seconds <= 0.0 {
        return Err(format!(
            "{context} bob_seconds must be a positive finite number"
        ));
    }
    if !float.heel_degrees.is_finite()
        || !(0.0..=crate::level::MAX_FLOAT_HEEL_DEGREES).contains(&float.heel_degrees)
    {
        return Err(format!(
            "{context} heel_degrees must be finite, 0 or positive, and at most {}",
            crate::level::MAX_FLOAT_HEEL_DEGREES
        ));
    }
    if !float.heel_seconds.is_finite() || float.heel_seconds <= 0.0 {
        return Err(format!(
            "{context} heel_seconds must be a positive finite number"
        ));
    }
    if let Some(phase) = float.phase
        && (!phase.is_finite() || !(0.0..=1.0).contains(&phase))
    {
        return Err(format!(
            "{context} phase must be a finite number between 0.0 and 1.0"
        ));
    }
    Ok((width, height, depth))
}

/// Authored entity routes: unique resolving ids, bounded step lists and
/// every waypoint finite, walkable and reachable in a straight line at the
/// entity's own body size.
///
/// A route is a promise that a character can actually walk it, so the checks
/// mirror the runtime: the waypoint must sit on a real walkable surface, each
/// straight segment must stay on the floor without a step taller than the
/// entity can climb, and no wall may block the body anywhere along it. A
/// `solid: true` prop is refused because its own collision box would block
/// its first step.
#[allow(clippy::too_many_lines)] // one cohesive route validation pass
fn validate_routes(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.routes.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_ROUTES {
        return Err(format!(
            "Level contains too many entity routes: {} (limit: {})",
            level.routes.len(),
            crate::level::MAX_LEVEL_ROUTES
        ));
    }
    if level.routes.is_empty() {
        return Ok(());
    }
    let ids = level.prop_instance_ids();
    let id_index: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let walls = level.collision_aabbs();
    let collision_index = crate::collision_index::CollisionIndex::build(&walls);
    let floor = crate::level::WalkableFloor::from_level(level);
    let surfaces = LevelSurfaces::new(level);
    let mut seen: HashSet<String> = HashSet::new();
    for (route_index, route) in level.routes.iter().enumerate() {
        let id = route.id.trim();
        if id.is_empty() {
            return Err(format!("Entity route {route_index} names no instance id"));
        }
        if !seen.insert(id.to_string()) {
            return Err(format!(
                "Entity route {route_index} duplicates instance `{id}`"
            ));
        }
        let Some(&prop_index) = id_index.get(id) else {
            return Err(format!(
                "Entity route {route_index} targets unknown instance `{id}`"
            ));
        };
        if route.steps.is_empty() {
            return Err(format!("Entity route `{id}` declares no steps"));
        }
        if route.steps.len() > crate::level::MAX_ROUTE_STEPS {
            return Err(format!(
                "Entity route `{id}` declares {} steps; the limit is {}",
                route.steps.len(),
                crate::level::MAX_ROUTE_STEPS
            ));
        }
        let Some(prop) = level.props.get(prop_index) else {
            continue;
        };
        if prop.solid {
            return Err(format!(
                "Entity route `{id}` drives a `solid: true` prop; its own collision box \
                 would block every step. Make the entity non-solid."
            ));
        }
        if prop
            .components
            .iter()
            .any(|component| matches!(component, ComponentDef::Ai(_)))
        {
            return Err(format!(
                "Entity route `{id}` drives an entity that also authors an `ai` component; \
                 one entity has one locomotion owner: keep the route or the AI, not both"
            ));
        }
        let size = prop.resolved_size(crate::level::PROP_FALLBACK_SIZE);
        // The authored `nav_agent` body (when present) is validated exactly as
        // the mover uses it; otherwise validation uses the *wider* axis,
        // clamped to the same minimum the runtime disc uses, so an elongated
        // body can never overlap a wall the runtime disc would miss.
        let (radius, body_height, step_height) = prop
            .components
            .iter()
            .find_map(|component| match component {
                ComponentDef::NavAgent {
                    radius,
                    height,
                    step_height,
                    ..
                } => Some((*radius, *height, *step_height)),
                ComponentDef::Interactable { .. }
                | ComponentDef::Animation { .. }
                | ComponentDef::Audio { .. }
                | ComponentDef::Light { .. }
                | ComponentDef::Material { .. }
                | ComponentDef::State { .. }
                | ComponentDef::Lifetime { .. }
                | ComponentDef::Steam { .. }
                | ComponentDef::Water { .. }
                | ComponentDef::NavObstacle { .. }
                | ComponentDef::Fade(_)
                | ComponentDef::Glow(_)
                | ComponentDef::Ai(_) => None,
            })
            .unwrap_or_else(|| {
                (
                    (size[0].max(size[2]) * 0.5).max(crate::entity::ENTITY_MIN_RADIUS_M),
                    size[1].max(0.05),
                    crate::entity::ENTITY_STEP_HEIGHT_M,
                )
            });
        let position = Vec3::new(
            prop.x,
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
            prop.z,
        );
        validate_route_steps(
            id,
            &route.steps,
            position,
            radius,
            body_height,
            step_height,
            &walls,
            &collision_index,
            &floor,
        )?;
    }
    Ok(())
}

/// One route's ordered steps: finite data, real floors and clear straight
/// segments between consecutive waypoints.
#[allow(clippy::too_many_arguments)]
fn validate_route_steps(
    id: &str,
    steps: &[crate::level::RouteStepDef],
    start: Vec3,
    radius: f32,
    body_height: f32,
    step_height: f32,
    walls: &[crate::collision::WallAabb],
    collision_index: &crate::collision_index::CollisionIndex,
    floor: &crate::level::WalkableFloor,
) -> Result<(), String> {
    let mut position = start;
    for (step_index, step) in steps.iter().enumerate() {
        let context = format!("Entity route `{id}` step {step_index}");
        match step {
            crate::level::RouteStepDef::MoveTo { x, z, speed } => {
                if !x.is_finite() || !z.is_finite() || !speed.is_finite() {
                    return Err(format!(
                        "{context} (`move_to`) position and speed must be finite"
                    ));
                }
                if *speed <= 0.0 || *speed > crate::level::MAX_ROUTE_SPEED_MPS {
                    return Err(format!(
                        "{context} (`move_to`) speed must be between 0 and {} m/s",
                        crate::level::MAX_ROUTE_SPEED_MPS
                    ));
                }
                let Some(waypoint_y) = floor.walk_height_at(*x, *z) else {
                    return Err(format!(
                        "{context} (`move_to`) waypoint ({x:.2}, {z:.2}) is not on \
                         any walkable floor"
                    ));
                };
                route_path_is_clear(
                    &context,
                    walls,
                    collision_index,
                    floor,
                    position,
                    (*x, *z),
                    radius,
                    body_height,
                    step_height,
                )?;
                position = Vec3::new(*x, waypoint_y, *z);
            }
            crate::level::RouteStepDef::Face { yaw_degrees } => {
                if !yaw_degrees.is_finite() {
                    return Err(format!("{context} (`face`) yaw must be finite"));
                }
            }
            crate::level::RouteStepDef::Wait { seconds } => {
                if !seconds.is_finite()
                    || *seconds <= 0.0
                    || *seconds > crate::level::MAX_ROUTE_WAIT_SECONDS
                {
                    return Err(format!(
                        "{context} (`wait`) seconds must be between 0 and {}",
                        crate::level::MAX_ROUTE_WAIT_SECONDS
                    ));
                }
            }
            crate::level::RouteStepDef::Play { clip, seconds, .. } => {
                if clip.trim().is_empty() {
                    return Err(format!("{context} (`play`) needs a clip name"));
                }
                if !seconds.is_finite()
                    || *seconds <= 0.0
                    || *seconds > crate::level::MAX_ROUTE_PLAY_SECONDS
                {
                    return Err(format!(
                        "{context} (`play`) seconds must be between 0 and {}",
                        crate::level::MAX_ROUTE_PLAY_SECONDS
                    ));
                }
            }
        }
    }
    Ok(())
}

/// True when a straight route segment stays clear of walls and floor breaks.
///
/// The segment is sampled every 10 cm; each sample must rest on a walkable
/// floor, and no wall may block the body there. The height change between
/// consecutive samples is bounded by the entity step, so a route cannot climb
/// a cliff in one sample.
#[allow(clippy::arithmetic_side_effects)] // bounded world-space segment sampling
#[allow(clippy::too_many_arguments)]
fn route_path_is_clear(
    context: &str,
    walls: &[crate::collision::WallAabb],
    collision_index: &crate::collision_index::CollisionIndex,
    floor: &crate::level::WalkableFloor,
    from: Vec3,
    to: (f32, f32),
    radius: f32,
    body_height: f32,
    step_height: f32,
) -> Result<(), String> {
    const SAMPLE_M: f32 = 0.02;
    let start = glam::Vec2::new(from.x, from.z);
    let end = glam::Vec2::new(to.0, to.1);
    let delta = end - start;
    let distance = delta.length();
    if !distance.is_finite() {
        return Err(format!("{context} (`move_to`) segment is not finite"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let samples = ((distance / SAMPLE_M).ceil() as u32).max(1);
    let mut previous_y = from.y;
    for index in 0..=samples {
        #[allow(clippy::cast_precision_loss)]
        let t = index as f32 / samples as f32;
        let point = start + delta * t;
        let Some(floor_y) = floor.walk_height_at(point.x, point.y) else {
            return Err(format!(
                "{context} (`move_to`) crosses off the walkable floor at \
                 ({:.2}, {:.2})",
                point.x, point.y
            ));
        };
        if (floor_y - previous_y).abs() > step_height + crate::collision::STEP_EPS {
            return Err(format!(
                "{context} (`move_to`) steps more than {step_height:.2} m at ({:.2}, {:.2}); \
                 the entity cannot climb it",
                point.x, point.y
            ));
        }
        let mut blocked = false;
        collision_index.for_each_disc(point.x, point.y, radius, walls, |wall| {
            if wall.blocks_body(floor_y, body_height)
                && wall.overlaps_disc(point.x, point.y, radius)
            {
                blocked = true;
            }
        });
        if blocked {
            return Err(format!(
                "{context} (`move_to`) is blocked by geometry at ({:.2}, {:.2})",
                point.x, point.y
            ));
        }
        previous_y = floor_y;
    }
    Ok(())
}

/// Authored trigger volumes: count, geometry, vertical bounds and room
/// containment.
///
/// A trigger is a real volume with an effect, so the checks mirror the water
/// volumes and ladders: the footprint must be finite and positive, the
/// resolved top strictly above the resolved bottom, and the volume must
/// overlap a room section. What each volume's `enter_volume`/`exit_volume`
/// bindings do is validated with every other binding.
fn validate_volumes(level: &LevelDef) -> Result<(), String> {
    if level.timers.len() > crate::level::MAX_LEVEL_TIMERS {
        return Err(format!(
            "Level contains too many timers: {} (limit: {})",
            level.timers.len(),
            crate::level::MAX_LEVEL_TIMERS
        ));
    }
    if u64::try_from(level.volumes.len()).unwrap_or(u64::MAX)
        > crate::level::MAX_LEVEL_AREA_TRIGGERS
    {
        return Err(format!(
            "Level contains too many trigger volumes: {} (limit: {})",
            level.volumes.len(),
            crate::level::MAX_LEVEL_AREA_TRIGGERS
        ));
    }
    for (i, volume) in level.volumes.iter().enumerate() {
        let context = format!("Trigger volume {i} (`{}`)", volume_id(i, volume));
        if !volume.x.is_finite()
            || !volume.z.is_finite()
            || !volume.width.is_finite()
            || !volume.depth.is_finite()
        {
            return Err(format!(
                "{context} position and size must be finite numbers"
            ));
        }
        if volume.width <= 0.0 || volume.depth <= 0.0 {
            return Err(format!("{context} width and depth must be positive"));
        }
        for (name, value) in [("bottom_y", volume.bottom_y), ("top_y", volume.top_y)] {
            if value.is_some_and(|value| !value.is_finite()) {
                return Err(format!(
                    "{context} {name} must be a finite number when specified"
                ));
            }
        }
        let (bottom, top) = volume.resolved_y_bounds(level);
        if !bottom.is_finite() || !top.is_finite() || top <= bottom {
            return Err(format!(
                "{context} top_y ({top:.2} m) must be above its bottom_y ({bottom:.2} m)"
            ));
        }
        if !rect_overlaps_room(level, volume.bounds()) {
            return Err(format!("{context} lies outside every room section"));
        }
    }
    Ok(())
}

/// Doors: count, geometry, swing, state and interaction contract.
///
/// A door is a physical leaf, so the checks are geometric: a positive leaf,
/// a non-zero bounded swing, finite positive speeds, a swing direction that
/// agrees with its sign, a closed leaf that does not start inside solid
/// geometry, and an interaction reach inside the engine cap. A door's id is
/// validated with every other instance id, so a door and a prop can never share
/// a name.
fn validate_doors(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.doors.len()).unwrap_or(u64::MAX) > crate::level::MAX_LEVEL_DOORS {
        return Err(format!(
            "Level contains too many doors: {} (limit: {})",
            level.doors.len(),
            crate::level::MAX_LEVEL_DOORS
        ));
    }
    let surfaces = LevelSurfaces::new(level);
    let solids = level.collision_aabbs();
    for (i, door) in level.doors.iter().enumerate() {
        validate_door_fields(i, door)?;
        // The hinge must stand somewhere with a floor: a door floating in the
        // void has no base to swing from.
        if surfaces.floor_y_at(door.x, door.z).is_none() {
            return Err(format!(
                "Door {i} hinge ({:.2}, {:.2}) is outside every room section",
                door.x, door.z
            ));
        }
        validate_door_leaf_is_clear(i, door, level, &solids)?;
    }
    Ok(())
}

/// One door's own dimensions, swing, state and lock contract.
///
/// The leaf's authored prompt and reach live in its `interactable` component
/// and are validated by [`validate_instance_ids`] with every other component.
fn validate_door_fields(i: usize, door: &crate::level::DoorDef) -> Result<(), String> {
    if !door.x.is_finite()
        || !door.y.is_finite()
        || !door.z.is_finite()
        || !door.rotation_degrees.is_finite()
        || !door.width.is_finite()
        || !door.height.is_finite()
        || !door.thickness.is_finite()
        || !door.swing_degrees.is_finite()
        || !door.open_speed_degrees.is_finite()
    {
        return Err(format!("Door {i} geometry must be finite numbers"));
    }
    if door.width <= 0.0 || door.height <= 0.0 || door.thickness <= 0.0 {
        return Err(format!(
            "Door {i} width, height and thickness must be positive"
        ));
    }
    if door.width > crate::level::MAX_DOOR_DIMENSION_M
        || door.height > crate::level::MAX_DOOR_DIMENSION_M
        || door.thickness > crate::level::MAX_DOOR_DIMENSION_M
    {
        return Err(format!(
            "Door {i} dimensions exceed the {} m limit",
            crate::level::MAX_DOOR_DIMENSION_M
        ));
    }
    if door.swing_degrees.abs() < crate::level::MIN_DOOR_SWING_DEGREES
        || door.swing_degrees.abs() > crate::level::MAX_DOOR_SWING_DEGREES
    {
        return Err(format!(
            "Door {i} swing_degrees must be between {} and {} degrees",
            crate::level::MIN_DOOR_SWING_DEGREES,
            crate::level::MAX_DOOR_SWING_DEGREES
        ));
    }
    if door.open_speed_degrees <= 0.0
        || door.open_speed_degrees > crate::level::MAX_DOOR_SPEED_DEGREES
    {
        return Err(format!(
            "Door {i} open_speed_degrees must be between 0 and {}",
            crate::level::MAX_DOOR_SPEED_DEGREES
        ));
    }
    if let Some(close) = door.close_speed_degrees
        && (!close.is_finite() || close <= 0.0 || close > crate::level::MAX_DOOR_SPEED_DEGREES)
    {
        return Err(format!(
            "Door {i} close_speed_degrees must be between 0 and {}",
            crate::level::MAX_DOOR_SPEED_DEGREES
        ));
    }
    if let Some(depth) = door.frame_depth
        && (!depth.is_finite() || depth <= 0.0 || depth > crate::level::MAX_DOOR_FRAME_DEPTH_M)
    {
        return Err(format!(
            "Door {i} frame_depth must be between 0 and {} m",
            crate::level::MAX_DOOR_FRAME_DEPTH_M
        ));
    }
    if let Some(center) = door.frame_center {
        let depth = door.frame_depth.ok_or_else(|| {
            format!("Door {i} frame_center needs a frame_depth to be measured inside")
        })?;
        if !center.is_finite() || center.abs() > depth {
            return Err(format!(
                "Door {i} frame_center must be within ±frame_depth ({depth} m)"
            ));
        }
    }
    // A door's authored prompt and reach now live in its `interactable`
    // component and are validated with every other component by
    // [`validate_instance_ids`].
    Ok(())
}

/// The closed leaf must be clear of solid geometry.
///
/// The hinge, the leaf centre and the latch edge are sampled: this is the
/// authoring mistake that matters (a leaf inside a wall or a closed cabinet).
/// A full sweep belongs to the runtime, not the loader.
fn validate_door_leaf_is_clear(
    i: usize,
    door: &crate::level::DoorDef,
    level: &LevelDef,
    solids: &[crate::collision::WallAabb],
) -> Result<(), String> {
    let base = door.base_y(level);
    let (dx, dz) = door.closed_direction();
    let half = door.width * 0.5;
    let samples = [
        (door.x, door.z),
        (dx.mul_add(half, door.x), dz.mul_add(half, door.z)),
        (
            dx.mul_add(door.width, door.x),
            dz.mul_add(door.width, door.z),
        ),
    ];
    for (sx, sz) in samples {
        for solid in solids {
            if solid.blocks_body(base, door.height)
                && sx > solid.min_x
                && sx < solid.max_x
                && sz > solid.min_z
                && sz < solid.max_z
            {
                return Err(format!(
                    "Door {i} leaf starts inside solid geometry at ({sx:.2}, {sz:.2}); \
                     cut a wall opening for it"
                ));
            }
        }
    }
    Ok(())
}

/// Effects: count, kind, bounds and material.
fn validate_effects(level: &LevelDef) -> Result<(), String> {
    if level.effects.len() > crate::level::MAX_LEVEL_EFFECTS {
        return Err(format!(
            "Level contains too many effects: {} (limit: {})",
            level.effects.len(),
            crate::level::MAX_LEVEL_EFFECTS
        ));
    }
    for (i, effect) in level.effects.iter().enumerate() {
        if !effect
            .kind
            .eq_ignore_ascii_case(crate::level::EFFECT_KIND_STEAM)
        {
            return Err(format!(
                "Effect {i} has unknown kind `{}`; expected `{}`",
                effect.kind,
                crate::level::EFFECT_KIND_STEAM
            ));
        }
        if !effect.x.is_finite()
            || !effect.y.is_finite()
            || !effect.z.is_finite()
            || !effect.width.is_finite()
            || !effect.depth.is_finite()
            || !effect.height.is_finite()
            || !effect.size.is_finite()
            || !effect.drift.is_finite()
            || !effect.lifetime_seconds.is_finite()
        {
            return Err(format!("Effect {i} parameters must be finite numbers"));
        }
        if effect.width <= 0.0 || effect.depth <= 0.0 || effect.height <= 0.0 || effect.size <= 0.0
        {
            return Err(format!(
                "Effect {i} width, depth, height and size must be positive"
            ));
        }
        if effect.count == 0 || effect.count > crate::level::MAX_EFFECT_PARTICLES {
            return Err(format!(
                "Effect {i} count must be between 1 and {}",
                crate::level::MAX_EFFECT_PARTICLES
            ));
        }
        if effect.drift < 0.0 {
            return Err(format!("Effect {i} drift cannot be negative"));
        }
        if effect.lifetime_seconds <= 0.0 || effect.lifetime_seconds > 60.0 {
            return Err(format!(
                "Effect {i} lifetime_seconds must be between 0 and 60"
            ));
        }
        if let Some(material) = effect.material.as_deref()
            && material.trim().is_empty()
        {
            return Err(format!(
                "Effect {i} material must not be blank when specified"
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
    // Decode the distinct fixture sheets ahead of the serial pass, exactly like
    // a level's own materials: each family's sheet is one independent PNG, and
    // a level with many fixtures of one family then pays its decode once.
    if let Some(root) = root.as_deref() {
        let references: Vec<(String, String)> = level
            .ceiling_lights
            .iter()
            .filter_map(|light| catalog.fixture_sheet_path(&light.fixture))
            .map(|path| (path.to_string(), path.to_string()))
            .collect();
        cache.prefetch_catalog(root, &references);
    }
    // Resolve one sheet per family used, then one per switchable fixture (its
    // own face), each at the material index the geometry emitter assigns it.
    let mut family_sheets: Vec<Option<ResolvedFixtureSheet>> =
        vec![None; crate::lighting::FixtureKind::ALL.len()];
    let mut switchable: Vec<ResolvedFixtureSheet> = Vec::new();
    for (fixture_index, light) in level.ceiling_lights.iter().enumerate() {
        let kind = crate::lighting::fixture_profile(&light.fixture).kind;
        let source = match resolve_fixture_sheet(
            &light.fixture,
            kind,
            catalog,
            pack,
            root.as_deref(),
            cache,
        ) {
            Ok(Some(mut sheet)) => {
                sheet.slot = crate::level::fixture_face_material_index(
                    fixture_index,
                    light.switchable,
                    kind,
                );
                Some(sheet)
            }
            Ok(None) => None,
            Err(error) => {
                crate::logging::warn_once(
                    format!("fixture-sheet:{}:{error}", light.fixture),
                    format!("[fixtures] {error}; drawing the untextured sheet instead"),
                );
                None
            }
        };
        if light.switchable {
            if let Some(sheet) = source {
                switchable.push(sheet);
            }
        } else if source.is_some() {
            // The shared family sheet: first authored fixture of the family wins.
            let slot = crate::level::fixture_face_material_index(fixture_index, false, kind);
            if let Some(existing) =
                family_sheets.get_mut(usize::try_from(slot).unwrap_or(usize::MAX))
                && existing.is_none()
            {
                *existing = source;
            }
        }
    }
    let mut sheets: Vec<ResolvedFixtureSheet> = family_sheets.into_iter().flatten().collect();
    sheets.extend(switchable);
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
            slot: 0,
            key,
            origin: crate::materials::TextureOrigin::Pack,
            image,
        }));
    }

    let Some(path) = catalog.fixture_sheet_path(fixture_id) else {
        return Ok(None);
    };
    let Some(root) = asset_root else {
        return Err(format!(
            "fixture `{fixture_id}` sheet `{path}`: the asset root is missing"
        ));
    };
    let (image, key) = cache
        .load_relative(root, path, path)
        .map_err(|error| format!("fixture `{fixture_id}` sheet `{path}`: {error}"))?;
    Ok(Some(ResolvedFixtureSheet {
        kind,
        slot: 0,
        key,
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
    level.snap_ceiling_decals(&materials);
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
        });
        let mut unique: Vec<LevelEntry> = Vec::with_capacity(discovered.len());
        for entry in discovered {
            if let Some(existing) = unique.iter().find(|existing| existing.id == entry.id) {
                crate::logging::warn_once(
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

        // Places Demo is always offered. When no installed package was
        // discovered (no asset tree, or the installed copy is malformed), the
        // embedded package is exposed as an ordinary entry so the Level Select
        // menu and `PLACES_LEVEL=places_demo` both keep working.
        if !unique.iter().any(|entry| entry.id == DEMO_LEVEL_ID) {
            unique.push(LevelEntry {
                id: DEMO_LEVEL_ID.to_string(),
                name: "Places Demo".to_string(),
                author: "Places Team".to_string(),
                source_type: LevelSourceType::Embedded,
                path: PathBuf::new(),
            });
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
            if !crate::package::is_package_path(&path) {
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
        crate::logging::warn_once(
            format!("level-skipped:{}", path.display()),
            format!("[levels] skipping {}: {error}", path.display()),
        );
    }

    /// Reads one package's manifest and semantics for discovery.
    fn probe_package_file(path: &Path, source_type: LevelSourceType) -> Result<LevelEntry, String> {
        let opened = crate::package::world::open(path)?;
        Ok(LevelEntry {
            id: opened.level.id,
            name: opened.level.name,
            author: opened.level.author,
            source_type,
            path: path.to_path_buf(),
        })
    }

    /// Loads the official demo, or the embedded package when it is not
    /// installed.
    ///
    /// `Places Demo` is the only level bundled with the game, so it is also the
    /// default level the game boots into. External/user levels are unaffected:
    /// they are discovered alongside it and can be selected from the menu.
    ///
    /// # Errors
    ///
    /// Returns a message when the demo cannot be loaded, or when neither an
    /// installed nor an embedded demo package opens and validates.
    pub fn load_default(&self) -> Result<LoadedLevel, String> {
        let Some(entry) = self.entries.iter().find(|e| e.id == DEMO_LEVEL_ID) else {
            return self.load_embedded_demo();
        };
        match self.load_level(entry) {
            Ok(loaded) => Ok(loaded),
            Err(error) => {
                if entry.source_type == LevelSourceType::Embedded {
                    return Err(error);
                }
                // A broken installed copy must not turn into a failed boot:
                // the embedded package is the recovery copy.
                crate::logging::warn(format!(
                    "[levels] installed Places Demo failed to load ({error}); using the embedded copy"
                ));
                self.load_embedded_demo()
            }
        }
    }

    /// Loads the compiled demo embedded in the executable.
    ///
    /// # Errors
    ///
    /// Returns an error only if the embedded package does not open; it is
    /// compiled into the binary, so that is a build defect, not player input.
    fn load_embedded_demo(&self) -> Result<LoadedLevel, String> {
        let opened = crate::package::world::open_bytes(FALLBACK_DEMO_PACKAGE)
            .map_err(|error| format!("embedded Places Demo package is invalid: {error}"))?;
        self.assembled_level(
            LevelEntry {
                id: DEMO_LEVEL_ID.into(),
                name: "Places Demo".into(),
                author: "Places Team".into(),
                source_type: LevelSourceType::Embedded,
                path: PathBuf::new(),
            },
            opened,
        )
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
    /// [`crate::level::MAX_LEVEL_TEXTURE_BYTES`]. The check happens here,
    /// after resolution and before any GPU upload; the material *count* is
    /// already bounded by `validate_level`.
    fn assembled_level(
        &self,
        entry: LevelEntry,
        opened: crate::package::world::OpenedPackage,
    ) -> Result<LoadedLevel, String> {
        let level = opened.level;
        let materials = self.resolve_level_materials(&level, None);
        crate::materials::check_texture_budget(&materials)
            .map_err(|error| format!("level `{}`: {error}", level.id))?;
        let light_sheets = self.resolve_level_fixture_sheets(&level, None);
        Ok(LoadedLevel {
            catalog: Arc::new(self.prop_catalog.clone()),
            level,
            materials,
            light_sheets,
            entry,
        })
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
        cache.begin_level();
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

    /// Loads a validated compiled level package.
    ///
    /// Missing or corrupt texture files resolve to the diagnostic material and
    /// a logged error; a level never fails to load because of one bad PNG.
    ///
    /// # Errors
    ///
    /// Returns a message when the package cannot be opened, its manifest or
    /// semantics fail validation, or the entry names a missing file.
    pub fn load_level(&self, entry: &LevelEntry) -> Result<LoadedLevel, String> {
        match entry.source_type {
            LevelSourceType::Bundled | LevelSourceType::Installed => {
                let opened = crate::package::world::open(&entry.path)?;
                self.assembled_level(entry.clone(), opened)
            }
            LevelSourceType::Embedded => self.load_embedded_demo(),
        }
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
    /// cannot be validated, or cannot be copied into the installed levels
    /// directory.
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
        if !crate::package::is_package_path(source_path) {
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
        // Validate before copying: a malformed package never lands in the
        // playable directory.
        let opened = crate::package::world::open(source_path)?;
        let file_name = source_path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;
        fs::create_dir_all(&self.levels_dir)
            .map_err(|e| format!("Failed to create levels directory: {e}"))?;
        let target_path = self.levels_dir.join(file_name);
        if source_path != target_path {
            fs::copy(source_path, &target_path)
                .map_err(|e| format!("Failed to copy file to {}: {e}", target_path.display()))?;
        }
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
        let _ = fs::create_dir_all(&self.import_dir);
        let _ = fs::create_dir_all(&self.levels_dir);
        let mut imported_count: usize = 0;
        let nested_import = self.levels_dir.join("import");
        let candidate_dirs = [self.import_dir.clone(), nested_import];
        for dir in &candidate_dirs {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if crate::package::is_package_path(&path) {
                    self.import_file(&path)?;
                    imported_count = imported_count.saturating_add(1);
                } else if path
                    .extension()
                    .is_some_and(|ext| ext == "json" || ext == "zip")
                {
                    crate::logging::warn_once(
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
        Ok(imported_count)
    }
}

#[cfg(test)]
mod tests;

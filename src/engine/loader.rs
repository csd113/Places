use crate::engine::level::LevelDef;
use crate::engine::materials::{MaterialTable, PackMaterials};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod manager;
pub use manager::LevelManager;
mod validation;
pub(super) use validation::props::validate_prop_physics;
pub(crate) use validation::validation_has_checkpoints;
pub use validation::{MAX_LIFETIME_SECONDS, validate_level};
pub(crate) use validation::{point_buried_in_wall, script_calls};

/// Re-exported so the rest of the crate keeps its historical import paths.
pub use crate::engine::materials::{
    RawImage, TextureCache, decode_png, encode_png, parse_materials_json,
};

/// Source type of an installed level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceType {
    /// A compiled package below the read-only bundled `assets/levels/`
    /// directory.
    Bundled,
    /// A compiled package installed in the writable drop-in `levels/`
    /// directory (or copied there by the Import action).
    Installed,
}

impl LevelSourceType {
    /// Stable lower-case label, used by `--list-levels` and the diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::Installed => "installed",
        }
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
/// [`crate::engine::lighting::FixtureKind::index`], with a family missing from the list
/// drawing the shared untextured white sheet.
#[derive(Clone, Debug)]
pub struct LoadedLevel {
    /// Project scope captured with the loaded semantics, never inferred from a replacement file.
    pub project_id: Option<String>,
    /// Canonical manifest identity read with these semantics. Compiler capture
    /// worlds have no package yet; runtime worlds always carry this identity.
    pub package_identity: Option<String>,
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
    pub kind: crate::engine::lighting::FixtureKind,
    /// Fixture-face material index this entry fills.
    ///
    /// A family sheet fills its family's slot; a switchable fixture's own face
    /// fills its private slot after
    /// [`crate::engine::level::FIXTURE_SWITCHABLE_MATERIAL_BASE`].
    pub slot: u32,
    /// Session-unique decode/dedupe key: the catalog PNG path, or the pack's own
    /// `pack:<namespace>:<path>` key.
    pub key: String,
    /// Where the sheet came from; decides its GPU lifetime.
    pub origin: crate::engine::materials::TextureOrigin,
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
/// ([`crate::engine::assets`]); this re-export keeps the level loader's existing call
/// sites and tests unchanged.
pub use crate::engine::assets::parse_hex_color;

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
/// This is the placement view of the authoritative [`crate::engine::assets::AssetCatalog`]:
/// levels reference a logical id such as `core:chair` or `spooner-man`, and the
/// catalog maps it to the canonical model resource under the asset root.
/// Entities resolve through exactly the same lookup, so `spooner-man` keeps
/// working unchanged.
///
/// Lookups always succeed: unknown or non-placeable ids resolve to a generated
/// fallback entry using [`crate::engine::level::PROP_FALLBACK_SIZE`] and a neutral
/// colour, so a level referencing a missing prop still loads with an obvious
/// placeholder instead of failing or rendering the wrong model.
#[derive(Debug, Clone, Default)]
pub struct PropCatalog {
    assets: crate::engine::assets::AssetCatalog,
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
            assets: crate::engine::assets::AssetCatalog::from_json_str(json)?,
        })
    }

    /// Loads a catalog from `path`, returning `None` when the file is missing
    /// or invalid. Never panics.
    #[must_use]
    pub fn load_from_path(path: &Path) -> Option<Self> {
        Some(Self {
            assets: crate::engine::assets::AssetCatalog::load_from_path(path)?,
        })
    }

    /// Loads the shipped asset catalog, falling back to an empty catalog when
    /// no `assets/catalog.json` can be found.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            assets: crate::engine::assets::AssetCatalog::load_default(),
        }
    }

    /// The authoritative catalog behind the placement view.
    #[must_use]
    pub const fn assets(&self) -> &crate::engine::assets::AssetCatalog {
        &self.assets
    }

    /// The declared environment themes, in catalog order.
    #[must_use]
    pub fn themes(&self) -> &[crate::engine::assets::AssetThemeDef] {
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
            size: crate::engine::level::PROP_FALLBACK_SIZE,
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

    /// Linear albedo preset for a placed static model; decoded geometry and
    /// textures remain shared by canonical resource path.
    #[must_use]
    pub fn tint(&self, model: &str) -> Option<[f32; 3]> {
        self.assets.placeable(model).and_then(|entry| entry.tint)
    }

    /// True when the catalog has no placeable entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Converts a catalog entry into the runtime's placeable view, applying the
/// neutral fallbacks for missing optional metadata.
fn placeable_entry(entry: &crate::engine::assets::AssetEntry) -> PropCatalogEntry {
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
            .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE),
        color: entry.color.unwrap_or_else(prop_fallback_color),
        model: entry.model.clone(),
        solid: entry.solid,
    }
}

/// Resolves the visible-face sheets of the fixture families a level places.
///
/// Built-in fixtures resolve the PNG their catalog entry names, exactly like an
/// external decal sheet: the catalog owns the file, this decodes it through the
/// shared session cache and the standard PNG loader. A `pack:` fixture id uses
/// the pack's own sheet for its family instead, so a level pack can still
/// restyle a built-in fixture without touching the catalog.
///
/// Shared faces use [`crate::engine::lighting::FixtureKind::index`], with the first
/// shared light deciding its family sheet. Switched and animated faces use
/// private slots matching the geometry. A missing sheet draws the white
/// fallback with the fixture's authored emission; a named sheet that cannot be
/// read or decoded logs the fixture id and falls back in the same way.
///
/// # Errors
/// Returns the level texture-budget error before retaining excessive sheets.
pub fn resolve_fixture_sheets(
    level: &LevelDef,
    catalog: &crate::engine::assets::AssetCatalog,
    pack: Option<&PackMaterials>,
    cache: &mut TextureCache,
    materials: &MaterialTable,
) -> Result<Vec<ResolvedFixtureSheet>, String> {
    let mut images: HashSet<_> = materials
        .textures()
        .iter()
        .map(|texture| Arc::as_ptr(&texture.image))
        .collect();
    let mut texture_bytes = materials.decoded_texture_bytes();
    let root = crate::engine::assets::resolve_asset_root();
    // Decode the distinct fixture sheets ahead of the serial pass, exactly like
    // a level's own materials: each family's sheet is one independent PNG, and
    // a level with many fixtures of one family then pays its decode once.
    if let Some(asset_root) = root.as_deref() {
        let references: Vec<(String, String)> = level
            .ceiling_lights
            .iter()
            .filter_map(|light| catalog.fixture_sheet_path(&light.fixture))
            .map(|path| (path.to_string(), path.to_string()))
            .collect();
        cache.prefetch_catalog(asset_root, &references);
    }
    // Resolve shared family sheets and private switched/animated faces at the
    // exact material indices the geometry emitter assigns them.
    let mut family_sheets: Vec<Option<ResolvedFixtureSheet>> =
        vec![None; crate::engine::lighting::FixtureKind::ALL.len()];
    let mut private_sheets: Vec<ResolvedFixtureSheet> = Vec::new();
    for (fixture_index, light) in level.ceiling_lights.iter().enumerate() {
        let kind = crate::engine::lighting::fixture_profile(&light.fixture).kind;
        let private_face = crate::engine::level::fixture_face_uses_private_material(level, light);
        let slot =
            crate::engine::level::fixture_face_material_index(fixture_index, private_face, kind);
        if !private_face
            && kind != crate::engine::lighting::FixtureKind::BlacklightTube
            && family_sheets
                .get(usize::try_from(slot).unwrap_or(usize::MAX))
                .is_some_and(Option::is_some)
        {
            continue;
        }
        let source = match resolve_fixture_sheet(
            &light.fixture,
            kind,
            catalog,
            pack,
            root.as_deref(),
            cache,
        ) {
            Ok(Some(mut sheet)) => {
                sheet.slot = slot;
                Some(sheet)
            }
            Ok(None) => None,
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("fixture-sheet:{}:{error}", light.fixture),
                    format!("[fixtures] {error}; drawing the untextured sheet instead"),
                );
                None
            }
        };
        if let Some(sheet) = &source
            && images.insert(Arc::as_ptr(&sheet.image))
        {
            texture_bytes = texture_bytes.saturating_add(sheet.image.rgba.len());
            crate::engine::materials::check_texture_bytes(texture_bytes)?;
        }
        if private_face || kind == crate::engine::lighting::FixtureKind::BlacklightTube {
            if let Some(sheet) = source {
                private_sheets.push(sheet);
            }
        } else if source.is_some() {
            // The shared family sheet: first authored fixture of the family wins.
            if let Some(existing) =
                family_sheets.get_mut(usize::try_from(slot).unwrap_or(usize::MAX))
                && existing.is_none()
            {
                *existing = source;
            }
        }
    }
    let mut sheets: Vec<ResolvedFixtureSheet> = family_sheets.into_iter().flatten().collect();
    sheets.extend(private_sheets);
    Ok(sheets)
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
    kind: crate::engine::lighting::FixtureKind,
    catalog: &crate::engine::assets::AssetCatalog,
    pack: Option<&PackMaterials>,
    asset_root: Option<&Path>,
    cache: &mut TextureCache,
) -> Result<Option<ResolvedFixtureSheet>, String> {
    if fixture_id.starts_with("pack:") {
        let Some(pack_id) = pack else {
            return Ok(None);
        };
        let Some(path) = pack_id.texture_for(fixture_id) else {
            return Ok(None);
        };
        let key = pack_id.cache_key(&path);
        let image = pack_id
            .decode_texture(cache, &path)
            .map_err(|error| format!("fixture `{fixture_id}`: {error}"))?;
        return Ok(Some(ResolvedFixtureSheet {
            kind,
            slot: 0,
            key,
            origin: crate::engine::materials::TextureOrigin::Pack,
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
        origin: crate::engine::materials::TextureOrigin::Catalog,
        image,
    }))
}

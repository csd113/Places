use crate::engine::level::LevelDef;
use crate::engine::materials::MaterialTable;
use std::path::PathBuf;
use std::sync::Arc;

mod manager;
pub use manager::{ImportCounts, ImportOutcome, ImportedPackage, LevelManager};
mod validation;
pub(super) use validation::props::validate_prop_physics;
pub(crate) use validation::validation_has_checkpoints;
pub use validation::{MAX_LIFETIME_SECONDS, validate_level};
pub(crate) use validation::{point_buried_in_wall, script_calls};

/// Re-exported so the rest of the crate keeps its historical import paths.
pub use crate::engine::materials::{
    RawImage, TextureCache, decode_png, encode_png, parse_materials_json,
};

pub use crate::engine::materials::fixture_sheets::{ResolvedFixtureSheet, resolve_fixture_sheets};
pub use crate::engine::props::catalog::{PropCatalog, PropCatalogEntry};

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

/// Parses `#rrggbb` (or bare `rrggbb`) into 0..1 RGB components.
///
/// The parser lives with the catalog that stores those colours
/// ([`crate::engine::assets`]); this re-export keeps the level loader's existing call
/// sites and tests unchanged.
pub use crate::engine::assets::parse_hex_color;

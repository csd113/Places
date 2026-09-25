//! Authoritative asset catalog: logical ids, classification and resource paths.
//!
//! Levels never store where an asset file lives. They store a logical id
//! (`core:desk`, `spooner-man`); the catalog in `assets/catalog.json` maps that
//! id to its
//!
//! * **asset class** — `environment`, `entity`, `core` or `diagnostic`;
//! * **asset type** — `prop`, `material`, `texture`, `light`, `decal`, `entity`;
//! * **theme** — an organizational environment collection such as `office` or
//!   `pool`, absent for generic/shared content;
//! * **source** — a file below the asset root (`assets/`) or a resource the
//!   renderer generates in code;
//! * **resource** — the canonical model path relative to the asset root.
//!
//! Classification is organizational only. Nothing in the runtime filters
//! placement by theme or class: an Office fixture may light a Pool level, a
//! Pool decal may sit in an Office corridor and an entity may stand in any
//! room. Themes exist for discoverability, documentation and future tools, not
//! for restriction.
//!
//! The catalog is data, not code. Adding a theme is adding a `themes` record;
//! adding a future class or type is a validated identifier, so an unknown value
//! never corrupts lookup. Physical files can move between directories without
//! rewriting a single level, because levels only ever reference the logical id.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::materials::{
    MAX_EMISSION_COLOR, MAX_EMISSION_INTENSITY, MAX_NORMAL_STRENGTH, MAX_REFLECTION_STRENGTH,
    MAX_SPECULAR, ReflectionMode,
};

/// Environment variable that overrides every other asset-root candidate.
///
/// An absolute or relative path to the directory that *contains* `assets/`,
/// i.e. the package root. Set it to pin a specific installation, to run a
/// packaged build from an unusual directory, or to test a build against an
/// asset tree other than the one next to the executable.
pub const ASSET_ROOT_ENV: &str = "PLACES_ASSET_ROOT";

/// Directory candidates for the shipped asset root, in search order.
///
/// These are the working-directory-relative names kept for development: a run
/// from the repository root and a `cargo run` from a subdirectory both find
/// `assets/`. A packaged build is located through
/// [`package_root_candidates`] instead, so it never depends on the working
/// directory.
pub const ASSET_ROOT_CANDIDATES: [&str; 3] = ["assets", "./assets", "../assets"];

/// Catalog file name inside the asset root.
pub const CATALOG_FILE_NAME: &str = "catalog.json";

/// Environment variable that overrides where writable runtime state lives.
///
/// The state root is the directory that owns `settings.json`, the drop-in
/// `levels/` and `import/` directories and the level cache. A relative value is
/// resolved against the working directory at read time, like
/// [`ASSET_ROOT_ENV`].
pub const STATE_ROOT_ENV: &str = "PLACES_STATE_ROOT";

/// The directory that owns every writable runtime file.
///
/// Precedence is deliberate and deterministic:
///
/// 1. the [`STATE_ROOT_ENV`] override — tests and benchmark runs pin their own
///    scratch state;
/// 2. the package root, i.e. the parent of the resolved asset root — a portable
///    install keeps settings, drop-in levels and its cache next to its payload,
///    no matter what the working directory is;
/// 3. the working directory, for the degenerate case where no asset root was
///    found at all.
///
/// Nothing here creates the directory: callers create the subdirectories they
/// need, so a read-only installation still fails only where it must.
#[must_use]
pub fn state_root() -> PathBuf {
    static RESOLVED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            if let Ok(raw) = std::env::var(STATE_ROOT_ENV) {
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    let path = PathBuf::from(trimmed);
                    return if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir()
                            .map(|cwd| cwd.join(&path))
                            .unwrap_or(path)
                    };
                }
            }
            if let Some(assets) = resolve_asset_root()
                && let Some(root) = assets.parent()
            {
                return root.to_path_buf();
            }
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        })
        .clone()
}

/// A runtime path below the [`state_root`].
///
/// An absolute `relative` is returned unchanged, so a caller with a specific
/// file in mind can still pass one.
#[must_use]
pub fn state_path(relative: impl AsRef<Path>) -> PathBuf {
    state_root().join(relative)
}

/// First line of the missing-asset-root diagnostic.
///
/// Deliberately loud and self-describing: a distribution that cannot find its
/// payload must not look like a successful launch, and the reader needs to know
/// what was expected before the candidate list that follows.
pub const NO_ASSET_ROOT_MESSAGE: &str = "\
[assets] no asset root found: the game could not locate a directory containing \
`assets/catalog.json`, so every texture, model, decal and level request will \
fall back to placeholder or diagnostic content.

[assets] searched, in order:";

/// How many parent directories of the executable are searched for a package.
///
/// `target/release/places` and `target/debug/deps/<test>` need two and
/// three. A macOS `Places.app/Contents/MacOS/places` bundle is reached through
/// its `Resources` directory instead, which is only one level up.
const EXECUTABLE_ANCESTOR_DEPTH: usize = 3;

/// Where a packaged runtime looks for its `assets/` directory, in order.
///
/// A candidate is accepted only when it is a *complete* asset root — the
/// directory exists and carries a `catalog.json` — so a stray `assets/`
/// directory above an installation cannot hijack it.
#[must_use]
pub fn package_root_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(override_path) = asset_root_override() {
        candidates.push(override_path);
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        // macOS application bundle: `Contents/MacOS/<exe>` with the payload in
        // `Contents/Resources/`. Checked before the ancestor walk because the
        // bundle's own parent directory is not part of its payload.
        candidates.push(directory.join("../Resources"));
        let mut ancestor = Some(directory);
        for _ in 0..=EXECUTABLE_ANCESTOR_DEPTH {
            let Some(current) = ancestor else { break };
            candidates.push(current.to_path_buf());
            ancestor = current.parent();
        }
    }
    candidates
}

/// Reads and normalises [`ASSET_ROOT_ENV`].
///
/// A relative override is made absolute against the working directory at the
/// moment it is read, so a later `set_current_dir` cannot silently retarget it.
fn asset_root_override() -> Option<PathBuf> {
    let raw = std::env::var(ASSET_ROOT_ENV).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    Some(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    })
}

/// True when `root` is a complete asset root: a directory with a catalog.
fn is_complete_asset_root(root: &Path) -> bool {
    root.is_dir() && root.join(CATALOG_FILE_NAME).is_file()
}

/// The package directories that hold a complete [`CATALOG_FILE_NAME`] asset
/// tree, in precedence order.
///
/// This is the authoritative lookup for a packaged build: the executable's own
/// directory, up to three ancestors (including Cargo build/test directories),
/// and a macOS bundle's `Contents/Resources`.
#[must_use]
pub fn resolved_package_roots() -> Vec<PathBuf> {
    package_root_candidates()
        .into_iter()
        .filter(|root| is_complete_asset_root(&root.join("assets")))
        .collect()
}

/// The asset root resolved once for this process.
///
/// Resolution is deliberately deterministic and cached: the precedence is
///
/// 1. the [`ASSET_ROOT_ENV`] override — an explicit override always wins;
/// 2. the executable's own location — a packaged build resolves its own
///    payload no matter what the working directory is;
/// 3. the working directory — `assets`, `./assets`, `../assets`, so
///    development from the repository root keeps working;
/// 4. the compile-time crate directory, **development builds only**
///    (`debug_assertions`), so a release binary can never quietly read the
///    source tree it was built from.
///
/// [`None`] means no asset root was found at all, which
/// [`AssetCatalog::load_default`] reports with the full candidate list.
#[must_use]
pub fn resolve_asset_root() -> Option<PathBuf> {
    static RESOLVED: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    RESOLVED.get_or_init(asset_root_search).clone()
}

/// Performs the [`resolve_asset_root`] search. Called at most once per process.
fn asset_root_search() -> Option<PathBuf> {
    let packaged = resolved_package_roots()
        .into_iter()
        .map(|root| root.join("assets"))
        .find(|assets| assets.is_dir());
    if packaged.is_some() {
        return packaged;
    }
    let development = ASSET_ROOT_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_dir());
    if development.is_some() {
        return development;
    }
    #[cfg(debug_assertions)]
    {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        if crate_root.is_dir() {
            return Some(crate_root);
        }
    }
    None
}

/// Every location [`resolve_asset_root`] would consider, in precedence order,
/// paired with whether it currently holds a usable asset root.
///
/// Used by the startup diagnostic so a missing installation says what was
/// expected and what was actually checked instead of failing opaquely.
#[must_use]
pub fn asset_root_search_report() -> Vec<(PathBuf, bool)> {
    let mut report: Vec<(PathBuf, bool)> = Vec::new();
    let push = |path: PathBuf, report: &mut Vec<(PathBuf, bool)>| {
        if report.iter().any(|(seen, _)| *seen == path) {
            return;
        }
        let ok = path.is_dir();
        report.push((path, ok));
    };
    for root in resolved_package_roots() {
        push(root.join("assets"), &mut report);
    }
    for candidate in ASSET_ROOT_CANDIDATES {
        push(PathBuf::from(candidate), &mut report);
    }
    #[cfg(debug_assertions)]
    push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        &mut report,
    );
    report
}

/// World metres covered by one repeat of a material's texture when the catalog
/// does not author `tile_metres`.
///
/// It matches the historical 2 m authored sheets
/// (128 px at 64 px/m), so an entry written before tiling was data keeps its
/// exact appearance.
pub const DEFAULT_TILE_METRES: f32 = 2.0;

/// Smallest accepted `tile_metres`. Below this a repeat is finer than a
/// centimetre and a texture reads as noise.
pub const MIN_TILE_METRES: f32 = 0.05;

/// Largest accepted `tile_metres`. Above this one repeat covers a very large
/// surface and the material is almost certainly a unit mistake (metres vs
/// centimetres vs pixels).
pub const MAX_TILE_METRES: f32 = 64.0;

/// Maximum PNG edge length the runtime decoder accepts.
pub const MAX_TEXTURE_DIMENSION: u32 = 1024;

/// Soft warning edge for shipped textures.
///
/// This is a tooling preference, not a low-end-memory budget: the tiling
/// surface sheets ship at 1024x1024 on purpose (one sheet covers 2 m of wall
/// within a 4 MiB decoded footprint), and the renderer samples whatever the
/// file holds. Tooling warns above this edge so that adding a larger sheet is
/// a deliberate decision rather than an accident.
pub const PREFERRED_TEXTURE_DIMENSION: u32 = 256;

/// Largest decoded RGBA8 surface sheet the renderer accepts.
///
/// One 1024x1024 sheet is exactly 4 MiB of pixels
/// (`4 * 1024 * 1024`), so this is the surface class's per-sheet byte budget:
/// the upgraded Office/Pool artwork sits at it, and nothing may exceed it.
pub const MAX_SURFACE_TEXTURE_BYTES: usize = 4 * 1024 * 1024;

/// Decoded RGBA8 byte count of a `width` x `height` sheet.
///
/// Saturates instead of overflowing, so a malformed dimension can never wrap
/// into a small byte count that passes a budget check.
#[must_use]
pub const fn decoded_rgba_bytes(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

/// The dimension contract one shipped PNG is held to.
///
/// The renderer is the source of truth, so the policy is deliberately
/// conservative rather than aspirational: it encodes what the runtime actually
/// requires, and it never special-cases an individual file. A deliberate
/// exception — the 96x64 diagnostic, a future non-POT `pack:` sheet — is
/// asserted at the call site that loads it, not hidden here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShippedTextureKind {
    /// A tiling surface sheet (wall, floor or ceiling).
    ///
    /// Surfaces are sampled as square `tile_metres` cells, so the sheet must
    /// be square: a non-square sheet would stretch. Surfaces are *not*
    /// power-of-two constrained; the runtime decoder and the renderer accept
    /// NPOT, and POT is only a portability preference.
    Surface,
    /// A fitted fixture face (a light's visible artwork).
    ///
    /// Fixture UVs never leave the sheet and the face is drawn with mipmaps,
    /// so both edges must be powers of two for an exact mip chain.
    FixtureFace,
    /// A decal cut-out sheet drawn by the decal pass.
    ///
    /// Decals are sampled with mipmaps exactly like a fixture face, so both
    /// edges must be powers of two.
    DecalSheet,
}

impl ShippedTextureKind {
    /// True when either edge exceeds the soft [`PREFERRED_TEXTURE_DIMENSION`].
    ///
    /// Over-preferred is not an error: the upgraded Office/Pool surfaces are
    /// intentionally 1024x1024. Tooling warns; the runtime loads.
    #[must_use]
    pub const fn is_over_preferred(width: u32, height: u32) -> bool {
        width > PREFERRED_TEXTURE_DIMENSION || height > PREFERRED_TEXTURE_DIMENSION
    }

    /// Checks one shipped PNG against this class's contract.
    ///
    /// `Ok(())` means the sheet satisfies every hard requirement; every
    /// violation is accumulated into the error text otherwise. The soft
    /// [`PREFERRED_TEXTURE_DIMENSION`] budget is never a violation — use
    /// [`Self::is_over_preferred`] to report it separately.
    ///
    /// # Errors
    ///
    /// Returns the accumulated violations when a sheet is zero-sized, exceeds
    /// [`MAX_TEXTURE_DIMENSION`], is a non-square [`Self::Surface`], is a
    /// surface sheet over [`MAX_SURFACE_TEXTURE_BYTES`] decoded, or is a
    /// non-power-of-two [`Self::FixtureFace`] / [`Self::DecalSheet`].
    pub fn check_dimensions(self, width: u32, height: u32) -> Result<(), String> {
        let mut problems: Vec<String> = Vec::new();
        if width == 0 || height == 0 {
            problems.push(format!(
                "dimensions must be non-zero, found {width}x{height}"
            ));
        }
        if width > MAX_TEXTURE_DIMENSION || height > MAX_TEXTURE_DIMENSION {
            problems.push(format!(
                "found {width}x{height}, over the {MAX_TEXTURE_DIMENSION}x{MAX_TEXTURE_DIMENSION} hard limit"
            ));
        }
        match self {
            Self::Surface => {
                if width != height {
                    problems.push(format!(
                        "a surface sheet must be square, found {width}x{height}"
                    ));
                }
                if decoded_rgba_bytes(width, height) > MAX_SURFACE_TEXTURE_BYTES {
                    problems.push(format!(
                        "a surface sheet decodes to {} bytes, over the {MAX_SURFACE_TEXTURE_BYTES}-byte budget",
                        decoded_rgba_bytes(width, height)
                    ));
                }
            }
            Self::FixtureFace | Self::DecalSheet => {
                if !width.is_power_of_two() || !height.is_power_of_two() {
                    problems.push(format!(
                        "a fitted sheet (fixture face or decal) must be power-of-two on both edges, found {width}x{height}"
                    ));
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }
}

/// Every catalog path the loader will try, in order.
///
/// Derived from the same precedence as [`resolve_asset_root`], so the catalog
/// and the textures it names can never resolve against two different trees.
#[must_use]
pub fn catalog_path_candidates() -> Vec<PathBuf> {
    let roots = resolved_package_roots()
        .into_iter()
        .map(|root| root.join("assets"))
        .chain(ASSET_ROOT_CANDIDATES.iter().map(PathBuf::from));
    #[cfg(debug_assertions)]
    let roots = roots.chain([PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")]);
    roots
        .map(|root| Path::new(&root).join(CATALOG_FILE_NAME))
        .collect()
}

/// True when `id` is a well-formed logical asset id.
///
/// Ids are stable names such as `core:chair` or `spooner-man`: non-empty, no
/// whitespace and no path separators, so a level can never smuggle a filesystem
/// path in through the id.
#[must_use]
pub fn is_valid_asset_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with(':')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
}

/// True when `value` is a well-formed lower-case metadata identifier.
fn is_valid_slug(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
}

/// Parses an optional metadata identifier, rejecting malformed values.
fn parse_optional_slug<T>(
    raw: Option<&str>,
    what: &str,
    namespace: &str,
    from_str: fn(&str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    from_str(raw)
        .map(Some)
        .map_err(|error| format!("{namespace}: invalid {what} '{raw}': {error}"))
}

/// Broad semantic classification of an asset.
///
/// `environment` and `entity` are the two classes the game ships; `core` is the
/// home for engine-level shared resources and `diagnostic` for development
/// content. The value is a validated identifier rather than a closed enum so a
/// future class parses and resolves without an engine change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetClass(String);

impl AssetClass {
    /// Content that furnishes an environment: props, materials, lights, decals.
    pub const ENVIRONMENT: &'static str = "environment";
    /// A character or creature: Spooner-Man today, player/NPC models later.
    pub const ENTITY: &'static str = "entity";
    /// Engine-level shared resources that belong to no environment.
    pub const CORE: &'static str = "core";
    /// Development and validation content such as the diagnostic decal sheets.
    pub const DIAGNOSTIC: &'static str = "diagnostic";

    /// The classes this build knows about, for tooling and documentation.
    pub const KNOWN: [&'static str; 4] = [
        Self::ENVIRONMENT,
        Self::ENTITY,
        Self::CORE,
        Self::DIAGNOSTIC,
    ];

    /// Validates a class identifier. Unknown classes parse: they are reported
    /// by tooling but never rejected by the runtime.
    /// # Errors
    ///
    /// Returns a message when `raw` is not a lower-case identifier.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_valid_slug(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err("expected a lower-case identifier such as `environment`".to_string())
        }
    }

    /// The identifier as written in the catalog.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when this is one of the classes this build ships.
    #[must_use]
    pub fn is_known(&self) -> bool {
        Self::KNOWN.contains(&self.0.as_str())
    }
}

impl fmt::Display for AssetClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Organizational environment collection such as `office` or `pool`.
///
/// A theme is metadata, never a placement gate: the runtime exposes no query
/// that filters assets by theme. `None` on an asset means generic/shared
/// content that belongs to no particular environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetTheme(String);

impl AssetTheme {
    /// Validates a theme identifier.
    /// # Errors
    ///
    /// Returns a message when `raw` is not a lower-case identifier.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_valid_slug(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err("expected a lower-case identifier such as `office`".to_string())
        }
    }

    /// The identifier as written in the catalog.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What kind of resource an asset is.
///
/// Distinct from [`AssetClass`] (which environment/entity world it belongs to)
/// and from [`AssetTheme`] (which collection organizes it). Like the other
/// identifiers the value is validated rather than closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetType(String);

impl AssetType {
    /// A placeable model (`model` points at a GLB below the asset root).
    pub const PROP: &'static str = "prop";
    /// A surface material sheet.
    pub const MATERIAL: &'static str = "material";
    /// A texture resource.
    pub const TEXTURE: &'static str = "texture";
    /// A light fixture definition.
    pub const LIGHT: &'static str = "light";
    /// A surface marking drawn by the decal pass.
    pub const DECAL: &'static str = "decal";
    /// A creature/character asset placed through the model pipeline.
    pub const ENTITY: &'static str = "entity";

    /// The types this build ships, for tooling and documentation.
    pub const KNOWN: [&'static str; 6] = [
        Self::PROP,
        Self::MATERIAL,
        Self::TEXTURE,
        Self::LIGHT,
        Self::DECAL,
        Self::ENTITY,
    ];

    /// Validates a type identifier.
    /// # Errors
    ///
    /// Returns a message when `raw` is not a lower-case identifier.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_valid_slug(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err("expected a lower-case identifier such as `prop`".to_string())
        }
    }

    /// The identifier as written in the catalog.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where an asset's resource comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSource {
    /// A physical file below the asset root, named by `model`: a GLB for a
    /// placeable, a PNG for a surface texture, an external decal sheet or a
    /// fixture's visible face.
    File,
    /// A resource the renderer generates in code (the decal atlas patterns).
    Generated,
    /// A resource-less definition composed from other catalog assets: a
    /// surface material names its base `texture` and carries the static
    /// properties the renderer needs. It has no file of its own.
    Definition,
}

impl AssetSource {
    /// The identifier as written in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Generated => "generated",
            Self::Definition => "definition",
        }
    }

    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "file" => Ok(Self::File),
            "generated" => Ok(Self::Generated),
            "definition" => Ok(Self::Definition),
            other => Err(format!(
                "expected `file`, `generated` or `definition`, found `{other}`"
            )),
        }
    }
}

/// One environment theme declared by the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetThemeDef {
    pub id: AssetTheme,
    pub display_name: String,
    pub description: String,
}

/// One logical asset.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetEntry {
    /// Stable logical id used by levels (`core:desk`, `spooner-man`).
    pub id: String,
    /// Human-readable name for tools and menus.
    pub display_name: String,
    pub asset_class: AssetClass,
    /// Organizational collection; `None` means generic/shared content.
    pub theme: Option<AssetTheme>,
    pub asset_type: AssetType,
    pub source: AssetSource,
    /// Canonical resource path relative to the asset root, for file assets.
    pub model: Option<String>,
    /// Catalogue box size and editor/collision metadata for placeable assets.
    pub size: Option<[f32; 3]>,
    pub color: Option<[f32; 3]>,
    pub category: Option<String>,
    pub solid: bool,
    /// Surface a material applies to (`wall`, `floor`, `ceiling`).
    pub surface: Option<String>,
    /// Base texture asset a material draws with (`core:tex_carpet_beige_01`).
    pub texture: Option<String>,
    /// World metres covered by one repeat of a material's texture.
    pub tile_metres: Option<f32>,
    /// Static multiply tint the renderer applies to a material's texture.
    pub tint: Option<[f32; 3]>,
    /// Emissive colour a material's surface reads with, if the catalog authors
    /// one. Emission is additive on top of baked illumination and never lights
    /// anything else; a material without it stays non-emissive.
    pub emissive: Option<[f32; 3]>,
    /// Scalar multiplier for [`Self::emissive`]; authored only together with it.
    pub emissive_intensity: Option<f32>,
    /// Logical texture asset id whose texels select where emission applies.
    pub emissive_mask: Option<String>,
    /// Logical texture asset id of the material's normal map, if it authors one.
    pub normal_texture: Option<String>,
    /// Multiplier applied to the decoded normal map, `0.0..=2.0`.
    pub normal_strength: Option<f32>,
    /// Sheen strength (white) and, optionally, an explicit sheen colour.
    pub specular: Option<f32>,
    pub specular_color: Option<[f32; 3]>,
    /// Author-facing glossiness, `0.0` matte .. `1.0` extremely glossy.
    ///
    /// The preferred spelling; the engine stores `roughness = 1 - shine`.
    pub shine: Option<f32>,
    /// Legacy inverse of [`Self::shine`], `0.0` mirror-tight .. `1.0` fully
    /// matte. Still accepted so catalogs authored before `shine` keep loading;
    /// authoring both fields is a catalog error.
    pub roughness: Option<f32>,
    /// `opaque` (default), `cutout` or `blend`.
    pub alpha_mode: Option<String>,
    /// Multiplier applied to a blended material's sampled alpha.
    pub opacity: Option<f32>,
    /// Alpha cut-off of a `cutout` material.
    pub alpha_cutoff: Option<f32>,
    /// `none` (default), `probe` or `planar`: where a material's reflection
    /// image comes from. Absent means the surface never reflects.
    pub reflection_mode: Option<String>,
    /// `0.0..=1.0` weight of [`Self::reflection_mode`]; authored only with it.
    pub reflection_strength: Option<f32>,
    /// Future entity kind (`character`, `npc`, `creature`, ...).
    pub entity_type: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl AssetEntry {
    /// True when this asset is placed as a model by the prop pipeline.
    ///
    /// Environment props and entities are both placeable, which is exactly how
    /// `spooner-man` keeps working through the ordinary placement format.
    #[must_use]
    pub fn is_placeable(&self) -> bool {
        matches!(
            self.asset_type.as_str(),
            AssetType::PROP | AssetType::ENTITY
        )
    }

    /// True when this asset is a surface material definition.
    #[must_use]
    pub fn is_material(&self) -> bool {
        self.asset_type.as_str() == AssetType::MATERIAL
    }

    /// True when this asset is a texture image resource.
    #[must_use]
    pub fn is_texture(&self) -> bool {
        self.asset_type.as_str() == AssetType::TEXTURE
    }

    /// True when this asset is a light fixture definition.
    #[must_use]
    pub fn is_light(&self) -> bool {
        self.asset_type.as_str() == AssetType::LIGHT
    }
}

/// Parsed `assets/catalog.json`, or the legacy `props.json` shape.
#[derive(Debug, Clone, Default)]
pub struct AssetCatalog {
    entries: HashMap<String, AssetEntry>,
    themes: Vec<AssetThemeDef>,
}

#[derive(serde::Deserialize)]
struct CatalogFile {
    #[serde(default)]
    themes: Vec<CatalogThemeFile>,
    #[serde(default)]
    assets: Vec<CatalogEntryFile>,
    /// Legacy `props.json` shape (`format_version: 1`). New catalogs use `assets`.
    #[serde(default)]
    props: Vec<CatalogEntryFile>,
}

#[derive(serde::Deserialize)]
struct CatalogThemeFile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
}

/// Emission fields of one catalog entry once they have been validated.
struct ValidatedEmission {
    color: Option<[f32; 3]>,
    intensity: Option<f32>,
    mask: Option<String>,
}

/// Validates a material's reflection fields, naming the problem when one is
/// malformed.
///
/// A mode is only meaningful with a strength and vice versa, so a strength
/// without a mode is an authoring error rather than a silent no-op.
fn validate_reflection(
    id: &str,
    entry: &CatalogEntryFile,
) -> Result<(Option<String>, Option<f32>), String> {
    let reflection_mode = match entry
        .reflection_mode
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
    {
        Some(mode) => Some(
            ReflectionMode::parse(mode)
                .ok_or_else(|| {
                    format!(
                        "{id}: `reflection_mode` must be one of `none`, `probe` or `planar`, found `{mode}`"
                    )
                })?
                .name()
                .to_string(),
        ),
        None => None,
    };
    if entry.reflection_strength.is_some() && reflection_mode.is_none() {
        return Err(format!(
            "{id}: `reflection_strength` requires an explicit `reflection_mode`"
        ));
    }
    let reflection_strength = match entry.reflection_strength {
        Some(value) if !value.is_finite() || !(0.0..=MAX_REFLECTION_STRENGTH).contains(&value) => {
            return Err(format!(
                "{id}: reflection_strength must be between 0.0 and {MAX_REFLECTION_STRENGTH:?}, found {value:?}"
            ));
        }
        Some(value) => Some(value),
        None => None,
    };
    Ok((reflection_mode, reflection_strength))
}

/// Validates one unit-interval number, naming the field when it is out of range.
fn unit_number(value: Option<f32>, field: &str) -> Result<Option<f32>, String> {
    match value {
        Some(value) if !value.is_finite() || !(0.0..=1.0).contains(&value) => Err(format!(
            "{field} must be between 0.0 and 1.0, found {value:?}"
        )),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

/// Validates an authored specular colour, defaulting to nothing.
fn specular_color(id: &str, raw: Option<&[f32]>) -> Result<Option<[f32; 3]>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() != 3 {
        return Err(format!(
            "{id}: specular_color must be exactly three channels, found {} in {raw:?}",
            raw.len()
        ));
    }
    let mut color = [0.0_f32; 3];
    for (slot, channel) in color.iter_mut().zip(raw) {
        if !channel.is_finite() || !(0.0..=MAX_SPECULAR).contains(channel) {
            return Err(format!(
                "{id}: specular_color channels must be between 0.0 and {MAX_SPECULAR:?}, found {raw:?}"
            ));
        }
        *slot = *channel;
    }
    Ok(Some(color))
}

/// Validates an authored alpha mode, normalising its spelling.
fn validated_alpha_mode(id: &str, mode: Option<String>) -> Result<Option<String>, String> {
    let Some(mode) = mode else {
        return Ok(None);
    };
    if crate::materials::AlphaMode::parse(&mode).is_none() {
        return Err(format!(
            "{id}: alpha_mode `{mode}` is not one of `opaque`, `cutout` or `blend`"
        ));
    }
    Ok(Some(mode.to_lowercase()))
}

/// Surface-response, alpha and reflection fields of one catalog entry, validated.
struct ValidatedResponse {
    normal_texture: Option<String>,
    normal_strength: Option<f32>,
    specular: Option<f32>,
    specular_color: Option<[f32; 3]>,
    shine: Option<f32>,
    roughness: Option<f32>,
    alpha_mode: Option<String>,
    opacity: Option<f32>,
    alpha_cutoff: Option<f32>,
    reflection_mode: Option<String>,
    reflection_strength: Option<f32>,
}

#[derive(serde::Deserialize)]
struct CatalogEntryFile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    asset_class: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    asset_type: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    size: Option<[f32; 3]>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    solid: bool,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    texture: Option<String>,
    #[serde(default)]
    tile_metres: Option<f32>,
    #[serde(default)]
    tint: Option<[f32; 3]>,
    /// Raw `emissive` channels; the exact length is validated on conversion so
    /// a wrong channel count gets a descriptive error, not a serde error.
    #[serde(default)]
    emissive: Option<Vec<f32>>,
    #[serde(default)]
    emissive_intensity: Option<f32>,
    #[serde(default)]
    emissive_mask: Option<String>,
    /// Surface response: an optional normal map with a strength, a sheen
    /// strength/colour and a roughness.
    #[serde(default)]
    normal_texture: Option<String>,
    #[serde(default)]
    normal_strength: Option<f32>,
    #[serde(default)]
    specular: Option<f32>,
    #[serde(default)]
    specular_color: Option<Vec<f32>>,
    /// Author-facing glossiness; the shader-facing `roughness` is its inverse
    /// and stays accepted for catalogs authored before `shine` existed.
    #[serde(default)]
    shine: Option<f32>,
    #[serde(default)]
    roughness: Option<f32>,
    /// Alpha: `opaque` | `cutout` | `blend`, plus the opacity multiplier and
    /// the cut-out threshold.
    #[serde(default)]
    alpha_mode: Option<String>,
    #[serde(default)]
    opacity: Option<f32>,
    #[serde(default)]
    alpha_cutoff: Option<f32>,
    /// Selective reflections: `none` | `probe` | `planar`, plus the weight of
    /// the reflected image.
    #[serde(default)]
    reflection_mode: Option<String>,
    #[serde(default)]
    reflection_strength: Option<f32>,
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

impl CatalogEntryFile {
    /// Converts one file entry. `legacy` entries come from the `props` array and
    /// default their class/type; `assets` entries must declare them.
    fn convert(&self, legacy: bool) -> Result<Option<AssetEntry>, String> {
        let Some(id) = self.logical_id(legacy)? else {
            return Ok(None);
        };
        let asset_class = self.resolve_class(&id, legacy)?;
        let asset_type = self.resolve_type(&id, legacy)?;
        let theme = parse_optional_slug(self.theme.as_deref(), "theme", &id, AssetTheme::parse)?;
        let model = self.resolve_model(&id)?;
        let texture = self.resolve_texture(&id, &asset_type)?;
        let tile_metres = self.resolve_tile_metres(&id, &asset_type)?;
        let tint = self.resolve_tint(&id, &asset_type)?;
        let source = self.resolve_source(&id, model.as_deref(), texture.as_deref())?;
        let emission = self.resolve_emissive(&id, &asset_type, source)?;
        let response = self.resolve_response(&id, &asset_type, source)?;
        let size = self
            .size
            .filter(|size| size.iter().all(|value| value.is_finite() && *value > 0.0));
        Ok(Some(AssetEntry {
            display_name: self.resolved_display_name(&id),
            id,
            asset_class,
            theme,
            asset_type,
            source,
            model,
            size,
            color: self.color.as_deref().and_then(parse_hex_color),
            category: self
                .category
                .as_deref()
                .map(str::trim)
                .filter(|category| !category.is_empty())
                .map(str::to_string),
            solid: self.solid,
            surface: self
                .surface
                .as_deref()
                .map(str::trim)
                .filter(|surface| !surface.is_empty())
                .map(str::to_string),
            texture,
            tile_metres,
            tint,
            emissive: emission.color,
            emissive_intensity: emission.intensity,
            emissive_mask: emission.mask,
            normal_texture: response.normal_texture,
            normal_strength: response.normal_strength,
            specular: response.specular,
            specular_color: response.specular_color,
            shine: response.shine,
            roughness: response.roughness,
            alpha_mode: response.alpha_mode,
            opacity: response.opacity,
            alpha_cutoff: response.alpha_cutoff,
            reflection_mode: response.reflection_mode,
            reflection_strength: response.reflection_strength,
            entity_type: self
                .entity_type
                .as_deref()
                .map(str::trim)
                .filter(|entity_type| !entity_type.is_empty())
                .map(str::to_string),
            description: self
                .description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .map(str::to_string),
            tags: self.tags.clone(),
        }))
    }

    /// Trims and validates the logical id; `Ok(None)` for a legacy entry that
    /// declares none.
    fn logical_id(&self, legacy: bool) -> Result<Option<String>, String> {
        let id = self.id.trim().to_string();
        if id.is_empty() {
            if legacy {
                return Ok(None);
            }
            return Err("asset entry with an empty id".to_string());
        }
        if !is_valid_asset_id(&id) {
            return Err(format!(
                "asset id `{id}` is malformed; ids are names such as `core:chair` or `spooner-man`"
            ));
        }
        Ok(Some(id))
    }

    /// The asset class, defaulting to the legacy environment for `props` data.
    fn resolve_class(&self, id: &str, legacy: bool) -> Result<AssetClass, String> {
        match self.asset_class.as_deref().map(str::trim) {
            Some(class) if !class.is_empty() => AssetClass::parse(class),
            _ if legacy => AssetClass::parse(AssetClass::ENVIRONMENT),
            _ => Err(format!("{id}: missing `asset_class`")),
        }
    }

    /// The asset type, defaulting to the legacy prop for `props` data.
    fn resolve_type(&self, id: &str, legacy: bool) -> Result<AssetType, String> {
        match self.asset_type.as_deref().map(str::trim) {
            Some(asset_type) if !asset_type.is_empty() => AssetType::parse(asset_type),
            _ if legacy => AssetType::parse(AssetType::PROP),
            _ => Err(format!("{id}: missing `asset_type`")),
        }
    }

    /// The trimmed model path, rejected when it could escape the asset root.
    fn resolve_model(&self, id: &str) -> Result<Option<String>, String> {
        let model = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string);
        if let Some(model) = &model
            && (!is_relative_resource_path(model))
        {
            return Err(format!(
                "{id}: model path `{model}` must be a relative path below the asset root"
            ));
        }
        Ok(model)
    }

    /// The trimmed texture id a material draws with.
    fn resolve_texture(&self, id: &str, asset_type: &AssetType) -> Result<Option<String>, String> {
        let texture = self
            .texture
            .as_deref()
            .map(str::trim)
            .filter(|texture| !texture.is_empty())
            .map(str::to_string);
        if let Some(texture) = &texture {
            if !is_valid_asset_id(texture) {
                return Err(format!(
                    "{id}: texture `{texture}` is not a well-formed logical asset id"
                ));
            }
            if asset_type.as_str() != AssetType::MATERIAL {
                return Err(format!(
                    "{id}: only a `material` asset may declare a `texture`"
                ));
            }
        }
        if asset_type.as_str() == AssetType::MATERIAL && texture.is_none() {
            return Err(format!(
                "{id}: a material must declare the logical `texture` it draws with"
            ));
        }
        Ok(texture)
    }

    /// The validated `tile_metres` of a material.
    fn resolve_tile_metres(&self, id: &str, asset_type: &AssetType) -> Result<Option<f32>, String> {
        let Some(value) = self.tile_metres else {
            return Ok(None);
        };
        if asset_type.as_str() != AssetType::MATERIAL {
            return Err(format!(
                "{id}: only a `material` asset may declare `tile_metres`"
            ));
        }
        if !value.is_finite() || !(MIN_TILE_METRES..=MAX_TILE_METRES).contains(&value) {
            return Err(format!(
                "{id}: tile_metres must be between {MIN_TILE_METRES} and {MAX_TILE_METRES} metres, found {value}"
            ));
        }
        Ok(Some(value))
    }

    /// The validated static tint of a material.
    fn resolve_tint(&self, id: &str, asset_type: &AssetType) -> Result<Option<[f32; 3]>, String> {
        let Some(tint) = self.tint else {
            return Ok(None);
        };
        if asset_type.as_str() != AssetType::MATERIAL {
            return Err(format!(
                "{id}: only a `material` asset may declare a `tint`"
            ));
        }
        if !tint
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(format!(
                "{id}: tint must be three channels between 0.0 and 1.0, found {tint:?}"
            ));
        }
        Ok(Some(tint))
    }

    /// The validated emission of a material: colour, optional intensity and
    /// optional mask id.
    ///
    /// Emission changes how a surface *reads*, so it is only meaningful on a
    /// material definition: a prop, texture, light or generated asset may not
    /// author any of the three fields. An intensity or mask without a colour
    /// is rejected because there would be nothing for it to shape.
    fn resolve_emissive(
        &self,
        id: &str,
        asset_type: &AssetType,
        source: AssetSource,
    ) -> Result<ValidatedEmission, String> {
        let mask = self
            .emissive_mask
            .as_deref()
            .map(str::trim)
            .filter(|mask| !mask.is_empty())
            .map(str::to_string);
        if self.emissive.is_none() && self.emissive_intensity.is_none() && mask.is_none() {
            return Ok(ValidatedEmission {
                color: None,
                intensity: None,
                mask: None,
            });
        }
        if asset_type.as_str() != AssetType::MATERIAL || source != AssetSource::Definition {
            return Err(format!(
                "{id}: only a `material` `definition` asset may declare `emissive`, \
                 `emissive_intensity` or `emissive_mask`"
            ));
        }
        let Some(raw) = self.emissive.as_deref() else {
            return Err(if self.emissive_intensity.is_some() {
                format!("{id}: `emissive_intensity` requires an `emissive` colour")
            } else {
                format!("{id}: `emissive_mask` requires an `emissive` colour")
            });
        };
        if raw.len() != 3 {
            return Err(format!(
                "{id}: emissive must be exactly three channels, found {} in {raw:?}",
                raw.len()
            ));
        }
        let mut color = [0.0_f32; 3];
        for (slot, channel) in color.iter_mut().zip(raw) {
            if !channel.is_finite() || !(0.0..=MAX_EMISSION_COLOR).contains(channel) {
                return Err(format!(
                    "{id}: emissive channels must be between 0.0 and {MAX_EMISSION_COLOR:?}, found {raw:?}"
                ));
            }
            *slot = *channel;
        }
        if let Some(intensity) = self.emissive_intensity
            && (!intensity.is_finite() || !(0.0..=MAX_EMISSION_INTENSITY).contains(&intensity))
        {
            return Err(format!(
                "{id}: emissive_intensity must be between 0.0 and {MAX_EMISSION_INTENSITY:?}, found {intensity:?}"
            ));
        }
        if let Some(mask) = &mask
            && !is_valid_asset_id(mask)
        {
            return Err(format!(
                "{id}: emissive_mask `{mask}` is not a well-formed logical asset id"
            ));
        }
        Ok(ValidatedEmission {
            color: Some(color),
            intensity: self.emissive_intensity,
            mask,
        })
    }

    /// Validates the surface-response, alpha and reflection fields.
    ///
    /// A material that declares none of them renders as a legacy flat-shaded
    /// material: no normal map, no sheen and opaque. Every authored value is
    /// range-checked here so a malformed entry is a named catalog error rather
    /// than a silently different surface.
    fn resolve_response(
        &self,
        id: &str,
        asset_type: &AssetType,
        source: AssetSource,
    ) -> Result<ValidatedResponse, String> {
        let normal_texture = self
            .normal_texture
            .as_deref()
            .map(str::trim)
            .filter(|texture| !texture.is_empty())
            .map(str::to_string);
        let alpha_mode = self
            .alpha_mode
            .as_deref()
            .map(str::trim)
            .filter(|mode| !mode.is_empty())
            .map(str::to_string);
        let untouched = normal_texture.is_none()
            && self.normal_strength.is_none()
            && self.specular.is_none()
            && self.specular_color.is_none()
            && self.shine.is_none()
            && self.roughness.is_none()
            && alpha_mode.is_none()
            && self.opacity.is_none()
            && self.alpha_cutoff.is_none()
            && self.reflection_mode.is_none()
            && self.reflection_strength.is_none();
        if untouched {
            return Ok(ValidatedResponse {
                normal_texture: None,
                normal_strength: None,
                specular: None,
                specular_color: None,
                shine: None,
                roughness: None,
                alpha_mode: None,
                opacity: None,
                alpha_cutoff: None,
                reflection_mode: None,
                reflection_strength: None,
            });
        }
        if asset_type.as_str() != AssetType::MATERIAL || source != AssetSource::Definition {
            return Err(format!(
                "{id}: only a `material` `definition` asset may declare surface-response \
                 (`normal_texture`, `normal_strength`, `specular`, `specular_color`, `shine`, \
                 `roughness`), alpha (`alpha_mode`, `opacity`, `alpha_cutoff`) or a \
                 reflection (`reflection_mode`, `reflection_strength`) field"
            ));
        }
        if self.shine.is_some() && self.roughness.is_some() {
            return Err(format!(
                "{id}: author either `shine` or `roughness`, not both; `shine` is the \
                 author-facing spelling (`roughness` is its inverse)"
            ));
        }
        if let Some(texture) = &normal_texture
            && !is_valid_asset_id(texture)
        {
            return Err(format!(
                "{id}: normal_texture `{texture}` is not a well-formed logical asset id"
            ));
        }
        if self.normal_strength.is_some() && normal_texture.is_none() {
            return Err(format!(
                "{id}: `normal_strength` requires a `normal_texture`"
            ));
        }
        if let Some(value) = self.normal_strength
            && (!value.is_finite() || !(0.0..=MAX_NORMAL_STRENGTH).contains(&value))
        {
            return Err(format!(
                "{id}: normal_strength must be between 0.0 and {MAX_NORMAL_STRENGTH:?}, found {value:?}"
            ));
        }
        let specular_color = specular_color(id, self.specular_color.as_deref())?;
        let alpha_mode = validated_alpha_mode(id, alpha_mode)?;
        let (reflection_mode, reflection_strength) = validate_reflection(id, self)?;
        if (self.opacity.is_some() || self.alpha_cutoff.is_some()) && alpha_mode.is_none() {
            return Err(format!(
                "{id}: `opacity` and `alpha_cutoff` require an explicit `alpha_mode`"
            ));
        }
        Ok(ValidatedResponse {
            normal_texture,
            normal_strength: self.normal_strength,
            specular: unit_number(self.specular, "specular")
                .map_err(|field| format!("{id}: {field}"))?,
            specular_color,
            shine: unit_number(self.shine, "shine").map_err(|field| format!("{id}: {field}"))?,
            roughness: unit_number(self.roughness, "roughness")
                .map_err(|field| format!("{id}: {field}"))?,
            alpha_mode,
            opacity: unit_number(self.opacity, "opacity")
                .map_err(|field| format!("{id}: {field}"))?,
            alpha_cutoff: unit_number(self.alpha_cutoff, "alpha_cutoff")
                .map_err(|field| format!("{id}: {field}"))?,
            reflection_mode,
            reflection_strength,
        })
    }

    /// Where the resource comes from, inferred from the declared model/texture
    /// when no explicit `source` is given.
    fn resolve_source(
        &self,
        id: &str,
        model: Option<&str>,
        texture: Option<&str>,
    ) -> Result<AssetSource, String> {
        let Some(raw) = self.source.as_deref().map(str::trim) else {
            return Ok(inferred_source(model, texture));
        };
        if raw.is_empty() {
            return Ok(inferred_source(model, texture));
        }
        let source = AssetSource::parse(raw).map_err(|error| format!("{id}: {error}"))?;
        if source == AssetSource::File && model.is_none() {
            return Err(format!("{id}: a `file` asset must declare a `model` path"));
        }
        if source == AssetSource::Generated && model.is_some() {
            return Err(format!(
                "{id}: a `generated` asset must not declare a `model`"
            ));
        }
        if source == AssetSource::Definition {
            if model.is_some() {
                return Err(format!(
                    "{id}: a `definition` asset must not declare a `model`"
                ));
            }
            if texture.is_none() {
                return Err(format!(
                    "{id}: a `definition` asset must declare a `texture` to draw with"
                ));
            }
        }
        if model.is_some() && texture.is_some() {
            return Err(format!(
                "{id}: an asset cannot be both a file resource and a texture-backed material"
            ));
        }
        if source == AssetSource::File && texture.is_some() {
            return Err(format!(
                "{id}: a `file` asset must not declare a `texture`; use `definition`"
            ));
        }
        Ok(source)
    }

    /// The human-readable name: `display_name`, then the legacy `name`, then
    /// the logical id.
    fn resolved_display_name(&self, id: &str) -> String {
        let display = self.display_name.trim();
        if !display.is_empty() {
            return display.to_string();
        }
        let legacy_name = self.name.trim();
        if legacy_name.is_empty() {
            id.to_string()
        } else {
            legacy_name.to_string()
        }
    }
}

/// The source implied by the declared model/texture.
const fn inferred_source(model: Option<&str>, texture: Option<&str>) -> AssetSource {
    if texture.is_some() {
        AssetSource::Definition
    } else if model.is_some() {
        AssetSource::File
    } else {
        AssetSource::Generated
    }
}

/// True when a catalog resource path is relative and cannot escape the root.
fn is_relative_resource_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|component| component == ".." || component.is_empty())
}

/// Checks that a material's emissive mask names a loadable texture asset.
///
/// The mask follows exactly the albedo `texture` rule: it must be a declared
/// catalog texture with a file behind it. A dangling mask would otherwise only
/// surface as a runtime diagnostic fallback.
fn check_emissive_mask(entry: &AssetEntry, catalog: &AssetCatalog) -> Result<(), String> {
    let Some(mask_id) = entry.emissive_mask.as_deref() else {
        return Ok(());
    };
    let Some(mask) = catalog.entries.get(mask_id) else {
        return Err(format!(
            "{}: emissive mask `{mask_id}` is not declared in the asset catalog",
            entry.id
        ));
    };
    if !mask.is_texture() {
        return Err(format!(
            "{}: emissive mask `{mask_id}` is a `{}` asset, not a texture",
            entry.id,
            mask.asset_type.as_str()
        ));
    }
    if mask.source != AssetSource::File {
        return Err(format!(
            "{}: emissive mask `{mask_id}` has no PNG file to load",
            entry.id
        ));
    }
    Ok(())
}

/// True when a resource path names a PNG (case-insensitive extension).
#[must_use]
pub fn has_png_extension(path: Option<&str>) -> bool {
    path.is_some_and(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    })
}

impl AssetCatalog {
    /// Empty catalog; every lookup falls back.
    #[must_use]
    pub fn builtin() -> Self {
        Self::default()
    }

    /// Parses a catalog document.
    ///
    /// Accepts the current `assets` array and the legacy `props` array so old
    /// tooling keeps working. Duplicate logical ids are rejected: two entries
    /// claiming `spooner-man` is a catalog error, never last-one-wins.
    /// # Errors
    ///
    /// Returns a message when the document is not valid JSON, when a logical id
    /// is duplicated or malformed, or when an entry declares a malformed
    /// class/theme/type/source/resource path.
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        let file: CatalogFile =
            serde_json::from_str(json).map_err(|e| format!("Invalid asset catalog JSON: {e}"))?;

        let mut catalog = Self::default();
        let mut theme_ids = std::collections::HashSet::new();
        for theme in file.themes {
            let id = theme.id.trim();
            if id.is_empty() {
                return Err("theme entry with an empty id".to_string());
            }
            let id = AssetTheme::parse(id)?;
            if !theme_ids.insert(id.clone()) {
                return Err(format!("duplicate theme id `{id}` in the asset catalog"));
            }
            let display_name = {
                let display = theme.display_name.trim();
                if display.is_empty() {
                    let legacy = theme.name.trim();
                    if legacy.is_empty() {
                        id.to_string()
                    } else {
                        legacy.to_string()
                    }
                } else {
                    display.to_string()
                }
            };
            catalog.themes.push(AssetThemeDef {
                id,
                display_name,
                description: theme.description.trim().to_string(),
            });
        }

        for (entry, legacy) in file
            .assets
            .iter()
            .map(|entry| (entry, false))
            .chain(file.props.iter().map(|entry| (entry, true)))
        {
            let Some(entry) = entry.convert(legacy)? else {
                continue;
            };
            if catalog.entries.contains_key(&entry.id) {
                return Err(format!(
                    "duplicate asset id `{}` in the asset catalog",
                    entry.id
                ));
            }
            catalog.entries.insert(entry.id.clone(), entry);
        }

        // Second pass: every material must reference a texture this catalog
        // actually declares (its albedo and any emissive mask), every texture
        // asset must be a PNG, and every file-backed fixture must name the PNG
        // face it draws. Doing this after all entries exist means a material
        // may be declared before the texture it draws with, but never with a
        // dangling reference.
        let entries: Vec<&AssetEntry> = catalog.entries.values().collect();
        for entry in entries {
            check_emissive_mask(entry, &catalog)?;
            if entry.is_texture() && !has_png_extension(entry.model.as_deref()) {
                return Err(format!(
                    "{}: a texture asset must name a `.png` file, found `{}`",
                    entry.id,
                    entry.model.as_deref().unwrap_or("(no model)")
                ));
            }
            if entry.is_light()
                && entry.source == AssetSource::File
                && !has_png_extension(entry.model.as_deref())
            {
                return Err(format!(
                    "{}: a file-backed light fixture must name a `.png` sheet, found `{}`",
                    entry.id,
                    entry.model.as_deref().unwrap_or("(no model)")
                ));
            }
            let Some(texture_id) = entry.texture.as_deref() else {
                continue;
            };
            let Some(texture) = catalog.entries.get(texture_id) else {
                return Err(format!(
                    "{}: material texture `{texture_id}` is not declared in the asset catalog",
                    entry.id
                ));
            };
            if !texture.is_texture() {
                return Err(format!(
                    "{}: material texture `{texture_id}` is a `{}` asset, not a texture",
                    entry.id,
                    texture.asset_type.as_str()
                ));
            }
            if texture.source != AssetSource::File {
                return Err(format!(
                    "{}: material texture `{texture_id}` has no PNG file to load",
                    entry.id
                ));
            }
        }
        Ok(catalog)
    }

    /// Loads a catalog from `path`, returning `None` when the file is missing
    /// or invalid. Never panics.
    #[must_use]
    pub fn load_from_path(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        match Self::from_json_str(&content) {
            Ok(catalog) => Some(catalog),
            Err(error) => {
                crate::logging::warn_once(
                    format!("catalog:{}", path.display()),
                    format!("[assets] {}: {error}", path.display()),
                );
                None
            }
        }
    }

    /// Loads the shipped catalog, falling back to an empty catalog with a
    /// developer-facing message when no catalog can be found.
    ///
    /// The missing-asset-root report is printed once per process even though
    /// the catalog is loaded by several independent callers.
    #[must_use]
    pub fn load_default() -> Self {
        for candidate in catalog_path_candidates() {
            if let Some(catalog) = Self::load_from_path(&candidate) {
                return catalog;
            }
        }
        let mut message = String::from(NO_ASSET_ROOT_MESSAGE);
        for (path, exists) in asset_root_search_report() {
            let _ = std::fmt::Write::write_fmt(
                &mut message,
                format_args!(
                    "\n  {} {}",
                    if exists { "found   " } else { "missing " },
                    path.display()
                ),
            );
        }
        let _ = std::fmt::Write::write_fmt(
            &mut message,
            format_args!(
                "\n  set {ASSET_ROOT_ENV}=<directory containing assets/> to override, \
                 or run from a directory that contains assets/."
            ),
        );
        crate::logging::warn_once("no-asset-root", message);
        Self::builtin()
    }

    /// The entry with this logical id, if the catalog declares it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AssetEntry> {
        self.entries.get(id)
    }

    /// The entry with this logical id when it is placeable (prop or entity).
    #[must_use]
    pub fn placeable(&self, id: &str) -> Option<&AssetEntry> {
        self.get(id).filter(|entry| entry.is_placeable())
    }

    /// The entry with this logical id when it is a surface material.
    #[must_use]
    pub fn material(&self, id: &str) -> Option<&AssetEntry> {
        self.get(id).filter(|entry| entry.is_material())
    }

    /// The logical texture id a material draws with, if it declares one.
    #[must_use]
    pub fn material_texture(&self, id: &str) -> Option<&str> {
        self.material(id)?.texture.as_deref()
    }

    /// The emissive colour a material authors; `None` when it emits nothing.
    ///
    /// Only a material can carry emission, so a prop or texture id is `None`
    /// here even if an entry somehow declared one: emission can never be read
    /// off a non-material.
    #[must_use]
    pub fn material_emissive(&self, id: &str) -> Option<[f32; 3]> {
        self.material(id)?.emissive
    }

    /// The scalar emissive intensity a material authors, if it authors one.
    ///
    /// `None` means "use the default for an authored colour"; it is not the
    /// same as an authored `0.0`, which keeps the material dark.
    #[must_use]
    pub fn material_emissive_intensity(&self, id: &str) -> Option<f32> {
        self.material(id)?.emissive_intensity
    }

    /// The logical texture id of a material's emissive mask, if it declares one.
    #[must_use]
    pub fn material_emissive_mask(&self, id: &str) -> Option<&str> {
        self.material(id)?.emissive_mask.as_deref()
    }

    /// The canonical PNG path of a texture asset, relative to the asset root.
    #[must_use]
    pub fn texture_path(&self, id: &str) -> Option<&str> {
        self.get(id)
            .filter(|entry| entry.is_texture() && entry.source == AssetSource::File)
            .and_then(|entry| entry.model.as_deref())
    }

    /// The canonical PNG of a file-backed light fixture's visible face.
    ///
    /// A built-in fixture's mesh is generated in code, but what that mesh shows
    /// is ordinary external artwork: the light entry names its own `.png` sheet
    /// exactly like a file-backed decal, and the renderer resolves it through
    /// the same catalog -> PNG -> texture-cache path a surface texture uses.
    /// `None` for an unknown id, a generated fixture or a non-PNG resource.
    #[must_use]
    pub fn fixture_sheet_path(&self, id: &str) -> Option<&str> {
        self.get(id)
            .filter(|entry| entry.is_light() && entry.source == AssetSource::File)
            .and_then(|entry| entry.model.as_deref())
            .filter(|model| has_png_extension(Some(model)))
    }

    /// Every material entry, ordered by id.
    #[must_use]
    pub fn materials(&self) -> Vec<&AssetEntry> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.is_material())
            .collect()
    }

    /// Every texture entry, ordered by id.
    #[must_use]
    pub fn textures(&self) -> Vec<&AssetEntry> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.is_texture())
            .collect()
    }

    /// True when the catalog declares this logical id.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// Number of catalog entries of every class.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the catalog has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry, ordered by id so validation and reports are stable.
    #[must_use]
    pub fn entries(&self) -> Vec<&AssetEntry> {
        let mut entries: Vec<&AssetEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    /// Every placeable entry (prop or entity), ordered by id.
    #[must_use]
    pub fn placeable_entries(&self) -> Vec<&AssetEntry> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.is_placeable())
            .collect()
    }

    /// The declared environment themes, in catalog order.
    #[must_use]
    pub fn themes(&self) -> &[AssetThemeDef] {
        &self.themes
    }
}

/// Parses a `#rrggbb` catalog colour. The leading `#` is optional.
#[must_use]
pub fn parse_hex_color(value: &str) -> Option<[f32; 3]> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut components = [0.0_f32; 3];
    for (component, pair) in components.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
        // Every byte was validated as a hex digit above, so both conversions
        // succeed; `unwrap_or(0)` keeps the historical fallback.
        let text = std::str::from_utf8(pair).unwrap_or("");
        *component = f32::from(u8::from_str_radix(text, 16).unwrap_or(0)) / 255.0;
    }
    Some(components)
}

#[cfg(test)]
mod tests;

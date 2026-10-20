//! Player-side compiled package loading.
//!
//! The player opens a package, reads its manifest and validated semantics once
//! for discovery and material resolution, then reads exactly one quality
//! variant's prepared records for installation. Nothing here emits geometry,
//! bakes light, plans charts or captures reflections: the records are the
//! compiler's finished products, decoded under the package's own bounds.
//!
//! Reflection captures are required for a variant that renders reflections:
//! the player uploads the compiler's cubemaps instead of rendering its own.
//! A package built without the GPU capture step is refused with an actionable
//! error rather than silently rendering without reflections.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::lightmap::LevelLightmaps;
use crate::engine::materials::MaterialTable;
use crate::engine::package::collision::{CompiledCollision, read_collision};
use crate::engine::package::ktx2::{self, Ktx2Rgba16f};
use crate::engine::package::lighting::read_lighting;
use crate::engine::package::lightmaps::read_lightmaps_owned;
use crate::engine::package::manifest::{Manifest, ProbePayload, Variant};
use crate::engine::package::mesh::read_mesh;
use crate::engine::package::props::read_props;
use crate::engine::package::{
    MAX_BINARY_BYTES, MAX_COLLISION_BYTES, MAX_ENTRY_BYTES, MAX_LIGHTING_BYTES,
    MAX_LIGHTMAP_ATLAS_BYTES, MAX_LIGHTMAP_METADATA_BYTES, MAX_MATERIALS_BYTES,
    MAX_NAVIGATION_BYTES, MAX_SEMANTICS_BYTES, PackageReader,
};
use crate::engine::quality::LightmapQuality;
use crate::engine::render::{LevelMesh, PropMeshBatch};

#[derive(Deserialize)]
struct CatalogBuildInput {
    revision: u32,
    catalog_sha256: String,
}

/// The part of a package a discovery or material pass needs: the manifest and
/// the validated semantic level.
#[derive(Clone, Debug)]
pub struct OpenedPackage {
    /// Package manifest.
    pub manifest: Manifest,
    /// Validated, authoring-prepared level semantics.
    pub level: LevelDef,
}

/// One prepared quality variant, decoded and ready for installation.
pub struct LoadedVariant {
    pub static_light_off: Option<Arc<crate::engine::render::StaticLightState>>,
    /// The lightmap quality this variant answers.
    pub lightmap_quality: LightmapQuality,
    /// Prepared static geometry.
    pub mesh: LevelMesh,
    /// Prepared static prop batches, with textures attached from the catalog
    /// models the manifest identifies.
    pub props: Vec<PropMeshBatch>,
    /// The compiler's baked lighting.
    pub lighting: LevelLighting,
    /// Prepared lightmap atlas, absent for the vertex-lit variant or a
    /// recorded fill fallback.
    pub lightmaps: Option<Arc<LevelLightmaps>>,
    /// The prepared irradiance field for moving objects, when the variant has
    /// one.
    pub irradiance: Option<Arc<crate::engine::lighting::probes::ProbeField>>,
    /// Compiled static collision.
    pub collision: CompiledCollision,
    /// The baked navigation grid this variant was compiled with.
    pub navigation: crate::engine::package::navigation::NavGrid,
    /// Prepared reflection probe captures, keyed by face size.
    pub probes: ProbeCaptures,
}

/// Reflection probe captures for both runtime face sizes.
#[derive(Clone, Debug, Default)]
pub struct ProbeCaptures {
    /// 48-texel captures (Reflections Medium).
    pub medium: Option<ProbeCapture>,
    /// 64-texel captures (Reflections Full).
    pub full: Option<ProbeCapture>,
}

impl ProbeCaptures {
    /// True when both runtime face sizes are present.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.medium.is_some() && self.full.is_some()
    }

    /// The capture for one face size.
    #[must_use]
    pub const fn for_face_size(&self, face_edge: u32) -> Option<&ProbeCapture> {
        match face_edge {
            crate::engine::render::PROBE_FACE_SIZE_MEDIUM => self.medium.as_ref(),
            crate::engine::render::PROBE_FACE_SIZE_FULL => self.full.as_ref(),
            _ => None,
        }
    }
}

/// One face-size set of probe captures.
#[derive(Clone, Debug)]
pub struct ProbeCapture {
    /// Base face edge in texels.
    pub face_edge: u32,
    /// Mip levels every probe chain carries, base level first.
    pub levels: u32,
    /// Probe positions, in routing order.
    pub points: Vec<[f32; 3]>,
    /// One mip chain per probe, in routing order: level 0 first, six RGBA8
    /// faces per level, each level's faces sized `(face_edge >> level)`.
    pub chains: Vec<Vec<[Vec<u8>; 6]>>,
}

/// Positions record of one probe payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbePositions {
    /// Record version.
    pub record_version: u16,
    /// Face edge this payload was captured at.
    pub face_edge: u32,
    /// Mip levels every probe chain in this payload carries.
    pub levels: u32,
    /// Probe positions in routing order.
    pub points: Vec<[f32; 3]>,
}

/// Current pre-release probe positions version; superseded layouts require recompilation.
pub const PROBE_POSITIONS_VERSION: u16 = 1;

/// Opens a package and reads its manifest and semantics.
///
/// # Errors
///
/// Returns an error when the archive, manifest, semantics or level validation
/// fails, or a declared semantics/resource hash does not match the manifest.
pub fn open(path: &Path) -> Result<OpenedPackage, String> {
    let file = super::archive::open_file(path)?;
    open_reader(PackageReader::new(file).map_err(|error| error.to_string())?)
}

/// Opens only the explicitly selected level in a project package.
/// # Errors
/// Returns package validation errors or an undeclared identity.
pub fn open_level(path: &Path, id: &str) -> Result<OpenedPackage, String> {
    let file = super::archive::open_file(path)?;
    let mut reader = PackageReader::new(file).map_err(|e| e.to_string())?;
    super::project::select(&mut reader, Some(id))?;
    let opened = open_reader(reader)?;
    if opened.level.id != id {
        return Err(format!("Package does not contain requested level '{id}'"));
    }
    Ok(opened)
}

/// Opens an in-memory package for import and compiler controls.
///
/// # Errors
///
/// Returns the same errors as [`open`].
pub fn open_bytes(bytes: &[u8]) -> Result<OpenedPackage, String> {
    open_reader(package_reader(bytes)?)
}

fn package_reader(bytes: &[u8]) -> Result<PackageReader<std::io::Cursor<&[u8]>>, String> {
    PackageReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())
}

fn open_reader<R: std::io::Read + std::io::Seek>(
    reader: PackageReader<R>,
) -> Result<OpenedPackage, String> {
    let root = crate::engine::assets::resolve_asset_root();
    open_reader_with_asset_root(reader, root.as_deref())
}

fn open_reader_with_asset_root<R: std::io::Read + std::io::Seek>(
    mut reader: PackageReader<R>,
    root: Option<&Path>,
) -> Result<OpenedPackage, String> {
    let manifest = read_manifest(&mut reader)?;
    let semantics = read_named_entry(
        &mut reader,
        &manifest,
        "semantics.json",
        MAX_SEMANTICS_BYTES,
    )?;
    let text = std::str::from_utf8(&semantics)
        .map_err(|error| format!("semantics.json is not UTF-8: {error}"))?;
    let level = LevelDef::from_json(text)
        .map_err(|error| format!("package semantics are not valid: {error}"))?;
    validate_semantics(&reader, &manifest, &level)?;
    check_catalog_build_input(&mut reader, &manifest, root)?;
    check_dependencies(&manifest, root)?;
    Ok(OpenedPackage { manifest, level })
}

/// Shared compiler/player semantic admission, including package-only ownership
/// and capability contracts that ordinary editable level validation cannot know.
pub(crate) fn validate_semantics<R: std::io::Read + std::io::Seek>(
    reader: &PackageReader<R>,
    manifest: &Manifest,
    level: &LevelDef,
) -> Result<(), String> {
    if manifest.id != level.id {
        return Err("Package manifest and semantic level IDs disagree".into());
    }
    for destination in &level.transitions {
        if !reader.permits_project_level(destination)
            || !manifest
                .required_capabilities
                .iter()
                .any(|capability| capability == "project-transitions-v1")
        {
            return Err("Level transitions require the complete project package and project-transitions-v1 capability; use Export project or Build+Play".into());
        }
    }
    crate::engine::loader::validate_level(level)?;
    if level.props.iter().any(|prop| prop.animated_door.is_some())
        && !manifest
            .required_capabilities
            .iter()
            .any(|capability| capability == "animated-door-v1")
    {
        return Err("Animated solid mechanisms require animated-door-v1 capability".into());
    }
    validate_shadow_only(level, manifest)?;
    validate_item_receipts(level, manifest)?;
    validate_lighting_requirement(level, manifest)?;
    crate::engine::scripting::validate_package_sources(level, &manifest.required_capabilities)
}

/// Hidden entities must never silently become visible on an older player.
pub(crate) fn validate_shadow_only(level: &LevelDef, manifest: &Manifest) -> Result<(), String> {
    if level.has_shadow_only_entities()
        && !manifest
            .required_capabilities
            .iter()
            .any(|capability| capability == "shadow-only-v1")
    {
        return Err("Shadow-only entities require shadow-only-v1 capability".into());
    }
    Ok(())
}

/// Nondefault receipt policies must never be ignored by an older host.
pub(crate) fn validate_item_receipts(level: &LevelDef, manifest: &Manifest) -> Result<(), String> {
    if level
        .volumes
        .iter()
        .any(|volume| volume.item_receipt != crate::engine::level::ItemReceiptDef::Overlap)
        && !manifest
            .required_capabilities
            .iter()
            .any(|capability| capability == "item-receipts-v1")
    {
        return Err("Centre/settled item receipts require item-receipts-v1 capability".into());
    }
    Ok(())
}

/// Required presentation policy is semantic; never accept a package whose
/// prepared variants cannot satisfy it, or let older hosts ignore the policy.
pub(crate) fn validate_lighting_requirement(
    level: &LevelDef,
    manifest: &Manifest,
) -> Result<(), String> {
    if level.requires_full_lighting
        && (!manifest.variants.iter().any(|variant| {
            variant.lightmap_quality == "full"
                && variant.entries.lightmaps.is_some()
                && variant.entries.lightmaps_meta.is_some()
                && variant.lightmap_failure.is_none()
        }) || !manifest
            .required_capabilities
            .iter()
            .any(|capability| capability == "full-lighting-v1"))
    {
        return Err("This map requires a prepared Full lighting atlas and full-lighting-v1 capability; rebuild its package with Full lighting (vertex fallback is unavailable for this map)".into());
    }
    Ok(())
}

/// Optional compiler provenance pins new packages to their prepared catalog.
/// Older packages without this record retain their existing format contract.
fn check_catalog_build_input<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    root: Option<&Path>,
) -> Result<(), String> {
    const NAME: &str = "build-inputs.json";
    let Some(entry) = manifest.entry(NAME) else {
        return Ok(());
    };
    if entry.role != "compiler-inputs" || entry.bytes > crate::engine::package::MAX_MANIFEST_BYTES {
        return Err(format!(
            "package '{NAME}' has an invalid role or declared size"
        ));
    }
    let bytes = read_named_entry(
        reader,
        manifest,
        NAME,
        crate::engine::package::MAX_MANIFEST_BYTES,
    )?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != entry.bytes {
        return Err(format!(
            "package '{NAME}' length does not match its declared size"
        ));
    }
    // Source, tool and capture fields govern developer rebuilds; runtime only
    // consumes the catalog identity from this declared revision.
    let inputs: CatalogBuildInput = serde_json::from_slice(&bytes)
        .map_err(|error| format!("package '{NAME}' is malformed: {error}"))?;
    if inputs.revision != 1 {
        return Err(format!(
            "package '{NAME}' revision {} is unsupported; rebuild with places-compile",
            inputs.revision
        ));
    }
    if inputs.catalog_sha256.len() != 64
        || !inputs
            .catalog_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("package '{NAME}' has an invalid catalog SHA-256"));
    }
    let Some(installed_root) = root else {
        return Err("Package requires installed catalog.json for provenance verification; install the complete payload or set PLACES_ASSET_ROOT".into());
    };
    let actual = crate::engine::package::hash::sha256_file(&installed_root.join("catalog.json"))
        .map_err(|error| format!("package catalog.json could not be verified: {error}"))?;
    if actual != inputs.catalog_sha256 {
        return Err(format!(
            "package catalog.json SHA-256 changed (recorded {}, installed {actual}); \
             rebuild this package against the installed catalog with places-compile",
            inputs.catalog_sha256
        ));
    }
    Ok(())
}

/// Enforces the declared dependency identities before a package is used.
///
/// With an installed asset bundle, a declared model or texture must exist with
/// exactly the recorded size and SHA-256. The digest streams once during this
/// package-open pass; frame rendering never rehashes assets. A substituted or
/// resized resource is a load error naming the dependency. Required external
/// dependencies cannot be bypassed by running without an asset root.
fn check_dependencies(manifest: &Manifest, root: Option<&Path>) -> Result<(), String> {
    let Some(installed_root) = root else {
        return if manifest
            .dependencies
            .iter()
            .any(|dependency| dependency.kind != crate::engine::package::DependencyKind::Embedded)
        {
            Err("Package requires installed assets for dependency verification; install the complete payload or set PLACES_ASSET_ROOT".into())
        } else {
            Ok(())
        };
    };
    for dependency in &manifest.dependencies {
        if dependency.kind == crate::engine::package::DependencyKind::Embedded {
            continue;
        }
        super::dependencies::validate(installed_root, dependency)?;
    }
    Ok(())
}

/// Reads only the manifest of a package, verifying its structure and hashes.
///
/// # Errors
///
/// Returns an error when the archive cannot be opened or the manifest is
/// malformed.
pub fn inspect(path: &Path) -> Result<Manifest, String> {
    let file = super::archive::open_file(path)?;
    let mut reader = PackageReader::new(file).map_err(|error| error.to_string())?;
    read_manifest(&mut reader)
}

/// Reads only the explicitly selected map manifest for lightweight cache identity.
/// # Errors
/// Returns invalid archive/index/manifest errors or an undeclared map identity.
pub fn inspect_level(path: &Path, id: &str) -> Result<Manifest, String> {
    let file = super::archive::open_file(path)?;
    let mut reader = PackageReader::new(file).map_err(|e| e.to_string())?;
    super::project::select(&mut reader, Some(id))?;
    let manifest = read_manifest(&mut reader)?;
    if manifest.id != id {
        return Err(format!(
            "Package manifest does not describe requested level '{id}'"
        ));
    }
    Ok(manifest)
}

fn read_manifest<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
) -> Result<Manifest, String> {
    super::project::select(reader, None)?;
    let bytes = reader.read_entry("manifest.json", crate::engine::package::MAX_MANIFEST_BYTES)?;
    let manifest = Manifest::from_json(&bytes, reader.names())?;
    for entry in &manifest.entries {
        let indexed_bytes = reader.entry_size(&entry.name)?;
        if indexed_bytes != entry.bytes {
            return Err(format!(
                "package entry '{}' declares {} bytes in the manifest but {indexed_bytes} in the archive",
                entry.name, entry.bytes
            ));
        }
    }
    Ok(manifest)
}

/// Canonical content identity shared by package preparation and its cache key.
pub(crate) fn manifest_identity(manifest: &Manifest) -> Result<String, String> {
    Ok(super::hash::sha256_hex(&manifest.to_json()?))
}

/// Content identity of the package a level entry names.
///
/// For an installed package this hashes the canonical manifest record — the
/// package's own statement of what it contains — rather than the file bytes, so
/// the worker can coalesce requests and key its cache without reading every
/// prepared record again.
///
/// # Errors
///
/// Returns an error when the package cannot be opened or its manifest is
/// malformed.
pub fn package_identity(entry: &crate::engine::loader::LevelEntry) -> Result<String, String> {
    match entry.source_type {
        crate::engine::loader::LevelSourceType::Bundled
        | crate::engine::loader::LevelSourceType::Installed => {
            let manifest = inspect_level(&entry.path, &entry.id)?;
            manifest_identity(&manifest)
        }
    }
}

/// Reads one quality variant and attaches its prop textures from the catalog.
///
/// # Errors
///
/// Returns an error when the variant is not declared, any record fails to
/// decode or validate, a declared probe payload is missing, or the variant's
/// reflection captures are incomplete.
pub fn load_variant(
    path: &Path,
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::engine::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let file = super::archive::open_file(path)?;
    let mut reader = PackageReader::new(file).map_err(|error| error.to_string())?;
    decode_named_variant(&mut reader, manifest, quality, assets)
}

/// Reads one quality variant of an in-memory package.
///
/// # Errors
///
/// Returns the same errors as [`load_variant`].
pub fn load_variant_bytes(
    bytes: &[u8],
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::engine::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let mut reader = package_reader(bytes)?;
    decode_named_variant(&mut reader, manifest, quality, assets)
}

fn decode_named_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::engine::props::PropAssets,
) -> Result<LoadedVariant, String> {
    // Semantics and variants can be read in separate operations. The archive
    // opened for this operation must still be the one those semantics name;
    // check before dependency synchronization can mutate the asset cache.
    super::project::select(reader, Some(&manifest.id))?;
    let current_manifest = read_manifest(reader)?;
    if manifest_identity(&current_manifest)? != manifest_identity(manifest)? {
        return Err(format!(
            "package '{}' changed while loading; reload the level",
            manifest.id
        ));
    }
    assets.synchronize_dependencies(&manifest.dependencies);
    let name = quality.name();
    let variant = manifest
        .variant(name)
        .ok_or_else(|| format!("package '{}' has no '{name}' variant", manifest.id))?;
    decode_variant(reader, variant, quality, assets, Some(manifest))
}

/// Reattaches PNG allocations for compiled material variants. Only semantic
/// catalog references are resolved here; geometry and lighting stay prepared.
fn material_variant_textures<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    assets: &mut crate::engine::props::PropAssets,
    batches: &[PropMeshBatch],
) -> Result<std::collections::HashMap<String, Vec<Arc<crate::engine::loader::RawImage>>>, String> {
    let required: std::collections::HashSet<&str> = batches
        .iter()
        .filter(|batch| batch.model.contains("#materials-"))
        .map(|batch| batch.model.as_str())
        .collect();
    let mut textures = std::collections::HashMap::new();
    if required.is_empty() {
        return Ok(textures);
    }
    let bytes = read_named_entry(reader, manifest, "semantics.json", MAX_SEMANTICS_BYTES)?;
    let level: LevelDef = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let root = assets
        .root()
        .ok_or_else(|| "prop material variants require the installed asset bundle".to_string())?
        .to_path_buf();
    let catalog = crate::engine::loader::PropCatalog::load_from_path(&root.join("catalog.json"))
        .ok_or_else(|| "prop material variants require the installed catalog".to_string())?;
    // This secondary lookup needs only override materials. Do not decode all
    // architecture PNGs again while attaching a few timber/metal sheets.
    let mut material_level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"prop_materials","name":"Prop materials","spawn":{"x":0,"z":0}}"#,
    ).map_err(|error| error.to_string())?;
    material_level.props = level
        .props
        .into_iter()
        .filter(|prop| {
            !prop.material_overrides.is_empty()
                && catalog.get(&prop.model).model.is_some_and(|path| {
                    required.contains(
                        crate::engine::props::model_variant_identity(
                            &path,
                            &prop.material_overrides,
                        )
                        .as_str(),
                    )
                })
        })
        .collect();
    let first_material = material_level
        .props
        .iter()
        .flat_map(|prop| prop.material_overrides.values())
        .next()
        .cloned()
        .ok_or_else(|| "material variant references are empty".to_string())?;
    material_level.defaults.wall.clone_from(&first_material);
    material_level.defaults.floor.clone_from(&first_material);
    material_level.defaults.ceiling = first_material;
    let materials = crate::engine::materials::resolve_materials(
        &material_level,
        catalog.assets(),
        None,
        Some(&root),
        &mut crate::engine::materials::TextureCache::new(),
    )?;
    for prop in &material_level.props {
        if prop.material_overrides.is_empty() {
            continue;
        }
        let path = catalog
            .get(&prop.model)
            .model
            .ok_or_else(|| format!("unknown prop material variant model `{}`", prop.model))?;
        let identity =
            crate::engine::props::model_variant_identity(&path, &prop.material_overrides);
        if textures.contains_key(&identity) {
            continue;
        }
        let source = assets.resolve(&path)?;
        let images = crate::engine::render::override_prop_textures(
            &source,
            &prop.material_overrides,
            &materials,
        )?;
        let _previous = textures.insert(identity, images);
    }
    Ok(textures)
}

fn attach_prop_textures(
    props: &mut [PropMeshBatch],
    assets: &mut crate::engine::props::PropAssets,
    variant_textures: &std::collections::HashMap<String, Vec<Arc<crate::engine::loader::RawImage>>>,
) {
    for batch in props {
        if let Some(textures) = variant_textures.get(&batch.model) {
            batch.textures.clone_from(textures);
            continue;
        }
        match assets.resolve(&batch.model) {
            Ok(asset) => {
                batch.textures.clone_from(&asset.model.textures);
            }
            Err(error) => {
                // A missing model degrades to the diagnostic sheet, exactly
                // like a missing surface texture: the world still boots (the
                // empty-install recovery path has no asset tree at all), and
                // the failure is reported once per model.
                crate::engine::logging::warn_once(
                    format!("package-model:{}", batch.model),
                    format!(
                        "[props] compiled prop model '{}' is unavailable ({error}); \
                         drawing the diagnostic sheet",
                        batch.model
                    ),
                );
                let slots = batch
                    .submeshes
                    .iter()
                    .filter_map(|submesh| submesh.texture)
                    .max()
                    .map_or(1_usize, |slot| usize::from(slot).saturating_add(1));
                let diagnostic = Arc::new(crate::engine::materials::missing_texture());
                batch.textures = std::iter::repeat_n(diagnostic, slots).collect();
            }
        }
    }
}

fn decode_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    variant: &Variant,
    quality: LightmapQuality,
    assets: &mut crate::engine::props::PropAssets,
    manifest: Option<&Manifest>,
) -> Result<LoadedVariant, String> {
    let mesh_bytes = reader.read_blob(&variant.entries.mesh, MAX_BINARY_BYTES)?;
    let mesh = read_mesh(&mesh_bytes)?;
    drop(mesh_bytes);
    let props_bytes = super::shared_props::read_record(reader, &variant.entries.props)?;
    let mut props = read_props(&props_bytes)?;
    drop(props_bytes);
    let variant_textures = manifest.map_or_else(
        || Ok(std::collections::HashMap::new()),
        |package_manifest| material_variant_textures(reader, package_manifest, assets, &props),
    )?;
    attach_prop_textures(&mut props, assets, &variant_textures);
    let lighting_bytes = reader.read_blob(&variant.entries.lighting, MAX_LIGHTING_BYTES)?;
    let lighting = read_lighting(&lighting_bytes)?;
    drop(lighting_bytes);
    let static_light_off = match &variant.entries.static_light_off {
        Some(state) => {
            if state
                .fixtures
                .iter()
                .any(|index| lighting.fixture_light_index(*index).is_none())
            {
                return Err("static light state references a missing fixture".to_string());
            }
            let off_mesh = read_mesh(&reader.read_blob(&state.mesh, MAX_BINARY_BYTES)?)?;
            let mut batches = read_props(&super::shared_props::read_record(reader, &state.props)?)?;
            for batch in &mut batches {
                batch.textures = props
                    .iter()
                    .find(|source| source.model == batch.model)
                    .ok_or_else(|| "static light state references an unexpected model".to_string())?
                    .textures
                    .clone();
            }
            Some(Arc::new(crate::engine::render::StaticLightState {
                fixtures: state.fixtures.clone(),
                mesh: off_mesh,
                batches,
            }))
        }
        None => None,
    };
    let collision_bytes = reader.read_blob(&variant.entries.collision, MAX_COLLISION_BYTES)?;
    let collision = read_collision(&collision_bytes)?;
    drop(collision_bytes);
    let navigation_bytes = reader.read_blob(&variant.entries.navigation, MAX_NAVIGATION_BYTES)?;
    let navigation = crate::engine::package::navigation::read_navigation(&navigation_bytes)?;
    drop(navigation_bytes);
    let lightmaps = match (&variant.entries.lightmaps, &variant.entries.lightmaps_meta) {
        (Some(pages), Some(meta)) => {
            let page_bytes = reader.read_blob(pages, MAX_LIGHTMAP_ATLAS_BYTES)?;
            let meta_bytes = read_entry_checked(reader, meta, MAX_LIGHTMAP_METADATA_BYTES)?;
            Some(Arc::new(read_lightmaps_owned(&meta_bytes, page_bytes)?))
        }
        _ => None,
    };
    let irradiance = match &variant.entries.irradiance {
        Some(entry) => {
            let bytes = reader.read_blob(entry, MAX_ENTRY_BYTES)?;
            let field = crate::engine::lighting::probes::ProbeField::read(&bytes)?;
            validate_probe_references(&field, lighting.rooms().len(), lighting.lights().len())?;
            Some(Arc::new(field))
        }
        None => None,
    };
    let probes = read_probes(reader, variant)?;
    Ok(LoadedVariant {
        static_light_off,
        lightmap_quality: quality,
        mesh,
        props,
        lighting,
        lightmaps,
        irradiance,
        collision,
        navigation,
        probes,
    })
}

fn validate_probe_references(
    field: &crate::engine::lighting::probes::ProbeField,
    rooms: usize,
    lights: usize,
) -> Result<(), String> {
    for probe in &field.probes {
        if probe.room >= 0_i32 && usize::try_from(probe.room).map_or(true, |room| room >= rooms) {
            return Err(format!(
                "irradiance probe references missing room {}",
                probe.room
            ));
        }
    }
    for source in field.runtime_direct_lights() {
        if usize::try_from(*source).map_or(true, |index| index >= lights) {
            return Err(format!(
                "irradiance probe references missing direct source {source}"
            ));
        }
    }
    for plane in &field.switchable_indirect {
        if usize::try_from(plane.light_index).map_or(true, |index| index >= lights) {
            return Err(format!(
                "irradiance probe references missing switched indirect source {}",
                plane.light_index
            ));
        }
    }
    Ok(())
}

fn read_probes<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    variant: &Variant,
) -> Result<ProbeCaptures, String> {
    let mut captures = ProbeCaptures::default();
    for payload in &variant.entries.probes {
        let positions_bytes = read_entry_checked(reader, &payload.positions, MAX_MATERIALS_BYTES)?;
        let positions: ProbePositions = serde_json::from_slice(&positions_bytes)
            .map_err(|error| format!("probe positions record is not valid JSON: {error}"))?;
        validate_positions_record(&positions, payload)?;
        let mut chains = Vec::with_capacity(payload.cubemaps.len());
        for cubemap in &payload.cubemaps {
            let cube_bytes = reader.read_blob(cubemap, MAX_ENTRY_BYTES)?;
            chains.push(split_cube(&cube_bytes, payload.face_edge, payload.levels)?);
        }
        let capture = ProbeCapture {
            face_edge: payload.face_edge,
            levels: payload.levels,
            points: positions.points,
            chains,
        };
        match payload.face_edge {
            48 => captures.medium = Some(capture),
            64 => captures.full = Some(capture),
            other => {
                return Err(format!("unsupported probe face edge {other}"));
            }
        }
    }
    Ok(captures)
}

/// Validates one positions record against the manifest payload it belongs to.
///
/// Only the current prepared HDR mip-chain schema is accepted.
fn validate_positions_record(
    positions: &ProbePositions,
    payload: &ProbePayload,
) -> Result<(), String> {
    if positions.record_version != PROBE_POSITIONS_VERSION {
        return Err(format!(
            "probe positions record version {} is not supported",
            positions.record_version
        ));
    }
    if positions.face_edge != payload.face_edge || positions.levels != payload.levels {
        return Err("probe positions record does not match its payload".to_string());
    }
    if positions.points.is_empty() {
        return Err("probe positions record has no points".to_string());
    }
    if u32::try_from(positions.points.len()).unwrap_or(u32::MAX) != payload.count {
        return Err("probe positions count does not match the manifest".to_string());
    }
    for point in &positions.points {
        if !point.iter().all(|value| value.is_finite()) {
            return Err("probe position is non-finite".to_string());
        }
    }
    Ok(())
}

/// Decode the current linear binary16 reflection representation.
pub(crate) fn read_probe_image(bytes: &[u8]) -> Result<Ktx2Rgba16f, String> {
    ktx2::read_rgba16f(bytes)
}

/// Splits a decoded cube KTX2 into its full mip chain of six face buffers.
fn split_cube(bytes: &[u8], face_edge: u32, levels: u32) -> Result<Vec<[Vec<u8>; 6]>, String> {
    let image = read_probe_image(bytes)?;
    if image.faces != 6 || image.layers != 0 || image.edge != face_edge {
        return Err("probe cubemap shape does not match its payload".to_string());
    }
    split_cube_levels(&image, face_edge, levels)
}

/// Checks the decoded chain's level count and per-level byte length and splits
/// every level into six face buffers.
fn split_cube_levels(
    image: &Ktx2Rgba16f,
    face_edge: u32,
    levels: u32,
) -> Result<Vec<[Vec<u8>; 6]>, String> {
    if u32::try_from(image.levels.len()).unwrap_or(u32::MAX) != levels {
        return Err(format!(
            "probe cubemap has {} mip levels; the record says {levels}",
            image.levels.len()
        ));
    }
    let mut chain = Vec::with_capacity(image.levels.len());
    for (level, data) in image.levels.iter().enumerate() {
        let level_index = u32::try_from(level)
            .map_err(|error| format!("probe mip level is too large: {error}"))?;
        let level_edge = face_edge
            .checked_shr(level_index)
            .filter(|edge| *edge > 0)
            .ok_or_else(|| format!("probe cubemap has no mip level {level}"))?;
        let face_bytes = usize::try_from(level_edge)
            .map_err(|error| format!("probe face edge is too large: {error}"))?
            .checked_mul(
                usize::try_from(level_edge)
                    .map_err(|error| format!("probe face edge is too large: {error}"))?,
            )
            .and_then(|value| value.checked_mul(8))
            .ok_or_else(|| "probe face size overflows".to_string())?;
        if data.len() != face_bytes.saturating_mul(6) {
            return Err(format!(
                "probe cubemap level {level} holds {} bytes, expected {}",
                data.len(),
                face_bytes.saturating_mul(6)
            ));
        }
        let mut faces: Vec<Vec<u8>> = Vec::with_capacity(6);
        for face in data.chunks_exact(face_bytes) {
            faces.push(face.to_vec());
        }
        chain.push(faces.try_into().map_err(|incomplete_faces: Vec<Vec<u8>>| {
            format!(
                "probe cubemap holds {} faces instead of six",
                incomplete_faces.len()
            )
        })?);
    }
    Ok(chain)
}

/// Validates packaged captures against the emitted geometry and materials.
///
/// The player never captures probes, so a mismatched or incomplete payload is
/// a load error: installing the world without its reflections would silently
/// change what the level looks like.
///
/// # Errors
///
/// Returns an error when the routing expects probes and either face size is
/// absent, or the captured points do not match the routing's probe points.
pub fn validate_probe_captures(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    probes: &ProbeCaptures,
) -> Result<(), String> {
    let state = crate::engine::render::MaterialRenderState::from_table(materials);
    let routing = crate::engine::render::routing_from_mesh(
        mesh,
        &state.reflections,
        materials.entries().len(),
    );
    if routing.probe_points.is_empty() {
        return Ok(());
    }
    if !probes.complete() {
        return Err(format!(
            "package is missing its reflection captures ({} probe point(s)); \
             rebuild it with the probe capture step",
            routing.probe_points.len()
        ));
    }
    for (face_edge, capture) in [
        (
            crate::engine::render::PROBE_FACE_SIZE_MEDIUM,
            probes.medium.as_ref(),
        ),
        (
            crate::engine::render::PROBE_FACE_SIZE_FULL,
            probes.full.as_ref(),
        ),
    ] {
        let Some(resident_capture) = capture else {
            return Err("package is missing a reflection capture face size".to_string());
        };
        if resident_capture.face_edge != face_edge
            || resident_capture.points.len() != routing.probe_points.len()
        {
            return Err("reflection capture shape does not match the emitted geometry".to_string());
        }
        for (captured, expected) in resident_capture.points.iter().zip(&routing.probe_points) {
            let close = captured
                .iter()
                .zip(expected.iter())
                .all(|(a, b)| (a - b).abs() <= 1.0e-3);
            if !close {
                return Err(
                    "reflection capture positions do not match the emitted geometry".to_string(),
                );
            }
        }
    }
    Ok(())
}

fn read_named_entry<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = manifest
        .entry(name)
        .ok_or_else(|| format!("manifest does not declare '{name}'"))?;
    let bytes = reader.read_entry(name, limit)?;
    let actual = crate::engine::package::hash::sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(format!(
            "package entry '{name}' has hash {actual}, manifest says {}",
            entry.sha256
        ));
    }
    Ok(bytes)
}

fn read_entry_checked<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let bytes = reader.read_entry(name, limit)?;
    if name.starts_with("blobs/") {
        let expected = crate::engine::package::hash::sha256_from_blob_name(name)
            .ok_or_else(|| format!("package blob '{name}' has no content hash"))?;
        let actual = crate::engine::package::hash::sha256_hex(&bytes);
        if expected != actual {
            return Err(format!(
                "package blob '{name}' has hash {actual}, its name says {expected}"
            ));
        }
    }
    Ok(bytes)
}

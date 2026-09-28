//! The offline map compiler.
//!
//! This module owns the whole edit-source -> build-package pipeline. It runs
//! only in `places-compile` (and in tests): the player never names it, never
//! links a call to it, and its dependencies stay in the same crate so the
//! compiler and the player always agree on the record layouts.
//!
//! What a build does, in order:
//!
//! 1. Read and validate the authoring source, then run the semantic authoring
//!    preparation (`prepare_level`: fixture alignment, decal snapping,
//!    automatic baseboards). The result is the *semantics* record the package
//!    carries; the player never repeats this step.
//! 2. Resolve the level's logical materials and the content dependencies it
//!    names (prop models, surface and fixture textures), with SHA-256
//!    identities.
//! 3. For every requested lightmap quality, run the proven CPU preparation:
//!    the lighting bake, geometry emission with a lightmap plan, the atlas
//!    fill, prop instancing and reflection routing. Emit the prepared geometry,
//!    props, baked lighting, lightmap atlas and static collision as package
//!    records.
//! 4. Assemble the manifest and publish the archive atomically. A failed build
//!    leaves the previous package untouched.
//!
//! Developer rebuild identity is the `compiler_fingerprint`: source bytes,
//! dependency identities, record versions, variant list and the compiler
//! version. It is deliberately *not* a runtime validity check; a package whose
//! fingerprint is stale still loads if its records satisfy the format.
//!
//! Reflection probe *captures* are the one part of the build that needs a GPU:
//! they are produced by the headless capture step, which reads the prepared
//! package, renders each probe with the installed renderer and rewrites the
//! package with the captured cubemaps. A package without captures is refused by
//! the player rather than silently rendered without reflections.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::game::CollisionWorld;
use crate::level::LevelDef;
use crate::materials::{MaterialTable, TextureCache};
use crate::package::archive::{PendingEntry, write_archive};
use crate::package::collision::{CompiledCollision, write_collision};
use crate::package::hash::{blob_name, sha256_hex};
use crate::package::lighting::write_lighting;
use crate::package::lightmaps::write_lightmaps;
use crate::package::manifest::{
    DependencyKind, Manifest, PackageDependency, PackageEntry, ProbePayload, Variant,
    VariantEntries,
};
use crate::package::mesh::write_mesh;
use crate::package::props::write_props;
use crate::package::{FORMAT_VERSION, PACKAGE_EXTENSION};
use crate::quality::LightmapQuality;
use crate::quality::{QualityLevel, ReflectionQuality};
use crate::render::{
    LightmapBuildOptions, fill_lightmaps_cancellable, logical_materials,
    prepare_level_geometry_with_lightmaps, rebuild_vertex_lit_level,
};

/// The compiler's product identity, for `created_by`.
pub const COMPILER_NAME: &str = concat!("places-compile ", env!("CARGO_PKG_VERSION"));

/// One map build request.
#[derive(Clone, Debug)]
pub struct BuildRequest {
    /// Authoring source (`*.json`).
    pub source: PathBuf,
    /// Output package path.
    pub out: PathBuf,
    /// Asset root holding `catalog.json` and the asset tree.
    pub asset_root: PathBuf,
    /// Lightmap qualities to prepare, in this order.
    pub variants: Vec<LightmapQuality>,
    /// Shared CPU budget; `1` selects the serial reference path.
    pub workers: usize,
    /// Rebuild even when the fingerprint matches.
    pub force: bool,
    /// Capture reflection probes with the headless renderer.
    ///
    /// The CLI always sets this: a package without captures is refused by the
    /// player. In-crate tests that only exercise the CPU records set it false.
    pub capture_probes: bool,
}

/// What one build did.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildReport {
    /// Source path.
    pub source: String,
    /// Output package path.
    pub out: String,
    /// True when the output was (re)written.
    pub rebuilt: bool,
    /// Why a matching package was reused.
    pub reuse_reason: String,
    /// Variant names emitted.
    pub variants: Vec<String>,
    /// Package size in bytes.
    pub bytes: u64,
    /// Wall-clock build time in milliseconds.
    pub millis: f64,
    /// Non-fatal notes.
    pub warnings: Vec<String>,
    /// Developer rebuild fingerprint.
    pub fingerprint: String,
    /// Per-variant statistics.
    pub variant_stats: Vec<VariantStats>,
}

/// Prepared statistics of one variant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantStats {
    /// `off`, `medium` or `full`.
    pub lightmap_quality: String,
    /// Static ranges.
    pub mesh_ranges: usize,
    /// Static vertices.
    pub mesh_vertices: usize,
    /// Prop batches.
    pub prop_batches: usize,
    /// Lightmap pages (0 for the vertex-lit variant).
    pub lightmap_pages: usize,
    /// Charts (0 for the vertex-lit variant).
    pub lightmap_charts: usize,
    /// Probes in the prepared irradiance field (0 when none was baked).
    pub irradiance_probes: usize,
    /// Reflection probe points from the emitted geometry.
    pub probe_points: usize,
    /// Static wall boxes.
    pub wall_boxes: usize,
    /// A recorded lightmap failure, if the variant fell back to vertex light.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightmap_failure: Option<String>,
}

/// A validation report for one package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Package path.
    pub package: String,
    /// Level id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Package format version.
    pub format_version: u32,
    /// Variants found.
    pub variants: Vec<String>,
    /// Archive entries checked.
    pub entries: usize,
    /// Total uncompressed record bytes.
    pub bytes: u64,
    /// Dependencies checked.
    pub dependencies: usize,
    /// True when every declared dependency exists with its recorded size.
    pub dependencies_intact: bool,
    /// Developer fingerprint stored in the package.
    pub fingerprint: String,
    /// Non-fatal notes.
    pub warnings: Vec<String>,
}

/// Why a package is (not) current with a source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Source path.
    pub source: String,
    /// Package path.
    pub package: String,
    /// Package fingerprint.
    pub package_fingerprint: String,
    /// Fingerprint the source and assets would produce now.
    pub current_fingerprint: String,
    /// True when they match.
    pub current: bool,
    /// Input differences that made the build stale.
    pub differences: Vec<String>,
}

/// Builds one map package, reusing a current package unless `force`.
///
/// # Errors
///
/// Returns an error when the source cannot be read or validated, the asset root
/// or catalog is missing, any preparation step fails, a record cannot be
/// encoded, or the archive cannot be published. A failed build never replaces
/// an existing package.
#[allow(clippy::too_many_lines)] // one cohesive build pipeline
pub fn build(request: &BuildRequest) -> Result<BuildReport, String> {
    let started = Instant::now();
    if request.workers == 0 {
        return Err("--workers must be at least 1".to_string());
    }
    if request.variants.is_empty() {
        return Err("no lightmap variants requested".to_string());
    }
    crate::render::set_fill_workers(request.workers);
    let mut warnings = Vec::new();

    let source_bytes = read_source(&request.source)?;
    let source_text = std::str::from_utf8(&source_bytes)
        .map_err(|_| "level source is not valid UTF-8".to_string())?;
    let mut level = LevelDef::from_json(source_text)
        .map_err(|error| format!("level source is not valid: {error}"))?;
    crate::loader::validate_level(&level)?;
    let catalog_path = request.asset_root.join("catalog.json");
    let catalog = crate::loader::PropCatalog::load_from_path(&catalog_path).ok_or_else(|| {
        format!(
            "asset root {} has no loadable catalog.json",
            request.asset_root.display()
        )
    })?;
    crate::loader::prepare_level(&mut level, catalog.assets(), None);
    let source_hash = sha256_hex(&source_bytes);
    let mut assets = crate::props::PropAssets::with_root(request.asset_root.clone());
    let materials = logical_materials(&level);

    let dependencies = collect_dependencies(&level, &catalog, &request.asset_root, &mut warnings)?;
    let fingerprint = fingerprint(&source_hash, &request.variants, &dependencies);

    if !request.force
        && request.out.exists()
        && let Some(reason) = reuse_current(&request.out, &fingerprint)
    {
        return Ok(BuildReport {
            source: request.source.display().to_string(),
            out: request.out.display().to_string(),
            rebuilt: false,
            reuse_reason: reason,
            variants: request
                .variants
                .iter()
                .map(|quality| quality.name().to_string())
                .collect(),
            bytes: std::fs::metadata(&request.out).map_or(0, |metadata| metadata.len()),
            millis: started.elapsed().as_secs_f64() * 1000.0,
            warnings,
            fingerprint,
            variant_stats: Vec::new(),
        });
    }

    // Canonical bytes: the semantic record is part of the package's identity
    // and two builds of the same content must publish identical archives.
    let semantics = crate::canonical_json::canonical_json_bytes(&level)
        .map_err(|error| format!("could not serialize semantics: {error}"))?;
    let mut blobs: BlobMap = BTreeMap::new();
    let mut variants = Vec::with_capacity(request.variants.len());
    let mut variant_stats = Vec::with_capacity(request.variants.len());
    let mut cache = crate::lighting::lightmap::LightmapCache::memory_only();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let mut capture = if request.capture_probes {
        Some(CaptureContext::new(&level, &catalog, &request.asset_root)?)
    } else {
        None
    };
    for quality in &request.variants {
        let (mut variant, stats) = build_variant(
            &level,
            &catalog,
            &mut assets,
            &materials,
            *quality,
            &mut cache,
            &cancelled,
            &mut blobs,
            &mut warnings,
        )?;
        if let Some(capture) = capture.as_mut() {
            variant.entries.probes = capture
                .capture_variant(&level, &catalog, &assets, *quality, &variant, &mut blobs)?;
        }
        variants.push(variant);
        variant_stats.push(stats);
    }

    let mut entries: Vec<PendingEntry> = Vec::with_capacity(blobs.len().saturating_add(2));
    let mut package_entries: Vec<PackageEntry> = Vec::with_capacity(blobs.len().saturating_add(2));
    for (name, (bytes, role)) in &blobs {
        package_entries.push(PackageEntry {
            name: name.clone(),
            role: (*role).to_string(),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(bytes),
        });
        entries.push(PendingEntry {
            name: name.clone(),
            bytes: bytes.clone(),
        });
    }
    package_entries.push(PackageEntry {
        name: "semantics.json".to_string(),
        role: "semantics".to_string(),
        bytes: u64::try_from(semantics.len()).unwrap_or(u64::MAX),
        sha256: sha256_hex(&semantics),
    });
    entries.push(PendingEntry {
        name: "semantics.json".to_string(),
        bytes: semantics,
    });

    let mut required = vec![
        "geometry".to_string(),
        "props".to_string(),
        "lighting".to_string(),
        "collision".to_string(),
    ];
    if variants
        .iter()
        .any(|variant| variant.entries.lightmaps.is_some())
    {
        required.push("lightmaps-hdr".to_string());
    }
    if variants
        .iter()
        .any(|variant| variant.entries.irradiance.is_some())
    {
        required.push("irradiance-probes".to_string());
    }
    if variants
        .iter()
        .any(|variant| !variant.entries.probes.is_empty())
    {
        required.push("probes-rgba8".to_string());
    }
    let manifest = Manifest {
        package_format: FORMAT_VERSION,
        id: level.id.clone(),
        name: level.name.clone(),
        author: level.author.clone(),
        created_by: COMPILER_NAME.to_string(),
        compiler_fingerprint: fingerprint.clone(),
        required_capabilities: required,
        dependencies,
        entries: package_entries,
        variants,
    };
    let manifest_bytes = manifest.to_json()?;
    let mut all_names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    all_names.push("manifest.json".to_string());
    all_names.sort();
    manifest.validate(&all_names)?;
    entries.push(PendingEntry {
        name: "manifest.json".to_string(),
        bytes: manifest_bytes,
    });
    write_archive(&request.out, entries)?;
    let bytes = std::fs::metadata(&request.out).map_or(0, |metadata| metadata.len());
    Ok(BuildReport {
        source: request.source.display().to_string(),
        out: request.out.display().to_string(),
        rebuilt: true,
        reuse_reason: String::new(),
        variants: request
            .variants
            .iter()
            .map(|quality| quality.name().to_string())
            .collect(),
        bytes,
        millis: started.elapsed().as_secs_f64() * 1000.0,
        warnings,
        fingerprint,
        variant_stats,
    })
}

/// Builds every `*.json` source in one directory into sibling packages.
///
/// Structural failures are reported per source, so one broken map never blocks
/// an independent conversion.
///
/// # Errors
///
/// Returns an error only when the directory cannot be read.
pub type CollectionBuildResult = Result<BuildReport, (String, String)>;

/// # Errors
///
/// Returns an error only when the directory cannot be read; per-source failures
/// are returned inside the result list.
pub fn build_collection(
    directory: &Path,
    asset_root: &Path,
    variants: &[LightmapQuality],
    workers: usize,
    force: bool,
) -> Result<Vec<CollectionBuildResult>, String> {
    let mut sources: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    sources.sort();
    let mut results = Vec::with_capacity(sources.len());
    for source in sources {
        let request = BuildRequest {
            source: source.clone(),
            out: package_path_for(&source),
            asset_root: asset_root.to_path_buf(),
            variants: variants.to_vec(),
            workers,
            force,
            capture_probes: true,
        };
        results.push(build(&request).map_err(|error| (source.display().to_string(), error)));
    }
    Ok(results)
}

/// Opens a package, decodes every record and checks every dependency.
///
/// # Errors
///
/// Returns an error when the archive cannot be opened or any record fails its
/// structural or hash validation. Missing or resized dependencies are reported
/// in [`ValidationReport::dependencies_intact`] instead, because a read-only
/// install with a drifted asset tree can still report exactly what changed.
pub fn validate(path: &Path) -> Result<ValidationReport, String> {
    let mut reader = open_package(path)?;
    let manifest = read_manifest(&mut reader)?;
    let mut warnings = Vec::new();
    let mut bytes = 0_u64;
    for entry in &manifest.entries {
        let data = read_declared_entry(&mut reader, entry)?;
        bytes = bytes.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
        if entry.name == "semantics.json" {
            let semantics = std::str::from_utf8(&data)
                .map_err(|_| "semantics.json is not valid UTF-8".to_string())?;
            let level = LevelDef::from_json(semantics)
                .map_err(|error| format!("semantics.json is not valid: {error}"))?;
            crate::loader::validate_level(&level)?;
        }
    }

    for variant in &manifest.variants {
        validate_variant(&mut reader, variant)?;
    }
    check_dependencies(&manifest, &mut warnings);
    Ok(ValidationReport {
        package: path.display().to_string(),
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        format_version: manifest.package_format,
        variants: manifest
            .variants
            .iter()
            .map(|variant| variant.lightmap_quality.clone())
            .collect(),
        entries: manifest.entries.len().saturating_add(1),
        bytes,
        dependencies: manifest.dependencies.len(),
        dependencies_intact: !warnings
            .iter()
            .any(|warning| warning.starts_with("dependency ")),
        fingerprint: manifest.compiler_fingerprint.clone(),
        warnings,
    })
}

/// Reads only the manifest of a package.
///
/// # Errors
///
/// Returns an error when the archive cannot be opened or the manifest is
/// malformed.
pub fn inspect(path: &Path) -> Result<Manifest, String> {
    crate::package::world::inspect(path)
}

/// Recomputes the build fingerprint of `source` and compares it with a package.
///
/// # Errors
///
/// Returns an error when the source cannot be read or validated, or the package
/// cannot be opened.
pub fn verify(source: &Path, package: &Path, asset_root: &Path) -> Result<VerifyReport, String> {
    let source_bytes = read_source(source)?;
    let source_text = std::str::from_utf8(&source_bytes)
        .map_err(|_| "level source is not valid UTF-8".to_string())?;
    let mut level = LevelDef::from_json(source_text)
        .map_err(|error| format!("level source is not valid: {error}"))?;
    crate::loader::validate_level(&level)?;
    let manifest = inspect(package)?;
    let catalog_path = asset_root.join("catalog.json");
    let mut warnings = Vec::new();
    let dependencies = match crate::loader::PropCatalog::load_from_path(&catalog_path) {
        Some(catalog) => {
            crate::loader::prepare_level(&mut level, catalog.assets(), None);
            collect_dependencies(&level, &catalog, asset_root, &mut warnings)?
        }
        None => Vec::new(),
    };
    let variants: Vec<LightmapQuality> = manifest
        .variants
        .iter()
        .filter_map(|variant| match variant.lightmap_quality.as_str() {
            "off" => Some(LightmapQuality::Off),
            "medium" => Some(LightmapQuality::Medium),
            "full" => Some(LightmapQuality::Full),
            _ => None,
        })
        .collect();
    let current_fingerprint = fingerprint(&sha256_hex(&source_bytes), &variants, &dependencies);
    let mut differences = warnings
        .into_iter()
        .filter(|warning| warning.starts_with("dependency "))
        .collect::<Vec<_>>();
    if current_fingerprint != manifest.compiler_fingerprint {
        differences.push("source, assets, variants or compiler version changed".to_string());
    }
    Ok(VerifyReport {
        source: source.display().to_string(),
        package: package.display().to_string(),
        package_fingerprint: manifest.compiler_fingerprint,
        current_fingerprint,
        current: differences.is_empty(),
        differences,
    })
}

/// The output path a source compiles to by default: the sibling `.placesmap`.
#[must_use]
pub fn package_path_for(source: &Path) -> PathBuf {
    source.with_extension(PACKAGE_EXTENSION)
}

/// Resolves the asset root for a compiler run.
///
/// # Errors
///
/// Returns an error when no candidate root holds a `catalog.json`.
pub fn resolve_asset_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(root) = explicit {
        if root.join("catalog.json").is_file() {
            return Ok(root.to_path_buf());
        }
        return Err(format!("{} does not hold a catalog.json", root.display()));
    }
    let candidate = std::env::var_os("PLACES_ASSET_ROOT")
        .map(PathBuf::from)
        .or_else(crate::assets::resolve_asset_root);
    candidate
        .filter(|root| root.join("catalog.json").is_file())
        .ok_or_else(|| "no asset root with a catalog.json was found; pass --asset-root".to_string())
}

/// The compiler's headless capture context: a window-free renderer plus the
/// resolved material state the install path needs.
struct CaptureContext {
    renderer: crate::render::Renderer,
    loaded: crate::loader::LoadedLevel,
}

impl CaptureContext {
    fn new(
        level: &LevelDef,
        catalog: &crate::loader::PropCatalog,
        asset_root: &Path,
    ) -> Result<Self, String> {
        let mut texture_cache = TextureCache::new();
        let materials = crate::materials::resolve_materials(
            level,
            catalog.assets(),
            None,
            Some(asset_root),
            &mut texture_cache,
        );
        let light_sheets = crate::loader::resolve_fixture_sheets(
            level,
            catalog.assets(),
            None,
            &mut texture_cache,
        );
        let loaded = crate::loader::LoadedLevel {
            catalog: Arc::new(catalog.clone()),
            level: level.clone(),
            materials,
            light_sheets,
            entry: crate::loader::LevelEntry {
                id: level.id.clone(),
                name: level.name.clone(),
                author: level.author.clone(),
                source_type: crate::loader::LevelSourceType::Bundled,
                path: PathBuf::new(),
            },
        };
        let renderer =
            crate::render::Renderer::new_headless(crate::render::DrawableSize::new(1280, 720))?;
        Ok(Self { renderer, loaded })
    }

    /// Installs one prepared variant, captures both probe face sizes and
    /// returns the manifest payloads.
    fn capture_variant(
        &mut self,
        level: &LevelDef,
        catalog: &crate::loader::PropCatalog,
        assets: &crate::props::PropAssets,
        quality: LightmapQuality,
        variant: &Variant,
        blobs: &mut BlobMap,
    ) -> Result<Vec<ProbePayload>, String> {
        let build = prepare_build_for_capture(level, catalog, assets, quality)?;
        let probe_points = crate::render::routing_from_mesh(
            &build.mesh,
            &crate::render::MaterialRenderState::from_table(&self.loaded.materials).reflections,
            self.loaded.materials.entries().len(),
        )
        .probe_points;
        if probe_points.is_empty() {
            return Ok(Vec::new());
        }
        self.renderer.set_quality(QualityLevel::High);
        self.renderer.set_lightmap_quality(quality);
        self.renderer
            .set_reflection_quality(ReflectionQuality::Full);
        self.renderer.install_prepared(
            &self.loaded,
            Arc::new(build),
            assets.clone(),
            crate::render::CharacterScene::new(),
            false,
        );
        while !self.renderer.advance_prepared_install() {
            // Upload phases advance one bounded step per call.
        }
        let mut payloads = Vec::with_capacity(2);
        let full = self.renderer.read_back_probe_faces()?;
        payloads.push(probe_payload(&full, &probe_points, blobs)?);
        self.renderer
            .reprepare_reflection_probes(ReflectionQuality::Medium);
        let medium = self.renderer.read_back_probe_faces()?;
        payloads.push(probe_payload(&medium, &probe_points, blobs)?);
        let _ = variant;
        Ok(payloads)
    }
}

/// Rebuilds the same prepared world the variant records came from, for the
/// capture install. The CPU work is repeated here because records are already
/// encoded; the alternative (holding every variant's build in memory) costs
/// more than one extra bake on a developer machine.
fn prepare_build_for_capture(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &crate::props::PropAssets,
    quality: LightmapQuality,
) -> Result<crate::render::LevelBuild, String> {
    let materials = crate::render::logical_materials(level);
    let mut assets = assets.clone();
    let mut cache = crate::lighting::lightmap::LightmapCache::memory_only();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    crate::lighting::set_preparation_assets(catalog.clone(), assets.clone());
    let prepared = prepare_level_geometry_with_lightmaps(
        level,
        catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_lightmaps(quality),
        Some(&mut cache),
    );
    let mut build = prepared.build;
    if let Some(fill) = prepared.fill {
        match fill_lightmaps_cancellable(&fill, &cancelled) {
            crate::render::LightmapFillOutcome::Filled(product) => {
                build.lightmap_millis = product.lightmaps.stats.bake_millis;
                crate::render::dump_lightmaps_for_level(level, &product.lightmaps);
                build.lightmaps = Some(std::sync::Arc::new(product.lightmaps));
                build.lightmap_failure = None;
            }
            crate::render::LightmapFillOutcome::Failed(failure) => {
                build.mesh = rebuild_vertex_lit_level(
                    level,
                    catalog,
                    &mut assets,
                    &materials,
                    &build.lighting,
                );
                build.lightmaps = None;
                build.lightmap_failure = Some(failure);
            }
            crate::render::LightmapFillOutcome::Cancelled => {
                return Err("capture preparation was cancelled".to_string());
            }
        }
    }
    Ok(build)
}

/// Encodes one captured face set as one KTX2 cube per probe plus a positions
/// record, returning the manifest payload.
///
/// Each probe's base faces are offline-prefiltered into the roughness mip
/// chain the player uploads verbatim; the filter is deterministic, so two
/// builds of the same capture package identical bytes.
fn probe_payload(
    probes: &[crate::render::ProbeFaceReadback],
    points: &[[f32; 3]],
    blobs: &mut BlobMap,
) -> Result<ProbePayload, String> {
    let Some(first) = probes.first() else {
        return Err("probe capture produced no probes".to_string());
    };
    if probes.len() != points.len() {
        return Err(format!(
            "probe capture produced {} probe(s) for {} routing point(s)",
            probes.len(),
            points.len()
        ));
    }
    let face_edge = first.face_size;
    let levels = crate::render::ProbeFaceReadback::packaged_mip_levels(face_edge);
    if face_edge == 0
        || u32::try_from(probes.len()).unwrap_or(u32::MAX)
            > u32::try_from(crate::package::MAX_PROBES).unwrap_or(u32::MAX)
    {
        return Err(format!("probe capture produced {} probes", probes.len()));
    }
    let mut cubemaps = Vec::with_capacity(probes.len());
    for probe in probes {
        if probe.face_size != face_edge {
            return Err("probe capture mixed face sizes".to_string());
        }
        let chain = probe.packaged_mips()?;
        cubemaps.push(insert_blob(
            blobs,
            crate::package::ktx2::write_rgba8_cube_with_mips(face_edge, &chain)?,
            ".probe.ktx2",
            "probes",
        ));
    }
    // The positions record names the *routing* probe points, which the
    // player recomputes from the decoded mesh and matches against; the cube
    // itself is captured from the lifted bake position.
    let positions = crate::package::world::ProbePositions {
        record_version: crate::package::world::PROBE_POSITIONS_VERSION,
        face_edge,
        levels,
        points: points.to_vec(),
    };
    let mut positions_bytes = serde_json::to_vec(&positions)
        .map_err(|error| format!("could not serialize probe positions: {error}"))?;
    positions_bytes.push(b'\n');
    let positions_name = insert_blob(blobs, positions_bytes, ".probes.json", "probes-meta");
    Ok(ProbePayload {
        face_edge,
        levels,
        count: u32::try_from(probes.len()).map_err(|_| "probe count is too large".to_string())?,
        cubemaps,
        positions: positions_name,
    })
}

fn open_package(path: &Path) -> Result<crate::package::PackageReader<std::fs::File>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    crate::package::PackageReader::new(file).map_err(|error| error.to_string())
}

fn read_manifest<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::package::PackageReader<R>,
) -> Result<Manifest, String> {
    let bytes = reader.read_entry("manifest.json", crate::package::MAX_MANIFEST_BYTES)?;
    let names = reader.names().to_vec();
    Manifest::from_json(&bytes, &names)
}

/// Reads one declared entry and verifies both its manifest hash and, for a
/// content-addressed blob, the hash embedded in its name.
fn read_declared_entry<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::package::PackageReader<R>,
    entry: &PackageEntry,
) -> Result<Vec<u8>, String> {
    let limit = if entry.name == "semantics.json" {
        crate::package::MAX_SEMANTICS_BYTES
    } else {
        crate::package::MAX_ENTRY_BYTES
    };
    let bytes = reader.read_entry(&entry.name, limit)?;
    let actual = sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(format!(
            "package entry '{}' has hash {actual}, manifest says {}",
            entry.name, entry.sha256
        ));
    }
    if entry.name.starts_with("blobs/") {
        let expected = crate::package::hash::sha256_from_blob_name(&entry.name)
            .ok_or_else(|| format!("package blob '{}' has no content hash", entry.name))?;
        if expected != actual {
            return Err(format!(
                "package blob '{}' has hash {actual}, its name says {expected}",
                entry.name
            ));
        }
    }
    Ok(bytes)
}

fn validate_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::package::PackageReader<R>,
    variant: &Variant,
) -> Result<(), String> {
    let mesh_bytes = reader.read_blob(&variant.entries.mesh, crate::package::MAX_BINARY_BYTES)?;
    let _ = crate::package::mesh::read_mesh(&mesh_bytes)?;
    let props_bytes = reader.read_blob(&variant.entries.props, crate::package::MAX_BINARY_BYTES)?;
    let _ = crate::package::props::read_props(&props_bytes)?;
    let lighting_bytes = reader.read_blob(
        &variant.entries.lighting,
        crate::package::MAX_LIGHTING_BYTES,
    )?;
    let _ = crate::package::lighting::read_lighting(&lighting_bytes)?;
    let collision_bytes = reader.read_blob(
        &variant.entries.collision,
        crate::package::MAX_COLLISION_BYTES,
    )?;
    let _ = crate::package::collision::read_collision(&collision_bytes)?;
    if let (Some(pages), Some(meta)) = (&variant.entries.lightmaps, &variant.entries.lightmaps_meta)
    {
        let page_bytes = reader.read_blob(pages, crate::package::MAX_ENTRY_BYTES)?;
        let meta_bytes = reader.read_entry(meta, crate::package::MAX_MATERIALS_BYTES)?;
        let _ = crate::package::lightmaps::read_lightmaps(&meta_bytes, &page_bytes)?;
    }
    if let Some(irradiance) = &variant.entries.irradiance {
        let bytes = reader.read_blob(irradiance, crate::package::MAX_ENTRY_BYTES)?;
        let _ = crate::lighting::probes::ProbeField::read(&bytes)?;
    }
    for probe in &variant.entries.probes {
        for cubemap in &probe.cubemaps {
            let cube_bytes = reader.read_blob(cubemap, crate::package::MAX_ENTRY_BYTES)?;
            let image = crate::package::ktx2::read_rgba8(&cube_bytes)?;
            if image.faces != 6
                || image.layers != 0
                || image.edge != probe.face_edge
                || u32::try_from(image.levels.len()).unwrap_or(u32::MAX) != probe.levels
            {
                return Err(format!(
                    "probe cubemap '{cubemap}' does not match its declared shape"
                ));
            }
        }
    }
    Ok(())
}

fn check_dependencies(manifest: &Manifest, warnings: &mut Vec<String>) {
    let asset_root = crate::assets::resolve_asset_root().unwrap_or_else(|| PathBuf::from("assets"));
    for dependency in &manifest.dependencies {
        if dependency.kind == DependencyKind::Embedded {
            warnings.push(format!(
                "dependency {} is embedded in the package",
                dependency.path
            ));
            continue;
        }
        let path = asset_root.join(&dependency.path);
        match std::fs::read(&path) {
            Ok(bytes) => {
                if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != dependency.bytes {
                    warnings.push(format!(
                        "dependency {} is {} bytes, recorded {}",
                        dependency.path,
                        bytes.len(),
                        dependency.bytes
                    ));
                } else if sha256_hex(&bytes) != dependency.sha256 {
                    warnings.push(format!(
                        "dependency {} has different content than recorded",
                        dependency.path
                    ));
                }
            }
            Err(_) => warnings.push(format!("dependency {} is missing", dependency.path)),
        }
    }
}

/// `(blob name, (bytes, role))` for one variant's prepared records.
type BlobMap = BTreeMap<String, (Vec<u8>, &'static str)>;

#[allow(clippy::too_many_arguments)] // one cohesive variant build
#[allow(clippy::too_many_lines)] // one cohesive variant build
fn build_variant(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
    quality: LightmapQuality,
    cache: &mut crate::lighting::lightmap::LightmapCache,
    cancelled: &std::sync::atomic::AtomicBool,
    blobs: &mut BlobMap,
    warnings: &mut Vec<String>,
) -> Result<(Variant, VariantStats), String> {
    let options = LightmapBuildOptions::for_lightmaps(quality);
    // The bake derives prop occlusion boxes from the same request's model
    // bytes; an edited model can never reuse stale boxes.
    crate::lighting::set_preparation_assets(catalog.clone(), assets.clone());
    let prepared = prepare_level_geometry_with_lightmaps(
        level,
        catalog,
        assets,
        materials,
        options,
        Some(cache),
    );
    let mut build = prepared.build;
    let mut lightmap_failure = build
        .lightmap_failure
        .map(|failure| failure.name().to_string());
    if let Some(fill) = prepared.fill {
        match fill_lightmaps_cancellable(&fill, cancelled) {
            crate::render::LightmapFillOutcome::Filled(product)
                if product.lightmaps.pages.is_empty() =>
            {
                // Nothing in the level wanted a chart (an empty world, or a
                // fully vertex-lit surface set): the mesh already carries the
                // vertex-colour path everywhere, so the variant ships without
                // an atlas and says why.
                build.lightmaps = None;
                build.probes = None;
                build.lightmap_failure = None;
                lightmap_failure = Some("no charts".to_string());
            }
            crate::render::LightmapFillOutcome::Filled(product) => {
                let mut product = product;
                // Label the prepared moving-object field: a probe in no room or
                // inside a wall is never sampled, which is what stops light
                // from bleeding through a floor or a full-height wall.
                if let Some(mut field) = product.probes.take() {
                    let lighting = &build.lighting;
                    field.assign_rooms(|position| {
                        if lighting.wall_contains_point(position[0], position[2]) {
                            return None;
                        }
                        lighting.room_index_at_height(position[0], position[1], position[2])
                    });
                    let (total, valid) = field.summary();
                    crate::logging::info(format_args!(
                        "[lightmaps] irradiance field probes={total} valid={valid}"
                    ));
                    build.probes = Some(std::sync::Arc::new(field));
                }
                let atlas = std::sync::Arc::new(product.lightmaps);
                build.lightmap_millis = atlas.stats.bake_millis;
                build.lightmaps = Some(atlas);
                build.lightmap_failure = None;
                lightmap_failure = None;
            }
            crate::render::LightmapFillOutcome::Failed(failure) => {
                // The runtime contract falls back to the vertex-lit build with
                // the same baked lighting when an atlas cannot be produced.
                lightmap_failure = Some(failure.name().to_string());
                build.mesh =
                    rebuild_vertex_lit_level(level, catalog, assets, materials, &build.lighting);
                build.lightmaps = None;
                build.lightmap_failure = Some(failure);
                warnings.push(format!(
                    "{} lightmaps fell back to vertex lighting: {}",
                    quality.name(),
                    failure.name()
                ));
            }
            crate::render::LightmapFillOutcome::Cancelled => {
                return Err("lightmap fill was cancelled".to_string());
            }
        }
    }

    let material_state = crate::render::MaterialRenderState::from_table(materials);
    let routing = crate::render::routing_from_mesh(
        &build.mesh,
        &material_state.reflections,
        materials.entries().len(),
    );
    let collision = CollisionWorld::from_level(level);
    let compiled_collision = CompiledCollision {
        walls: collision.walls,
        floor: collision.floor,
        ceiling: collision.ceiling,
        water: collision.water,
        ladders: collision.ladders,
    };

    let mesh_name = insert_blob(blobs, write_mesh(&build.mesh)?, ".mesh", "mesh");
    let props_name = insert_blob(blobs, write_props(&build.batches)?, ".props", "props");
    let lighting_name = insert_blob(
        blobs,
        write_lighting(&build.lighting)?,
        ".lighting",
        "lighting",
    );
    let collision_name = insert_blob(
        blobs,
        write_collision(&compiled_collision)?,
        ".collision",
        "collision",
    );

    let irradiance_name = match &build.probes {
        Some(field) => Some(insert_blob(
            blobs,
            field.write()?,
            ".irradiance",
            "irradiance",
        )),
        None => None,
    };
    let (lightmaps_name, lightmaps_meta_name, pages, charts) = match &build.lightmaps {
        Some(atlas) => {
            let (meta, ktx2) = write_lightmaps(atlas)?;
            let pages = atlas.pages.len();
            let charts = atlas.charts.len();
            (
                Some(insert_blob(blobs, ktx2, ".lightmaps.ktx2", "lightmaps")),
                Some(insert_blob(
                    blobs,
                    meta,
                    ".lightmaps.json",
                    "lightmaps-meta",
                )),
                pages,
                charts,
            )
        }
        None => (None, None, 0, 0),
    };

    let stats = VariantStats {
        lightmap_quality: quality.name().to_string(),
        mesh_ranges: build.mesh.ranges.len(),
        mesh_vertices: build.mesh.vertex_count,
        prop_batches: build.batches.len(),
        lightmap_pages: pages,
        lightmap_charts: charts,
        irradiance_probes: build.probes.as_ref().map_or(0, |field| field.probes.len()),
        probe_points: routing.probe_points.len(),
        wall_boxes: compiled_collision.walls.len(),
        lightmap_failure,
    };
    let variant = Variant {
        lightmap_quality: quality.name().to_string(),
        quality_profile: match quality {
            LightmapQuality::Off => "low",
            LightmapQuality::Medium | LightmapQuality::Full => "full",
        }
        .to_string(),
        lightmap_failure: stats.lightmap_failure.clone(),
        entries: VariantEntries {
            mesh: mesh_name,
            props: props_name,
            lighting: lighting_name,
            collision: collision_name,
            lightmaps: lightmaps_name,
            lightmaps_meta: lightmaps_meta_name,
            irradiance: irradiance_name,
            probes: Vec::new(),
        },
    };
    Ok((variant, stats))
}

fn insert_blob(blobs: &mut BlobMap, bytes: Vec<u8>, suffix: &str, role: &'static str) -> String {
    let name = blob_name(&bytes, suffix);
    blobs.entry(name.clone()).or_insert((bytes, role));
    name
}

fn collect_dependencies(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    asset_root: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<PackageDependency>, String> {
    let mut identities: BTreeMap<(String, String), PackageDependency> = BTreeMap::new();
    let mut add = |kind: DependencyKind, path: &str| -> Result<(), String> {
        let key = (dependency_kind_name(kind).to_string(), path.to_string());
        if identities.contains_key(&key) {
            return Ok(());
        }
        let bytes = match std::fs::read(asset_root.join(path)) {
            Ok(bytes) => bytes,
            Err(error) => {
                // A resource absent at compile time contributes nothing to the
                // prepared world (its material degrades to the diagnostic
                // sheet, its prop to the fallback box), so it is not a
                // dependency the player must find.
                warnings.push(format!(
                    "dependency '{path}' could not be read ({error}); not recorded"
                ));
                return Ok(());
            }
        };
        identities.insert(
            key,
            PackageDependency {
                kind,
                path: path.to_string(),
                sha256: sha256_hex(&bytes),
                bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            },
        );
        Ok(())
    };

    for prop in &level.props {
        if let Some(model) = catalog.get(&prop.model).model.clone() {
            add(DependencyKind::Model, &model)?;
        }
    }
    let mut material_ids = crate::materials::referenced_material_ids(level);
    material_ids.sort();
    material_ids.dedup();
    for id in material_ids {
        if let Some(texture) = catalog.assets().material_texture(&id)
            && let Some(path) = catalog.assets().texture_path(texture)
        {
            add(DependencyKind::Texture, path)?;
        }
    }
    for light in &level.ceiling_lights {
        if let Some(path) = catalog.assets().fixture_sheet_path(&light.fixture) {
            add(DependencyKind::Texture, path)?;
        }
    }
    Ok(identities.into_values().collect())
}

const fn dependency_kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Model => "model",
        DependencyKind::Texture => "texture",
        DependencyKind::Embedded => "embedded",
    }
}

fn fingerprint(
    source_sha256: &str,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
) -> String {
    use std::fmt::Write as _;
    let mut canonical = String::with_capacity(1024);
    canonical.push_str(COMPILER_NAME);
    canonical.push('\n');
    let _ = writeln!(canonical, "format {FORMAT_VERSION}");
    // The semantics record is the authored level itself; a schema revision
    // changes what a package means even when nothing else moved, so the level
    // format version is part of the build identity.
    let _ = writeln!(
        canonical,
        "level_format {}",
        crate::level::LEVEL_FORMAT_VERSION
    );
    let _ = writeln!(canonical, "source {source_sha256}");
    let _ = writeln!(
        canonical,
        "records mesh {} props {} collision {} lighting {} probes {}",
        crate::package::mesh::MESH_RECORD_VERSION,
        crate::package::props::PROPS_RECORD_VERSION,
        crate::package::collision::COLLISION_RECORD_VERSION,
        crate::package::lighting::LIGHTING_RECORD_VERSION,
        crate::package::world::PROBE_POSITIONS_VERSION,
    );
    // The prepared data is a function of the lighting model and the transport
    // solver as well as of the source. Folding both revisions in means a
    // recalibrated model or a moved solver invalidates an existing package
    // instead of silently reusing its captures.
    let _ = writeln!(
        canonical,
        "solver {} model {}",
        crate::lighting::transport::solver_fingerprint(),
        crate::lighting::model_fingerprint()
    );
    for quality in variants {
        let _ = writeln!(canonical, "variant {}", quality.name());
    }
    for dependency in dependencies {
        let _ = writeln!(
            canonical,
            "dep {} {} {} {}",
            dependency_kind_name(dependency.kind),
            dependency.path,
            dependency.sha256,
            dependency.bytes
        );
    }
    sha256_hex(canonical.as_bytes())
}

fn reuse_current(out: &Path, fingerprint: &str) -> Option<String> {
    let manifest = inspect(out).ok()?;
    if manifest.compiler_fingerprint != fingerprint {
        return None;
    }
    // A fingerprint match is not enough: a package whose declared entry hashes
    // no longer match its bytes (a corrupt or half-written artifact) must be
    // rebuilt, never reused.
    match verify_declared_entries(out, &manifest) {
        Ok(()) => Some(format!(
            "{} is current for this source and asset identity",
            out.display()
        )),
        Err(error) => {
            crate::logging::warn(format!("[compiler] rebuilding {}: {error}", out.display()));
            None
        }
    }
}

/// Re-reads every declared entry and compares its bytes with the manifest and
/// blob-name hashes.
fn verify_declared_entries(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let mut reader = open_package(path)?;
    for entry in &manifest.entries {
        let data = reader.read_entry(&entry.name, crate::package::MAX_ENTRY_BYTES)?;
        let actual = sha256_hex(&data);
        if actual != entry.sha256 {
            return Err(format!("entry '{}' no longer matches its hash", entry.name));
        }
        if let Some(expected) = crate::package::hash::sha256_from_blob_name(&entry.name)
            && expected != actual
        {
            return Err(format!("blob '{}' no longer matches its name", entry.name));
        }
    }
    Ok(())
}

fn read_source(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if metadata.len() > crate::level::MAX_LEVEL_JSON_BYTES {
        return Err(format!(
            "{} is {} bytes (limit {})",
            path.display(),
            metadata.len(),
            crate::level::MAX_LEVEL_JSON_BYTES
        ));
    }
    std::fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]

    use super::*;

    /// Deterministic per-texel pattern, distinct per face.
    fn patterned_faces(edge: u32) -> [Vec<u8>; 6] {
        let bytes = usize::try_from(edge).unwrap() * usize::try_from(edge).unwrap() * 4;
        std::array::from_fn(|face| {
            let mut out = Vec::with_capacity(bytes);
            for texel in 0..bytes / 4 {
                out.extend_from_slice(&[
                    u8::try_from((texel + face * 13) % 256).unwrap(),
                    u8::try_from((texel * 3 + face) % 256).unwrap(),
                    u8::try_from((texel * 7 + face * 5) % 256).unwrap(),
                    255,
                ]);
            }
            out
        })
    }

    fn readback(edge: u32) -> crate::render::ProbeFaceReadback {
        crate::render::ProbeFaceReadback {
            position: [0.0, 0.0, 0.0],
            face_size: edge,
            faces: patterned_faces(edge),
        }
    }

    #[test]
    fn packaged_probes_round_trip_a_full_mip_chain() {
        let edge = 8;
        let probe = readback(edge);
        let points = vec![[1.0, 2.0, 3.0]];
        let mut blobs = BlobMap::new();
        let payload =
            probe_payload(std::slice::from_ref(&probe), &points, &mut blobs).expect("payload");

        let levels = crate::render::ProbeFaceReadback::packaged_mip_levels(edge);
        assert_eq!(payload.levels, levels);
        assert_eq!(payload.face_edge, edge);

        let (positions_bytes, _) = blobs.get(&payload.positions).expect("positions blob");
        let record: crate::package::world::ProbePositions =
            serde_json::from_slice(positions_bytes).expect("record");
        assert_eq!(
            record.record_version,
            crate::package::world::PROBE_POSITIONS_VERSION
        );
        assert_eq!(record.record_version, 2);
        assert_eq!(record.face_edge, edge);
        assert_eq!(record.levels, levels);
        assert_eq!(record.points, points);

        let (cube_bytes, _) = blobs.get(&payload.cubemaps[0]).expect("cube blob");
        let image = crate::package::ktx2::read_rgba8(cube_bytes).expect("decode");
        assert_eq!(image.faces, 6);
        assert_eq!(image.layers, 0);
        assert_eq!(image.edge, edge);
        assert_eq!(u32::try_from(image.levels.len()).unwrap(), levels);
        let base_bytes = usize::try_from(edge).unwrap() * usize::try_from(edge).unwrap() * 4;
        let decoded_base: Vec<Vec<u8>> = image.levels[0]
            .chunks_exact(base_bytes)
            .map(<[u8]>::to_vec)
            .collect();
        assert_eq!(decoded_base, probe.faces.to_vec(), "level 0 is the capture");
        let mut expected = edge;
        for level in &image.levels {
            assert_eq!(
                level.len(),
                usize::try_from(expected).unwrap() * usize::try_from(expected).unwrap() * 4 * 6
            );
            expected /= 2;
        }
    }

    #[test]
    fn two_builds_package_identical_probe_bytes() {
        let edge = 16;
        let probe = readback(edge);
        let points = vec![[4.0, 5.0, 6.0]];
        let mut first_blobs = BlobMap::new();
        let first = probe_payload(std::slice::from_ref(&probe), &points, &mut first_blobs)
            .expect("first payload");
        let mut second_blobs = BlobMap::new();
        let second = probe_payload(std::slice::from_ref(&probe), &points, &mut second_blobs)
            .expect("second payload");
        assert_eq!(first.cubemaps, second.cubemaps, "content-addressed names");
        assert_eq!(first.positions, second.positions);
        for name in &first.cubemaps {
            assert_eq!(
                first_blobs.get(name).map(|(bytes, _)| bytes),
                second_blobs.get(name).map(|(bytes, _)| bytes),
                "the prefilter must be deterministic"
            );
        }
        assert_eq!(
            first_blobs.get(&first.positions).map(|(bytes, _)| bytes),
            second_blobs.get(&second.positions).map(|(bytes, _)| bytes)
        );
    }
}

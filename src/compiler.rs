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

mod probes;

use std::collections::{BTreeMap, BTreeSet};
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
    GEOMETRY_REVISION, LightmapBuildOptions, fill_lightmaps_cancellable,
    prepare_level_geometry_with_lightmaps, rebuild_vertex_lit_level,
};

/// The compiler's product identity, for `created_by`.
pub const COMPILER_NAME: &str = concat!("places-compile ", env!("CARGO_PKG_VERSION"));

/// Shared maximum CPU allocation for one offline compiler process.
pub const MAX_WORKERS: usize = 12;

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
    /// Shared CPU budget; `1` selects serial execution, capped at [`MAX_WORKERS`].
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
    /// Major pipeline phases; nested lighting timings are in verbose diagnostics.
    #[serde(default)]
    pub phases: Vec<BuildPhase>,
}

/// Wall time of one compiler phase, excluding phases it does not invoke.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildPhase {
    /// Stage name, optionally suffixed with the lightmap quality.
    pub phase: String,
    /// Elapsed wall time in milliseconds.
    pub millis: f64,
}

fn record_phase(phases: &mut Vec<BuildPhase>, phase: impl Into<String>, started: Instant) {
    let phase_name = phase.into();
    let millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
    crate::logging::info(format_args!(
        "[compiler-timing] phase={phase_name} millis={millis:.3}"
    ));
    phases.push(BuildPhase {
        phase: phase_name,
        millis,
    });
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
    /// Walkable navigation cells for the reference class.
    #[serde(default)]
    pub navigation_cells: usize,
    /// Connected navigation regions for the reference class.
    #[serde(default)]
    pub navigation_regions: usize,
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
#[expect(clippy::too_many_lines, reason = "one cohesive build pipeline")] // one cohesive build pipeline
pub fn build(request: &BuildRequest) -> Result<BuildReport, String> {
    let started = Instant::now();
    let mut phases = Vec::new();
    if request.workers == 0 {
        return Err("--workers must be at least 1".to_string());
    }
    if request.variants.is_empty() {
        return Err("no lightmap variants requested".to_string());
    }
    let workers = request.workers.min(MAX_WORKERS);
    crate::render::set_fill_workers(workers);
    let mut warnings = Vec::new();

    let source_bytes = read_source(&request.source)?;
    let source_text = std::str::from_utf8(&source_bytes)
        .map_err(|error| format!("level source is not valid UTF-8: {error}"))?;
    let mut level = LevelDef::from_json(source_text)
        .map_err(|error| format!("level source is not valid: {error}"))?;
    crate::loader::validate_level(&level)?;
    let catalog_path = request.asset_root.join("catalog.json");
    let (catalog, catalog_hash) = load_catalog_identity(&catalog_path)?;
    crate::loader::prepare_level(&mut level, catalog.assets(), None);
    let source_hash = sha256_hex(&source_bytes);
    let mut assets = crate::props::PropAssets::with_root(request.asset_root.clone());
    let materials = MaterialTable::logical(&level, catalog.assets(), None);

    let dependencies = collect_dependencies(&level, &catalog, &request.asset_root, &mut warnings)?;
    record_phase(&mut phases, "load_validate_prepare_dependencies", started);
    let phase_started = Instant::now();
    let fingerprint = fingerprint_with_catalog(
        &fingerprint(&source_hash, &request.variants, &dependencies),
        &catalog_hash,
    );
    // Stage fingerprint: everything the illumination/geometry/collision/probe
    // preparation reads, with the navigation and AI components removed. An
    // encounter or AI tuning edit therefore matches the previous package's
    // stage key and only the semantics and navigation records are rebuilt.
    let lighting_fingerprint = fingerprint_with_catalog(
        &lighting_fingerprint(&level, &request.variants, &dependencies)?,
        &catalog_hash,
    );
    record_phase(&mut phases, "fingerprints", phase_started);
    let integrity_started = Instant::now();

    if !request.force
        && request.out.exists()
        && let Some(reason) = reuse_current(&request.out, &fingerprint)
    {
        record_phase(&mut phases, "current_package_integrity", integrity_started);
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
            phases,
        });
    }
    record_phase(&mut phases, "current_package_integrity", integrity_started);
    let semantics_started = Instant::now();

    // Canonical bytes: the semantic record is part of the package's identity
    // and two builds of the same content must publish identical archives.
    let semantics = crate::canonical_json::canonical_json_bytes(&level)
        .map_err(|error| format!("could not serialize semantics: {error}"))?;
    record_phase(&mut phases, "semantics", semantics_started);
    let mut blobs: BlobMap = BTreeMap::new();
    let mut variants = Vec::with_capacity(request.variants.len());
    let mut variant_stats = Vec::with_capacity(request.variants.len());
    let mut cache = crate::lighting::lightmap::LightmapCache::memory_only();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let navigation_started = Instant::now();
    // Navigation is variant-independent: bake and insert it once, then let
    // every variant reference the same content-addressed blob.
    let (navigation_bytes, navigation_report) = bake_navigation(&level, workers, &mut warnings)?;
    let navigation_name = insert_blob(&mut blobs, navigation_bytes, ".navigation", "navigation");
    record_phase(
        &mut phases,
        "navigation_collision_inputs",
        navigation_started,
    );
    let reuse_started = Instant::now();
    // Reuse the previous package's prepared geometry, lighting, probes and
    // collision when the lighting stage fingerprint matches: an AI-only edit
    // must not rebake illumination.
    let reused = if request.force {
        None
    } else {
        reuse_prepared_lighting(&request.out, &lighting_fingerprint, &level, &mut warnings)
    };
    record_phase(&mut phases, "prepared_package_integrity", reuse_started);
    if let Some((reused_variants, reused_blobs)) = reused {
        for (name, (bytes, role)) in reused_blobs {
            let _interned_blob = blobs.entry(name).or_insert((bytes, role));
        }
        for mut variant in reused_variants {
            variant.entries.navigation.clone_from(&navigation_name);
            variant_stats.push(VariantStats {
                lightmap_quality: variant.lightmap_quality.clone(),
                mesh_ranges: 0,
                mesh_vertices: 0,
                prop_batches: 0,
                lightmap_pages: 0,
                lightmap_charts: 0,
                irradiance_probes: 0,
                probe_points: 0,
                wall_boxes: 0,
                navigation_cells: navigation_report
                    .walkable_cells
                    .first()
                    .copied()
                    .unwrap_or(0),
                navigation_regions: navigation_report
                    .regions
                    .first()
                    .copied()
                    .and_then(|regions| usize::try_from(regions).ok())
                    .unwrap_or(0),
                lightmap_failure: variant.lightmap_failure.clone(),
            });
            variants.push(variant);
        }
    } else {
        let device_started = Instant::now();
        let mut capture = if request.capture_probes {
            Some(CaptureContext::new(&level, &catalog, &request.asset_root)?)
        } else {
            None
        };
        record_phase(&mut phases, "capture_device_materials", device_started);
        for quality in &request.variants {
            crate::logging::info(format_args!(
                "[compiler-progress] preparing {}",
                quality.name()
            ));
            let variant_started = Instant::now();
            let (mut variant, stats, build) = build_variant(
                &level,
                &catalog,
                &mut assets,
                &materials,
                *quality,
                &mut cache,
                &cancelled,
                &mut blobs,
                &mut warnings,
                &navigation_name,
                &navigation_report,
            )?;
            record_phase(
                &mut phases,
                format!("prepare_encode_{}", quality.name()),
                variant_started,
            );
            if let Some(capture_context) = capture.as_mut() {
                // A custom asset root can resolve different reflection state
                // from the logical build. Skip only a proven empty capture.
                let same_reflections = crate::render::MaterialRenderState::from_table(&materials)
                    .reflections
                    == crate::render::MaterialRenderState::from_table(
                        &capture_context.loaded.materials,
                    )
                    .reflections;
                if stats.probe_points > 0 || !same_reflections {
                    let capture_started = Instant::now();
                    crate::logging::info(format_args!(
                        "[compiler-progress] capturing {}",
                        quality.name()
                    ));
                    variant.entries.probes =
                        capture_context.capture_variant(&assets, *quality, build, &mut blobs)?;
                    record_phase(
                        &mut phases,
                        format!("capture_{}", quality.name()),
                        capture_started,
                    );
                }
            }
            variants.push(variant);
            variant_stats.push(stats);
        }
    }

    let manifest_started = Instant::now();
    let mut entries: Vec<PendingEntry> = Vec::with_capacity(blobs.len().saturating_add(2));
    let mut package_entries: Vec<PackageEntry> = Vec::with_capacity(blobs.len().saturating_add(2));
    for (name, (bytes, role)) in blobs {
        package_entries.push(PackageEntry {
            name: name.clone(),
            role,
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(&bytes),
        });
        entries.push(PendingEntry { name, bytes });
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
        "navigation".to_string(),
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
        lighting_fingerprint: Some(lighting_fingerprint.clone()),
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
    record_phase(&mut phases, "record_hashes_manifest", manifest_started);
    let publication_started = Instant::now();
    write_archive(&request.out, entries)?;
    record_phase(&mut phases, "archive_compress_publish", publication_started);
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
        phases,
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
                .map_err(|error| format!("semantics.json is not valid UTF-8: {error}"))?;
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
        .map_err(|error| format!("level source is not valid UTF-8: {error}"))?;
    let mut level = LevelDef::from_json(source_text)
        .map_err(|error| format!("level source is not valid: {error}"))?;
    crate::loader::validate_level(&level)?;
    let manifest = inspect(package)?;
    let catalog_path = asset_root.join("catalog.json");
    let mut warnings = Vec::new();
    let (catalog, catalog_hash) = load_catalog_identity(&catalog_path)?;
    crate::loader::prepare_level(&mut level, catalog.assets(), None);
    let dependencies = collect_dependencies(&level, &catalog, asset_root, &mut warnings)?;
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
    let current_fingerprint = fingerprint_with_catalog(
        &fingerprint(&sha256_hex(&source_bytes), &variants, &dependencies),
        &catalog_hash,
    );
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
    renderer: Option<crate::render::Renderer>,
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
        crate::materials::check_texture_budget(&materials)
            .map_err(|error| format!("level `{}`: {error}", level.id))?;
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
        // Resolve authored reflection state before deciding whether a GPU is
        // needed. A world with no routing points has no captures to render.
        Ok(Self {
            renderer: None,
            loaded,
        })
    }

    /// Installs one prepared variant, captures both probe face sizes and
    /// returns the manifest payloads.
    fn capture_variant(
        &mut self,
        assets: &crate::props::PropAssets,
        quality: LightmapQuality,
        build: crate::render::LevelBuild,
        blobs: &mut BlobMap,
    ) -> Result<Vec<ProbePayload>, String> {
        let probe_points = crate::render::routing_from_mesh(
            &build.mesh,
            &crate::render::MaterialRenderState::from_table(&self.loaded.materials).reflections,
            self.loaded.materials.entries().len(),
        )
        .probe_points;
        if probe_points.is_empty() {
            return Ok(Vec::new());
        }
        if self.renderer.is_none() {
            self.renderer = Some(crate::render::Renderer::new_headless(
                crate::render::DrawableSize::new(1280, 720),
            )?);
        }
        let renderer = self
            .renderer
            .as_mut()
            .ok_or("capture device is unavailable")?;
        renderer.set_quality(QualityLevel::High);
        renderer.set_lightmap_quality(quality);
        renderer.set_reflection_quality(ReflectionQuality::Full);
        renderer.install_prepared(
            &self.loaded,
            Arc::new(build),
            assets.clone(),
            crate::render::CharacterScene::new(),
            false,
        );
        while !renderer.advance_prepared_install() {
            // Upload phases advance one bounded step per call.
        }
        let mut payloads = Vec::with_capacity(2);
        let full = renderer.read_back_probe_faces()?;
        payloads.push(probe_payload(&full, &probe_points, blobs)?);
        renderer.reprepare_reflection_probes(ReflectionQuality::Medium);
        let medium = renderer.read_back_probe_faces()?;
        payloads.push(probe_payload(&medium, &probe_points, blobs)?);
        Ok(payloads)
    }
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
        count: u32::try_from(probes.len())
            .map_err(|error| format!("probe count is too large: {error}"))?,
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
    let bytes = reader.read_entry(&entry.name, declared_entry_limit(entry))?;
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

/// Use the runtime record's existing allocation contract, including for
/// integrity-only reads. The archive-wide total cap remains authoritative.
fn declared_entry_limit(entry: &PackageEntry) -> u64 {
    if entry.name == "semantics.json" {
        crate::package::MAX_SEMANTICS_BYTES
    } else if entry.role == "lightmaps" {
        crate::package::MAX_LIGHTMAP_ATLAS_BYTES
    } else if matches!(entry.role.as_str(), "mesh" | "props") {
        crate::package::MAX_BINARY_BYTES
    } else {
        crate::package::MAX_ENTRY_BYTES
    }
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
    let navigation_bytes = reader.read_blob(
        &variant.entries.navigation,
        crate::package::MAX_NAVIGATION_BYTES,
    )?;
    let _ = crate::package::navigation::read_navigation(&navigation_bytes)?;
    if let (Some(pages), Some(meta)) = (&variant.entries.lightmaps, &variant.entries.lightmaps_meta)
    {
        let page_bytes = reader.read_blob(pages, crate::package::MAX_LIGHTMAP_ATLAS_BYTES)?;
        let meta_bytes = reader.read_entry(meta, crate::package::MAX_LIGHTMAP_METADATA_BYTES)?;
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
type BlobMap = BTreeMap<String, (Vec<u8>, String)>;

#[expect(clippy::too_many_arguments, reason = "one cohesive variant build")] // one cohesive variant build
#[expect(clippy::too_many_lines, reason = "one cohesive variant build")] // one cohesive variant build
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
    navigation_name: &str,
    navigation_report: &crate::nav::NavBakeReport,
) -> Result<(Variant, VariantStats, crate::render::LevelBuild), String> {
    let options = LightmapBuildOptions::for_lightmaps(quality);
    let collision = CollisionWorld::from_level(level);
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
    let mut capture_colours = None;
    let mut capture_empty_atlas = None;
    report_preparation(quality, &build);
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
                capture_empty_atlas = Some(Arc::new(product.lightmaps));
                build.lightmaps = None;
                build.probes = None;
                build.lightmap_failure = None;
                lightmap_failure = Some("no charts".to_string());
            }
            crate::render::LightmapFillOutcome::Filled(mut product) => {
                // Label the prepared moving-object field: a probe in no room or
                // inside a wall is never sampled, which is what stops light
                // from bleeding through a floor or a full-height wall.
                if let Some(mut field) = product.probes.take() {
                    probes::label(
                        &mut field,
                        level,
                        &build.lighting,
                        &fill.transport,
                        &collision.walls,
                    );
                    let mut covered = vec![false; build.lighting.rooms().len()];
                    for probe in &field.probes {
                        if let Ok(room) = usize::try_from(probe.room)
                            && let Some(room_coverage) = covered.get_mut(room)
                        {
                            *room_coverage = true;
                        }
                    }
                    for (room, is_covered) in covered.iter().enumerate() {
                        if !is_covered {
                            warnings.push(format!(
                                "{} room {room} has no valid irradiance probe at {:.3} m spacing",
                                quality.name(),
                                field.cell_m
                            ));
                        }
                    }
                    crate::lighting::transport::probe_audit::dump_labels(&field, quality.name())?;
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
                // A feather decal must read like the prepared surface under it:
                // rewrite the blended decals' vertex light from the same probe
                // field moving objects use. Cut-out decals are untouched.
                if let Some(field) = build.probes.as_deref() {
                    // Captures historically use the pre-relit decal colours
                    // and no moving-object field. Retain just those colours,
                    // rather than rebaking the whole world to recover them.
                    capture_colours = Some(
                        build
                            .mesh
                            .ranges
                            .iter()
                            .flat_map(|range| range.vertices.iter().map(|vertex| vertex.color))
                            .collect::<Vec<_>>(),
                    );
                    crate::render::relight_blend_decals(
                        &mut build.mesh,
                        level,
                        catalog.assets(),
                        &build.lighting,
                        field,
                    );
                }
            }
            crate::render::LightmapFillOutcome::Failed(
                failure @ (crate::lighting::lightmap::LightmapFailure::FillNonFinite
                | crate::lighting::lightmap::LightmapFailure::Worker
                | crate::lighting::lightmap::LightmapFailure::TransportEnergy),
            ) => {
                return Err(format!(
                    "{} lighting bake failed physical validation: {}",
                    quality.name(),
                    failure.name()
                ));
            }
            crate::render::LightmapFillOutcome::Failed(failure) => {
                // The runtime contract falls back to the vertex-lit build with
                // the same baked lighting when an atlas cannot be produced.
                lightmap_failure = Some(failure.name().to_string());
                build.mesh = rebuild_vertex_lit_level(
                    level,
                    catalog,
                    assets,
                    materials,
                    &build.lighting,
                    &mut build.batches,
                );
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
        navigation_cells: navigation_report
            .walkable_cells
            .first()
            .copied()
            .unwrap_or(0),
        navigation_regions: navigation_report
            .regions
            .first()
            .copied()
            .and_then(|regions| usize::try_from(regions).ok())
            .unwrap_or(0),
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
            navigation: navigation_name.to_string(),
            lightmaps: lightmaps_name,
            lightmaps_meta: lightmaps_meta_name,
            irradiance: irradiance_name,
            probes: Vec::new(),
        },
    };
    // Records above retain the labelled field and relit runtime mesh. Move the
    // same variant directly into capture with its historical capture state;
    // at most one prepared variant is retained, and the atlas is never copied.
    if let Some(colours) = capture_colours {
        for (vertex, colour) in build
            .mesh
            .ranges
            .iter_mut()
            .flat_map(|range| range.vertices.iter_mut())
            .zip(colours)
        {
            vertex.color = colour;
        }
    }
    build.probes = None;
    if let Some(empty) = capture_empty_atlas {
        // The old capture path installed a successfully filled empty atlas;
        // keep that state even though the runtime package omits empty pages.
        build.lightmaps = Some(empty);
    }
    if let Some(atlas) = build.lightmaps.as_deref() {
        crate::render::dump_lightmaps_for_level(level, atlas);
    }
    Ok((variant, stats, build))
}

fn report_preparation(quality: LightmapQuality, build: &crate::render::LevelBuild) {
    crate::logging::info(format_args!(
        "[compile-timing] quality={} legacy_lighting_ms={:.3} prop_geometry_and_charts_ms={:.3} architecture_and_charts_ms={:.3}",
        quality.name(),
        build.timings.lighting_millis,
        build.timings.props_millis,
        build.timings.surfaces_millis
    ));
}

fn insert_blob(
    blobs: &mut BlobMap,
    bytes: Vec<u8>,
    suffix: &str,
    role: impl Into<String>,
) -> String {
    let name = blob_name(&bytes, suffix);
    let _interned_blob = blobs
        .entry(name.clone())
        .or_insert_with(|| (bytes, role.into()));
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
        drop(identities.insert(
            key,
            PackageDependency {
                kind,
                path: path.to_string(),
                sha256: sha256_hex(&bytes),
                bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            },
        ));
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
        // Reflection captures consume albedo, normal and emission-mask images.
        // Changing any of their bytes must invalidate the shared prepared stage.
        let material = catalog.assets().material(&id);
        for texture in [
            catalog.assets().material_texture(&id),
            material.and_then(|entry| entry.normal_texture.as_deref()),
            material.and_then(|entry| entry.emissive_mask.as_deref()),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(path) = catalog.assets().texture_path(texture) {
                add(DependencyKind::Texture, path)?;
            }
        }
    }
    for light in &level.ceiling_lights {
        if let Some(path) = catalog.assets().fixture_sheet_path(&light.fixture) {
            add(DependencyKind::Texture, path)?;
        }
    }
    // The sky sheet is a level dependency like any surface texture: editing it
    // must invalidate the prepared package.
    if let Some(sky) = &level.sky
        && let Some(path) = catalog.assets().texture_path(sky.texture.trim())
    {
        add(DependencyKind::Texture, path)?;
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

/// Bakes one level's navigation grid and reports placement problems.
///
/// Classes are the reference humanoid plus every distinct `nav_agent` body the
/// level authors (props and spawn templates); `nav_obstacle` components add
/// explicit boxes. A `nav_agent`-carrying entity or spawn point that has no
/// navigable cell for its class is reported as a build warning naming it,
/// because an actor that cannot navigate is an authoring defect, not a load
/// failure of an otherwise valid map.
#[expect(
    clippy::too_many_lines,
    reason = "one cohesive offline bake plus its diagnostics"
)] // one cohesive offline bake plus its diagnostics
pub(crate) fn bake_navigation(
    level: &LevelDef,
    workers: usize,
    warnings: &mut Vec<String>,
) -> Result<(Vec<u8>, crate::nav::NavBakeReport), String> {
    use crate::level::ComponentDef;
    let collision = CollisionWorld::from_level(level);
    let doors = crate::door::Doors::from_level(level);
    let surfaces = crate::level::LevelSurfaces::new(level);
    let mut classes = vec![crate::nav::reference_class()];
    let register_profile = |registered_classes: &mut Vec<crate::package::navigation::NavClass>,
                            class: crate::package::navigation::NavClass|
     -> Result<(), String> {
        if registered_classes
            .iter()
            .any(|existing| existing.matches(&class))
        {
            return Ok(());
        }
        if registered_classes.len() >= crate::package::MAX_NAV_CLASSES {
            return Err(format!(
                "level authors more than {} distinct nav_agent bodies; the navigation record \
                 supports at most that many baked classes",
                crate::package::MAX_NAV_CLASSES
            ));
        }
        registered_classes.push(class);
        Ok(())
    };
    let profile_class = |component: &ComponentDef| -> Option<crate::package::navigation::NavClass> {
        if let ComponentDef::NavAgent {
            radius,
            height,
            step_height,
            max_slope,
            speed_mps: _,
        } = component
        {
            return crate::package::navigation::NavClass::new(
                *radius,
                *height,
                *step_height,
                *max_slope,
            );
        }
        None
    };
    let profile_of = |component: &ComponentDef| -> Option<crate::nav::NavAgentProfile> {
        if let ComponentDef::NavAgent {
            radius,
            height,
            step_height,
            max_slope,
            speed_mps: _,
        } = component
        {
            return Some(crate::nav::NavAgentProfile {
                radius: *radius,
                height: *height,
                step_height: *step_height,
                max_slope: *max_slope,
                can_open_doors: false,
            });
        }
        None
    };
    for prop in &level.props {
        for component in &prop.components {
            if let Some(class) = profile_class(component) {
                register_profile(&mut classes, class)?;
            }
        }
    }
    for template in &level.spawn_templates {
        for component in &template.components {
            if let Some(class) = profile_class(component) {
                register_profile(&mut classes, class)?;
            }
        }
    }
    let mut obstacles: Vec<crate::collision::WallAabb> = Vec::new();
    for prop in &level.props {
        for component in &prop.components {
            let ComponentDef::NavObstacle { size, affects_nav } = component else {
                continue;
            };
            if !*affects_nav {
                continue;
            }
            let body_size =
                size.unwrap_or_else(|| prop.resolved_size(crate::level::PROP_FALLBACK_SIZE));
            let (half_x, half_z) = crate::interact::rotated_half_extents(
                body_size[0] * 0.5,
                body_size[2] * 0.5,
                prop.rotation_degrees,
            );
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            obstacles.push(crate::collision::WallAabb::with_y(
                prop.x - half_x,
                base_y,
                prop.z - half_z,
                half_x * 2.0,
                body_size[1],
                half_z * 2.0,
            ));
        }
    }
    let input = crate::nav::NavBakeInput {
        level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &obstacles,
        walk_proxies: &[],
    };
    let options = crate::nav::NavBakeOptions {
        cell_m: crate::nav::DEFAULT_NAV_CELL_M,
        workers,
    };
    let (grid, report) = crate::nav::bake(&input, &options)?;
    if report.walkable_cells.first().copied().unwrap_or(0) == 0 {
        warnings.push(format!("{} navigation baked no walkable cells", level.id));
    }
    // Placement diagnostics: every actor body and spawn point must have a
    // navigable cell, or the encounter that uses it can never move.
    if let Ok(mesh) = crate::nav::NavMesh::from_record(grid.clone()) {
        let no_doors = crate::nav::NoDoors;
        let check = |what: &str, profile: &crate::nav::NavAgentProfile, position: glam::Vec3| {
            let mut placement_warnings = Vec::new();
            let Some(class) = mesh.class_index(&profile.class()) else {
                placement_warnings.push(format!(
                    "{} navigation has no baked class for {what}",
                    level.id
                ));
                return placement_warnings;
            };
            if mesh
                .nearest(class, position, 2.0, 2.0, &no_doors, profile.can_open_doors)
                .is_none()
            {
                placement_warnings.push(format!(
                    "{} navigation: {what} at ({:.2}, {:.2}) has no navigable placement",
                    level.id, position.x, position.z
                ));
            }
            placement_warnings
        };
        let ids = level.prop_instance_ids();
        for (index, prop) in level.props.iter().enumerate() {
            let Some(profile) = prop.components.iter().find_map(&profile_of) else {
                continue;
            };
            let id = ids.get(index).map_or(prop.model.as_str(), String::as_str);
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            warnings.extend(check(
                &format!("`{id}`"),
                &profile,
                glam::Vec3::new(prop.x, base_y, prop.z),
            ));
        }
        for point in &level.spawn_points {
            let Some(template) = level
                .spawn_templates
                .iter()
                .find(|template| template.id == point.template)
            else {
                continue;
            };
            let Some(profile) = template.components.iter().find_map(&profile_of) else {
                continue;
            };
            let base_y = point
                .y
                .filter(|y| y.is_finite())
                .unwrap_or_else(|| surfaces.floor_y_at(point.x, point.z).unwrap_or(0.0));
            warnings.extend(check(
                &format!("spawn point `{}`", point.id),
                &profile,
                glam::Vec3::new(point.x, base_y, point.z),
            ));
        }
    }
    let bytes = crate::package::navigation::write_navigation(&grid)?;
    Ok((bytes, report))
}

/// Parse exactly the catalogue bytes included in the build key, including
/// definitions that reference unchanged model/texture files.
fn load_catalog_identity(path: &Path) -> Result<(crate::loader::PropCatalog, String), String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read catalog {}: {error}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("catalog {} is not valid UTF-8: {error}", path.display()))?;
    let catalog = crate::loader::PropCatalog::from_json_str(text)
        .map_err(|error| format!("catalog {} is invalid: {error}", path.display()))?;
    Ok((catalog, sha256_hex(&bytes)))
}

/// Versioned catalogue identity surrounds both complete and reusable stage
/// keys. No map record or package schema changes; old keys miss safely once.
fn fingerprint_with_catalog(input: &str, catalog_hash: &str) -> String {
    // Version 2 finalizes canonical ray-boundary eligibility. Invalidate private
    // experimental packages too, even when their asset and solver keys match.
    sha256_hex(format!("asset_inputs_v2\ninput {input}\ncatalog {catalog_hash}\n").as_bytes())
}

/// The illumination/geometry stage fingerprint.
///
/// It folds everything the prepared static world depends on — the level with
/// the navigation and AI components removed, the dependency identities, the
/// record versions, the lighting model and the transport solver, and the
/// requested variants. Display name/author and navigation/AI components are
/// excluded; the outer key additionally hashes the exact catalogue snapshot.
/// An edit restricted to those display/navigation fields keeps the same key and the previous package's prepared
/// geometry, lightmaps, probes and collision can be reused.
///
/// # Errors
///
/// Returns an error when the stripped level cannot be serialized.
fn lighting_fingerprint(
    level: &LevelDef,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
) -> Result<String, String> {
    lighting_fingerprint_with_revision(level, variants, dependencies, GEOMETRY_REVISION)
}

/// [`lighting_fingerprint`] with an explicit geometry revision.
///
/// Crate-visible so the fingerprint tests can prove a revision bump changes
/// the stage key without editing the level source.
///
/// # Errors
///
/// Returns an error when the stripped level cannot be serialized.
pub(crate) fn lighting_fingerprint_with_revision(
    level: &LevelDef,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
    geometry_revision: u32,
) -> Result<String, String> {
    use std::fmt::Write as _;
    let mut stripped = level.clone();
    // Display metadata is encoded anew in semantics and the manifest. It is
    // never read by static geometry, collision, illumination or probe baking.
    stripped.name.clear();
    stripped.author.clear();
    strip_navigation_components(&mut stripped);
    let bytes = crate::canonical_json::canonical_json_bytes(&stripped)
        .map_err(|error| format!("could not serialize the lighting stage input: {error}"))?;
    let mut canonical = String::with_capacity(1024);
    canonical.push_str(COMPILER_NAME);
    canonical.push('\n');
    let _formatted_format_format = writeln!(canonical, "format {FORMAT_VERSION}");
    let _formatted_level_format = writeln!(
        canonical,
        "level_format {}",
        crate::level::LEVEL_FORMAT_VERSION
    );
    // Explicit stage-key version: older packages miss once, then reuse safely.
    let _formatted_stage_lighting = writeln!(
        canonical,
        "stage lighting v2 navigation_display_metadata_excluded"
    );
    // This reusable stage also stores collision boxes, including region rims.
    let _formatted_rim_backing = writeln!(
        canonical,
        "rim_backing {}",
        crate::level::RIM_BACKING.to_bits()
    );
    let _formatted_records_mesh = writeln!(
        canonical,
        "records mesh {} props {} collision {} lighting {} probes {} lightmaps {}",
        crate::package::mesh::MESH_RECORD_VERSION,
        crate::package::props::PROPS_RECORD_VERSION,
        crate::package::collision::COLLISION_RECORD_VERSION,
        crate::package::lighting::LIGHTING_RECORD_VERSION,
        crate::package::world::PROBE_POSITIONS_VERSION,
        crate::package::lightmaps::LIGHTMAPS_RECORD_VERSION,
    );
    let _formatted_solver_model = writeln!(
        canonical,
        "solver {} model {}",
        crate::lighting::transport::solver_fingerprint(),
        crate::lighting::model_fingerprint()
    );
    // The prepared geometry is a function of the emitter as well as of the
    // source, and the lightmaps are baked against that geometry. A geometry
    // revision therefore invalidates both stages together instead of letting
    // a stale mesh and its old lightmaps be reused.
    let _formatted_geometry_geometry = writeln!(canonical, "geometry {geometry_revision}");
    for quality in variants {
        let _formatted_variant = writeln!(canonical, "variant {}", quality.name());
    }
    for dependency in dependencies {
        let _formatted_dep = writeln!(
            canonical,
            "dep {} {} {} {}",
            dependency_kind_name(dependency.kind),
            dependency.path,
            dependency.sha256,
            dependency.bytes
        );
    }
    let _formatted_prepared = writeln!(
        canonical,
        "prepared {}",
        crate::package::hash::sha256_hex(&bytes)
    );
    Ok(crate::package::hash::sha256_hex(canonical.as_bytes()))
}

/// Removes every navigation/AI component from a level copy.
fn strip_navigation_components(level: &mut LevelDef) {
    use crate::level::ComponentDef;
    let strip = |components: &mut Vec<ComponentDef>| {
        components.retain(|component| {
            !matches!(
                component,
                ComponentDef::Ai(_)
                    | ComponentDef::NavAgent {
                        radius: _,
                        speed_mps: _,
                        height: _,
                        step_height: _,
                        max_slope: _
                    }
                    | ComponentDef::NavObstacle {
                        size: _,
                        affects_nav: _
                    }
            )
        });
    };
    for prop in &mut level.props {
        strip(&mut prop.components);
    }
    for door in &mut level.doors {
        strip(&mut door.components);
    }
    for template in &mut level.spawn_templates {
        strip(&mut template.components);
    }
}

/// Tries to reuse a previous package's prepared world for the current lighting
/// stage fingerprint.
///
/// Returns the previous variants (with their entry names) and every blob the
/// new archive still needs when the stage key matches and every declared entry
/// verifies; `None` when there is no previous package, the key differs, or the
/// archive cannot be trusted, in which case the caller prepares everything.
fn reuse_prepared_lighting(
    out: &Path,
    lighting_fingerprint: &str,
    level: &LevelDef,
    warnings: &mut Vec<String>,
) -> Option<(Vec<Variant>, BlobMap)> {
    if !out.exists() {
        return None;
    }
    let file = std::fs::File::open(out).ok()?;
    let mut reader = crate::package::PackageReader::new(std::io::BufReader::new(file)).ok()?;
    let names = reader.names().to_vec();
    let manifest_bytes = reader
        .read_entry("manifest.json", crate::package::MAX_MANIFEST_BYTES)
        .ok()?;
    let manifest = Manifest::from_json(&manifest_bytes, &names).ok()?;
    if manifest.lighting_fingerprint.as_deref() != Some(lighting_fingerprint) {
        return None;
    }
    let mut needed = BTreeSet::new();
    for variant in &manifest.variants {
        let entries = &variant.entries;
        needed.extend([
            entries.mesh.as_str(),
            entries.props.as_str(),
            entries.lighting.as_str(),
            entries.collision.as_str(),
        ]);
        needed.extend(entries.lightmaps.as_deref());
        needed.extend(entries.lightmaps_meta.as_deref());
        needed.extend(entries.irradiance.as_deref());
        for probe in &entries.probes {
            let _new_entry = needed.insert(probe.positions.as_str());
            needed.extend(probe.cubemaps.iter().map(String::as_str));
        }
    }
    let mut blobs: BlobMap = BTreeMap::new();
    for entry in &manifest.entries {
        if entry.name == "semantics.json" {
            continue;
        }
        let bytes = read_declared_entry(&mut reader, entry).ok()?;
        // Verify all old records, but carry forward only referenced static
        // data. Navigation is rebuilt; retaining its previous blobs would
        // accumulate orphan records after repeated navigation edits.
        if needed.contains(entry.name.as_str()) {
            drop(blobs.insert(entry.name.clone(), (bytes, entry.role.clone())));
        }
    }
    // Every variant must still name the mandatory records and every named
    // entry must resolve in the reused blob set.
    if needed.iter().any(|name| !blobs.contains_key(*name)) {
        return None;
    }
    let mut variants = manifest.variants;
    refresh_reused_atlas_keys(level, &mut variants, &mut blobs)?;
    warnings.push(format!(
        "reused prepared geometry, lighting, probes and collision from {} (lighting stage fingerprint \
         unchanged; only the semantics and navigation records were rebuilt)",
        out.display()
    ));
    Some((variants, blobs))
}

/// A clean build's diagnostic atlas key includes display/navigation fields.
/// Refresh that small record while reusing all solved texel/probe bytes, so an
/// incremental archive is exactly the independent clean archive of its source.
fn refresh_reused_atlas_keys(
    level: &LevelDef,
    variants: &mut [Variant],
    blobs: &mut BlobMap,
) -> Option<()> {
    for variant in variants {
        let Some(previous) = variant.entries.lightmaps_meta.clone() else {
            continue;
        };
        let quality = LightmapQuality::parse(&variant.lightmap_quality)?;
        let lighting =
            crate::package::lighting::read_lighting(&blobs.get(&variant.entries.lighting)?.0)
                .ok()?;
        let mut meta: crate::package::lightmaps::LightmapsMeta =
            serde_json::from_slice(&blobs.get(&previous)?.0).ok()?;
        meta.content_key = crate::render::lightmap_content_key(
            level,
            &lighting,
            LightmapBuildOptions::for_lightmaps(quality),
        );
        let mut bytes = serde_json::to_vec(&meta).ok()?;
        bytes.push(b'\n');
        let name = insert_blob(blobs, bytes, ".lightmaps.json", "lightmaps-meta");
        if name != previous {
            drop(blobs.remove(&previous));
        }
        variant.entries.lightmaps_meta = Some(name);
    }
    Some(())
}

fn fingerprint(
    source_sha256: &str,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
) -> String {
    fingerprint_with_revision(source_sha256, variants, dependencies, GEOMETRY_REVISION)
}

/// [`fingerprint`] with an explicit geometry revision.
///
/// Crate-visible so the fingerprint tests can prove a revision bump changes
/// the package key without editing the level source.
pub(crate) fn fingerprint_with_revision(
    source_sha256: &str,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
    geometry_revision: u32,
) -> String {
    use std::fmt::Write as _;
    let mut canonical = String::with_capacity(1024);
    canonical.push_str(COMPILER_NAME);
    canonical.push('\n');
    let _formatted_format_format = writeln!(canonical, "format {FORMAT_VERSION}");
    // The semantics record is the authored level itself; a schema revision
    // changes what a package means even when nothing else moved, so the level
    // format version is part of the build identity.
    let _formatted_level_format = writeln!(
        canonical,
        "level_format {}",
        crate::level::LEVEL_FORMAT_VERSION
    );
    let _formatted_source_source = writeln!(canonical, "source {source_sha256}");
    // Collision and navigation are prepared offline. A rim-width change must
    // invalidate packages even when their authored source and mesh are unchanged.
    let _formatted_rim_backing = writeln!(
        canonical,
        "rim_backing {}",
        crate::level::RIM_BACKING.to_bits()
    );
    let _formatted_records_mesh = writeln!(
        canonical,
        "records mesh {} props {} collision {} lighting {} probes {} navigation {}",
        crate::package::mesh::MESH_RECORD_VERSION,
        crate::package::props::PROPS_RECORD_VERSION,
        crate::package::collision::COLLISION_RECORD_VERSION,
        crate::package::lighting::LIGHTING_RECORD_VERSION,
        crate::package::world::PROBE_POSITIONS_VERSION,
        crate::package::navigation::NAVIGATION_RECORD_VERSION,
    );
    // The prepared data is a function of the lighting model and the transport
    // solver as well as of the source. Folding both revisions in means a
    // recalibrated model or a moved solver invalidates an existing package
    // instead of silently reusing its captures.
    let _formatted_solver_model = writeln!(
        canonical,
        "solver {} model {}",
        crate::lighting::transport::solver_fingerprint(),
        crate::lighting::model_fingerprint()
    );
    // The emitted static geometry is a function of the emitter as well as of
    // the source, so the geometry revision is part of the package identity
    // even when the source bytes did not move.
    let _formatted_geometry_geometry = writeln!(canonical, "geometry {geometry_revision}");
    for quality in variants {
        let _formatted_variant = writeln!(canonical, "variant {}", quality.name());
    }
    for dependency in dependencies {
        let _formatted_dep = writeln!(
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
        drop(read_declared_entry(&mut reader, entry)?);
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
        clippy::arithmetic_side_effects,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    mod performance;

    #[test]
    fn maintained_dense_props_fit_the_runtime_record_budget() {
        // Measured serialized prop record from capacity_dense, independently
        // of the generic archive-entry limit (which is only 256 MiB).
        let mut entry = PackageEntry {
            name: "blobs/dense.props".to_string(),
            role: "props".to_string(),
            bytes: 308_295_329,
            sha256: String::new(),
        };
        assert!(entry.bytes <= declared_entry_limit(&entry));
        assert_eq!(
            declared_entry_limit(&entry),
            crate::package::MAX_BINARY_BYTES
        );
        entry.bytes = 536_870_913; // One byte beyond the runtime's 512 MiB cap.
        assert!(entry.bytes > declared_entry_limit(&entry));
        entry.bytes = 308_295_329;
        entry.role = "texture".to_string();
        assert!(entry.bytes > declared_entry_limit(&entry));
    }

    #[test]
    fn full_eight_page_atlas_includes_its_container_in_the_record_budget() {
        let mut entry = PackageEntry {
            name: "blobs/full.lightmaps.ktx2".to_string(),
            role: "lightmaps".to_string(),
            bytes: 268_435_652, // Actual final Places Demo: 256 MiB plus KTX2 header.
            sha256: String::new(),
        };
        assert!(entry.bytes > crate::package::MAX_ENTRY_BYTES);
        assert!(entry.bytes <= declared_entry_limit(&entry));
        entry.bytes = crate::package::MAX_LIGHTMAP_ATLAS_BYTES + 1;
        assert!(entry.bytes > declared_entry_limit(&entry));
        entry.bytes = 268_435_652;
        entry.role = "texture".to_string();
        assert!(entry.bytes > declared_entry_limit(&entry));
    }

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

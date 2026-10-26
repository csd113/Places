//! The offline map compiler.
//!
//! Map Compiler owns the edit-source -> build-package pipeline, its command-line
//! application and its native GUI. The player loads the resulting records;
//! source preparation and offline baking stay here. Shared engine APIs remain
//! in the same crate so both applications agree on the record layouts.
//!
//! What a build does, in order:
//!
//! 1. Read and validate the authoring source, then run the semantic authoring
//!    preparation (`prepare_level`: fixture alignment, decal snapping,
//!    automatic baseboards). The result is the *semantics* record the package
//!    carries; the player never repeats this step.
//! 2. Resolve the level's logical materials and the content dependencies it
//!    names (placed and runtime-template models, surface and fixture textures), with SHA-256
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
//! preparation-source identity and capture mode. It is deliberately *not* a runtime validity check; a package whose
//! fingerprint is stale still loads if its records satisfy the format.
//!
//! Reflection probe *captures* are the one part of the build that needs a GPU:
//! they are produced by the headless capture step, which reads the prepared
//! package, renders each probe with the installed renderer and rewrites the
//! package with the captured cubemaps. A package without captures is refused by
//! the player rather than silently rendered without reflections.

mod application;
pub mod diagnostics;
pub mod distribution;
pub mod events;
pub mod geometry_check;
pub mod gui;
pub mod navigation;
mod probes;
pub mod progress;
pub mod project;
mod project_cache;
mod project_spool;
pub mod source;
mod storage;

pub use application::run;
pub use source::prepare_level;

use storage::factor_prepared_props;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::engine::level::LevelDef;
use crate::engine::materials::{MaterialTable, TextureCache};
use crate::engine::package::archive::{PendingEntry, write_archive_checked};
use crate::engine::package::collision::{CompiledCollision, write_collision};
use crate::engine::package::hash::{blob_name, sha256_hex};
use crate::engine::package::lighting::write_lighting;
use crate::engine::package::lightmaps::write_lightmaps;
use crate::engine::package::manifest::{
    DependencyKind, Manifest, PackageDependency, PackageEntry, ProbePayload, Variant,
    VariantEntries,
};
use crate::engine::package::mesh::write_mesh;
use crate::engine::package::props::{PropsEncodingStats, write_props_with_stats};
use crate::engine::package::{FORMAT_VERSION, PACKAGE_EXTENSION};
use crate::engine::quality::LightmapQuality;
use crate::engine::quality::{QualityLevel, ReflectionQuality};
use crate::engine::render::{
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
    /// Standalone authoring JSON, project directory or `places.project.json`.
    pub source: PathBuf,
    /// Exact project level ID; omitted selects the manifest's entry level.
    /// Standalone JSON sources require `None`.
    pub project_level: Option<String>,
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
    /// Validated cache decisions, with actionable input or integrity reasons.
    #[serde(default)]
    pub cache_decisions: Vec<CacheDecision>,
}

/// A hit, miss or explicit bypass of one existing compiler cache stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheDecision {
    /// `package` or `prepared_world`.
    pub stage: String,
    /// `hit`, `miss` or `bypass`.
    pub status: String,
    /// Inputs or validation results responsible for this decision.
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildInputs {
    revision: u32,
    source_sha256: String,
    catalog_sha256: String,
    #[serde(default)]
    catalog_inputs: Option<crate::engine::package::catalog_inputs::CatalogInputs>,
    #[serde(default)]
    compiler_code_sha256: Option<String>,
    tool_sha256: String,
    capture_probes: bool,
}

const BUILD_INPUTS_ENTRY: &str = "build-inputs.json";

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
    crate::engine::logging::info(format_args!(
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
    /// Exact vertex storage reduction during encoding; absent for reused blobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_encoding: Option<PropsEncodingStats>,
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
    /// Selected member, including non-entry project levels.
    pub id: String,
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
    /// Every declared payload passed integrity validation. Resource drift is
    /// reported separately among input differences.
    pub integrity_valid: bool,
    /// A new lighting/geometry preparation is required, rather than a refresh.
    pub bake_required: bool,
    /// Stable state for editor admission: current, refresh, bake, invalid.
    pub status: String,
}

/// Builds one map package, reusing a current package unless `force`.
///
/// # Errors
///
/// Returns an error when the source cannot be read or validated, the asset root
/// or catalog is missing, any preparation step fails, a record cannot be
/// encoded, or the archive cannot be published. A failed build never replaces
/// an existing package.
pub fn build(request: &BuildRequest) -> Result<BuildReport, String> {
    // Direct editor/API builds must use the same resource tree as fixture and
    // renderer consumers, just as compiler CLI builds do.
    crate::engine::paths::configure_assets(&request.asset_root)?;
    let scope = progress::BuildScope::start(&request.source, request.project_level.as_deref());
    let result = build_inner(request);
    scope.finish(&result);
    result
}

#[expect(clippy::too_many_lines, reason = "one cohesive build pipeline")]
fn build_inner(request: &BuildRequest) -> Result<BuildReport, String> {
    progress::check_cancelled()?;
    let started = Instant::now();
    let mut phases = Vec::new();
    if request.workers == 0 {
        return Err("--workers must be at least 1".to_string());
    }
    if request.variants.is_empty() {
        return Err("no lightmap variants requested".to_string());
    }
    let workers = request.workers.min(MAX_WORKERS);
    crate::engine::render::set_fill_workers(workers);
    let mut warnings = Vec::new();

    let snapshot = project::read_source(&request.source, request.project_level.as_deref())?;
    let source_bytes = snapshot.identity;
    let mut level = snapshot.level;
    if level.requires_full_lighting && !request.variants.contains(&LightmapQuality::Full) {
        return Err("This map requires Full lighting; include full in --variants".into());
    }
    let catalog_path = request.asset_root.join("catalog.json");
    let (catalog, catalog_hash) = load_catalog_identity(&catalog_path)?;
    crate::map_compiler::prepare_level(&mut level, catalog.assets(), None);
    crate::engine::project::validate_level_assets(&level, catalog.assets())?;
    if let Some(issue) = crate::engine::structure::clearances(&level).first() {
        return Err(format!(
            "Structural clearance: room {} ceiling and room {} floor leave {:.3} m; need at least {:.2} m. Review in editor Safety; move no room automatically.",
            issue.lower,
            issue.upper,
            issue.gap,
            crate::engine::level::MIN_STRUCTURE_M
        ));
    }
    let source_hash = sha256_hex(&source_bytes);
    let mut assets = crate::engine::props::PropAssets::with_root(request.asset_root.clone());

    progress::stage("resolving assets and dependency identities");
    let dependencies = collect_dependencies(&level, &catalog, &request.asset_root, &mut warnings)?;
    record_phase(&mut phases, "load_validate_prepare_dependencies", started);
    let tool_started = Instant::now();
    progress::stage("hashing compiler identity");
    let build_inputs = BuildInputs {
        revision: 2,
        catalog_inputs: Some(catalog_inputs(&level, &catalog_path)?),
        compiler_code_sha256: Some(env!("PLACES_COMPILER_CODE_SHA256").into()),
        source_sha256: source_hash.clone(),
        catalog_sha256: catalog_hash.clone(),
        tool_sha256: compiler_tool_sha256()?.to_string(),
        capture_probes: request.capture_probes,
    };
    record_phase(&mut phases, "compiler_tool_identity", tool_started);
    let phase_started = Instant::now();
    let fingerprint = fingerprint_with_build_inputs(
        &fingerprint(&source_hash, &request.variants, &dependencies),
        &build_inputs,
    );
    // Stage fingerprint: everything the illumination/geometry/collision/probe
    // preparation reads, with display metadata, navigation/AI components and
    // final presentation excluded. Those edits refresh semantics, navigation,
    // input provenance and atlas metadata without solving stored light again.
    let lighting_fingerprint = lighting_stage_fingerprint(
        &level,
        &request.variants,
        &dependencies,
        &build_inputs,
        &catalog,
        &catalog_path,
        &request.asset_root,
    )?;
    record_phase(&mut phases, "fingerprints", phase_started);
    let integrity_started = Instant::now();
    progress::stage("checking current package integrity");

    let mut cache_decisions = Vec::new();
    let current = if request.force {
        Err(vec!["forced rebuild requested".to_string()])
    } else {
        reuse_current(
            &request.out,
            &fingerprint,
            &build_inputs,
            &dependencies,
            &request.variants,
        )
    };
    if let Ok(reason) = current.as_ref() {
        cache_decisions.push(CacheDecision {
            stage: "package".to_string(),
            status: "hit".to_string(),
            reasons: vec![reason.clone()],
        });
        record_phase(&mut phases, "current_package_integrity", integrity_started);
        return Ok(BuildReport {
            source: request.source.display().to_string(),
            out: request.out.display().to_string(),
            rebuilt: false,
            reuse_reason: reason.clone(),
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
            cache_decisions,
        });
    }
    if let Err(reasons) = current {
        cache_decisions.push(CacheDecision {
            stage: "package".to_string(),
            status: if request.force { "bypass" } else { "miss" }.to_string(),
            reasons,
        });
    }
    record_phase(&mut phases, "current_package_integrity", integrity_started);
    let semantics_started = Instant::now();

    // Canonical bytes: the semantic record is part of the package's identity
    // and two builds of the same content must publish identical archives.
    let semantics = crate::engine::canonical_json::canonical_json_bytes(&level)
        .map_err(|error| format!("could not serialize semantics: {error}"))?;
    record_phase(&mut phases, "semantics", semantics_started);
    let mut blobs: BlobMap = BTreeMap::new();
    let mut variants = Vec::with_capacity(request.variants.len());
    let mut variant_stats = Vec::with_capacity(request.variants.len());
    let mut cache = crate::engine::lighting::lightmap::LightmapCache::memory_only();
    let cancelled = progress::cancellation_flag();
    let navigation_started = Instant::now();
    progress::check_cancelled()?;
    progress::stage("preparing navigation and collision inputs");
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
    progress::stage("checking reusable prepared records");
    // Reuse the previous package's prepared geometry, lighting, probes and
    // collision when the lighting stage fingerprint matches: an AI-only edit
    // must not rebake illumination.
    let reused = if request.force {
        Err("forced rebuild requested".to_string())
    } else {
        reuse_prepared_lighting(
            &request.out,
            &lighting_fingerprint,
            &level,
            &request.variants,
            &build_inputs,
            &mut warnings,
        )
        .or_else(|original| {
            let Some(exported) = project::exported_cache(&request.source) else {
                return Err(original);
            };
            if exported == request.out || !exported.is_file() {
                return Err(original);
            }
            reuse_prepared_lighting(
                &exported,
                &lighting_fingerprint,
                &level,
                &request.variants,
                &build_inputs,
                &mut warnings,
            )
            .map_err(|reason| format!("{original}; project export: {reason}"))
        })
    };
    record_phase(&mut phases, "prepared_package_integrity", reuse_started);
    if let Ok((reused_variants, reused_blobs)) = reused {
        progress::stage("reusing verified geometry, lighting, probes and collision");
        cache_decisions.push(CacheDecision {
            stage: "prepared_world".to_string(),
            status: if reused_variants.len() == request.variants.len() {
                "hit"
            } else {
                "partial"
            }
            .to_string(),
            reasons: vec![
                "common prepared-stage identity and every declared record verified".to_string(),
            ],
        });
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
                prop_encoding: None,
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
        let reason = reused
            .err()
            .unwrap_or_else(|| "prepared stage unavailable".to_string());
        cache_decisions.push(CacheDecision {
            stage: "prepared_world".to_string(),
            status: if request.force { "bypass" } else { "miss" }.to_string(),
            reasons: vec![reason],
        });
    }
    let missing: Vec<_> = request
        .variants
        .iter()
        .copied()
        .filter(|quality| {
            !variants
                .iter()
                .any(|variant| variant.lightmap_quality == quality.name())
        })
        .collect();
    for quality in &request.variants {
        let hit = !missing.contains(quality);
        cache_decisions.push(CacheDecision {
            stage: format!("prepared_world/{}", quality.name()),
            status: if hit {
                "hit"
            } else if request.force {
                "bypass"
            } else {
                "miss"
            }
            .into(),
            reasons: vec![
                if hit {
                    "quality and common preparation inputs verified"
                } else {
                    "quality unavailable with current preparation inputs"
                }
                .into(),
            ],
        });
    }
    if !missing.is_empty() {
        let device_started = Instant::now();
        progress::check_cancelled()?;
        progress::stage("resolving capture materials");
        let mut capture = if request.capture_probes {
            Some(CaptureContext::new(&level, &catalog, &request.asset_root)?)
        } else {
            None
        };
        // The solve needs the same numeric alpha and linear texture reflectance
        // as drawing. Reuse the capture's shared decoded images when available;
        // a CPU-only build must resolve them too, rather than bake a logical table.
        let materials = capture.as_ref().map_or_else(
            || resolve_build_materials(&level, &catalog, &request.asset_root),
            |context| Ok(context.loaded.materials.clone()),
        )?;
        record_phase(&mut phases, "capture_device_materials", device_started);
        for quality in &missing {
            progress::check_cancelled()?;
            progress::variant(quality.name());
            progress::stage("preparing geometry and lightmap layout");
            crate::engine::logging::info(format_args!(
                "[compiler-progress] preparing {}",
                quality.name()
            ));
            let variant_started = Instant::now();
            let diagnostic_scope = diagnostics::begin_variant(
                &diagnostics::BuildIdentity {
                    request,
                    source_sha256: &source_hash,
                    semantics: &semantics,
                    catalog_sha256: &catalog_hash,
                    compiler_fingerprint: &fingerprint,
                    lighting_fingerprint: &lighting_fingerprint,
                    dependencies: &dependencies,
                    materials: &materials,
                },
                *quality,
            );
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
            if let Some(scope) = &diagnostic_scope {
                scope.finish(&variant, &stats);
            }
            record_phase(
                &mut phases,
                format!("prepare_encode_{}", quality.name()),
                variant_started,
            );
            if let Some(capture_context) = capture.as_mut() {
                // Preparation and capture share the resolved material state.
                if stats.probe_points > 0 && !quality.is_off() {
                    let capture_started = Instant::now();
                    progress::check_cancelled()?;
                    progress::stage("capturing reflection probes");
                    crate::engine::logging::info(format_args!(
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

    // Reused and newly prepared qualities follow the caller's requested order.
    variants.sort_by_key(|variant| {
        request
            .variants
            .iter()
            .position(|quality| quality.name() == variant.lightmap_quality)
    });
    variant_stats.sort_by_key(|stats| {
        request
            .variants
            .iter()
            .position(|quality| quality.name() == stats.lightmap_quality)
    });

    let sharing_started = Instant::now();
    progress::check_cancelled()?;
    progress::variant("");
    progress::stage("sharing prepared prop records");
    factor_prepared_props(&mut variants, &mut blobs)?;
    record_phase(&mut phases, "share_prepared_props", sharing_started);

    let manifest_started = Instant::now();
    progress::stage("hashing records and validating manifest");
    let input_bytes = crate::engine::canonical_json::canonical_json_bytes(&build_inputs)
        .map_err(|error| format!("could not serialize compiler input provenance: {error}"))?;
    drop(blobs.insert(
        BUILD_INPUTS_ENTRY.to_string(),
        (input_bytes, "compiler-inputs".to_string()),
    ));
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
        "zip-zstd-v1".to_string(),
        "geometry".to_string(),
        "props".to_string(),
        "lighting".to_string(),
        "collision".to_string(),
        "navigation".to_string(),
    ];
    if !level.script_sources.is_empty() {
        required.push("luau-v1".to_string());
        required.push("campaign-progress-v1".to_string());
        required.push("world-observations-v1".to_string());
    }
    // New host calls and reveal-only semantics must not silently disappear
    // in an older player which understands the original Luau snapshot format.
    if !level.script_sources.is_empty()
        || level.props.iter().any(|prop| prop.components.iter().any(|component| {
            matches!(component, crate::engine::level::ComponentDef::LightResponse(response) if response.reveal_only)
        }))
        || level.spawn_templates.iter().any(|template| template.components.iter().any(|component| {
            matches!(component, crate::engine::level::ComponentDef::LightResponse(response) if response.reveal_only)
        }))
    {
        required.push("scene-actions-v1".to_string());
    }
    if !level.script_sources.is_empty()
        || level.props.iter().any(|prop| {
            prop.components
                .iter()
                .any(|component| matches!(component, crate::engine::level::ComponentDef::Carry(_)))
        })
        || level.spawn_templates.iter().any(|template| {
            template
                .components
                .iter()
                .any(|component| matches!(component, crate::engine::level::ComponentDef::Carry(_)))
        })
    {
        required.push("carry-assemblies-v1".to_string());
        if !level.script_sources.is_empty() {
            required.push("script-animation-v1".to_string());
        }
    }
    if blobs_have_shared_props(&entries) {
        required.push("props-shared-streams".to_string());
    }
    if blobs_need_shared_props_v2(&entries)? {
        required.push("props-shared-streams-v2".to_string());
    }
    if blobs_need_shared_props_revision(&entries, 3)? {
        required.push("props-shared-streams-v3".to_string());
    }
    if blobs_need_shared_props_revision(&entries, 4)? {
        required.push("props-shared-streams-v4".to_string());
    }
    if variants
        .iter()
        .any(|variant| variant.entries.lightmaps.is_some())
    {
        required.push("lightmaps-hdr".to_string());
        required.push("lightmaps-compact-v1".to_string());
        required.push("lightmaps-compact-v2".to_string());
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
        required.push("probes-hdr".to_string());
    }
    if variants
        .iter()
        .any(|variant| variant.entries.static_light_off.is_some())
    {
        required.push("static-light-state-v1".to_string());
    }
    if level
        .ceiling_lights
        .iter()
        .any(|fixture| fixture.switch_group.is_some())
    {
        required.push("light-circuits-v1".to_string());
    }
    if !level.transitions.is_empty() {
        required.push("project-transitions-v1".into());
    }
    if level
        .global_illuminators
        .iter()
        .any(|light| light.visual.is_some())
    {
        required.push("celestial-visuals-v1".into());
    }
    if level.has_character_textures() {
        required.push("character-textures-v1".into());
    }
    if level.has_mannequin_pins() {
        required.push("miniature-pins-v1".into());
    }
    if level.has_shadow_only_entities() {
        required.push("shadow-only-v1".into());
    }
    if level.requires_full_lighting {
        required.push("full-lighting-v1".into());
    }
    if level
        .volumes
        .iter()
        .any(|volume| volume.item_receipt != crate::engine::level::ItemReceiptDef::Overlap)
    {
        required.push("item-receipts-v1".into());
    }
    if !level.acoustic_materials.is_empty()
        || level
            .rooms
            .iter()
            .any(|room| room.acoustics != crate::engine::audio::acoustics::RoomAcoustics::default())
    {
        required.push("material-acoustics-v1".into());
    }
    if !level.water.is_empty() {
        required.push("water-motion-v1".into());
    }
    if level.props.iter().any(|prop| prop.animated_door.is_some()) {
        required.push("animated-door-v1".into());
    }
    let manifest = Manifest {
        schema: crate::engine::package::PACKAGE_SCHEMA.to_string(),
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
    crate::engine::package::world::validate_lighting_requirement(&level, &manifest)?;
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
    progress::check_cancelled()?;
    progress::stage("compressing and publishing archive");
    write_archive_checked(&request.out, entries, progress::check_cancelled)?;
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
        cache_decisions,
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
        progress::check_cancelled()?;
        let request = BuildRequest {
            source: source.clone(),
            project_level: None,
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
    if let Some(index) = crate::engine::package::project::inspect(path)? {
        crate::engine::package::project::validate(path, &progress::check_cancelled)?;
        let mut combined: Option<ValidationReport> = None;
        let mut identities = BTreeSet::new();
        for level in &index.levels {
            progress::check_cancelled()?;
            let mut selected = open_package(path)?;
            crate::engine::package::project::select(&mut selected, Some(&level.id))?;
            let manifest = read_manifest(&mut selected)?;
            identities.extend(manifest.dependencies.iter().map(|dependency| {
                format!(
                    "{} {} {}",
                    dependency_kind_name(dependency.kind),
                    dependency.path,
                    dependency.sha256
                )
            }));
            let report = validate_reader(path, selected)?;
            if let Some(aggregate) = &mut combined {
                aggregate.dependencies_intact &= report.dependencies_intact;
                aggregate.warnings.extend(report.warnings);
            } else {
                combined = Some(report);
            }
        }
        let mut report = combined.ok_or("Project has no levels")?;
        report.id = index.project.id;
        report.name = index.project.name;
        let physical = open_package(path)?;
        report.entries = physical.names().len();
        report.bytes = 0;
        for name in physical.names().to_vec() {
            report.bytes = report.bytes.saturating_add(physical.entry_size(&name)?);
        }
        report.dependencies = identities.len();
        return Ok(report);
    }
    validate_reader(path, open_package(path)?)
}

fn validate_reader(
    path: &Path,
    mut reader: crate::engine::package::PackageReader<std::fs::File>,
) -> Result<ValidationReport, String> {
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
            crate::engine::package::world::validate_semantics(&reader, &manifest, &level)?;
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
    crate::engine::package::world::inspect(path)
}

/// Recomputes the build fingerprint of `source` and compares it with a package.
///
/// # Errors
///
/// Returns an error when the source cannot be read or validated, or the package
/// cannot be opened.
pub fn verify(source: &Path, package: &Path, asset_root: &Path) -> Result<VerifyReport, String> {
    verify_selected(source, None, package, asset_root)
}

/// Verifies an explicitly selected project level, or a standalone source.
///
/// # Errors
/// Returns source/project, asset or package validation errors.
pub fn verify_selected(
    source: &Path,
    selected: Option<&str>,
    package: &Path,
    asset_root: &Path,
) -> Result<VerifyReport, String> {
    crate::engine::paths::configure_assets(asset_root)?;
    let snapshot = project::read_source(source, selected)?;
    let source_bytes = snapshot.identity;
    let mut level = snapshot.level;
    let manifest = crate::engine::package::world::inspect_level(package, &level.id)?;
    let catalog_path = asset_root.join("catalog.json");
    let mut warnings = Vec::new();
    let (catalog, catalog_hash) = load_catalog_identity(&catalog_path)?;
    crate::map_compiler::prepare_level(&mut level, catalog.assets(), None);
    crate::engine::project::validate_level_assets(&level, catalog.assets())?;
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
    let build_inputs = BuildInputs {
        revision: 2,
        catalog_inputs: Some(catalog_inputs(&level, &catalog_path)?),
        compiler_code_sha256: Some(env!("PLACES_COMPILER_CODE_SHA256").into()),
        source_sha256: sha256_hex(&source_bytes),
        catalog_sha256: catalog_hash,
        tool_sha256: compiler_tool_sha256()?.to_string(),
        capture_probes: true,
    };
    let current_fingerprint = fingerprint_with_build_inputs(
        &fingerprint(&sha256_hex(&source_bytes), &variants, &dependencies),
        &build_inputs,
    );
    let mut differences = warnings
        .into_iter()
        .filter(|warning| warning.starts_with("dependency "))
        .collect::<Vec<_>>();
    differences.extend(package_input_differences(
        package,
        &manifest,
        &build_inputs,
        &dependencies,
        &variants,
        &current_fingerprint,
    ));
    let integrity = verify_declared_entries(package, &manifest);
    let integrity_valid = integrity.is_ok() && warnings_absent(&differences);
    if let Err(error) = integrity {
        differences.push(format!("package integrity failed: {error}"));
    }
    let prepared = lighting_stage_fingerprint(
        &level,
        &variants,
        &dependencies,
        &build_inputs,
        &catalog,
        &catalog_path,
        asset_root,
    )?;
    let prepared_reusable = integrity_valid
        && manifest.lighting_fingerprint.as_deref() == Some(&prepared)
        && read_build_inputs(package, &manifest)
            .is_ok_and(|previous| preparation_inputs_match(&previous, &build_inputs));
    let current = differences.is_empty();
    let status = if !integrity_valid {
        "invalid"
    } else if current {
        "current"
    } else if prepared_reusable {
        "refresh"
    } else {
        "bake"
    }
    .into();
    Ok(VerifyReport {
        id: level.id,
        source: source.display().to_string(),
        package: package.display().to_string(),
        package_fingerprint: manifest.compiler_fingerprint,
        current_fingerprint,
        current,
        differences,
        integrity_valid,
        bake_required: !prepared_reusable,
        status,
    })
}

fn warnings_absent(differences: &[String]) -> bool {
    !differences
        .iter()
        .any(|reason| reason.starts_with("dependency '") && reason.contains("could not be read"))
}

/// Read-only complete-project validity for the editor/build planner. Never bakes.
/// # Errors
/// Returns unsafe project, source, or package errors.
pub fn verify_project(
    source: &Path,
    package: &Path,
    asset_root: &Path,
) -> Result<Vec<VerifyReport>, String> {
    let folder = crate::engine::project::ProjectFolder::open(source)?;
    folder
        .manifest()
        .levels
        .iter()
        .map(|level| verify_selected(source, Some(&level.id), package, asset_root))
        .collect()
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
    crate::engine::paths::PathConfig::from_env()?.resolve_assets(explicit)
}

/// The compiler's headless capture context: a window-free renderer plus the
/// resolved material state the install path needs.
struct CaptureContext {
    renderer: Option<crate::engine::render::Renderer>,
    loaded: crate::engine::loader::LoadedLevel,
}

/// Resolve once for the offline solve, including CPU-only builds. The capture
/// context and transport sidecars share these image allocations through Arc.
fn resolve_build_materials(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    asset_root: &Path,
) -> Result<MaterialTable, String> {
    let materials = crate::engine::materials::resolve_materials(
        level,
        catalog.assets(),
        None,
        Some(asset_root),
        &mut TextureCache::new(),
    )?;
    crate::engine::materials::check_texture_budget(&materials)
        .map_err(|error| format!("level `{}`: {error}", level.id))?;
    Ok(materials)
}

impl CaptureContext {
    fn new(
        level: &LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        asset_root: &Path,
    ) -> Result<Self, String> {
        let materials = resolve_build_materials(level, catalog, asset_root)?;
        let mut texture_cache = TextureCache::new();
        let light_sheets = crate::engine::loader::resolve_fixture_sheets(
            level,
            catalog.assets(),
            None,
            &mut texture_cache,
            &materials,
        )?;
        let loaded = crate::engine::loader::LoadedLevel {
            package_identity: None,
            catalog: Arc::new(catalog.clone()),
            level: level.clone(),
            materials,
            light_sheets,
            entry: crate::engine::loader::LevelEntry {
                id: level.id.clone(),
                name: level.name.clone(),
                author: level.author.clone(),
                source_type: crate::engine::loader::LevelSourceType::Bundled,
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
        assets: &crate::engine::props::PropAssets,
        quality: LightmapQuality,
        build: crate::engine::render::LevelBuild,
        blobs: &mut BlobMap,
    ) -> Result<Vec<ProbePayload>, String> {
        let probe_points = crate::engine::render::routing_from_mesh(
            &build.mesh,
            &crate::engine::render::MaterialRenderState::from_table(&self.loaded.materials)
                .reflections,
            self.loaded.materials.entries().len(),
        )
        .probe_points;
        if probe_points.is_empty() {
            return Ok(Vec::new());
        }
        if self.renderer.is_none() {
            progress::stage("initializing reflection capture device");
            self.renderer = Some(crate::engine::render::Renderer::new_headless(
                crate::engine::render::DrawableSize::new(1280, 720),
            )?);
        }
        let renderer = self
            .renderer
            .as_mut()
            .ok_or("capture device is unavailable")?;
        renderer.set_quality(QualityLevel::High);
        progress::stage("uploading prepared reflection scene");
        renderer.set_lightmap_quality(quality);
        renderer.set_reflection_quality(ReflectionQuality::Full);
        renderer.install_prepared(
            &self.loaded,
            Arc::new(build),
            assets.clone(),
            crate::engine::render::CharacterScene::new(),
            false,
        );
        while !renderer.advance_prepared_install() {
            progress::check_cancelled()?;
            // Upload phases advance one bounded step per call.
        }
        let mut payloads = Vec::with_capacity(2);
        progress::check_cancelled()?;
        progress::stage("rendering and reading Full reflection faces");
        let full = renderer.read_back_probe_faces()?;
        payloads.push(probe_payload(&full, &probe_points, blobs)?);
        renderer.reprepare_reflection_probes(ReflectionQuality::Medium);
        progress::check_cancelled()?;
        progress::stage("rendering and reading Medium reflection faces");
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
    probes: &[crate::engine::render::ProbeFaceReadback],
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
    let levels = crate::engine::render::ProbeFaceReadback::packaged_mip_levels(face_edge);
    if face_edge == 0
        || u32::try_from(probes.len()).unwrap_or(u32::MAX)
            > u32::try_from(crate::engine::package::MAX_PROBES).unwrap_or(u32::MAX)
    {
        return Err(format!("probe capture produced {} probes", probes.len()));
    }
    let mut cubemaps = Vec::with_capacity(probes.len());
    let work = progress::Work::start(
        "prefiltering and encoding reflection probes",
        probes.len(),
        "probes",
    );
    for probe in probes {
        progress::check_cancelled()?;
        if probe.face_size != face_edge {
            return Err("probe capture mixed face sizes".to_string());
        }
        let chain = probe.packaged_mips()?;
        cubemaps.push(insert_blob(
            blobs,
            crate::engine::package::ktx2::write_rgba16f_cube_with_mips(face_edge, &chain)?,
            ".probe.ktx2",
            "probes",
        ));
        work.advance(1);
    }
    work.finish();
    // The positions record names the *routing* probe points, which the
    // player recomputes from the decoded mesh and matches against; the cube
    // itself is captured from the lifted bake position.
    let positions = crate::engine::package::world::ProbePositions {
        record_version: crate::engine::package::world::PROBE_POSITIONS_VERSION,
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

fn open_package(
    path: &Path,
) -> Result<crate::engine::package::PackageReader<std::fs::File>, String> {
    let file = crate::engine::package::archive::open_file(path)?;
    crate::engine::package::PackageReader::new(file).map_err(|error| error.to_string())
}

fn read_manifest<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::engine::package::PackageReader<R>,
) -> Result<Manifest, String> {
    crate::engine::package::project::select(reader, None)?;
    let bytes = reader.read_entry("manifest.json", crate::engine::package::MAX_MANIFEST_BYTES)?;
    let names = reader.names().to_vec();
    Manifest::from_json(&bytes, &names)
}

/// Reads one declared entry and verifies both its manifest hash and, for a
/// content-addressed blob, the hash embedded in its name.
fn read_declared_entry<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::engine::package::PackageReader<R>,
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
        let expected = crate::engine::package::hash::sha256_from_blob_name(&entry.name)
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
        crate::engine::package::MAX_SEMANTICS_BYTES
    } else if entry.role == "lightmaps" {
        crate::engine::package::MAX_LIGHTMAP_ATLAS_BYTES
    } else if matches!(entry.role.as_str(), "mesh" | "props") {
        crate::engine::package::MAX_BINARY_BYTES
    } else {
        crate::engine::package::MAX_ENTRY_BYTES
    }
}

fn validate_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::engine::package::PackageReader<R>,
    variant: &Variant,
) -> Result<(), String> {
    let mesh_bytes = reader.read_blob(
        &variant.entries.mesh,
        crate::engine::package::MAX_BINARY_BYTES,
    )?;
    drop(crate::engine::package::mesh::read_mesh(&mesh_bytes)?);
    drop(mesh_bytes);
    let props_bytes =
        crate::engine::package::shared_props::read_record(reader, &variant.entries.props)?;
    let main_models: std::collections::BTreeSet<String> =
        crate::engine::package::props::read_props(&props_bytes)?
            .into_iter()
            .map(|batch| batch.model)
            .collect();
    drop(props_bytes);
    let lighting_bytes = reader.read_blob(
        &variant.entries.lighting,
        crate::engine::package::MAX_LIGHTING_BYTES,
    )?;
    let lighting = crate::engine::package::lighting::read_lighting(&lighting_bytes)?;
    drop(lighting_bytes);
    validate_static_light_off(reader, variant, &lighting, &main_models)?;
    let collision_bytes = reader.read_blob(
        &variant.entries.collision,
        crate::engine::package::MAX_COLLISION_BYTES,
    )?;
    drop(crate::engine::package::collision::read_collision(
        &collision_bytes,
    )?);
    drop(collision_bytes);
    let navigation_bytes = reader.read_blob(
        &variant.entries.navigation,
        crate::engine::package::MAX_NAVIGATION_BYTES,
    )?;
    drop(crate::engine::package::navigation::read_navigation(
        &navigation_bytes,
    )?);
    drop(navigation_bytes);
    if let (Some(pages), Some(meta)) = (&variant.entries.lightmaps, &variant.entries.lightmaps_meta)
    {
        let page_bytes =
            reader.read_blob(pages, crate::engine::package::MAX_LIGHTMAP_ATLAS_BYTES)?;
        let meta_bytes =
            reader.read_entry(meta, crate::engine::package::MAX_LIGHTMAP_METADATA_BYTES)?;
        let _ = crate::engine::package::lightmaps::read_lightmaps_owned(&meta_bytes, page_bytes)?;
    }
    if let Some(irradiance) = &variant.entries.irradiance {
        let bytes = reader.read_blob(irradiance, crate::engine::package::MAX_ENTRY_BYTES)?;
        let _ = crate::engine::lighting::probes::ProbeField::read(&bytes)?;
    }
    for probe in &variant.entries.probes {
        for cubemap in &probe.cubemaps {
            let cube_bytes = reader.read_blob(cubemap, crate::engine::package::MAX_ENTRY_BYTES)?;
            let image = crate::engine::package::world::read_probe_image(&cube_bytes)?;
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

/// Optional states obey the same structural/runtime checks as primary records.
fn validate_static_light_off<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::engine::package::PackageReader<R>,
    variant: &Variant,
    lighting: &crate::engine::lighting::LevelLighting,
    main_models: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    let Some(state) = &variant.entries.static_light_off else {
        return Ok(());
    };
    if state
        .fixtures
        .iter()
        .any(|fixture| lighting.fixture_light_index(*fixture).is_none())
    {
        return Err("static light state references a missing fixture".to_string());
    }
    let mesh_bytes = reader.read_blob(&state.mesh, crate::engine::package::MAX_BINARY_BYTES)?;
    drop(crate::engine::package::mesh::read_mesh(&mesh_bytes)?);
    drop(mesh_bytes);
    let props_bytes = crate::engine::package::shared_props::read_record(reader, &state.props)?;
    let props = crate::engine::package::props::read_props(&props_bytes)?;
    if props
        .iter()
        .any(|batch| !main_models.contains(&batch.model))
    {
        return Err("static light state references an unexpected model".to_string());
    }
    Ok(())
}

fn check_dependencies(manifest: &Manifest, warnings: &mut Vec<String>) {
    let asset_root = crate::engine::paths::assets();
    for dependency in &manifest.dependencies {
        if dependency.kind == DependencyKind::Embedded {
            warnings.push(format!(
                "dependency {} is embedded in the package",
                dependency.path
            ));
            continue;
        }
        let root = match &asset_root {
            Ok(root) => root,
            Err(error) => {
                warnings.push(format!(
                    "dependency {} cannot resolve: {error}",
                    dependency.path
                ));
                continue;
            }
        };
        if let Err(error) = crate::engine::package::dependencies::validate(root, dependency) {
            warnings.push(error);
        }
    }
}

/// `(blob name, (bytes, role))` for one variant's prepared records.
type BlobMap = BTreeMap<String, (Vec<u8>, String)>;

#[expect(clippy::too_many_arguments, reason = "one cohesive variant build")] // one cohesive variant build
#[expect(clippy::too_many_lines, reason = "one cohesive variant build")] // one cohesive variant build
fn build_variant(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
    quality: LightmapQuality,
    cache: &mut crate::engine::lighting::lightmap::LightmapCache,
    cancelled: &std::sync::atomic::AtomicBool,
    blobs: &mut BlobMap,
    warnings: &mut Vec<String>,
    navigation_name: &str,
    navigation_report: &crate::map_compiler::navigation::NavBakeReport,
) -> Result<(Variant, VariantStats, crate::engine::render::LevelBuild), String> {
    let options = LightmapBuildOptions::for_lightmaps(quality);
    let compiled_collision = CompiledCollision::from_level(level);
    // The bake derives prop occlusion boxes from the same request's model
    // bytes; an edited model can never reuse stale boxes.
    crate::engine::lighting::set_preparation_assets(catalog.clone(), assets.clone());
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
            crate::engine::render::LightmapFillOutcome::Filled(product)
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
                crate::engine::render::relight_probe_props(
                    &mut build.batches,
                    &build.lighting,
                    None,
                );
            }
            crate::engine::render::LightmapFillOutcome::Filled(mut product) => {
                // Label the prepared moving-object field: a probe in no room or
                // inside a wall is never sampled, which is what stops light
                // from bleeding through a floor or a full-height wall.
                if let Some(mut field) = product.probes.take() {
                    probes::label(
                        &mut field,
                        level,
                        &build.lighting,
                        &fill.transport,
                        &compiled_collision.walls,
                        quality.name(),
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
                    if let Err(error) = crate::engine::lighting::transport::probe_audit::dump_labels(
                        &field,
                        quality.name(),
                    ) {
                        crate::engine::logging::warn(format_args!("[probe-diagnostics] {error}"));
                    }
                    let (total, valid) = field.summary();
                    crate::engine::logging::info(format_args!(
                        "[lightmaps] irradiance field probes={total} valid={valid}"
                    ));
                    build.probes = Some(std::sync::Arc::new(field));
                }
                let atlas = std::sync::Arc::new(product.lightmaps);
                build.lightmap_millis = atlas.stats.bake_millis;
                build.lightmaps = Some(atlas);
                build.lightmap_failure = None;
                lightmap_failure = None;
                crate::engine::render::relight_probe_props(
                    &mut build.batches,
                    &build.lighting,
                    build.probes.as_deref(),
                );
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
                    crate::engine::render::relight_blend_decals(
                        Arc::make_mut(&mut build.mesh),
                        level,
                        catalog.assets(),
                        &build.lighting,
                        field,
                    );
                }
            }
            crate::engine::render::LightmapFillOutcome::Failed(
                failure @ (crate::engine::lighting::lightmap::LightmapFailure::FillNonFinite
                | crate::engine::lighting::lightmap::LightmapFailure::Worker
                | crate::engine::lighting::lightmap::LightmapFailure::TransportEnergy),
            ) => {
                return Err(format!(
                    "{} lighting bake failed physical validation: {}",
                    quality.name(),
                    failure.name()
                ));
            }
            crate::engine::render::LightmapFillOutcome::Failed(failure) => {
                // The runtime contract falls back to the vertex-lit build with
                // the same baked lighting when an atlas cannot be produced.
                lightmap_failure = Some(failure.name().to_string());
                build.mesh = Arc::new(rebuild_vertex_lit_level(
                    level,
                    catalog,
                    assets,
                    materials,
                    &build.lighting,
                    &mut build.batches,
                ));
                build.lightmaps = None;
                build.lightmap_failure = Some(failure);
                warnings.push(format!(
                    "{} lightmaps fell back to vertex lighting: {}",
                    quality.name(),
                    failure.name()
                ));
            }
            crate::engine::render::LightmapFillOutcome::Cancelled => {
                return Err("lightmap fill was cancelled".to_string());
            }
        }
    }

    let material_state = crate::engine::render::MaterialRenderState::from_table(materials);
    progress::check_cancelled()?;
    progress::stage("encoding prepared records and Static circuit state");
    let routing = crate::engine::render::routing_from_mesh(
        &build.mesh,
        &material_state.reflections,
        materials.entries().len(),
    );

    let mesh_name = insert_blob(blobs, write_mesh(&build.mesh)?, ".mesh", "mesh");
    let static_light_off =
        prepare_static_light_off(level, catalog, assets, materials, quality, &build, blobs)?;
    let (props_bytes, prop_encoding) = write_props_with_stats(&build.batches)?;
    let props_name = insert_blob(blobs, props_bytes, ".props", "props");
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
                Some(insert_blob(blobs, ktx2, ".lightmaps.pllc", "lightmaps")),
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
        prop_encoding: Some(prop_encoding),
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
            static_light_off,
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
    report_record_storage(quality, &variant.entries, blobs, prop_encoding);
    // Records above retain the labelled field and relit runtime mesh. Move the
    // same variant directly into capture with its historical capture state;
    // at most one prepared variant is retained, and the atlas is never copied.
    if let Some(colours) = capture_colours {
        for (vertex, colour) in Arc::make_mut(&mut build.mesh)
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
        crate::engine::render::dump_lightmaps_for_level(level, atlas);
    }
    Ok((variant, stats, build))
}

/// One opt-in circuit gets exact Static colours without an additional bake.
fn prepare_static_light_off(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &MaterialTable,
    quality: LightmapQuality,
    build: &crate::engine::render::LevelBuild,
    blobs: &mut BlobMap,
) -> Result<Option<crate::engine::package::manifest::StaticLightEntries>, String> {
    if quality != LightmapQuality::Off {
        return Ok(None);
    }
    let fixtures: Vec<usize> = level
        .ceiling_lights
        .iter()
        .enumerate()
        .filter_map(|(index, fixture)| {
            (fixture.static_switch == crate::engine::level::StaticSwitchMode::Prepared)
                .then_some(index)
        })
        .collect();
    if fixtures.is_empty() {
        return Ok(None);
    }
    let state_started = Instant::now();
    let mut lighting = build.lighting.clone();
    crate::engine::logging::info(format_args!(
        "[compiler-state] shared_visibility_bytes={} clone_ms={:.3}",
        build.lighting.shared_visibility_bytes(),
        state_started.elapsed().as_secs_f64() * 1_000.0_f64
    ));
    for fixture in &fixtures {
        let sources = lighting.fixture_light_indices(*fixture);
        if sources.is_empty() {
            return Err("Static circuit fixture has no resolved light".into());
        }
        for source in sources {
            let _changed = lighting.set_light_enabled(source, false);
        }
    }
    let mut batches = Vec::new();
    let mesh = rebuild_vertex_lit_level(level, catalog, assets, materials, &lighting, &mut batches);
    crate::engine::logging::info(format_args!(
        "[compiler-state] static_off_prepare_ms={:.3}",
        state_started.elapsed().as_secs_f64() * 1_000.0_f64
    ));
    Ok(Some(crate::engine::package::manifest::StaticLightEntries {
        fixtures,
        mesh: insert_blob(blobs, write_mesh(&mesh)?, ".mesh", "mesh"),
        props: insert_blob(
            blobs,
            write_props_with_stats(&batches)?.0,
            ".props",
            "props",
        ),
    }))
}

fn report_preparation(quality: LightmapQuality, build: &crate::engine::render::LevelBuild) {
    crate::engine::logging::info(format_args!(
        "[compile-timing] quality={} legacy_lighting_ms={:.3} prop_geometry_and_charts_ms={:.3} architecture_and_charts_ms={:.3}",
        quality.name(),
        build.timings.lighting_millis,
        build.timings.props_millis,
        build.timings.surfaces_millis
    ));
}

fn report_record_storage(
    quality: LightmapQuality,
    entries: &VariantEntries,
    blobs: &BlobMap,
    props: PropsEncodingStats,
) {
    let size = |name: &str| blobs.get(name).map_or(0, |(bytes, _)| bytes.len());
    let unique_bytes = blobs
        .values()
        .fold(0_usize, |sum, (bytes, _)| sum.saturating_add(bytes.len()));
    crate::engine::logging::info(format_args!(
        "[compile-storage] quality={} mesh_bytes={} props_bytes={} lighting_bytes={} collision_bytes={} navigation_bytes={} atlas_bytes={} irradiance_bytes={} prop_input_slots={} prop_stored_slots={} prop_duplicate_slots={} unique_record_bytes={}",
        quality.name(),
        size(&entries.mesh),
        size(&entries.props),
        size(&entries.lighting),
        size(&entries.collision),
        size(&entries.navigation),
        entries.lightmaps.as_deref().map_or(0, size),
        entries.irradiance.as_deref().map_or(0, size),
        props.input_vertex_slots,
        props.stored_vertex_slots,
        props.duplicate_slots,
        unique_bytes,
    ));
}

fn blobs_have_shared_props(entries: &[PendingEntry]) -> bool {
    entries
        .iter()
        .any(|entry| entry.name.ends_with(".props.shared"))
}

fn blobs_need_shared_props_v2(entries: &[PendingEntry]) -> Result<bool, String> {
    blobs_need_shared_props_revision(entries, 2)
}

fn blobs_need_shared_props_revision(
    entries: &[PendingEntry],
    minimum: u16,
) -> Result<bool, String> {
    let mut needed = false;
    for entry in entries
        .iter()
        .filter(|entry| entry.name.ends_with(".props.recipe"))
    {
        let recipe = crate::engine::package::shared_props::read_recipe(&entry.bytes)?;
        // Inspect the stored revision: reusable v1 recipes keep their original
        // capability, while v2 must be refused by older hosts before fetching.
        needed |= recipe.version() >= minimum;
    }
    Ok(needed)
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

fn appearance_texture_paths(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    asset_root: &Path,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    let mut character_assets = crate::engine::props::PropAssets::with_root(asset_root);
    for texture in level
        .props
        .iter()
        .filter_map(|prop| prop.character_texture.as_deref())
        .chain(
            level
                .spawn_templates
                .iter()
                .filter_map(|template| template.character_texture.as_deref()),
        )
    {
        let _validated_image = character_assets.character_texture(catalog.assets(), texture)?;
        let path = catalog
            .assets()
            .texture_path(texture)
            .ok_or_else(|| format!("Unknown character texture `{texture}`"))?;
        paths.push(path.to_owned());
    }
    for id in level
        .global_illuminators
        .iter()
        .filter_map(crate::engine::level::GlobalIlluminatorDef::visual_texture)
    {
        let path = catalog
            .assets()
            .texture_path(id)
            .ok_or_else(|| format!("Missing celestial texture `{id}`"))?;
        let _image = crate::engine::materials::load_celestial_png_relative(asset_root, path)?;
        paths.push(path.to_owned());
    }
    Ok(paths)
}

fn collect_dependencies(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    asset_root: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<PackageDependency>, String> {
    let mut identities: BTreeMap<(String, String), PackageDependency> = BTreeMap::new();
    let mut add = |kind: DependencyKind, path: &str| -> Result<(), String> {
        let key = (dependency_kind_name(kind).to_string(), path.to_string());
        if identities.contains_key(&key) {
            return Ok(());
        }
        let (bytes, sha256) =
            match crate::engine::package::dependencies::identity(asset_root, path, kind) {
                Ok(identity) => identity,
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
                sha256,
                bytes,
            },
        ));
        Ok(())
    };

    // All templates are eligible for a spawn action, including an override
    // different from a point's default. The runtime uses the placement lookup
    // for both routes. GLBs embed their PNGs and reject external images, so one
    // model identity covers its complete runtime texture/mesh closure.
    for model_id in level
        .props
        .iter()
        .map(|prop| &prop.model)
        .chain(level.spawn_templates.iter().map(|template| &template.model))
    {
        if let Some(model) = catalog.get(model_id).model {
            add(DependencyKind::Model, &model)?;
        }
    }
    for path in appearance_texture_paths(level, catalog, asset_root)? {
        add(DependencyKind::Texture, &path)?;
    }
    let mut material_ids = crate::engine::materials::referenced_material_ids(level);
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
            material.and_then(|entry| entry.uv_response_mask.as_deref()),
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
        if let Some(model_id) = crate::engine::lighting::fixture_model_id(&light.fixture)
            && let Some(path) = catalog.get(model_id).model
        {
            add(DependencyKind::Model, &path)?;
        }
        if let Some(path) = catalog.assets().fixture_sheet_path(&light.fixture) {
            add(DependencyKind::Texture, path)?;
        }
    }
    for decal in &level.decals {
        if let Some(entry) = catalog.assets().get(&decal.material)
            && entry.asset_type.as_str() == crate::engine::assets::AssetType::DECAL
            && let Some(path) = entry.model.as_deref()
        {
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
) -> Result<(Vec<u8>, crate::map_compiler::navigation::NavBakeReport), String> {
    use crate::engine::level::ComponentDef;
    let collision = CompiledCollision::from_level(level);
    let doors = crate::engine::door::Doors::from_level(level);
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    let mut classes = vec![crate::engine::nav::reference_class()];
    let register_profile =
        |registered_classes: &mut Vec<crate::engine::package::navigation::NavClass>,
         class: crate::engine::package::navigation::NavClass|
         -> Result<(), String> {
            if registered_classes
                .iter()
                .any(|existing| existing.matches(&class))
            {
                return Ok(());
            }
            if registered_classes.len() >= crate::engine::package::MAX_NAV_CLASSES {
                return Err(format!(
                    "level authors more than {} distinct nav_agent bodies; the navigation record \
                 supports at most that many baked classes",
                    crate::engine::package::MAX_NAV_CLASSES
                ));
            }
            registered_classes.push(class);
            Ok(())
        };
    let profile_class =
        |component: &ComponentDef| -> Option<crate::engine::package::navigation::NavClass> {
            if let ComponentDef::NavAgent {
                radius,
                height,
                step_height,
                max_slope,
                speed_mps: _,
            } = component
            {
                return crate::engine::package::navigation::NavClass::new(
                    *radius,
                    *height,
                    *step_height,
                    *max_slope,
                );
            }
            None
        };
    let profile_of = |component: &ComponentDef| -> Option<crate::engine::nav::NavAgentProfile> {
        if let ComponentDef::NavAgent {
            radius,
            height,
            step_height,
            max_slope,
            speed_mps: _,
        } = component
        {
            return Some(crate::engine::nav::NavAgentProfile {
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
    let mut obstacles: Vec<crate::engine::collision::WallAabb> = Vec::new();
    for prop in &level.props {
        for component in &prop.components {
            let ComponentDef::NavObstacle { size, affects_nav } = component else {
                continue;
            };
            if !*affects_nav {
                continue;
            }
            // Explicit obstacle dimensions have historically been world metres;
            // the default follows the prop's scale. Undo that scale only for
            // the explicit box before applying the shared placement matrix.
            let body_size = size.map_or_else(
                || {
                    prop.size
                        .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE)
                },
                |dimensions| dimensions.map(|value| value / prop.scale),
            );
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let bounds = prop.placement_bounds(base_y, body_size);
            obstacles.push(crate::engine::collision::WallAabb::with_y(
                bounds.min[0],
                bounds.min[1],
                bounds.min[2],
                bounds.max[0] - bounds.min[0],
                bounds.max[1] - bounds.min[1],
                bounds.max[2] - bounds.min[2],
            ));
        }
    }
    let input = crate::map_compiler::navigation::NavBakeInput {
        level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &obstacles,
        walk_proxies: &[],
    };
    let options = crate::map_compiler::navigation::NavBakeOptions {
        cell_m: crate::map_compiler::navigation::DEFAULT_NAV_CELL_M,
        workers,
    };
    let (grid, report) = crate::map_compiler::navigation::bake(&input, &options)?;
    if report.walkable_cells.first().copied().unwrap_or(0) == 0 {
        warnings.push(format!("{} navigation baked no walkable cells", level.id));
    }
    // Placement diagnostics: every actor body and spawn point must have a
    // navigable cell, or the encounter that uses it can never move.
    if let Ok(mesh) = crate::engine::nav::NavMesh::from_record(grid.clone()) {
        let no_doors = crate::engine::nav::NoDoors;
        let check =
            |what: &str, profile: &crate::engine::nav::NavAgentProfile, position: glam::Vec3| {
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
    let bytes = crate::engine::package::navigation::write_navigation(&grid)?;
    Ok((bytes, report))
}

/// Parse exactly the catalogue bytes included in the build key, including
/// definitions that reference unchanged model/texture files.
fn load_catalog_identity(
    path: &Path,
) -> Result<(crate::engine::loader::PropCatalog, String), String> {
    let bytes = crate::engine::project::read_bounded_file(path, 8 * 1024 * 1024)
        .map_err(|error| format!("could not read catalog {}: {error}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("catalog {} is not valid UTF-8: {error}", path.display()))?;
    let catalog = crate::engine::loader::PropCatalog::from_json_str(text)
        .map_err(|error| format!("catalog {} is invalid: {error}", path.display()))?;
    Ok((catalog, sha256_hex(&bytes)))
}

fn catalog_inputs(
    level: &LevelDef,
    path: &Path,
) -> Result<crate::engine::package::catalog_inputs::CatalogInputs, String> {
    let bytes = crate::engine::project::read_bounded_file(path, 8 * 1024 * 1024)?;
    crate::engine::package::catalog_inputs::CatalogInputs::for_level(level, &bytes)
}

/// Versioned catalogue identity surrounds both complete and reusable stage
/// keys. No map record or package schema changes; old keys miss safely once.
fn fingerprint_with_catalog(input: &str, catalog_hash: &str) -> String {
    // Input-key revision 1 belongs to the current pre-release schema.
    sha256_hex(format!("asset_inputs_v1\ninput {input}\ncatalog {catalog_hash}\n").as_bytes())
}

/// Hash the installed tool once, independent of checkout paths or source access.
fn compiler_tool_sha256() -> Result<&'static str, String> {
    static IDENTITY: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            use sha2::Digest as _;
            use std::io::Read as _;
            let executable = std::env::current_exe()
                .map_err(|error| format!("could not locate compiler executable: {error}"))?;
            let mut reader = std::fs::File::open(&executable)
                .map_err(|error| format!("could not open compiler executable: {error}"))?;
            let mut hasher = sha2::Sha256::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let count = reader
                    .read(&mut buffer)
                    .map_err(|error| format!("could not hash compiler executable: {error}"))?;
                if count == 0 {
                    break;
                }
                hasher.update(
                    buffer
                        .get(..count)
                        .ok_or_else(|| "compiler hash read exceeded its buffer".to_string())?,
                );
            }
            Ok(crate::engine::package::hash::digest_hex(&hasher.finalize()))
        })
        .as_ref()
        .map(String::as_str)
        .map_err(Clone::clone)
}

fn fingerprint_with_build_inputs(input: &str, inputs: &BuildInputs) -> String {
    let catalog_identity = inputs
        .catalog_inputs
        .as_ref()
        .and_then(|closure| closure.identity().ok())
        .unwrap_or_else(|| inputs.catalog_sha256.clone());
    let catalogued = fingerprint_with_catalog(input, &catalog_identity);
    sha256_hex(
        format!(
            "compiler_inputs_v{}\ninput {catalogued}\ntool {}\ncapture_probes {}\n",
            inputs.revision,
            inputs
                .compiler_code_sha256
                .as_deref()
                .unwrap_or(&inputs.tool_sha256),
            inputs.capture_probes
        )
        .as_bytes(),
    )
}

/// Celestial discs are background-only: their presence, image bindings and
/// exposure never enter captured probes or the illumination solve. Keep every
/// real source property and any image also referenced by world materials.
fn lighting_stage_fingerprint(
    level: &LevelDef,
    variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
    inputs: &BuildInputs,
    catalog: &crate::engine::loader::PropCatalog,
    catalog_path: &Path,
    asset_root: &Path,
) -> Result<String, String> {
    if !level
        .global_illuminators
        .iter()
        .any(|light| light.visual.is_some())
    {
        return Ok(fingerprint_with_build_inputs(
            &lighting_fingerprint(level, variants, dependencies)?,
            inputs,
        ));
    }
    let mut lighting_level = level.clone();
    for light in &mut lighting_level.global_illuminators {
        light.visual = None;
    }
    let mut lighting_inputs = inputs.clone();
    lighting_inputs.catalog_inputs = Some(catalog_inputs(&lighting_level, catalog_path)?);
    // The complete dependency pass already reports fallback warnings. Resolve
    // the same subset normally so a disc image reused by a wall remains a real
    // bake dependency rather than filtering its filename speculatively.
    let mut subset_warnings = Vec::new();
    let lighting_dependencies =
        collect_dependencies(&lighting_level, catalog, asset_root, &mut subset_warnings)?;
    Ok(fingerprint_with_build_inputs(
        &lighting_fingerprint(&lighting_level, variants, &lighting_dependencies)?,
        &lighting_inputs,
    ))
}

/// The illumination/geometry stage fingerprint.
///
/// It folds everything the prepared static world depends on — the level with
/// the navigation and AI components removed, the dependency identities, the
/// record versions, the lighting model and the transport solver, and
/// each quality's implementation (through preparation source identity). Requested
/// variant selection/order is excluded. Display name/author, navigation/AI components and final
/// presentation are excluded; the outer key additionally hashes the exact
/// referenced catalog closure, preparation source and reflection capture mode. An edit to the
/// excluded fields reuses the previous geometry, lightmaps, probes and collision.
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
    _variants: &[LightmapQuality],
    dependencies: &[PackageDependency],
    geometry_revision: u32,
) -> Result<String, String> {
    use std::fmt::Write as _;
    let mut stripped = level.clone();
    // Display metadata is encoded anew in semantics and the manifest. It is
    // never read by static geometry, collision, illumination or probe baking.
    stripped.acoustic_materials.clear();
    for room in &mut stripped.rooms {
        room.acoustics = crate::engine::audio::acoustics::RoomAcoustics::default();
    }
    stripped.name.clear();
    stripped.author.clear();
    stripped.requires_full_lighting = false;
    for light in &mut stripped.global_illuminators {
        light.visual = None;
    }
    // Luau runs only after load; these bytes cannot change prepared geometry,
    // lighting, collision or probes. They remain in the full package identity.
    stripped.scripts.clear();
    stripped.script_sources.clear();
    strip_navigation_components(&mut stripped);
    // Motion is applied only by the live renderer/controller and does not
    // change the mean-plane mesh, collision, transport or baked probes.
    for water in &mut stripped.water {
        water.motion = crate::engine::level::WaterMotionDef::default();
    }
    // Exposure, shoulder and display grade are applied after linear-HDR probe
    // capture. Fog remains in the stage key: it is present in captured cubes.
    let mut environment = stripped.environment.unwrap_or_default();
    environment.presentation = crate::engine::environment::PresentationDef::default();
    stripped.environment = Some(environment);
    let bytes = crate::engine::canonical_json::canonical_json_bytes(&stripped)
        .map_err(|error| format!("could not serialize the lighting stage input: {error}"))?;
    let mut canonical = String::with_capacity(1024);
    canonical.push_str(COMPILER_NAME);
    canonical.push('\n');
    let _formatted_format_format = writeln!(
        canonical,
        "schema {} format {FORMAT_VERSION}",
        crate::engine::package::PACKAGE_SCHEMA
    );
    let _formatted_level_format = writeln!(
        canonical,
        "level_format {}",
        crate::engine::level::LEVEL_FORMAT_VERSION
    );
    // Explicit stage-key version: older packages miss once, then reuse safely.
    let _formatted_stage_lighting = writeln!(
        canonical,
        "stage lighting v2 independent_quality navigation_display_metadata_presentation_scripts_excluded"
    );
    // This reusable stage also stores collision boxes, including region rims.
    let _formatted_rim_backing = writeln!(
        canonical,
        "rim_backing {}",
        crate::engine::level::RIM_BACKING.to_bits()
    );
    let _formatted_records_mesh = writeln!(
        canonical,
        "records mesh {} props {} collision {} lighting {} probes {} lightmaps {}",
        crate::engine::package::mesh::MESH_RECORD_VERSION,
        crate::engine::package::props::PROPS_RECORD_VERSION,
        crate::engine::package::collision::COLLISION_RECORD_VERSION,
        crate::engine::package::lighting::LIGHTING_RECORD_VERSION,
        crate::engine::package::world::PROBE_POSITIONS_VERSION,
        crate::engine::package::lightmaps::LIGHTMAPS_RECORD_VERSION,
    );
    let _formatted_solver_model = writeln!(
        canonical,
        "solver {} model {}",
        crate::engine::lighting::transport::solver_fingerprint(),
        crate::engine::lighting::model_fingerprint()
    );
    // The prepared geometry is a function of the emitter as well as of the
    // source, and the lightmaps are baked against that geometry. A geometry
    // revision therefore invalidates both stages together instead of letting
    // a stale mesh and its old lightmaps be reused.
    let _formatted_geometry_geometry = writeln!(canonical, "geometry {geometry_revision}");
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
        crate::engine::package::hash::sha256_hex(&bytes)
    );
    Ok(crate::engine::package::hash::sha256_hex(
        canonical.as_bytes(),
    ))
}

/// Removes every navigation/AI component from a level copy.
fn strip_navigation_components(level: &mut LevelDef) {
    use crate::engine::level::ComponentDef;
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
/// verifies; a named error when the previous stage cannot safely be reused.
fn reuse_prepared_lighting(
    out: &Path,
    lighting_fingerprint: &str,
    level: &LevelDef,
    requested: &[LightmapQuality],
    expected_inputs: &BuildInputs,
    warnings: &mut Vec<String>,
) -> Result<(Vec<Variant>, BlobMap), String> {
    if !out.exists() {
        return Err("no previous package".to_string());
    }
    let mut reader = open_package(out)?;
    crate::engine::package::project::select(&mut reader, Some(&level.id))?;
    let names = reader.names().to_vec();
    let manifest_bytes =
        reader.read_entry("manifest.json", crate::engine::package::MAX_MANIFEST_BYTES)?;
    let mut manifest = Manifest::from_json(&manifest_bytes, &names)?;
    let previous_inputs = read_build_inputs(out, &manifest)?;
    if !preparation_inputs_match(&previous_inputs, expected_inputs) {
        return Err("compiler preparation provenance or capture mode changed".into());
    }
    if manifest.lighting_fingerprint.as_deref() != Some(lighting_fingerprint) {
        return Err("prepared geometry, illumination, assets or tool identity changed".to_string());
    }
    manifest.variants.retain(|variant| {
        requested
            .iter()
            .any(|quality| quality.name() == variant.lightmap_quality)
    });
    if manifest.variants.is_empty() {
        return Err("no requested quality is available in the verified prepared stage".into());
    }
    let mut needed = BTreeSet::new();
    let mut shared_names = Vec::new();
    for variant in &manifest.variants {
        let entries = &variant.entries;
        if entries.static_light_off.is_some() {
            let lighting = crate::engine::package::lighting::read_lighting(&reader.read_blob(
                &entries.lighting,
                crate::engine::package::MAX_LIGHTING_BYTES,
            )?)?;
            let props = crate::engine::package::props::read_props(
                &crate::engine::package::shared_props::read_record(&mut reader, &entries.props)?,
            )?;
            let models = props.into_iter().map(|batch| batch.model).collect();
            validate_static_light_off(&mut reader, variant, &lighting, &models)?;
        }
        needed.extend([
            entries.mesh.as_str(),
            entries.props.as_str(),
            entries.lighting.as_str(),
            entries.collision.as_str(),
        ]);
        if let Some(state) = &entries.static_light_off {
            needed.extend([state.mesh.as_str(), state.props.as_str()]);
        }
        for props in std::iter::once(entries.props.as_str()).chain(
            entries
                .static_light_off
                .as_ref()
                .map(|state| state.props.as_str()),
        ) {
            let prop_record = reader.read_blob(props, crate::engine::package::MAX_BINARY_BYTES)?;
            if prop_record.starts_with(b"PPS1") {
                let recipe = crate::engine::package::shared_props::read_recipe(&prop_record)?;
                for segment in recipe.segments() {
                    shared_names.push(segment.blob_name());
                }
            }
        }
        needed.extend(entries.lightmaps.as_deref());
        needed.extend(entries.lightmaps_meta.as_deref());
        needed.extend(entries.irradiance.as_deref());
        for probe in &entries.probes {
            let _new_entry = needed.insert(probe.positions.as_str());
            needed.extend(probe.cubemaps.iter().map(String::as_str));
        }
    }
    needed.extend(shared_names.iter().map(String::as_str));
    let mut blobs: BlobMap = BTreeMap::new();
    for entry in &manifest.entries {
        let bytes = read_declared_entry(&mut reader, entry)?;
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
        return Err("previous package has a missing prepared record".to_string());
    }
    let mut variants = manifest.variants;
    refresh_reused_atlas_keys(level, &mut variants, &mut blobs)
        .ok_or_else(|| "previous lightmap metadata could not be refreshed".to_string())?;
    warnings.push(format!(
        "reused prepared geometry, lighting, probes and collision from {} (lighting stage fingerprint \
         unchanged; semantics, navigation, input provenance and atlas metadata refreshed)",
        out.display()
    ));
    Ok((variants, blobs))
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
        let lighting = crate::engine::package::lighting::read_lighting(
            &blobs.get(&variant.entries.lighting)?.0,
        )
        .ok()?;
        let mut meta: crate::engine::package::lightmaps::LightmapsMeta =
            serde_json::from_slice(&blobs.get(&previous)?.0).ok()?;
        meta.content_key = crate::engine::render::lightmap_content_key(
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
    let _formatted_format_format = writeln!(
        canonical,
        "schema {} format {FORMAT_VERSION}",
        crate::engine::package::PACKAGE_SCHEMA
    );
    // The semantics record is the authored level itself; a schema revision
    // changes what a package means even when nothing else moved, so the level
    // format version is part of the build identity.
    let _formatted_level_format = writeln!(
        canonical,
        "level_format {}",
        crate::engine::level::LEVEL_FORMAT_VERSION
    );
    let _formatted_source_source = writeln!(canonical, "source {source_sha256}");
    // Collision and navigation are prepared offline. A rim-width change must
    // invalidate packages even when their authored source and mesh are unchanged.
    let _formatted_rim_backing = writeln!(
        canonical,
        "rim_backing {}",
        crate::engine::level::RIM_BACKING.to_bits()
    );
    let _formatted_records_mesh = writeln!(
        canonical,
        "records mesh {} props {} collision {} lighting {} probes {} navigation {}",
        crate::engine::package::mesh::MESH_RECORD_VERSION,
        crate::engine::package::props::PROPS_RECORD_VERSION,
        crate::engine::package::collision::COLLISION_RECORD_VERSION,
        crate::engine::package::lighting::LIGHTING_RECORD_VERSION,
        crate::engine::package::world::PROBE_POSITIONS_VERSION,
        crate::engine::package::navigation::NAVIGATION_RECORD_VERSION,
    );
    // The prepared data is a function of the lighting model and the transport
    // solver as well as of the source. Folding both revisions in means a
    // recalibrated model or a moved solver invalidates an existing package
    // instead of silently reusing its captures.
    let _formatted_solver_model = writeln!(
        canonical,
        "solver {} model {}",
        crate::engine::lighting::transport::solver_fingerprint(),
        crate::engine::lighting::model_fingerprint()
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

fn reuse_current(
    out: &Path,
    fingerprint: &str,
    inputs: &BuildInputs,
    dependencies: &[PackageDependency],
    variants: &[LightmapQuality],
) -> Result<String, Vec<String>> {
    if !out.exists() {
        return Err(vec!["no previous package".to_string()]);
    }
    let manifest = inspect(out).map_err(|error| vec![error])?;
    let differences =
        package_input_differences(out, &manifest, inputs, dependencies, variants, fingerprint);
    if !differences.is_empty() {
        return Err(differences);
    }
    // A fingerprint match is not enough: a package whose declared entry hashes
    // no longer match its bytes (a corrupt or half-written artifact) must be
    // rebuilt, never reused.
    match verify_declared_entries(out, &manifest) {
        Ok(()) => Ok(format!(
            "{} is current for this source and asset identity",
            out.display()
        )),
        Err(error) => {
            crate::engine::logging::warn(format!(
                "[compiler] rebuilding {}: {error}",
                out.display()
            ));
            Err(vec![format!("package integrity failed: {error}")])
        }
    }
}

fn preparation_inputs_match(previous: &BuildInputs, current: &BuildInputs) -> bool {
    previous.revision == current.revision
        && previous.compiler_code_sha256 == current.compiler_code_sha256
        && (previous.compiler_code_sha256.is_some() || previous.tool_sha256 == current.tool_sha256)
        && previous.capture_probes == current.capture_probes
}

fn read_build_inputs(out: &Path, manifest: &Manifest) -> Result<BuildInputs, String> {
    let entry = manifest
        .entries
        .iter()
        .find(|entry| entry.name == BUILD_INPUTS_ENTRY)
        .ok_or_else(|| {
            "compiler input provenance is absent (one full rebuild required)".to_string()
        })?;
    if entry.role != "compiler-inputs" {
        return Err("compiler input provenance has an invalid declared role".to_string());
    }
    if entry.bytes > crate::engine::package::MAX_MANIFEST_BYTES {
        return Err("compiler input provenance exceeds the manifest byte limit".to_string());
    }
    let mut reader = open_package(out)?;
    crate::engine::package::project::select(&mut reader, Some(&manifest.id))?;
    let bytes = read_declared_entry(&mut reader, entry)?;
    let inputs: BuildInputs = serde_json::from_slice(&bytes)
        .map_err(|error| format!("compiler input provenance is invalid: {error}"))?;
    if !matches!(inputs.revision, 1 | 2) {
        return Err("compiler input provenance revision is unsupported".to_string());
    }
    Ok(inputs)
}

fn package_input_differences(
    out: &Path,
    manifest: &Manifest,
    inputs: &BuildInputs,
    dependencies: &[PackageDependency],
    variants: &[LightmapQuality],
    fingerprint: &str,
) -> Vec<String> {
    let mut differences = Vec::new();
    match read_build_inputs(out, manifest) {
        Ok(previous) => {
            for (changed, reason) in [
                (
                    previous.revision != inputs.revision,
                    "compiler input contract changed",
                ),
                (
                    previous.source_sha256 != inputs.source_sha256,
                    "source bytes changed",
                ),
                (
                    previous.catalog_inputs != inputs.catalog_inputs,
                    "referenced catalog definitions changed",
                ),
                (
                    previous.compiler_code_sha256 != inputs.compiler_code_sha256,
                    "compiler preparation code changed",
                ),
                (
                    previous.capture_probes != inputs.capture_probes,
                    "reflection capture mode changed",
                ),
            ] {
                if changed {
                    differences.push(reason.to_string());
                }
            }
        }
        Err(error) => differences.push(error),
    }
    differences.extend(dependency_differences(&manifest.dependencies, dependencies));
    let current_variants: Vec<_> = variants.iter().map(|quality| quality.name()).collect();
    let previous_variants: Vec<_> = manifest
        .variants
        .iter()
        .map(|variant| variant.lightmap_quality.as_str())
        .collect();
    if current_variants != previous_variants {
        differences.push("requested lightmap variants changed".to_string());
    }
    if fingerprint != manifest.compiler_fingerprint && differences.is_empty() {
        differences
            .push("compiler, solver, geometry or record-format identity changed".to_string());
    }
    differences
}

fn dependency_differences(
    previous: &[PackageDependency],
    current: &[PackageDependency],
) -> Vec<String> {
    let index = |dependencies: &[PackageDependency]| -> BTreeMap<(String, String), (String, u64)> {
        dependencies
            .iter()
            .map(|dependency| {
                (
                    (
                        dependency_kind_name(dependency.kind).to_string(),
                        dependency.path.clone(),
                    ),
                    (dependency.sha256.clone(), dependency.bytes),
                )
            })
            .collect()
    };
    let previous_index = index(previous);
    let current_index = index(current);
    let mut differences = Vec::new();
    for ((kind, path), identity) in &current_index {
        match previous_index.get(&(kind.clone(), path.clone())) {
            Some(old_identity) if old_identity != identity => {
                differences.push(format!("dependency {kind} '{path}' changed"));
            }
            None => differences.push(format!("dependency {kind} '{path}' added")),
            Some(_) => {}
        }
    }
    for (kind, path) in previous_index.keys() {
        if !current_index.contains_key(&(kind.clone(), path.clone())) {
            differences.push(format!("dependency {kind} '{path}' removed"));
        }
    }
    differences
}

/// Re-reads every declared entry and compares its bytes with the manifest and
/// blob-name hashes.
fn verify_declared_entries(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let mut reader = open_package(path)?;
    crate::engine::package::project::select(&mut reader, Some(&manifest.id))?;
    for entry in &manifest.entries {
        drop(read_declared_entry(&mut reader, entry)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

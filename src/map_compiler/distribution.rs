//! Complete project orchestration through the existing ordinary map compiler.
use std::collections::BTreeMap;
use std::fs::{self, File};

use crate::engine::package::{PackageReader, archive, hash, project};
use crate::engine::project::ProjectFolder;

use super::project_spool::ProjectSpool;
use super::{BuildReport, BuildRequest, progress};

pub(super) fn operation_report(request: &BuildRequest) -> BuildReport {
    BuildReport {
        source: request.source.display().to_string(),
        out: request.out.display().to_string(),
        rebuilt: true,
        reuse_reason: String::new(),
        variants: Vec::new(),
        bytes: 0,
        millis: 0.0,
        warnings: Vec::new(),
        fingerprint: String::new(),
        variant_stats: Vec::new(),
        phases: Vec::new(),
        cache_decisions: Vec::new(),
    }
}

/// Builds all levels, deduplicates records and atomically publishes one project package.
/// # Errors
/// Returns project/build errors, unsafe output conflicts or cancellation.
pub fn export(request: &BuildRequest) -> Result<BuildReport, String> {
    crate::engine::paths::configure_assets(&request.asset_root)?;
    let scope = progress::BuildScope::start(&request.source, Some("project export"));
    let result = export_inner(request);
    scope.finish(&result);
    result
}

fn export_inner(request: &BuildRequest) -> Result<BuildReport, String> {
    let started = std::time::Instant::now();
    let folder = ProjectFolder::open(&request.source)?;
    let publication_guard = super::publication::Guard::new(&request.source, &request.out)?;
    check_destination(&request.out, &folder.manifest().id)?;
    progress::stage("validating complete project sources");
    let sources = project::snapshot(&folder, &progress::check_cancelled)?;
    publication_guard.check()?;
    if !request.force && request.out.exists() && project_is_current(request, &folder, &sources)? {
        publication_guard.check()?;
        return current_project_report(request, started);
    }
    let staging = archive::OwnedStaging::reserve(&request.out)?;
    let result = (|| {
        let staged_source = stage_sources(&staging, &sources)?;
        let mut blobs = ProjectSpool::new_owned(&staging)?;
        let mut index = stage_project_index(&folder, &sources, &mut blobs)?;
        let mut last_report = None;
        let mut warnings = Vec::new();
        let mut cache_decisions = Vec::new();
        for level in &folder.manifest().levels {
            progress::check_cancelled()?;
            #[cfg(unix)]
            staging.check()?;
            if !request.force && member_current(request, &staged_source, &level.id) {
                #[cfg(unix)]
                staging.check()?;
                index
                    .levels
                    .push(spool_current(request, &level.id, &mut blobs)?);
                cache_decisions.push(package_cache_hit(
                    &level.id,
                    "Verified current member compressed records reused; no seed/recompression or bake",
                ));
                last_report = Some(operation_report(request));
                continue;
            }
            #[cfg(unix)]
            if !request.force
                && let Some(mut sibling) = super::project_cache::stage_current_sibling(
                    request,
                    &folder,
                    level,
                    &staged_source,
                    &staging,
                )?
            {
                staging.check()?;
                index
                    .levels
                    .push(spool_records(&mut sibling.reader, &level.id, &mut blobs)?);
                warnings.extend(sibling.warnings);
                cache_decisions.push(package_cache_hit(
                    &level.id,
                    "Verified current Final Build sibling reused",
                ));
                let mut report = operation_report(request);
                report.variants = request
                    .variants
                    .iter()
                    .map(|quality| quality.name().into())
                    .collect();
                last_report = Some(report);
                continue;
            }
            let built = staging.with_path(|root| {
                let output = root.join(format!("{}.placesmap", level.id));
                build_staged(request, &staged_source, &output, &level.id)
            })?;
            warnings.extend(built.warnings.iter().cloned());
            cache_decisions.extend(built.cache_decisions.iter().cloned().map(|mut decision| {
                decision.stage = format!("{}/{}", level.id, decision.stage);
                decision
            }));
            last_report = Some(built);
            #[cfg(unix)]
            staging.check()?;
            index
                .levels
                .push(spool_map(request, &staging, &level.id, &mut blobs)?);
        }
        let publication = progress::BuildScope::start(&request.source, Some("project export"));
        progress::stage("checking source snapshot and publishing project");
        let mut report = last_report.ok_or("Project has no levels")?;
        report.warnings = warnings;
        report.cache_decisions = cache_decisions;
        staging.with_path(|root| {
            let check = || staging.with_path(|_| publication_guard.check());
            let outcome = publish(request, &index, blobs, root, report, check, started);
            publication.finish(&outcome);
            outcome
        })
    })();
    finish_staging(result, &staging)
}

fn current_project_report(
    request: &BuildRequest,
    started: std::time::Instant,
) -> Result<BuildReport, String> {
    let mut report = operation_report(request);
    report.rebuilt = false;
    report.reuse_reason =
        "Complete project sources, level dependencies and all records verified".into();
    report.bytes = fs::metadata(&request.out)
        .map_err(|error| error.to_string())?
        .len();
    report.millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
    Ok(report)
}

fn stage_project_index(
    folder: &ProjectFolder,
    sources: &BTreeMap<crate::engine::project::ProjectPath, Vec<u8>>,
    blobs: &mut ProjectSpool,
) -> Result<project::ProjectPackage, String> {
    let mut index = project::ProjectPackage {
        places_version: crate::engine::package::release::PLACES_VERSION.into(),
        schema: "places-project-package".into(),
        package_format: project::PROJECT_FORMAT_VERSION,
        project: folder.manifest().clone(),
        levels: Vec::new(),
        sources: BTreeMap::new(),
    };
    for (path, bytes) in sources {
        let name = blobs.add_bytes(bytes)?;
        let _old = index.sources.insert(path.clone(), name);
    }
    Ok(index)
}

fn package_cache_hit(id: &str, reason: &str) -> super::CacheDecision {
    super::CacheDecision {
        stage: format!("{id}/package"),
        status: "hit".into(),
        reasons: vec![reason.into()],
    }
}

fn finish_staging(
    result: Result<BuildReport, String>,
    staging: &archive::OwnedStaging,
) -> Result<BuildReport, String> {
    let cleanup = staging.cleanup();
    match (result, cleanup) {
        (Ok(mut report), Err(error)) => {
            report.warnings.push(format!(
                "Export published; staging cleanup {}: {error}",
                staging.display()
            ));
            Ok(report)
        }
        (Err(error), Err(cleanup_error)) => Err(format!(
            "{error}; staging cleanup {}: {cleanup_error}",
            staging.display()
        )),
        (outcome, Ok(())) => outcome,
    }
}

fn spool_current(
    request: &BuildRequest,
    id: &str,
    blobs: &mut ProjectSpool,
) -> Result<project::ProjectMap, String> {
    let mut reader = super::open_package(&request.out)?;
    project::select(&mut reader, Some(id))?;
    let manifest = super::read_manifest(&mut reader)?;
    super::check_cached_lighting(&mut reader, &manifest)?;
    spool_records(&mut reader, id, blobs)
}

fn spool_records(
    reader: &mut PackageReader<File>,
    id: &str,
    blobs: &mut ProjectSpool,
) -> Result<project::ProjectMap, String> {
    let mut records = BTreeMap::new();
    for name in reader.names().to_vec() {
        let physical = blobs.add_compressed_entry(reader, &name, &progress::check_cancelled)?;
        drop(records.insert(name, physical));
    }
    Ok(project::ProjectMap {
        id: id.into(),
        records,
    })
}

fn member_current(request: &BuildRequest, source: &std::path::Path, id: &str) -> bool {
    super::verify_selected(source, Some(id), &request.out, &request.asset_root)
        .is_ok_and(|report| report.current)
        && crate::engine::package::world::inspect_level(&request.out, id).is_ok_and(|manifest| {
            if request.compact
                && !super::read_build_inputs(&request.out, &manifest)
                    .is_ok_and(|inputs| inputs.compact)
            {
                return false;
            }
            manifest
                .variants
                .iter()
                .map(|variant| variant.lightmap_quality.as_str())
                .eq(request.variants.iter().map(|quality| quality.name()))
        })
}

fn project_is_current(
    request: &BuildRequest,
    folder: &ProjectFolder,
    sources: &BTreeMap<crate::engine::project::ProjectPath, Vec<u8>>,
) -> Result<bool, String> {
    let mut reader = super::open_package(&request.out)?;
    let Some(index) = project::read_index(&mut reader)? else {
        return Ok(false);
    };
    if index.project != *folder.manifest() || index.sources.len() != sources.len() {
        return Ok(false);
    }
    for (path, bytes) in sources {
        let Some(record) = index.sources.get(path) else {
            return Ok(false);
        };
        if reader.read_blob(record, project::MAX_SOURCE_BYTES)? != *bytes {
            return Ok(false);
        }
    }
    Ok(index
        .levels
        .iter()
        .all(|level| member_current(request, &request.source, &level.id)))
}

fn spool_map(
    request: &BuildRequest,
    staging: &archive::OwnedStaging,
    id: &str,
    spool: &mut ProjectSpool,
) -> Result<project::ProjectMap, String> {
    let scope = progress::BuildScope::start(&request.source, Some("project spool"));
    let result = (|| {
        progress::stage(format!(
            "streaming {id} prepared records to owned disk staging"
        ));
        let name = format!("{id}.placesmap");
        let mut reader = PackageReader::new(staging.take_member(std::ffi::OsStr::new(&name))?)
            .map_err(|error| error.to_string())?;
        let member = spool_records(&mut reader, id, spool)?;
        #[cfg(not(unix))]
        {
            drop(reader);
            staging.with_path(|root| {
                fs::remove_file(root.join(name)).map_err(|error| error.to_string())
            })?;
        }
        progress::stage(format!(
            "spooled {id}: {} unique records, {} record bytes",
            spool.len(),
            spool.total()
        ));
        Ok(member)
    })();
    scope.finish(
        &result
            .as_ref()
            .map(|_| operation_report(request))
            .map_err(Clone::clone),
    );
    result
}

fn build_staged(
    request: &BuildRequest,
    source: &std::path::Path,
    out: &std::path::Path,
    level: &str,
) -> Result<BuildReport, String> {
    let mut selected = request.clone();
    selected.source = source.into();
    selected.project_level = Some(level.into());
    selected.out = out.into();
    let rejected = if request.force {
        None
    } else {
        super::project_cache::seed(&request.out, level, out).err()
    };
    let mut report = super::build(&selected)?;
    if let Some(error) = rejected {
        report.warnings.push(format!(
            "Previous prepared cache for '{level}' was rejected: {error}"
        ));
    }
    Ok(report)
}

fn check_destination(output: &std::path::Path, id: &str) -> Result<(), String> {
    if output.extension().is_none_or(|e| e != "placesmap") {
        return Err("Project export requires a .placesmap output".into());
    }
    if output.symlink_metadata().is_ok() {
        let metadata = output.symlink_metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Export refuses nonregular/symlink output".into());
        }
        let previous = project::inspect(output)?
            .ok_or("Export would overwrite an ordinary map; choose another filename")?;
        if previous.project.id != id {
            return Err("Export would overwrite another project; choose another filename".into());
        }
    }
    Ok(())
}

pub(super) fn package_entries(
    index: &project::ProjectPackage,
    blobs: ProjectSpool,
    temporary: &std::path::Path,
) -> Result<(Vec<archive::SpoolEntry>, String), String> {
    let index_bytes = serde_json::to_vec(index).map_err(|e| e.to_string())?;
    if u64::try_from(index_bytes.len()).unwrap_or(u64::MAX)
        > crate::engine::package::MAX_MANIFEST_BYTES
    {
        return Err("Project index exceeds 2 MiB".into());
    }
    let fingerprint = hash::sha256_hex(&index_bytes);
    let index_path = temporary.join("project.json");
    let mut index_file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&index_path)
        .map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut index_file, &index_bytes).map_err(|error| error.to_string())?;
    drop(index_file);
    let mut entries = blobs.into_entries();
    entries.push(archive::SpoolEntry {
        compressed: None,
        name: project::INDEX.into(),
        path: index_path,
        bytes: u64::try_from(index_bytes.len()).map_err(|error| error.to_string())?,
        sha256: fingerprint.clone(),
    });
    Ok((entries, fingerprint))
}

fn stage_sources(
    staging: &archive::OwnedStaging,
    sources: &BTreeMap<crate::engine::project::ProjectPath, Vec<u8>>,
) -> Result<std::path::PathBuf, String> {
    use std::io::Write as _;
    let root = std::path::Path::new("source");
    #[cfg(unix)]
    staging.create_directories(root)?;
    #[cfg(not(unix))]
    staging.with_path(|path| fs::create_dir(path.join(root)).map_err(|error| error.to_string()))?;
    for (path, bytes) in sources {
        progress::check_cancelled()?;
        let relative = root.join(path.as_str());
        staging.create_directories(relative.parent().ok_or("Staged source has no parent")?)?;
        let mut file = staging.with_path(|staging_root| {
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(staging_root.join(relative))
                .map_err(|error| error.to_string())
        })?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
    }
    staging.with_path(|path| Ok(path.join(root)))
}

fn publish(
    request: &BuildRequest,
    index: &project::ProjectPackage,
    blobs: ProjectSpool,
    temporary: &std::path::Path,
    mut report: BuildReport,
    publish_check: impl Fn() -> Result<(), String>,
    started: std::time::Instant,
) -> Result<BuildReport, String> {
    let (entries, fingerprint) = package_entries(index, blobs, temporary)?;
    if request.compact {
        archive::write_project_archive_files_with_level_guarded(
            &request.out,
            entries,
            progress::check_cancelled,
            publish_check,
            19,
        )?;
    } else {
        archive::write_project_archive_files_guarded(
            &request.out,
            entries,
            progress::check_cancelled,
            publish_check,
        )?;
    }
    report.source = request.source.display().to_string();
    report.out = request.out.display().to_string();
    report.bytes = fs::metadata(&request.out).map_err(|e| e.to_string())?.len();
    report.fingerprint = fingerprint;
    report.variant_stats.clear();
    report.phases.clear();
    report.rebuilt = true;
    report.millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
    Ok(report)
}

/// Recovers editable sources through the same observed compiler job boundary.
/// # Errors
/// Returns malformed input, destination conflicts or cooperative cancellation.
pub fn recover(request: &BuildRequest) -> Result<BuildReport, String> {
    crate::engine::paths::configure_assets(&request.asset_root)?;
    let scope = progress::BuildScope::start(&request.source, Some("source recovery"));
    let result = project::recover(&request.source, &request.out, &progress::check_cancelled)
        .map(|()| operation_report(request));
    scope.finish(&result);
    result
}

/// Validates supported source data without pretending a migration occurred.
/// # Errors
/// Returns unsupported schema, invalid source or cancellation errors.
pub fn check(request: &BuildRequest) -> Result<BuildReport, String> {
    crate::engine::paths::configure_assets(&request.asset_root)?;
    let scope = progress::BuildScope::start(&request.source, Some("project check"));
    let result = (|| {
        progress::stage("validating supported editable project data");
        let folder = ProjectFolder::open(&request.source)?;
        drop(project::snapshot(&folder, &progress::check_cancelled)?);
        Ok(operation_report(request))
    })();
    scope.finish(&result);
    result
}

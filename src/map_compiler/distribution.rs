//! Complete project orchestration through the existing ordinary map compiler.
use std::collections::BTreeMap;
use std::fs::{self, File};

use crate::engine::package::{PackageReader, archive, hash, project};
use crate::engine::project::ProjectFolder;

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
    check_destination(&request.out, &folder.manifest().id)?;
    let original_output = output_identity(&request.out)?;
    progress::stage("validating complete project sources");
    let sources = project::snapshot(&folder, &progress::check_cancelled)?;
    let temporary = archive::create_temporary_directory(&request.out)?;
    let result = (|| {
        let staged_source = stage_sources(&temporary, &sources)?;
        let mut blobs = BTreeMap::new();
        let mut total = 0_u64;
        let mut index = project::ProjectPackage {
            schema: "places-project-package".into(),
            package_format: 1,
            project: folder.manifest().clone(),
            levels: Vec::new(),
            sources: BTreeMap::new(),
        };
        for (path, bytes) in &sources {
            let name = add_blob(&mut blobs, &mut total, bytes.clone())?;
            let _old = index.sources.insert(path.clone(), name);
        }
        let mut last_report = None;
        let mut warnings = Vec::new();
        let mut cache_decisions = Vec::new();
        for level in &folder.manifest().levels {
            progress::check_cancelled()?;
            let selected_output = temporary.join(format!("{}.placesmap", level.id));
            let built = build_staged(request, &staged_source, &selected_output, &level.id)?;
            warnings.extend(built.warnings.iter().cloned());
            cache_decisions.extend(built.cache_decisions.iter().cloned().map(|mut decision| {
                decision.stage = format!("{}/{}", level.id, decision.stage);
                decision
            }));
            last_report = Some(built);
            let mut reader =
                PackageReader::new(File::open(&selected_output).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let mut records = BTreeMap::new();
            for name in reader.names().to_vec() {
                progress::check_cancelled()?;
                let bytes = reader.read_entry(&name, crate::engine::package::MAX_BINARY_BYTES)?;
                let blob = add_blob(&mut blobs, &mut total, bytes)?;
                let _old = records.insert(name, blob);
            }
            drop(reader);
            fs::remove_file(&selected_output).map_err(|e| e.to_string())?;
            index.levels.push(project::ProjectMap {
                id: level.id.clone(),
                records,
            });
        }
        let publication = progress::BuildScope::start(&request.source, Some("project export"));
        progress::stage("checking source snapshot and publishing project");
        let mut report = last_report.ok_or("Project has no levels")?;
        report.warnings = warnings;
        report.cache_decisions = cache_decisions;
        let outcome = publish(
            request,
            &index,
            blobs,
            report,
            || {
                if project::snapshot(&folder, &progress::check_cancelled)? != sources {
                    return Err(
                        "Project changed during export; previous distribution retained".into(),
                    );
                }
                if output_identity(&request.out)?.as_deref() != original_output.as_deref() {
                    return Err(
                        "Export destination changed during compilation; external work retained"
                            .into(),
                    );
                }
                Ok(())
            },
            started,
        );
        publication.finish(&outcome);
        outcome
    })();
    let cleanup = fs::remove_dir_all(&temporary);
    match (result, cleanup) {
        (Ok(mut report), Err(error)) => {
            report.warnings.push(format!(
                "Export published; staging cleanup {}: {error}",
                temporary.display()
            ));
            Ok(report)
        }
        (Err(error), Err(cleanup_error)) => Err(format!(
            "{error}; staging cleanup {}: {cleanup_error}",
            temporary.display()
        )),
        (outcome, Ok(())) => outcome,
    }
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

fn add_blob(
    blobs: &mut BTreeMap<String, Vec<u8>>,
    total: &mut u64,
    bytes: Vec<u8>,
) -> Result<String, String> {
    let name = hash::blob_name(&bytes, ".record");
    if !blobs.contains_key(&name) {
        *total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if *total > crate::engine::package::MAX_TOTAL_BYTES
            || blobs.len() >= crate::engine::package::MAX_ENTRIES.saturating_sub(1)
        {
            return Err("Project exceeds package size/record limits".into());
        }
        let _old = blobs.insert(name.clone(), bytes);
    }
    Ok(name)
}

fn output_identity(path: &std::path::Path) -> Result<Option<String>, String> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            hash::sha256_file(path).map(Some)
        }
        Ok(_) => Err("Export destination is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
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

fn package_entries(
    index: &project::ProjectPackage,
    blobs: BTreeMap<String, Vec<u8>>,
) -> Result<(Vec<archive::PendingEntry>, String), String> {
    let index_bytes = serde_json::to_vec(index).map_err(|e| e.to_string())?;
    if u64::try_from(index_bytes.len()).unwrap_or(u64::MAX)
        > crate::engine::package::MAX_MANIFEST_BYTES
    {
        return Err("Project index exceeds 2 MiB".into());
    }
    let fingerprint = hash::sha256_hex(&index_bytes);
    let mut entries: Vec<_> = blobs
        .into_iter()
        .map(|(name, bytes)| archive::PendingEntry { name, bytes })
        .collect();
    entries.push(archive::PendingEntry {
        name: project::INDEX.into(),
        bytes: index_bytes,
    });
    Ok((entries, fingerprint))
}

fn stage_sources(
    temporary: &std::path::Path,
    sources: &BTreeMap<crate::engine::project::ProjectPath, Vec<u8>>,
) -> Result<std::path::PathBuf, String> {
    use std::io::Write as _;
    let root = temporary.join("source");
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    for (path, bytes) in sources {
        progress::check_cancelled()?;
        let destination = root.join(path.as_str());
        fs::create_dir_all(destination.parent().ok_or("Staged source has no parent")?)
            .map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(root)
}

fn publish(
    request: &BuildRequest,
    index: &project::ProjectPackage,
    blobs: BTreeMap<String, Vec<u8>>,
    mut report: BuildReport,
    publish_check: impl Fn() -> Result<(), String>,
    started: std::time::Instant,
) -> Result<BuildReport, String> {
    let (entries, fingerprint) = package_entries(index, blobs)?;
    archive::write_archive_guarded(
        &request.out,
        entries,
        progress::check_cancelled,
        publish_check,
    )?;
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

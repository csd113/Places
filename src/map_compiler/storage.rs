//! Exact prepared-prop factoring and compressed archive size selection.
//! Kept separate from validation, lighting preparation and package publication.

use std::collections::BTreeSet;

use super::{BlobMap, insert_blob};
use crate::engine::package::hash::blob_name;
use crate::engine::package::manifest::Variant;

/// Lossless storage migration. It verifies the complete input, preserves exact
/// semantics/source/provenance, and never prepares geometry or solves light.
/// A new destination is required; cancellation cannot replace the usable input.
pub(super) fn optimize(request: &super::BuildRequest) -> Result<super::BuildReport, String> {
    let scope = super::progress::BuildScope::start(&request.source, Some("storage migration"));
    let result = optimize_inner(request);
    scope.finish(&result);
    result
}

fn optimize_inner(request: &super::BuildRequest) -> Result<super::BuildReport, String> {
    use crate::engine::package::{archive, project};
    let started = std::time::Instant::now();
    if request.out.symlink_metadata().is_ok() || request.out == request.source {
        return Err("Storage optimization requires a new output filename".into());
    }
    if request
        .out
        .extension()
        .is_none_or(|extension| extension != "placesmap")
    {
        return Err("Storage output requires a .placesmap filename".into());
    }
    let original = crate::engine::package::hash::sha256_file(&request.source)?;
    let validated = super::validate(&request.source)?;
    if !validated.dependencies_intact {
        return Err("Storage migration refuses changed or missing installed dependencies".into());
    }
    let temporary = archive::create_temporary_directory(&request.out)?;
    let result = (|| {
        let mut reader = super::open_package(&request.source)?;
        let mut index = project::read_index(&mut reader)?;
        let mut spool = super::project_spool::ProjectSpool::new(&temporary)?;
        if let Some(project) = index.as_mut() {
            for source in project.sources.values_mut() {
                *source =
                    spool.add_entry(&mut reader, source, &super::progress::check_cancelled)?;
            }
        }
        let ids = index.as_ref().map_or_else(
            || vec![None],
            |project| {
                project
                    .levels
                    .iter()
                    .map(|level| Some(level.id.clone()))
                    .collect()
            },
        );
        for id in ids {
            super::progress::check_cancelled()?;
            let (manifest, records) = optimize_member(&request.source, id.as_deref(), &mut spool)?;
            if let Some(project) = index.as_mut() {
                let member = project
                    .levels
                    .iter_mut()
                    .find(|member| member.id == manifest.id)
                    .ok_or("Project member disappeared")?;
                member.records = records;
            } else {
                publish_ordinary(request, &original, records, spool)?;
                return storage_report(request, started);
            }
        }
        let project = index.ok_or("Project index disappeared")?;
        let (entries, _) = super::distribution::package_entries(&project, spool, &temporary)?;
        archive::write_project_archive_files_with_level_guarded(
            &request.out,
            entries,
            super::progress::check_cancelled,
            || check_unchanged(request, &original),
            9,
        )?;
        storage_report(request, started)
    })();
    let cleanup = std::fs::remove_dir_all(&temporary);
    match (result, cleanup) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(format!(
            "Storage output published; staging cleanup failed: {error}"
        )),
    }
}

fn publish_ordinary(
    request: &super::BuildRequest,
    original: &str,
    records: std::collections::BTreeMap<String, String>,
    spool: super::project_spool::ProjectSpool,
) -> Result<(), String> {
    use crate::engine::package::archive;
    let entries = spool.into_entries();
    let lookup: std::collections::BTreeMap<_, _> = entries
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect();
    let ordinary = records
        .into_iter()
        .map(|(name, physical)| {
            let entry = lookup
                .get(&physical)
                .ok_or("Storage spool record disappeared")?;
            let bytes = std::fs::read(&entry.path).map_err(|e| e.to_string())?;
            Ok(archive::PendingEntry { name, bytes })
        })
        .collect::<Result<Vec<_>, String>>()?;
    archive::write_archive_with_level_guarded(
        &request.out,
        ordinary,
        super::progress::check_cancelled,
        || check_unchanged(request, original),
        9,
    )?;
    Ok(())
}

fn optimize_member(
    source: &std::path::Path,
    id: Option<&str>,
    spool: &mut super::project_spool::ProjectSpool,
) -> Result<
    (
        crate::engine::package::Manifest,
        std::collections::BTreeMap<String, String>,
    ),
    String,
> {
    use crate::engine::package::{PackageEntry, PackageReader, project};
    use std::fs::File;
    let mut selected = PackageReader::new(File::open(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    project::select(&mut selected, id)?;
    let mut manifest = super::read_manifest(&mut selected)?;
    let _runtime_valid = crate::engine::package::world::open_level(source, &manifest.id)?;
    super::progress::stage(format!("lossless prop storage migration: {}", manifest.id));
    let mut blobs = super::BlobMap::new();
    let names: BTreeSet<String> = manifest
        .variants
        .iter()
        .flat_map(|variant| {
            std::iter::once(variant.entries.props.clone()).chain(
                variant
                    .entries
                    .static_light_off
                    .as_ref()
                    .map(|state| state.props.clone()),
            )
        })
        .collect();
    for name in names {
        let bytes = crate::engine::package::shared_props::read_record(&mut selected, &name)?;
        let replacement = super::insert_blob(&mut blobs, bytes, ".props", "props");
        for variant in &mut manifest.variants {
            if variant.entries.props == name {
                variant.entries.props.clone_from(&replacement);
            }
            if let Some(state) = variant.entries.static_light_off.as_mut()
                && state.props == name
            {
                state.props.clone_from(&replacement);
            }
        }
    }
    factor_prepared_props_with_compression(&mut manifest.variants, &mut blobs, 9)?;
    let mut records = std::collections::BTreeMap::new();
    manifest
        .entries
        .retain(|entry| !matches!(entry.role.as_str(), "props" | "props-shared"));
    for entry in &manifest.entries {
        let physical = spool.add_entry(
            &mut selected,
            &entry.name,
            &super::progress::check_cancelled,
        )?;
        drop(records.insert(entry.name.clone(), physical));
    }
    let mut shared_version = 0;
    for (name, (bytes, _)) in &blobs {
        if name.ends_with(".props.recipe") {
            shared_version = shared_version
                .max(crate::engine::package::shared_props::read_recipe(bytes)?.version());
        }
    }
    manifest
        .required_capabilities
        .retain(|capability| !capability.starts_with("props-shared-streams"));
    if shared_version > 0 {
        manifest
            .required_capabilities
            .push("props-shared-streams".into());
        for version in 2..=shared_version {
            manifest
                .required_capabilities
                .push(format!("props-shared-streams-v{version}"));
        }
    }
    for (name, (bytes, role)) in blobs {
        manifest.entries.push(PackageEntry {
            name: name.clone(),
            role,
            bytes: u64::try_from(bytes.len()).map_err(|e| e.to_string())?,
            sha256: crate::engine::package::hash::sha256_hex(&bytes),
        });
        drop(records.insert(name, spool.add_bytes(&bytes)?));
    }
    manifest.entries.sort_by(|a, b| a.name.cmp(&b.name));
    drop(records.insert(
        "manifest.json".into(),
        spool.add_bytes(&manifest.to_json()?)?,
    ));
    Ok((manifest, records))
}

fn check_unchanged(request: &super::BuildRequest, original: &str) -> Result<(), String> {
    if request.out.symlink_metadata().is_ok() {
        return Err("Storage destination appeared during migration".into());
    }
    if crate::engine::package::hash::sha256_file(&request.source)? != original {
        return Err("Input package changed during migration".into());
    }
    Ok(())
}

fn storage_report(
    request: &super::BuildRequest,
    started: std::time::Instant,
) -> Result<super::BuildReport, String> {
    let mut report = super::distribution::operation_report(request);
    report.bytes = std::fs::metadata(&request.out)
        .map_err(|e| e.to_string())?
        .len();
    report.millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
    report.reuse_reason =
        "All lighting states and exact decoded records retained; storage only".into();
    Ok(report)
}

/// Preserve the exact prepared record stream while sharing repeated model
/// data between placements and variants. Already-factored reusable records
/// retain their original identities.
pub(super) fn factor_prepared_props(
    variants: &mut [Variant],
    blobs: &mut BlobMap,
) -> Result<(), String> {
    factor_prepared_props_with_compression(variants, blobs, 6)
}

fn factor_prepared_props_with_compression(
    variants: &mut [Variant],
    blobs: &mut BlobMap,
    compression: i64,
) -> Result<(), String> {
    let names: BTreeSet<String> = variants
        .iter()
        .flat_map(|variant| {
            std::iter::once(variant.entries.props.clone()).chain(
                variant
                    .entries
                    .static_light_off
                    .as_ref()
                    .map(|state| state.props.clone()),
            )
        })
        .collect();
    let source_names: Vec<String> = names
        .into_iter()
        .filter(|name| {
            blobs
                .get(name)
                .is_some_and(|(bytes, _)| bytes.starts_with(b"PMP1"))
        })
        .collect();
    if source_names.is_empty() {
        return Ok(());
    }
    let records: Vec<&[u8]> = source_names
        .iter()
        .map(|name| {
            blobs
                .get(name)
                .map(|(bytes, _)| bytes.as_slice())
                .ok_or_else(|| "prepared prop record is missing".to_string())
        })
        .collect::<Result<_, _>>()?;
    let original_bytes = records
        .iter()
        .fold(0_usize, |total, bytes| total.saturating_add(bytes.len()));
    let (factored, shared_archive_bytes) = best_factoring(&records, compression)?;
    let mut recipes_fit = true;
    for recipe in &factored.recipes {
        if !crate::engine::package::shared_props::recipe_fits_runtime_budget(recipe)? {
            recipes_fit = false;
        }
    }
    if !recipes_fit {
        return Ok(());
    }
    let stored_bytes = factored
        .recipes
        .iter()
        .chain(&factored.segments)
        .fold(0_usize, |total, bytes| total.saturating_add(bytes.len()));
    // A tiny or nonrepeated model set need not pay for a dictionary.
    if stored_bytes >= original_bytes {
        return Ok(());
    }
    let original_archive_bytes = deflated_entries_bytes(
        source_names
            .iter()
            .zip(&records)
            .map(|(name, bytes)| (name.as_str(), *bytes)),
        compression,
    )?;
    if shared_archive_bytes >= original_archive_bytes {
        return Ok(());
    }
    for segment in factored.segments {
        let _name = insert_blob(blobs, segment, ".props.shared", "props-shared");
    }
    for (original, recipe) in source_names.iter().zip(factored.recipes) {
        let name = insert_blob(blobs, recipe, ".props.recipe", "props");
        for variant in variants.iter_mut() {
            if variant.entries.props == *original {
                variant.entries.props.clone_from(&name);
            }
            if let Some(state) = variant.entries.static_light_off.as_mut()
                && state.props == *original
            {
                state.props.clone_from(&name);
            }
        }
        drop(blobs.remove(original));
    }
    crate::engine::logging::info(format_args!(
        "[compiler] prepared props exact sharing: {original_bytes} -> {stored_bytes} raw bytes; {original_archive_bytes} -> {shared_archive_bytes} archive bytes including manifest allowance"
    ));
    Ok(())
}

/// Count both storage alternatives with the publication codec before choosing.
/// ZIP names, headers and directory entries are measured by the same writer;
/// a deliberately generous manifest allowance includes each replacement/new
/// entry plus variant references, the capability and JSON framing changes.
fn best_factoring(
    records: &[&[u8]],
    compression: i64,
) -> Result<(crate::engine::package::shared_props::FactoredProps, u64), String> {
    use crate::engine::package::shared_props::{factor_props, factor_props_with_version};
    let established = factor_props_with_version(records, 2)?;
    let established_size = prepared_props_archive_size(&established, compression)?;
    drop(established);
    let shuffled = factor_props(records)?;
    let shuffled_size = prepared_props_archive_size(&shuffled, compression)?;
    if shuffled_size < established_size {
        Ok((shuffled, shuffled_size))
    } else {
        drop(shuffled);
        Ok((factor_props_with_version(records, 2)?, established_size))
    }
}

fn prepared_props_archive_size(
    factored: &crate::engine::package::shared_props::FactoredProps,
    compression: i64,
) -> Result<u64, String> {
    for recipe in &factored.recipes {
        if !crate::engine::package::shared_props::recipe_fits_runtime_budget(recipe)? {
            return Ok(u64::MAX);
        }
    }
    let shared_names: Vec<String> = factored
        .recipes
        .iter()
        .map(|bytes| blob_name(bytes, ".props.recipe"))
        .chain(
            factored
                .segments
                .iter()
                .map(|bytes| blob_name(bytes, ".props.shared")),
        )
        .collect();
    let shared = deflated_entries_bytes(
        shared_names
            .iter()
            .zip(factored.recipes.iter().chain(&factored.segments))
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
        compression,
    )?;
    let changed_entries = u64::try_from(shared_names.len())
        .map_err(|error| format!("shared prop manifest entry count is too large: {error}"))?;
    let manifest_allowance = changed_entries
        .checked_mul(1024)
        .and_then(|bytes| bytes.checked_add(1024))
        .ok_or_else(|| "shared prop manifest allowance overflows".to_string())?;
    let proposed = shared
        .checked_add(manifest_allowance)
        .ok_or_else(|| "shared prop archive size estimate overflows".to_string())?;
    Ok(proposed)
}

fn deflated_entries_bytes<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    compression: i64,
) -> Result<u64, String> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(ArchiveSizeCounter::default());
    for (name, bytes) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression))
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        writer
            .start_file(name, options)
            .map_err(|error| format!("could not estimate prop entry '{name}': {error}"))?;
        writer.write_all(bytes).map_err(|error| {
            format!("could not compress prop entry '{name}' for sizing: {error}")
        })?;
    }
    writer
        .finish()
        .map(|counter| counter.length)
        .map_err(|error| format!("could not finish prop archive size estimate: {error}"))
}

/// A ZIP-compatible seekable sink that retains no compressed payload bytes.
#[derive(Default)]
struct ArchiveSizeCounter {
    position: u64,
    length: u64,
}

impl std::io::Write for ArchiveSizeCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
        self.position = self
            .position
            .checked_add(count)
            .ok_or_else(|| std::io::Error::other("prop archive size counter overflows"))?;
        self.length = self.length.max(self.position);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::io::Seek for ArchiveSizeCounter {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            std::io::SeekFrom::Start(offset) => Some(offset),
            std::io::SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            std::io::SeekFrom::End(offset) => self.length.checked_add_signed(offset),
        };
        self.position = next.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "prop archive size counter seek is out of bounds",
            )
        })?;
        Ok(self.position)
    }
}

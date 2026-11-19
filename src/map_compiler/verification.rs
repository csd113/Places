//! Read-only currentness checks of standalone maps and complete projects.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::engine::loader::PropCatalog;
use crate::engine::package::{PackageReader, archive, hash, project as package_project, world};
use crate::engine::project::{ProjectFolder, ProjectManifest};

use super::{
    BuildInputs, LightmapQuality, Manifest, VerifyReport, catalog_inputs, check_cached_lighting,
    collect_dependencies, compiler_tool_sha256, fingerprint, fingerprint_with_build_inputs,
    lighting_stage_fingerprint, load_catalog_identity, open_package, package_input_differences,
    progress, project, read_build_inputs_from, verify_entry_hashes,
};

/// Recomputes the build fingerprint of `source` and compares it with a package.
/// # Errors
/// Returns source, asset or package validation errors.
pub fn verify(source: &Path, package: &Path, asset_root: &Path) -> Result<VerifyReport, String> {
    verify_selected(source, None, package, asset_root)
}

/// Verifies an explicitly selected project level, or a standalone source.
/// # Errors
/// Returns source, asset or package validation errors.
pub fn verify_selected(
    source: &Path,
    selected: Option<&str>,
    package: &Path,
    asset_root: &Path,
) -> Result<VerifyReport, String> {
    crate::engine::paths::configure_assets(asset_root)?;
    let snapshot = project::read_source(source, selected)?;
    let mut reader = open_package(package)?;
    package_project::select(&mut reader, Some(&snapshot.level.id))?;
    let manifest = world::read_manifest(&mut reader)?;
    if manifest.id != snapshot.level.id {
        return Err(format!(
            "Package manifest does not describe requested level '{}'",
            snapshot.level.id
        ));
    }
    let context = Verification::new(source, package, asset_root)?;
    context.member(snapshot, manifest, &mut reader, verify_entry_hashes)
}

/// Verifies all project members from one archive and one validated project index.
/// # Errors
/// Returns source, asset or package validation errors.
pub fn verify_project(
    source: &Path,
    package: &Path,
    asset_root: &Path,
) -> Result<Vec<VerifyReport>, String> {
    crate::engine::paths::configure_assets(asset_root)?;
    let folder = ProjectFolder::open(source)?;
    let (mut reader, project_index) = PackageReader::new_indexed(archive::open_file(package)?)
        .map_err(|error| error.to_string())?;
    let index = project_index.ok_or("verify-project requires a compiled project package")?;
    crate::engine::package::release::admit_places_version(&index.places_version)?;
    check_project_metadata(folder.manifest(), &index.project)?;
    let context = Verification::new(source, package, asset_root)?;
    let mut checked_records = BTreeMap::new();
    folder
        .manifest()
        .levels
        .iter()
        .map(|level| {
            progress::check_cancelled()?;
            let snapshot = project::read_member(&folder, Some(&level.id))?;
            let member = package_project::select_index(&mut reader, &index, Some(&level.id))?;
            let manifest = world::read_manifest(&mut reader)?;
            context.member(snapshot, manifest, &mut reader, |archive, manifest_ref| {
                verify_project_entries(archive, manifest_ref, member, &mut checked_records)
            })
        })
        .collect()
}

fn check_project_metadata(
    authored: &ProjectManifest,
    indexed: &ProjectManifest,
) -> Result<(), String> {
    let authored_ids: BTreeSet<_> = authored
        .levels
        .iter()
        .map(|level| level.id.as_str())
        .collect();
    let indexed_ids: BTreeSet<_> = indexed
        .levels
        .iter()
        .map(|level| level.id.as_str())
        .collect();
    if authored_ids != indexed_ids {
        let missing: Vec<_> = authored_ids.difference(&indexed_ids).copied().collect();
        let extra: Vec<_> = indexed_ids.difference(&authored_ids).copied().collect();
        return Err(format!(
            "Package project membership differs from authored project: missing {missing:?}, extra {extra:?}"
        ));
    }
    // Both manifests are parsed and validated, including default release values.
    if authored != indexed {
        return Err("Package project metadata differs from authored project".into());
    }
    Ok(())
}

type Reader = PackageReader<std::fs::File>;

struct Verification<'a> {
    source: &'a Path,
    package: &'a Path,
    asset_root: &'a Path,
    catalog_path: PathBuf,
    catalog: PropCatalog,
    catalog_hash: String,
}

impl<'a> Verification<'a> {
    fn new(source: &'a Path, package: &'a Path, asset_root: &'a Path) -> Result<Self, String> {
        let catalog_path = asset_root.join("catalog.json");
        let (catalog, catalog_hash) = load_catalog_identity(&catalog_path)?;
        Ok(Self {
            source,
            package,
            asset_root,
            catalog_path,
            catalog,
            catalog_hash,
        })
    }

    fn member(
        &self,
        snapshot: project::SourceSnapshot,
        manifest: Manifest,
        reader: &mut Reader,
        verify_hashes: impl FnOnce(&mut Reader, &Manifest) -> Result<(), String>,
    ) -> Result<VerifyReport, String> {
        let mut level = snapshot.level;
        let mut warnings = Vec::new();
        super::prepare_level(&mut level, self.catalog.assets(), None);
        crate::engine::project::validate_level_assets(&level, self.catalog.assets())?;
        let dependencies =
            collect_dependencies(&level, &self.catalog, self.asset_root, &mut warnings)?;
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
            catalog_inputs: Some(catalog_inputs(&level, &self.catalog_path)?),
            compiler_code_sha256: Some(env!("PLACES_COMPILER_CODE_SHA256").into()),
            source_sha256: hash::sha256_hex(&snapshot.identity),
            catalog_sha256: self.catalog_hash.clone(),
            tool_sha256: compiler_tool_sha256()?.to_string(),
            capture_probes: true,
            compact: false,
        };
        let current_fingerprint = fingerprint_with_build_inputs(
            &fingerprint(&build_inputs.source_sha256, &variants, &dependencies),
            &build_inputs,
        );
        let previous = read_build_inputs_from(reader, &manifest);
        let mut differences = warnings
            .into_iter()
            .filter(|warning| warning.starts_with("dependency "))
            .collect::<Vec<_>>();
        differences.extend(package_input_differences(
            previous.as_ref(),
            &manifest,
            &build_inputs,
            &dependencies,
            &variants,
            &current_fingerprint,
        ));
        if manifest.package_id != snapshot.package_id
            || manifest.package_version != snapshot.package_version
        {
            differences.push("package release metadata differs from authored source".into());
        }
        let integrity = check_cached_lighting(reader, &manifest)
            .and_then(|()| verify_hashes(reader, &manifest));
        let integrity_valid = integrity.is_ok()
            && !differences.iter().any(|reason| {
                reason.starts_with("dependency '") && reason.contains("could not be read")
            });
        if let Err(error) = integrity {
            differences.push(format!("package integrity failed: {error}"));
        }
        let prepared = lighting_stage_fingerprint(
            &level,
            &variants,
            &dependencies,
            &build_inputs,
            &self.catalog,
            &self.catalog_path,
            self.asset_root,
        )?;
        let prepared_reusable = integrity_valid
            && manifest.lighting_fingerprint.as_deref() == Some(&prepared)
            && previous.is_ok_and(|record| super::preparation_inputs_match(&record, &build_inputs));
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
            source: self.source.display().to_string(),
            package: self.package.display().to_string(),
            package_fingerprint: manifest.compiler_fingerprint,
            current_fingerprint,
            current,
            differences,
            integrity_valid,
            bake_required: !prepared_reusable,
            status,
        })
    }
}

fn verify_project_entries<'a>(
    reader: &mut Reader,
    manifest: &Manifest,
    member: &'a package_project::ProjectMap,
    checked_records: &mut BTreeMap<&'a str, Result<(), String>>,
) -> Result<(), String> {
    for entry in &manifest.entries {
        progress::check_cancelled()?;
        let physical = member
            .records
            .get(&entry.name)
            .ok_or_else(|| format!("Package entry '{}' has no physical record", entry.name))?;
        let limit = manifest.entry_read_limit(entry);
        if entry.bytes > limit {
            return Err(format!(
                "package entry '{physical}' declares {} bytes (limit {limit})",
                entry.bytes
            ));
        }

        // The index already validates the physical name and indexed size. The
        // manifest still constrains each alias, even when its payload is shared.
        let actual = hash::sha256_from_blob_name(physical)
            .ok_or_else(|| format!("Invalid project blob: {physical}"))?;
        let checked = checked_records
            .entry(physical)
            .or_insert_with(|| reader.verify_physical_record(physical, limit));
        if let Err(error) = checked {
            return Err(error.clone());
        }
        super::check_declared_hash(entry, &actual)?;
    }
    Ok(())
}

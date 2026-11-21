//! Read-only currentness checks of standalone maps and complete projects.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::engine::package::{PackageReader, archive, hash, project as package_project, world};
use crate::engine::project::{ProjectFolder, ProjectManifest};

use super::{
    AssetInputs, BuildInputs, LightmapQuality, Manifest, VerifyReport, check_cached_lighting,
    collect_dependencies, compiler_tool_sha256, fingerprint, fingerprint_with_build_inputs,
    lighting_stage_fingerprint, open_package, package_input_differences, probe_inputs_match,
    probes, progress, project, read_build_inputs_from, validate_appearance_textures,
    verify_entry_hashes,
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
    let assets = AssetInputs::read(crate::engine::paths::assets()?)?;
    let report = verify_selected_with_assets(source, selected, package, &assets, true)?;
    assets.check()?;
    Ok(report)
}

pub(super) fn verify_selected_with_assets(
    source: &Path,
    selected: Option<&str>,
    package: &Path,
    assets: &AssetInputs,
    capture_probes: bool,
) -> Result<VerifyReport, String> {
    let _asset_scope = assets.scope();
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
    Verification {
        source,
        package,
        assets,
        capture_probes,
    }
    .member(snapshot, manifest, &mut reader, verify_entry_hashes)
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
    let assets = AssetInputs::read(crate::engine::paths::assets()?)?;
    let _asset_scope = assets.scope();
    let context = Verification {
        source,
        package,
        assets: &assets,
        capture_probes: true,
    };
    let mut checked_records = BTreeMap::new();
    let reports = folder
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
        .collect::<Result<_, _>>()?;
    assets.check()?;
    Ok(reports)
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
    assets: &'a AssetInputs,
    capture_probes: bool,
}

impl Verification<'_> {
    fn member(
        &self,
        snapshot: project::SourceSnapshot,
        manifest: Manifest,
        reader: &mut Reader,
        verify_hashes: impl FnOnce(&mut Reader, &Manifest) -> Result<(), String>,
    ) -> Result<VerifyReport, String> {
        let mut level = snapshot.level;
        let mut warnings = Vec::new();
        let catalog = self.assets.catalog();
        super::prepare_level(&mut level, catalog.assets(), None);
        crate::engine::project::validate_level_assets(&level, catalog.assets())?;
        let dependencies = collect_dependencies(&level, self.assets, &mut warnings)?;
        validate_appearance_textures(&level, catalog, self.assets.root())?;
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
            catalog_inputs: Some(self.assets.catalog_inputs(&level)?),
            compiler_code_sha256: Some(env!("PLACES_COMPILER_CODE_SHA256").into()),
            source_sha256: hash::sha256_hex(&snapshot.identity),
            catalog_sha256: self.assets.catalog_hash().into(),
            tool_sha256: compiler_tool_sha256()?.to_string(),
            capture_probes: self.capture_probes,
            probe_sky_sha256: self
                .capture_probes
                .then(|| probes::sky_fingerprint(&level, catalog.assets(), &dependencies))
                .transpose()?,
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
            self.assets,
        )?;
        let prepared_reusable = integrity_valid
            && manifest.lighting_fingerprint.as_deref() == Some(&prepared)
            && previous.is_ok_and(|record| {
                super::preparation_inputs_match(&record, &build_inputs)
                    && manifest.variants.iter().all(|variant| {
                        probe_inputs_match(&record, &build_inputs, &variant.entries.probes)
                    })
            });
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

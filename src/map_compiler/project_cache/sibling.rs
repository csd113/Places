//! Verifies a Final Build snapshot before distribution can spool its records.
use std::fs::File;
use std::path::Path;

use crate::engine::package::{PackageReader, archive, hash, manifest::Manifest, project, world};
use crate::engine::project::{ProjectFolder, ProjectLevel};
use crate::map_compiler::{self as compiler, AssetInputs, BuildRequest, progress};

mod variants;

pub(in crate::map_compiler) struct VerifiedSibling {
    pub(in crate::map_compiler) reader: PackageReader<File>,
    pub(in crate::map_compiler) warnings: Vec<String>,
}

pub(in crate::map_compiler) fn stage_current_sibling(
    request: &BuildRequest,
    folder: &ProjectFolder,
    member: &ProjectLevel,
    source: &Path,
    staging: &archive::OwnedStaging,
    assets: &AssetInputs,
) -> Result<Option<VerifiedSibling>, String> {
    let result = (|| {
        let sibling = compiler::package_path_for(&folder.root().join(member.source.as_str()));
        progress::stage(format!(
            "snapshotting Final Build sibling for {}",
            member.id
        ));
        let file = archive::snapshot_ordinary_file(
            &sibling,
            staging.directory(),
            &progress::check_cancelled,
        )?;
        progress::stage(format!("verifying Final Build sibling for {}", member.id));
        let mut reader = PackageReader::new(file).map_err(|error| error.to_string())?;
        let warnings = verify_sibling(&mut reader, request, folder, member, source, assets)?;
        Ok::<_, String>(VerifiedSibling { reader, warnings })
    })();
    progress::check_cancelled()?;
    match result {
        Ok(candidate) => Ok(Some(candidate)),
        Err(reason) => {
            crate::engine::logging::info(format_args!(
                "[compiler] Final Build sibling miss for '{}': {reason}",
                member.id
            ));
            Ok(None)
        }
    }
}

fn verify_sibling(
    reader: &mut PackageReader<File>,
    request: &BuildRequest,
    folder: &ProjectFolder,
    member: &ProjectLevel,
    source: &Path,
    assets: &AssetInputs,
) -> Result<Vec<String>, String> {
    let _scope = crate::engine::resources::admission_scope();
    let _asset_scope = assets.scope();
    if reader.contains(project::INDEX) {
        return Err("Expected an ordinary map package".into());
    }
    let manifest = compiler::read_manifest(reader)?;
    let mut snapshot = compiler::project::read_source(source, Some(&member.id))?;
    verify_current_inputs(reader, request, &manifest, &mut snapshot, assets)?;
    let level = &snapshot.level;
    let semantics = crate::engine::canonical_json::canonical_json_bytes(level)?;
    let semantic_entry = manifest
        .entry("semantics.json")
        .ok_or("Missing map semantics")?;
    if semantic_entry.sha256 != hash::sha256_hex(&semantics)
        || manifest.name != level.name
        || manifest.author != level.author
    {
        return Err("Map semantics differ from the saved source".into());
    }
    let typed_level = world::read_semantic_level(reader, &manifest)?;
    world::validate_semantics_for_project(&manifest, &typed_level, |id| {
        folder
            .manifest()
            .levels
            .iter()
            .any(|declared| declared.id == id)
    })?;
    verify_record_identities(reader, &manifest)?;
    let mut warnings = compiler::navigation_cache::sibling_warnings(reader, &manifest, level)?;
    variants::verify(reader, request, &manifest, level, assets, &mut warnings)?;
    Ok(warnings)
}

fn verify_current_inputs(
    reader: &mut PackageReader<File>,
    request: &BuildRequest,
    manifest: &Manifest,
    snapshot: &mut compiler::project::SourceSnapshot,
    assets: &AssetInputs,
) -> Result<(), String> {
    let level = &mut snapshot.level;
    if manifest.id != level.id
        || manifest.package_id != snapshot.package_id
        || manifest.package_version != snapshot.package_version
        || manifest.places_version != crate::engine::package::release::PLACES_VERSION
        || manifest.created_by != compiler::COMPILER_NAME
        || request.workers == 0
        || !manifest
            .variants
            .iter()
            .map(|variant| variant.lightmap_quality.as_str())
            .eq(request.variants.iter().map(|quality| quality.name()))
    {
        return Err("Map identity, compiler target or requested variants differ".into());
    }

    let source_hash = hash::sha256_hex(&snapshot.identity);
    let previous = compiler::read_build_inputs_from(reader, manifest)?;
    if previous.source_sha256 != source_hash
        || previous.capture_probes != request.capture_probes
        || previous.compact != request.compact
    {
        return Err("Saved source, capture or storage mode changed".into());
    }
    let catalog = assets.catalog();
    compiler::admit_build_level(level, &request.variants, catalog)?;
    let mut warnings = Vec::new();
    let dependencies = compiler::collect_dependencies(level, assets, &mut warnings)?;
    compiler::validate_appearance_textures(level, catalog, assets.root())?;
    let inputs = compiler::build_inputs(level, assets, &dependencies, &source_hash, request)?;
    if previous != inputs || manifest.dependencies != dependencies || !warnings.is_empty() {
        return Err("Compiler provenance or asset dependencies changed".into());
    }
    let fingerprint = compiler::fingerprint_with_build_inputs(
        &compiler::fingerprint(&source_hash, &request.variants, &dependencies),
        &inputs,
    );
    let lighting = compiler::lighting_stage_fingerprint(
        level,
        &request.variants,
        &dependencies,
        &inputs,
        assets,
    )?;
    if manifest.compiler_fingerprint != fingerprint
        || manifest.lighting_fingerprint.as_deref() != Some(&lighting)
    {
        return Err("Compiler or prepared lighting identity changed".into());
    }

    Ok(())
}

fn verify_record_identities(
    reader: &mut PackageReader<File>,
    manifest: &Manifest,
) -> Result<(), String> {
    for entry in &manifest.entries {
        let (bytes, sha256) = reader.copy_entry_to(
            &entry.name,
            manifest
                .entry_read_limit(entry)
                .min(crate::engine::package::MAX_BINARY_BYTES),
            &mut std::io::sink(),
            &progress::check_cancelled,
        )?;
        if bytes != entry.bytes || sha256 != entry.sha256 {
            return Err(format!(
                "Record '{}' disagrees with its manifest",
                entry.name
            ));
        }
    }
    Ok(())
}

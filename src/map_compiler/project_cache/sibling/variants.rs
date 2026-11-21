//! Prepared records required by the ordinary compiler's selected qualities.
use std::fs::File;

use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::materials::MaterialTable;
use crate::engine::package::manifest::{Manifest, Variant};
use crate::engine::package::{self, PackageReader, world};
use crate::engine::quality::LightmapQuality;
use crate::engine::render::{self, MaterialRenderState};
use crate::map_compiler::{self as compiler, AssetInputs, BuildRequest, progress};

pub(super) fn verify(
    reader: &mut PackageReader<File>,
    request: &BuildRequest,
    manifest: &Manifest,
    level: &LevelDef,
    assets: &AssetInputs,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let fixture_indices = LevelLighting::prepared_static_fixtures(level);
    let materials =
        if request.capture_probes && request.variants.iter().any(|quality| !quality.is_off()) {
            Some(compiler::resolve_build_materials(
                level,
                assets.catalog(),
                assets.root(),
            )?)
        } else {
            None
        };
    for (variant, quality) in manifest.variants.iter().zip(&request.variants) {
        progress::check_cancelled()?;
        world::validate_cached_lighting(reader, variant)?;
        let static_fixtures = if quality.is_off() {
            fixture_indices.as_slice()
        } else {
            &[]
        };
        verify_static_off(variant, static_fixtures)?;
        verify_irradiance(reader, variant, *quality, warnings)?;
        verify_captures(
            reader,
            variant,
            materials.as_ref().filter(|_| !quality.is_off()),
        )?;
        compiler::validate_variant(reader, variant)?;
    }
    Ok(())
}

fn verify_static_off(variant: &Variant, expected: &[usize]) -> Result<(), String> {
    match &variant.entries.static_light_off {
        None if expected.is_empty() => Ok(()),
        Some(state) if !expected.is_empty() && state.fixtures == expected => Ok(()),
        _ => Err(format!(
            "{} Static-off state disagrees with the authored fixture list",
            variant.lightmap_quality
        )),
    }
}

fn verify_irradiance(
    reader: &mut PackageReader<File>,
    variant: &Variant,
    quality: LightmapQuality,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let atlas_expected = !quality.is_off() && variant.lightmap_failure.is_none();
    if variant.entries.lightmaps.is_some() != atlas_expected
        || variant.entries.irradiance.is_some() != atlas_expected
    {
        return Err(format!(
            "{} atlas or irradiance records are incomplete",
            quality.name()
        ));
    }
    if let Some(name) = &variant.entries.irradiance {
        let lighting = package::lighting::read_lighting(
            &reader.read_blob(&variant.entries.lighting, package::MAX_LIGHTING_BYTES)?,
        )?;
        let field = crate::engine::lighting::probes::ProbeField::read(
            &reader.read_blob(name, package::MAX_ENTRY_BYTES)?,
        )?;
        compiler::probes::append_coverage_warnings(
            &field,
            lighting.rooms().len(),
            quality.name(),
            warnings,
        );
    }
    Ok(())
}

fn verify_captures(
    reader: &mut PackageReader<File>,
    variant: &Variant,
    capture_materials: Option<&MaterialTable>,
) -> Result<(), String> {
    let points = if let Some(materials) = capture_materials {
        let mesh = package::mesh::read_mesh(
            &reader.read_blob(&variant.entries.mesh, package::MAX_BINARY_BYTES)?,
        )?;
        render::routing_from_mesh(
            &mesh,
            &MaterialRenderState::from_table(materials).reflections,
            materials.entries().len(),
        )
        .probe_points
    } else {
        Vec::new()
    };
    let expected_edges = if points.is_empty() {
        &[][..]
    } else {
        &[render::PROBE_FACE_SIZE_FULL, render::PROBE_FACE_SIZE_MEDIUM]
    };
    if !variant
        .entries
        .probes
        .iter()
        .map(|probe| probe.face_edge)
        .eq(expected_edges.iter().copied())
    {
        return Err(format!(
            "{} reflection capture records disagree with the routing points",
            variant.lightmap_quality
        ));
    }
    for payload in &variant.entries.probes {
        let recorded = world::read_probe_positions(reader, payload)?;
        if recorded.points != points
            || recorded.levels != render::ProbeFaceReadback::packaged_mip_levels(payload.face_edge)
        {
            return Err("Reflection capture positions or mip count changed".into());
        }
    }
    Ok(())
}

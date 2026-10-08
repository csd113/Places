//! Opt-in compiler provenance and read-only exports of existing lighting.
//! This module does not prepare geometry, trace rays or alter package records.

use std::io::{BufWriter, Read as _, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::lighting::lightmap::{LevelLightmaps, LightmapPage};
use crate::lighting::probes::ProbeField;
use crate::lighting::transport::{self, diagnostics as dump};
use crate::package::manifest::{Manifest, PackageDependency, Variant};
use crate::quality::LightmapQuality;

use super::{BuildRequest, VariantStats};

pub(super) struct BuildIdentity<'a> {
    pub request: &'a BuildRequest,
    pub source_sha256: &'a str,
    pub semantics: &'a [u8],
    pub catalog_sha256: &'a str,
    pub compiler_fingerprint: &'a str,
    pub lighting_fingerprint: &'a str,
    pub dependencies: &'a [PackageDependency],
    pub materials: &'a crate::materials::MaterialTable,
}

pub(super) struct VariantDump {
    _scope: dump::DumpScope,
    directories: Vec<PathBuf>,
}

/// Diagnostic failures are warnings and never change compilation results.
pub(super) fn begin_variant(
    identity: &BuildIdentity<'_>,
    quality: LightmapQuality,
) -> Option<VariantDump> {
    if std::env::var_os(dump::DUMP_ENV).is_none()
        && std::env::var_os(transport::probe_audit::DUMP_ENV).is_none()
    {
        return None;
    }
    let lighting = fresh_variant_directory(dump::DUMP_ENV, quality);
    let probes =
        if std::env::var_os(dump::DUMP_ENV) == std::env::var_os(transport::probe_audit::DUMP_ENV) {
            lighting.clone()
        } else {
            fresh_variant_directory(transport::probe_audit::DUMP_ENV, quality)
        };
    let mut directories = Vec::new();
    let mut active = [false; 2];
    for (index, directory) in [lighting, probes].into_iter().enumerate() {
        if let Some(path) = directory {
            let succeeded = if directories.contains(&path) {
                true
            } else {
                match write_build_identity(&path, identity, quality) {
                    Ok(()) => {
                        directories.push(path);
                        true
                    }
                    Err(error) => {
                        warn(&error);
                        false
                    }
                }
            };
            if let Some(slot) = active.get_mut(index) {
                *slot = succeeded;
            }
        }
    }
    Some(VariantDump {
        _scope: dump::scope_variant(
            quality,
            active.first().copied().unwrap_or(false),
            active.get(1).copied().unwrap_or(false),
        ),
        directories,
    })
}

fn fresh_variant_directory(environment: &str, quality: LightmapQuality) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os(environment)?);
    let path = root.join(quality.name());
    let result = std::fs::create_dir_all(&root).and_then(|()| std::fs::create_dir(&path));
    match result {
        Ok(()) => Some(path),
        Err(error) => {
            warn(&format!(
                "{environment}: refusing unavailable/existing {}: {error}; this variant's diagnostic output is disabled",
                path.display()
            ));
            None
        }
    }
}

fn write_build_identity(
    directory: &Path,
    identity: &BuildIdentity<'_>,
    quality: LightmapQuality,
) -> Result<(), String> {
    let options = crate::render::LightmapBuildOptions::for_lightmaps(quality);
    dump::write_json(
        directory,
        "provenance.json",
        &serde_json::json!({
            "format_version": 1_u32, "origin": "live-compiler-variant",
            "source": identity.request.source, "source_sha256": identity.source_sha256,
            "prepared_semantics_sha256": crate::package::hash::sha256_hex(identity.semantics),
            "output_package": identity.request.out, "asset_root": identity.request.asset_root,
            "catalog_sha256": identity.catalog_sha256,
            "compiler": super::COMPILER_NAME, "compiler_fingerprint": identity.compiler_fingerprint,
            "lighting_fingerprint": identity.lighting_fingerprint,
            "solver_revision": transport::SOLVER_REVISION,
            "solver_fingerprint": format!("{:016x}", transport::solver_fingerprint()),
            "geometry_revision": crate::render::GEOMETRY_REVISION,
            "quality": quality.name(), "dependencies": identity.dependencies,
            "settings": {
                "lightmaps_enabled": quality != LightmapQuality::Off,
                "config_when_enabled": {
                    "texels_per_metre": options.config.texels_per_metre,
                    "page_edge": options.config.page_edge, "max_pages": options.config.max_pages,
                    "padding": options.config.padding, "bytes_per_texel": options.config.bytes_per_texel
                },
                "legacy_shadow_taps_per_axis": options.bake.sampling.taps_per_axis,
                "legacy_prop_occlusion_cell_m": options.bake.prop_occlusion_cell_m,
                "transport_taps_per_axis": options.solve.taps_per_axis,
                "diffuse_bounces": options.solve.bounces, "gather_samples": options.solve.gather_samples,
                "worker_budget": identity.request.workers.min(super::MAX_WORKERS),
                "worker_contract": "compiler override; ordinary tests use serial fill"
            },
            "materials": "logical-materials.json (resolved compiler architecture table, including decoded colour image metadata; model sheets are separate)",
            "shadow_evidence": "caster-rays.json is exact requested-ray nearest opaque geometry; no standalone shadow/AO layer is stored",
            "completion": "result.json appears only after this variant is encoded; stage JSON appears only after its output succeeds"
        }),
    )?;
    let materials = identity.materials.entries().iter().enumerate().map(|(index, material)| serde_json::json!({
        "index": index, "id": material.id, "texture_key": material.texture_key,
        "tile_metres": material.tile_metres, "grid_metres": material.grid_metres,
        "tint": material.tint, "alpha_mode": material.alpha.mode.name(),
        "opacity": material.alpha.opacity, "alpha_cutoff": material.alpha.cutoff,
        "emission_color": material.emission.color, "emission_intensity": material.emission.intensity,
        "normal_strength": material.response.normal_strength, "specular": material.response.specular,
        "roughness": material.response.roughness,
        "decoded_color_image": material.image.as_ref().map(|image| serde_json::json!({
            "width": image.width, "height": image.height, "rgba_bytes": image.rgba.len(),
            "alpha_min": image.rgba.as_chunks::<4>().0.iter().filter_map(|pixel| pixel.get(3)).min(),
            "alpha_max": image.rgba.as_chunks::<4>().0.iter().filter_map(|pixel| pixel.get(3)).max()
        })),
        "decoded_normal_or_emission_mask": {
            "normal_texture_index": material.response.normal,
            "emission_mask_texture_index": material.emission.mask
        },
        "note": "resolved source image metadata; catalog/dependency hashes identify source PNGs; emission is appearance, not transport illumination"
    })).collect::<Vec<_>>();
    dump::write_json(directory, "logical-materials.json", &materials)
}

impl VariantDump {
    pub(super) fn finish(&self, variant: &Variant, stats: &VariantStats) {
        for directory in &self.directories {
            if let Err(error) = dump::write_json(
                directory,
                "result.json",
                &serde_json::json!({
                    "format_version": 1_u32, "variant": variant, "stats": stats,
                    "status": "variant-encoded",
                    "package_published": false,
                    "note": "compiler package publication is a later step; this records only the encoded variant"
                }),
            ) {
                warn(&error);
            }
        }
    }
}

fn warn(error: &str) {
    crate::logging::warn(format_args!("[lighting-diagnostics] {error}"));
}

/// Summary of a successful saved-package export.
#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub package: String,
    pub out: String,
    pub package_sha256: String,
    pub variants: Vec<ExportVariant>,
}

/// Available saved lighting in one exported variant.
#[derive(Debug, Serialize)]
pub struct ExportVariant {
    pub quality: String,
    pub charts: usize,
    pub probes: usize,
    pub lightmap_failure: Option<String>,
}

/// Exports exact saved atlas records and probe data without a bake or GPU.
///
/// Destination must not exist. A successful `export.json` is published last;
/// an error can leave partial files in the newly created directory.
///
/// # Errors
/// Rejects existing outputs, missing variants, malformed packages, invalid
/// payloads or filesystem failures using the normal bounded package readers.
pub fn export_lighting(
    package: &Path,
    out: &Path,
    qualities: &[LightmapQuality],
) -> Result<ExportReport, String> {
    if qualities.is_empty() {
        return Err("lighting export needs at least one variant".to_string());
    }
    let mut reader = super::open_package(package)?;
    let manifest = super::read_manifest(&mut reader)?;
    let selected = qualities
        .iter()
        .map(|quality| {
            manifest
                .variants
                .iter()
                .find(|variant| variant.lightmap_quality == quality.name())
                .ok_or_else(|| format!("package has no {} variant", quality.name()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if selected.iter().enumerate().any(|(index, variant)| {
        selected.get(..index).is_some_and(|previous| {
            previous
                .iter()
                .any(|candidate| candidate.lightmap_quality == variant.lightmap_quality)
        })
    }) {
        return Err("lighting export variants must be unique".to_string());
    }
    let package_sha256 = package_hash(package)?;
    let semantics = read_named(&mut reader, &manifest, "semantics.json")?;
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("lighting export parent: {error}"))?;
    }
    std::fs::create_dir(out).map_err(|error| {
        format!(
            "lighting export refuses existing/unavailable {}: {error}",
            out.display()
        )
    })?;
    dump::write_bytes(out, "semantics.json", &semantics)?;
    let manifest_bytes = reader.read_entry("manifest.json", crate::package::MAX_MANIFEST_BYTES)?;
    dump::write_bytes(out, "manifest.json", &manifest_bytes)?;
    dump::write_json(
        out,
        "provenance.json",
        &serde_json::json!({
            "format_version": 1_u32, "origin": "saved-package",
            "package": package, "package_sha256": package_sha256,
            "prepared_semantics_sha256": crate::package::hash::sha256_hex(&semantics),
            "created_by": manifest.created_by, "exporter": super::COMPILER_NAME,
            "compiler_fingerprint": manifest.compiler_fingerprint,
            "lighting_fingerprint": manifest.lighting_fingerprint,
            "dependencies": manifest.dependencies,
            "source_sha256": null, "catalog_sha256": null, "solver_revision": null,
            "solver_fingerprint": null, "solver_settings": null, "resolved_material_snapshot": null,
            "missing_metadata_reason": "this package format stores fingerprints and prepared semantics, not original source bytes, solver revision/settings or a resolved material table; exporter settings must not be attributed to the package",
            "intermediate_stages": { "direct": null, "indirect": null, "filtered": null, "fill": null, "shadow": null },
            "intermediate_stages_reason": "package stores final combined coefficients and switchable groups only; no intermediate decomposition can be recovered",
            "geometry_contract": "copied mesh/props/collision records are prepared runtime geometry; transport triangle ownership and exact caster rays require a live diagnostic bake",
            "completion": "export.json is written only after every requested export succeeds"
        }),
    )?;
    let mut variants = Vec::with_capacity(selected.len());
    for variant in selected {
        let directory = out.join(&variant.lightmap_quality);
        std::fs::create_dir(&directory)
            .map_err(|error| format!("lighting export variant: {error}"))?;
        variants.push(export_variant(&mut reader, &manifest, variant, &directory)?);
    }
    let report = ExportReport {
        package: package.display().to_string(),
        out: out.display().to_string(),
        package_sha256,
        variants,
    };
    dump::write_json(out, "export.json", &report)?;
    Ok(report)
}

fn read_named(
    reader: &mut crate::package::PackageReader<std::fs::File>,
    manifest: &Manifest,
    name: &str,
) -> Result<Vec<u8>, String> {
    let entry = manifest
        .entries
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| format!("lighting export entry '{name}' is undeclared"))?;
    super::read_declared_entry(reader, entry)
}

fn export_variant(
    reader: &mut crate::package::PackageReader<std::fs::File>,
    manifest: &Manifest,
    variant: &Variant,
    directory: &Path,
) -> Result<ExportVariant, String> {
    for (name, output) in [
        (&variant.entries.mesh, "mesh.record"),
        (&variant.entries.props, "props.record"),
        (&variant.entries.lighting, "lighting.record"),
        (&variant.entries.collision, "collision.record"),
    ] {
        let bytes = read_named(reader, manifest, name)?;
        // Validate the exported evidence using the ordinary runtime decoders.
        match output {
            "mesh.record" => {
                let _decoded = crate::package::mesh::read_mesh(&bytes)?;
            }
            "props.record" => {
                let _decoded = crate::package::props::read_props(&bytes)?;
            }
            "lighting.record" => {
                let _decoded = crate::package::lighting::read_lighting(&bytes)?;
            }
            "collision.record" => {
                let _decoded = crate::package::collision::read_collision(&bytes)?;
            }
            _ => return Err("unsupported diagnostic record".to_string()),
        }
        dump::write_bytes(directory, output, &bytes)?;
    }
    let charts = match (&variant.entries.lightmaps_meta, &variant.entries.lightmaps) {
        (Some(meta_name), Some(pages_name)) => {
            let meta = read_named(reader, manifest, meta_name)?;
            let pages = read_named(reader, manifest, pages_name)?;
            let atlas = crate::package::lightmaps::read_lightmaps(&meta, &pages)?;
            dump::write_bytes(directory, "lightmaps-meta.json", &meta)?;
            dump::write_bytes(directory, "lightmaps.ktx2", &pages)?;
            dump::write_charts(directory, &atlas.charts)?;
            export_atlas(directory, &atlas)?;
            atlas.charts.len()
        }
        (None, None) => 0,
        _ => return Err("lighting export requires matching lightmap metadata/pages".to_string()),
    };
    let probes = if let Some(name) = &variant.entries.irradiance {
        let bytes = read_named(reader, manifest, name)?;
        let field = ProbeField::read(&bytes)?;
        dump::write_bytes(directory, "probe-field.plpf", &bytes)?;
        export_probes(directory, &field)?;
        field.probes.len()
    } else {
        0
    };
    dump::write_json(
        directory,
        "variant.json",
        &serde_json::json!({
            "format_version": 1_u32, "origin": "saved-package", "variant": variant,
            "charts": charts, "probes": probes,
            "unavailable_lightmaps_reason": if charts == 0 { Some("saved variant has no atlas; inspect lightmap_failure and vertex colors in mesh/props records") } else { None },
            "probe_labels": "actual stored air-region IDs; room field is not necessarily an authored room index",
            "decomposition": "base combined coefficients and separate switchable contributions; no saved direct/indirect/filter/fill/shadow stages"
        }),
    )?;
    Ok(ExportVariant {
        quality: variant.lightmap_quality.clone(),
        charts,
        probes,
        lightmap_failure: variant.lightmap_failure.clone(),
    })
}

fn export_atlas(directory: &Path, atlas: &LevelLightmaps) -> Result<(), String> {
    export_group(directory, atlas, &atlas.pages, "package-final-base", None)?;
    for group in &atlas.switchable {
        export_group(
            directory,
            atlas,
            &group.pages,
            &format!("package-final-switchable-{}", group.light_index),
            Some(group.light_index),
        )?;
    }
    Ok(())
}

fn export_group(
    directory: &Path,
    atlas: &LevelLightmaps,
    pages: &[LightmapPage],
    name: &str,
    switchable: Option<usize>,
) -> Result<(), String> {
    let mut coefficients = BufWriter::new(dump::create_file(
        &directory.join(format!("{name}.coefficients-f32le")),
    )?);
    let mut rgb = BufWriter::new(dump::create_file(
        &directory.join(format!("{name}.rgb-f32le")),
    )?);
    let mut samples = 0_usize;
    for (patch, chart) in &atlas.charts {
        let page = pages
            .get(usize::from(chart.page))
            .ok_or_else(|| "lighting export chart page is missing".to_string())?;
        let normal = transport::patch_normal(patch);
        for row in 0..chart.height {
            for column in 0..chart.width {
                let texel = page
                    .texel(chart.x.saturating_add(column), chart.y.saturating_add(row))
                    .ok_or_else(|| "lighting export chart texel is missing".to_string())?;
                write_floats(
                    &mut coefficients,
                    texel
                        .irradiance
                        .iter()
                        .chain(&texel.direction)
                        .chain(&texel.axis),
                )?;
                write_floats(&mut rgb, texel.light_at(normal).iter())?;
                samples = samples
                    .checked_add(1)
                    .ok_or_else(|| "lighting export sample count overflows".to_string())?;
            }
        }
    }
    coefficients
        .flush()
        .map_err(|error| format!("lighting export coefficients: {error}"))?;
    rgb.flush()
        .map_err(|error| format!("lighting export RGB: {error}"))?;
    dump::write_json(
        directory,
        &format!("{name}.json"),
        &serde_json::json!({
            "format_version": 1_u32, "origin": "saved-package", "stage": name,
            "semantics": "final saved combined transport coefficients after package half-float quantization",
            "switchable_light_index": switchable, "base_excludes_switchable_lights": true,
            "content_key": atlas.cache_key, "samples": samples,
            "coefficients": format!("{name}.coefficients-f32le"), "coefficient_stride_f32": 8_u32,
            "coefficient_fields": ["irradiance.rgb", "summed_first_moment.xyz", "reserved_axis.xy"],
            "coefficient_encoding": "IEEE754 f32 little-endian, decoded exactly from saved RGBA16F planes",
            "data": format!("{name}.rgb-f32le"), "channel": "geometric-normal-reconstructed-linear-rgb",
            "normal": "chart geometric normal; saved chart metadata does not contain imported prop shading normals or shader normal maps",
            "order": "chart index, row, column; charts.json offset is a sample offset",
            "reconstruction": "LightmapTexel::light_at; nonlinear, so add coefficients before reconstructing combined groups"
        }),
    )
}

fn export_probes(directory: &Path, field: &ProbeField) -> Result<(), String> {
    let mut coefficients = BufWriter::new(dump::create_file(
        &directory.join("probe-coefficients.f32le"),
    )?);
    let mut labels = BufWriter::new(dump::create_file(&directory.join("probe-labels.i32le"))?);
    for probe in &field.probes {
        write_floats(
            &mut coefficients,
            probe
                .irradiance
                .iter()
                .chain(&probe.direction)
                .chain(&probe.axis),
        )?;
        labels
            .write_all(&probe.room.to_le_bytes())
            .map_err(|error| format!("probe export: {error}"))?;
    }
    coefficients
        .flush()
        .map_err(|error| format!("probe export: {error}"))?;
    labels
        .flush()
        .map_err(|error| format!("probe export: {error}"))?;
    let (_, valid) = field.summary();
    dump::write_json(
        directory,
        "probes.json",
        &serde_json::json!({
            "format_version": 1_u32, "origin": "saved-package", "record": "probe-field.plpf",
            "min": field.min, "cell_m": field.cell_m, "dims": field.dims,
            "samples": field.probes.len(), "valid_samples": valid,
            "coefficients": "probe-coefficients.f32le", "coefficient_stride_f32": 8_u32,
            "coefficient_fields": ["irradiance.rgb", "summed_first_moment.xyz", "reserved_axis.xy"],
            "labels": "probe-labels.i32le", "invalid_label": -1_i32,
            "label_semantics": "stored connected-air-region identity (ProbeSample.room); never infer authored room identity from the integer",
            "order": "x fastest, then y, then z",
            "world_position": "min + (lattice index + 0.5) * cell_m on each axis",
            "semantics": "actual saved final combined field, f32 coefficients without atlas half-float quantization; intermediate stages unavailable",
            "visualization_extension": "a later probe viewer can read this grid, validity labels and coefficients without creating or moving probes"
        }),
    )
}

fn write_floats<'a>(
    writer: &mut impl std::io::Write,
    values: impl Iterator<Item = &'a f32>,
) -> Result<(), String> {
    for value in values {
        writer
            .write_all(&value.to_le_bytes())
            .map_err(|error| format!("lighting export: {error}"))?;
    }
    Ok(())
}

fn package_hash(path: &Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("lighting export package: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("lighting export package: {error}"))?
        .len();
    if size > crate::package::MAX_TOTAL_BYTES.saturating_mul(2) {
        return Err(
            "lighting export package file exceeds bounded archive overhead allowance".to_string(),
        );
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 8192];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("lighting export hash: {error}"))?;
        if count == 0 {
            break;
        }
        let bytes = buffer
            .get(..count)
            .ok_or_else(|| "lighting export hash read exceeds buffer".to_string())?;
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::lightmap::{
        Chart, LightmapPatch, LightmapStats, LightmapTexel, PatchKind, SwitchableLightmaps,
    };
    use crate::lighting::probes::ProbeSample;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    fn directory() -> Result<PathBuf, String> {
        let path = std::env::temp_dir().join(format!(
            "places-lighting-export-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| format!("test directory: {error}"))?;
        Ok(path)
    }

    fn read_floats(path: &Path) -> Result<Vec<f32>, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let (chunks, remainder) = bytes.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err("f32 data has trailing bytes".to_string());
        }
        Ok(chunks
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect())
    }

    #[test]
    fn package_export_preserves_saved_coefficients_and_switchable_identity() -> Result<(), String> {
        let patch = LightmapPatch::from_quad(
            PatchKind::Prop,
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            Some(0),
        )
        .ok_or_else(|| "test patch rejected".to_string())?;
        let texel = LightmapTexel {
            irradiance: [0.123_45, 0.25, 0.375],
            direction: [0.0, -0.1, 0.0],
            axis: [0.5; 2],
        };
        let pages = vec![LightmapPage {
            width: 1,
            height: 1,
            texels: vec![texel],
        }];
        let atlas = LevelLightmaps {
            pages: pages.clone(),
            charts: vec![(
                patch,
                Chart {
                    page: 0,
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
            )],
            stats: LightmapStats::default(),
            cache_key: "test-content".to_string(),
            padding: 0,
            switchable: vec![SwitchableLightmaps {
                light_index: 9,
                pages,
            }],
        };
        let (metadata, image) = crate::package::lightmaps::write_lightmaps(&atlas)?;
        let saved = crate::package::lightmaps::read_lightmaps(&metadata, &image)?;
        let destination = directory()?;
        export_atlas(&destination, &saved)?;
        let actual = read_floats(&destination.join("package-final-base.coefficients-f32le"))?;
        let expected = saved
            .pages
            .first()
            .and_then(|page| page.texels.first())
            .ok_or_else(|| "saved texel missing".to_string())?;
        let expected_bits = expected
            .irradiance
            .iter()
            .chain(&expected.direction)
            .chain(&expected.axis)
            .map(|value| value.to_bits())
            .collect::<Vec<_>>();
        assert_eq!(
            actual
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_bits,
            "exports must use saved half-float-decoded coefficients"
        );
        let description: serde_json::Value = serde_json::from_slice(
            &std::fs::read(destination.join("package-final-switchable-9.json"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            description
                .get("switchable_light_index")
                .and_then(serde_json::Value::as_u64),
            Some(9_u64),
            "switchable source identity survives export"
        );
        assert_eq!(
            description
                .get("channel")
                .and_then(serde_json::Value::as_str),
            Some("geometric-normal-reconstructed-linear-rgb"),
            "package reconstruction must not claim actual prop shading normals"
        );
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())
    }

    #[test]
    fn package_probe_export_preserves_stored_labels_and_f32_data() -> Result<(), String> {
        let probe = ProbeSample {
            irradiance: [0.123_45; 3],
            direction: [0.0; 3],
            axis: [0.5; 2],
            room: 7,
        };
        let field = ProbeField {
            local_direct: None,
            min: [-1.0, 0.0, 2.0],
            cell_m: 1.5,
            dims: [2, 1, 1],
            probes: vec![probe, ProbeSample { room: -1, ..probe }],
        };
        let saved = ProbeField::read(&field.write()?)?;
        let destination = directory()?;
        export_probes(&destination, &saved)?;
        let coefficients = read_floats(&destination.join("probe-coefficients.f32le"))?;
        assert_eq!(
            coefficients.first().map(|value| value.to_bits()),
            Some(
                probe
                    .irradiance
                    .first()
                    .copied()
                    .unwrap_or_default()
                    .to_bits()
            ),
            "probe coefficients stay f32 without atlas quantization"
        );
        let labels = std::fs::read(destination.join("probe-labels.i32le"))
            .map_err(|error| error.to_string())?;
        let expected = [7_i32, -1_i32]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            labels, expected,
            "valid air-region IDs and invalid labels remain unchanged"
        );
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())
    }

    #[test]
    fn diagnostic_outputs_refuse_to_replace_existing_evidence() -> Result<(), String> {
        let destination = directory()?;
        dump::write_json(
            &destination,
            "identity.json",
            &serde_json::json!({"value": 1_u32}),
        )?;
        assert!(
            dump::write_json(
                &destination,
                "identity.json",
                &serde_json::json!({"value": 2_u32})
            )
            .is_err(),
            "existing JSON must not be replaced"
        );
        dump::write_bytes(&destination, "raw.bin", &[1, 2, 3])?;
        assert!(
            dump::write_bytes(&destination, "raw.bin", &[4]).is_err(),
            "existing raw evidence must not be replaced"
        );
        assert_eq!(
            std::fs::read(destination.join("raw.bin")).map_err(|error| error.to_string())?,
            [1, 2, 3],
            "original evidence remains intact"
        );
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())
    }
}

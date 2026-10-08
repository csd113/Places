//! Codec fixture construction stays outside guarded player loading sources.

use super::*;

fn irradiance_variant_archive(
    level: &LevelDef,
    field: &crate::lighting::probes::ProbeField,
) -> Result<(Vec<u8>, Variant), String> {
    use crate::package::navigation::{NavClass, NavGrid};
    use std::io::Write as _;
    let prepared_mesh = LevelMesh {
        ranges: Vec::new(),
        batches: crate::render::LevelMeshBatches::default(),
        vertex_count: 0,
        index_count: 0,
    };
    let compiled_collision = CompiledCollision {
        walls: Vec::new(),
        floor: crate::level::WalkableFloor::from_level(level),
        ceiling: crate::level::WalkableCeiling::from_level(level),
        water: crate::level::WaterVolumes::new(),
        ladders: crate::level::Ladders::new(),
    };
    let prepared_navigation = NavGrid {
        cell_m: 1.0,
        cells_x: 2,
        cells_z: 1,
        classes: vec![NavClass::new(0.3, 1.8, 0.3, 1.0).ok_or("navigation class")?],
        cell_y: vec![0.0; 2],
        cell_flags: vec![crate::package::navigation::CELL_SURFACE; 2],
        cell_headroom_cm: vec![300; 2],
        cell_portal: vec![crate::package::navigation::NO_PORTAL; 2],
        walkable: vec![vec![0b11]],
        region: vec![vec![0; 2]],
        ..NavGrid::default()
    };
    let records = [
        (".mesh", crate::package::mesh::write_mesh(&prepared_mesh)?),
        (".props", crate::package::props::write_props(&[])?),
        (
            ".lighting",
            crate::package::lighting::write_lighting(&LevelLighting::bake(level))?,
        ),
        (
            ".collision",
            crate::package::collision::write_collision(&compiled_collision)?,
        ),
        (
            ".navigation",
            crate::package::navigation::write_navigation(&prepared_navigation)?,
        ),
        (".irradiance", field.write()?),
    ];
    let [mesh, props, lighting, collision, navigation, irradiance] = records
        .each_ref()
        .map(|(suffix, bytes)| crate::package::hash::blob_name(bytes, suffix));
    let variant = Variant {
        lightmap_quality: "medium".to_owned(),
        quality_profile: "low".to_owned(),
        lightmap_failure: Some("codec fixture has no surface atlas".to_owned()),
        entries: crate::package::VariantEntries {
            mesh,
            props,
            lighting,
            collision,
            navigation,
            lightmaps: None,
            lightmaps_meta: None,
            irradiance: Some(irradiance),
            probes: Vec::new(),
        },
    };
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (suffix, bytes) in records {
        let name = crate::package::hash::blob_name(&bytes, suffix);
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .map_err(|error| error.to_string())?;
        archive
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
    }
    let bytes = archive
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    Ok((bytes, variant))
}

#[test]
fn variant_reader_preserves_empty_selected_v3_and_legacy_presence() -> Result<(), String> {
    use crate::lighting::lightmap::LightmapTexel;
    use crate::lighting::probes::{ProbeDirectField, ProbeField, ProbeSample};
    let level = LevelDef::from_json(r#"{"format_version":3,"id":"empty_direct_package","name":"Empty direct package",
        "spawn":{"x":0.5,"z":0.5},"rooms":[{"x":0,"z":0,"width":2,"depth":1,"height":3}],
        "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":1,"z":0.5,"align":"none","switchable":true}]}"#)
        .map_err(|error| error.to_string())?;
    for legacy in [false, true] {
        let field = ProbeField {
            min: [0.0; 3],
            cell_m: 1.0,
            dims: [2, 1, 1],
            probes: vec![
                ProbeSample {
                    irradiance: [0.25, 0.5, 0.75],
                    room: 0,
                    ..ProbeSample::default()
                };
                2
            ],
            local_direct: (!legacy).then(|| ProbeDirectField {
                light_indices: Vec::new(),
                probes: vec![LightmapTexel::ZERO; 2],
            }),
        };
        let (bytes, variant) = irradiance_variant_archive(&level, &field)?;
        let mut reader = package_reader(&bytes)?;
        let loaded = decode_variant(
            &mut reader,
            &variant,
            LightmapQuality::Medium,
            &mut crate::props::PropAssets::load_default(),
        )?;
        assert!(
            loaded.props.is_empty(),
            "normal prop codec decoded the fixture"
        );
        assert_eq!(
            loaded.lighting.lights().len(),
            1,
            "packaged switch source survives"
        );
        let decoded = loaded.irradiance.ok_or("loaded field missing")?;
        assert_eq!(
            *decoded, field,
            "variant decode preserves exact field and format presence"
        );
        if legacy {
            assert!(decoded.local_direct.is_none(), "genuine v2 stays legacy");
        } else {
            let direct = decoded.local_direct.as_ref().ok_or("v3 marker collapsed")?;
            assert_eq!(direct.light_indices, [0_u32; 0]);
            assert_eq!(direct.probes.len(), decoded.probes.len());
            assert!(
                direct
                    .probes
                    .iter()
                    .all(|sample| *sample == LightmapTexel::ZERO)
            );
        }
    }
    Ok(())
}

//! Compiler-only air-volume validation for the serialized probe lattice.

use crate::collision::WallAabb;
use crate::level::{LevelDef, LevelSurfaces};
use crate::lighting::LevelLighting;
use crate::lighting::probe_placement::placement_at;
use crate::lighting::probes::ProbeField;
use crate::lighting::transport::TransportScene;

/// Label only positions in actual air. Runtime's forgiving room lookup is
/// useful for shading boundary vertices, but must not place probes in solids.
pub(super) fn label(
    field: &mut ProbeField,
    level: &LevelDef,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
    quality: &str,
) {
    let surfaces = LevelSurfaces::new(level);
    field.assign_rooms(|position| room_at(position, &surfaces, lighting, scene, walls));
    if crate::lighting::transport::probe_audit::enabled() {
        let placements = placements(field, &surfaces, lighting, scene, walls);
        if let Err(error) =
            crate::lighting::transport::probe_audit::dump_validity(quality, &placements)
        {
            crate::logging::warn(format_args!("[probe-diagnostics] {error}"));
        }
    }
}

#[derive(serde::Serialize)]
struct Placement {
    id: usize,
    position: [f32; 3],
    room: Option<usize>,
    status: &'static str,
}

fn placements(
    field: &ProbeField,
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> Vec<Placement> {
    field
        .probes
        .iter()
        .enumerate()
        .filter_map(|(id, _)| {
            let position = field.probe_position(id)?;
            let (room, status) = placement_at(position, surfaces, lighting, scene, walls);
            Some(Placement {
                id,
                position,
                room,
                status,
            })
        })
        .collect()
}

fn room_at(
    [x, y, z]: [f32; 3],
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> Option<usize> {
    placement_at([x, y, z], surfaces, lighting, scene, walls).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level() -> Result<LevelDef, String> {
        LevelDef::from_json(
            r#"{
            "format_version":3,"id":"probe_placement","name":"Probe placement",
            "spawn":{"x":12,"z":22},
            "rooms":[{"x":10,"z":20,"width":6,"depth":6,"floor_y":10,"height":3}],
            "floor_regions":[{"x":10,"z":20,"width":2,"depth":2,"offset_y":1}]
        }"#,
        )
        .map_err(|error| error.to_string())
    }

    fn floor_charts() -> Vec<(
        crate::lighting::lightmap::LightmapPatch,
        crate::lighting::lightmap::Chart,
    )> {
        use crate::lighting::lightmap::{Chart, LightmapPatch, PatchKind};
        vec![(
            LightmapPatch {
                origin: [10.0, 10.0, 26.0],
                u_axis: [6.0, 0.0, 0.0],
                v_axis: [0.0, 0.0, -6.0],
                diagonal_correction: [0.0; 3],
                triangle: false,
                room: Some(0),
                kind: PatchKind::Floor,
            },
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
        )]
    }

    fn assert_zero_selected_spatial_field(field: &ProbeField) -> Result<(), String> {
        let sidecar = field
            .local_direct
            .as_ref()
            .ok_or("spatial sidecar missing")?;
        assert_eq!(sidecar.light_indices, [0_u32; 0]);
        assert_eq!(sidecar.probes.len(), field.probes.len());
        assert!(
            sidecar
                .probes
                .iter()
                .all(|probe| *probe == crate::lighting::lightmap::LightmapTexel::ZERO)
        );
        assert!(
            field
                .probes
                .iter()
                .any(crate::lighting::probes::ProbeSample::is_valid)
        );
        Ok(())
    }

    #[test]
    fn world_offsets_floor_regions_and_real_vertical_spans_bound_probes() -> Result<(), String> {
        let level = level()?;
        let lighting = LevelLighting::bake(&level);
        let surfaces = LevelSurfaces::new(&level);
        let scene = TransportScene::new(Vec::new(), Vec::new()).ok_or("scene")?;
        let room = |position| room_at(position, &surfaces, &lighting, &scene, &[]);
        assert_eq!(room([13.0, 11.5, 23.0]), Some(0));
        assert_eq!(room([13.0, 9.0, 23.0]), None, "below floor");
        assert_eq!(
            room([13.0, 14.0, 23.0]),
            None,
            "above ceiling despite footprint fallback"
        );
        assert_eq!(room([11.0, 10.75, 21.0]), None, "inside raised region");
        assert_eq!(room([11.0, 11.75, 21.0]), Some(0));
        assert_eq!(
            room([0.0, 11.5, 0.0]),
            None,
            "coordinates must stay world-space"
        );
        Ok(())
    }

    /// A known HDR sky value must survive every CPU stage, including real
    /// compiler air labels and the runtime entity selector.
    #[test]
    fn solved_probe_round_trips_into_runtime_entity_lighting() -> Result<(), String> {
        use crate::lighting::transport::SolveOptions;
        use crate::render::{EntityLightingSource, entity_lighting};

        let level = level()?;
        let lighting = LevelLighting::bake(&level);
        let sky = [2.0, 0.125, 0.001];
        let scene = TransportScene::new(Vec::new(), Vec::new())
            .ok_or("scene")?
            .with_sky(sky);
        let charts = floor_charts();
        let mut field = scene
            .solve_with_probes(&charts, SolveOptions::default(), None, true)
            .map_err(|error| format!("{error:?}"))?
            .probes
            .ok_or("no field")?;
        label(&mut field, &level, &lighting, &scene, &[], "test");
        assert_zero_selected_spatial_field(&field)?;
        let bytes = field.write()?;
        let decoded = ProbeField::read(&bytes)?;
        assert_zero_selected_spatial_field(&decoded)?;
        assert_eq!(
            decoded, field,
            "serialization preserves f32 coefficients and labels"
        );
        let position = [13.0, 10.75, 23.0];
        assert!(
            crate::render::entity_spatial_lighting(
                &lighting,
                Some(&decoded),
                crate::spatial::Aabb {
                    min: [-0.1; 3],
                    max: [0.1; 3]
                },
                glam::Mat4::from_translation(glam::Vec3::from_array(position)),
                Some(&scene),
            )
            .is_some(),
            "sky-only v3 uses the spatial runtime path"
        );
        let candidates = decoded.sample_diagnostics_with_rooms(position, None, |probe, label| {
            lighting.labelled_probe_visible_from(position, probe, label)
        });
        assert!(!candidates.is_empty());
        assert!(
            (candidates
                .iter()
                .map(|candidate| candidate.weight)
                .sum::<f64>()
                - 1.0)
                .abs()
                < 1.0e-12_f64
        );
        let actual = entity_lighting(&lighting, Some(&decoded), position);
        assert_eq!(actual.source, EntityLightingSource::Prepared);
        let texel = actual.prepared.ok_or("runtime fallback")?;
        for (value, expected) in texel.irradiance.into_iter().zip(sky) {
            assert!(
                (value - expected).abs() < 1.0e-5,
                "HDR energy must not be display-clamped"
            );
        }
        assert!(
            texel.direction.iter().all(|value| value.abs() < 1.0e-5),
            "antipodal uniform sky is isotropic"
        );
        for (value, expected) in texel.light_at([0.0, 1.0, 0.0]).into_iter().zip(sky) {
            assert!((value - expected).abs() < 1.0e-5);
        }
        Ok(())
    }

    #[test]
    fn authored_switch_only_probe_solve_labels_and_reload_preserve_spatial_base()
    -> Result<(), String> {
        use crate::lighting::transport::SolveOptions;
        use crate::render::{build_transport_scene, switchable_lights};

        let mut level = LevelDef::from_json(
            r#"{
            "format_version":3,"id":"switch_only_probes","name":"Switch-only probes",
            "spawn":{"x":13,"z":23},
            "rooms":[{"x":10,"z":20,"width":6,"depth":6,"floor_y":10,"height":3}],
            "ceiling_lights":[{"id":"switch_only","fixture":"core:fluorescent_panel_01",
                "x":13,"z":23,"align":"none","brightness":1,"switchable":true}]
        }"#,
        )
        .map_err(|error| error.to_string())?;
        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.lights().len(), 1);
        assert_eq!(switchable_lights(&level, &lighting), vec![0]);
        let charts = floor_charts();
        let mesh = crate::render::LevelMesh {
            ranges: Vec::new(),
            batches: crate::render::LevelMeshBatches::default(),
            vertex_count: 0,
            index_count: 0,
        };
        let materials = crate::materials::MaterialTable::default();
        let (transport_scene, stats) =
            build_transport_scene(&level, &mesh, &[], &materials, &lighting, &charts)
                .ok_or("authored switch scene")?;
        assert_eq!(stats.emitters, 1);
        assert_eq!(stats.switchable_emitters, 1);
        let scene = transport_scene.with_sky([0.0; 3]);
        let solved = scene
            .solve_with_probes(&charts, SolveOptions::default(), None, true)
            .map_err(|error| format!("{error:?}"))?;
        assert!(solved.solution.switchable.iter().any(|(_, switch_charts)| {
            switch_charts.iter().any(|chart| {
                chart
                    .texels
                    .iter()
                    .any(|texel| texel.irradiance.iter().any(|value| *value > 0.0))
            })
        }));
        let mut field = solved.probes.ok_or("switch-only field")?;
        label(&mut field, &level, &lighting, &scene, &[], "test-switch");
        assert_zero_selected_spatial_field(&field)?;
        let decoded = ProbeField::read(&field.write()?)?;
        assert_eq!(decoded, field);
        assert_zero_selected_spatial_field(&decoded)?;
        assert!(decoded.probes.iter().all(|probe| probe.room == 0_i32));

        level.ceiling_lights.clear();
        let unlit = LevelLighting::bake(&level);
        let (unlit_scene, _) =
            build_transport_scene(&level, &mesh, &[], &materials, &unlit, &charts)
                .ok_or("unlit control scene")?;
        let without_switch = unlit_scene.with_sky([0.0; 3]);
        let control = without_switch
            .solve_with_probes(&charts, SolveOptions::default(), None, true)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            solved.solution.charts, control.solution.charts,
            "switch-only energy stays outside the base atlas"
        );
        let mut control_field = control.probes.ok_or("control field")?;
        label(
            &mut control_field,
            &level,
            &unlit,
            &without_switch,
            &[],
            "test-control",
        );
        assert_eq!(
            decoded.probes, control_field.probes,
            "switch-only energy stays outside base probes"
        );
        Ok(())
    }

    #[test]
    fn wall_labels_use_height_and_preserve_clear_openings() -> Result<(), String> {
        let level = level()?;
        let lighting = LevelLighting::bake(&level);
        let surfaces = LevelSurfaces::new(&level);
        let scene = TransportScene::new(Vec::new(), Vec::new()).ok_or("scene")?;
        let walls = [WallAabb::with_y(12.0, 10.0, 20.0, 0.2, 1.0, 6.0)];
        assert_eq!(
            room_at([12.1, 10.75, 23.0], &surfaces, &lighting, &scene, &walls),
            None
        );
        assert_eq!(
            room_at([12.1, 11.75, 23.0], &surfaces, &lighting, &scene, &walls),
            Some(0),
            "air above a half wall must remain available"
        );
        assert_eq!(
            room_at([12.1, 10.75, 23.0], &surfaces, &lighting, &scene, &[]),
            Some(0),
            "an open passage has no wall solid"
        );
        Ok(())
    }

    #[test]
    fn validity_reasons_identify_real_air_constraints_independently_of_energy() -> Result<(), String>
    {
        let level = level()?;
        let lighting = LevelLighting::bake(&level);
        let surfaces = LevelSurfaces::new(&level);
        let scene = TransportScene::new(Vec::new(), Vec::new()).ok_or("scene")?;
        let walls = [WallAabb::with_y(12.0, 10.0, 20.0, 0.2, 1.0, 6.0)];
        for (position, status) in [
            ([f32::NAN, 11.5, 23.0], "nonfinite-position"),
            ([0.0, 11.5, 0.0], "outside-room-footprints"),
            ([13.0, 9.0, 23.0], "below-floor-clearance"),
            ([13.0, 14.0, 23.0], "above-ceiling-clearance"),
            ([12.1, 10.75, 23.0], "authored-wall-clearance"),
            ([13.0, 11.5, 23.0], "valid-air"),
        ] {
            assert_eq!(
                placement_at(position, &surfaces, &lighting, &scene, &walls).1,
                status
            );
        }
        let field = ProbeField {
            min: [12.5, 11.0, 22.5],
            cell_m: 1.0,
            dims: [1; 3],
            probes: vec![crate::lighting::probes::ProbeSample {
                room: 0,
                ..crate::lighting::probes::ProbeSample::default()
            }],
            local_direct: None,
        };
        let diagnostic = placements(&field, &surfaces, &lighting, &scene, &walls);
        assert_eq!(diagnostic.len(), 1);
        let placement = diagnostic.first().ok_or("placement missing")?;
        assert_eq!(placement.position, [13.0, 11.5, 23.0]);
        assert_eq!(placement.room, Some(0));
        assert_eq!(placement.status, "valid-air", "black is valid darkness");
        Ok(())
    }
}

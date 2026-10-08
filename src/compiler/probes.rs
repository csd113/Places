//! Compiler-only air-volume validation for the serialized probe lattice.

use crate::collision::WallAabb;
use crate::level::{LevelDef, LevelSurfaces};
use crate::lighting::LevelLighting;
use crate::lighting::probes::ProbeField;
use crate::lighting::transport::{PROBE_CLEARANCE_M, TransportScene};

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

fn placement_at(
    [x, y, z]: [f32; 3],
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> (Option<usize>, &'static str) {
    if ![x, y, z].iter().all(|value| value.is_finite()) {
        return (None, "nonfinite-position");
    }
    let Some(room) = lighting.room_index_at_height(x, y, z) else {
        return (None, "outside-room-footprints");
    };
    let Some(volume) = lighting.rooms().get(room) else {
        return (None, "missing-room-volume");
    };
    let floor = volume.floor_y + surfaces.walkable_offset_at(x, z);
    let ceiling = volume.ceiling_y_at(x, z);
    let clearance = PROBE_CLEARANCE_M;
    if x < volume.x0 || x > volume.x1 || z < volume.z0 || z > volume.z1 {
        return (None, "outside-room-footprint");
    }
    if y < floor + clearance {
        return (None, "below-floor-clearance");
    }
    if y > ceiling - clearance {
        return (None, "above-ceiling-clearance");
    }
    if walls.iter().any(|wall| {
        x >= wall.min_x - clearance
            && x <= wall.max_x + clearance
            && y >= wall.min_y - clearance
            && y <= wall.max_y + clearance
            && z >= wall.min_z - clearance
            && z <= wall.max_z + clearance
    }) {
        return (None, "authored-wall-clearance");
    }
    if !scene.probe_is_clear([x, y, z]) {
        return (None, "opaque-surface-clearance-or-closed-solid");
    }
    (Some(room), "valid-air")
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
        use crate::lighting::lightmap::{Chart, LightmapPatch, PatchKind};
        use crate::lighting::transport::SolveOptions;
        use crate::render::{EntityLightingSource, entity_lighting};

        let level = level()?;
        let lighting = LevelLighting::bake(&level);
        let sky = [2.0, 0.125, 0.001];
        let scene = TransportScene::new(Vec::new(), Vec::new())
            .ok_or("scene")?
            .with_sky(sky);
        let charts = [(
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
        )];
        let mut field = scene
            .solve_with_probes(&charts, SolveOptions::default(), None, true)
            .map_err(|error| format!("{error:?}"))?
            .probes
            .ok_or("no field")?;
        label(&mut field, &level, &lighting, &scene, &[], "test");
        let bytes = field.write()?;
        let decoded = ProbeField::read(&bytes)?;
        assert_eq!(
            decoded, field,
            "serialization preserves f32 coefficients and labels"
        );
        let position = [13.0, 10.75, 23.0];
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

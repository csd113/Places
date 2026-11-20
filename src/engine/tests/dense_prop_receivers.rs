//! Essential coverage of the dense prop integration. The visibility helper has
//! its own tests, but those could pass while the prop path ignored it. Two
//! probe cells and one vertex exercise the real relight with no assets or solve.

use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::probes::{ProbeField, ProbeSample};
use crate::engine::lighting::prop_receivers::relight_probe_props;
use crate::engine::render::{PropMeshBatch, Vertex};
use crate::engine::spatial::Aabb;

fn pending_batch(vertices: Vec<Vertex>) -> PropMeshBatch {
    PropMeshBatch {
        model: "dense-test".into(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: true,
        textures: Vec::new(),
        submeshes: Vec::new(),
        source_primitives: Vec::new(),
        vertices: vertices.into(),
        indices: std::sync::Arc::default(),
        bounds: Aabb {
            min: [-1.0, 0.5, -0.5],
            max: [1.0, 1.5, 0.5],
        },
    }
}

#[test]
fn dense_prop_rejects_bright_probe_behind_partition_and_keeps_passage() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"dense_prop","name":"Dense prop",
        "spawn":{"x":-1,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":0,"height":2.5}],
        "walls":[{"x":-0.55,"z":-2,"width":0.1,"depth":4,"height":2.5,
            "openings":[{"kind":"passage","offset":1.4,"width":1.2,"height":2.2}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let open = LevelLighting::bake(&level);
    let mut solid_level = level;
    solid_level
        .walls
        .first_mut()
        .ok_or("missing partition")?
        .openings
        .clear();
    let solid = LevelLighting::bake(&solid_level);
    let vertex = Vertex::new([-0.75, 1.0, -0.025], [1.0, 1.0, 1.0, 0.5], [0.0; 2]);
    let mut field = ProbeField {
        min: [-1.5, 0.5, -0.5],
        cell_m: 1.0,
        dims: [2, 1, 1],
        probes: vec![
            ProbeSample {
                room: 0,
                axis: [0.5; 2],
                ..ProbeSample::default()
            },
            ProbeSample {
                room: 0,
                axis: [0.5; 2],
                irradiance: [12.0, 0.0, 0.0],
                ..ProbeSample::default()
            },
        ],
        local_direct: None,
        switchable_indirect: Vec::new(),
    };
    let relight = |lighting: &LevelLighting, probes: Option<&ProbeField>| {
        let mut batch = pending_batch(vec![vertex]);
        relight_probe_props(std::slice::from_mut(&mut batch), lighting, probes);
        batch
            .vertices
            .get(0)
            .map(|v| v.color)
            .ok_or("missing dense vertex")
    };
    let open_color = relight(&open, Some(&field))?;
    let solid_color = relight(&solid, Some(&field))?;
    assert!(
        open_color[0] > 0.5,
        "the same bright probe must light the receiver through the opening: {open_color:?}"
    );
    assert_eq!(
        solid_color.map(f32::to_bits),
        [0.0_f32, 0.0, 0.0, 0.5].map(f32::to_bits),
        "the local dark probe remains valid when the partition occludes the bright probe"
    );

    let mut unresolved = field.clone();
    unresolved
        .probes
        .first_mut()
        .ok_or("missing local probe")?
        .room = -1_i32;
    let fallback = relight(&solid, None)?;
    assert!(
        fallback[0] > 0.0,
        "a missing field uses the established environment"
    );
    assert_eq!(
        relight(&solid, Some(&unresolved))?.map(f32::to_bits),
        fallback.map(f32::to_bits),
        "a fully rejected field uses the same fallback"
    );

    field.probes.last_mut().ok_or("missing bright probe")?.room = 1_i32;
    assert_eq!(
        relight(&open, Some(&field))?.map(f32::to_bits),
        solid_color.map(f32::to_bits),
        "the open passage never admits a falsely labelled probe"
    );
    Ok(())
}

/// An authored switch snapshot is essential coverage at the prop integration:
/// the moving probe and atlas tests cannot detect either a dark prepared prop
/// or accidental tinting of its charted neighbour. One cell, two vertices.
#[test]
fn dense_prop_records_authored_switch_bounce_and_leaves_atlas_vertex_unchanged()
-> Result<(), String> {
    use crate::engine::lighting::lightmap::LightmapTexel;
    use crate::engine::lighting::probes::{ProbeDirectField, ProbeSwitchableField};

    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"dense_switch","name":"Dense switch",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":0,"height":2.5}],
        "ceiling_lights":[
          {"id":"on","fixture":"core:fluorescent_panel_01","x":-1,"z":0,"switchable":true},
          {"id":"off","fixture":"core:fluorescent_panel_01","x":1,"z":0,"switchable":true,"enabled":false}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut lighting = LevelLighting::bake(&level);
    let base = ProbeSample {
        room: 0,
        axis: [0.5; 2],
        irradiance: [0.25; 3],
        ..ProbeSample::default()
    };
    let on = LightmapTexel {
        irradiance: [0.5, 0.25, 0.125],
        direction: [0.0, 0.0, 0.125],
        ..LightmapTexel::ZERO
    };
    let off = LightmapTexel {
        irradiance: [5.0; 3],
        ..LightmapTexel::ZERO
    };
    let field = ProbeField {
        min: [-0.5, 0.5, -0.5],
        cell_m: 1.0,
        dims: [1, 1, 1],
        probes: vec![base],
        local_direct: Some(ProbeDirectField {
            light_indices: Vec::new(),
            probes: vec![LightmapTexel::ZERO],
        }),
        switchable_indirect: vec![
            ProbeSwitchableField {
                light_index: 0,
                probes: vec![on],
            },
            ProbeSwitchableField {
                light_index: 1,
                probes: vec![off],
            },
        ],
    };
    let dense = Vertex::new([0.0, 1.0, -0.025], [1.0, 1.0, 1.0, 0.5], [0.0; 2]);
    let chart = Vertex::lightmapped(dense.pos, dense.color, dense.uv, [128, 256], 0);
    let mut batch = pending_batch(vec![dense, chart]);
    relight_probe_props(std::slice::from_mut(&mut batch), &lighting, Some(&field));
    let baked = batch.vertices.get(0).ok_or("missing dense vertex")?;
    let expected = LightmapTexel {
        irradiance: [0.75, 0.5, 0.375],
        ..on
    }
    .light_at(dense.normal);
    assert_eq!(
        baked.color.map(f32::to_bits),
        [expected[0], expected[1], expected[2], 0.5].map(f32::to_bits)
    );
    assert_eq!(
        batch.vertices.get(1),
        Some(chart),
        "atlas vertex keeps its exact tint and page"
    );
    assert!(!batch.pending_probe_lighting);

    assert!(lighting.set_light_enabled(0, false));
    relight_probe_props(std::slice::from_mut(&mut batch), &lighting, Some(&field));
    assert_eq!(
        batch.vertices.get(0),
        Some(baked),
        "compiled dense colour is an authored illumination snapshot"
    );
    let mut authored_off = pending_batch(vec![dense]);
    relight_probe_props(
        std::slice::from_mut(&mut authored_off),
        &lighting,
        Some(&field),
    );
    let control = authored_off.vertices.get(0).ok_or("missing off control")?;
    assert_eq!(
        control.color.map(f32::to_bits),
        [0.25_f32, 0.25, 0.25, 0.5].map(f32::to_bits)
    );
    Ok(())
}

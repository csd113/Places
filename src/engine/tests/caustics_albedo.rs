//! Essential regression for Static source colour: neutral model RGB must not
//! borrow excluded baked RGB, and both prepared codecs must preserve it.
//! One three-vertex source and a source-only room use no assets, transport
//! solve, shipped map or GPU; encoded records remain below one KiB each.

use super::{append_instance_vertices, lighting_sources};
use crate::engine::gltf::PropVertex;
use crate::engine::level::LevelDef;
use crate::engine::lighting::{LevelLighting, LightColor};
use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::package::{mesh, props};
use crate::engine::render::{
    LevelMesh, LevelMeshBatches, LevelMeshRange, PropMeshBatch, PropSubmeshBatch, SurfaceKey,
    SurfaceKind, Vertex,
};
use crate::engine::spatial::Aabb;

fn model_vertices(lighting: &LevelLighting) -> Vec<Vertex> {
    let source = [
        PropVertex {
            pos: [-0.2, -0.5, 0.0],
            normal: Some([0.0, 1.0, 0.0]),
            color: [0.25, 0.5, 0.75, 1.0],
            uv: [0.0, 0.0],
        },
        PropVertex {
            pos: [0.2, -0.5, 0.0],
            normal: Some([0.0, 1.0, 0.0]),
            color: [0.75, 0.5, 0.25, 1.0],
            uv: [1.0, 0.0],
        },
        PropVertex {
            pos: [0.0, -0.5, 0.2],
            normal: Some([0.0, 1.0, 0.0]),
            color: [0.5, 0.75, 0.25, 1.0],
            uv: [0.0, 1.0],
        },
    ];
    let mut vertices = Vec::new();
    append_instance_vertices(
        &mut vertices,
        &glam::Mat4::IDENTITY,
        &source,
        lighting,
        &lighting_sources(&source),
        &mut Vec::new(),
    );
    vertices
}

#[test]
fn static_model_neutral_rgb_is_independent_of_excluded_baked_rgb_and_roundtrips()
-> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"format_version":3,"id":"caustics_albedo","name":"Caustics albedo",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-1,"z":-1,"width":2,"depth":2,"floor_y":-1,"height":4}],
        "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":0,"z":0,"y":2,"brightness":2,"color":[1,0,0]}]}"#)
        .map_err(|error| error.to_string())?;
    let red = model_vertices(&LevelLighting::bake(&level));
    level
        .ceiling_lights
        .first_mut()
        .ok_or("missing source")?
        .color = Some(LightColor {
        r: 0.0,
        g: 1.0,
        b: 0.0,
    });
    let green = model_vertices(&LevelLighting::bake(&level));
    assert!(
        red.iter()
            .zip(&green)
            .any(|(a, b)| a.color.map(f32::to_bits) != b.color.map(f32::to_bits)),
        "ordinary baked lighting still changes"
    );
    assert!(
        red.iter().zip(&green).all(
            |(a, b)| a.caustics_albedo.map(f32::to_bits) == b.caustics_albedo.map(f32::to_bits)
        ),
        "excluded source RGB cannot alter the focused-light material factor"
    );
    assert_eq!(
        red.first()
            .ok_or("missing model vertex")?
            .caustics_albedo
            .map(f32::to_bits),
        [0.25_f32, 0.5, 0.75].map(f32::to_bits),
        "source-model vertex pigment survives the Static lighting path"
    );

    let bounds = Aabb {
        min: [-0.2, -0.5, 0.0],
        max: [0.2, -0.5, 0.2],
    };
    let mesh = LevelMesh {
        ranges: vec![LevelMeshRange {
            key: SurfaceKey::bare(SurfaceKind::Floor),
            vertices: red.clone(),
            indices: vec![0, 1, 2],
            bounds,
        }],
        batches: LevelMeshBatches::default(),
        vertex_count: 3,
        index_count: 3,
    };
    let mesh_encoded = mesh::write_mesh(&mesh)?;
    assert_eq!(
        mesh::read_mesh(&mesh_encoded)?.ranges,
        mesh.ranges,
        "PLMW preserves both lit and neutral factors exactly"
    );

    let batch = PropMeshBatch {
        model: "models/caustics-test.glb".to_string(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        textures: Vec::new(),
        source_primitives: Vec::new(),
        submeshes: vec![PropSubmeshBatch {
            response: MaterialResponse::default(),
            texture: None,
            emission: MaterialEmission::default(),
            alpha: MaterialAlpha::default(),
            first_index: 0,
            index_count: 3,
        }],
        vertices: red,
        indices: vec![0, 1, 2],
        bounds,
    };
    let props_encoded = props::write_props(std::slice::from_ref(&batch))?;
    let props_decoded = props::read_props(&props_encoded)?;
    assert_eq!(
        props_decoded
            .first()
            .ok_or("missing decoded batch")?
            .vertices,
        batch.vertices,
        "PLMP palette storage preserves lit and neutral factors independently"
    );
    check_legacy_props(&props_encoded, &batch.vertices)?;
    Ok(())
}

fn check_legacy_props(encoded: &[u8], expected: &[Vertex]) -> Result<(), String> {
    let regions = props::vertex_regions(encoded)?;
    let region = regions.first().ok_or("missing prop vertex region")?;
    let run = encoded
        .get(region.start..region.end)
        .ok_or("prop run truncated")?;
    let mut legacy = encoded
        .get(..region.start)
        .ok_or("prop header truncated")?
        .to_vec();
    for key in run.chunks_exact(region.stride) {
        let old_literal_bytes = if region.stride == 81 { 69 } else { 41 };
        legacy.extend_from_slice(
            key.get(..old_literal_bytes)
                .ok_or("prop literal truncated")?,
        );
        if region.stride != 81 {
            legacy.extend_from_slice(key.get(53..).ok_or("prop frame reference truncated")?);
        }
    }
    legacy.extend_from_slice(encoded.get(region.end..).ok_or("prop suffix truncated")?);
    legacy
        .get_mut(4..6)
        .ok_or("missing prop version")?
        .copy_from_slice(&8_u16.to_le_bytes());
    let decoded = props::read_props(&legacy)?;
    let old = decoded.first().ok_or("missing legacy prop batch")?;
    assert!(
        old.vertices
            .iter()
            .zip(expected)
            .all(
                |(a, b)| a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
                    && a.caustics_albedo.map(f32::to_bits) == [1.0_f32; 3].map(f32::to_bits)
            ),
        "legacy palette storage retains ordinary colours and explicitly defaults only the new factor"
    );
    Ok(())
}

#[test]
fn legacy_mesh_defaults_neutral_rgb_without_reusing_baked_colour_and_rejects_nan()
-> Result<(), String> {
    let vertex = Vertex {
        color: [0.1, 0.8, 0.2, 1.0],
        caustics_albedo: [0.3, 0.4, 0.5],
        ..Vertex::UNLIT
    };
    let mesh = LevelMesh {
        ranges: vec![LevelMeshRange {
            key: SurfaceKey::bare(SurfaceKind::Floor),
            vertices: vec![vertex],
            indices: vec![0, 0, 0],
            bounds: Aabb::from_point([0.0; 3]),
        }],
        batches: LevelMeshBatches::default(),
        vertex_count: 1,
        index_count: 3,
    };
    let mesh_encoded = mesh::write_mesh(&mesh)?;
    // One record header (66), one range header (34), then a vertex whose
    // legacy prefix is 69 bytes. Removing only the appended 12 yields PLMW2.
    let mut legacy = mesh_encoded
        .get(..169)
        .ok_or("mesh prefix truncated")?
        .to_vec();
    legacy.extend_from_slice(mesh_encoded.get(181..).ok_or("mesh suffix truncated")?);
    legacy
        .get_mut(4..6)
        .ok_or("missing version")?
        .copy_from_slice(&2_u16.to_le_bytes());
    let decoded = mesh::read_mesh(&legacy)?;
    let old = decoded
        .ranges
        .first()
        .and_then(|range| range.vertices.first())
        .ok_or("legacy vertex missing")?;
    assert_eq!(
        old.color.map(f32::to_bits),
        vertex.color.map(f32::to_bits),
        "legacy ordinary illumination remains exact"
    );
    assert_eq!(
        old.caustics_albedo.map(f32::to_bits),
        [1.0_f32; 3].map(f32::to_bits),
        "legacy factors explicitly default to white without guessing from baked RGB"
    );
    let mut invalid = mesh_encoded;
    invalid
        .get_mut(169..173)
        .ok_or("missing neutral factor")?
        .copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(
        mesh::read_mesh(&invalid).is_err(),
        "untrusted nonfinite neutral factors are rejected"
    );
    Ok(())
}

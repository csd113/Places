//! Three-vertex CPU regression: discarded lighting must not change Inspect
//! topology, source colour, alpha, normals, tangents or material/tint behavior.
//! Both normal routes use under 8 KiB; no files, assets, GPU or bake.
use super::BatchBuilder;
use crate::engine::{
    gltf::{PropModel, PropSubmesh, PropVertex},
    lighting::LevelLighting,
    materials::{MaterialAlpha, MaterialEmission, MaterialResponse},
    props::LoadedPropAsset,
    spatial::Aabb,
};

#[test]
fn inspect_skips_discarded_lighting_without_changing_vertices() {
    for normal in [Some([0.0_f32, 1.0_f32, 0.0_f32]), None] {
        let asset = tiny_asset(normal);
        let transform = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(0.7_f32),
            glam::Quat::from_rotation_y(0.4_f32),
            glam::Vec3::new(1.0_f32, 2.0_f32, 3.0_f32),
        );
        let mut reference = BatchBuilder::new("tiny", &asset.model, Vec::new());
        let mut optimized = BatchBuilder::new("tiny", &asset.model, Vec::new());
        let lighting = LevelLighting::default();
        for (target, neutral) in [(&mut reference, false), (&mut optimized, true)] {
            target.push_resolved_instance(
                &transform,
                &asset,
                &lighting,
                None,
                &Aabb::EMPTY,
                (false, neutral, Some([0.8_f32, 0.7_f32, 0.6_f32])),
            );
            crate::engine::render::common::api::illuminate_preview_vertices(&mut target.vertices);
        }
        assert_eq!(
            format!("{:?}", reference.finish()),
            format!("{:?}", optimized.finish()),
            "Inspect must retain every model attribute on either normal route"
        );
    }
}

fn tiny_asset(normal: Option<[f32; 3]>) -> LoadedPropAsset {
    let model = PropModel {
        vertices: [
            [0.0_f32, 0.0_f32, 0.0_f32],
            [1.0_f32, 0.0_f32, 0.0_f32],
            [0.0_f32, 0.0_f32, 1.0_f32],
        ]
        .map(|pos| PropVertex {
            pos,
            normal,
            color: [0.2_f32, 0.4_f32, 0.6_f32, 0.8_f32],
            uv: [0.0_f32, 1.0_f32],
        })
        .to_vec(),
        indices: vec![0_u16, 1_u16, 2_u16],
        submeshes: vec![PropSubmesh {
            double_sided: false,
            response: MaterialResponse::default(),
            material: 0_u16,
            texture: None,
            emission: MaterialEmission::NONE,
            alpha: MaterialAlpha::default(),
            first_index: 0_u32,
            index_count: 3_u32,
        }],
        triangles: 1,
        ..PropModel::default()
    };
    LoadedPropAsset {
        model_path: "tiny".into(),
        model,
    }
}

/// Protects immutable batch reuse and placement invalidation using the same
/// three-vertex fixture; under 16 KiB, milliseconds, no file/GPU/map work.
#[test]
fn environment_batch_reuse_shares_storage_and_rejects_changed_placements() -> Result<(), String> {
    use super::{PlannedBatch, PlannedInstance, preview_batch_identity, preview_instance_identity};
    use std::sync::Arc;
    let asset = Arc::new(tiny_asset(Some([0.0, 1.0, 0.0])));
    let level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"cache","name":"tiny","spawn":{"x":0,"z":0},"props":[{"model":"tiny","x":0,"z":0}]}"#).map_err(|error|error.to_string())?;
    let prop = level.props.first().ok_or("missing prop")?;
    let materials = crate::engine::materials::MaterialTable::default();
    let planned = |model| PlannedBatch {
        builder: BatchBuilder::for_prop(&asset, prop, None, true),
        instances: vec![PlannedInstance {
            model,
            asset: Arc::clone(&asset),
            bounds: Aabb::EMPTY,
            tint: None,
            identity: preview_instance_identity(prop, &model, None, &materials),
        }],
        vertex_count: 3,
        deferred: true,
    };
    let initial = planned(glam::Mat4::IDENTITY);
    let key = preview_batch_identity(&initial);
    let mut builder = initial.builder;
    builder.push_resolved_instance(
        &glam::Mat4::IDENTITY,
        &asset,
        &LevelLighting::default(),
        None,
        &Aabb::EMPTY,
        (false, false, None),
    );
    let batches = vec![builder.finish()];
    let keys = vec![key.clone()];
    let mut cache =
        crate::engine::render::common::preview_cache::PropReuse::new(Some((&keys, &batches)));
    let mut reused = cache.batch(key).ok_or("unchanged batch not reused")?;
    assert_eq!(
        reused.vertices.identity(),
        batches.first().ok_or("missing batch")?.vertices.identity()
    );
    assert!(Arc::ptr_eq(
        &reused.indices,
        &batches.first().ok_or("missing batch")?.indices
    ));
    let moved = planned(glam::Mat4::from_translation(glam::Vec3::X));
    assert!(
        cache.batch(preview_batch_identity(&moved)).is_none(),
        "changed support or transform must not reuse old vertices"
    );
    if let Some(vertex) = reused.vertices.make_mut().first_mut() {
        vertex.pos[0] = 7.0;
    }
    assert_ne!(
        reused.vertices.get(0),
        batches.first().ok_or("missing batch")?.vertices.get(0),
        "COW edits cannot mutate the displayed owner"
    );
    Ok(())
}

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
        let asset = LoadedPropAsset {
            model_path: "tiny".into(),
            model,
        };
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

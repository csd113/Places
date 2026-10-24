//! Three small regressions protect exact per-placement lighting/shape, COW
//! editing, and rejection of valid-but-different topology/attributes. Under
//! 40 KiB synthetic input; no filesystem assets, renderer, physics or bake.

use super::*;
use crate::engine::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::render::{PropSubmeshBatch, PropVertices, Vertex};

fn fixture() -> (Arc<LoadedPropAsset>, PropMeshBatch, Vec<CanonicalPlacement>) {
    let vertices: Vec<_> = (0_u16..96)
        .map(|index| PropVertex {
            pos: [f32::from(index), f32::from(index % 3), -0.0],
            normal: Some([0.0, 0.0, 1.0]),
            uv: [f32::from(index % 2), 0.0],
            color: [0.25, 0.5, 0.75, 1.0],
        })
        .collect();
    let indices: Vec<_> = (0_u16..96).collect();
    let asset = Arc::new(LoadedPropAsset {
        model_path: "synthetic/source.glb".into(),
        model: PropModel {
            vertices,
            indices,
            submeshes: vec![PropSubmesh {
                double_sided: true,
                response: MaterialResponse::default(),
                material: 0,
                texture: None,
                emission: MaterialEmission::NONE,
                alpha: MaterialAlpha::OPAQUE,
                first_index: 0,
                index_count: 96,
            }],
            textures: Vec::new(),
            triangles: 32,
            materials: 1,
            skin: None,
            joints: Vec::new(),
            weights: Vec::new(),
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
            mesh_morph_ranges: Vec::new(),
            animations: Vec::new(),
            nodes: Vec::new(),
        },
    });
    let placements: Vec<_> = [
        glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(-1.25, 0.75, 2.0),
            glam::Quat::from_rotation_y(0.37),
            glam::Vec3::new(5.0, 2.0, -7.0),
        ),
        glam::Mat4::from_translation(glam::Vec3::new(-5.0, 4.0, 1.0)),
    ]
    .into_iter()
    .map(|model| CanonicalPlacement {
        model,
        normal: model.inverse().transpose(),
        tint: [0.5, 1.0, 1.25],
        bounds: Aabb::EMPTY,
    })
    .collect();
    let expanded: Vec<_> = placements
        .iter()
        .enumerate()
        .flat_map(|(instance, placement)| {
            asset.model.vertices.iter().map(move |source| {
                let mut v = placement.vertex(source);
                v.color = if instance == 0 {
                    [32.25, -0.0, 0.5, 0.375]
                } else {
                    [0.0, 4.25, 8.5, 0.75]
                };
                v
            })
        })
        .collect();
    let batch = PropMeshBatch {
        model: asset.model_path.clone(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        textures: Vec::new(),
        source_primitives: vec![0],
        submeshes: vec![PropSubmeshBatch {
            response: MaterialResponse::default(),
            texture: None,
            emission: MaterialEmission::NONE,
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: 192,
        }],
        vertices: expanded.into(),
        indices: (0_u16..192).collect::<Vec<_>>().into(),
        bounds: Aabb::EMPTY,
    };
    (asset, batch, placements)
}

fn bits(v: Vertex) -> Vec<u32> {
    v.pos
        .into_iter()
        .chain(v.normal)
        .chain(v.uv)
        .chain(v.tangent)
        .chain([v.handedness])
        .chain(v.caustics_albedo)
        .chain(v.color)
        .map(f32::to_bits)
        .chain(v.lightmap.map(u32::from))
        .chain([u32::from(v.lightmap_page)])
        .collect()
}

#[test]
fn canonical_shape_keeps_exact_transforms_and_independent_hdr_lighting() -> Result<(), String> {
    let (asset, mut batch, placements) = fixture();
    let expected: Vec<_> = batch.vertices.iter().map(bits).collect();
    let expected_colors: Vec<_> = batch
        .vertices
        .iter()
        .map(|v| v.color.map(f32::to_bits))
        .collect();
    let matched = match_batch(&batch, &asset, &placements).ok_or("canonical fixture rejected")?;
    batch.vertices = PropVertices::packed(batch.vertices.iter().collect())?;
    batch.vertices.share(Arc::clone(&asset), matched)?;
    assert!(
        batch.vertices.shared().is_some(),
        "fixture must exercise canonical storage"
    );
    assert_eq!(
        batch.vertices.iter().map(bits).collect::<Vec<_>>(),
        expected,
        "all CPU shape and lighting bits must survive factoring"
    );
    assert!(
        Arc::ptr_eq(
            &batch
                .vertices
                .shared()
                .ok_or("missing shared vertices")?
                .asset,
            &asset
        ),
        "the verified source allocation is shared"
    );
    assert_eq!(
        batch
            .vertices
            .shared()
            .ok_or("missing shared colors")?
            .colors()
            .map(|color| color.map(f32::to_bits))
            .collect::<Vec<_>>(),
        expected_colors,
        "GPU lighting upload keeps independent HDR/alpha/signed-zero bits without reconstructing shape"
    );
    assert!(
        batch.vertices.get(batch.vertices.len()).is_none(),
        "canonical bounds stay checked"
    );
    Ok(())
}

#[test]
fn canonical_edits_detach_without_changing_another_placement_owner() -> Result<(), String> {
    let (asset, mut batch, placements) = fixture();
    batch.vertices.share(asset, placements)?;
    let original = batch.vertices.clone();
    let expected: Vec<_> = original.iter().map(bits).collect();
    if let Some(vertex) = batch.vertices.make_mut().first_mut() {
        vertex.pos = [99.0; 3];
        vertex.color = [1.0; 4];
    }
    assert!(
        batch.vertices.shared().is_none(),
        "editing must return to independent geometry"
    );
    assert_eq!(
        original.iter().map(bits).collect::<Vec<_>>(),
        expected,
        "editing cannot change shared shape or lighting owners"
    );
    Ok(())
}

#[test]
fn changed_topology_and_single_bit_shape_changes_refuse_canonical_admission() {
    let (asset, batch, placements) = fixture();
    let mut changed = batch.clone();
    if let Some(index) = Arc::make_mut(&mut changed.indices).first_mut() {
        *index = 1;
    }
    assert!(
        match_batch(&changed, &asset, &placements).is_none(),
        "same counts do not establish identical topology"
    );
    let mut changed_shape = batch;
    if let Some(vertex) = changed_shape.vertices.make_mut().first_mut() {
        vertex.pos[0] = f32::from_bits(vertex.pos[0].to_bits().saturating_add(1));
    }
    assert!(
        match_batch(&changed_shape, &asset, &placements).is_none(),
        "there is no tolerance that swallows unique geometry"
    );
}

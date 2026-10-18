//! Protects Pool's native startup failure: door meshes plus distinct portable
//! models exhausted the old sixteen-slot cache despite the object budget.
//! Eighteen untextured single triangles exercise real admission, shared meshes,
//! stable handles and eviction. Aggregate/overflow checks use lengths only.
//! No assets, GPU, physics simulation or map bake; a few KiB and milliseconds.

use std::sync::Arc;

use crate::engine::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::props::LoadedPropAsset;

use super::{
    DynamicScene, MAX_DYNAMIC_INDEX_COUNT, MAX_DYNAMIC_VERTEX_COUNT, SpawnOrientation,
    aggregate_geometry_admitted,
};

fn triangle() -> PropModel {
    PropModel {
        vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .map(|pos| PropVertex {
                pos,
                normal: None,
                color: [1.0; 4],
                uv: [0.0; 2],
            })
            .to_vec(),
        indices: vec![0, 1, 2],
        submeshes: vec![PropSubmesh {
            double_sided: false,
            response: MaterialResponse::default(),
            material: 0,
            texture: None,
            emission: MaterialEmission::default(),
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: 3,
        }],
        triangles: 1,
        materials: 1,
        ..PropModel::default()
    }
}

#[test]
fn distinct_portable_meshes_fit_beside_reserved_doors_and_keep_live_handles() -> Result<(), String>
{
    let mut scene = DynamicScene::new();
    let reserved = scene
        .register_model("door:frame", &triangle(), Vec::new(), &[])
        .ok_or("reserved door mesh refused")?;
    let door = scene
        .spawn_registered(reserved, [0.0; 3], SpawnOrientation::YAW_ONLY, 0.0, 1.0)
        .ok_or("reserved door object refused")?;
    let mut live = Vec::new();
    for index in 0usize..17 {
        let asset = Arc::new(LoadedPropAsset {
            model_path: format!("portable-{index}"),
            model: triangle(),
        });
        let id = scene
            .spawn(&asset, [0.0; 3], 0.0, 1.0, 0.0)
            .ok_or_else(|| format!("distinct portable mesh {index} refused"))?;
        live.push((id, asset));
    }
    assert_eq!(scene.mesh_count(), 18, "door and all portable models fit");
    let (removed, asset) = live.pop().ok_or("missing portable fixture")?;
    assert!(scene.despawn(removed), "portable object must despawn");
    let replacement = Arc::new(LoadedPropAsset {
        model_path: "replacement".into(),
        model: triangle(),
    });
    assert!(
        scene.spawn(&replacement, [0.0; 3], 0.0, 1.0, 0.0).is_some(),
        "unused asset mesh slot is reusable"
    );
    assert_eq!(scene.mesh_count(), 18, "eviction must not grow the table");
    assert!(
        scene.get(removed).is_none(),
        "stale object handle stays stale"
    );
    assert_eq!(
        scene.get(door).map(|object| object.mesh_index),
        Some(reserved),
        "registered door slot must stay stable"
    );
    for (id, original) in live {
        assert_eq!(
            scene.get(id).map(|object| &object.mesh.model_path),
            Some(&original.model_path),
            "live objects must keep their original mesh"
        );
    }
    assert!(
        scene.spawn(&asset, [0.0; 3], 0.0, 1.0, 0.0).is_some(),
        "a previously evicted asset can be admitted again"
    );
    Ok(())
}

#[test]
fn aggregate_dynamic_geometry_keeps_the_previous_footprint_and_refuses_overflow() {
    assert!(
        aggregate_geometry_admitted(std::iter::once((
            MAX_DYNAMIC_VERTEX_COUNT,
            MAX_DYNAMIC_INDEX_COUNT,
        )))
        .is_ok(),
        "exact aggregate geometry limits remain admissible"
    );
    for (counts, reason) in [
        ([(MAX_DYNAMIC_VERTEX_COUNT, 0), (1, 0)], "vertices"),
        ([(0, MAX_DYNAMIC_INDEX_COUNT), (0, 1)], "indices"),
        ([(usize::MAX, 0), (1, 0)], "overflow"),
    ] {
        let error = aggregate_geometry_admitted(counts.into_iter())
            .err()
            .unwrap_or_default();
        assert!(
            error.contains(reason),
            "aggregate refusal must identify {reason}; got {error}"
        );
    }
}

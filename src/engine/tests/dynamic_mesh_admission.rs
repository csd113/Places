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

fn assert_probe_path_dependency(
    lighting: &crate::engine::lighting::LevelLighting,
    bounds: crate::engine::spatial::Aabb,
) {
    use crate::engine::{
        lighting::probes::{ProbeField, ProbeSample},
        spatial::Aabb,
    };
    let field = ProbeField {
        min: [1.0, -0.5, -0.5],
        cell_m: 1.0,
        dims: [1; 3],
        probes: vec![ProbeSample {
            irradiance: [1.0; 3],
            axis: [0.5; 2],
            ..ProbeSample::default()
        }],
        local_direct: None,
        switchable_indirect: Vec::new(),
    };
    let dirty = Aabb {
        min: [0.9, 0.0, -0.05],
        max: [1.1, 0.1, 0.05],
    };
    let direct = super::EntityLightingRefreshContext::new(lighting, None, None, None, Some(1));
    assert!(
        !direct.visibility_affects(bounds, dirty),
        "the blocker misses direct source paths"
    );
    let indirect =
        super::EntityLightingRefreshContext::new(lighting, Some(&field), None, None, Some(1));
    assert!(
        indirect.visibility_affects(bounds, dirty),
        "probe-to-anchor support remains a visibility dependency"
    );
}

// Essential Pool regression: unrelated caster revisions must retain a stationary
// receiver, while contact paths, lost revisions, motion and source edits refresh.
// One untextured triangle and a tiny light fixture use no assets/GPU/map bake.
#[test]
fn receiver_visibility_tracks_relevant_bounds_and_observed_revisions() -> Result<(), String> {
    use super::EntityLightingRefreshContext;
    use crate::engine::render::common::dynamic_visibility::DynamicVisibility;
    use crate::engine::{level::LevelDef, lighting::LevelLighting, spatial::Aabb};
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"receiver","name":"Receiver",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"height":4}],
        "ceiling_lights":[{"fixture":"core:pool_light_round","x":0,"z":0,"y":3}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let bounds = Aabb {
        min: [-0.1, 0.0, -0.1],
        max: [0.1, 0.2, 0.1],
    };
    assert_probe_path_dependency(&lighting, bounds);
    let mut scene = DynamicScene::new();
    let mesh = scene
        .register_model("caster", &triangle(), Vec::new(), &[])
        .ok_or("mesh refused")?;
    let caster = scene
        .spawn_registered(mesh, [10.0, 1.0, 0.0], SpawnOrientation::YAW_ONLY, 0.0, 1.0)
        .ok_or("caster refused")?;
    let opposite = scene
        .spawn_registered(
            mesh,
            [-10.0, 1.0, 0.0],
            SpawnOrientation::YAW_ONLY,
            0.0,
            1.0,
        )
        .ok_or("opposite caster refused")?;
    let mut visibility = DynamicVisibility::new();
    assert!(visibility.sync(&scene)?);
    let key = |current_visibility: &DynamicVisibility, previous, model, revision| {
        EntityLightingRefreshContext::new(
            &lighting,
            None,
            None,
            Some(current_visibility),
            Some(revision),
        )
        .key(model, bounds, [0.0; 3], None, previous, None)
        .ok_or("key missing")
    };
    let first = key(&visibility, None, glam::Mat4::IDENTITY, 1)?;
    assert!(scene.set_transform(caster, [11.0, 1.0, 0.0], 0.0, 1.0));
    assert!(scene.set_transform(opposite, [-11.0, 1.0, 0.0], 45.0, 2.0));
    assert!(visibility.sync(&scene)?);
    let unrelated = key(&visibility, Some(first), glam::Mat4::IDENTITY, 1)?;
    assert!(
        !unrelated.spatial_changed(Some(first)),
        "off-path movement retains exact lighting"
    );
    assert!(scene.set_transform(caster, [12.0, 1.0, 0.0], 0.0, 1.0));
    assert!(visibility.sync(&scene)?);
    let repeated = key(&visibility, Some(unrelated), glam::Mat4::IDENTITY, 1)?;
    assert!(
        !repeated.spatial_changed(Some(unrelated)),
        "observed revisions advance even on reuse"
    );
    assert!(scene.set_transform(caster, [0.0, 1.0, 0.0], 0.0, 1.0));
    assert!(visibility.sync(&scene)?);
    let blocked = key(&visibility, Some(repeated), glam::Mat4::IDENTITY, 1)?;
    assert!(
        blocked.spatial_changed(Some(repeated)),
        "a newly relevant caster must refresh"
    );
    assert!(scene.set_visible(caster, false));
    assert!(visibility.sync(&scene)?);
    let vacated = key(&visibility, Some(blocked), glam::Mat4::IDENTITY, 1)?;
    assert!(
        vacated.spatial_changed(Some(blocked)),
        "old bounds restore vacated shadows"
    );
    assert!(
        key(&visibility, Some(repeated), glam::Mat4::IDENTITY, 1)?.spatial_changed(Some(repeated)),
        "missed revisions fail to a full refresh"
    );
    assert!(
        key(&visibility, Some(vacated), glam::Mat4::IDENTITY, 2)?.spatial_changed(Some(vacated)),
        "source changes invalidate"
    );
    assert!(
        key(
            &visibility,
            Some(vacated),
            glam::Mat4::from_translation(glam::Vec3::X),
            1
        )?
        .spatial_changed(Some(vacated)),
        "moving receivers invalidate"
    );
    Ok(())
}

// New shared UV-cache dependency path must match uncached anchor visibility
// for distant movement, entry, exit and removal. One opaque triangle and one
// ultraviolet fixture add milliseconds, without models, map baking or a GPU.
#[test]
fn ultraviolet_receiver_dependencies_preserve_full_refresh_visibility() -> Result<(), String> {
    use crate::engine::{
        level::LevelDef,
        lighting::{LevelLighting, transport::TransportScene},
        render::common::{
            blacklight::{BlacklightSet, BlacklightVisibility},
            dynamic_visibility::{BlacklightReceiverCache, DynamicVisibility},
        },
        spatial::Aabb,
    };
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"uv_dependency","name":"UV dependency",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"height":3}],
        "ceiling_lights":[{"id":"uv","fixture":"core:pool_light_round","x":0,"z":0,"y":2.5,"ultraviolet":1}]}"#)
        .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let sources = BlacklightSet::from_level(&level, &lighting);
    let scene = TransportScene::new(Vec::new(), Vec::new()).ok_or("empty transport refused")?;
    let bounds = Aabb {
        min: [-0.1, 0.0, -0.1],
        max: [0.1, 0.2, 0.1],
    };
    let mut model = triangle();
    for (vertex, position) in
        model
            .vertices
            .iter_mut()
            .zip([[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 0.0, 1.0]])
    {
        vertex.pos = position;
    }
    let mut dynamic = DynamicScene::new();
    let mesh = dynamic
        .register_model("UV blocker", &model, Vec::new(), &[])
        .ok_or("mesh refused")?;
    let caster = dynamic
        .spawn_registered(mesh, [10.0, 1.0, 0.0], SpawnOrientation::YAW_ONLY, 0.0, 1.0)
        .ok_or("caster refused")?;
    let mut visibility = DynamicVisibility::new();
    let mut cache = BlacklightReceiverCache::default();
    let mut first = None;
    for (position, blocked) in [
        ([10.0, 1.0, 0.0], false),
        ([-10.0, 1.0, 0.0], false),
        ([0.0, 1.0, 0.0], true),
        ([10.0, 1.0, 0.0], false),
    ] {
        let _moved = dynamic.set_transform(caster, position, 0.0, 1.0);
        assert!(visibility.sync(&dynamic)?);
        let context = BlacklightVisibility {
            sources: &sources,
            scene: &scene,
            dynamic: Some(&visibility),
        };
        let sampled = cache.sample(Some(context), bounds, glam::Mat4::IDENTITY, None);
        assert_eq!(
            sampled,
            context.receiver(bounds, glam::Mat4::IDENTITY, None)
        );
        if let Some(clear) = first {
            if blocked {
                assert_ne!(sampled, clear, "entering blocker must shade UV anchors");
            } else {
                assert_eq!(
                    sampled, clear,
                    "off-path motion and exit preserve clear anchors"
                );
            }
        } else {
            first = Some(sampled);
        }
    }
    assert!(dynamic.despawn(caster));
    assert!(visibility.sync(&dynamic)?);
    let context = BlacklightVisibility {
        sources: &sources,
        scene: &scene,
        dynamic: Some(&visibility),
    };
    assert_eq!(
        cache.sample(Some(context), bounds, glam::Mat4::IDENTITY, None),
        context.receiver(bounds, glam::Mat4::IDENTITY, None)
    );
    Ok(())
}

//! Two actors share one tiny synthetic rig while owning different gait inputs.
//! This catches the actual player/NPC fallback coupling, transform-only motion,
//! and explicit action precedence. No model files, textures, GPU or map bake.

use super::{Character, CharacterAnimator, CharacterScene, EntityFrame, PoseCue};
use crate::engine::gltf::{PropAnimation, PropModel, PropNode, PropSkin};
use crate::engine::props::LoadedPropAsset;
use crate::engine::render::common::light_transport::{EntityLighting, EntityLightingSource};
use crate::engine::scene::{LocomotionSnapshot, LocomotionState};
use crate::engine::spatial::Aabb;
use glam::{Mat4, Quat, Vec3};
use std::sync::Arc;

fn actor(id: &str, asset: Arc<LoadedPropAsset>) -> Result<Character, String> {
    let animator = CharacterAnimator::new(&asset.model).ok_or("invalid tiny rig")?;
    Ok(Character {
        ragdoll_owned: false,
        ragdoll_bounds: None,
        authored_transform: Mat4::IDENTITY,
        pose_history: Vec::new(),
        pose_delta: 0.0,
        previous_transform: Mat4::IDENTITY,
        asset,
        transform: Mat4::IDENTITY,
        animator,
        albedo: Vec::new(),
        entity_lighting: EntityLighting {
            prepared: None,
            display: [1.0; 3],
            source: EntityLightingSource::NoField,
        },
        center_light_key: None,
        lighting_refresh_key: None,
        spatial_lighting: None,
        spatial_key: None,
        static_rays: crate::engine::render::common::light_transport::EntityStaticRayCache::default(
        ),
        lighting_local_centre: Vec3::ZERO,
        lighting_local_bounds: Aabb::default(),
        balloon_color: None,
        world_bounds: Aabb::default(),
        instance_id: Some(id.into()),
        scale: 1.0,
        pose_tilt: Quat::IDENTITY,
        opacity: 1.0,
        runtime_locomotion: None,
        runtime_frame_delta: None,
        runtime_glow: None,
        runtime_visible: true,
        shadow_only: false,
    })
}

fn tiny_rig() -> Arc<LoadedPropAsset> {
    Arc::new(LoadedPropAsset {
        model_path: "tiny-rig".into(),
        model: PropModel {
            skin: Some(PropSkin {
                joints: vec![0],
                inverse_bind: vec![Mat4::IDENTITY],
                nodes: vec![PropNode {
                    name: "root".into(),
                    parent: None,
                    children: Vec::new(),
                    translation: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0; 3],
                }],
                root: Some(0),
                mesh_node: None,
            }),
            animations: ["pose_stand", "walk"]
                .into_iter()
                .map(|name| PropAnimation {
                    name: name.into(),
                    duration: 1.0,
                    channels: Vec::new(),
                    looped: true,
                    reference_speed_mps: None,
                    kind: None,
                })
                .collect(),
            ..PropModel::default()
        },
    })
}

#[test]
fn actors_own_idle_movement_and_action_independently_of_the_player() -> Result<(), String> {
    let asset = tiny_rig();
    let npc = actor("npc", Arc::clone(&asset))?;
    let mut player = actor("player", asset)?;
    player.runtime_locomotion = Some(LocomotionSnapshot {
        state: LocomotionState::Walking,
        speed: 3.0,
    });
    let mut scene = CharacterScene {
        characters: vec![npc],
        runtime: vec![player],
        ..CharacterScene::default()
    };
    let _idle_update = scene.update(0.1, &[]);
    assert!(
        scene
            .character("npc")
            .ok_or("missing NPC")?
            .animator
            .state_weight(LocomotionState::Idle)
            > 0.99,
        "a walking player must leave a stationary NPC idle"
    );
    assert!(
        scene
            .character("player")
            .ok_or("missing player")?
            .animator
            .state_weight(LocomotionState::Walking)
            > 0.0,
        "explicit player locomotion remains active"
    );
    let frame = EntityFrame {
        instance_id: "npc".into(),
        transform: Some((Vec3::new(0.1, 0.0, 0.0), 0.0)),
        cue: None,
        opacity: 1.0,
        shadow_only: false,
        glow: None,
        balloon: None,
    };
    let _motion_update = scene.update(0.1, std::slice::from_ref(&frame));
    assert!(
        scene
            .character("npc")
            .ok_or("missing moving NPC")?
            .animator
            .state_weight(LocomotionState::Walking)
            > 0.0,
        "a transform-only actor follows its own displacement"
    );
    let _stop_update = scene.update(1.0, &[]);
    assert!(
        scene
            .character("npc")
            .ok_or("missing stopped NPC")?
            .animator
            .state_weight(LocomotionState::Idle)
            > 0.99,
        "an actor without further motion returns to idle"
    );
    let _action_update = scene.update(
        0.1,
        &[EntityFrame {
            cue: Some(PoseCue::Walk { speed_mps: 1.0 }),
            ..frame
        }],
    );
    assert!(
        scene
            .character("npc")
            .ok_or("missing cued NPC")?
            .animator
            .cue
            .is_some(),
        "an authored stationary action still overrides automatic idle"
    );
    Ok(())
}

// Exact single-weight normals use a per-joint inverse transpose. This tiny rig
// protects rotated/nonuniform/reflected and singular poses against the existing
// blended-weight oracle; no model decode, bake or GPU allocation is involved.
#[test]
fn cached_rigid_normals_follow_each_pose_and_preserve_blended_fallback() -> Result<(), String> {
    let asset = tiny_rig();
    let mut animator = CharacterAnimator::new(&asset.model).ok_or("missing rig")?;
    for scale in [
        Vec3::new(1.0, 2.0, 0.5),
        Vec3::new(-1.0, 2.0, 1.0),
        Vec3::ZERO,
    ] {
        *animator.node_globals.first_mut().ok_or("missing root")? =
            Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_z(0.7), Vec3::ZERO);
        animator.compute_deltas();
        let fast = animator.skin_normal([0; 4], [1.0, 0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
        let oracle = animator.skin_normal([0; 4], [0.5, 0.5, 0.0, 0.0], [1.0, 2.0, 3.0]);
        assert!(
            (Vec3::from_array(fast) - Vec3::from_array(oracle)).length() < 1.0e-6,
            "cached normals retain the exact inverse-transpose behavior under every pose"
        );
    }
    Ok(())
}

// Protects adjacent blend reuse against wrong keys, stale poses, singular
// transforms and zero weights. A tiny synthetic rig and scalar normals add
// essential exact-output coverage without assets, GPU work or a map bake.
#[test]
fn adjacent_blended_normals_preserve_exact_results_and_pose_lifetime() -> Result<(), String> {
    let asset = tiny_rig();
    let mut animator = CharacterAnimator::new(&asset.model).ok_or("missing rig")?;
    for scale in [Vec3::new(-1.0, 2.0, 0.5), Vec3::ZERO, Vec3::splat(1.0e-8)] {
        *animator.node_globals.first_mut().ok_or("missing root")? =
            Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_z(0.7), Vec3::ZERO);
        animator.compute_deltas();
        let mut cache = super::SkinNormalCache::new(&animator);
        for (joints, weights) in [
            ([0; 4], [0.25, 0.75, 0.0, 0.0]),
            ([0; 4], [1.0, 0.0, 0.0, 0.0]),
            ([0; 4], [0.25, 0.75, 0.0, 0.0]),
            ([0; 4], [0.5, 0.5, 0.0, 0.0]),
            ([0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0]),
            ([u16::MAX; 4], [0.5, 0.5, 0.0, 0.0]),
            ([0; 4], [0.0; 4]),
            ([0; 4], [-0.0, 0.0, 0.0, 0.0]),
            ([0; 4], [1.0e-20, 1.0e-20, 0.0, 0.0]),
        ] {
            for normal in [[1.0, 2.0, 3.0], [0.0, -1.0, 0.0], [0.0; 3]] {
                let expected = animator
                    .skin_normal(joints, weights, normal)
                    .map(f32::to_bits);
                assert_eq!(
                    cache.skin_normal(joints, weights, normal).map(f32::to_bits),
                    expected
                );
            }
        }
    }
    Ok(())
}

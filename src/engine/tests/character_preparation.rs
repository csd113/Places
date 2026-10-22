//! Six vertices protect the retained wire fields and suspension catch-up.
//! The scalar rebuild oracle checks every GPU byte and exact bounds across real
//! pose/tint changes, missing data and singular scales. These two regressions
//! require no asset files, GPU, map bake or simulation; execution is sub-ms.

use super::{
    CharacterAnimator, CharacterVertexState, SkinNormalCache, WorldVertex, character_vertex,
};
use crate::engine::gltf::{
    AnimationInterpolation, AnimationPath, PropAnimation, PropAnimationChannel, PropModel,
    PropNode, PropSkin, PropVertex,
};
use crate::engine::spatial::Aabb;
use glam::Mat4;

fn model() -> PropModel {
    let nodes = vec![PropNode {
        name: "root".into(),
        parent: None,
        children: Vec::new(),
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    }];
    PropModel {
        vertices: (0_u8..6)
            .map(|slot| PropVertex {
                pos: [f32::from(slot), -0.0, 0.5],
                normal: (slot != 2).then_some([0.0, 1.0, 0.0]),
                uv: [f32::from(slot) * 0.1, 0.75],
                color: [0.25, 0.5, 0.75, 0.4],
            })
            .collect(),
        skin: Some(PropSkin {
            joints: vec![0],
            inverse_bind: vec![Mat4::IDENTITY],
            nodes,
            root: Some(0),
            mesh_node: None,
        }),
        joints: vec![[0; 4], [0; 4], [0; 4], [u16::MAX; 4], [0; 4]],
        weights: vec![
            [1.0, 0.0, 0.0, 0.0],
            [0.25, 0.75, 0.0, 0.0],
            [0.0; 4],
            [0.5, 0.5, 0.0, 0.0],
            [1.0e-20, 1.0e-20, 0.0, 0.0],
        ],
        animations: vec![PropAnimation {
            name: "pose".into(),
            duration: 1.0,
            channels: vec![
                PropAnimationChannel {
                    node: 0,
                    path: AnimationPath::Scale,
                    interpolation: AnimationInterpolation::Linear,
                    times: vec![0.0, 1.0],
                    values: vec![1.0, 2.0, 0.5, 0.0, 0.0, 0.0],
                    values_per_key: 3,
                },
                PropAnimationChannel {
                    node: 0,
                    path: AnimationPath::Translation,
                    interpolation: AnimationInterpolation::Linear,
                    times: vec![0.0, 1.0],
                    values: vec![0.0, 0.0, 0.0, 0.5, -0.2, 0.3],
                    values_per_key: 3,
                },
            ],
            looped: false,
            reference_speed_mps: None,
            kind: None,
        }],
        ..PropModel::default()
    }
}

/// Rebuilds the full wire vertex as the pre-optimization uploader did.
fn rebuild(
    model: &PropModel,
    animator: &CharacterAnimator,
    albedo: &[[f32; 4]],
) -> (Vec<WorldVertex>, Aabb) {
    let mut vertices = Vec::new();
    let mut bounds = Aabb::EMPTY;
    let mut normals = SkinNormalCache::new(animator);
    for (index, vertex) in model.vertices.iter().enumerate() {
        let joints = model.joints.get(index).copied().unwrap_or([0; 4]);
        let weights = model.weights.get(index).copied().unwrap_or([0.0; 4]);
        let position = animator.skin_position(joints, weights, vertex.pos);
        bounds.expand(position);
        let mut posed = character_vertex(
            albedo.get(index).copied().unwrap_or([1.0; 4]),
            vertex.uv,
            position,
        );
        posed.normal = vertex.normal.map_or([0.0; 3], |normal| {
            normals.skin_normal(joints, weights, normal)
        });
        vertices.push(WorldVertex::from(posed));
    }
    (vertices, bounds)
}

#[test]
fn retained_character_vertices_match_rebuild_bytes_bounds_and_tint() -> Result<(), String> {
    let model = model();
    let mut animator = CharacterAnimator::new(&model).ok_or("missing tiny rig")?;
    let mut state = CharacterVertexState::new(&model);
    let mut albedo = vec![[0.4, 0.3, 0.2, 0.6]; model.vertices.len()];
    let capacity = state.vertices.capacity();
    let allocation = state.vertices.as_ptr();
    for fraction in [0.0, 0.375, 1.0, 0.125] {
        assert!(
            animator.sample_fraction("pose", fraction),
            "sample authored pose"
        );
        assert!(
            state.prepare(&model, &animator, &albedo, true),
            "publish pose"
        );
        let (expected, bounds) = rebuild(&model, &animator, &albedo);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&state.vertices),
            bytemuck::cast_slice::<_, u8>(&expected),
            "every wire field must match the original rebuild"
        );
        assert_eq!(state.bounds, bounds, "bounds use exact submitted positions");
        assert_eq!(
            state.vertices.capacity(),
            capacity,
            "pose updates retain capacity"
        );
        assert_eq!(
            state.vertices.as_ptr(),
            allocation,
            "pose updates retain allocation"
        );
        albedo.clear(); // Missing albedo must restore the original white fallback.
    }
    Ok(())
}

#[test]
fn absent_character_keeps_pending_pose_until_restore_and_settled_pose_writes_nothing()
-> Result<(), String> {
    let model = model();
    let mut animator = CharacterAnimator::new(&model).ok_or("missing tiny rig")?;
    let mut state = CharacterVertexState::new(&model);
    assert!(state.prepare(&model, &animator, &[], true));
    let revision = state.revision;
    let vertices = state.vertices.clone();
    let bounds = state.bounds;
    for fraction in [0.25, 0.5, 0.75] {
        assert!(
            animator.sample_fraction("pose", fraction),
            "hidden clock still advances"
        );
        assert!(!state.prepare(&model, &animator, &[], false));
        assert!(!state.pending_upload, "no hidden GPU upload");
        assert_eq!(
            state.revision, revision,
            "hidden state retains stale dependency"
        );
        assert_eq!(state.vertices, vertices, "hidden pose does no vertex work");
        assert_eq!(state.bounds, bounds, "hidden bounds remain unprepared");
    }
    assert!(
        state.prepare(&model, &animator, &[], true),
        "restore latest pose before drawing, including shadow-only presence"
    );
    let (expected, restored_bounds) = rebuild(&model, &animator, &[]);
    assert_eq!(
        state.vertices, expected,
        "restore samples current clock without replay"
    );
    assert_eq!(state.bounds, restored_bounds);
    assert_eq!(state.revision, Some(animator.revision()));
    assert!(
        !state.prepare(&model, &animator, &[], true),
        "settled/unchanged ragdoll pose needs no upload"
    );
    assert!(!state.pending_upload);
    Ok(())
}

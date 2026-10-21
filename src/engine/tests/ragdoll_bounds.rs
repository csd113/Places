//! Essential conservative-visibility regression for the active ragdoll cache.
//! A single installed mesh and three extreme clip poses compare cheap joint
//! envelopes with actual weighted vertices. This uniquely guards disappearing
//! limbs while avoiding a duplicate per-frame skin pass; no GPU or map bake.
use super::{CharacterAnimator, InfluenceBounds};
use glam::{Mat4, Vec3};

#[test]
fn ragdoll_influence_envelope_contains_every_weighted_vertex() -> Result<(), String> {
    let model = crate::engine::gltf::parse_glb(include_bytes!(
        "../../../assets/entities/mannequin/model/mannequin.glb"
    ))
    .map_err(|error| error.to_string())?;
    let mut animator = CharacterAnimator::new(&model).ok_or("missing fixture animator")?;
    let influences = InfluenceBounds::new(&model, animator.deltas.len());
    let placement = Mat4::from_rotation_y(0.65);
    for clip in ["walk", "pose_arms_up", "set_down_floor"] {
        let _changed = animator.sample_fraction(clip, 0.5);
        let bounds = influences.posed(&animator, placement);
        for (index, vertex) in model.vertices.iter().enumerate() {
            let joints = model
                .joints
                .get(index)
                .copied()
                .ok_or("missing vertex joints")?;
            let weights = model
                .weights
                .get(index)
                .copied()
                .ok_or("missing vertex weights")?;
            let point = placement.transform_point3(Vec3::from_array(
                animator.skin_position(joints, weights, vertex.pos),
            ));
            for ((component, minimum), maximum) in
                point.to_array().into_iter().zip(bounds.min).zip(bounds.max)
            {
                assert!(
                    component >= minimum - 0.0002 && component <= maximum + 0.0002,
                    "{clip} must retain every weighted limb in conservative bounds"
                );
            }
        }
    }
    Ok(())
}

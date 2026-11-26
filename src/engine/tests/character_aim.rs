//! A CPU-only oracle checks exact candidate socket transforms against the
//! full-pose path on the shipped rig, including an interrupted gait crossfade.
//! Unlike clearance tests it catches changed aim scores and stray pose writes.
//! Two poses and 17 weights each, without GPU, level load, or bake.

use super::CharacterAnimator;

#[test]
fn inspection_candidates_match_full_pose_without_mutating_it() -> Result<(), String> {
    let model = crate::engine::gltf::parse_glb(include_bytes!(
        "../../../assets/entities/tall_guest/model/tall_guest.glb"
    ))
    .map_err(|error| error.to_string())?;
    let mut guest = CharacterAnimator::new(&model).ok_or("missing Tall Guest animator")?;
    let mut reference = CharacterAnimator::new(&model).ok_or("missing reference animator")?;
    for clip in ["run_pursuit", "walk_slow"] {
        assert!(
            guest
                .clip_duration(clip)
                .is_some_and(|duration| duration > 0.0),
            "the fixture must provide the gait clip {clip}"
        );
    }
    let socket = "acoustic_origin";
    let weights = (0_u8..=16).map(|step| f32::from(step) / 20.0);
    for (run, walk, fade, posture) in [(0.11, 0.43, 1.0, 0.0), (0.38, 0.72, 0.4, 0.3)] {
        let sample_base = |animator: &mut CharacterAnimator| {
            let _sampled =
                animator.sample_blended_fraction("run_pursuit", run, "walk_slow", walk, fade);
            if posture > 0.0 {
                assert!(
                    animator.blend_upper_clip("walk_slow", run, posture),
                    "base posture must be layered"
                );
            }
        };
        sample_base(&mut guest);
        let base = guest.node_global(socket).ok_or("missing aim socket")?;
        let revision = guest.revision;
        let mut candidates = 0_u8;
        let mut socket_moved = false;
        guest.visit_upper_inspection_node_globals(socket, weights.clone(), |weight, candidate| {
            sample_base(&mut reference);
            assert_eq!(
                reference.blend_upper_inspection(weight),
                weight > 0.0,
                "old path applied at {weight}"
            );
            assert_eq!(candidate, reference.node_global(socket), "weight {weight}");
            candidates = candidates.saturating_add(1);
            socket_moved |= candidate != Some(base);
        });
        assert_eq!(candidates, 17, "all weights must be visited");
        assert!(socket_moved, "inspection must affect the acoustic socket");
        assert_eq!(
            guest.node_global(socket),
            Some(base),
            "base transform changed"
        );
        assert_eq!(guest.revision, revision, "visible pose revision changed");
    }
    Ok(())
}

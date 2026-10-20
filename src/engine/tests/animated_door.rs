//! Delivered rigid-clip regression: a 792-triangle asset and nine node boxes,
//! no full map, GPU or bake. Protects shared pose ownership, blocked prospective
//! motion, mid-travel reversal and oriented clearance at negligible CPU cost.

use crate::engine::animated_door::AnimatedDoor;
use crate::engine::entity::PoseCue;
use crate::engine::level::PropDef;
use crate::engine::render::CharacterAnimator;

fn fixture(yaw: f32) -> Result<(AnimatedDoor, CharacterAnimator), String> {
    let mut prop: PropDef = serde_json::from_str(include_str!("fixtures/folding_door.json"))
        .map_err(|error| error.to_string())?;
    prop.x = 0.0;
    prop.z = 0.0;
    prop.rotation_degrees = yaw;
    let model = crate::engine::gltf::parse_glb(include_bytes!(
        "../../../assets/environments/home/props/closet_folding_door.glb"
    ))
    .map_err(|error| error.to_string())?;
    let animator = CharacterAnimator::new(&model).ok_or("missing rigid animator")?;
    Ok((
        AnimatedDoor::new("closet".into(), &prop, 0.0, &model)?,
        animator,
    ))
}

fn progress(door: &AnimatedDoor) -> f32 {
    if let PoseCue::Sample { progress, name: _ } = door.cue() {
        progress
    } else {
        -1.0
    }
}

#[test]
fn clip_pose_collision_reversal_and_repeated_cycles_share_one_clock() -> Result<(), String> {
    let (mut door, mut visual) = fixture(0.0)?;
    assert!(
        door.colliders()
            .iter()
            .any(|shape| shape.overlaps_body_y(0.0, 1.8) && shape.overlaps_disc(0.0, 0.0, 0.3)),
        "closed leaves block the centre"
    );
    for _ in 0_u8..3 {
        assert!(
            door.request("fold_open", true),
            "matching clip request succeeds"
        );
        let _moved = door.advance(0.1, |_| false);
        let before = progress(&door);
        assert!(
            before > 0.0 && before < 0.1,
            "authored 1.4 second duration is retained"
        );
        let _posed = visual.update_cued(99.0, &door.cue());
        let pose = visual.node_global("Pivot_L_Jamb").ok_or("missing jamb")?;
        let leaf = door.colliders().first().ok_or("missing leaf")?;
        let center = pose.transform_point3(glam::Vec3::new(0.218, 1.05, 0.003));
        assert!(
            (leaf.dir_x.mul_add(leaf.width * 0.5, leaf.hinge_x) - center.x).abs() < 1.0e-5,
            "visual and collider sample the exact same node"
        );
        assert!(
            door.request("fold_open", true),
            "mid-opening reversal succeeds"
        );
        let _reversed = door.advance(0.025, |_| false);
        assert!(
            progress(&door) < before && progress(&door) > 0.0,
            "reversal starts from accepted progress"
        );
        assert!(
            door.request("fold_open", true),
            "reverse again without resetting"
        );
        for _ in 0_u8..15 {
            let _step = door.advance(0.1, |_| false);
        }
        assert!((progress(&door) - 1.0).abs() < 1.0e-5, "fully open settles");
        assert!(
            !door
                .colliders()
                .iter()
                .any(|shape| shape.overlaps_body_y(0.0, 1.8) && shape.overlaps_disc(0.0, 0.0, 0.3)),
            "folded leaves leave real player clearance"
        );
        assert!(door.request("fold_open", true), "close request succeeds");
        for _ in 0_u8..15 {
            let _step = door.advance(0.1, |_| false);
        }
        assert!(
            progress(&door).abs() < 1.0e-5,
            "closed settles on every cycle"
        );
    }
    Ok(())
}

#[test]
fn prospective_obstruction_holds_pose_and_yawed_instances_remain_independent() -> Result<(), String>
{
    let (mut door, _) = fixture(37.0)?;
    let (mut other, _) = fixture(0.0)?;
    assert!(door.request("fold_open", true), "start yawed mechanism");
    let before = door.colliders().to_vec();
    assert!(
        !door.advance(0.1, |_| true),
        "obstruction refuses prospective movement"
    );
    assert_eq!(
        door.colliders(),
        before,
        "no leaf advances into an obstruction"
    );
    assert!(door.obstructed, "obstruction is observable");
    for _ in 0_u8..15 {
        let _step = door.advance(0.1, |_| false);
    }
    assert!(
        progress(&door) > 0.999,
        "cleared obstruction permits travel"
    );
    assert!(
        progress(&other).abs() < 1.0e-6,
        "second instance stays closed"
    );
    assert!(door.request("fold_open", true), "request closing");
    let open = progress(&door);
    let _blocked = door.advance(0.1, |_| true);
    assert!(
        (progress(&door) - open).abs() < 1.0e-6,
        "occupied close retains accepted progress"
    );
    assert!(
        door.request("fold_open", true),
        "interrupted close can reopen"
    );
    door.reset();
    other.reset();
    assert!(
        progress(&door).abs() < 1.0e-6,
        "reset restores authored closed state"
    );
    Ok(())
}

#[test]
fn closing_leaf_sweep_tests_small_carried_shape_at_hand_height() -> Result<(), String> {
    let (mut door, _) = fixture(0.0)?;
    let obstacle =
        crate::engine::collision::WallAabb::with_y(0.28094, 1.555, 0.14494, 0.1178, 0.09, 0.0792);
    assert!(
        door.request("fold_open", true),
        "open for carried-shape check"
    );
    for _ in 0_u8..15 {
        let _step = door.advance(0.1, |_| false);
    }
    assert!(
        door.request("fold_open", true),
        "close toward carried shape"
    );
    for _ in 0_u8..15 {
        let _step = door.advance(0.1, |leaf| {
            crate::engine::entities::door_pose_hits_box(leaf, &obstacle)
        });
    }
    assert!(
        door.obstructed && progress(&door) > 0.1,
        "the complete prospective leaf pose is checked against an elevated small prop"
    );
    Ok(())
}

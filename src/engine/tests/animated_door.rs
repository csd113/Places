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

// Delivered exit exports add a distinct host-fit risk absent from the folding
// fixture: two mirrored 90-degree leaves, shallow frame returns and overhead
// closer links. Two tiny rigs, sampled with real wall solids, need no map bake.
#[test]
fn exit_doors_fit_host_and_clear_real_aperture_after_reversible_motion() -> Result<(), String> {
    use crate::engine::{collision::WallAabb, entities::door_pose_hits_box};
    for (variant, bytes, width) in [
        (
            "single",
            include_bytes!("../../../assets/shared/doors/hollow_metal_exit_door_single.glb")
                .as_slice(),
            1.0_f32,
        ),
        (
            "double",
            include_bytes!("../../../assets/shared/doors/hollow_metal_exit_door_double.glb")
                .as_slice(),
            1.9_f32,
        ),
    ] {
        let id = format!("core:hollow_metal_exit_door_{variant}");
        let prop: PropDef =
            serde_json::from_value(crate::engine::animated_door::preset(&id).ok_or("preset")?)
                .map_err(|error| error.to_string())?;
        let model = crate::engine::gltf::parse_glb(bytes).map_err(|error| error.to_string())?;
        let mut door = AnimatedDoor::new(variant.into(), &prop, 0.0, &model)?;
        let walls = [
            WallAabb::with_y(-3.0, 0.0, -0.08, width.mul_add(-0.5, 3.0), 3.0, 0.16),
            WallAabb::with_y(width * 0.5, 0.0, -0.08, 3.0, 3.0, 0.16),
            WallAabb::with_y(-width * 0.5, 2.15, -0.08, width, 0.85, 0.16),
        ];
        assert!(
            door.colliders()
                .iter()
                .any(|leaf| leaf.overlaps_body_y(0.0, 1.8) && leaf.overlaps_disc(0.0, 0.0, 0.4)),
            "{variant}: closed passage blocks"
        );
        exit_sight_gate(&door, &walls, false);
        assert!(door.request_end(true), "open request");
        for _ in 0_u8..12 {
            let _moved = door.advance(0.1, |leaf| {
                walls.iter().any(|wall| door_pose_hits_box(leaf, wall))
            });
        }
        assert!(
            progress(&door) > 0.999,
            "{variant}: host must not obstruct its own moving hardware: {}",
            door.diagnostics()
        );
        for z in [-0.6_f32, -0.2, 0.0, 0.2, 0.6] {
            assert!(
                !door
                    .colliders()
                    .iter()
                    .any(|leaf| leaf.overlaps_body_y(0.0, 1.8)
                        && crate::engine::collision::square_overlaps_door(
                            glam::Vec2::new(0.035, z),
                            0.4,
                            leaf
                        )),
                "{variant}: full-open player-sized path at {z}"
            );
        }
        exit_sight_gate(&door, &walls, true);
        exit_contact_gate(prop, variant, door.cue())?;
        assert!(door.request_end(false), "close request");
        let before = progress(&door);
        assert!(!door.advance(0.1, |_| true), "prospective blocker holds");
        assert_eq!(progress(&door), before, "accepted pose remains shared");
        assert!(door.toggle(), "mid-close reverse");
        door.reset();
        assert!(progress(&door).abs() < 1.0e-6, "reset closed");
        exit_sight_gate(&door, &walls, false);
    }
    Ok(())
}

// Common actions previously ignored every clip door. One small world protects
// lock/toggle/condition/reset compatibility and both live collider publication
// and clearance revision, in milliseconds without a GPU or prepared level.
#[test]
fn compound_doors_share_actions_conditions_clearance_and_reset() -> Result<(), String> {
    use crate::engine::{
        collision_index::CollisionIndex,
        entities::{EntityWorld, WorldContext},
        level::{ActionDef, WalkableFloor},
    };
    let preset = crate::engine::animated_door::preset("core:hollow_metal_exit_door_double")
        .ok_or("preset")?;
    let prop: PropDef = serde_json::from_value(preset).map_err(|error| error.to_string())?;
    let mut level = exit_level(prop)?;
    let script = add_exit_probe(&mut level)?;
    let mut world = EntityWorld::from_level(&level);
    let id = "review_hollow_metal_exit_double";
    exercise_compound_lock(&mut world, id);
    let index = CollisionIndex::empty();
    let floor = WalkableFloor::from_level(&level);
    let ctx = WorldContext {
        delta_seconds: 0.1,
        player_walk_speed: 3.0,
        feet_from: glam::Vec3::new(3.0, 0.0, 3.0),
        feet: glam::Vec3::new(3.0, 0.0, 3.0),
        eye: glam::Vec3::new(3.0, 1.7, 3.0),
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    for _ in 0_u8..12 {
        let _moved = world.update_doors(&ctx);
    }
    assert!(
        world
            .door_status(id)
            .is_some_and(|status| status.phase == crate::engine::door::DoorPhase::Open),
        "settled state published"
    );
    assert!(
        world.doors().version() > 0,
        "navigation sees changed clearance"
    );
    assert!(
        !world
            .door_colliders()
            .iter()
            .any(|leaf| leaf.overlaps_body_y(0.0, 1.8) && leaf.overlaps_disc(0.0, 0.0, 0.3)),
        "no stale closed blocker"
    );
    let _probe = world.dispatch_on(
        id,
        &[ActionDef::RunScript {
            script,
            handler: "probe".into(),
        }],
    );
    assert!(
        !world.script_failed(),
        "actual Luau common door query accepts the compound clip"
    );
    assert!(
        world
            .door_status(id)
            .is_some_and(|status| status.phase == crate::engine::door::DoorPhase::Closing),
        "Luau close uses the same dispatcher"
    );
    world.reset_runtime();
    assert!(
        world.door_status(id).is_some_and(
            |status| !status.locked && status.phase == crate::engine::door::DoorPhase::Closed
        ),
        "authored lock and pose restored"
    );
    Ok(())
}

fn exit_level(prop: PropDef) -> Result<crate::engine::level::LevelDef, String> {
    let mut level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"exit_fixture","name":"Exit fixture","spawn":{"x":0,"z":3},"rooms":[{"x":-4,"z":-4,"width":8,"depth":8}]}"#).map_err(|error| error.to_string())?;
    level.props.push(prop);
    Ok(level)
}

fn exit_contact_gate(prop: PropDef, variant: &str, cue: PoseCue) -> Result<(), String> {
    let level = exit_level(prop)?;
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let mut assets = crate::engine::props::PropAssets::load_default();
    let mut scene = crate::engine::render::CharacterScene::spawn_characters(
        &level,
        &catalog,
        &mut assets,
        &crate::engine::lighting::LevelLighting::default(),
    );
    let instance_id = format!("review_hollow_metal_exit_{variant}");
    let _updated = scene.update(
        0.0,
        &[crate::engine::entity::EntityFrame {
            instance_id: instance_id.clone(),
            rigid_transform: None,
            transform: None,
            cue: Some(cue),
            opacity: 1.0,
            shadow_only: false,
            glow: None,
            balloon: None,
        }],
    );
    let visual = scene.character(&instance_id).ok_or("posed exit visual")?;
    let contacts: Vec<_> = visual.contact_subject_bounds().collect();
    assert_eq!(
        contacts.len(),
        if variant == "single" { 3 } else { 4 },
        "only two jambs and actual leaves ground; threshold/hardware do not double-darken the floor"
    );
    assert!(
        contacts.iter().all(|bounds| !(bounds.min[0] < 0.0
            && bounds.max[0] > 0.0
            && bounds.min[2] < 0.0
            && bounds.max[2] > 0.0)),
        "no broad fake contact beneath the empty aperture"
    );
    assert!(
        visual.posed_bounds().min[2] < -0.9,
        "visual really follows open leaf beyond closed-frame bounds"
    );
    Ok(())
}

fn add_exit_probe(
    level: &mut crate::engine::level::LevelDef,
) -> Result<crate::engine::project::ProjectPath, String> {
    let script = crate::engine::project::ProjectPath::new("scripts/door_probe.luau".into())?;
    level.script_sources.push(crate::engine::scripting::ScriptSource { path: script.clone(), source: "return {probe=function(api) local d=api.door_status('review_hollow_metal_exit_double'); assert(d.phase=='open' and not d.locked and d.progress>.999 and d.angle==nil); api.door('review_hollow_metal_exit_double','close'); api.message('Compound query',{'passed'}) end}".into() });
    Ok(script)
}

fn exercise_compound_lock(world: &mut crate::engine::entities::EntityWorld, id: &str) {
    use crate::engine::level::{ActionDef, ConditionDef};
    let _locked = world.dispatch_actions(
        &[ActionDef::Lock {
            target: Some(id.into()),
        }],
        None,
    );
    assert!(
        world.condition_holds(&ConditionDef::Locked { target: id.into() }),
        "shared lock query"
    );
    let refused = world.dispatch_actions(
        &[ActionDef::Open {
            target: Some(id.into()),
        }],
        None,
    );
    assert_eq!(refused.doors_acted, 0, "locked opening refused");
    let _unlocked = world.dispatch_actions(
        &[
            ActionDef::Unlock {
                target: Some(id.into()),
            },
            ActionDef::Toggle {
                target: Some(id.into()),
            },
        ],
        None,
    );
}

fn exit_sight_gate(door: &AnimatedDoor, walls: &[crate::engine::collision::WallAabb], clear: bool) {
    let index = crate::engine::collision_index::CollisionIndex::build(walls);
    assert_eq!(
        crate::engine::ai::perception::sight_clear_with_leaves(
            &index,
            walls,
            door.colliders(),
            glam::Vec3::new(0.035, 1.6, 1.0),
            glam::Vec3::new(0.035, 1.6, -1.5),
        ),
        clear,
        "real AI sight shares the accepted physical pose, including after reset"
    );
}

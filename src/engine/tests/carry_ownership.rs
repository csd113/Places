//! One held sphere and one unrelated body protect the handoff ownership
//! transaction: wrong/repeated handovers cannot release, clone or consume items.
//! No assets, GPU, map bake or simulation campaign; cost is milliseconds.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext};
use crate::engine::interact::nearest_target;
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};
use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef};

#[test]
fn handoff_transfers_only_the_exact_currently_held_body_once() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"carry","name":"Carry","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
    let mut world = ItemWorld::from_level(&level, &[]);
    let definition = PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        shape: Some(ShapeDef::Sphere { radius: 0.1 }),
        ..PhysicsDef::default()
    };
    let bounds = (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1));
    world.spawn_authored(1, &definition, bounds, Vec3::new(0.0, 1.4, -1.0), 0.0, 1.0)?;
    world.spawn_authored(2, &definition, bounds, Vec3::new(2.0, 1.4, -1.0), 0.0, 1.0)?;
    assert!(
        !world.transfer_held(1),
        "an unheld body cannot be claimed by an NPC"
    );
    assert!(
        world.pickup(1, Vec3::new(0.0, 1.5, 0.0), Vec3::NEG_Z, 0.0),
        "fixture sphere is reachable"
    );
    assert!(
        !world.transfer_held(2),
        "the wrong object must leave ownership intact"
    );
    assert_eq!(
        world.held(),
        Some(1),
        "a refusal must preserve the player's actual hold"
    );
    assert!(
        world.transfer_held(1),
        "the exact held item transfers atomically"
    );
    assert!(world.held().is_none(), "no stale player ownership remains");
    assert!(
        !world.contains_body(1),
        "the handed item no longer has pickup or simulation ownership"
    );
    assert!(world.contains_body(2), "unrelated items remain physical");
    assert!(
        !world.transfer_held(1),
        "repeated interaction cannot consume another item"
    );
    Ok(())
}

#[test]
fn retired_carry_actor_reset_restores_interaction_navigation_and_empty_hands() -> Result<(), String>
{
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"carry_reset","name":"Carry reset","spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-4,"width":12,"depth":8}],"props":[
      {"id":"caretaker","model":"mannequin","x":0,"z":-2,"size":[0.6,1.8,0.4],"solid":false,"components":[
        {"component":"interactable","prompt":"Talk"},
        {"component":"nav_agent","radius":0.2,"speed_mps":1,"height":1.8},
        {"component":"ai","behavior":"idler"},
        {"component":"carry","item":"requested","socket":"hand.R","idle_clip":"carry_idle","walk_clip":"carry_walk"}
      ]},
      {"id":"requested","model":"core:crate","x":2,"physics":{"motion":"dynamic","pickup":true}}
    ],"sequences":[{"id":"leave","steps":[{"step":"move","x":6,"z":-2,"speed":3}]}]}"#).map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let handle = world.handle_of("caretaker").ok_or("missing caretaker")?;
    world.confirm_handoff("caretaker", "requested");
    move_caretaker(&mut world, &WalkableFloor::from_level(&level));
    assert!(
        aimed_at(&world, 6.0).is_some() && aimed_at(&world, 0.0).is_none(),
        "ordinary movement must publish the departed aim proxy before reset"
    );
    world.confirm_carry_retired("caretaker");
    assert!(
        world
            .components()
            .carries
            .get(handle)
            .is_some_and(|carry| carry.retired),
        "hidden retirement is recorded"
    );
    world.reset_runtime();
    let carry = world
        .components()
        .carries
        .get(handle)
        .ok_or("missing reset carry")?;
    assert!(
        !carry.retired && carry.item.is_none(),
        "full reset restores authored ownership"
    );
    assert!(
        world
            .ai()
            .agent("caretaker")
            .is_some_and(|agent| !agent.frozen()),
        "the authored AI survives retirement and resets"
    );
    assert!(
        world
            .interactables()
            .items()
            .iter()
            .any(|item| item.id == "caretaker" && item.enabled),
        "the player can repeat the quest after reset"
    );
    assert_eq!(
        aimed_at(&world, 0.0),
        Some("caretaker"),
        "reset must restore aim bounds immediately, even for a freshly idle AI"
    );
    assert!(
        aimed_at(&world, 6.0).is_none(),
        "the departed interaction proxy cannot survive reset"
    );
    Ok(())
}

fn aimed_at(world: &EntityWorld, x: f32) -> Option<&str> {
    let items = world.interactables().items();
    nearest_target(Vec3::new(x, 1.5, 0.0), Vec3::NEG_Z, items, &[], &[])
        .and_then(|index| items.get(index))
        .map(|item| item.id.as_str())
}

// Two real movement ticks reproduce the stale reset proxy without assets,
// navigation baking or a GPU. The existing reset test now covers this boundary.
fn move_caretaker(world: &mut EntityWorld, floor: &WalkableFloor) {
    let report = world.dispatch_on(
        "caretaker",
        &[ActionDef::StartSequence {
            sequence: "leave".to_string(),
            target: None,
        }],
    );
    assert_eq!(report.sequences_started, 1);
    let index = CollisionIndex::empty();
    for _ in 0_u8..2 {
        let _tick = world.tick(&WorldContext {
            delta_seconds: 1.0,
            feet_from: Vec3::ZERO,
            feet: Vec3::ZERO,
            eye: Vec3::new(0.0, 1.6, 0.0),
            body_height: 1.8,
            body_half: crate::engine::collision::PLAYER_RADIUS,
            form_model: None,
            walls: &[],
            index: &index,
            floor,
            nav: None,
        });
    }
}

// Repeated pickup/putdown must reuse one key and reset to the original source
// location, not the last socket. One small sphere, no renderer or map bake.
#[test]
fn released_npc_item_keeps_original_reset_pose_and_single_key() -> Result<(), String> {
    use crate::engine::physics::ItemPose;
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"release","name":"Release","spawn":{"x":8,"z":8}}"#).map_err(|error|error.to_string())?;
    let mut physics = ItemWorld::from_level(&level, &[]);
    let definition = PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        reset_group: Some("parcel".into()),
        shape: Some(ShapeDef::Sphere { radius: 0.1 }),
        ..PhysicsDef::default()
    };
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    let authored = ItemPose {
        position: Vec3::new(0.0, 1.0, 0.0),
        rotation: glam::Quat::IDENTITY,
    };
    let live = ItemPose {
        position: Vec3::new(2.0, 1.0, 0.0),
        ..authored
    };
    physics.restore_transferred(7, &definition, bounds, authored, live, 1.0)?;
    assert_eq!(
        physics.snapshots().len(),
        1,
        "one original key, one solver body"
    );
    assert!(
        physics
            .restore_transferred(7, &definition, bounds, authored, live, 1.0)
            .is_err(),
        "duplicate release cannot replace live ownership"
    );
    let report = physics.reset_group("parcel")?;
    assert_eq!(report.reset_count, 1);
    let body = physics
        .snapshots()
        .into_iter()
        .next()
        .ok_or("missing released body")?;
    assert!(
        body.pose.position.distance(authored.position) < 0.001,
        "reset retains original source placement"
    );
    Ok(())
}

// Explicit actor poses previously disappeared behind the idle AI cue. One
// asset-free actor protects pose ownership, completion, navigation and reset;
// no nav bake or renderer is needed, and two ticks cost milliseconds.
#[test]
fn explicit_actor_pose_releases_to_locomotion_and_reset() -> Result<(), String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"pose","name":"Pose","spawn":{"x":4,"z":4},"rooms":[{"width":5,"depth":5}],"props":[{"id":"actor","model":"mannequin","x":1,"z":1,"solid":false,"components":[{"component":"animation","clip":"pose_stand"},{"component":"nav_agent","radius":0.2,"speed_mps":1},{"component":"ai","behavior":"idler"}]}]}"#).map_err(|error|error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let index = CollisionIndex::empty();
    let floor = WalkableFloor::from_level(&level);
    let context = WorldContext {
        delta_seconds: 0.01,
        feet_from: Vec3::new(4.0, 0.0, 4.0),
        feet: Vec3::new(4.0, 0.0, 4.0),
        eye: Vec3::new(4.0, 1.6, 4.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let action = ActionDef::PlayAnimation {
        target: Some("actor".into()),
        clip: Some("pick_up_floor".into()),
        looped: false,
    };
    let _performed = world.dispatch_actions(std::slice::from_ref(&action), None);
    let _first = world.tick(&context);
    assert!(
        world
            .ai()
            .agents()
            .iter()
            .any(crate::engine::ai::AiAgent::frozen),
        "explicit pose owns the actor"
    );
    world.notify_animation_complete("actor", "pick_up_floor");
    let _complete = world.tick(&context);
    assert!(
        world.ai().agents().iter().all(|agent| !agent.frozen()),
        "finished one-shot releases locomotion"
    );
    let _performed_again = world.dispatch_actions(&[action], None);
    let _second_pose = world.tick(&context);
    let _navigation = world.dispatch_actions(
        &[ActionDef::Navigate {
            target: "actor".into(),
            position: [2.0, 0.0, 1.0],
            speed_mps: None,
        }],
        None,
    );
    let _resumed = world.tick(&context);
    assert!(
        world.ai().agents().iter().all(|agent| !agent.frozen()),
        "navigation releases an authored pose"
    );
    world.reset_runtime();
    assert!(
        world.animation_override("actor").is_none(),
        "reset cancels pose ownership"
    );
    Ok(())
}

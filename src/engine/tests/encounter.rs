//! Essential encounter regressions: independent deadlines/repeated commands,
//! complete bounded ownership cleanup, and priority/single-contact melee. Tiny
//! in-memory level (two zones); no assets, renderer, map baking or wall-clock waits.
use super::*;
use crate::engine::{
    collision::{DoorCollider, WallAabb},
    collision_index::CollisionIndex,
    level::WalkableFloor,
};
use serde_json::json;

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(&json!({
        "schema":"places-level","format_version":1,"id":"encounter_test","name":"Encounter fixture",
        "spawn":{"x":20,"z":20},"health":{"enabled":true},"tools":{"crowbar":true},
        "props":[{"id":"control","model":"core:crate","x":10,"z":10,"components":[
            {"component":"state","name":"start","value":false},
            {"component":"state","name":"clear","value":false},
            {"component":"state","name":"companion","value":false}]}],
        "encounter":{"control":"control","companion_point":"robot"},
        "spawn_templates":[{"id":"actor","model":"mannequin","components":[
            {"component":"health","max_hp":4,"starting_hp":4},
            {"component":"nav_agent","radius":0.3,"height":1.8,"speed_mps":1},
            {"component":"ai","behavior":"idler","sight_range":0,"hearing_range":0}]}],
        "spawn_points":[
            {"id":"left","x":-1.2,"z":0,"template":"actor","zone":{"cooldown_seconds":1}},
            {"id":"right","x":0.8,"z":0,"template":"actor","zone":{"cooldown_seconds":2}},
            {"id":"robot","x":0,"z":0,"template":"actor"}]
    }).to_string()).map_err(|error| error.to_string())
}
fn signal(world: &mut EntityWorld, key: &str) -> Result<(), String> {
    let handle = world.handle_of("control").ok_or("control missing")?;
    let state = world
        .components
        .states
        .get_mut(handle)
        .ok_or("state missing")?;
    let _set = state.set(key, StateValue::Bool(true));
    Ok(())
}
fn step(world: &mut EntityWorld, seconds: f32, player: Vec3) {
    step_at_walk_speed(world, seconds, player, 2.0);
}
fn step_with_walls(world: &mut EntityWorld, seconds: f32, player: Vec3, walls: &[WallAabb]) {
    step_with_walls_and_walk_speed(world, seconds, player, walls, 2.0);
}
fn step_at_walk_speed(world: &mut EntityWorld, seconds: f32, player: Vec3, walk_speed: f32) {
    step_with_walls_and_walk_speed(world, seconds, player, &[], walk_speed);
}
fn step_with_walls_and_walk_speed(
    world: &mut EntityWorld,
    seconds: f32,
    player: Vec3,
    walls: &[WallAabb],
    walk_speed: f32,
) {
    let index = CollisionIndex::build(walls);
    let floor = WalkableFloor::default();
    let _tick = world.tick(&WorldContext {
        delta_seconds: seconds,
        player_walk_speed: walk_speed,
        feet_from: player,
        feet: player,
        eye: Vec3::new(player.x, 1.6, player.z),
        body_height: 1.8,
        body_half: 0.3,
        form_model: None,
        walls,
        index: &index,
        floor: &floor,
        nav: None,
    });
}
#[test]
fn independent_clocks_and_repeated_rounds_retire_all_owned_state() -> Result<(), String> {
    let definition = level()?;
    validate(&definition)?;
    let mut world = EntityWorld::from_level(&definition);
    let baseline = (
        world.store.len(),
        world.components.len(),
        world.binding_count(),
    );
    for _ in 0_u8..4 {
        signal(&mut world, "start")?;
        step(&mut world, 0.5, Vec3::splat(20.0));
        signal(&mut world, "start")?;
        step(&mut world, 0.5, Vec3::splat(20.0));
        let encounter = world.encounter.as_ref().ok_or("encounter missing")?;
        assert_eq!(
            encounter.zombies.len(),
            1,
            "repeat start must not reset the first zone deadline"
        );
        assert_eq!(
            encounter.zones.get(1).ok_or("zone missing")?.spawned,
            0,
            "second zone retains its independent cooldown"
        );
        for _ in 0_u8..38 {
            step(&mut world, 0.5, Vec3::splat(20.0));
        }
        let completed = world.encounter.as_ref().ok_or("completed missing")?;
        assert_eq!(completed.zombies.len(), 20);
        assert_eq!(completed.zombies.iter().filter(|a| a.fast).count(), 4);
        for _ in 0_u8..8 {
            signal(&mut world, "companion")?;
            step(&mut world, 0.01, Vec3::splat(20.0));
        }
        signal(&mut world, "clear")?;
        step(&mut world, 0.01, Vec3::splat(20.0));
        drop(world.take_commands());
        drop(world.take_health_events());
        assert_eq!(
            (
                world.store.len(),
                world.components.len(),
                world.binding_count()
            ),
            baseline,
            "clears retire entities, component entries and bindings"
        );
        assert_eq!(
            world.dynamic_key_entries(),
            0,
            "no runtime renderer identities remain"
        );
        assert!(world.ai.agents().is_empty() && world.live_spawns.is_empty());
    }
    signal(&mut world, "start")?;
    step(&mut world, 1.0, Vec3::splat(20.0));
    world.reset_runtime();
    assert!(
        world
            .encounter
            .as_ref()
            .is_some_and(|e| !e.started && e.zombies.is_empty())
    );
    signal(&mut world, "start")?;
    step(&mut world, 1.0, Vec3::splat(20.0));
    assert_eq!(
        world.live_spawns.len(),
        1,
        "whole-world reset must permit a new round"
    );
    let mut ordinary = definition;
    ordinary.encounter = None;
    ordinary.health.enabled = false;
    let mut walking = EntityWorld::from_level(&ordinary);
    signal(&mut walking, "start")?;
    step(&mut walking, 10.0, Vec3::ZERO);
    assert!(
        !walking.has_encounter() && walking.live_spawns.is_empty(),
        "ordinary walking maps must not run encounters"
    );
    Ok(())
}
#[test]
fn companion_prioritizes_its_pursuer_and_swings_hit_only_once() -> Result<(), String> {
    let mut world = EntityWorld::from_level(&level()?);
    let mut encounter = world.encounter.take().ok_or("encounter missing")?;
    encounter
        .robots
        .push(world.spawn_combat_actor("robot", "encounter:robot", false, None)?);
    encounter
        .zombies
        .push(world.spawn_combat_actor("left", "left_zombie", false, None)?);
    encounter
        .zombies
        .push(world.spawn_combat_actor("right", "right_zombie", false, None)?);
    world.encounter = Some(encounter);
    for _ in 0_u8..30 {
        step(&mut world, 0.01, Vec3::X);
    }
    assert_eq!(
        world
            .entity_health("left_zombie")
            .ok_or("left missing")?
            .current_hp(),
        2,
        "farther pursuer takes one crowbar contact"
    );
    assert_eq!(
        world
            .entity_health("right_zombie")
            .ok_or("right missing")?
            .current_hp(),
        4,
        "closer zombie chasing player loses priority"
    );
    for _ in 0_u8..30 {
        step(&mut world, 0.01, Vec3::X);
    }
    assert_eq!(
        world
            .entity_health("left_zombie")
            .ok_or("left missing")?
            .current_hp(),
        2,
        "one contact cannot repeat during recovery"
    );
    for _ in 0_u8..25 {
        step(&mut world, 0.01, Vec3::X);
    }
    assert!(
        world
            .entity_health("left_zombie")
            .ok_or("left missing")?
            .is_dead(),
        "two crowbar contacts kill a zombie"
    );
    let commands = world.take_commands();
    assert_eq!(commands.iter().filter(|c| matches!(c, super::super::WorldCommand::Ragdoll { target, enabled: true, velocity: _ } if target == "left_zombie")).count(), 1, "death requests exactly one physical handoff");
    Ok(())
}

// Covers attack admission and contact on different ticks, beyond the existing
// single-hit test. Reuses two CPU-only actors and one box or door collider.
#[test]
fn blocked_combat_pursues_and_a_door_closing_during_the_swing_blocks_contact() -> Result<(), String>
{
    let mut world = EntityWorld::from_level(&level()?);
    let mut encounter = world.encounter.take().ok_or("encounter missing")?;
    encounter
        .robots
        .push(world.spawn_combat_actor("robot", "encounter:robot", false, None)?);
    encounter
        .zombies
        .push(world.spawn_combat_actor("right", "right_zombie", false, None)?);
    world.encounter = Some(encounter);
    let player = Vec3::new(-3.0, 0.0, 0.0);
    let wall = [WallAabb::new(0.35, -1.0, 0.1, 2.0)];
    step_with_walls(&mut world, 0.01, player, &wall);
    let behind_wall = world.encounter.as_ref().ok_or("encounter missing")?;
    assert!(
        behind_wall
            .robots
            .iter()
            .chain(&behind_wall.zombies)
            .all(|actor| actor.swing.is_none())
    );
    for (actor, target) in [
        ("encounter:robot", "right_zombie"),
        ("right_zombie", "encounter:robot"),
    ] {
        let agent = world
            .ai
            .agents()
            .iter()
            .find(|agent| agent.instance_id == actor)
            .ok_or("missing combat agent")?;
        assert_eq!(
            agent.navigation_goal,
            world.combat_position(target),
            "{actor} must still pursue an in-range target behind a wall"
        );
    }

    // The same installed leaf opens clear of the combat line, then rotates
    // closed between admission and the first contact of either swing.
    let hinge = [0.4, 0.0, -1.0];
    world.door_colliders = vec![DoorCollider::from_pose(hinge, [1.0, 0.0], 2.0, 0.05, 2.2)];
    step(&mut world, 0.01, player);
    let admitted = world.encounter.as_ref().ok_or("encounter missing")?;
    assert!(
        admitted
            .robots
            .iter()
            .chain(&admitted.zombies)
            .all(|actor| actor.swing.as_ref().is_some_and(|swing| swing.pending))
    );
    let closed_leaf = DoorCollider::from_pose(hinge, [0.0, 1.0], 2.0, 0.05, 2.2);
    world.door_colliders = vec![closed_leaf];
    step(&mut world, 0.35, player);
    let after_contact = world.encounter.as_ref().ok_or("encounter missing")?;
    assert!(
        after_contact
            .robots
            .iter()
            .chain(&after_contact.zombies)
            .all(|actor| actor.swing.as_ref().is_some_and(|swing| !swing.pending))
    );
    for id in ["encounter:robot", "right_zombie"] {
        assert_eq!(
            world
                .entity_health(id)
                .ok_or("missing actor health")?
                .current_hp(),
            4,
            "closed leaf must block {id}'s incoming contact"
        );
    }
    // Publish the closed pose again after the world refreshes its collider list.
    world.door_colliders = vec![closed_leaf];
    step(&mut world, 1.0, player);
    let recovered = world.encounter.as_ref().ok_or("encounter missing")?;
    assert!(
        recovered
            .robots
            .iter()
            .chain(&recovered.zombies)
            .all(|actor| actor.swing.is_none()),
        "recovery must release the hold without starting another swing at the closed leaf"
    );
    Ok(())
}

fn arm_contact(actor: &mut Actor, target: &str, interval: f32) {
    actor.cooldown = interval * 0.75;
    actor.swing = Some(Swing {
        interval,
        pending: true,
        target: HealthTarget::Entity { id: target.into() },
    });
}

fn damage_actor(world: &mut EntityWorld, id: &str, hp: u16) -> Result<(), String> {
    world
        .dispatch_health(
            &HealthTarget::Entity { id: id.into() },
            HealthAction::Damage { hp },
        )
        .map_err(|error| error.to_string())
}

fn same_tick_duel(companion_damage: u16) -> Result<EntityWorld, String> {
    let mut world = EntityWorld::from_level(&level()?);
    let mut encounter = world.encounter.take().ok_or("encounter missing")?;
    let mut robot = world.spawn_combat_actor("robot", "companion", false, None)?;
    let mut zombie = world.spawn_combat_actor("right", "zombie", false, None)?;
    arm_contact(
        &mut robot,
        "zombie",
        world.tool_settings.swings_per_second.recip(),
    );
    arm_contact(&mut zombie, "companion", 1.0);
    damage_actor(&mut world, "companion", companion_damage)?;
    drop(world.take_health_events());
    encounter.robots.push(robot);
    encounter.zombies.push(zombie);
    world.encounter = Some(encounter);
    step(&mut world, 0.1, Vec3::splat(20.0));
    Ok(world)
}

#[test]
fn same_tick_lethal_contact_prevents_the_companions_pending_swing() -> Result<(), String> {
    let mut world = same_tick_duel(3)?;
    assert!(
        world
            .entity_health("companion")
            .is_some_and(crate::engine::health::HealthState::is_dead),
        "the zombie must land the lethal hit first"
    );
    assert_eq!(
        world
            .entity_health("zombie")
            .ok_or("zombie missing")?
            .current_hp(),
        4,
        "the companion killed earlier in this tick cannot land its pending swing"
    );

    for _ in 0_u8..2 {
        step(&mut world, 0.1, Vec3::splat(20.0));
    }
    let commands = world.take_commands();
    assert_eq!(commands.iter().filter(|command| matches!(command, super::super::WorldCommand::Ragdoll { target, enabled: true, velocity: _ } if target == "companion")).count(), 1, "companion death requests one ragdoll handoff");
    Ok(())
}

#[test]
fn same_tick_nonlethal_contact_preserves_both_swings_in_order() -> Result<(), String> {
    let mut world = same_tick_duel(2)?;
    let contacts: Vec<_> = world
        .take_health_events()
        .into_iter()
        .map(|event| (event.target, event.change.current_hp))
        .collect();
    assert_eq!(
        contacts,
        [
            (
                HealthTarget::Entity {
                    id: "companion".into()
                },
                1
            ),
            (
                HealthTarget::Entity {
                    id: "zombie".into()
                },
                2
            ),
        ],
        "a surviving companion still lands its contact after the zombie"
    );
    Ok(())
}

#[test]
fn same_tick_later_zombie_excludes_the_companion_killed_by_the_first() -> Result<(), String> {
    let mut world = EntityWorld::from_level(&level()?);
    let mut encounter = world.encounter.take().ok_or("encounter missing")?;
    encounter
        .robots
        .push(world.spawn_combat_actor("robot", "companion", false, None)?);
    for (point, id) in [("left", "zombie_left"), ("right", "zombie_right")] {
        let mut zombie = world.spawn_combat_actor(point, id, false, None)?;
        arm_contact(&mut zombie, "companion", 1.0);
        encounter.zombies.push(zombie);
    }
    damage_actor(&mut world, "companion", 3)?;
    drop(world.take_health_events());
    world.encounter = Some(encounter);
    step(&mut world, 0.1, Vec3::splat(20.0));

    let contacts = world.take_health_events();
    assert!(
        matches!(contacts.as_slice(), [event] if matches!(&event.target, HealthTarget::Entity { id } if id == "companion") && event.change.became_dead()),
        "one lethal contact reaches the shared companion"
    );
    let after_contact = world.encounter.as_ref().ok_or("encounter missing")?;
    assert_eq!(
        after_contact
            .zombies
            .first()
            .and_then(|zombie| zombie.target_robot.as_deref()),
        Some("companion"),
        "the first zombie chose the still-living companion"
    );
    assert_eq!(
        after_contact
            .zombies
            .get(1)
            .ok_or("second zombie missing")?
            .target_robot,
        None,
        "the later zombie must fall back to the player"
    );
    Ok(())
}

#[test]
fn same_tick_later_companion_excludes_the_zombie_killed_by_the_first() -> Result<(), String> {
    let mut world = EntityWorld::from_level(&level()?);
    let mut encounter = world.encounter.take().ok_or("encounter missing")?;
    for (id, offset) in [("robot_first", None), ("robot_later", Some(Vec3::Z * 0.25))] {
        let mut robot = world.spawn_combat_actor("robot", id, false, offset)?;
        arm_contact(
            &mut robot,
            "zombie_right",
            world.tool_settings.swings_per_second.recip(),
        );
        encounter.robots.push(robot);
    }
    for (point, id) in [("left", "zombie_left"), ("right", "zombie_right")] {
        encounter
            .zombies
            .push(world.spawn_combat_actor(point, id, false, None)?);
    }
    damage_actor(&mut world, "zombie_right", 2)?;
    drop(world.take_health_events());
    world.encounter = Some(encounter);
    step(&mut world, 0.1, Vec3::splat(20.0));

    let contacts = world.take_health_events();
    assert!(
        matches!(contacts.as_slice(), [event] if matches!(&event.target, HealthTarget::Entity { id } if id == "zombie_right") && event.change.became_dead()),
        "one lethal contact reaches the shared zombie and the old swing does not transfer"
    );
    let after_contact = world.encounter.as_ref().ok_or("encounter missing")?;
    assert_eq!(
        after_contact
            .robots
            .get(1)
            .and_then(|robot| robot.target_enemy.as_deref()),
        Some("zombie_left"),
        "the later companion must select the other living zombie"
    );
    Ok(())
}

// Five companions must retain individual target identity and release every
// owned actor/tool slot. This extends the tiny CPU fixture; no assets or baking.
#[test]
fn companion_group_targets_individuals_and_toggle_retires_every_member() -> Result<(), String> {
    let mut definition = level()?;
    definition
        .encounter
        .as_mut()
        .ok_or("definition missing")?
        .companion_count = 5;
    validate(&definition)?;
    let mut world = EntityWorld::from_level(&definition);
    let baseline = (world.store.len(), world.components.len());
    signal(&mut world, "companion")?;
    step(&mut world, 0.01, Vec3::splat(20.0));
    assert_eq!(world.live_spawns.len(), 5);
    assert_eq!(
        (0..MAX_COMPANIONS)
            .filter(|i| world.companion_tool_id(*i).is_some())
            .count(),
        5
    );
    let positions: Vec<_> = world
        .encounter
        .as_ref()
        .ok_or("encounter missing")?
        .robots
        .iter()
        .filter_map(|r| world.combat_position(&r.id))
        .collect();
    assert!(positions.windows(2).all(|pair| {
        pair.first()
            .zip(pair.last())
            .is_some_and(|(a, b)| a.distance(*b) > 1.0)
    }));
    signal(&mut world, "start")?;
    step(&mut world, 1.0, Vec3::splat(20.0));
    assert_eq!(
        world
            .encounter
            .as_ref()
            .and_then(|e| e.zombies.first())
            .and_then(|z| z.target_robot.as_deref()),
        Some("encounter:robot:1")
    );
    assert_eq!(
        world
            .encounter_pose("encounter:zombie:0", 3.375)
            .and_then(|p| p.clip_name().map(str::to_string))
            .as_deref(),
        Some("run_arms_forward")
    );
    let _killed = world.dispatch_health(
        &HealthTarget::Entity {
            id: "encounter:robot:1".into(),
        },
        HealthAction::Damage { hp: u16::MAX },
    );
    step(&mut world, 0.01, Vec3::splat(20.0));
    assert_eq!(
        world
            .encounter
            .as_ref()
            .and_then(|e| e.zombies.first())
            .and_then(|z| z.target_robot.as_deref()),
        Some("encounter:robot")
    );
    assert!(world.companion_tool_id(1).is_none());
    signal(&mut world, "companion")?;
    step(&mut world, 0.01, Vec3::splat(20.0));
    assert!((0..MAX_COMPANIONS).all(|i| world.companion_tool_id(i).is_none()));
    signal(&mut world, "clear")?;
    step(&mut world, 0.01, Vec3::splat(20.0));
    assert_eq!((world.store.len(), world.components.len()), baseline);
    assert!(world.live_spawns.is_empty() && world.ai.agents().is_empty());
    for invalid in [0, MAX_COMPANIONS + 1] {
        definition
            .encounter
            .as_mut()
            .ok_or("definition missing")?
            .companion_count = invalid;
        assert!(validate(&definition).is_err());
    }
    Ok(())
}

#[test]
fn companion_follow_and_clear_use_bounded_navigation_at_maximum_walk_speed() -> Result<(), String> {
    let mut settings = crate::game::settings::Settings {
        walk_speed: 10.0,
        ..Default::default()
    };
    settings.sanitize();
    assert_eq!(settings.walk_speed.to_bits(), 10.0_f32.to_bits());

    let mut world = EntityWorld::from_level(&level()?);
    let id = "encounter:robot";
    let initial_goal = Vec3::new(6.0, 0.0, 0.0);
    signal(&mut world, "companion")?;
    step(&mut world, 0.01, initial_goal);
    assert_eq!(
        world
            .ai
            .agent(id)
            .ok_or("companion missing")?
            .navigation_goal,
        Some(initial_goal),
        "ordinary follow installs the initial goal"
    );

    let new_goal = Vec3::new(0.0, 0.0, 6.0);
    step_at_walk_speed(&mut world, 0.01, new_goal, settings.walk_speed);
    let following = world.ai.agent(id).ok_or("companion missing")?;
    assert_eq!(
        following.navigation_goal,
        Some(new_goal),
        "following at the maximum must replace the previous goal"
    );
    assert_eq!(
        following.navigation_speed,
        Some(8.0),
        "companion pace stays within the existing navigation limit"
    );
    assert!(
        !world
            .ai
            .set_navigation_goal_with_speed(id, initial_goal, Some(8.0_f32.next_up())),
        "other navigation requests still reject speeds above 8 m/s"
    );

    signal(&mut world, "start")?;
    step_at_walk_speed(&mut world, 1.0, new_goal, settings.walk_speed);
    let opponent = world
        .encounter
        .as_ref()
        .and_then(|encounter| encounter.robots.first())
        .and_then(|robot| robot.target_enemy.as_deref());
    assert!(opponent.is_some(), "the companion has engaged an opponent");
    assert_ne!(
        world
            .ai
            .agent(id)
            .ok_or("companion missing")?
            .navigation_goal,
        Some(new_goal),
        "combat replaced the follow goal before Clear"
    );

    signal(&mut world, "clear")?;
    step_at_walk_speed(&mut world, 0.01, initial_goal, settings.walk_speed);
    let after_clear = world.ai.agent(id).ok_or("companion missing after Clear")?;
    assert_eq!(
        after_clear.navigation_goal,
        Some(initial_goal),
        "Clear returns the surviving companion to player follow"
    );
    assert_eq!(after_clear.navigation_speed, Some(8.0));
    assert!(world.ai.agent("encounter:zombie:0").is_none());

    let invalid_speed = settings.walk_speed.next_up();
    for clear_before_follow in [false, true] {
        if clear_before_follow {
            signal(&mut world, "start")?;
            step_at_walk_speed(&mut world, 1.0, initial_goal, settings.walk_speed);
            assert!(world.ai.agent("encounter:zombie:0").is_some());
            signal(&mut world, "clear")?;
        }
        let hold = world.ai.agent(id).ok_or("companion missing")?.position;
        step_at_walk_speed(&mut world, 0.01, new_goal, invalid_speed);
        let rejected = world.ai.agent(id).ok_or("companion missing")?;
        assert_eq!(
            rejected.navigation_goal,
            Some(hold),
            "an invalid walking speed must discard the stale goal (Clear: {clear_before_follow})"
        );
        assert_eq!(rejected.navigation_speed, None);
        assert!(world.ai.agent("encounter:zombie:0").is_none());
    }
    Ok(())
}

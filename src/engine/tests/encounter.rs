//! Essential encounter regressions: independent deadlines/repeated commands,
//! complete bounded ownership cleanup, and priority/single-contact melee. Tiny
//! in-memory level (two zones); no assets, renderer, map baking or wall-clock waits.
use super::*;
use crate::engine::{collision_index::CollisionIndex, level::WalkableFloor};
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
    let index = CollisionIndex::empty();
    let floor = WalkableFloor::default();
    let _tick = world.tick(&WorldContext {
        delta_seconds: seconds,
        player_walk_speed: 2.0,
        feet_from: player,
        feet: player,
        eye: Vec3::new(player.x, 1.6, player.z),
        body_height: 1.8,
        body_half: 0.3,
        form_model: None,
        walls: &[],
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

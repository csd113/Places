//! Transport crosses world-local authored definitions. These tiny, asset-free
//! worlds protect rotated shelter points, cabin/map ownership and checkpoint
//! reconstruction; ordinary movement tests do not cross this boundary.
use crate::engine::{
    ai::{AiHomeBounds, travel::Record},
    elevator::{CabinFrame, map_cabin_point},
    entities::EntityWorld,
    level::{ComponentDef, LevelDef},
};
use glam::{Quat, Vec3};

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"travel_ai","name":"Travel AI","spawn":{"x":2,"z":2},"rooms":[{"width":8,"depth":8}],"props":[{"id":"npc","model":"core:chair","x":5,"z":5,"solid":false,"components":[{"component":"nav_agent","radius":0.22,"height":1.8,"speed_mps":1.0,"step_height":0.3,"max_slope":2.0},{"component":"ai","behavior":"skittish","hide_anchors":[[0.5,0.0,1.5],[6.0,0.0,5.0],[7.0,0.0,5.0]]}]}]}"#)
        .map_err(|e| e.to_string())
}

#[test]
fn transported_shelters_rotate_and_survive_checkpoint_reconstruction() -> Result<(), String> {
    let source = level()?;
    let original = EntityWorld::from_level(&source);
    let mut passenger = original.passenger_record("npc")?;
    // The actor walked into the cabin; its authored post and two shelters
    // remain outside. Normalize exactly where the elevator capture does.
    let position = Vec3::new(0.25, 0.0, 1.2);
    passenger.set_pose("npc", position, 30.0);
    passenger.cabin_home(&CabinFrame {
        origin: Vec3::ZERO,
        rotation_degrees: 0.0,
    });
    let to = Vec3::new(100.0, 4.0, 100.0);
    let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    passenger.relocate("npc", Vec3::ZERO, to, turn);
    let mut restored = EntityWorld::from_level(&source);
    restored.restore_passenger(&passenger)?;
    let arrived = restored.ai().agent("npc").ok_or("NPC")?;
    let mapped_position = map_cabin_point(Vec3::ZERO, to, turn, position);
    let mapped_anchor = map_cabin_point(Vec3::ZERO, to, turn, Vec3::new(0.5, 0.0, 1.5));
    assert!(arrived.position.distance(mapped_position) < 0.0001);
    assert!(arrived.home.distance(mapped_position) < 0.0001);
    assert!((arrived.home_yaw - 120.0).abs() < 0.001);
    assert_eq!(
        arrived.def.hide_anchors.len(),
        2,
        "external map shelters collapse to one arrival post"
    );
    assert!(
        Vec3::from_array(*arrived.def.hide_anchors.first().ok_or("cabin shelter")?)
            .distance(mapped_anchor)
            < 0.0001
    );
    assert!(
        Vec3::from_array(*arrived.def.hide_anchors.last().ok_or("arrival shelter")?)
            .distance(mapped_position)
            < 0.0001
    );
    let expected = arrived.def.hide_anchors.clone();
    let encoded = serde_json::to_vec(&restored.checkpoint()?).map_err(|e| e.to_string())?;
    let saved = serde_json::from_slice(&encoded).map_err(|e| e.to_string())?;
    let mut fresh = EntityWorld::from_level(&source);
    fresh.restore_checkpoint(&saved)?;
    assert_eq!(
        fresh
            .ai()
            .agent("npc")
            .ok_or("checkpoint NPC")?
            .def
            .hide_anchors,
        expected
    );
    // A later capture/restore must retain current coordinates rather than the
    // source PropDef that the traveller ledger keeps for reconstruction.
    let again = fresh.passenger_record("npc")?;
    let mut next_visit = EntityWorld::from_level(&source);
    next_visit.restore_passenger(&again)?;
    assert_eq!(
        next_visit
            .ai()
            .agent("npc")
            .ok_or("next visit NPC")?
            .def
            .hide_anchors,
        expected
    );
    Ok(())
}

#[test]
fn shelter_transform_roundtrip_legacy_records_and_map_bounds_are_explicit() -> Result<(), String> {
    let source = level()?;
    let original = EntityWorld::from_level(&source);
    let agent = original.ai().agent("npc").ok_or("NPC")?;
    let mut record = Record::capture(agent, |_| None)?;
    let from = Vec3::new(3.0, 2.0, 7.0);
    let to = Vec3::new(30.0, 5.0, -20.0);
    let turn = Quat::from_rotation_y(0.73);
    record.transform(from, to, turn);
    record.transform(to, from, turn.inverse());
    let mut restored = EntityWorld::from_level(&source);
    // Use the same semantic Record restore API used by ordinary checkpoints.
    restored
        .ai_mut()
        .restore_passenger("npc", &record, agent.position, 0.0, |_| None)?;
    let roundtrip = restored.ai().agent("npc").ok_or("roundtrip NPC")?;
    for (actual, expected) in roundtrip
        .def
        .hide_anchors
        .iter()
        .zip(&agent.def.hide_anchors)
    {
        assert!(Vec3::from_array(*actual).distance(Vec3::from_array(*expected)) < 0.0001);
    }
    let mut legacy_json = serde_json::to_value(&record).map_err(|e| e.to_string())?;
    let _removed = legacy_json
        .as_object_mut()
        .ok_or("record object")?
        .remove("shelters");
    let legacy = serde_json::from_value(legacy_json).map_err(|e| e.to_string())?;
    let mut legacy_world = EntityWorld::from_level(&source);
    legacy_world
        .ai_mut()
        .restore_passenger("npc", &legacy, agent.position, 0.0, |_| None)?;
    assert_eq!(
        legacy_world
            .ai()
            .agent("npc")
            .ok_or("legacy NPC")?
            .def
            .hide_anchors,
        agent.def.hide_anchors
    );
    let mut malformed_json = serde_json::to_value(record).map_err(|e| e.to_string())?;
    *malformed_json.get_mut("shelters").ok_or("shelters")? =
        serde_json::json!([[100_001.0_f32, 0.0_f32, 0.0_f32]]);
    let malformed = serde_json::from_value(malformed_json).map_err(|e| e.to_string())?;
    assert!(
        restored
            .ai_mut()
            .restore_passenger("npc", &malformed, agent.position, 0.0, |_| None)
            .is_err()
    );
    let mut bounded_level = source;
    for component in &mut bounded_level.props.first_mut().ok_or("prop")?.components {
        if let ComponentDef::Ai(def) = component {
            def.home_bounds = Some(AiHomeBounds {
                x: 0.0,
                z: 0.0,
                width: 8.0,
                depth: 8.0,
                min_y: 0.0,
                max_y: 0.0,
            });
        }
    }
    let bounded = EntityWorld::from_level(&bounded_level);
    assert!(
        bounded.passenger_record("npc").is_err(),
        "map-owned home bounds cannot be transported"
    );
    Ok(())
}

// LSR-03: actual checkpoint/JSON reconstruction must retain authored goal
// ownership and pace, including repeats, legacy input and malformed overrides.
// Small CPU worlds only; no map bake or renderer.
#[test]
fn checkpoint_navigation_retains_speed_bounds_and_legacy_defaults() -> Result<(), String> {
    let source = level()?;
    let goal = Vec3::new(6.0, 0.0, 6.0);
    for speed in [0.05, 0.1, 2.8, 8.0] {
        let mut world = EntityWorld::from_level(&source);
        assert!(
            world
                .ai_mut()
                .set_navigation_goal_with_speed("npc", goal, Some(speed))
        );
        for _ in 0..3_u8 {
            let json = serde_json::to_vec(&world.checkpoint()?).map_err(|e| e.to_string())?;
            let snapshot = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
            world = EntityWorld::from_level(&source);
            world.restore_checkpoint(&snapshot)?;
            let agent = world.ai().agent("npc").ok_or("NPC")?;
            assert_eq!(agent.navigation_goal, Some(goal));
            assert_eq!(agent.navigation_speed, Some(speed));
            assert!(
                agent.path.waypoints.is_empty(),
                "world-local path is reconstructed"
            );
        }
        let record = Record::capture(world.ai().agent("npc").ok_or("NPC")?, |_| None)?;
        let mut encoded = serde_json::to_value(&record).map_err(|e| e.to_string())?;
        let _removed = encoded
            .as_object_mut()
            .ok_or("record")?
            .remove("navigation");
        let legacy = serde_json::from_value(encoded).map_err(|e| e.to_string())?;
        world
            .ai_mut()
            .restore_passenger("npc", &legacy, Vec3::ZERO, 0.0, |_| None)?;
        let agent = world.ai().agent("npc").ok_or("NPC")?;
        assert!(agent.navigation_goal.is_none());
        assert!(agent.navigation_speed.is_none());
        assert!(
            matches!(agent.state, crate::engine::ai::AiState::Investigate { point } if point == goal)
        );
        for invalid in [-1.0_f32, 0.0, 0.049, 8.01] {
            let mut malformed = serde_json::to_value(&record).map_err(|e| e.to_string())?;
            *malformed
                .get_mut("navigation")
                .and_then(|value| value.get_mut("speed"))
                .ok_or("navigation speed")? = serde_json::json!(invalid);
            let bad = serde_json::from_value(malformed).map_err(|e| e.to_string())?;
            assert!(
                world
                    .ai_mut()
                    .restore_passenger("npc", &bad, Vec3::ZERO, 0.0, |_| None)
                    .is_err()
            );
        }
        let mut moved = record;
        moved.transform(Vec3::ZERO, Vec3::X * 10.0, Quat::IDENTITY);
        world
            .ai_mut()
            .restore_passenger("npc", &moved, Vec3::ZERO, 0.0, |_| None)?;
        assert_eq!(
            world.ai().agent("npc").ok_or("NPC")?.navigation_goal,
            Some(goal + Vec3::X * 10.0)
        );
    }
    Ok(())
}

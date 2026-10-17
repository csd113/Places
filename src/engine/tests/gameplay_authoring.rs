//! Tiny metadata-only regressions for authorable two-way doors, exact entity
//! sweeps and independent sound emitters. They protect occupant-origin dispatch,
//! interrupted swing continuity and reset policy without assets, a bake, GPU or
//! audio device; all fixtures contain at most two props and one door/volume.

use glam::Vec3;

use crate::engine::door::{DoorPhase, DoorRuntime};
use crate::engine::entities::{EntityWorld, WorldCommand};
use crate::engine::level::{ActionDef, DoorDef, LevelDef};

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"gameplay_fixture","name":"Gameplay","spawn":{"x":4,"z":4},
      "props":[{"id":"actor","model":"core:plant","x":-2,"y":0.5,"z":0.5},
        {"id":"other","model":"core:plant","x":-2,"y":0.5,"z":0.5}],
      "doors":[{"id":"door","x":0,"z":0,"width":1,"height":2,"bidirectional":true,
        "components":[{"component":"interactable"}],"bindings":[{"on":"interact","actions":[{"action":"toggle"}]}]}],
      "sound_emitters":[{"id":"ambience","x":2,"y":1,"z":2,"sound":"ambient.fixture","spatial":false,"looped":true,"playing":true}],
      "spawn_templates":[{"id":"template","model":"core:plant"}],
      "spawn_points":[{"id":"point","x":2,"z":2,"template":"template"}],
      "volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1,
        "filter":{"kind":"entity","instance":"actor"},"bindings":[{"on":"enter_volume","once":true,"actions":[
          {"action":"play_sound","target":"ambience","loop":true},
          {"action":"open","target":"door"},
          {"action":"spawn_entity","point":"point","name":"arrival"}]}]}]}"#)
        .map_err(|error| error.to_string())
}

#[test]
fn bidirectional_doors_start_closed_and_keep_their_side_until_closed() -> Result<(), String> {
    let fixture = level()?;
    let def = fixture.doors.first().ok_or("missing door")?;
    let mut door = DoorRuntime::new(def, &fixture);
    assert_eq!(door.phase(), DoorPhase::Closed, "door must rest closed");
    assert!(
        door.angle().abs() < f32::EPSILON,
        "closed pose has zero angle"
    );
    assert!(
        door.request_open_from(Some(Vec3::new(0.5, 1.0, 2.0))),
        "closed leaf accepts positive-side opening"
    );
    let _ = door.advance(0.25, |_| false);
    assert!(
        door.angle() > 0.0,
        "positive side opens away toward negative Z"
    );
    assert!(door.request_close(), "open leaf accepts closing");
    assert!(
        door.request_open_from(Some(Vec3::new(0.5, 1.0, -2.0))),
        "leaf accepts opposite-side opening"
    );
    let _ = door.advance(1.0, |_| false);
    assert!(
        door.angle() > 0.0,
        "mid-swing reversal retains the chosen endpoint"
    );
    assert!(door.request_close(), "open leaf accepts closing");
    let _ = door.advance(1.0, |_| false);
    assert_eq!(door.phase(), DoorPhase::Closed, "door must rest closed");
    assert!(
        door.request_open_from(Some(Vec3::new(0.5, 1.0, -2.0))),
        "leaf accepts opposite-side opening"
    );
    let _ = door.advance(1.0, |_| false);
    assert!(
        door.angle() < 0.0,
        "a new opening may choose the opposite side"
    );
    door.reset();
    assert!(
        door.angle().abs() < f32::EPSILON,
        "closed pose has zero angle"
    );
    assert!(!door.def.locked, "authored default is unlocked");
    let mut one_way = DoorRuntime::new(
        &DoorDef {
            bidirectional: false,
            ..def.clone()
        },
        &fixture,
    );
    assert!(
        one_way.request_open_from(Some(Vec3::new(0.5, 1.0, -2.0))),
        "ordinary leaf accepts opening"
    );
    let _ = one_way.advance(1.0, |_| false);
    assert!(
        one_way.angle() > 0.0,
        "ordinary doors preserve authored swing"
    );
    let mut world = EntityWorld::from_level(&fixture);
    let aimed = world
        .interactables()
        .index_of("door")
        .ok_or("missing aimed door")?;
    let report = world
        .dispatch_interaction_from(Some(aimed), Some(Vec3::new(0.5, 1.0, -2.0)))
        .ok_or("missing interaction report")?;
    assert_eq!(
        report.doors_acted, 1,
        "interaction reaches shared door dispatcher"
    );
    let _ = world.doors_mut().advance(1.0, |_, _| false);
    assert!(
        world.doors().get(0).is_some_and(|leaf| leaf.angle() < 0.0),
        "ordinary interaction bindings retain the interacting player's origin"
    );
    Ok(())
}

#[test]
fn exact_nonphysical_entity_crossing_dispatches_shared_sound_door_and_spawn_actions()
-> Result<(), String> {
    let mut world = EntityWorld::from_level(&level()?);
    let _ = world.take_commands();
    let outside = Vec3::splat(5.0);
    world.seed_volumes(outside);
    let other = world.handle_of("other").ok_or("missing unrelated actor")?;
    world
        .components_mut()
        .transforms
        .get_mut(other)
        .ok_or("missing pose")?
        .position
        .x = 2.0;
    world.update_volumes(outside, outside);
    assert!(
        world.events().is_empty(),
        "unselected entity cannot fire the trigger"
    );
    let actor = world.handle_of("actor").ok_or("missing actor")?;
    world
        .components_mut()
        .transforms
        .get_mut(actor)
        .ok_or("missing actor pose")?
        .position
        .x = 2.0;
    world.update_volumes(outside, outside);
    assert_eq!(
        world.events().len(),
        2,
        "complete crossing produces enter and exit"
    );
    assert!(
        world
            .events()
            .peek()
            .is_some_and(|event| event.actor == Some(actor)),
        "crossing identifies the selected entity"
    );
    let report = world.dispatch_queued_events();
    assert_eq!(
        report.actions_run, 3,
        "entry runs all three reusable actions"
    );
    assert_eq!(report.spawned, 1, "entry spawns exactly one entity");
    assert!(
        world.handle_of("arrival").is_some(),
        "spawn gets its reusable identity"
    );
    assert!(world.take_commands().iter().any(|command| matches!(command, WorldCommand::PlaySound { sound, entity: _, gain: _, looped: _, time_domain: _ } if sound == "ambient.fixture")), "entry starts the selected emitter cue");
    let _ = world.doors_mut().advance(1.0, |_, _| false);
    assert!(
        world.doors().get(0).is_some_and(|door| door.angle() > 0.0),
        "trigger occupant origin chooses away swing"
    );
    Ok(())
}

#[test]
fn standalone_emitter_preserves_spatial_policy_and_authored_playback_on_reset() -> Result<(), String>
{
    let mut world = EntityWorld::from_level(&level()?);
    let commands = world.take_commands();
    let key = commands
        .iter()
        .find_map(|command| {
            if let WorldCommand::PlaySound {
                entity,
                sound,
                looped: true,
                gain: _,
                time_domain: _,
            } = command
                && sound == "ambient.fixture"
            {
                Some(*entity)
            } else {
                None
            }
        })
        .ok_or("authored ambience did not start")?;
    assert_eq!(
        world.audio_source_spatial(key),
        Some(false),
        "ambient override survives lifecycle actions"
    );
    assert_eq!(
        world.audio_source_anchor(key),
        Some(("ambience", Vec3::new(2.0, 1.0, 2.0))),
        "sound-only entity retains its authored world anchor"
    );
    let report = world.dispatch_actions(
        &[ActionDef::Disable {
            target: Some("ambience".to_string()),
        }],
        None,
    );
    assert_eq!(
        report.actions_run, 1,
        "disable dispatches on a sound-only entity"
    );
    assert!(
        world
            .take_commands()
            .iter()
            .any(|command| matches!(command, WorldCommand::StopSound { entity } if *entity == key)),
        "disable stops the corresponding source"
    );
    world.reset_runtime();
    assert!(
        world.take_commands().iter().any(
            |command| matches!(command, WorldCommand::PlaySound { entity, sound: _, gain: _, looped: _, time_domain: _ } if *entity == key)
        ),
        "reset restores authored playing ambience"
    );
    assert_eq!(
        world.audio_source_spatial(key),
        Some(false),
        "ambient override survives lifecycle actions"
    );
    Ok(())
}

// A recycled spawn slot must not inherit a despawned actor's occupancy. This
// extends the exact-filter crossing fixture with generation reuse in <1 ms.
#[test]
fn exact_named_spawn_filter_drops_despawned_generation_and_accepts_its_replacement()
-> Result<(), String> {
    let mut fixture = level()?;
    let volume = fixture.volumes.first_mut().ok_or("missing trigger")?;
    volume.filter = crate::engine::level::TriggerFilterDef::Entity {
        instance: "arrival".to_string(),
    };
    volume.bindings.clear();
    let point = fixture.spawn_points.first_mut().ok_or("missing point")?;
    point.x = 0.5;
    point.y = Some(0.5);
    point.z = 0.5;
    let mut world = EntityWorld::from_level(&fixture);
    let outside = Vec3::splat(5.0);
    world.seed_volumes(outside);
    let spawn = ActionDef::SpawnEntity {
        template: None,
        point: Some("point".to_string()),
        group: None,
        name: Some("arrival".to_string()),
    };
    let _ = world.dispatch_actions(std::slice::from_ref(&spawn), None);
    let _ = world.dispatch_queued_events();
    let original = world.handle_of("arrival").ok_or("missing original spawn")?;
    world.update_volumes(outside, outside);
    assert_eq!(world.events().len(), 1, "new named spawn emits one entry");
    assert!(
        world
            .events()
            .peek()
            .is_some_and(|event| event.actor == Some(original)),
        "entry records the original generation"
    );
    let _ = world.dispatch_queued_events();
    let _ = world.dispatch_actions(
        &[ActionDef::DespawnEntity {
            target: "arrival".to_string(),
        }],
        None,
    );
    world.update_volumes(outside, outside);
    assert!(
        world.events().peek().is_some_and(
            |event| event.kind == crate::engine::entities::events::EventKind::ExitVolume
        ),
        "despawn clears occupancy through an ordinary exit"
    );
    let _ = world.dispatch_queued_events();
    let _ = world.dispatch_actions(&[spawn], None);
    let _ = world.dispatch_queued_events();
    let replacement = world
        .handle_of("arrival")
        .ok_or("missing replacement spawn")?;
    assert_ne!(
        replacement, original,
        "slot reuse must not reuse the generation"
    );
    world.update_volumes(outside, outside);
    assert!(
        world
            .events()
            .peek()
            .is_some_and(|event| event.actor == Some(replacement)
                && event.kind == crate::engine::entities::events::EventKind::EnterVolume),
        "replacement enters under its new generation"
    );
    Ok(())
}

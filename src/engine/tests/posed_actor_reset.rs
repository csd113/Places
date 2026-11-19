//! Covers the live capture-to-reset boundary missing from the policy and
//! checkpoint tests. One flat CPU room and the two production rigs exercise
//! contact, reset publication and the ordinary reverse path without a map bake.
use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::components::StateValue;
use crate::engine::entities::{EntityWorld, WorldCommand, WorldContext};
use crate::engine::level::{ActionDef, LevelDef, WalkableCeiling, WalkableFloor};
use crate::engine::nav::NavMesh;
use crate::engine::tall_guest::{audio_fence::Fence, navigation_class};
use glam::Vec3;

fn fixture() -> Result<LevelDef, String> {
    LevelDef::from_json(
        &serde_json::json!({
            "schema": "places-level", "format_version": 1, "id": "posed_actor_reset",
            "name": "Posed actor reset", "spawn": {"x": -10, "z": -10},
            "rooms": [{"x": -4, "z": -3, "width": 8, "depth": 13, "height": 5}],
            "props": [
                {"id": "hunter", "model": "tall-guest", "x": 0, "z": 0, "solid": false,
                 "tall_guest": {"prey_targets": ["prey"]}},
                {"id": "prey", "model": "feltling", "x": 0, "z": 5, "solid": false,
                 "feltling": {"mode": "flee", "routes": [
                     {"id": "first", "points": [[0, 0, 8]]},
                     {"id": "alternate", "points": [[8, 0, 8]]}
                 ]},
                 "components": [
                     {"component": "interactable", "prompt": "Greet"},
                     {"component": "state", "name": "feltling_mode", "value": "flee"},
                     {"component": "state", "name": "feltling_route", "value": "first"},
                     {"component": "audio", "sound": "feltling_babble", "playing": false}
                 ]}
            ]
        })
        .to_string(),
    )
    .map_err(|error| error.to_string())
}

fn navigation(level: &LevelDef, floor: &WalkableFloor) -> Result<NavMesh, String> {
    let (grid, _) = crate::map_compiler::navigation::bake(
        &crate::map_compiler::navigation::NavBakeInput {
            level,
            walls: &[],
            floor,
            ceiling: &WalkableCeiling::from_level(level),
            doors: &crate::engine::door::Doors::from_level(level),
            classes: &[navigation_class()],
            obstacles: &[],
            walk_proxies: &[],
        },
        &crate::map_compiler::navigation::NavBakeOptions {
            cell_m: 0.4,
            workers: 1,
        },
    )?;
    NavMesh::from_record(grid)
}

fn wait_for_latch(world: &mut EntityWorld, ctx: &WorldContext<'_>) -> Result<(), String> {
    for _ in 0_u16..210 {
        let _tick = world.tick(ctx);
        if world
            .guest_diagnostics()
            .pointer("/0/capture/latched")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            return Ok(());
        }
    }
    Err(format!(
        "Fixture never reached contact: guest {}; Feltling {}",
        world.guest_diagnostics(),
        world.feltling_diagnostics()
    ))
}

#[test]
fn reset_restores_authored_controllers_during_capture() -> Result<(), String> {
    let level = fixture()?;
    let floor = WalkableFloor::from_level(&level);
    let nav = navigation(&level, &floor)?;
    let index = CollisionIndex::empty();
    let feet = Vec3::new(-10.0, 0.0, -10.0);
    let ctx = WorldContext {
        delta_seconds: 1.0 / 30.0,
        player_walk_speed: 3.75,
        feet_from: feet,
        feet,
        eye: Vec3::new(feet.x, 1.6, feet.z),
        body_height: 1.8,
        body_half: 0.2,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: Some(&nav),
    };
    let mut world = EntityWorld::from_level(&level);
    world.validate_guest_capability()?;
    let authored_frames = world.entity_frames().to_vec();
    let authored_colliders = world.door_colliders().to_vec();
    let authored_prey = world.feltling_diagnostics();
    let hunter = world.handle_of("hunter").ok_or("Missing hunter")?;
    let prey = world.handle_of("prey").ok_or("Missing prey")?;
    let prey_start = world.components().transforms.get(prey).copied();
    let initial_epoch = world
        .guest_audio_epoch("hunter")
        .ok_or("Missing audio epoch")?;
    wait_for_latch(&mut world, &ctx)?;
    assert_ne!(world.entity_frames(), authored_frames);
    let (key, delayed) = world
        .take_guest_sounds()
        .into_iter()
        .next()
        .ok_or("Missing capture audio")?;
    alter_actors(&mut world);
    assert_ne!(world.feltling_diagnostics(), authored_prey);
    world.reset_runtime();

    assert_eq!(world.handle_of("hunter"), Some(hunter));
    assert_eq!(world.handle_of("prey"), Some(prey));
    assert_eq!(world.entity_frames(), authored_frames);
    assert_eq!(world.door_colliders(), authored_colliders);
    assert_eq!(world.feltling_diagnostics(), authored_prey);
    assert_eq!(world.components().transforms.get(prey).copied(), prey_start);
    assert!(world.components().states.get(hunter).is_none());
    let states = world
        .components()
        .states
        .get(prey)
        .ok_or("Missing authored state")?;
    assert_eq!(
        states.get("feltling_mode").and_then(StateValue::as_text),
        Some("flee")
    );
    assert_eq!(
        states.get("feltling_route").and_then(StateValue::as_text),
        Some("first")
    );
    assert!(
        world
            .guest_diagnostics()
            .pointer("/0/capture")
            .is_some_and(serde_json::Value::is_null)
    );
    assert!(world.take_guest_sounds().is_empty());
    assert!(world.take_commands().iter().all(|command| !matches!(
        command,
        WorldCommand::PlaySound {
            entity: _,
            sound: _,
            gain: _,
            looped: _,
            time_domain: _
        }
    )));
    let new_epoch = world
        .guest_audio_epoch("hunter")
        .ok_or("Reset left hunter disabled")?;
    assert_ne!(new_epoch, initial_epoch);
    assert!(!Fence::default().admit(key, Some(new_epoch), &delayed));

    check_live_release(&mut world, &ctx)?;
    assert_eq!(world.handle_of("prey"), Some(prey));
    Ok(())
}

fn alter_actors(world: &mut EntityWorld) {
    let report = world.dispatch_actions(
        &[
            ActionDef::SetState {
                target: Some("prey".into()),
                name: "feltling_mode".into(),
                value: StateValue::Text("wait".into()),
            },
            ActionDef::SetState {
                target: Some("prey".into()),
                name: "feltling_route".into(),
                value: StateValue::Text("alternate".into()),
            },
            ActionDef::PlaySound {
                target: Some("prey".into()),
                sound: Some("feltling_cheer".into()),
                looped: false,
            },
            ActionDef::Disable {
                target: Some("prey".into()),
            },
            ActionDef::Disable {
                target: Some("hunter".into()),
            },
        ],
        None,
    );
    assert_eq!(report.actions_run, 5);
}

fn check_live_release(world: &mut EntityWorld, ctx: &WorldContext<'_>) -> Result<(), String> {
    // Interruption in the live run still returns the attached prey through
    // checked recovery. A second capture also proves the new run can re-grip.
    wait_for_latch(world, ctx)?;
    let _report = world.dispatch_on("hunter", &[ActionDef::Disable { target: None }]);
    let _reverse = world.tick(ctx);
    assert_eq!(
        world
            .guest_diagnostics()
            .pointer("/0/capture/recovering")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(
        world
            .feltling_diagnostics()
            .pointer("/0/attachment")
            .is_some_and(|value| !value.is_null())
    );
    for _ in 0_u16..120 {
        let _tick = world.tick(ctx);
        if world
            .guest_diagnostics()
            .pointer("/0/capture")
            .is_some_and(serde_json::Value::is_null)
        {
            break;
        }
    }
    assert!(
        world
            .guest_diagnostics()
            .pointer("/0/capture")
            .is_some_and(serde_json::Value::is_null)
    );
    assert!(
        world
            .feltling_diagnostics()
            .pointer("/0/attachment")
            .is_some_and(serde_json::Value::is_null)
    );
    assert!(
        world
            .feltling_diagnostics()
            .pointer("/0/reserved_by")
            .is_some_and(serde_json::Value::is_null)
    );
    Ok(())
}

// Protect both authored glow defaults at the controller-to-renderer handoff.
// Four placements share the two rigs. Three CPU scene updates resolve the
// attached emitters without navigation, a map bake or a graphics device.
#[test]
fn reset_restores_authored_glow_for_both_posed_actors() -> Result<(), String> {
    use crate::engine::entity::{EntityFrame, GlowCue};
    use crate::engine::lighting::LevelLighting;
    use crate::engine::loader::PropCatalog;
    use crate::engine::props::PropAssets;
    use crate::engine::render::CharacterScene;

    let placements = [
        ("guest_on", true),
        ("guest_off", false),
        ("feltling_on", true),
        ("feltling_off", false),
    ];
    let level = glow_fixture()?;
    let mut world = EntityWorld::from_level(&level);
    world.validate_guest_capability()?;
    let mut scene = CharacterScene::spawn_characters(
        &level,
        &PropCatalog::load_default(),
        &mut PropAssets::load_default(),
        &LevelLighting::default(),
    );
    let glow_cues = |frames: &[EntityFrame]| -> Vec<(String, Option<GlowCue>)> {
        frames
            .iter()
            .map(|frame| (frame.instance_id.clone(), frame.glow.clone()))
            .collect()
    };
    let authored_cues = glow_cues(world.entity_frames());
    assert_eq!(authored_cues.len(), placements.len());
    let _authored = scene.update(0.0, world.entity_frames());
    let authored_emitters = scene.dynamic_lights().clone();
    assert_eq!(authored_emitters.len(), 2);

    for (id, enabled) in placements {
        let key = format!("glow:{id}");
        assert_eq!(authored_emitters.get(&key).is_some(), enabled);
        let action = if enabled {
            ActionDef::Disable { target: None }
        } else {
            ActionDef::Enable { target: None }
        };
        assert_eq!(world.dispatch_on(id, &[action]).actions_run, 1);
    }
    let _changed = scene.update(0.0, world.entity_frames());
    for (id, enabled) in placements {
        assert_eq!(
            scene.dynamic_lights().get(&format!("glow:{id}")).is_some(),
            !enabled
        );
    }

    world.reset_runtime();
    let _reset = scene.update(0.0, world.entity_frames());
    assert_eq!(
        (glow_cues(world.entity_frames()), scene.dynamic_lights()),
        (authored_cues, &authored_emitters),
        "reset must republish exactly the authored glow cues and emitters"
    );
    Ok(())
}

fn glow_fixture() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{
            "schema":"places-level", "format_version":1, "id":"posed_actor_glow_reset",
            "name":"Posed actor glow reset", "spawn":{"x":2,"z":2},
            "rooms":[{"x":-2,"z":-2,"width":10,"depth":12,"height":5}],
            "props":[
                {"id":"guest_on","model":"tall-guest","x":0,"z":0,"solid":false,
                 "tall_guest":{},"components":[{"component":"glow","socket":"head","enabled":true}]},
                {"id":"guest_off","model":"tall-guest","x":4,"z":0,"solid":false,
                 "tall_guest":{},"components":[{"component":"glow","enabled":false}]},
                {"id":"feltling_on","model":"feltling","x":0,"z":5,"solid":false,
                 "feltling":{"mode":"wait"},"components":[{"component":"glow","socket":"head","enabled":true}]},
                {"id":"feltling_off","model":"feltling","x":4,"z":5,"solid":false,
                 "feltling":{"mode":"wait"},"components":[{"component":"glow","enabled":false}]}
            ]
        }"#,
    )
    .map_err(|error| error.to_string())
}

//! Small native aiming and checkpoint test, without assets, GPU or a full bake.
use super::super::{AppState, CollisionWorld, EYE_HEIGHT, Game, Settings};
use crate::engine::level::LevelDef;
use crate::game::input::InputState;
use glam::Vec3;
use serde_json::json;

fn aim(game: &Game) -> Option<String> {
    game.interaction_target()
        .and_then(|slot| game.interactables().get(slot))
        .map(|item| item.id.clone())
}

fn settle(game: &mut Game) {
    for _ in 0..35_u8 {
        game.set_benchmark_simulation_delta(1.0 / 60.0);
        game.update_player_movement(&mut InputState::default(), &Settings::default());
    }
}

fn assert_power(game: &Game, on: bool) -> Result<(), String> {
    let states = game.entities().light_states();
    assert_eq!(
        states.len(),
        usize::from(FIXTURES),
        "every authored fixture remains present"
    );
    assert!(
        states.iter().all(|(_, enabled)| *enabled == on),
        "master power applies to the entire circuit"
    );
    let panel = game
        .entities()
        .electrical_panels()
        .first()
        .ok_or("missing cabinet")?;
    let [_, lever] = panel.parts();
    let visual_tip = lever.rotation.mul_vec3(Vec3::Z).y;
    assert!(
        panel.powered() == on
            && if on {
                visual_tip > 0.5
            } else {
                visual_tip < -0.5
            },
        "settled lever pose matches the latched output"
    );
    Ok(())
}

fn assert_fixture_emission_handoff(game: &mut Game, on: bool) {
    let toggles = game.take_light_toggles();
    assert_eq!(
        toggles.len(),
        usize::from(FIXTURES),
        "every fixture reaches the native renderer"
    );
    assert!(
        toggles.iter().all(|(_, enabled)| *enabled == on),
        "lit faces and compiled emitter controls receive the same power state"
    );
}

const FIXTURES: u16 = 27;

fn circuit_scene() -> Result<LevelDef, String> {
    let lights: Vec<_> = (0..FIXTURES)
        .map(|i| {
            let (fixture, mount) = if i < 23 {
                ("core:fluorescent_panel_01", "ceiling")
            } else {
                ("maintenance:warm_bulkhead", "wall")
            };
            json!({
                "id":format!("light_{i}"),"fixture":fixture,"mount":mount,"x":1_u32,"y":2.1_f32,"z":1_u32,
                "switch_group":"main","enabled":true,"switchable":true
            })
        })
        .collect();
    let level: LevelDef = serde_json::from_value(json!({
        "schema":"places-level","format_version":1_u32,"id":"panel","name":"Electrical panel",
        "saving":true,"requires_full_lighting":true,"spawn":{"x":2_u32,"z":2_u32},
        "rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],
        "walls":[{"x":0_u32,"z":0.8_f32,"width":7_u32,"depth":0.198_f32,"height":3_u32}],
        "props":[{
            "id":"panel","model":"maintenance:electrical_panel","x":2_u32,"y":EYE_HEIGHT - 0.02,"z":1_u32,
            "solid":true,"electrical_panel":{},"light_switch":"main","components":[
                {"component":"interactable"},{"component":"state","name":"noticed","value":false}
            ],"bindings":[{"on":"interact","key":"off","actions":[{"action":"set_state","name":"noticed","value":true}]}]
        }],"ceiling_lights":lights
    })).map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    Ok(level)
}

#[test]
fn electrical_panel_lever_stays_independent_and_can_repower_a_dark_room() -> Result<(), String> {
    let level = circuit_scene()?;
    let eye = Vec3::new(2.0, EYE_HEIGHT, 2.0);
    let clear_eye = Vec3::new(2.0, EYE_HEIGHT, 2.55);
    let mut game = Game::new(eye, 0.0, CollisionWorld::from_level(&level));
    game.set_app_state(AppState::Playing);
    assert_power(&game, true)?;
    assert_eq!(
        aim(&game).as_deref(),
        Some("@panel:panel:door"),
        "closed door covers the lever"
    );
    let lever_aim = game
        .interactables()
        .index_of("panel")
        .ok_or("missing lever")?;
    assert!(
        game.dispatch_interaction_at(Some(lever_aim)).is_none(),
        "even explicit stale aim cannot use the closed lever"
    );
    let opened = game.dispatch_interaction().ok_or("cannot open cabinet")?;
    assert_eq!(opened.lights_toggled, 0, "opening never affects output");
    settle(&mut game);
    assert!(
        game.entities()
            .electrical_panels()
            .first()
            .is_some_and(|panel| !panel.lever_accessible()),
        "the player safely blocks the swing when standing too close"
    );
    game.set_benchmark_player_position(clear_eye, 0.0, 0.0);
    settle(&mut game);
    assert_eq!(
        aim(&game).as_deref(),
        Some("panel"),
        "the exposed lever becomes the native aim target"
    );
    let off = game
        .dispatch_interaction()
        .ok_or("cannot switch off power")?;
    assert_eq!(off.lights_toggled, 1, "one native circuit transaction");
    assert_eq!(
        off.actions_run, 2,
        "existing keyed wiring reacts to lever movement only"
    );
    settle(&mut game);
    assert_power(&game, false)?;
    assert_fixture_emission_handoff(&mut game, false);
    let door = game
        .interactables()
        .index_of("@panel:panel:door")
        .ok_or("missing door")?;
    let closing = game
        .dispatch_interaction_at(Some(door))
        .ok_or("cannot close cabinet")?;
    assert_eq!(closing.lights_toggled, 0, "closing leaves the power off");
    settle(&mut game);
    let checkpoint = game.entities().checkpoint()?;
    let mut resumed = Game::new(clear_eye, 0.0, CollisionWorld::from_level(&level));
    resumed.set_app_state(AppState::Playing);
    resumed.entities_mut().restore_checkpoint(&checkpoint)?;
    assert_power(&resumed, false)?;
    let reopening = resumed
        .dispatch_interaction()
        .ok_or("cannot aim at the door in darkness")?;
    assert_eq!(
        reopening.lights_toggled, 0,
        "reopening restores access, not power"
    );
    settle(&mut resumed);
    assert_eq!(
        aim(&resumed).as_deref(),
        Some("panel"),
        "lever prompt remains targetable with all LEDs off"
    );
    let _on = resumed
        .dispatch_interaction()
        .ok_or("cannot restore power from darkness")?;
    settle(&mut resumed);
    assert_power(&resumed, true)?;
    assert_fixture_emission_handoff(&mut resumed, true);
    Ok(())
}

#[test]
fn electrical_panel_does_not_broaden_trigger_or_local_light_controls() -> Result<(), String> {
    use crate::engine::{entities::events::EventKind, level::ActionDef};
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"panel_isolation","name":"Separate controls",
        "requires_full_lighting":true,"spawn":{"x":2,"z":2.55},
        "rooms":[{"width":8,"depth":8,"height":3}],
        "walls":[{"x":0,"z":0.8,"width":7,"depth":0.198,"height":3}],
        "props":[{"id":"panel","model":"maintenance:electrical_panel","solid":true,"x":2,"y":1.6,"z":1,
            "electrical_panel":{},"light_switch":"master","components":[{"component":"interactable"}]},
            {"id":"local_switch","model":"home:wall_switch","x":4,"z":1,"light_switch":"local","components":[{"component":"interactable"}]}],
        "ceiling_lights":[
          {"id":"master_a","fixture":"core:fluorescent_panel_01","x":1,"z":2,"switchable":true,"switch_group":"master"},
          {"id":"master_b","fixture":"core:fluorescent_panel_01","x":2,"z":2,"switchable":true,"switch_group":"master"},
          {"id":"local_a","fixture":"core:fluorescent_panel_01","x":3,"z":2,"switchable":true,"switch_group":"local"},
          {"id":"local_b","fixture":"core:fluorescent_panel_01","x":4,"z":2,"switchable":true,"switch_group":"local"},
          {"id":"isolated","fixture":"core:fluorescent_panel_01","x":5,"z":2,"switchable":true}],
        "volumes":[{"id":"trigger","x":6,"z":6,"width":1,"depth":1,"bottom_y":0,"top_y":2,"bindings":[
          {"on":"enter_volume","actions":[{"action":"set_light","target":"isolated","on":false}]}]}]
    }"#).map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    let eye = Vec3::new(2.0, EYE_HEIGHT, 2.55);
    let mut game = Game::new(eye, 0.0, CollisionWorld::from_level(&level));
    game.set_app_state(AppState::Playing);
    let read = |state: &Game| {
        state
            .entities()
            .light_states()
            .into_iter()
            .map(|(_, enabled)| enabled)
            .collect::<Vec<_>>()
    };
    let refused = game.dispatch_actions(
        &[ActionDef::SetLight {
            target: Some("master_a".into()),
            on: false,
        }],
        None,
    );
    assert_eq!(
        refused.unsupported, 1,
        "one member cannot change the panel circuit by action"
    );
    assert_eq!(
        read(&game),
        [true; 5],
        "direct command does not affect the panel or its neighbors"
    );
    let _open = game.dispatch_interaction().ok_or("panel door")?;
    settle(&mut game);
    let _off = game.dispatch_interaction().ok_or("panel lever")?;
    let local = game
        .interactables()
        .index_of("local_switch")
        .ok_or("local switch")?;
    let _local = game
        .dispatch_interaction_at(Some(local))
        .ok_or("local interaction")?;
    let trigger = game
        .entities()
        .handle_of("trigger")
        .ok_or("fixture trigger")?;
    assert!(
        game.entities_mut()
            .emit(EventKind::EnterVolume, trigger, "", None),
        "trigger event enqueued"
    );
    settle(&mut game);
    assert_eq!(
        read(&game),
        [false; 5],
        "local switch and isolated trigger work with the panel off"
    );
    let checkpoint = game.entities().checkpoint()?;
    let mut resumed = Game::new(eye, 0.0, CollisionWorld::from_level(&level));
    resumed.set_app_state(AppState::Playing);
    resumed.entities_mut().restore_checkpoint(&checkpoint)?;
    assert_eq!(
        read(&resumed),
        [false; 5],
        "all three control states restore separately"
    );
    let _on = resumed
        .dispatch_interaction()
        .ok_or("restore panel power")?;
    assert_eq!(
        read(&resumed),
        [true, true, false, false, false],
        "restoring the panel cannot repower an independent circuit or fixture"
    );
    Ok(())
}

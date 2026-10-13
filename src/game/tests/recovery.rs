//! Bounded geometry and reset regressions; no production maps or prepared bakes.

use super::*;

fn recovery_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
        "format_version":3,"id":"escape_recovery","name":"Escape recovery",
        "spawn":{"x":1,"z":1,"yaw_degrees":90},
        "rooms":[
            {"x":0,"z":0,"width":4,"depth":4,"height":3},
            {"x":8,"z":0,"width":4,"depth":4,"floor_y":10,"height":3},
            {"x":0,"z":8,"width":4,"depth":4,"height":6,"ceiling":{"kind":"open"}}
        ],
        "floor_regions":[{"x":2,"z":0,"width":2,"depth":4,"offset_y":-4}],
        "ramps":[{"x":0,"z":8,"width":1,"depth":4,"rise":4}],
        "water":[{"shape":"circle","x":14,"z":0,"radius":2,"surface_y":-1,"bottom_y":-5}],
        "props":[{"id":"marker","model":"core:plant","x":1,"z":2,
            "components":[{"component":"interactable"}]}],
        "volumes":[{"id":"return_crossing","x":4.5,"z":0,"width":0.5,"depth":4,
            "bottom_y":-100,"top_y":100,
            "bindings":[{"on":"enter_volume","actions":[{"action":"toggle_label","target":"marker"}]}]}]
        }"#,
    )
    .expect("bounded recovery fixture")
}

/// A room union must retain its gaps and local vertical reach; a global
/// bounding box would incorrectly accept the gap and the circular pool corner.
#[test]
fn escape_detection_preserves_recesses_storeys_ramps_and_round_water() {
    let level = recovery_level();
    let mut game = game_for(&level);
    for (feet, outside) in [
        (Vec3::new(1.0, JUMP_APEX_M, 1.0), false),
        (Vec3::new(3.0, -4.0, 2.0), false),
        (Vec3::new(9.0, 10.0, 2.0), false),
        (Vec3::new(0.5, 3.0, 11.0), false),
        (Vec3::new(16.0, -4.0, 2.0), false),
        (Vec3::new(6.0, 0.0, 2.0), true),
        (Vec3::new(14.2, -4.0, 0.2), true),
        (Vec3::new(1.0, 5.0, 1.0), true),
        (Vec3::new(3.0, -6.1, 2.0), true),
        (Vec3::new(16.0, -7.0, 2.0), true),
    ] {
        game.player_position = Vec3::new(feet.x, feet.y + EYE_HEIGHT, feet.z);
        game.feet_y = feet.y;
        assert_eq!(game.player_is_out_of_map(), outside, "feet {feet:?}");
    }
}

/// Recovery clears player motion/stance and held-key state, preserves entity
/// state, and re-baselines triggers so a teleport cannot sweep a remote volume.
#[test]
fn escape_recovery_restores_once_without_motion_or_cross_map_trigger_edges() {
    let level = recovery_level();
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    let marker = game.interactables().index_of("marker").expect("marker");
    let _label_report = game.dispatch_actions(
        &[ActionDef::ToggleLabel {
            target: Some("marker".into()),
        }],
        None,
    );
    game.player_position = Vec3::new(6.0, -10.0, 2.0);
    game.feet_y = -10.0 - CROUCH_EYE_HEIGHT;
    game.eye_offset_current = CROUCH_EYE_HEIGHT;
    game.stance = Stance::Crouched;
    game.horizontal_velocity = Vec2::new(3.0, 1.0);
    game.ice_airborne = true;
    game.vertical_velocity = -8.0;
    game.vertical_accumulator = 0.004;
    game.grounded = false;
    game.swimming = true;
    game.water_exit = Some(WaterExit {
        support_y: 0.0,
        surface_y: -1.0,
    });
    game.pressed_exit_support = Some(0.0);
    game.float_hold = true;
    game.climbing = Some(0);
    game.reseed_trigger_inside();
    game.sim_delta_seconds = 0.1;
    assert!(
        !game.recover_out_of_map(),
        "a brief excursion receives grace"
    );
    game.outside_map_seconds = 1.0;
    assert!(game.recover_out_of_map(), "a sustained escape recovers");
    assert_eq!(game.player_position, spawn_position(&level));
    assert_eq!(game.reset_count(), 1);
    assert!(game.grounded && game.stance == Stance::Standing);
    assert_eq!(game.horizontal_velocity, Vec2::ZERO);
    assert_exact(game.vertical_velocity, 0.0);
    assert_exact(game.vertical_accumulator, 0.0);
    assert!(!game.ice_airborne && !game.swimming && !game.float_hold);
    assert!(
        game.water_exit.is_none() && game.pressed_exit_support.is_none() && game.climbing.is_none()
    );
    assert!(
        game.is_label_visible(marker),
        "recovery preserves entity state"
    );
    game.sim_delta_seconds = 1.0 / 60.0;
    game.update_player_movement(
        &mut InputState::holding(&[Control::Jump, Control::Crouch, Control::Interact]),
        &Settings::default(),
    );
    assert!(
        game.grounded && game.stance == Stance::Standing,
        "held keys stay latched after recovery"
    );
    assert!(
        !game.take_interact_press(),
        "held Interact does not dispatch"
    );
    assert!(
        game.is_label_visible(marker),
        "teleport cannot trigger the remote crossing"
    );
    assert_eq!(game.reset_count(), 1, "the restored pose cannot loop");
    game.player_position.x = f32::NAN;
    game.sim_delta_seconds = 1.0 / 60.0;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert_eq!(
        game.player_position,
        spawn_position(&level),
        "non-finite poses recover before collision"
    );
    assert_eq!(game.reset_count(), 2);
}

/// A map with an invalid spawn must never repeatedly teleport into that same
/// invalid location. No candidate is installed without clear body/support.
#[test]
fn an_invalid_spawn_cannot_become_a_recovery_loop() {
    let level = recovery_level();
    let mut game = Game::new(
        Vec3::new(6.0, EYE_HEIGHT, 2.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    assert!(game.recovery_spawn.is_none(), "gap spawn has no support");
    game.outside_map_seconds = 1.0;
    game.sim_delta_seconds = 0.1;
    assert!(!game.recover_out_of_map(), "invalid destination is refused");
    assert_eq!(game.reset_count(), 0);
}

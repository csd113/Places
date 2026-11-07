//! Essential regression: an out-of-body lease must not freeze gravity or accept
//! stale movement/look/interact when it ends. One tiny metadata-only level and
//! tens of CPU ticks exercise the actual Game controller, not a toy input copy;
//! no renderer, assets, process, bake or wall-clock wait is needed.
use glam::Vec3;

use crate::engine::level::{ActionDef, LevelDef};
use crate::game::input::{Control, InputState, ScriptedHold};
use crate::game::settings::Settings;
use crate::game::{AppState, CROUCH_EYE_HEIGHT, CollisionWorld, EYE_HEIGHT, Game, Stance};

fn press(input: &mut InputState) {
    let holds = [
        Control::MoveForward,
        Control::LookRight,
        Control::Jump,
        Control::Interact,
    ]
    .map(|control| ScriptedHold {
        control,
        first_seconds: 0.0,
        last_seconds: 10.0,
    });
    input.apply_move_script(&holds, 0.0);
    input.accumulate_mouse_motion(80.0, -35.0);
}

fn start(game: &mut Game) {
    let report = game.entities_mut().dispatch_on(
        "controller",
        &[ActionDef::StartSequence {
            sequence: "view".into(),
            target: None,
        }],
    );
    assert_eq!(
        report.sequences_started, 1,
        "a fresh cinematic must acquire the view"
    );
}

#[test]
fn cinematic_controls_keep_live_physics_and_restore_only_fresh_input() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
      "schema":"places-level","format_version":1,"id":"camera_control","name":"Camera control",
      "spawn":{"x":2,"z":2},"rooms":[{"width":8,"depth":8,"height":6}],
      "props":[{"id":"controller","model":"core:rubber_duck","x":1,"z":1}],
      "sequences":[{"id":"view","cinematic":{"start":{"position":[5,3,5],"yaw_degrees":90,"pitch_degrees":0}},
        "steps":[{"step":"camera","seconds":2,"motion":{"kind":"move","position":[5,5,1],"easing":"linear"},"aim":{"kind":"keep"}}]}]
    }"#).map_err(|error| error.to_string())?;
    let mut game = Game::new(
        Vec3::new(2.0, EYE_HEIGHT, 2.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    game.set_app_state(AppState::Playing);
    game.stance = Stance::Crouched;
    game.duck_fraction = 1.0;
    game.eye_offset_current = CROUCH_EYE_HEIGHT;
    game.set_feet_y(1.0);
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::default();
    press(&mut input);
    start(&mut game);
    for _ in 0_u8..20 {
        press(&mut input);
        game.set_benchmark_simulation_delta(1.0 / 60.0);
        game.update_player_movement(&mut input, &settings);
        assert!(
            !game.take_interact_press(),
            "remote view cannot interact through the body"
        );
    }
    assert!(
        game.feet_y() < 0.8,
        "gravity must continue while controls are leased"
    );
    assert_eq!((game.player_position.x, game.player_position.z), (2.0, 2.0));
    assert_eq!(
        (game.player_yaw, game.player_pitch),
        (0.0, 0.0),
        "real look input must not mutate the return view"
    );
    assert!(
        game.is_crouched(),
        "neutral camera input must not uncrouch a live body"
    );
    assert!(
        game.entities()
            .cinematic_view()
            .is_some_and(|view| view.position.distance(game.player_position) > 2.0)
    );
    let view = game.entities().cinematic_view();
    game.set_app_state(AppState::Paused);
    game.set_benchmark_simulation_delta(0.2);
    game.update_player_movement(&mut input, &settings);
    assert_eq!(
        game.entities().cinematic_view(),
        view,
        "pause cannot advance a shot"
    );
    game.set_app_state(AppState::Playing);
    game.entities_mut().cancel_cinematic();
    press(&mut input);
    game.set_benchmark_simulation_delta(1.0 / 60.0);
    game.update_player_movement(&mut input, &settings);
    assert_eq!(
        (game.player_position.x, game.player_position.z),
        (2.0, 2.0),
        "a stale edge on release cannot move the body"
    );
    assert_eq!((game.player_yaw, game.player_pitch), (0.0, 0.0));
    assert!(!game.take_interact_press());
    press(&mut input);
    game.set_benchmark_simulation_delta(1.0 / 60.0);
    game.update_player_movement(&mut input, &settings);
    assert!(
        game.player_yaw != 0.0,
        "a fresh look press must regain control"
    );
    assert!(
        game.take_interact_press(),
        "a fresh interaction must regain control"
    );
    start(&mut game);
    let _respawn = game.developer_respawn()?;
    assert!(
        !game.cinematic_owns_controls(),
        "player recovery cannot strand an out-of-body view"
    );
    Ok(())
}

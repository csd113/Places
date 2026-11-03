//! Seven tiny CPU fixtures protect real lift travel, door safety and save boundaries.
//! No GPU, model I/O or lighting bake; each replay advances at most 1,500 ticks.
use super::super::{AppState, CollisionWorld, EYE_HEIGHT, Game, Settings};
use crate::engine::elevator::{Elevator, Phase};
use crate::engine::level::LevelDef;
use crate::game::input::{Control, InputState, ScriptedHold};
use glam::Vec3;

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"lift_test","name":"Lift test","saving":true,"spawn":{"x":0,"z":-1},"transitions":["annex"],"rooms":[{"x":-3,"z":-4,"width":6,"depth":4,"height":2.8},{"x":-3,"z":-4,"width":6,"depth":4,"height":2.8,"floor_y":3.5}],"elevators":[{"id":"lift","x":0,"z":0,"stops":[{"label":"G","y":0},{"label":"1","y":3.5},{"label":"Annex","y":0,"level":"annex","arrival_elevator":"arrival"}]}]}"#).map_err(|e|e.to_string())
}
fn game() -> Result<Game, String> {
    let def = level()?;
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, -1.0),
        std::f32::consts::PI,
        CollisionWorld::from_level(&def),
    );
    game.set_app_state(AppState::Playing);
    Ok(game)
}
fn tick(game: &mut Game, seconds: f32, input: &mut InputState) {
    game.set_benchmark_simulation_delta(seconds);
    game.update_player_movement(input, &Settings::default());
}
fn interact(game: &mut Game, id: &str) -> Result<(), String> {
    let index = game
        .interactables()
        .index_of(id)
        .ok_or("Missing lift control")?;
    let _report = game
        .dispatch_interaction_at(Some(index))
        .ok_or("Missing dispatch")?;
    Ok(())
}
fn settle(game: &mut Game, ticks: usize) {
    for _ in 0..ticks {
        tick(game, 1.0 / 60.0, &mut InputState::default());
    }
}
fn lift(game: &Game) -> Result<&Elevator, String> {
    game.entities()
        .elevators()
        .first()
        .ok_or_else(|| "Missing lift".into())
}

#[test]
fn floor_ride_carries_real_controller_without_escape_recovery() -> Result<(), String> {
    for hz in [30_u16, 120] {
        let mut game = game()?;
        interact(&mut game, "@elevator:lift:call:0")?;
        settle(&mut game, 100);
        game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
        interact(&mut game, "@elevator:lift:floor:1")?;
        for _ in 0..usize::from(hz).saturating_mul(7) {
            tick(&mut game, 1.0 / f32::from(hz), &mut InputState::default());
        }
        assert!(
            (game.feet_y() - 3.5).abs() < 0.002,
            "rider stays on the moving cabin at both rates"
        );
        assert_eq!(
            lift(&game)?.phase,
            Phase::Open,
            "arrival opens the correct landing"
        );
        assert!(
            !game.player_is_out_of_map(),
            "the side shaft is valid occupied space"
        );
        assert_eq!(
            game.world_reset_count(),
            0,
            "travel never triggers escape recovery"
        );
    }
    Ok(())
}

#[test]
fn real_movement_stops_at_closed_leaves_and_enters_open_cabin() -> Result<(), String> {
    let mut game = game()?;
    let mut forward = InputState::default();
    forward.apply_move_script(
        &[ScriptedHold {
            control: Control::MoveForward,
            first_seconds: 0.0,
            last_seconds: 20.0,
        }],
        1.0,
    );
    for _ in 0_u16..50 {
        tick(&mut game, 1.0 / 60.0, &mut forward);
    }
    assert!(
        game.player_position.z < 0.0,
        "closed visual leaves block the real controller"
    );
    interact(&mut game, "@elevator:lift:call:0")?;
    settle(&mut game, 100);
    for _ in 0_u16..30 {
        tick(&mut game, 1.0 / 60.0, &mut forward);
    }
    assert!(
        game.player_position.z > 0.4,
        "opened center gap admits a walking player"
    );
    assert!(
        (game.feet_y()).abs() < 0.002,
        "threshold and cabin floor are flush"
    );
    Ok(())
}

#[test]
fn obstructed_departure_reopens_and_never_moves_the_car() -> Result<(), String> {
    let mut game = game()?;
    interact(&mut game, "@elevator:lift:call:0")?;
    settle(&mut game, 100);
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
    interact(&mut game, "@elevator:lift:floor:1")?;
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 0.0), 0.0, 0.0);
    settle(&mut game, 100);
    assert_eq!(
        lift(&game)?.phase,
        Phase::Open,
        "door beam reopens on an occupant"
    );
    assert!(
        lift(&game)?.y.abs() < 0.001,
        "blocked departure stays at its landing"
    );
    Ok(())
}

#[test]
fn level_button_requires_cabin_and_emits_once_after_door_closure() -> Result<(), String> {
    let mut game = game()?;
    interact(&mut game, "@elevator:lift:floor:2")?;
    settle(&mut game, 2);
    assert!(
        game.entities_mut().take_elevator_departure().is_none(),
        "hall player cannot trigger cabin travel"
    );
    interact(&mut game, "@elevator:lift:call:0")?;
    settle(&mut game, 100);
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
    interact(&mut game, "@elevator:lift:floor:2")?;
    settle(&mut game, 20);
    assert!(
        game.entities_mut().take_elevator_departure().is_none(),
        "loading waits for real leaf closure"
    );
    settle(&mut game, 100);
    let departure = game
        .entities_mut()
        .take_elevator_departure()
        .ok_or("No departure")?;
    assert_eq!(
        departure.level, "annex",
        "declared project destination is retained"
    );
    assert_eq!(
        departure.arrival, "arrival",
        "destination cabin is explicit"
    );
    settle(&mut game, 20);
    assert!(
        game.entities_mut().take_elevator_departure().is_none(),
        "a held request cannot reload repeatedly"
    );
    game.entities_mut().recover_elevators();
    settle(&mut game, 100);
    assert_eq!(
        lift(&game)?.phase,
        Phase::Open,
        "cancel or failure releases the old cabin"
    );
    Ok(())
}

#[test]
fn checkpoint_restores_landing_and_refuses_a_trip_in_progress() -> Result<(), String> {
    let mut game = game()?;
    let initial = game.entities().checkpoint()?;
    interact(&mut game, "@elevator:lift:call:0")?;
    assert!(
        game.entities().checkpoint().is_err(),
        "moving leaves cannot be saved as a stable landing"
    );
    settle(&mut game, 100);
    let open = game.entities().checkpoint()?;
    game.entities_mut().restore_checkpoint(&initial)?;
    assert_eq!(
        lift(&game)?.phase,
        Phase::Closed,
        "closed stop survives reconstruction"
    );
    game.entities_mut().restore_checkpoint(&open)?;
    assert_eq!(
        lift(&game)?.phase,
        Phase::Open,
        "open stop survives reconstruction"
    );
    game.arrive_elevator("lift")?;
    assert!(
        (game.player_position.z - 1.2).abs() < 0.001,
        "linked load places the player inside the destination cabin"
    );
    assert!(
        game.arrive_elevator("missing").is_err(),
        "missing destinations fail explicitly"
    );
    Ok(())
}

#[test]
fn invalid_stops_and_undeclared_loading_targets_are_rejected() -> Result<(), String> {
    let mut def = level()?;
    crate::engine::elevator::validate(&def)?;
    def.transitions.clear();
    assert!(
        crate::engine::elevator::validate(&def).is_err(),
        "level loading cannot escape declared project destinations"
    );
    def = level()?;
    let elevator = def.elevators.first_mut().ok_or("No lift")?;
    elevator.initial_stop = 2;
    assert!(
        crate::engine::elevator::validate(&def).is_err(),
        "spawn cannot put a car at an external stop"
    );
    Ok(())
}

#[test]
fn upper_cabin_spawn_and_crossed_shaft_have_explicit_contracts() -> Result<(), String> {
    let mut def = level()?;
    def.spawn.z = 1.2;
    def.elevators.first_mut().ok_or("No lift")?.initial_stop = 1;
    let eye = crate::game::spawn_position(&def);
    assert!(
        (eye.y - 3.5 - EYE_HEIGHT).abs() < 0.001,
        "an authored cabin spawn resolves its upper deck"
    );
    let game = Game::new(eye, 0.0, CollisionWorld::from_level(&def));
    assert!(
        (game.feet_y() - 3.5).abs() < 0.001,
        "initial recovery retains the intended upper cabin"
    );
    def.rooms.first_mut().ok_or("No room")?.depth = 7.0;
    assert!(
        crate::engine::elevator::validate(&def).is_err(),
        "a room ceiling cannot be silently passed through by the car"
    );
    Ok(())
}

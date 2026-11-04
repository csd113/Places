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
            // Protect the reported rising floor: real render parts stay fixed on
            // every ride tick, and the rider changes landing in one sealed step.
            // This extends the existing tiny replay; no assets or bake are added.
            let cabins: Vec<_> = crate::engine::render::elevator_parts(lift(&game)?)
                .into_iter()
                .filter_map(|(part, position)| {
                    matches!(part, crate::engine::render::ElevatorPart::Cabin(_))
                        .then_some(position[1])
                })
                .collect();
            assert_eq!(
                cabins,
                vec![0.0, 3.5],
                "Both cabin decks remain at their authored positions"
            );
            assert!(
                game.feet_y().abs() < 0.002 || (game.feet_y() - 3.5).abs() < 0.002,
                "No intermediate physical ascent"
            );
        }
        assert!(
            (game.feet_y() - 3.5).abs() < 0.002,
            "rider teleports between fixed cabins at both rates"
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
    airborne_rider_reopens_at_source()
}

// Native Space input at the sealed transfer exposed a new stationary-ride
// regression: an airborne rider became a hall call and recovered to spawn.
// This reuses the tiny controller fixture (<800 ticks, no assets/bake), checks
// source pose/control continuity, and proves an ordinary grounded retry works.
fn airborne_rider_reopens_at_source() -> Result<(), String> {
    let mut game = game()?;
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
    interact(&mut game, "@elevator:lift:floor:1")?;
    settle(&mut game, 100);
    assert_eq!(lift(&game)?.phase, Phase::Moving);
    let mut jump = InputState::default();
    jump.apply_move_script(
        &[ScriptedHold {
            control: Control::Jump,
            first_seconds: 0.0,
            last_seconds: 1.0,
        }],
        0.5,
    );
    tick(&mut game, 1.0 / 60.0, &mut jump);
    settle(&mut game, 200);
    assert_eq!(lift(&game)?.at, 0, "Airborne admission retains the source");
    assert_eq!(lift(&game)?.phase, Phase::Open);
    assert!(
        game.player_position
            .distance(Vec3::new(0.0, EYE_HEIGHT, 1.2))
            < 0.002
    );
    assert_eq!(game.world_reset_count(), 0);
    interact(&mut game, "@elevator:lift:floor:1")?;
    settle(&mut game, 420);
    assert_eq!(lift(&game)?.at, 1);
    assert!(
        (game.feet_y() - 3.5).abs() < 0.002,
        "Grounded retry still travels"
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
    assert_eq!(
        lift(&game)?.phase,
        Phase::Loading,
        "Closed doors retain the simulated ride before loading"
    );
    assert!(
        game.entities_mut().take_elevator_departure().is_none(),
        "The motor cue has a real presentation interval"
    );
    settle(&mut game, 100);
    let departure = game
        .entities_mut()
        .take_elevator_departure()
        .ok_or("No departure")?;
    assert_eq!(
        departure.level.as_deref(),
        Some("annex"),
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
    let mut advanced = level()?;
    advanced.elevators.first_mut().ok_or("No lift")?.stops = (0_u16..8)
        .map(|i| crate::engine::elevator::Stop {
            label: i.to_string(),
            y: f32::from(i) * 3.5,
            level: None,
            arrival_elevator: None,
        })
        .collect();
    crate::engine::elevator::validate(&advanced)?;
    let mut too_many = ports()?;
    too_many
        .elevators
        .first_mut()
        .and_then(|e| e.stops.get_mut(2))
        .ok_or("No link")?
        .arrival_elevator = Some("d".into());
    assert!(
        crate::engine::elevator::validate(&too_many).is_err(),
        "raw data cannot merge independent groups beyond three cabins"
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

// These two small controller regressions cover linked-port behavior absent from
// shaft travel: same-height transfer/independent groups and busy-target rollback.
// They use no model loading, renderer or map bake; only a few hundred CPU ticks.
fn ports() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"ports","name":"Ports","spawn":{"x":0,"z":-1},"rooms":[],"elevators":[
      {"id":"a","x":0,"z":0,"stops":[{"label":"1","y":0},{"label":"4","y":0,"arrival_elevator":"b"},{"label":"8","y":7,"arrival_elevator":"c"}]},
      {"id":"b","x":8,"z":0,"rotation_degrees":90,"initial_stop":1,"stops":[{"label":"1","y":0,"arrival_elevator":"a"},{"label":"4","y":0},{"label":"8","y":7,"arrival_elevator":"c"}]},
      {"id":"c","x":8,"z":0,"initial_stop":2,"stops":[{"label":"1","y":0,"arrival_elevator":"a"},{"label":"4","y":0,"arrival_elevator":"b"},{"label":"8","y":7}]},
      {"id":"d","x":16,"z":0,"stops":[{"label":"A","y":0},{"label":"B","y":3.5,"arrival_elevator":"e"}]},
      {"id":"e","x":16,"z":0,"initial_stop":1,"stops":[{"label":"A","y":0,"arrival_elevator":"d"},{"label":"B","y":3.5}]}]}"#).map_err(|e|e.to_string())
}
#[test]
fn linked_ports_preserve_view_and_isolate_groups_at_any_height() -> Result<(), String> {
    let def = ports()?;
    let mut game = port_game(&def);
    interact(&mut game, "@elevator:a:call:0")?;
    settle(&mut game, 100);
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.35, -0.1);
    interact(&mut game, "@elevator:a:floor:1")?;
    settle(&mut game, 360);
    assert!(
        (game.player_position.x - 9.2).abs() < 0.002 && game.feet_y().abs() < 0.002,
        "same-plane transfer uses the linked cabin pose"
    );
    assert!(
        (game.player_yaw - (0.35 - std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU))
            .abs()
            < 0.002
            && (game.player_pitch + 0.1).abs() < 0.002,
        "view is preserved relative to the identical cabin"
    );
    interact(&mut game, "@elevator:b:floor:2")?;
    settle(&mut game, 360);
    assert!(
        (game.player_position.x - 8.0).abs() < 0.002 && (game.feet_y() - 7.0).abs() < 0.002,
        "labels 1,4,8 do not encode world height"
    );
    for lift in game.entities().elevators() {
        let buttons = lift
            .controls()
            .into_iter()
            .filter(|control| control.id.contains(":floor:"))
            .collect::<Vec<_>>();
        assert_eq!(buttons.len(), lift.def.stops.len().saturating_sub(1));
        assert!(
            buttons
                .iter()
                .all(|button| button.id != format!("@elevator:{}:floor:{}", lift.def.id, lift.at)),
            "current stop has no destination button"
        );
        if lift.def.id == "d" || lift.def.id == "e" {
            assert_eq!(
                lift.phase,
                Phase::Closed,
                "unrelated network stays untouched"
            );
        }
    }
    assert!(
        game.entities_mut().take_commands().is_empty(),
        "same-map transfer never reloads a level"
    );
    Ok(())
}
#[test]
fn local_departure_reopens_when_an_advanced_arrival_is_busy() -> Result<(), String> {
    let mut level = ports()?;
    level.elevators.truncate(2);
    let source = level.elevators.first_mut().ok_or("Missing source")?;
    source.stops.truncate(2);
    let target = level.elevators.get_mut(1).ok_or("Missing target")?;
    target.rotation_degrees = 0.0;
    target.initial_stop = 0;
    target.stops=serde_json::from_value(serde_json::json!([{"label":"G","y":0.0_f32},{"label":"Upper","y":7.0_f32},{"label":"Return","y":0.0_f32,"arrival_elevator":"a"}])).map_err(|e|e.to_string())?;
    crate::engine::elevator::validate(&level)?;
    let mut game = port_game(&level);
    interact(&mut game, "@elevator:a:call:0")?;
    settle(&mut game, 100);
    game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
    interact(&mut game, "@elevator:b:call:1")?;
    interact(&mut game, "@elevator:a:floor:1")?;
    settle(&mut game, 360);
    assert!(
        game.player_position.x.abs() < 0.002 && game.feet_y().abs() < 0.002,
        "busy arrival leaves the player in the source cabin"
    );
    assert_eq!(
        game.entities()
            .elevators()
            .first()
            .ok_or("Missing source")?
            .phase,
        Phase::Open,
        "only the source safely reopens"
    );
    Ok(())
}

// Cross-world admission cannot be covered by the existing same-world ride
// test: exercise exact offsets/view/stance and reject a blocked private target.
// Twelve tiny candidates need no assets, solver, renderer or lighting bake.
#[test]
fn cross_level_rider_admission_preserves_cabin_pose_or_rejects() -> Result<(), String> {
    for rotation in [0.0_f32, 90.0, 37.0, -135.0] {
        for (x, z, crouched) in [
            (-0.3_f32, 1.15_f32, false),
            (0.3, 1.6, false),
            (0.0, 1.3, true),
        ] {
            let mut def = ports()?;
            def.elevators
                .get_mut(2)
                .ok_or("Missing target")?
                .rotation_degrees = rotation;
            let mut source = port_game(&def);
            source.set_benchmark_player_position(Vec3::new(x, EYE_HEIGHT, z), 0.6, -0.2);
            if crouched {
                source.stance = super::super::Stance::Crouched;
                source.eye_offset_current = super::super::CROUCH_EYE_HEIGHT;
                source.duck_fraction = 1.0;
                source.set_feet_y(0.0);
            }
            let departure = crate::engine::elevator::Departure {
                landing: None,
                source: "a".into(),
                arrival: "c".into(),
                level: Some("next".into()),
            };
            let arrival = source.elevator_departure_pose(&departure)?;
            let mut target = port_game(&def);
            target
                .arrive_elevator_from(&arrival)
                .map_err(|e| format!("rotation={rotation} x={x} z={z} crouched={crouched}: {e}"))?;
            let lift = def.elevators.get(2).ok_or("Missing target")?;
            let expected = lift.point(x, 7.0, z);
            assert!((target.player_position.x - expected.x).abs() < 0.0001);
            assert!((target.player_position.z - expected.z).abs() < 0.0001);
            assert!((target.feet_y() - 7.0).abs() < 0.0001);
            let expected_view =
                glam::Quat::from_rotation_y(rotation.to_radians()) * source.view_direction();
            assert!((target.view_direction() - expected_view).length() < 0.0001);
            assert_eq!(source.is_crouched(), target.is_crouched());
            assert!((source.eye_offset() - target.eye_offset()).abs() < 0.0001);
            // The same record must reject a blocked exact position, rather
            // than silently moving the player to the cabin centre.
            let mut blocked = port_game(&def);
            blocked.walls.push(crate::engine::collision::WallAabb {
                min_x: expected.x - 0.2,
                max_x: expected.x + 0.2,
                min_z: expected.z - 0.2,
                max_z: expected.z + 0.2,
                min_y: 7.0,
                max_y: 9.0,
                step_up: 0.0,
            });
            blocked.collision_index =
                crate::engine::collision_index::CollisionIndex::build(&blocked.walls);
            let previous = blocked.player_position;
            assert!(blocked.arrive_elevator_from(&arrival).is_err());
            assert_eq!(blocked.player_position, previous);
            assert!((source.player_position.x - x).abs() < 0.0001);
        }
    }
    Ok(())
}

fn port_game(def: &LevelDef) -> Game {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, -1.0),
        0.0,
        CollisionWorld::from_level(def),
    );
    game.set_app_state(AppState::Playing);
    game
}

fn named_lift<'a>(game: &'a Game, id: &str) -> Result<&'a Elevator, String> {
    game.entities()
        .elevators()
        .iter()
        .find(|e| e.def.id == id)
        .ok_or_else(|| "Missing cabin".into())
}

// Door-only closure used to enter the generic trip path. These small real-game
// regressions guard against unintended local/foreign departures, obstruction
// bypass and opening a moving car. No renderer, assets or map bake is involved.
#[test]
fn separate_door_control_stays_put_and_preserves_obstruction_safety() -> Result<(), String> {
    for (id, x, destinations) in [("a", 0.0, 2), ("d", 16.0, 1)] {
        let mut game = port_game(&ports()?);
        game.set_benchmark_player_position(Vec3::new(x, EYE_HEIGHT, 1.2), 0.0, 0.0);
        let door = format!("@elevator:{id}:door");
        let cabin = named_lift(&game, id)?;
        let buttons = cabin.controls();
        let toggle = buttons
            .iter()
            .find(|b| b.id == door)
            .ok_or("Missing door button")?;
        let floors: Vec<_> = buttons
            .iter()
            .filter(|b| b.id.contains(":floor:"))
            .collect();
        assert_eq!(floors.len(), destinations);
        assert!(floors.iter().all(|b| b.anchor.y - toggle.anchor.y > 0.1));
        interact(&mut game, &door)?;
        settle(&mut game, 100);
        assert_eq!(named_lift(&game, id)?.phase, Phase::Open);
        interact(&mut game, &door)?;
        settle(&mut game, 200);
        assert_eq!(named_lift(&game, id)?.phase, Phase::Closed);
        assert_eq!(named_lift(&game, id)?.at, 0);
        assert_eq!(game.player_position, Vec3::new(x, EYE_HEIGHT, 1.2));
        assert!(game.entities_mut().take_elevator_departure().is_none());
        assert!(
            game.entities_mut().take_commands().is_empty(),
            "door operation cannot issue a level change"
        );
        assert!(
            game.entities_mut()
                .take_elevator_sounds()
                .iter()
                .all(|(_, cue)| *cue == "switch_click")
        );
        // A cabin control cannot be operated from the hall, even by ID.
        game.set_benchmark_player_position(Vec3::new(x, EYE_HEIGHT, -1.0), 0.0, 0.0);
        interact(&mut game, &door)?;
        assert_eq!(named_lift(&game, id)?.phase, Phase::Closed);
        game.set_benchmark_player_position(Vec3::new(x, EYE_HEIGHT, 1.2), 0.0, 0.0);
        interact(&mut game, &door)?;
        settle(&mut game, 100);
        interact(&mut game, &door)?;
        game.set_benchmark_player_position(Vec3::new(x, EYE_HEIGHT, 0.4), 0.0, 0.0);
        settle(&mut game, 100);
        assert_eq!(named_lift(&game, id)?.phase, Phase::Open);
        assert!(game.entities_mut().take_elevator_departure().is_none());
    }
    Ok(())
}

#[test]
fn door_control_cancels_unstarted_trips_but_never_opens_during_travel() -> Result<(), String> {
    for destination in [1_usize, 2] {
        let mut game = game()?;
        game.set_benchmark_player_position(Vec3::new(0.0, EYE_HEIGHT, 1.2), 0.0, 0.0);
        interact(&mut game, "@elevator:lift:door")?;
        settle(&mut game, 100);
        interact(&mut game, &format!("@elevator:lift:floor:{destination}"))?;
        settle(&mut game, 10);
        interact(&mut game, "@elevator:lift:door")?;
        settle(&mut game, 100);
        assert_eq!(lift(&game)?.phase, Phase::Open);
        assert_eq!(lift(&game)?.at, 0);
        assert!(game.entities_mut().take_elevator_departure().is_none());
        assert!(
            game.entities_mut().take_commands().is_empty(),
            "door operation cannot issue a level change"
        );
        interact(&mut game, &format!("@elevator:lift:floor:{destination}"))?;
        settle(&mut game, 85);
        let phase = lift(&game)?.phase;
        assert_eq!(
            phase,
            if destination == 1 {
                Phase::Moving
            } else {
                Phase::Loading
            }
        );
        interact(&mut game, "@elevator:lift:door")?;
        assert_eq!(lift(&game)?.phase, phase);
        assert_eq!(lift(&game)?.opening, 0.0);
    }
    Ok(())
}

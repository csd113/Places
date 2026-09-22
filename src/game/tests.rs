//! Unit tests for the game state, movement and collision.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used)]

use super::*;
use crate::test_support::assert_exact;

#[test]
fn test_pitch_movement_and_clamping() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        Vec::new(),
        WalkableFloor::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 10.0; // Large step to test pitch clamp
    let settings = Settings::default();

    let input_up = InputState::holding(&[Control::LookUp]);
    game.update_player_movement(&input_up, &settings);
    assert!((game.player_pitch - MAX_PITCH).abs() < 1e-4);

    let input_down = InputState::holding(&[Control::LookDown]);
    game.update_player_movement(&input_down, &settings);
    assert!((game.player_pitch - (-MAX_PITCH)).abs() < 1e-4);
}

#[test]
fn test_sim_delta_is_clamped() {
    assert_exact(clamp_sim_delta(0.016), 0.016);
    assert_exact(clamp_sim_delta(5.0), MAX_SIM_DELTA);
    assert_exact(clamp_sim_delta(-1.0), 0.0);
    assert_exact(clamp_sim_delta(f32::NAN), 0.0);
    assert_exact(clamp_sim_delta(f32::INFINITY), MAX_SIM_DELTA);
}

#[test]
fn test_escape_pause_toggle() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        Vec::new(),
        WalkableFloor::default(),
    );
    game.set_app_state(AppState::Playing);
    assert_eq!(game.app_state(), AppState::Playing);

    // Escape opens pause
    game.handle_escape();
    assert_eq!(game.app_state(), AppState::Paused);

    // Escape resumes playing
    game.handle_escape();
    assert_eq!(game.app_state(), AppState::Playing);
}

#[test]
fn test_paused_gameplay_does_not_move_or_turn() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        Vec::new(),
        WalkableFloor::default(),
    );
    game.set_app_state(AppState::Paused);
    game.delta_seconds = 1.0;
    let settings = Settings::default();

    let input = InputState::holding(&[Control::MoveForward, Control::LookLeft, Control::LookUp]);
    game.update_player_movement(&input, &settings);

    assert_eq!(game.player_position, Vec3::new(0.0, EYE_HEIGHT, 0.0));
    assert_exact(game.player_yaw, 0.0);
    assert_exact(game.player_pitch, 0.0);
}

/// One 16 m room with a shallow recess (a walkable step), a deep recess (a
/// cliff), and a two-step staircase, all on the +X side of the spawn.
fn step_rule_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "steps",
            "name": "Steps",
            "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 3.0, "z": 2.0, "width": 2.0, "depth": 4.0, "offset_y": -0.3 },
                { "x": 8.0, "z": 2.0, "width": 2.0, "depth": 4.0, "offset_y": -1.5 },
                { "x": 13.0, "z": 3.0, "width": 1.0, "depth": 2.0, "offset_y": 0.35 },
                { "x": 14.0, "z": 3.0, "width": 1.0, "depth": 2.0, "offset_y": 0.7 }
            ]
        }"#,
    )
    .expect("valid step json")
}

fn game_for(level: &LevelDef) -> Game {
    Game::new(
        spawn_position(level),
        level.spawn.yaw_degrees.to_radians(),
        level.collision_aabbs(),
        WalkableFloor::from_level(level),
    )
}

fn walk_forward(game: &mut Game, steps: usize) {
    let settings = Settings::default();
    let input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..steps {
        game.update_player_movement(&input, &settings);
    }
}

#[test]
fn test_spawn_position_resolves_the_local_floor() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "elevated_spawn",
            "name": "Elevated Spawn",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                      "height": 3.0, "floor_y": 2.0 },
            "floor_regions": [
                { "x": 3.0, "z": 3.0, "width": 4.0, "depth": 4.0, "offset_y": -0.5 }
            ]
        }"#,
    )
    .expect("elevated json");
    // The spawn sits over the recess, so the eye follows the recess floor.
    let spawn = spawn_position(&level);
    assert!(
        (spawn.y - (2.0 - 0.5 + EYE_HEIGHT)).abs() < 1e-4,
        "{spawn:?}"
    );
    let game = game_for(&level);
    assert!((game.player_floor_y - 1.5).abs() < 1e-4);
}

#[test]
fn test_controller_steps_down_into_a_shallow_recess_and_back_out() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    walk_forward(&mut game, 12);
    assert!(
        (game.player_floor_y - (-0.3)).abs() < 1e-4,
        "a walkable recess is stepped into: floor {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x > 3.0 && game.player_position.x < 6.0,
        "{:?}",
        game.player_position
    );

    // Turn around and walk back out; the step is climbed again.
    game.player_yaw = (-90.0f32).to_radians();
    walk_forward(&mut game, 20);
    assert!((game.player_floor_y - 0.0).abs() < 1e-4);
    assert!(game.player_position.x < 3.0);
}

#[test]
fn test_controller_climbs_a_staircase_of_floor_regions() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    // Skip over the recesses by spawning near the staircase.
    game.player_position = Vec3::new(11.5, EYE_HEIGHT, 4.0);
    game.player_floor_y = 0.0;
    walk_forward(&mut game, 25);
    assert!(
        (game.player_floor_y - 0.7).abs() < 1e-4,
        "two 0.35 m steps are climbable: floor {}",
        game.player_floor_y
    );
    assert!(game.player_position.x > 14.0);
}

#[test]
fn test_controller_refuses_a_drop_larger_than_a_step() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    // Walk from the room floor straight at the 1.5 m deep recess.
    game.player_position = Vec3::new(6.5, EYE_HEIGHT, 4.0);
    game.player_floor_y = 0.0;
    walk_forward(&mut game, 40);
    assert!(
        game.player_position.x < 8.0 + 1e-3,
        "the player stops at the cliff edge, not inside the pit: {}",
        game.player_position.x
    );
    assert!((game.player_floor_y - 0.0).abs() < 1e-4);
}

#[test]
fn test_controller_cannot_walk_off_the_last_floor_into_the_void() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "open_edge",
            "name": "Open Edge",
            "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 }
        }"#,
    )
    .expect("open edge json");
    let mut game = game_for(&level);
    walk_forward(&mut game, 60);
    assert!(
        game.player_position.x <= 10.0 + 1e-3,
        "walking out of the room is refused: {}",
        game.player_position.x
    );
}

#[test]
fn test_menu_state_transitions() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        Vec::new(),
        WalkableFloor::default(),
    );
    assert_eq!(game.app_state(), AppState::MainMenu);

    game.set_app_state(AppState::LevelSelect);
    assert_eq!(game.app_state(), AppState::LevelSelect);

    game.handle_escape();
    assert_eq!(game.app_state(), AppState::MainMenu);

    game.set_app_state(AppState::Settings);
    assert_eq!(game.app_state(), AppState::Settings);

    game.handle_escape();
    assert_eq!(game.app_state(), AppState::MainMenu);
}

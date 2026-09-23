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

/// The Home showcase's staircase and ramp are both walkable end to end, and the
/// platform's open edge refuses the drop.
#[test]
fn test_controller_climbs_the_home_staircase_and_the_ramp() {
    let content = std::fs::read_to_string("tests/fixtures/levels/home_showcase.json")
        .expect("the Home showcase fixture is present");
    let level = LevelDef::from_json(&content).expect("the Home showcase parses");

    // Up the staircase: five 0.15 m risers from the living-room floor.
    let mut game = game_for(&level);
    game.player_position = Vec3::new(1.4, EYE_HEIGHT, 3.2);
    game.player_floor_y = 0.0;
    game.player_yaw = 90.0_f32.to_radians();
    walk_forward(&mut game, 60);
    assert!(
        (game.player_floor_y - 0.75).abs() < 1e-4,
        "the flight ends on the platform: floor {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x > 4.5 && game.player_position.x < 5.4 + 1e-3,
        "the player crosses the platform but stops at its open edge: {}",
        game.player_position.x
    );

    // Up the ramp: the same 0.75 m, this time continuously.
    let mut game = game_for(&level);
    game.player_position = Vec3::new(4.5, EYE_HEIGHT, 0.5);
    game.player_floor_y = 0.0;
    game.player_yaw = 180.0_f32.to_radians();
    walk_forward(&mut game, 60);
    assert!(
        (game.player_floor_y - 0.75).abs() < 1e-4,
        "the ramp climbs to the platform: floor {}",
        game.player_floor_y
    );
    // Standing on the ramp part-way up answers a part-way height, and it is
    // monotone from the foot to the platform.
    let floor = WalkableFloor::from_level(&level);
    let mut previous = 0.0_f32;
    for z in 0..=32 {
        let at = 0.4 + f32::from(u16::try_from(z).unwrap_or(0)) * 0.05;
        let height = floor.height_at(4.5, at).unwrap_or(f32::NAN);
        assert!(
            height >= previous - 1e-4,
            "the ramp never dips: {height} at z {at}"
        );
        previous = height;
    }
    assert!((previous - 0.75).abs() < 1e-4);
}

/// The loader's maximum ramp slope is climbable at the slowest supported frame
/// rate and the fastest walk speed.
///
/// The step rule runs per sub-step, and a sub-step is at most half a player
/// radius (0.15 m), so a slope of 2.0 rises at most 0.3 m within a step. When
/// the rule was applied only to the frame's end point, 10 fps at 10 m/s moved
/// a whole metre and the 2 m rise was refused: the player stopped dead on a
/// ramp the loader had accepted.
#[test]
fn test_controller_climbs_a_maximum_slope_ramp_at_low_frame_rates() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "max_slope",
            "name": "Max Slope",
            "spawn": { "x": 0.5, "z": 0.5 },
            "room": { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 4.0 },
            "ramps": [
                { "x": 5.0, "z": 4.0, "width": 1.2, "depth": 2.0, "rise": 4.0 }
            ]
        }"#,
    )
    .expect("the maximum-slope level parses");
    // The ramp is exactly at the loader's limit: 4 m of rise over 2 m of run.
    let ramp = level.ramps.first().expect("one ramp");
    let slope = ramp.rise() / ramp.length();
    assert!((slope - crate::level::MAX_RAMP_SLOPE).abs() < 1e-5);
    let mut game = game_for(&level);
    game.player_position = Vec3::new(5.6, EYE_HEIGHT, 3.9);
    game.player_floor_y = 0.0;
    game.player_yaw = 180.0_f32.to_radians(); // south, up the ramp
    let settings = Settings {
        walk_speed: 10.0,
        ..Settings::default()
    };
    let input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..40 {
        game.update_player_movement(&input, &settings);
    }
    // The climb ends one sub-step short of the ramp's top edge, because the
    // sub-step that would leave the ramp meets a 4 m drop and is refused.
    assert!(
        game.player_floor_y > 3.5,
        "the player climbs the maximum slope at 10 fps: floor {} at {:?}",
        game.player_floor_y,
        game.player_position
    );
    assert!(game.player_position.z > 5.7, "{:?}", game.player_position);
}

/// An exact-limit riser (0.4 m) stays climbable at an elevated floor, where the
/// two floats that measure the step can differ by a few ulps.
#[test]
fn test_controller_climbs_an_exact_limit_riser_at_an_elevated_floor() {
    for floor_y in [0.3f32, 1.7, 10.3, 100.1] {
        let level = LevelDef::from_json(&format!(
            r#"{{
                "format_version": 1,
                "id": "limit_riser",
                "name": "Limit Riser",
                "spawn": {{ "x": 0.5, "z": 0.5 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                           "height": 4.0, "floor_y": {floor_y} }},
                "stairs": [
                    {{ "x": 4.0, "z": 2.0, "width": 1.0, "depth": 3.0,
                       "rise": 1.2, "steps": 3 }}
                ]
            }}"#
        ))
        .expect("the limit-riser level parses");
        let riser = level.stairs.first().expect("one staircase").riser_height();
        assert!((riser - PLAYER_STEP_HEIGHT).abs() < 1e-6, "{riser}");
        let mut game = game_for(&level);
        game.player_position = Vec3::new(4.5, EYE_HEIGHT, 1.9);
        game.player_floor_y = floor_y;
        game.player_yaw = 180.0_f32.to_radians(); // south, up the flight
        walk_forward(&mut game, 30);
        assert!(
            (game.player_floor_y - (floor_y + 1.2)).abs() < 1e-3,
            "at floor {floor_y} the flight tops out at {}",
            game.player_floor_y
        );
    }
}

/// Walking down a maximum-slope ramp never stalls or bounces.
#[test]
fn test_controller_descends_a_maximum_slope_ramp_without_stalling() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "max_slope_down",
            "name": "Max Slope Down",
            "spawn": { "x": 0.5, "z": 0.5 },
            "room": { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 6.0 },
            "floor_regions": [
                { "x": 5.0, "z": 6.0, "width": 1.2, "depth": 2.0, "offset_y": 4.0 }
            ],
            "ramps": [
                { "x": 5.0, "z": 4.0, "width": 1.2, "depth": 2.0, "rise": 4.0 }
            ]
        }"#,
    )
    .expect("the descending level parses");
    let mut game = game_for(&level);
    // Stand on the platform the ramp climbs to, facing the descent.
    game.player_position = Vec3::new(6.0, EYE_HEIGHT + 4.0, 6.4);
    game.player_floor_y = 4.0;
    game.player_yaw = 0.0; // north, down the ramp
    let settings = Settings {
        walk_speed: 10.0,
        ..Settings::default()
    };
    let input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    // A frame at 10 m/s can cover several treads' worth of height, so the
    // per-frame check is monotonicity, not the per-sub-step step size: the
    // player must never bounce back up the ramp.
    let mut previous = game.player_floor_y;
    for _ in 0..30 {
        game.update_player_movement(&input, &settings);
        assert!(
            game.player_floor_y <= previous + 1e-3,
            "the descent never climbs: {} then {}",
            previous,
            game.player_floor_y
        );
        previous = game.player_floor_y;
    }
    assert!(
        game.player_floor_y < 0.5,
        "the player descends the ramp: floor {} at {:?}",
        game.player_floor_y,
        game.player_position
    );
}

/// A full traversal of the Home showcase's split level: up the ramp, across the
/// platform, down the staircase and back onto the living-room floor. The eye
/// height never oscillates and the route never snags on the ramp sides, the
/// platform rim, the columns or the guardrail.
#[test]
fn test_controller_traverses_the_home_split_level_both_ways() {
    let content = std::fs::read_to_string("tests/fixtures/levels/home_showcase.json")
        .expect("the Home showcase fixture is present");
    let level = LevelDef::from_json(&content).expect("the Home showcase parses");
    let mut game = game_for(&level);
    // Up the ramp from the living-room floor.
    game.player_position = Vec3::new(4.5, EYE_HEIGHT, 0.25);
    game.player_floor_y = 0.0;
    game.player_yaw = 180.0_f32.to_radians();
    walk_forward(&mut game, 30);
    assert!(
        (game.player_floor_y - 0.75).abs() < 1e-3,
        "the ramp tops out on the platform: {}",
        game.player_floor_y
    );
    // The ramp phase carried the player to the platform's south wall; walk
    // back north onto the staircase's lane, then turn west and take it down.
    game.player_yaw = 0.0;
    walk_forward(&mut game, 4);
    game.player_yaw = (-90.0f32).to_radians();
    let mut previous = game.player_floor_y;
    let settings = Settings::default();
    let input = InputState::holding(&[Control::MoveForward]);
    for _ in 0..40 {
        game.update_player_movement(&input, &settings);
        assert!(
            game.player_floor_y <= previous + 1e-3,
            "descending the stairs never climbs: {} then {}",
            previous,
            game.player_floor_y
        );
        previous = game.player_floor_y;
    }
    assert!(
        game.player_floor_y < 1e-3,
        "the staircase returns to the living-room floor: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x < 3.0,
        "the route crosses the platform to the west: {:?}",
        game.player_position
    );
}

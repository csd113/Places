//! Unit tests for the game state, movement and collision.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used)]

use std::fmt::Write as _;

use super::*;
use crate::test_support::assert_exact;

#[test]
fn test_pitch_movement_and_clamping() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 10.0; // Large step to test pitch clamp
    let settings = Settings::default();

    let mut input_up = InputState::holding(&[Control::LookUp]);
    game.update_player_movement(&mut input_up, &settings);
    assert!((game.player_pitch - MAX_PITCH).abs() < 1e-4);

    let mut input_down = InputState::holding(&[Control::LookDown]);
    game.update_player_movement(&mut input_down, &settings);
    assert!((game.player_pitch - (-MAX_PITCH)).abs() < 1e-4);
}

/// `invert_look` flips only the vertical look direction; the horizontal turn
/// and the movement keys are untouched.
#[test]
fn test_invert_look_flips_vertical_look_only() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0;
    let settings = Settings {
        invert_look: true,
        ..Settings::default()
    };

    game.update_player_movement(&mut InputState::holding(&[Control::LookUp]), &settings);
    assert!(
        game.player_pitch < 0.0,
        "inverted look up must pitch down: {}",
        game.player_pitch
    );
    game.player_pitch = 0.0;
    game.update_player_movement(&mut InputState::holding(&[Control::LookDown]), &settings);
    assert!(game.player_pitch > 0.0, "inverted look down must pitch up");

    // Horizontal look is not inverted by the vertical preference.
    game.player_yaw = 0.0;
    game.update_player_movement(&mut InputState::holding(&[Control::LookRight]), &settings);
    assert!(game.player_yaw > 0.0, "yaw must keep its normal direction");

    // And the default stays the historical non-inverted behaviour.
    let upright = Settings::default();
    game.player_pitch = 0.0;
    game.update_player_movement(&mut InputState::holding(&[Control::LookUp]), &upright);
    assert!(game.player_pitch > 0.0);
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
        CollisionWorld::default(),
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
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Paused);
    game.delta_seconds = 1.0;
    let settings = Settings::default();

    let mut input =
        InputState::holding(&[Control::MoveForward, Control::LookLeft, Control::LookUp]);
    game.update_player_movement(&mut input, &settings);

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
        CollisionWorld::from_level(level),
    )
}

fn walk_forward(game: &mut Game, steps: usize) {
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..steps {
        game.update_player_movement(&mut input, &settings);
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
    // The two 0.35 m steps are climbable. Walking on past the 0.7 m platform
    // walks off its far edge, so the run records the highest floor reached
    // before the fall instead of assuming the player stops at the edge.
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let mut highest = game.player_floor_y;
    for _ in 0..25 {
        game.update_player_movement(&mut input, &settings);
        highest = highest.max(game.player_floor_y);
    }
    assert!(
        (highest - 0.7).abs() < 1e-4,
        "two 0.35 m steps are climbable: highest floor {highest}"
    );
    assert!(
        game.player_position.x > 14.0,
        "the steps are crossed: {:?}",
        game.player_position
    );
    // The far edge is a real drop: the player fell back to the room floor.
    assert!(
        game.player_floor_y.abs() < 1e-4,
        "fall: {}",
        game.player_floor_y
    );
}

#[test]
fn test_controller_walks_off_a_deep_edge_and_falls() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    // Walk from the room floor straight at the 1.5 m deep recess.
    game.player_position = Vec3::new(6.5, EYE_HEIGHT, 4.0);
    game.player_floor_y = 0.0;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let mut airborne = false;
    for _ in 0..40 {
        game.update_player_movement(&mut input, &settings);
        if !game.grounded {
            airborne = true;
        }
    }
    assert!(airborne, "walking off the cliff loses support and falls");
    assert!(
        game.player_position.x > 8.5,
        "the player crossed the edge instead of stopping at it: {:?}",
        game.player_position
    );
    assert!(
        (game.player_floor_y - (-1.5)).abs() < 1e-4,
        "the fall lands on the recess floor: {}",
        game.player_floor_y
    );
    assert!(game.grounded, "the fall lands");

    // The 1.5 m wall is taller than a step: walking back out is refused, and
    // the floor never rises toward the lip.
    game.player_yaw = (-90.0f32).to_radians();
    for _ in 0..40 {
        game.update_player_movement(&mut input, &settings);
    }
    assert!(
        game.player_floor_y < -1.0,
        "the cliff cannot be climbed back out: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x < 10.0,
        "the player stays inside the pit: {:?}",
        game.player_position
    );
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
        CollisionWorld::default(),
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
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    // The ramp is exactly at the loader's limit: 4 m of rise over 2 m of run.
    let ramp = level.ramps.first().expect("one ramp");
    let slope = ramp.rise() / ramp.length();
    assert!((slope - crate::level::MAX_RAMP_SLOPE).abs() < 1e-5);
    let mut highest = game.player_floor_y;
    for _ in 0..40 {
        game.update_player_movement(&mut input, &settings);
        highest = highest.max(game.player_floor_y);
    }
    // The climb reaches the ramp's top, and walking on past it is a real drop
    // onto the room floor: the player fell instead of stalling one sub-step
    // short of the edge.
    assert!(
        highest > 3.5,
        "the player climbs the maximum slope at 10 fps: highest floor {highest}"
    );
    assert!(
        game.player_floor_y.abs() < 1e-3 && game.grounded,
        "past the top the player falls to the room floor: {} grounded {}",
        game.player_floor_y,
        game.grounded
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
        // Ten 0.1 s frames at 3 m/s cover the 3 m tread run and stop on the
        // top tread; an eleventh would walk off the flight's far edge, which is
        // now a real drop.
        walk_forward(&mut game, 10);
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
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    // A frame at 10 m/s can cover several treads' worth of height, so the
    // per-frame check is monotonicity, not the per-sub-step step size: the
    // player must never bounce back up the ramp.
    let mut previous = game.player_floor_y;
    for _ in 0..30 {
        game.update_player_movement(&mut input, &settings);
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
    let mut input = InputState::holding(&[Control::MoveForward]);
    for _ in 0..40 {
        game.update_player_movement(&mut input, &settings);
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

/// Diagnostic: walk the demo's real staircase up and back down, one fixed
/// 60 Hz frame at a time, and write the eye Y, the controller's floor Y and
/// the rendered tread Y per frame.
///
/// Ignored by default because it only exists to produce evidence; run it with
/// the output path in the environment:
///
/// ```text
/// PLACES_STAIRS_TRACE=/abs/path/walk.csv \
///     cargo test --bin places capture_stairs_walk_trace -- --ignored
/// ```
///
/// The CSV columns are `phase,frame,x,z,eye_y,floor_y,render_y,step_dy`, and a
/// `<path>.summary.txt` beside it reports the largest per-frame eye step of
/// each phase against the authored riser.
#[test]
#[ignore = "developer diagnostic; writes a CSV when PLACES_STAIRS_TRACE is set"]
fn capture_stairs_walk_trace() {
    let Ok(path) = std::env::var("PLACES_STAIRS_TRACE") else {
        return;
    };
    let content = std::fs::read_to_string("assets/levels/places_demo.json")
        .expect("the Places demo level is present");
    let level = LevelDef::from_json(&content).expect("the Places demo parses");
    let stair = level.stairs.first().expect("the demo has one staircase");

    let mut game = game_for(&level);
    // The hall floor west of the flight, facing east up the stairs, on the
    // lane between the two handrails (the stair spans z 13.0..14.2).
    game.player_position = Vec3::new(54.0, -0.9 + EYE_HEIGHT, 13.6);
    game.player_floor_y = -0.9;
    game.player_yaw = 90.0_f32.to_radians();
    game.set_app_state(AppState::Playing);
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.sim_delta_seconds = 1.0 / 60.0;

    let mut csv = String::from("phase,frame,x,z,eye_y,floor_y,render_y,step_dy\n");
    let mut steps: Vec<(u8, u32, f32)> = Vec::new();
    let mut previous_eye = game.player_position.y;
    for phase in 0..2 {
        if phase == 1 {
            // Turn around on the upper floor and walk back west.
            game.player_yaw = (-90.0_f32).to_radians();
        }
        let frames = if phase == 0 { 110 } else { 130 };
        for frame in 0..frames {
            game.update_player_movement(&mut input, &settings);
            let x = game.player_position.x;
            let z = game.player_position.z;
            let eye = game.player_position.y;
            let floor = game.player_floor_y;
            let render = game.floor.height_at(x, z).unwrap_or(f32::NAN);
            let dy = eye - previous_eye;
            previous_eye = eye;
            writeln!(
                csv,
                "{phase},{frame},{x:.5},{z:.5},{eye:.5},{floor:.5},{render:.5},{dy:.6}"
            )
            .expect("the trace builds in memory");
            steps.push((phase, frame, dy));
        }
    }

    let riser = stair.riser_height();
    let tread = stair.tread_depth();
    let mut summary = String::new();
    for phase in 0..2_u8 {
        let mut max_dy = 0.0_f32;
        let mut near = Vec::new();
        for &(at_phase, frame, dy) in &steps {
            if at_phase != phase {
                continue;
            }
            let magnitude = dy.abs();
            if magnitude > max_dy {
                max_dy = magnitude;
            }
            if magnitude > riser * 0.9 {
                near.push(frame);
            }
        }
        let label = if phase == 0 { "up" } else { "down" };
        writeln!(
            summary,
            "{label}: riser={riser:.4} tread={tread:.4} max|step_dy|={max_dy:.5} \
             frames_over_0.9_riser={near:?}"
        )
        .expect("the summary builds in memory");
    }
    std::fs::write(&path, &csv).expect("the trace CSV is writable");
    std::fs::write(format!("{path}.summary.txt"), &summary).expect("the summary is writable");
}

/// A ten-step flight -- 0.2 m risers on 0.3 m treads -- whose top tread lands
/// flush on a raised platform.
fn smooth_stairs_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "smooth_stairs",
            "name": "Smooth Stairs",
            "spawn": { "x": 2.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 8.0, "height": 4.0 },
            "stairs": [
                { "x": 8.0, "z": 3.0, "width": 3.0, "depth": 2.0,
                  "rise": 2.0, "steps": 10 }
            ],
            "floor_regions": [
                { "x": 11.0, "z": 3.0, "width": 4.0, "depth": 2.0, "offset_y": 2.0 }
            ]
        }"#,
    )
    .expect("the smooth-stair level parses")
}

/// A game on the smooth-stairs fixture at `(x, z)` standing on `floor_y`,
/// facing `yaw_degrees`, stepping a fixed 60 Hz.
fn smooth_stairs_game(level: &LevelDef, x: f32, z: f32, floor_y: f32, yaw_degrees: f32) -> Game {
    let mut game = game_for(level);
    game.player_position = Vec3::new(x, floor_y + EYE_HEIGHT, z);
    game.player_floor_y = floor_y;
    game.player_yaw = yaw_degrees.to_radians();
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    game
}

/// Walking up a staircase is continuous: every frame inside the flight moves
/// the eye by at most the pitch line's own rise, not a whole riser per tread
/// boundary, and the walking surface stays between the tread underfoot and the
/// tread ahead. The one discrete step is the flight's real first riser.
#[test]
fn test_controller_walks_a_staircase_smoothly_up() {
    let level = smooth_stairs_level();
    let stair = level.stairs.first().expect("one staircase");
    let riser = stair.riser_height();
    let (foot_x, _x1, _z0, _z1) = stair.bounds();
    let top_x = foot_x + stair.length();
    assert!(
        (riser - 0.2).abs() < 1e-6,
        "the fixture is 10 by 0.2: {riser}"
    );
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    let frame_metres = settings.walk_speed / 60.0;
    // The pitch line rises one riser per tread, so a frame that moves
    // `frame_metres` along the flight changes the eye by this much.
    let pitch_rise_per_frame = riser / stair.tread_depth() * frame_metres;

    let mut game = smooth_stairs_game(&level, foot_x - 1.0, 4.0, 0.0, 90.0);
    let mut previous_x = game.player_position.x;
    let mut previous_eye = game.player_position.y;
    let mut foot_steps = 0_u32;
    let mut on_stair = Vec::new();
    for _ in 0..140 {
        game.update_player_movement(&mut input, &settings);
        let x = game.player_position.x;
        let floor = game.player_floor_y;
        let eye = game.player_position.y;
        assert_exact(eye, floor + EYE_HEIGHT);
        let dy = eye - previous_eye;
        if previous_x > foot_x + 0.1 && x < top_x - 1e-3 {
            assert!(dy >= -1e-4, "walking up never dips: {dy} at x {x}");
            assert!(
                dy <= pitch_rise_per_frame + 1e-4,
                "the tread boundary at x {x} is smoothed: dy {dy}"
            );
        }
        if previous_x < foot_x && x < foot_x {
            assert_exact(dy, 0.0);
        }
        if previous_x > top_x && x > top_x {
            assert_exact(dy, 0.0);
        }
        if dy > riser * 0.9 {
            foot_steps = foot_steps.saturating_add(1);
            assert!(
                dy <= riser + pitch_rise_per_frame + 1e-4,
                "the foot's riser is one step, no more: {dy}"
            );
        }
        assert!(dy <= PLAYER_STEP_HEIGHT + STEP_EPS, "step rule kept: {dy}");
        if x >= foot_x && x <= top_x {
            let stepped = game.floor.height_at(x, game.player_position.z);
            on_stair.push((x, floor, stepped.expect("inside the flight")));
        }
        previous_x = x;
        previous_eye = eye;
    }
    assert_eq!(foot_steps, 1, "only the foot's real riser is discrete");
    assert!(
        (game.player_floor_y - 2.0).abs() < 1e-4,
        "the flight tops out on the platform: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x > top_x + 1.0,
        "the walk crosses the landing: {:?}",
        game.player_position
    );
    assert!(!on_stair.is_empty());
    // The walking surface is always between the tread underfoot and the tread
    // ahead: never inside a step, never floating above the next one.
    for (x, floor, stepped) in on_stair {
        let next = (stepped + riser).min(stair.rise());
        assert!(
            floor >= stepped - 1e-4,
            "at x {x} the feet are below the tread: {floor} vs {stepped}"
        );
        assert!(
            floor <= next + 1e-4,
            "at x {x} the feet are above the next tread: {floor} vs {next}"
        );
    }
}

/// The same flight walked down: continuous again, never a bounce back up, and
/// the single foot riser is the only discrete drop.
#[test]
fn test_controller_walks_a_staircase_smoothly_down() {
    let level = smooth_stairs_level();
    let stair = level.stairs.first().expect("one staircase");
    let riser = stair.riser_height();
    let (foot_x, _x1, _z0, _z1) = stair.bounds();
    let top_x = foot_x + stair.length();
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    let pitch_drop_per_frame = riser / stair.tread_depth() * settings.walk_speed / 60.0;

    let mut game = smooth_stairs_game(&level, top_x + 1.0, 4.0, 2.0, -90.0);
    let mut previous_x = game.player_position.x;
    let mut previous_eye = game.player_position.y;
    let mut foot_steps = 0_u32;
    for _ in 0..150 {
        game.update_player_movement(&mut input, &settings);
        let x = game.player_position.x;
        let eye = game.player_position.y;
        assert_exact(eye, game.player_floor_y + EYE_HEIGHT);
        let dy = eye - previous_eye;
        assert!(dy <= 1e-4, "walking down never climbs: {dy} at x {x}");
        if previous_x < top_x - 1e-3 && x > foot_x + 0.1 {
            assert!(
                dy >= -pitch_drop_per_frame - 1e-4,
                "the tread boundary at x {x} is smoothed: dy {dy}"
            );
        }
        if dy < -riser * 0.9 {
            foot_steps = foot_steps.saturating_add(1);
            assert!(
                dy >= -riser - pitch_drop_per_frame - 1e-4,
                "the foot's riser is one step, no more: {dy}"
            );
        }
        assert!(dy >= -PLAYER_STEP_HEIGHT - STEP_EPS, "step rule kept: {dy}");
        previous_x = x;
        previous_eye = eye;
    }
    assert_eq!(foot_steps, 1, "only the foot's real riser is discrete");
    assert!(
        game.player_floor_y.abs() < 1e-4,
        "the descent returns to the ground floor: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x < foot_x,
        "the walk returns past the foot: {:?}",
        game.player_position
    );
}

/// The controller's walking surface never leaves the tread it is on: at every
/// sample of every flight orientation it sits at or above the rendered tread
/// underfoot and at or below the next tread's top, and it is monotone from the
/// foot to the top. That sandwich is what makes a continuous controller height
/// physically meaningful: the feet are never inside a step and never floating
/// over the one ahead.
#[test]
fn test_walk_surface_stays_between_the_treads_it_connects() {
    let cases: [(f32, f32, f32, f32, f32, f32); 4] = [
        // x, z, width, depth, offset_y, rise
        (1.0, 1.0, 3.0, 1.5, 0.0, 2.0),   // +X run
        (1.0, 1.0, 1.5, 3.0, 0.5, 1.5),   // +Z run on a raised floor
        (5.0, 1.0, 3.0, 1.5, -0.25, 0.9), // +X run, recessed foot
        (2.0, 2.0, 6.0, 2.0, 0.25, 2.4),  // long +X flight
    ];
    for (x, z, width, depth, offset_y, rise) in cases {
        let level = LevelDef::from_json(&format!(
            r#"{{
                "format_version": 1,
                "id": "stair_sandwich",
                "name": "Stair Sandwich",
                "spawn": {{ "x": 0.5, "z": 0.5 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 6.0 }},
                "stairs": [
                    {{ "x": {x}, "z": {z}, "width": {width}, "depth": {depth},
                       "offset_y": {offset_y}, "rise": {rise}, "steps": 6 }}
                ]
            }}"#
        ))
        .expect("the sandwich level parses");
        let floor = WalkableFloor::from_level(&level);
        let stair = level.stairs.first().expect("one staircase");
        let (x0, x1, z0, z1) = stair.bounds();
        let riser = stair.riser_height();
        let top = stair.top_offset();
        let mut previous = f32::NEG_INFINITY;
        for sample in 0..=32_u16 {
            let fraction = f32::from(sample) / 32.0;
            let (px, pz) = if stair.axis() == crate::level::WallAxis::X {
                ((x1 - x0).mul_add(fraction, x0), f32::midpoint(z0, z1))
            } else {
                (f32::midpoint(x0, x1), (z1 - z0).mul_add(fraction, z0))
            };
            let stepped = floor.height_at(px, pz).expect("inside the room");
            let walk = floor.walk_height_at(px, pz).expect("inside the room");
            let next = (stepped + riser).min(top);
            assert!(
                walk >= stepped - 1e-4,
                "at ({px}, {pz}) the walk sits below the tread: {walk} vs {stepped}"
            );
            assert!(
                walk <= next + 1e-4,
                "at ({px}, {pz}) the walk rises above the next tread: {walk} vs {next}"
            );
            assert!(
                walk >= previous - 1e-4,
                "at ({px}, {pz}) the walk surface dips: {walk} after {previous}"
            );
            previous = walk;
        }
        assert!(
            (previous - top).abs() < 1e-4,
            "the flight ends on its top tread: {previous} vs {top}"
        );
    }
}

/// A platform taller than `PLAYER_STEP_HEIGHT` and an authored wall are still
/// barriers after the staircase became continuous: the step rule and the wall
/// boxes keep their limits.
#[test]
fn test_controller_cannot_climb_a_tall_step_or_a_wall() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "no_climb",
            "name": "No Climb",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 6.0, "z": 0.0, "width": 2.0, "depth": 8.0, "offset_y": 0.5 }
            ],
            "walls": [
                { "x": 12.0, "z": 0.0, "width": 0.3, "depth": 8.0 }
            ]
        }"#,
    )
    .expect("the no-climb level parses");

    // The 0.5 m platform is half a metre tall: its rim stops the walk and the
    // floor never rises toward its top.
    let mut game = game_for(&level);
    game.player_position = Vec3::new(4.0, EYE_HEIGHT, 4.0);
    game.player_floor_y = 0.0;
    game.player_yaw = 90.0_f32.to_radians();
    walk_forward(&mut game, 60);
    assert!(
        game.player_floor_y.abs() < 1e-4,
        "a 0.5 m step is not climbed: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x <= 6.0 - PLAYER_RADIUS + 1e-3,
        "the player stops one radius short of the platform: {:?}",
        game.player_position
    );

    // An authored wall blocks exactly as before.
    let mut game = game_for(&level);
    game.player_position = Vec3::new(10.0, EYE_HEIGHT, 4.0);
    game.player_floor_y = 0.0;
    game.player_yaw = 90.0_f32.to_radians();
    walk_forward(&mut game, 60);
    assert!(
        game.player_floor_y.abs() < 1e-4,
        "a wall is not climbed: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x <= 12.0 - PLAYER_RADIUS + 1e-3,
        "the player stops at the wall face: {:?}",
        game.player_position
    );
}

// ---------------------------------------------------------------------------
// Gravity, jumping and swimming
// ---------------------------------------------------------------------------

/// A fresh game on `level` at a known floor, walking a fixed frame delta.
fn play_at(game: &mut Game, x: f32, floor_y: f32, z: f32, yaw_degrees: f32, delta: f32) {
    game.set_app_state(AppState::Playing);
    game.player_position = Vec3::new(x, floor_y + EYE_HEIGHT, z);
    game.player_floor_y = floor_y;
    game.grounded = true;
    game.vertical_velocity = 0.0;
    game.player_yaw = yaw_degrees.to_radians();
    game.sim_delta_seconds = delta;
}

/// A 16x8 room with a deep pool (x 4..10, floor -3.0, water surface -0.5) and
/// a wading step beside it (x 10..12, floor -0.85, the same surface).
fn pool_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "pool",
            "name": "Pool",
            "spawn": { "x": 1.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 6.0 },
            "floor_regions": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0, "offset_y": -3.0 },
                { "x": 10.0, "z": 1.0, "width": 2.0, "depth": 6.0, "offset_y": -0.85 }
            ],
            "water": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0,
                  "surface_y": -0.5, "bottom_y": -3.0 },
                { "x": 10.0, "z": 1.0, "width": 2.0, "depth": 6.0,
                  "surface_y": -0.5, "bottom_y": -0.85 }
            ]
        }"#,
    )
    .expect("the pool level parses")
}

/// The owned ceiling model follows the rendered room profile, including a
/// gable's ridge, and answers `None` outside every room.
#[test]
fn walkable_ceiling_follows_the_room_profile() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 2.0, "z": 2.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 1.0 } }
        }"#,
    )
    .expect("the gable room parses");
    let ceiling = WalkableCeiling::from_level(&level);
    assert_exact(ceiling.ceiling_y_at(4.0, 0.0).expect("inside"), 3.0);
    assert_exact(ceiling.ceiling_y_at(4.0, 8.0).expect("inside"), 3.0);
    assert_exact(ceiling.ceiling_y_at(4.0, 4.0).expect("inside"), 4.0);
    assert_exact(ceiling.ceiling_y_at(4.0, 2.0).expect("inside"), 3.5);
    assert!(
        ceiling.ceiling_y_at(-5.0, 4.0).is_none(),
        "outside every room"
    );
}

#[test]
fn grounded_player_never_falls_through_the_floor() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut input = InputState::default();
    for _ in 0..600 {
        game.update_player_movement(&mut input, &settings);
        assert!(game.grounded, "the player stays on the floor");
        assert_exact(game.player_position.y, game.player_floor_y + EYE_HEIGHT);
        assert!(game.player_floor_y.is_finite());
        assert_exact(game.vertical_velocity, 0.0);
    }
}

/// A legacy spawn outside every room stands on the historical floor at the
/// spawn's own height: it never falls into the void, and a jump from it lands
/// back on the same line.
#[test]
fn off_room_spawn_stands_on_the_historical_floor() {
    let mut game = Game::new(
        Vec3::new(2.0, EYE_HEIGHT, 3.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    let settings = Settings::default();
    let mut input = InputState::default();
    for _ in 0..600 {
        game.update_player_movement(&mut input, &settings);
        assert_exact(game.player_position.y, EYE_HEIGHT);
    }
    assert!(game.grounded);

    let mut jump = InputState::holding(&[Control::Jump]);
    let mut highest = game.player_position.y;
    for _ in 0..120 {
        game.update_player_movement(&mut jump, &settings);
        highest = highest.max(game.player_position.y);
    }
    assert!(
        highest > EYE_HEIGHT + 0.5,
        "the jump works off-room too: {highest}"
    );
    assert!(game.grounded, "and it lands back on the historical floor");
    assert_exact(game.player_position.y, EYE_HEIGHT);
}

#[test]
fn jump_starts_only_while_grounded_and_never_double_jumps() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();

    // A fresh press while grounded launches.
    let mut held = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut held, &settings);
    assert!(!game.grounded);
    assert!(game.vertical_velocity > 0.0);
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Airborne);

    // Holding the key never re-launches: the vertical speed only decreases.
    let mut previous = game.vertical_velocity;
    for _ in 0..30 {
        game.update_player_movement(&mut held, &settings);
        assert!(
            game.vertical_velocity <= previous + 1e-6,
            "a held jump re-launched: {previous} then {}",
            game.vertical_velocity
        );
        previous = game.vertical_velocity;
    }

    // The jump lands, and the still-held key does not bounce.
    let mut landed = false;
    for _ in 0..300 {
        game.update_player_movement(&mut held, &settings);
        if game.grounded {
            landed = true;
            break;
        }
    }
    assert!(landed, "the jump lands");
    let floor_after_landing = game.player_floor_y;
    game.update_player_movement(&mut held, &settings);
    assert!(game.grounded, "a held jump does not bounce on landing");
    assert_exact(game.player_position.y, floor_after_landing + EYE_HEIGHT);

    // Releasing and pressing again mid-air is rejected.
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut held, &settings); // launch
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings); // release
    assert!(!game.grounded);
    game.vertical_velocity = -1.0; // descending mid-air
    let mut pressed = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut pressed, &settings);
    assert!(
        game.vertical_velocity < 0.0,
        "a mid-air jump must be rejected: {}",
        game.vertical_velocity
    );
}

/// The apex one jump reaches at a fixed frame delta, in metres above the
/// take-off floor.
fn jump_apex_at(delta: f32) -> f32 {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, delta);
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::Jump]);
    let start = game.player_floor_y;
    let mut apex = 0.0_f32;
    let mut left_the_floor = false;
    for _ in 0..600 {
        game.update_player_movement(&mut input, &settings);
        apex = apex.max(game.player_position.y - EYE_HEIGHT - start);
        if game.grounded {
            if left_the_floor {
                break;
            }
        } else {
            left_the_floor = true;
        }
    }
    assert!(left_the_floor, "the jump leaves the floor");
    assert!(game.grounded, "the jump lands");
    apex
}

/// The jump apex is the desk height at every frame rate: vertical motion runs
/// in a fixed substep, so 30, 60 and 144 fps sample the same parabola.
#[test]
fn jump_apex_is_frame_rate_independent() {
    assert_exact((2.0 * GRAVITY * JUMP_APEX_M).sqrt(), JUMP_VELOCITY);
    // The integration step is the documented fixed step.
    assert_exact(VERTICAL_SUBSTEP, 1.0 / 120.0);
    let apexes = [
        jump_apex_at(1.0 / 30.0),
        jump_apex_at(1.0 / 60.0),
        jump_apex_at(1.0 / 144.0),
    ];
    for apex in apexes {
        assert!(
            (JUMP_APEX_M - 0.01..=JUMP_APEX_M + 0.01).contains(&apex),
            "the apex clears the {OFFICE_DESK_TOP_M} m desk by its margin: {apex}"
        );
    }
    let low = apexes.iter().copied().fold(f32::INFINITY, f32::min);
    let high = apexes.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        high - low <= 0.01,
        "the apex must not depend on the frame rate: {apexes:?}"
    );
}

#[test]
fn ceiling_bump_clamps_the_head_and_zeroes_upward_velocity() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "low_room",
            "name": "Low Room",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 2.2 }
        }"#,
    )
    .expect("the low room parses");
    let mut game = game_for(&level);
    play_at(&mut game, 4.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::Jump]);
    let max_eye = 2.2 - (PLAYER_HEIGHT - EYE_HEIGHT);
    let mut highest = game.player_position.y;
    let mut bumped = false;
    for _ in 0..180 {
        game.update_player_movement(&mut input, &settings);
        highest = highest.max(game.player_position.y);
        assert!(
            game.player_position.y <= max_eye + 1e-4,
            "the head stays under the ceiling: {}",
            game.player_position.y
        );
        if game.player_position.y >= max_eye - 1e-4 {
            bumped = true;
            assert_exact(game.vertical_velocity, 0.0);
        }
    }
    assert!(bumped, "the jump reaches the ceiling: {highest}");
    assert!(
        (highest - max_eye).abs() <= CONTACT_EPS + 1e-4,
        "the head is clamped to the ceiling: {highest} vs {max_eye}"
    );
    // With the upward velocity consumed by the bump, the player falls back and
    // lands instead of hovering pinned to the ceiling.
    assert!(game.grounded, "the bumped jump falls back down");
    assert_exact(game.player_position.y, game.player_floor_y + EYE_HEIGHT);
}

/// Falling through the surface of the deep pool switches the player to
/// swimming, and the body never sinks through the pool floor.
#[test]
fn falling_into_deep_water_switches_to_swimming() {
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 7.0, -3.0, 4.0, 0.0, 1.0 / 60.0);
    game.player_position.y = 1.0;
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::default();
    let mut switched = false;
    for _ in 0..300 {
        game.update_player_movement(&mut input, &settings);
        if game.is_swimming() && !switched {
            switched = true;
            assert!(!game.grounded, "the water clears the grounded flag");
        }
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-4,
            "the swimmer never sinks through the pool floor: {}",
            game.player_position.y
        );
    }
    assert!(switched, "falling into the pool starts swimming");
    assert!(game.is_underwater(), "the sunken swimmer is underwater");
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Swimming);
}

/// Holding Jump in deep water rises to the float line and stabilizes there:
/// the eye stays within the margin plus the deterministic bob and the vertical
/// velocity is spent at the line.
#[test]
fn holding_jump_rises_to_the_float_line_and_stabilizes() {
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 7.0, -3.0, 4.0, 0.0, 1.0 / 60.0);
    let surface = -0.5_f32;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::Jump]);
    let mut highest = game.player_position.y;
    let mut underwater_frames = 0_u32;
    for _ in 0..600 {
        game.update_player_movement(&mut input, &settings);
        assert!(game.is_swimming(), "still in the pool");
        let eye = game.player_position.y;
        highest = highest.max(eye);
        assert!(
            eye <= surface + FLOAT_EYE_MARGIN + SWIM_BOB_AMPLITUDE + 1e-4,
            "the float line holds: {eye}"
        );
        if game.is_underwater() {
            underwater_frames = underwater_frames.saturating_add(1);
        }
    }
    assert!(underwater_frames > 0, "the swimmer rises through the water");
    assert!(
        highest >= surface + FLOAT_EYE_MARGIN - SWIM_BOB_AMPLITUDE - 1e-4,
        "the swimmer reaches the line: {highest}"
    );
    assert!(
        game.player_position.y >= surface + FLOAT_EYE_MARGIN - SWIM_BOB_AMPLITUDE - 1e-4,
        "and stays there: {}",
        game.player_position.y
    );
    assert_exact(game.vertical_velocity, 0.0);
    assert_eq!(
        game.locomotion_snapshot().state,
        LocomotionState::SurfaceSwimming
    );
}

/// Releasing Jump in deep water descends at the reduced underwater gravity
/// until the body rests on the pool floor.
#[test]
fn releasing_jump_descends_in_water() {
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 7.0, -3.0, 4.0, 0.0, 1.0 / 60.0);
    game.player_position.y = -0.35;
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.is_swimming());

    let before = game.player_position.y;
    let mut released = InputState::default();
    let mut previous = before;
    for _ in 0..180 {
        game.update_player_movement(&mut released, &settings);
        assert!(
            game.player_position.y <= previous + 1e-4,
            "releasing only descends: {previous} then {}",
            game.player_position.y
        );
        previous = game.player_position.y;
    }
    assert!(
        game.player_position.y < before - 0.3,
        "the swimmer sinks: {before} then {}",
        game.player_position.y
    );
    assert!(game.is_underwater());
}

/// Swimming to the wading step beside the pool stands the player up on it, and
/// walking on from there is ordinary, full-speed walking.
#[test]
fn swimming_to_a_shallow_step_stands_up_and_walks() {
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 9.2, -3.0, 4.0, 90.0, 1.0 / 60.0);
    game.player_position.y = -0.38;
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut stood_up = false;
    for _ in 0..300 {
        game.update_player_movement(&mut input, &settings);
        if game.grounded {
            stood_up = true;
            break;
        }
    }
    assert!(stood_up, "the swimmer climbs out onto the step");
    assert!(!game.is_swimming());
    assert!(
        (game.player_floor_y - (-0.85)).abs() < 1e-3,
        "the exit floor is the shallow step: {}",
        game.player_floor_y
    );

    let start_x = game.player_position.x;
    let mut walk = InputState::holding(&[Control::MoveForward]);
    for _ in 0..10 {
        game.update_player_movement(&mut walk, &settings);
        assert!(game.grounded, "the step is walked");
        assert!(!game.is_swimming(), "the shallows never re-enter swimming");
    }
    assert!(
        game.player_position.x > start_x + 0.1,
        "the player walks on: {start_x} then {}",
        game.player_position.x
    );
    assert!(
        (game.locomotion_snapshot().speed - settings.walk_speed).abs() < 1e-2,
        "walking out of the pool is full speed: {}",
        game.locomotion_snapshot().speed
    );
}

/// Shallow water (at or below `WADE_DEPTH`) is waded at the full walking speed
/// and a jump still fires from it.
#[test]
fn shallow_water_is_walked_and_can_still_jump() {
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 10.4, -0.85, 4.0, 90.0, 1.0 / 60.0);
    let settings = Settings {
        walk_speed: 4.5,
        ..Settings::default()
    };

    let mut idle = InputState::default();
    game.update_player_movement(&mut idle, &settings);
    assert!(!game.is_swimming(), "shallow water is waded");
    assert!(!game.is_underwater());
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Idle);

    let mut walk = InputState::holding(&[Control::MoveForward]);
    let start_x = game.player_position.x;
    for _ in 0..5 {
        game.update_player_movement(&mut walk, &settings);
    }
    let walked = game.player_position.x - start_x;
    let expected = settings.walk_speed * (5.0 / 60.0);
    assert!(
        (walked - expected).abs() < 1e-3,
        "full walk speed in the shallows: {walked} vs {expected}"
    );
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Walking);

    // A fresh Jump press still fires while wading.
    let mut jump = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut jump, &settings);
    assert!(!game.grounded, "a wading jump leaves the floor");
    assert!(game.vertical_velocity > 0.0);
}

#[test]
fn locomotion_states_follow_walking_jumping_and_water() {
    let level = pool_level();
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 30.0;
    let settings = Settings::default();

    let mut idle = InputState::default();
    game.update_player_movement(&mut idle, &settings);
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Idle);
    assert_exact(game.locomotion_snapshot().speed, 0.0);
    assert!(!game.is_swimming() && !game.is_underwater());

    let mut walk = InputState::holding(&[Control::MoveForward]);
    game.update_player_movement(&mut walk, &settings);
    let snapshot = game.locomotion_snapshot();
    assert_eq!(snapshot.state, LocomotionState::Walking);
    assert!(
        (snapshot.speed - settings.walk_speed).abs() < 1e-3,
        "the snapshot reports the real speed: {}",
        snapshot.speed
    );

    let mut jump = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut jump, &settings);
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Airborne);
    assert!(!game.is_swimming());
}

/// Walking is classified by speed, not by the distance covered in one frame,
/// so a 3 m/s walk reads `Walking` at every supported frame rate. A per-frame
/// displacement threshold left a player at 144 fps (0.021 m/frame) classified
/// as idle, which froze the character's gait.
#[test]
fn walking_is_classified_at_every_frame_rate() {
    let level = step_rule_level();
    let settings = Settings::default();
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0, 1.0 / 240.0] {
        let mut game = game_for(&level);
        play_at(&mut game, 1.0, 0.0, 4.0, 0.0, delta);
        let mut walk = InputState::holding(&[Control::MoveForward]);
        game.update_player_movement(&mut walk, &settings);
        let snapshot = game.locomotion_snapshot();
        assert_eq!(
            snapshot.state,
            LocomotionState::Walking,
            "walking at {:.1} fps",
            1.0 / delta
        );
        assert!(
            (snapshot.speed - settings.walk_speed).abs() < 1e-3,
            "the walking speed is the walk speed at {:.1} fps: {}",
            1.0 / delta,
            snapshot.speed
        );
    }

    // A player the room edge refuses covers no ground and stays idle: the
    // speed threshold does not turn collision jitter into a walk. Facing
    // north from the room's north edge, the void refuses the step.
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 0.0, 0.0, 1.0 / 144.0);
    let mut blocked = InputState::holding(&[Control::MoveForward]);
    for _ in 0..4 {
        game.update_player_movement(&mut blocked, &settings);
    }
    assert_exact(game.locomotion_snapshot().speed, 0.0);
    assert_eq!(
        game.locomotion_snapshot().state,
        LocomotionState::Idle,
        "a blocked player is idle"
    );
}

#[test]
fn mouse_motion_turns_the_camera_by_sensitivity_and_is_consumed_once() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    // Pixel motion carries no time: a zero delta must still apply it.
    game.sim_delta_seconds = 0.0;
    let settings = Settings::default();
    let mut input = InputState::default();
    input.accumulate_mouse_motion(100.0, 50.0);
    game.update_player_movement(&mut input, &settings);

    let sensitivity = settings.mouse_sensitivity;
    assert!(
        (game.player_yaw - (100.0 * sensitivity).to_radians()).abs() < 1e-5,
        "yaw follows the pixel scale: {}",
        game.player_yaw
    );
    assert!(
        (game.player_pitch - (-(50.0 * sensitivity).to_radians())).abs() < 1e-5,
        "pitch follows the pixel scale: {}",
        game.player_pitch
    );

    // Consumed exactly once: a second update with no new motion is inert.
    let (dx, dy) = input.take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 0.0);
    let yaw = game.player_yaw;
    let pitch = game.player_pitch;
    game.update_player_movement(&mut input, &settings);
    assert_exact(game.player_yaw, yaw);
    assert_exact(game.player_pitch, pitch);
}

#[test]
fn mouse_motion_is_consumed_but_not_applied_outside_playing() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Paused);
    let settings = Settings::default();
    let mut input = InputState::default();
    input.accumulate_mouse_motion(100.0, 100.0);
    game.update_player_movement(&mut input, &settings);
    assert_exact(game.player_yaw, 0.0);
    assert_exact(game.player_pitch, 0.0);
    // It is discarded, not saved for the resume.
    let (dx, dy) = input.take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 0.0);
}

#[test]
fn non_finite_mouse_motion_never_reaches_the_camera() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 0.0;
    let settings = Settings::default();
    let mut input = InputState::default();
    input.accumulate_mouse_motion(f32::NAN, f32::INFINITY);
    game.update_player_movement(&mut input, &settings);
    assert!(game.player_yaw.is_finite() && game.player_pitch.is_finite());
    assert_exact(game.player_yaw, 0.0);
    assert_exact(game.player_pitch, 0.0);
}

/// A basin shallower than the standing eye height still lets the swimmer
/// submerge: the body is horizontal, so the eye reaches the pool floor
/// clearance rather than `floor + EYE_HEIGHT`.
#[test]
fn a_shallow_pool_still_lets_the_swimmer_submerge() {
    // The Places Demo's pool relationship: basin floor -3.0 and surface -1.65
    // are 1.35 m apart, less than the 1.6 m standing eye height.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "shallow_pool",
            "name": "Shallow Pool",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 8.0,
                      "height": 4.2, "floor_y": -1.5 },
            "floor_regions": [
                { "x": 2.0, "z": 1.0, "width": 8.0, "depth": 6.0, "offset_y": -1.5 }
            ],
            "water": [
                { "x": 2.0, "z": 1.0, "width": 8.0, "depth": 6.0,
                  "surface_y": -1.65, "bottom_y": -3.0 }
            ]
        }"#,
    )
    .expect("the shallow pool parses");
    let mut game = game_for(&level);
    play_at(&mut game, 6.0, -3.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut input = InputState::default();
    let mut deepest = game.player_position.y;
    for _ in 0..600 {
        game.update_player_movement(&mut input, &settings);
        deepest = deepest.min(game.player_position.y);
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-4,
            "the body stays above the pool floor: {}",
            game.player_position.y
        );
    }
    assert!(
        deepest <= -1.65 - 0.4,
        "the swimmer submerges well below the surface: {deepest}"
    );
    assert!(game.is_underwater(), "the sunken swimmer is underwater");
    assert_eq!(game.locomotion_snapshot().state, LocomotionState::Swimming);
}

// ---------------------------------------------------------------------------
// Run 01: ledge falls, prop tops, doorways, stance, water and ladders
// ---------------------------------------------------------------------------

/// The shipped Places Demo, parsed from the repository level file.
fn demo_level() -> LevelDef {
    let content = std::fs::read_to_string("assets/levels/places_demo.json")
        .expect("the Places demo level is present");
    LevelDef::from_json(&content).expect("the Places demo parses")
}

/// A fresh game on the real demo at `(x, z)` standing on `floor_y`, facing
/// `yaw_degrees`, at a fixed frame delta.
fn demo_game_at(x: f32, floor_y: f32, z: f32, yaw_degrees: f32, delta: f32) -> Game {
    let level = demo_level();
    let mut game = game_for(&level);
    play_at(&mut game, x, floor_y, z, yaw_degrees, delta);
    game
}

/// The demo desk's top is the jump's sizing target: the jump must clear it and
/// land on it, and its sides must still block a walking player.
#[test]
fn standing_jump_lands_on_the_demo_desk_top() {
    let settings = Settings::default();
    let mut game = demo_game_at(4.6, 0.0, 6.7, 0.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut landed_on_desk = false;
    for _ in 0..90 {
        game.update_player_movement(&mut input, &settings);
        if game.grounded && (game.player_floor_y - OFFICE_DESK_TOP_M).abs() < 1e-3 {
            landed_on_desk = true;
            assert!(
                (game.feet_y() - OFFICE_DESK_TOP_M).abs() < 1e-3,
                "the feet stand on the desk top: {}",
                game.feet_y()
            );
            break;
        }
    }
    assert!(
        landed_on_desk,
        "the jump lands on the 0.75 m desk: floor {} at {:?}",
        game.player_floor_y, game.player_position
    );

    // The desk's side blocks a walking player who is not jumping.
    let mut walker = demo_game_at(4.6, 0.0, 6.7, 0.0, 1.0 / 60.0);
    let mut walk = InputState::holding(&[Control::MoveForward]);
    for _ in 0..120 {
        walker.update_player_movement(&mut walk, &settings);
    }
    assert!(walker.grounded && walker.player_floor_y.abs() < 1e-3);
    assert!(
        walker.player_position.z >= 6.15 + PLAYER_RADIUS - 1e-3,
        "the desk face stops the walk one radius short: {:?}",
        walker.player_position
    );
}

/// A solid prop under a jumping head blocks the rise, its side blocks a
/// standing body, and a crouched body passes underneath it.
#[test]
fn a_prop_underside_blocks_the_head_and_a_crouch_fits_under() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "low_beam",
            "name": "Low Beam",
            "spawn": { "x": 2.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "props": [
                { "model": "core:crate", "x": 5.0, "z": 5.0, "y": 1.0,
                  "size": [2.0, 0.3, 2.0], "solid": true }
            ]
        }"#,
    )
    .expect("the low-beam level parses");
    let settings = Settings::default();

    // Standing, the beam is a wall: walking into its footprint is stopped.
    let mut game = game_for(&level);
    play_at(&mut game, 7.5, 0.0, 5.0, 270.0, 1.0 / 60.0);
    let mut walk = InputState::holding(&[Control::MoveForward]);
    for _ in 0..60 {
        game.update_player_movement(&mut walk, &settings);
    }
    assert!(
        game.player_position.x >= 6.0 + PLAYER_RADIUS - 1e-3,
        "the standing body is blocked by the beam edge: {:?}",
        game.player_position
    );

    // Crouched, the same body fits under the 1.0 m beam.
    let mut game = game_for(&level);
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    game.stance = Stance::Crouched;
    game.player_position.y = game.player_floor_y + CROUCH_EYE_HEIGHT;
    let mut jump = InputState::holding(&[Control::Jump]);
    let mut highest = game.player_position.y;
    let mut bumped = false;
    for _ in 0..120 {
        game.update_player_movement(&mut jump, &settings);
        highest = highest.max(game.player_position.y);
        if !game.grounded && game.vertical_velocity == 0.0 {
            bumped = true;
        }
    }
    let max_eye = 1.0 - (CROUCH_HEIGHT - CROUCH_EYE_HEIGHT) - CONTACT_EPS + 1e-3;
    assert!(
        highest <= max_eye,
        "the crouched head stops under the beam: {highest} vs {max_eye}"
    );
    assert!(bumped, "the beam underside consumes the upward velocity");
    assert!(game.grounded, "the bumped jump falls back to the floor");
}

/// Pressing C crouches and pressing it again stands: the stance is exactly half
/// the standing height, the feet are anchored, and holding the key never
/// repeats the toggle.
#[test]
fn crouch_toggles_and_anchors_the_feet() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let feet = game.feet_y();

    let mut pressed = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Crouched);
    assert!((game.body_height() - CROUCH_HEIGHT).abs() < 1e-6);
    assert!((CROUCH_HEIGHT - PLAYER_HEIGHT / 2.0).abs() < 1e-6);
    assert!((game.feet_y() - feet).abs() < 1e-5, "the feet do not move");
    assert!((game.player_position.y - (feet + CROUCH_EYE_HEIGHT)).abs() < 1e-5);

    // Holding the key is still one toggle.
    for _ in 0..30 {
        game.update_player_movement(&mut pressed, &settings);
    }
    assert!(game.is_crouched(), "a held key never repeats the toggle");

    // Release, then press again: stand back up with the feet in the same place.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    assert!((game.feet_y() - feet).abs() < 1e-5, "the feet do not move");
    assert!((game.player_position.y - (feet + EYE_HEIGHT)).abs() < 1e-5);
}

/// Uncrouching is clearance-checked: under a 1.0 m beam the request is refused
/// and the player stays crouched without being moved through the box.
#[test]
fn blocked_uncrouch_keeps_the_crouched_body() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "blocked_uncrouch",
            "name": "Blocked Uncrouch",
            "spawn": { "x": 2.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "props": [
                { "model": "core:crate", "x": 5.0, "z": 5.0, "y": 1.0,
                  "size": [2.0, 0.3, 2.0], "solid": true }
            ]
        }"#,
    )
    .expect("the blocked-uncrouch level parses");
    let mut game = game_for(&level);
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    game.stance = Stance::Crouched;
    game.player_position.y = game.player_floor_y + CROUCH_EYE_HEIGHT;
    let crouched_eye = game.player_position.y;
    let feet = game.feet_y();
    let settings = Settings::default();

    let mut pressed = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut pressed, &settings);
    assert!(
        game.is_crouched(),
        "the 1.0 m beam blocks standing up from 0.9 m"
    );
    assert!(
        (game.player_position.y - crouched_eye).abs() < 1e-5 && (game.feet_y() - feet).abs() < 1e-5,
        "a refused uncrouch never moves the body"
    );

    // Once clear of the beam (disc included), the second press stands.
    game.player_position.x = 2.0;
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    assert!((game.player_position.y - (feet + EYE_HEIGHT)).abs() < 1e-5);
}

/// Jumping while inside the demo's 2.1 m doorway bumps the head on the real
/// header and keeps crossing without the historical forward teleport: the head
/// never clips the header and no frame moves further than the walk speed
/// allows.
#[test]
fn jumping_through_the_demo_doorway_never_teleports_or_clips() {
    let settings = Settings::default();
    // Start inside the header's own footprint and jump forward: this is the
    // case the old centre-inside depenetration snapped forward.
    let mut game = demo_game_at(8.9, 0.0, 3.6, 90.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let header_bottom = 2.1_f32;
    let max_eye = header_bottom - (PLAYER_HEIGHT - EYE_HEIGHT) + CONTACT_EPS + 2e-3;
    let mut previous_x = game.player_position.x;
    let mut max_step = 0.0_f32;
    let mut bumped = false;
    for _ in 0..90 {
        game.update_player_movement(&mut input, &settings);
        max_step = max_step.max((game.player_position.x - previous_x).abs());
        previous_x = game.player_position.x;
        let under_header = game.player_position.x + PLAYER_RADIUS > 8.85
            && game.player_position.x - PLAYER_RADIUS < 9.15;
        if under_header {
            assert!(
                game.player_position.y <= max_eye,
                "the head stays under the header: {}",
                game.player_position.y
            );
        }
        if !game.grounded && game.vertical_velocity == 0.0 {
            bumped = true;
        }
    }
    assert!(bumped, "the jump under the header bumps the head");
    assert!(
        game.player_position.x > 9.6,
        "the jump crosses the doorway: {:?}",
        game.player_position
    );
    assert!(
        max_step <= settings.walk_speed / 60.0 + 1e-3,
        "no forward teleport: {max_step} m in one frame"
    );
}

/// Repeated jumps directly under the demo doorway header: the head clamps to
/// the same plane every time and each jump lands back on the floor.
#[test]
fn repeated_jumps_under_the_demo_header_stay_bounded() {
    let settings = Settings::default();
    let mut game = demo_game_at(9.0, 0.0, 3.6, 90.0, 1.0 / 60.0);
    let max_eye = 2.1 - (PLAYER_HEIGHT - EYE_HEIGHT) + CONTACT_EPS + 1e-3;
    let mut idle = InputState::default();
    let mut jump = InputState::holding(&[Control::Jump]);
    for round in 0..4 {
        game.update_player_movement(&mut idle, &settings);
        let mut highest = game.player_position.y;
        for _ in 0..90 {
            game.update_player_movement(&mut jump, &settings);
            highest = highest.max(game.player_position.y);
            assert!(
                game.player_position.y <= max_eye,
                "round {round} clips the header: {}",
                game.player_position.y
            );
            if game.grounded {
                break;
            }
        }
        assert!(
            game.grounded,
            "round {round} lands back on the doorway floor"
        );
        assert!(
            highest > max_eye - 0.2,
            "round {round} still reaches the header: {highest}"
        );
    }
}

/// A jump on a staircase lands on the rendered tread under the player, not the
/// continuous pitch line, and the floor is never pulled while airborne.
#[test]
fn landing_on_a_staircase_uses_the_rendered_tread() {
    let level = smooth_stairs_level();
    let floor = WalkableFloor::from_level(&level);
    // Near the end of the first tread, where the pitch line is nearly a whole
    // riser above the rendered step.
    let (x, z) = (8.29_f32, 4.0_f32);
    let stepped = floor.height_at(x, z).expect("inside the flight");
    let pitched = floor.walk_height_at(x, z).expect("inside the flight");
    assert!(
        pitched > stepped + 0.1,
        "the fixture must distinguish the pitch line from the tread: {pitched} vs {stepped}"
    );
    let mut game = game_for(&level);
    play_at(&mut game, x, stepped, z, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let takeoff = game.player_floor_y;
    let mut jump = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut jump, &settings);
    assert!(!game.grounded);
    let mut released = InputState::default();
    for _ in 0..120 {
        game.update_player_movement(&mut released, &settings);
        if !game.grounded {
            assert!(
                (game.player_floor_y - takeoff).abs() < 1e-6,
                "an airborne player is never pulled onto the stairs: {}",
                game.player_floor_y
            );
        }
        if game.grounded {
            break;
        }
    }
    assert!(game.grounded);
    assert!(
        (game.player_floor_y - stepped).abs() < 1e-4,
        "the landing uses the rendered tread {stepped}, not the pitch line {pitched}: {}",
        game.player_floor_y
    );
}

/// The demo's real staircase joins the hall floor to the balcony with no blip:
/// up and down, every frame steps by at most the authored riser and the top
/// lands flush on the walnut floor.
#[test]
fn the_demo_staircase_joins_the_hall_and_the_balcony() {
    let settings = Settings::default();
    let mut game = demo_game_at(54.0, -0.9, 13.6, 90.0, 1.0 / 60.0);
    let riser = 2.1_f32 / 8.0;
    // The pitch line adds at most one frame of its own slope at the foot or
    // the top, so the join is bounded by one riser plus that slope.
    let pitch_step = riser / 0.30 * (settings.walk_speed / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut previous_eye = game.player_position.y;
    let mut reached_top = false;
    for _ in 0..220 {
        game.update_player_movement(&mut input, &settings);
        let dy = game.player_position.y - previous_eye;
        previous_eye = game.player_position.y;
        assert!(
            dy.abs() <= riser + pitch_step + 1e-3,
            "the join steps by at most one riser plus the pitch slope: {dy}"
        );
        if game.player_floor_y > 1.19 {
            reached_top = true;
            break;
        }
    }
    assert!(
        reached_top,
        "the flight tops out on the balcony: {} at {:?}",
        game.player_floor_y, game.player_position
    );
    assert!((game.player_floor_y - 1.2).abs() < 1e-3);

    game.player_yaw = (-90.0_f32).to_radians();
    let mut previous_floor = game.player_floor_y;
    for _ in 0..220 {
        game.update_player_movement(&mut input, &settings);
        assert!(
            game.player_floor_y <= previous_floor + 1e-3,
            "descending never climbs: {} then {}",
            previous_floor,
            game.player_floor_y
        );
        previous_floor = game.player_floor_y;
        if game.player_position.x < 54.5 {
            break;
        }
    }
    assert!(
        (game.player_floor_y - (-0.9)).abs() < 1e-3,
        "the descent returns to the hall floor: {}",
        game.player_floor_y
    );
}

/// Walking off the demo pool deck is a real drop into the water; holding Jump
/// surfaces the swimmer and the body never passes the basin floor.
#[test]
fn walking_off_the_demo_deck_falls_into_the_pool_and_swims() {
    let settings = Settings::default();
    let mut game = demo_game_at(21.0, -1.5, 12.0, 270.0, 1.0 / 60.0);
    // Walk off the edge with no jump: the drop itself must lose support.
    let mut walk = InputState::holding(&[Control::MoveForward]);
    let mut airborne = false;
    let mut swimming = false;
    for _ in 0..240 {
        game.update_player_movement(&mut walk, &settings);
        if !game.grounded {
            airborne = true;
        }
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the body never passes the basin floor: {}",
            game.player_position.y
        );
        if game.is_swimming() {
            swimming = true;
            break;
        }
    }
    assert!(airborne, "the deck edge is a real drop with no jump");
    assert!(swimming, "the fall ends in swimming");

    // Holding Jump from the water surfaces the swimmer at the float line.
    let mut surface_input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    for _ in 0..240 {
        game.update_player_movement(&mut surface_input, &settings);
    }
    assert!(
        (game.player_position.y - (-1.65 + FLOAT_EYE_MARGIN)).abs() <= SWIM_BOB_AMPLITUDE + 1e-3,
        "the float line holds at the surface: {}",
        game.player_position.y
    );
}

/// The demo's own ladder carries a physically-entered swimmer from the basin
/// to the deck: fall in, swim to the face, attach with movement intent alone,
/// climb (collision-checked, no teleport) and step onto the real deck floor.
#[test]
fn the_demo_pool_ladder_climbs_from_the_water_to_the_deck() {
    let settings = Settings::default();
    let mut game = demo_game_at(21.0, -1.5, 12.0, 270.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);

    // Phase 1: walk off the deck west and fall into the basin.
    for _ in 0..240 {
        game.update_player_movement(&mut input, &settings);
        if game.is_swimming() {
            break;
        }
    }
    assert!(game.is_swimming(), "the route starts in the water");

    // Phase 2: turn east and swim into the ladder face; movement intent alone
    // must attach.
    game.player_yaw = 90.0_f32.to_radians();
    let mut attached = false;
    for _ in 0..240 {
        game.update_player_movement(&mut input, &settings);
        attached |= game.is_climbing();
        if attached {
            break;
        }
    }
    assert!(attached, "swimming into the face attaches the climber");

    // Phase 3: climb to the top and step onto the deck.
    let mut previous_x = game.player_position.x;
    let mut max_step = 0.0_f32;
    let mut max_eye = game.player_position.y;
    let mut landed_on_deck = false;
    for _ in 0..300 {
        game.update_player_movement(&mut input, &settings);
        max_step = max_step.max((game.player_position.x - previous_x).abs());
        previous_x = game.player_position.x;
        max_eye = max_eye.max(game.player_position.y);
        if game.grounded && game.player_position.x > 20.0 {
            landed_on_deck = true;
            break;
        }
    }
    assert!(
        landed_on_deck,
        "the climb tops out on the deck: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(
        (game.player_floor_y - (-1.5)).abs() < 1e-3,
        "the exit floor is the real deck: {}",
        game.player_floor_y
    );
    assert!(
        max_eye <= -1.5 + EYE_HEIGHT + 1e-3,
        "the climber never pops above the deck line: {max_eye}"
    );
    assert!(
        max_step <= settings.walk_speed * LADDER_SIDE_SPEED_FACTOR / 60.0 + 1e-3,
        "the climb never teleports: {max_step} m in one frame"
    );
}

/// The ladder never attaches from its exit side, even when the movement input
/// points along the climb direction: the side check is what decides, not the
/// intent alone. (The demo case above is the real level; this is the minimal
/// case that fails if the side rule is removed.)
#[test]
fn a_ladder_never_attaches_from_its_exit_side() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "ladder_sides",
            "name": "Ladder Sides",
            "spawn": { "x": 2.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "ladders": [
                { "x": 4.7, "z": 4.7, "width": 0.6, "depth": 0.6,
                  "bottom_y": 0.0, "top_y": 3.0, "facing_degrees": 90.0 }
            ]
        }"#,
    )
    .expect("the ladder-sides level parses");
    let settings = Settings::default();

    // Past the centre along +X, pressing +X (along the climb): intent alone
    // would attach, so only the side rule can refuse it.
    let mut game = game_for(&level);
    play_at(&mut game, 5.5, 0.0, 5.0, 90.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut max_eye = game.player_position.y;
    for _ in 0..90 {
        game.update_player_movement(&mut input, &settings);
        max_eye = max_eye.max(game.player_position.y);
        assert!(!game.is_climbing(), "the exit side never attaches");
    }
    assert!(
        max_eye <= EYE_HEIGHT + 1e-3,
        "no rise from the exit side: {max_eye}"
    );

    // Behind the centre, the same intent attaches.
    let mut game = game_for(&level);
    play_at(&mut game, 4.5, 0.0, 5.0, 90.0, 1.0 / 60.0);
    game.update_player_movement(&mut input, &settings);
    assert!(game.is_climbing(), "the approach side attaches");
}

/// An overhead obstruction stops a climb without pushing the climber down or
/// detaching them.
#[test]
fn a_ladder_obstruction_holds_the_climber_in_place() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "ladder_obstruction",
            "name": "Ladder Obstruction",
            "spawn": { "x": 2.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "props": [
                { "model": "core:crate", "x": 5.0, "z": 5.0, "y": 1.2,
                  "size": [2.0, 0.3, 2.0], "solid": true }
            ],
            "ladders": [
                { "x": 4.7, "z": 4.7, "width": 0.6, "depth": 0.6,
                  "bottom_y": 0.0, "top_y": 3.0, "facing_degrees": 90.0 }
            ]
        }"#,
    )
    .expect("the ladder-obstruction level parses");
    let settings = Settings::default();
    let mut game = game_for(&level);
    play_at(&mut game, 4.5, 0.0, 5.0, 90.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.is_climbing(), "the approach side attaches");
    let feet = game.feet_y();
    for _ in 0..30 {
        game.update_player_movement(&mut input, &settings);
        assert!(game.is_climbing(), "the obstruction does not detach");
        assert!(
            game.feet_y() >= feet - 1e-4,
            "the obstruction never pushes the climber down: {} vs {feet}",
            game.feet_y()
        );
    }
}

/// Releasing the movement key holds the climb; backing away and jumping both
/// detach, and a detach over water hands control back to the swimmer.
#[test]
fn the_pool_ladder_releases_on_backing_away_and_jump() {
    let settings = Settings::default();
    let make_attached = || {
        let mut game = demo_game_at(19.3, -3.0, 12.0, 90.0, 1.0 / 60.0);
        game.grounded = false;
        game.player_position.y = -1.55;
        game.swimming = true;
        let mut input = InputState::holding(&[Control::MoveForward]);
        game.update_player_movement(&mut input, &settings);
        assert!(game.is_climbing(), "the fixture attaches");
        game
    };

    // Release: hold position.
    let mut game = make_attached();
    let mut idle = InputState::default();
    let held_eye = game.player_position.y;
    for _ in 0..30 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert!(game.is_climbing(), "releasing holds the ladder");
    assert!(
        (game.player_position.y - held_eye).abs() < 1e-4,
        "the hold does not slide: {} vs {held_eye}",
        game.player_position.y
    );

    // Backing away (the camera now faces -X, so forward is away): detach and
    // resume swimming over the water.
    game.player_yaw = 270.0_f32.to_radians();
    let mut away = InputState::holding(&[Control::MoveForward]);
    game.update_player_movement(&mut away, &settings);
    assert!(!game.is_climbing(), "backing away releases the ladder");
    assert!(game.is_swimming(), "the released swimmer re-enters water");

    // Jump: detach with the ordinary launch. The climber is part-way up the
    // rails, above the waterline, so the jump rises clear instead of being
    // recaptured by the swim state.
    let mut game = make_attached();
    game.player_position.y = -0.9;
    game.vertical_velocity = 0.0;
    let before = game.player_position.y;
    let mut jump = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut jump, &settings);
    assert!(!game.is_climbing(), "jump releases the ladder");
    assert!(
        game.player_position.y > before,
        "the jump launch rises off the ladder: {before} then {}",
        game.player_position.y
    );
}

/// A stance change in deep water leaves the eye exactly where it was: the swim
/// pose's buoyancy is separate from the land eye offset, so there is no boost
/// or shrink when crouching or standing while floating.
#[test]
fn stance_changes_in_water_do_not_move_the_eye() {
    let settings = Settings::default();
    let mut game = demo_game_at(12.0, -3.0, 12.0, 90.0, 0.0);
    game.grounded = false;
    game.player_position.y = -2.4;
    game.swimming = true;
    let mut idle = InputState::default();
    game.update_player_movement(&mut idle, &settings);
    assert!(game.swimming);

    let eye = game.player_position.y;
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched(), "the crouch request applies in water");
    assert_exact(game.player_position.y, eye);

    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut crouch, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    assert_exact(game.player_position.y, eye);
}

/// Wading on the demo's submerged walk-in step: the eye is exactly the stance
/// offset above the real floor, and crouching in the shallows anchors the feet
/// on the step.
#[test]
fn wading_uses_the_stance_eye_offset_on_the_real_step() {
    let settings = Settings::default();
    let mut game = demo_game_at(12.0, -1.85, 16.4, 90.0, 1.0 / 60.0);
    let mut idle = InputState::default();
    game.update_player_movement(&mut idle, &settings);
    assert!(!game.is_swimming(), "0.2 m of water is waded");
    assert!(
        (game.player_position.y - (-1.85 + EYE_HEIGHT)).abs() < 1e-3,
        "the wading eye is floor plus the standing offset: {}",
        game.player_position.y
    );

    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched());
    assert!(
        (game.feet_y() - (-1.85)).abs() < 1e-3,
        "the feet stay on the step"
    );
    assert!(
        (game.player_position.y - (-1.85 + CROUCH_EYE_HEIGHT)).abs() < 1e-3,
        "the crouched eye is the crouch offset above the step: {}",
        game.player_position.y
    );
}

/// A mid-depth pool (0.6 m) is waded by a standing player; crouching drops the
/// eye into the swim band and floats, and standing back up recovers to wading
/// without a snap through the pool floor.
#[test]
fn a_mid_depth_pool_wades_and_recovers_from_a_crouch() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "mid_depth",
            "name": "Mid Depth",
            "spawn": { "x": 1.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 2.0, "z": 1.0, "width": 8.0, "depth": 6.0, "offset_y": -0.6 }
            ],
            "water": [
                { "x": 2.0, "z": 1.0, "width": 8.0, "depth": 6.0,
                  "surface_y": 0.0, "bottom_y": -0.6 }
            ]
        }"#,
    )
    .expect("the mid-depth pool parses");
    let settings = Settings::default();
    let mut game = game_for(&level);
    play_at(&mut game, 6.0, -0.6, 4.0, 0.0, 1.0 / 60.0);
    let mut idle = InputState::default();
    game.update_player_movement(&mut idle, &settings);
    assert!(
        !game.is_swimming(),
        "0.6 m of water is waded with the standing eye above the surface"
    );
    assert!(
        (game.player_position.y - (-0.6 + EYE_HEIGHT)).abs() < 1e-3,
        "the wading eye is the standing offset above the pool floor: {}",
        game.player_position.y
    );

    // Crouching lowers the eye into the swim band: the pose floats.
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched());
    game.update_player_movement(&mut idle, &settings);
    assert!(game.is_swimming(), "the crouched eye reaches the swim band");

    // Standing back up recovers the wading pose on the real floor.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut crouch, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    for _ in 0..30 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert!(!game.is_swimming(), "standing recovers to wading");
    assert!(game.grounded);
    assert!(
        (game.feet_y() - (-0.6)).abs() < 1e-3,
        "the feet are on the pool floor: {}",
        game.feet_y()
    );
    assert!(
        (game.player_position.y - (-0.6 + EYE_HEIGHT)).abs() < 1e-3,
        "the recovered eye is the standing offset: {}",
        game.player_position.y
    );
}

/// A single hitch-sized frame must not launch the player or tunnel through the
/// basin floor, and the jump apex is stable across the supported frame rates
/// plus the clamped hitch delta.
#[test]
fn a_frame_hitch_does_not_tunnel_or_launch() {
    let settings = Settings::default();
    let mut game = demo_game_at(12.0, -3.0, 12.0, 0.0, MAX_SIM_DELTA);
    game.grounded = false;
    game.player_position.y = -0.8;
    game.swimming = false;
    let mut idle = InputState::default();
    let mut deepest = game.player_position.y;
    for _ in 0..30 {
        game.update_player_movement(&mut idle, &settings);
        deepest = deepest.min(game.player_position.y);
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the hitch never tunnels through the basin floor: {}",
            game.player_position.y
        );
    }
    assert!(game.is_swimming(), "the fall still lands in the water");

    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0, MAX_SIM_DELTA] {
        let apex = jump_apex_at(delta);
        assert!(
            (apex - JUMP_APEX_M).abs() < 0.02,
            "the apex holds at delta {delta}: {apex}"
        );
    }
}

/// A floor-region rim beside a surface swimmer is a wall, not an overhead: the
/// swimmer's head clamp must never read the basin-to-step rim as a ceiling and
/// drag the eye toward the pool floor. (The historical clamp referenced the
/// swimmer's virtual feet and did exactly that.)
#[test]
fn a_floor_rim_never_drags_a_surface_swimmer_down() {
    let settings = Settings::default();
    let mut game = demo_game_at(12.0, -3.0, 15.3, 180.0, 1.0 / 60.0);
    game.grounded = false;
    game.player_position.y = -1.65 + FLOAT_EYE_MARGIN;
    game.swimming = true;
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut min_eye = game.player_position.y;
    for _ in 0..240 {
        game.update_player_movement(&mut input, &settings);
        min_eye = min_eye.min(game.player_position.y);
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the swimmer never passes the basin floor: {}",
            game.player_position.y
        );
    }
    assert!(
        min_eye > -2.0,
        "crossing the step rim never drags the surface swimmer down: {min_eye}"
    );
    // The route is the intended exit: over the step the swimmer rises and
    // stands on real shallow floor (the step or the deck beyond it).
    assert!(game.grounded, "the step exit stands the swimmer up");
    assert!(
        game.player_floor_y >= -1.85 - 1e-3,
        "the exit floor is shallow, not the basin: {}",
        game.player_floor_y
    );
}

/// The Pit's real carpet holes are 3.2 m deep recesses: walking in loses
/// support and falls, and the hole's reset trigger returns the player to the
/// authored spawn instead of leaving them at the bottom. The full 15-hole
/// sweep and the safe carpet between holes are covered by
/// `the_pit_carpet_holes_reset_and_the_carpet_is_safe`; this is the original
/// single-hole walk-in regression, updated for the run-2 reset trigger.
#[test]
fn the_pit_carpet_hole_is_a_real_fall() {
    let content =
        std::fs::read_to_string("levels/level0_pit.json").expect("The Pit level is present");
    let level = LevelDef::from_json(&content).expect("The Pit parses");
    let mut game = game_for(&level);
    let spawn = spawn_position(&level);
    // Walk south from the hall floor into the first 1.6 m hole (x 9.6..11.2,
    // z -23.0..-21.4), 3.2 m below the hall.
    play_at(&mut game, 10.4, 0.0, -20.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut airborne = false;
    for _ in 0..180 {
        game.update_player_movement(&mut input, &settings);
        airborne |= !game.grounded;
        if game.reset_count() >= 1 {
            break;
        }
    }
    assert!(airborne, "walking into the hole loses support");
    assert_eq!(game.reset_count(), 1, "the hole reset trigger fires");
    assert!(
        (game.player_position.x - spawn.x).abs() < 1e-4
            && (game.player_position.z - spawn.z).abs() < 1e-4,
        "the fall returns the player to the beginning: {:?}",
        game.player_position
    );
    assert!(game.grounded);
}

/// Stance changes anchor the feet on stairs, in the air and on a ladder: no
/// boost, no support snap and no invalid collider height.
#[test]
fn stance_changes_on_stairs_in_air_and_on_ladders_anchor_the_feet() {
    let settings = Settings::default();

    // On the smooth staircase's pitch line.
    let level = smooth_stairs_level();
    let floor = WalkableFloor::from_level(&level);
    let (x, z) = (8.6_f32, 4.0_f32);
    let pitch = floor.walk_height_at(x, z).expect("inside the flight");
    let mut game = game_for(&level);
    play_at(&mut game, x, pitch, z, 90.0, 1.0 / 60.0);
    let feet = game.feet_y();
    let mut pressed = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut pressed, &settings);
    assert!(game.is_crouched());
    assert!((game.feet_y() - feet).abs() < 1e-4, "the feet stay put");
    assert!(
        (game.player_floor_y - pitch).abs() < 1e-4,
        "the floor is unchanged"
    );

    // Mid-jump: the crouch shifts the eye down but keeps the feet on their
    // ballistic line, and the landing is still the rendered tread.
    let mut game = game_for(&level);
    play_at(&mut game, x, pitch, z, 90.0, 1.0 / 60.0);
    let mut released = InputState::default();
    let mut jump = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut jump, &settings);
    assert!(!game.grounded);
    let feet_before = game.feet_y();
    let eye_before = game.player_position.y;
    game.update_player_movement(&mut pressed, &settings);
    assert!(game.is_crouched());
    assert!(
        game.feet_y() > feet_before && game.feet_y() < feet_before + 0.1,
        "the airborne feet keep rising on their ballistic line: {} vs {feet_before}",
        game.feet_y()
    );
    assert!(
        game.player_position.y < eye_before - 0.7,
        "the eye drops with the stance, not the body: {} vs {eye_before}",
        game.player_position.y
    );
    for _ in 0..120 {
        game.update_player_movement(&mut released, &settings);
        if game.grounded {
            break;
        }
    }
    assert!(game.grounded, "the crouched jump still lands");

    // On the demo ladder: crouching anchors the feet and keeps the attachment.
    let mut game = demo_game_at(19.3, -3.0, 12.0, 90.0, 1.0 / 60.0);
    game.grounded = false;
    game.player_position.y = -1.9;
    game.swimming = false;
    let mut forward = InputState::holding(&[Control::MoveForward]);
    game.update_player_movement(&mut forward, &settings);
    assert!(game.is_climbing(), "the fixture attaches");
    let feet = game.feet_y();
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut pressed, &settings);
    assert!(game.is_crouched());
    assert!(game.is_climbing(), "the crouch keeps the attachment");
    assert!(
        (game.feet_y() - feet).abs() < 1e-4,
        "the ladder feet stay anchored: {} vs {feet}",
        game.feet_y()
    );
    assert!((game.body_height() - CROUCH_HEIGHT).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// Run 02: interactions, labels and area triggers
// ---------------------------------------------------------------------------

/// A 20x20 room with two aimable plants on one clear line of sight, plus an
/// optional low beam occluder and optional authored extras.
fn interaction_level(props_json: &str, walls_json: &str, triggers_json: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 1,
            "id": "interaction_test",
            "name": "Interaction Test",
            "spawn": {{ "x": 2.0, "z": 5.0, "yaw_degrees": 0.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }},
            "walls": {walls_json},
            "props": {props_json},
            "area_triggers": {triggers_json}
        }}"#
    ))
    .expect("valid interaction test json")
}

/// Two same-model plants on one clear line of sight: one 1.4 m in front of the
/// spawn, one 2.7 m out, both tall enough to intersect a flat view ray.
fn two_plant_level(walls_json: &str, triggers_json: &str) -> LevelDef {
    interaction_level(
        r#"[
            { "id": "near_plant", "display_name": "Near Plant", "model": "core:plant",
              "x": 3.4, "z": 5.0, "size": [0.6, 1.8, 0.6],
              "interaction": { "prompt": "Toggle name",
                               "actions": [{ "action": "toggle_label" }] } },
            { "id": "far_plant", "display_name": "Far Plant", "model": "core:plant",
              "x": 4.7, "z": 5.0, "size": [0.6, 1.8, 0.6],
              "interaction": { "prompt": "Toggle name",
                               "actions": [{ "action": "toggle_label" }] } }
        ]"#,
        walls_json,
        triggers_json,
    )
}

/// Aim the player's yaw and pitch at a world point, matching the camera.
fn aim_at(game: &mut Game, x: f32, z: f32, y: f32) {
    let dx = x - game.player_position.x;
    let dz = z - game.player_position.z;
    let flat = dx.hypot(dz);
    game.player_yaw = dx.atan2(-dz);
    game.player_pitch = (y - game.player_position.y).atan2(flat);
}

/// One press is one interaction: the edge latches, a held key never repeats,
/// release re-arms, and a paused game never latches at all.
#[test]
fn interaction_press_latches_once_and_never_fires_while_paused() {
    let level = two_plant_level("[]", "[]");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    aim_at(&mut game, 3.4, 5.0, 0.9);
    let settings = Settings::default();
    assert_eq!(
        game.interaction_target(),
        Some(0),
        "the near plant is aimed at"
    );

    let mut held = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut held, &settings);
    assert!(
        game.take_interact_press(),
        "the first frame latches a press"
    );
    let report = game.dispatch_interaction().expect("a target");
    assert_eq!(report.labels_shown, 1);
    assert!(game.is_label_visible(0));

    // Held: no second press, no second dispatch.
    game.update_player_movement(&mut held, &settings);
    assert!(!game.take_interact_press(), "a held key never repeats");
    assert!(game.is_label_visible(0));

    // Release, then press again: toggles off.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut held, &settings);
    assert!(game.take_interact_press(), "release re-arms the edge");
    let report = game.dispatch_interaction().expect("a target");
    assert_eq!(report.labels_hidden, 1);
    assert!(!game.is_label_visible(0));

    // Paused: the movement update never latches, even with the key down.
    game.set_app_state(AppState::Paused);
    let mut pressed = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut pressed, &settings);
    assert!(!game.take_interact_press(), "pause suppresses interaction");
}

/// Targeting picks the nearest instance in reach on the aim line, ignores an
/// instance beyond its reach, and keeps duplicate-model state independent.
#[test]
fn interaction_targeting_is_nearest_in_reach_and_duplicates_are_independent() {
    let level = two_plant_level("[]", "[]");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;

    // Aiming at the near plant resolves it; toggling it does not touch the far one.
    aim_at(&mut game, 3.4, 5.0, 0.9);
    assert_eq!(game.interaction_target(), Some(0));
    game.dispatch_actions(
        &game
            .interactables()
            .get(0)
            .expect("near plant")
            .actions
            .clone(),
        Some(0),
    );
    assert!(game.is_label_visible(0));
    assert!(!game.is_label_visible(1), "the far plant is untouched");

    // Aiming at the far plant from the spawn is out of its 2.5 m reach: the
    // ray still hits the near plant first, so the result stays index 0.
    aim_at(&mut game, 4.6, 5.0, 0.9);
    assert_eq!(
        game.interaction_target(),
        Some(0),
        "the near plant blocks the aim line"
    );

    // Stand beside the far plant instead.
    play_at(&mut game, 4.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    aim_at(&mut game, 4.6, 5.0, 0.9);
    assert_eq!(game.interaction_target(), Some(1));
    game.dispatch_actions(
        &game
            .interactables()
            .get(1)
            .expect("far plant")
            .actions
            .clone(),
        Some(1),
    );
    assert!(game.is_label_visible(1));
    assert!(game.is_label_visible(0), "the near plant's label survives");
}

/// A crouched eye targets under a low beam that blocks the standing line of
/// sight, and a nearer obstruction occludes a distant target.
#[test]
fn crouched_eye_height_and_occlusion_respect_geometry() {
    // A raised solid beam at x 2.8..3.6, y 1.0..1.4, z 4.6..5.4: the standing
    // eye (1.6) ray to the plant's bound (0..0.9) dips through it; the crouched
    // eye (0.8) passes under.
    let level = interaction_level(
        r#"[
            { "id": "far_plant", "display_name": "Far Plant", "model": "core:plant",
              "x": 4.6, "z": 5.0,
              "interaction": { "actions": [{ "action": "toggle_label" }] } }
        ]"#,
        "[]",
        "[]",
    );
    let mut level = level;
    level.props.push(crate::level::PropDef {
        id: Some("beam".into()),
        display_name: None,
        interaction: None,
        model: "core:beam".into(),
        x: 3.2,
        y: 1.0,
        z: 5.0,
        rotation_degrees: 0.0,
        scale: 1.0,
        size: Some([0.8, 0.4, 0.8]),
        solid: true,
        lights: Vec::new(),
        float: None,
    });
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    aim_at(&mut game, 4.6, 5.0, 0.9);
    assert_eq!(
        game.interaction_target(),
        None,
        "the beam occludes the standing line of sight"
    );

    // Crouch, and the same aim from the lower eye passes under the beam.
    let settings = Settings::default();
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    assert!(game.is_crouched());
    assert!(
        (game.player_position.y - (game.feet_y() + CROUCH_EYE_HEIGHT)).abs() < 1e-6,
        "the aim origin is the crouched eye"
    );
    aim_at(&mut game, 4.6, 5.0, 0.9);
    assert_eq!(
        game.interaction_target(),
        Some(0),
        "the crouched eye clears the low beam"
    );
}

/// Reloading a level clears every label; `reset_to_start` preserves them.
#[test]
fn labels_reset_on_level_load_and_survive_reset_to_start() {
    let level = two_plant_level("[]", "[]");
    let mut game = game_for(&level);
    assert_eq!(game.interactables().len(), 2);
    let actions = game
        .interactables()
        .get(0)
        .expect("near plant")
        .actions
        .clone();
    game.dispatch_actions(&actions, Some(0));
    assert!(game.is_label_visible(0));

    game.reset_to_spawn();
    assert!(
        game.is_label_visible(0),
        "a view toggle survives reset_to_start"
    );

    let spawn = game.spawn_position();
    let yaw = level.spawn.yaw_degrees.to_radians();
    game.reset_level(spawn, yaw, CollisionWorld::from_level(&level));
    assert!(
        !game.is_label_visible(0),
        "a fresh level load starts with every label hidden"
    );
}

/// Demo integration: Spooner-Man is an aimable entity, and the two pool chairs
/// are independent instances of one model.
#[test]
fn the_demo_authors_entity_and_duplicate_prop_labels() {
    let mut game = game_for(&demo_level());
    let items = game.interactables();
    let spooner = items
        .index_of("spooner_man")
        .expect("Spooner-Man carries an interaction");
    assert_eq!(
        items
            .get(spooner)
            .expect("spooner-man interactable")
            .display_name,
        "Spooner-Man"
    );
    let north = items
        .index_of("pool_chair_north")
        .expect("the north pool chair carries an interaction");
    let south = items
        .index_of("pool_chair_south")
        .expect("the south pool chair carries an interaction");
    assert_ne!(north, south);

    let north_actions = items.get(north).expect("north").actions.clone();
    let south_actions = items.get(south).expect("south").actions.clone();
    let spooner_actions = items.get(spooner).expect("spooner").actions.clone();
    game.dispatch_actions(&north_actions, Some(north));
    assert!(game.is_label_visible(north));
    assert!(!game.is_label_visible(south));
    assert!(!game.is_label_visible(spooner));

    game.dispatch_actions(&south_actions, Some(south));
    game.dispatch_actions(&spooner_actions, Some(spooner));
    assert!(game.is_label_visible(north));
    assert!(game.is_label_visible(south));
    assert!(game.is_label_visible(spooner));
}

/// One trigger entry runs its actions once and only re-arms after the player
/// leaves; `cooldown_seconds` bounds an immediate re-entry and `once` latches
/// per run until a reset re-arms it.
#[test]
fn area_triggers_enter_once_rearm_and_honour_cooldown_and_once() {
    let level = interaction_level(
        r#"[
            { "id": "pad_plant", "display_name": "Pad Plant", "model": "core:plant",
              "x": 8.0, "z": 8.0,
              "interaction": { "actions": [{ "action": "toggle_label" }] } }
        ]"#,
        "[]",
        r#"[
            { "id": "cooldown_pad", "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5, "cooldown_seconds": 1.0,
              "actions": [{ "action": "toggle_label", "target": "pad_plant" }] },
            { "id": "once_pad", "x": 8.0, "z": 12.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5, "once": true,
              "actions": [{ "action": "toggle_label", "target": "pad_plant" }] }
        ]"#,
    );
    let mut game = game_for(&level);
    assert_eq!(game.triggers().len(), 2);
    let plant = game.interactables().index_of("pad_plant").expect("plant");

    // Standing inside the cooldown pad does not repeat, even as time passes.
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    for _ in 0..30 {
        game.update_player_movement(&mut InputState::default(), &Settings::default());
    }
    assert!(game.is_label_visible(plant), "one entry fires once");

    // Leave, return immediately: the 1 s cooldown refuses the re-entry.
    play_at(&mut game, 1.0, 0.0, 1.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.is_label_visible(plant),
        "the cooldown refuses a re-entry"
    );

    // Leave, wait out the cooldown, return: fires again.
    for _ in 0..70 {
        play_at(&mut game, 1.0, 0.0, 1.0, 0.0, 1.0 / 60.0);
        game.update_player_movement(&mut InputState::default(), &Settings::default());
    }
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        !game.is_label_visible(plant),
        "the pad re-arms after leaving"
    );

    // The once pad fires once, then refuses re-entry even after leaving.
    let once_enter = |game: &mut Game| {
        play_at(game, 9.0, 0.0, 13.0, 0.0, 1.0 / 60.0);
        game.update_player_movement(&mut InputState::default(), &Settings::default());
    };
    once_enter(&mut game);
    assert!(game.is_label_visible(plant));
    play_at(&mut game, 1.0, 0.0, 1.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    once_enter(&mut game);
    assert!(game.is_label_visible(plant), "a once trigger stays fired");

    // A reset re-arms the once trigger.
    game.reset_to_spawn();
    once_enter(&mut game);
    assert!(!game.is_label_visible(plant));
}

/// A fast fall crosses a thin trigger band between two frames: the swept test
/// fires where an endpoint-only test would miss.
#[test]
fn a_fast_fall_through_a_thin_trigger_band_is_caught() {
    let level = interaction_level(
        r#"[
            { "id": "band_plant", "display_name": "Band Plant", "model": "core:plant",
              "x": 8.0, "z": 8.0,
              "interaction": { "actions": [{ "action": "toggle_label" }] } }
        ]"#,
        "[]",
        r#"[
            { "id": "band", "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 0.1,
              "actions": [{ "action": "toggle_label", "target": "band_plant" }] }
        ]"#,
    );
    let mut game = game_for(&level);
    let plant = game.interactables().index_of("band_plant").expect("plant");

    // One MAX_SIM_DELTA frame at 25 m/s: the feet move from 1.0 m to below the
    // 10 cm band in a single update.
    game.set_app_state(AppState::Playing);
    game.player_position = Vec3::new(5.0, 1.0 + EYE_HEIGHT, 5.0);
    game.player_floor_y = 1.0;
    game.grounded = false;
    game.vertical_velocity = -25.0;
    game.sim_delta_seconds = MAX_SIM_DELTA;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.is_label_visible(plant),
        "the swept segment crosses the thin band"
    );
}

/// A reset returns the player to the authored spawn, facing the authored yaw
/// with a level pitch, clears velocity/stance/ladder/water state, and re-arms
/// triggers from the spawn so no volume between the old and new positions
/// fires.
#[test]
fn reset_to_start_clears_state_and_never_sweeps_the_teleport() {
    let level = interaction_level(
        "[]",
        "[]",
        r#"[
            { "id": "spawn_pad", "x": 0.5, "z": 0.5, "width": 1.0, "depth": 1.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "actions": [{ "action": "reset_to_start" }] },
            { "id": "far_pad", "x": 8.0, "z": 8.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "actions": [{ "action": "reset_to_start" }] }
        ]"#,
    );
    // Move the spawn into the first pad's volume: the enter semantics must not
    // fire on load, and a reset must not fire it either.
    let mut level = level;
    level.spawn.x = 1.0;
    level.spawn.z = 1.0;
    let mut game = game_for(&level);
    let spawn = game.spawn_position();
    let settings = Settings::default();

    for _ in 0..10 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    assert_eq!(
        game.reset_count(),
        0,
        "a spawn inside a volume does not fire on load"
    );

    // Crouch and gain downward velocity, then enter the far pad.
    game.set_app_state(AppState::Playing);
    game.player_yaw = 1.234;
    game.player_pitch = 0.5;
    game.vertical_velocity = -4.0;
    game.player_position.y += 0.3;
    game.sim_delta_seconds = 1.0 / 60.0;
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched());
    play_at(&mut game, 9.0, 0.0, 9.0, 0.0, 1.0 / 60.0);
    game.player_pitch = 0.5; // play_at resets yaw only
    game.update_player_movement(&mut InputState::default(), &settings);
    assert_eq!(game.reset_count(), 1, "the far pad resets the player");

    // Back at the spawn: position, yaw and pitch restored; velocity and
    // stance cleared; grounded on the authored floor.
    assert!((game.player_position.x - spawn.x).abs() < 1e-5);
    assert!((game.player_position.z - spawn.z).abs() < 1e-5);
    assert!((game.player_position.y - spawn.y).abs() < 1e-5);
    assert!((game.player_yaw - level.spawn.yaw_degrees.to_radians()).abs() < 1e-5);
    assert!((game.player_pitch - 0.0).abs() < 1e-6);
    assert!((game.vertical_velocity - 0.0).abs() < 1e-6);
    assert_eq!(game.stance(), Stance::Standing);
    assert!(!game.is_climbing());
    assert!(!game.is_swimming());

    // Staying at the spawn (inside the first pad's volume) never loops: the
    // player was seeded inside, so there is no entry event to fire.
    for _ in 0..120 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    assert_eq!(
        game.reset_count(),
        1,
        "the spawn volume never re-fires while the player stands in it"
    );

    // The teleport did not sweep the pads between spawn and the far pad: the
    // only fire was the far pad itself.
    assert_eq!(game.triggers().len(), 2);
}

/// Jump held across a reset never launches on the first post-reset frame, and
/// a fresh press still jumps.
#[test]
fn holding_jump_across_a_reset_does_not_launch_or_stick() {
    let level = interaction_level(
        "[]",
        "[]",
        r#"[
            { "id": "far_pad", "x": 8.0, "z": 8.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "actions": [{ "action": "reset_to_start" }] }
        ]"#,
    );
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    play_at(&mut game, 9.0, 0.0, 9.0, 0.0, 1.0 / 60.0);
    let mut held = InputState::holding(&[Control::Jump]);
    let settings = Settings::default();
    game.update_player_movement(&mut held, &settings);
    assert_eq!(game.reset_count(), 1);
    assert!(game.grounded);
    for _ in 0..10 {
        game.update_player_movement(&mut held, &settings);
        assert!(
            game.grounded && game.vertical_velocity <= 0.0,
            "a held jump never launches after a reset"
        );
    }
    // Release, then press: the ordinary jump works.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut held, &settings);
    assert!(!game.grounded, "a fresh press jumps again");
}

/// The dispatcher is bounded, reports missing targets and unsupported actions,
/// and a reset stops the rest of its batch.
#[test]
fn dispatch_reports_missing_targets_unsupported_actions_and_stops_after_reset() {
    let level = two_plant_level("[]", "[]");
    let mut game = game_for(&level);

    let unsupported = [ActionDef::PlayAudio {
        target: None,
        sound: Some("beep".into()),
    }];
    let report = game.dispatch_actions(&unsupported, None);
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.actions_run, 0);

    let missing = [ActionDef::ToggleLabel {
        target: Some("ghost".into()),
    }];
    let report = game.dispatch_actions(&missing, None);
    assert_eq!(report.missing_targets, 1);

    // An explicit target addresses another instance, independently of the
    // acting one: the near plant's interaction toggles the far plant.
    let cross = [ActionDef::ToggleLabel {
        target: Some("far_plant".into()),
    }];
    let report = game.dispatch_actions(&cross, Some(0));
    assert_eq!(report.labels_shown, 1);
    assert!(game.is_label_visible(1));
    assert!(
        !game.is_label_visible(0),
        "the actor's own label is untouched"
    );
    game.dispatch_actions(&cross, Some(0));

    // An explicit target that cannot resolve must never fall back to the
    // actor: that would toggle the wrong instance.
    let unresolved = [
        ActionDef::ToggleLabel {
            target: Some("ghost".into()),
        },
        ActionDef::ToggleLabel {
            target: Some("   ".into()),
        },
    ];
    let report = game.dispatch_actions(&unresolved, Some(0));
    assert_eq!(report.missing_targets, 2);
    assert_eq!(report.labels_toggled(), 0);
    assert!(
        !game.is_label_visible(0),
        "a failed explicit target never retargets the actor"
    );

    // Composition in order, with a reset stopping the batch: the label after
    // the reset must not have been announced.
    let batch = [
        ActionDef::ToggleLabel { target: None },
        ActionDef::ResetToStart,
        ActionDef::ToggleLabel {
            target: Some("far_plant".into()),
        },
    ];
    let report = game.dispatch_actions(&batch, Some(0));
    assert_eq!(report.actions_run, 2);
    assert!(report.player_reset);
    assert_eq!(report.labels_shown, 1);
    assert_eq!(report.labels_hidden, 0);
    assert!(
        !game.is_label_visible(1),
        "the action after the reset was deferred"
    );

    // Programmatically oversized batches are truncated to the bound (validation
    // rejects them for real maps): 10 toggles run 8.
    let oversized: Vec<ActionDef> = (0..10)
        .map(|_| ActionDef::ToggleLabel {
            target: Some("near_plant".into()),
        })
        .collect();
    let report = game.dispatch_actions(&oversized, None);
    assert_eq!(
        report.actions_run,
        crate::level::MAX_ACTIONS_PER_SOURCE,
        "the dispatcher never runs more than the bound"
    );
}

/// Every intended carpet hole in The Pit resets the player, and the carpet
/// between the holes stays safe to walk on.
#[test]
fn the_pit_carpet_holes_reset_and_the_carpet_is_safe() {
    let content =
        std::fs::read_to_string("levels/level0_pit.json").expect("The Pit level is present");
    let level = LevelDef::from_json(&content).expect("The Pit parses");
    validate_pit_level(&level);
    let triggers = AreaTriggers::from_level(&level);
    assert_eq!(triggers.len(), 15, "one trigger per intended hole");
    let ids: Vec<&str> = triggers.triggers().iter().map(|t| t.id.as_str()).collect();
    assert!(ids.contains(&"pit_hole_1"));
    assert!(ids.contains(&"pit_hole_15"));

    let settings = Settings::default();
    let spawn = spawn_position(&level);
    let spawn_yaw = level.spawn.yaw_degrees.to_radians();
    for trigger in triggers.triggers() {
        let mut game = Game::new(spawn, spawn_yaw, CollisionWorld::from_level(&level));
        let cx = f32::midpoint(trigger.x0, trigger.x1);
        let cz = f32::midpoint(trigger.z0, trigger.z1);
        // Start on the carpet 0.8 m north of the hole and walk south into it.
        play_at(&mut game, cx, 0.0, cz - 0.8, 180.0, 1.0 / 60.0);
        let mut input = InputState::holding(&[Control::MoveForward]);
        let mut reset = false;
        for _ in 0..180 {
            game.update_player_movement(&mut input, &settings);
            if game.reset_count() >= 1 {
                reset = true;
                break;
            }
        }
        assert!(reset, "walking into {} resets the player", trigger.id);
        assert!(
            (game.player_position.x - spawn.x).abs() < 1e-4
                && (game.player_position.z - spawn.z).abs() < 1e-4,
            "{} returns the player to the authored spawn",
            trigger.id
        );
        assert!((game.player_yaw - spawn_yaw).abs() < 1e-4);
        assert!((game.vertical_velocity - 0.0).abs() < 1e-6);
        assert!(game.grounded);
    }

    // The carpet cross between the holes is ordinary floor: walk the full
    // column along the x gap (x 11.2..12.8) without tripping a trigger.
    let mut game = Game::new(spawn, spawn_yaw, CollisionWorld::from_level(&level));
    play_at(&mut game, 12.0, 0.0, -27.0, 180.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward]);
    for _ in 0..300 {
        game.update_player_movement(&mut input, &settings);
        assert_eq!(game.reset_count(), 0, "carpet between holes is safe");
    }
    assert!(game.grounded);
    assert!(
        game.player_floor_y.abs() < 1e-3,
        "the carpet stays the hall floor: {}",
        game.player_floor_y
    );

    // Repeated resets in one run: the same game falls in, returns to the
    // spawn, walks in again and returns again — no cooldown stall, no loop at
    // the spawn, and the counter advances each time.
    let mut repeat = Game::new(spawn, spawn_yaw, CollisionWorld::from_level(&level));
    let first = triggers.get(0).expect("the first hole");
    for expected in 1..=2 {
        let cx = f32::midpoint(first.x0, first.x1);
        let cz = f32::midpoint(first.z0, first.z1);
        play_at(&mut repeat, cx, 0.0, cz - 0.8, 180.0, 1.0 / 60.0);
        let mut walk = InputState::holding(&[Control::MoveForward]);
        let mut settled = false;
        for _ in 0..180 {
            repeat.update_player_movement(&mut walk, &settings);
            if repeat.reset_count() >= expected {
                settled = true;
                break;
            }
        }
        assert!(settled, "fall {expected} returns to the spawn");
        assert!((repeat.player_position.x - spawn.x).abs() < 1e-4);
    }
    assert_eq!(repeat.reset_count(), 2, "repeated Pit resets stay bounded");
}

/// The Pit level (a per-user drop-in) must pass the same validation the game
/// applies, including the new trigger schema.
fn validate_pit_level(level: &LevelDef) {
    crate::loader::validate_level(level).expect("The Pit validates");
}

/// An explicit `toggle_label` target may name a prop that has no interaction of
/// its own: it joins the resolved set as a label-only instance (never aimable),
/// and the action toggles *its* label rather than the actor's.
#[test]
fn explicit_targets_can_be_label_only_props() {
    let level = interaction_level(
        r#"[
            { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
              "x": 8.0, "z": 8.0 },
            { "id": "switch", "display_name": "Switch", "model": "core:switch",
              "x": 4.0, "z": 5.0,
              "interaction": { "actions": [{ "action": "toggle_label", "target": "lamp" }] } }
        ]"#,
        "[]",
        "[]",
    );
    let mut game = game_for(&level);
    let lamp = game
        .interactables()
        .index_of("lamp")
        .expect("the target prop resolves as a label-only instance");
    let switch = game
        .interactables()
        .index_of("switch")
        .expect("the switch is aimable");
    assert!(
        game.interactables()
            .get(lamp)
            .expect("lamp")
            .actions
            .is_empty(),
        "a label-only target has no actions and is never aimable"
    );

    let actions = game
        .interactables()
        .get(switch)
        .expect("switch")
        .actions
        .clone();
    let report = game.dispatch_actions(&actions, Some(switch));
    assert_eq!(report.labels_shown, 1);
    assert!(game.is_label_visible(lamp), "the target's label toggles");
    assert!(
        !game.is_label_visible(switch),
        "the actor's label is untouched"
    );
}

/// A stance toggle while swimming moves the feet without player movement; the
/// swept trigger origin is captured after the stance edge, so it must not
/// fabricate a crossing of a thin band.
#[test]
fn a_crouch_toggle_in_water_does_not_fabricate_a_trigger_crossing() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "swim_stance_trigger",
            "name": "Swim Stance Trigger",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0,
                      "floor_y": -2.0 },
            "water": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "surface_y": 0.0, "bottom_y": -2.0 }
            ],
            "props": [
                { "id": "band_plant", "display_name": "Band Plant", "model": "core:plant",
                  "x": 1.0, "z": 1.0,
                  "interaction": { "actions": [{ "action": "toggle_label" }] } }
            ],
            "area_triggers": [
                { "id": "band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": -1.5, "top_y": -1.0,
                  "actions": [{ "action": "toggle_label", "target": "band_plant" }] }
            ]
        }"#,
    )
    .expect("the swim trigger level parses");
    let mut game = game_for(&level);
    let plant = game
        .interactables()
        .index_of("band_plant")
        .expect("the plant");
    // Float at the surface: standing feet -1.6 are inside the band, crouched
    // feet -0.8 are above it.
    game.set_app_state(AppState::Playing);
    game.player_position = Vec3::new(4.0, 0.0, 4.0);
    game.grounded = false;
    game.swimming = true;
    game.vertical_velocity = 0.0;
    game.sim_delta_seconds = 1.0 / 60.0;

    let settings = Settings::default();
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched());
    assert!(
        !game.is_label_visible(plant),
        "the stance change did not cross the band"
    );
}

/// When one frame crosses two trigger volumes, the first dispatches and the
/// later one is deferred to the next frame rather than lost.
#[test]
fn a_swept_crossing_of_a_later_trigger_is_deferred_not_dropped() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "stacked_triggers",
            "name": "Stacked Triggers",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 6.0,
                      "floor_y": -3.0 },
            "props": [
                { "id": "upper", "display_name": "Upper", "model": "core:plant",
                  "x": 1.0, "z": 1.0,
                  "interaction": { "actions": [{ "action": "toggle_label" }] } },
                { "id": "lower", "display_name": "Lower", "model": "core:plant",
                  "x": 2.0, "z": 1.0,
                  "interaction": { "actions": [{ "action": "toggle_label" }] } }
            ],
            "area_triggers": [
                { "id": "upper_band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": 0.0, "top_y": 0.2,
                  "actions": [{ "action": "toggle_label", "target": "upper" }] },
                { "id": "lower_band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": -1.0, "top_y": -0.8,
                  "actions": [{ "action": "toggle_label", "target": "lower" }] }
            ]
        }"#,
    )
    .expect("the stacked trigger level parses");
    let mut game = game_for(&level);
    let upper = game.interactables().index_of("upper").expect("upper");
    let lower = game.interactables().index_of("lower").expect("lower");

    // One MAX_SIM_DELTA frame at 25 m/s crosses both bands.
    game.set_app_state(AppState::Playing);
    game.player_position = Vec3::new(4.0, 1.0 + EYE_HEIGHT, 4.0);
    game.player_floor_y = -3.0;
    game.grounded = false;
    game.vertical_velocity = -25.0;
    game.sim_delta_seconds = MAX_SIM_DELTA;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.is_label_visible(upper),
        "the first band fires this frame"
    );
    assert!(
        !game.is_label_visible(lower),
        "the later band is deferred, not fired in the same frame"
    );

    // The deferred crossing dispatches on the next frame even though the
    // player has already fallen past it.
    game.sim_delta_seconds = 1.0 / 60.0;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.is_label_visible(lower),
        "the deferred band still fires on a later frame"
    );
}

/// A level with one routed, interactable, non-solid entity in a walled room:
/// the entity walks from (2, 2) toward (4, 2), the wall at x = 5..5.2 blocks
/// the far side.
fn routed_entity_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "routed_entity",
            "name": "Routed Entity",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [
                { "x": 5.0, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.5 }
            ],
            "props": [
                { "id": "walker", "display_name": "Walker", "model": "entity:x",
                  "x": 2.0, "z": 2.0, "size": [0.4, 0.5, 0.4],
                  "interaction": { "actions": [{ "action": "toggle_label" }] } }
            ],
            "routes": [
                { "id": "walker", "loop": true, "steps": [
                    { "step": "move_to", "x": 4.0, "z": 2.0, "speed": 0.5 },
                    { "step": "wait", "seconds": 0.25 },
                    { "step": "play", "clip": "idle", "seconds": 0.5, "loop": true }
                ] }
            ]
        }"#,
    )
    .expect("the routed entity level parses")
}

#[test]
fn a_route_moves_the_entity_and_its_live_anchor_follows() {
    let level = routed_entity_level();
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    let authored_anchor = game.interactables().get(0).expect("one target").anchor;
    let settings = Settings::default();

    // Two metres at 0.5 m/s: four seconds of 0.1 s frames, with margin.
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..45 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    let state = game.route_state("walker").expect("the route exists");
    assert!(
        (state.position.x - 4.0).abs() < 0.05,
        "{:?}",
        state.position
    );
    assert!(!state.blocked);
    let frame = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "walker")
        .expect("the renderer handoff carries the entity");
    assert_eq!(frame.transform, Some((state.position, state.yaw)));
    let live_anchor = game.interactables().get(0).expect("one target").anchor;
    assert!(
        (live_anchor.x - authored_anchor.x - 2.0).abs() < 0.05,
        "the live anchor followed the entity: {live_anchor:?} vs {authored_anchor:?}"
    );
    // The frame's cue is the route's own (wait or play), never stale idle
    // from before the route existed.
    assert!(
        matches!(
            frame.cue,
            PoseCue::Idle | PoseCue::Walk { .. } | PoseCue::Clip { .. }
        ),
        "a supported cue: {:?}",
        frame.cue
    );
}

#[test]
fn a_route_never_walks_the_entity_through_a_wall() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "blocked_route",
            "name": "Blocked Route",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [
                { "x": 5.0, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.5 }
            ],
            "props": [
                { "id": "walker", "model": "entity:x", "x": 2.0, "z": 2.0,
                  "size": [0.4, 0.5, 0.4] }
            ],
            "routes": [
                { "id": "walker", "steps": [
                    { "step": "move_to", "x": 8.0, "z": 2.0, "speed": 1.0 }
                ] }
            ]
        }"#,
    )
    .expect("the blocked route level parses");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    let settings = Settings::default();
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..100 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    let state = game.route_state("walker").expect("the route exists");
    assert!(state.blocked, "the wall blocks the route");
    assert!(
        state.position.x < 5.0,
        "the entity never enters the wall: {:?}",
        state.position
    );
}

#[test]
fn play_animation_sets_a_pose_override_that_reset_clears() {
    let level = routed_entity_level();
    let mut game = game_for(&level);
    let index = game
        .interactables()
        .index_of("walker")
        .expect("the routed entity is interactable");
    let actions = vec![ActionDef::PlayAnimation {
        target: None,
        clip: Some("arms_up".into()),
        looped: false,
    }];
    let report = game.dispatch_actions(&actions, Some(index));
    assert_eq!(report.animations_started, 1);
    assert_eq!(report.missing_targets, 0);
    assert_eq!(
        game.animation_override("walker"),
        Some(&PoseCue::Clip {
            name: "arms_up".into(),
            once: true,
            paused: false
        })
    );
    let frame = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "walker")
        .expect("the override reaches the frame");
    assert_eq!(
        frame.cue,
        PoseCue::Clip {
            name: "arms_up".into(),
            once: true,
            paused: false
        }
    );

    // A looping action replaces the one-shot override on the same instance.
    let looping = vec![ActionDef::PlayAnimation {
        target: Some("walker".into()),
        clip: Some("idle".into()),
        looped: true,
    }];
    game.dispatch_actions(&looping, None);
    assert_eq!(
        game.animation_override("walker"),
        Some(&PoseCue::Clip {
            name: "idle".into(),
            once: false,
            paused: false
        })
    );

    // An unknown instance is a named miss, never a fallback onto the actor.
    let ghost = vec![ActionDef::PlayAnimation {
        target: Some("ghost".into()),
        clip: Some("idle".into()),
        looped: false,
    }];
    let report = game.dispatch_actions(&ghost, Some(index));
    assert_eq!(report.missing_targets, 1);
    assert_eq!(report.animations_started, 0);

    // A reset returns routed entities to their spawn and clears overrides.
    game.reset_to_spawn();
    assert!(game.animation_override("walker").is_none());
    let state = game.route_state("walker").expect("the route exists");
    assert!((state.position.x - 2.0).abs() < 1e-6);
    assert_eq!(state.step, 0);
}

#[test]
fn a_programmatic_animation_without_a_clip_is_unsupported_not_a_panic() {
    let level = routed_entity_level();
    let mut game = game_for(&level);
    let index = game.interactables().index_of("walker").expect("target");
    let report = game.dispatch_actions(
        &[ActionDef::PlayAnimation {
            target: None,
            clip: None,
            looped: false,
        }],
        Some(index),
    );
    assert_eq!(report.unsupported, 1);
    assert!(game.animation_override("walker").is_none());
}

/// The entity showcase fixture authors both rats' routes and the mannequin's
/// pose cycle; the two rat instances advance independently through the same
/// collision world.
#[test]
fn the_entity_showcase_runs_two_independent_rat_routes() {
    let content = std::fs::read_to_string("tests/fixtures/levels/entity_showcase.json")
        .expect("the entity showcase fixture is present");
    let level = LevelDef::from_json(&content).expect("the entity showcase parses");
    crate::loader::validate_level(&level).expect("the entity showcase validates");
    let mut game = game_for(&level);
    assert_eq!(
        game.routes().len(),
        5,
        "two rat routes, one mannequin pose cycle and two skeleton pose cycles"
    );

    game.set_app_state(AppState::Playing);
    let settings = Settings::default();
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..40 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    let rat_a = game.route_state("rat_a").expect("rat_a route").position;
    let rat_b = game.route_state("rat_b").expect("rat_b route").position;
    assert!(
        !rat_a.is_nan() && !rat_b.is_nan(),
        "both routes stay finite"
    );
    assert_ne!(
        rat_a, rat_b,
        "the two instances are in different places after the same frames"
    );
    // rat_a started at (7, 6) and walks toward (9, 6) at 0.198 m/s; four
    // seconds of frames moves it about 0.8 m and nowhere else.
    assert!((rat_a.x - 7.8).abs() < 0.05, "{rat_a:?}");
    assert!((rat_a.z - 6.0).abs() < 0.05, "{rat_a:?}");
    // rat_b runs the first leg at 0.573 m/s from (14, 12) toward (19, 12).
    assert!(
        (rat_b.x - 0.573f32.mul_add(4.0, 14.0)).abs() < 0.1,
        "{rat_b:?}"
    );
    assert!((rat_b.z - 12.0).abs() < 0.05, "{rat_b:?}");

    // The mannequin cycles poses: after the first two-second step it is in the
    // arms-forward clip, driven by its own route cue.
    let frame = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "mannequin_pose")
        .expect("the mannequin has a frame");
    assert!(
        matches!(
            frame.cue,
            PoseCue::Clip { ref name, once: true, .. } if name == "pose_arms_forward"
        ),
        "the pose cycle reached its second clip: {:?}",
        frame.cue
    );

    // The seated skeleton's cycle is on its second step at t = 4 s (3 s of
    // chair, then standing), and it is still seated in its own chair pose at
    // the moment its step changed.
    let seated = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "skeleton_chair")
        .expect("the seated skeleton has a frame");
    assert!(
        matches!(
            seated.cue,
            PoseCue::Clip { ref name, .. } if name == "pose_stand"
        ),
        "the chair cycle moved to standing: {:?}",
        seated.cue
    );

    // Its interactable resets the pose exactly like the map authors it.
    let index = game
        .interactables()
        .index_of("mannequin_pose")
        .expect("the mannequin is interactable");
    let actions = game
        .interactables()
        .get(index)
        .expect("target")
        .actions
        .clone();
    let report = game.dispatch_actions(&actions, Some(index));
    assert_eq!(report.animations_started, 1);
    assert_eq!(
        game.animation_override("mannequin_pose"),
        Some(&PoseCue::Clip {
            name: "pose_stand".into(),
            once: true,
            paused: false
        })
    );
}

/// A reset returns a routed entity to its spawn and republishes the authored
/// anchor and bounds, not the live values captured at reset time.
#[test]
fn reset_to_spawn_restores_a_routed_entitys_authored_anchor_and_bounds() {
    let level = routed_entity_level();
    let mut game = game_for(&level);
    let authored_anchor = game.interactables().get(0).expect("target").anchor;
    let authored_bounds = game.interactables().get(0).expect("target").bounds;
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..20 {
        game.update_player_movement(&mut InputState::default(), &Settings::default());
    }
    let live_anchor = game.interactables().get(0).expect("target").anchor;
    assert!(
        (live_anchor.x - authored_anchor.x).abs() > 0.5,
        "the entity moved before the reset: {live_anchor:?}"
    );

    game.reset_to_spawn();
    let restored = game.interactables().get(0).expect("target");
    assert_eq!(restored.anchor, authored_anchor);
    assert_eq!(restored.bounds, authored_bounds);

    // A frame after the reset keeps the authored values (the route is back at
    // its spawn), so the fix is not just the reset frame.
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    let after_frame = game.interactables().get(0).expect("target");
    assert_eq!(after_frame.anchor, authored_anchor);
    assert_eq!(after_frame.bounds, authored_bounds);
}

/// A `play_animation` whose explicit target carries no interaction of its own
/// validates and dispatches: the target joins the instance set as a cue-only
/// instance, exactly as the label-only target rule does.
#[test]
fn play_animation_can_pose_a_prop_with_no_interaction_of_its_own() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "cue_only_target",
            "name": "Cue Only Target",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "props": [
                { "id": "actor", "model": "core:crate", "x": 2.0, "z": 2.0,
                  "size": [0.4, 0.4, 0.4],
                  "interaction": { "actions": [
                    { "action": "play_animation", "target": "dummy",
                      "clip": "pose_sit_chair" } ] } },
                { "id": "dummy", "model": "skeleton", "x": 4.0, "z": 2.0,
                  "size": [0.42, 1.7, 0.24] }
            ]
        }"#,
    )
    .expect("the cue-only target level parses");
    crate::loader::validate_level(&level).expect("validation accepts the documented target");
    let mut game = game_for(&level);
    let actor = game.interactables().index_of("actor").expect("the actor");
    assert!(
        game.interactables().index_of("dummy").is_some(),
        "the explicit target is a resolvable cue-only instance"
    );
    let actions = game
        .interactables()
        .get(actor)
        .expect("target")
        .actions
        .clone();
    let report = game.dispatch_actions(&actions, Some(actor));
    assert_eq!(report.missing_targets, 0);
    assert_eq!(report.animations_started, 1);
    assert_eq!(
        game.animation_override("dummy"),
        Some(&PoseCue::Clip {
            name: "pose_sit_chair".into(),
            once: true,
            paused: false
        })
    );
}

/// A `face` step re-orients the routed entity's live aim bounds, not just its
/// label anchor.
#[test]
fn a_route_turn_reorients_the_live_aim_bounds() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "turning_route",
            "name": "Turning Route",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "props": [
                { "id": "turner", "display_name": "Turner", "model": "entity:x",
                  "x": 4.0, "z": 2.0, "rotation_degrees": 0.0,
                  "size": [0.2, 0.5, 0.8],
                  "interaction": { "actions": [{ "action": "toggle_label" }] } }
            ],
            "routes": [
                { "id": "turner", "steps": [
                    { "step": "face", "yaw_degrees": 90.0 },
                    { "step": "wait", "seconds": 0.5 }
                ] }
            ]
        }"#,
    )
    .expect("the turning route level parses");
    let mut game = game_for(&level);
    let before = game.interactables().get(0).expect("target").bounds;
    let (width_before, depth_before) =
        (before.max[0] - before.min[0], before.max[2] - before.min[2]);
    assert!(
        width_before < depth_before,
        "the authored yaw is long in z: {width_before} x {depth_before}"
    );

    game.set_app_state(AppState::Playing);
    let settings = Settings::default();
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0..10 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    let state = game.route_state("turner").expect("the route exists");
    assert!((state.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
    let after = game.interactables().get(0).expect("target").bounds;
    let (width_after, depth_after) = (after.max[0] - after.min[0], after.max[2] - after.min[2]);
    assert!(
        width_after > depth_after,
        "the turned aim box is long in x: {width_after} x {depth_after}"
    );
    assert!(
        (width_after - depth_before).abs() < 1.0e-3,
        "the turned extents swap, not grow: {width_after} vs {depth_before}"
    );
}

/// A small level with two copies of the wall switch, each carrying the demo's
/// composed interaction: the lever toggle plus the instance-local label.
fn switch_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "switch_test",
            "name": "Switch Test",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
            "props": [
                { "id": "switch_a", "display_name": "Switch A", "model": "home:wall_switch",
                  "x": 2.0, "z": 0.2, "size": [0.18, 0.18, 0.1],
                  "interaction": { "prompt": "Switch", "actions": [
                    { "action": "toggle_animation", "clip": "toggle" },
                    { "action": "toggle_label" } ] } },
                { "id": "switch_b", "display_name": "Switch B", "model": "home:wall_switch",
                  "x": 4.0, "z": 0.2, "size": [0.18, 0.18, 0.1],
                  "interaction": { "prompt": "Switch", "actions": [
                    { "action": "toggle_animation", "clip": "toggle" },
                    { "action": "toggle_label" } ] } }
            ]
        }"#,
    )
    .expect("the switch level parses")
}

/// Run 05: one press flips exactly one switch's lever target, composes with the
/// label toggle in the same batch, and leaves the other switch untouched.
#[test]
fn toggle_animation_flips_one_instance_and_composes_with_a_label() {
    let level = switch_level();
    let mut game = game_for(&level);
    let index = game
        .interactables()
        .index_of("switch_a")
        .expect("switch A is interactable");
    let report = game.dispatch_actions(
        &[
            ActionDef::ToggleAnimation {
                target: None,
                clip: Some("toggle".into()),
            },
            ActionDef::ToggleLabel { target: None },
        ],
        Some(index),
    );
    assert_eq!(report.actions_run, 2, "both actions run");
    assert_eq!(report.animations_started, 1);
    assert_eq!(report.labels_shown, 1);
    assert_eq!(report.missing_targets, 0);
    assert_eq!(
        game.animation_override("switch_a"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 1.0,
        })
    );
    assert_eq!(
        game.animation_override("switch_b"),
        None,
        "the other switch is untouched"
    );
    let frames: Vec<&EntityFrame> = game
        .entity_frames()
        .iter()
        .filter(|frame| frame.instance_id == "switch_a")
        .collect();
    assert_eq!(frames.len(), 1, "only switch A gets a cue frame");
    assert_eq!(
        frames.first().expect("one frame").cue,
        PoseCue::Scrub {
            name: "toggle".into(),
            target: 1.0,
        }
    );
    assert!(
        game.entity_frames()
            .iter()
            .all(|frame| frame.instance_id != "switch_b"),
        "switch B has no frame at all"
    );

    // A second press flips the target back toward the rest end rather than
    // restarting at it; the label toggles off again.
    let report = game.dispatch_actions(
        &[
            ActionDef::ToggleAnimation {
                target: Some("switch_a".into()),
                clip: Some("toggle".into()),
            },
            ActionDef::ToggleLabel {
                target: Some("switch_a".into()),
            },
        ],
        None,
    );
    assert_eq!(report.labels_hidden, 1);
    assert_eq!(
        game.animation_override("switch_a"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 0.0,
        })
    );
    assert_eq!(game.animation_override("switch_b"), None);

    // Switch B runs its own independent toggle.
    let b = game
        .interactables()
        .index_of("switch_b")
        .expect("switch B is interactable");
    game.dispatch_actions(
        &[ActionDef::ToggleAnimation {
            target: None,
            clip: Some("toggle".into()),
        }],
        Some(b),
    );
    assert_eq!(
        game.animation_override("switch_b"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 1.0,
        })
    );
    assert_eq!(
        game.animation_override("switch_a"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 0.0,
        }),
        "switch A keeps its own target"
    );
}

/// Run 05: a toggle needs a clip name and a target that resolves on its own.
#[test]
fn toggle_animation_rejects_a_missing_clip_or_target() {
    let level = switch_level();
    let mut game = game_for(&level);
    let report = game.dispatch_actions(
        &[ActionDef::ToggleAnimation {
            target: Some("ghost".into()),
            clip: Some("toggle".into()),
        }],
        None,
    );
    assert_eq!(report.missing_targets, 1);
    assert_eq!(report.actions_run, 0);
    let report = game.dispatch_actions(
        &[ActionDef::ToggleAnimation {
            target: Some("switch_a".into()),
            clip: Some("   ".into()),
        }],
        None,
    );
    assert_eq!(report.unsupported, 1, "a blank clip is unsupported");
    assert_eq!(game.animation_override("switch_a"), None);
}

/// Run 05: a reset returns every switch to its rest end while a playing
/// animation override is still cleared.
#[test]
fn reset_retargets_toggles_to_rest_and_clears_playing_animations() {
    let level = switch_level();
    let mut game = game_for(&level);
    let index = game
        .interactables()
        .index_of("switch_a")
        .expect("switch A is interactable");
    game.dispatch_actions(
        &[ActionDef::ToggleAnimation {
            target: None,
            clip: Some("toggle".into()),
        }],
        Some(index),
    );
    assert_eq!(
        game.animation_override("switch_a"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 1.0,
        })
    );
    game.reset_to_spawn();
    assert_eq!(
        game.animation_override("switch_a"),
        Some(&PoseCue::Scrub {
            name: "toggle".into(),
            target: 0.0,
        }),
        "the toggle survives the reset, aimed back at its rest end"
    );
    game.dispatch_actions(
        &[ActionDef::PlayAnimation {
            target: Some("switch_a".into()),
            clip: Some("toggle".into()),
            looped: false,
        }],
        None,
    );
    game.reset_to_spawn();
    assert_eq!(
        game.animation_override("switch_a"),
        None,
        "a playing animation override is cleared by the reset"
    );
}

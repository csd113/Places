//! Unit tests for the game state, movement and collision.

// Test code: unwrap/expect, indexing, loose casts, panics and permissive
// arithmetic are idiomatic in tests; the production lints stay enforced
// everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use std::fmt::Write as _;

use super::*;
use crate::entities::WorldCommand;
use crate::entities::components::StateValue;
use crate::render::SCENE_NEAR_M;
use crate::test_support::assert_exact;

mod movement_diagnostics;
mod movement_performance;
mod movement_regression;

/// Advances `frames` deterministic 60 Hz simulation frames with no input.
fn advance_frames(game: &mut Game, frames: usize) {
    let settings = Settings::default();
    let mut input = InputState::default();
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
    }
}

/// The typed state `name` on the authored entity `id`, cloned for assertion.
fn state_of(game: &Game, id: &str, name: &str) -> Option<StateValue> {
    let handle = game.entities().handle_of(id)?;
    game.entities()
        .components()
        .states
        .get(handle)?
        .get(name)
        .cloned()
}

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
            "format_version": 3,
            "id": "steps",
            "name": "Steps",
            "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 8.0, "height": 4.0 } ],
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

/// Places the rendered eye at `position` and derives the physical feet with
/// the current eye offset, keeping the smoothed-eye invariant exact for tests
/// that place the body directly.
fn set_eye_position(game: &mut Game, position: Vec3) {
    game.player_position = position;
    game.feet_y = position.y - game.eye_offset_current;
}

/// Moves the rendered eye to `eye` and derives the physical feet.
fn set_eye_y(game: &mut Game, eye: f32) {
    game.player_position.y = eye;
    game.feet_y = eye - game.eye_offset_current;
}

/// Forces a settled stance (and thus a settled eye offset) without waiting out
/// the transition, keeping the feet where they are.
fn force_stance(game: &mut Game, stance: Stance) {
    game.stance = stance;
    game.eye_offset_current = stance.eye_offset();
    game.player_position.y = game.feet_y + game.eye_offset_current;
}

#[test]
fn test_spawn_position_resolves_the_local_floor() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "elevated_spawn",
            "name": "Elevated Spawn",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                      "height": 3.0, "floor_y": 2.0 } ],
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
    set_eye_position(&mut game, Vec3::new(11.5, EYE_HEIGHT, 4.0));
    game.player_floor_y = 0.0;
    // The two 0.35 m steps are climbable. Walking on past the 0.7 m platform
    // walks off its far edge, so the run records the highest floor reached
    // before the fall instead of assuming the player stops at the edge.
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let mut highest = game.player_floor_y;
    for _ in 0_i32..25_i32 {
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
    set_eye_position(&mut game, Vec3::new(6.5, EYE_HEIGHT, 4.0));
    game.player_floor_y = 0.0;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let mut airborne = false;
    for _ in 0_i32..40_i32 {
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
    for _ in 0_i32..40_i32 {
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
fn test_controller_walks_off_the_last_floor_and_falls_into_the_void() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "open_edge",
            "name": "Open Edge",
            "spawn": { "x": 1.0, "z": 4.0, "yaw_degrees": 90.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 } ]
        }"#,
    )
    .expect("open edge json");
    let mut game = game_for(&level);
    walk_forward(&mut game, 60);
    assert!(
        game.player_position.x > 10.3,
        "the unsupported edge must be crossable: {}",
        game.player_position.x
    );
    assert!(!game.grounded, "the void cannot support the player");
    assert!(game.feet_y() < -1.0, "the player falls below world zero");
}

/// A walkable riser cannot raise the body into the underside of a header.
#[test]
fn stepping_under_a_header_requires_clearance_at_the_destination() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "step_header", "name": "Step Header",
            "spawn": {"x": 1.0, "z": 4.0, "yaw_degrees": 90.0},
            "rooms": [{"width": 10.0, "depth": 8.0, "height": 4.0}],
            "floor_regions": [{"x": 3.0, "z": 2.0, "width": 3.0,
                "depth": 4.0, "offset_y": 0.35}],
            "walls": [{"x": 3.0, "y": 2.0, "z": 2.0, "width": 3.0,
                "depth": 4.0, "height": 0.2}]
        }"#,
    )
    .expect("step with a low header");
    let mut game = game_for(&level);
    walk_forward(&mut game, 30);
    assert!(
        game.player_position.x < 3.0,
        "the body must stop before the riser"
    );
    assert!(
        game.feet_y().abs() < STEP_EPS,
        "a refused step leaves the feet on the floor"
    );
    force_stance(&mut game, Stance::Crouched);
    walk_forward(&mut game, 10);
    assert!(
        game.player_position.x > 3.2,
        "the crouched body can climb the riser"
    );
}

/// The landing query must not turn a descending body inside a prop into a
/// climb; the walking step allowance belongs only to supported locomotion.
#[test]
fn airborne_support_never_reaches_up_to_a_prop_top() {
    let level = table_top_level();
    let game = game_for(&level);
    let support = game.support_at(6.0, 4.0, OFFICE_DESK_TOP_M - 0.3);
    assert_eq!(support, Some(0.0), "a top above the feet is not a landing");
    assert_eq!(
        game.support_at(30.0, 30.0, 2.0),
        None,
        "no invisible world floor outside authored rooms"
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
    set_eye_position(&mut game, Vec3::new(1.4, EYE_HEIGHT, 3.2));
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
    let mut ramp_game = game_for(&level);
    let ramp_floor = ramp_game
        .floor
        .walk_height_at(4.5, 0.5)
        .expect("ramp spawn surface");
    set_eye_position(&mut ramp_game, Vec3::new(4.5, ramp_floor + EYE_HEIGHT, 0.5));
    ramp_game.player_floor_y = ramp_floor;
    ramp_game.player_yaw = 180.0_f32.to_radians();
    walk_forward(&mut ramp_game, 60);
    assert!(
        (ramp_game.player_floor_y - 0.75).abs() < 1e-4,
        "the ramp climbs to the platform: floor {}",
        ramp_game.player_floor_y
    );
    // Standing on the ramp part-way up answers a part-way height, and it is
    // monotone from the foot to the platform.
    let floor = WalkableFloor::from_level(&level);
    let mut previous = 0.0_f32;
    for z in 0_i32..=32_i32 {
        let at = f32::mul_add(f32::from(u16::try_from(z).unwrap_or(0)), 0.05, 0.4);
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
            "format_version": 3,
            "id": "max_slope",
            "name": "Max Slope",
            "spawn": { "x": 0.5, "z": 0.5 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 6.0 } ],
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
    set_eye_position(&mut game, Vec3::new(5.6, EYE_HEIGHT, 3.9));
    game.player_floor_y = 0.0;
    game.player_yaw = 180.0_f32.to_radians(); // south, up the ramp
    let settings = Settings {
        walk_speed: 10.0,
        ..Settings::default()
    };
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let mut highest = game.player_floor_y;
    for _ in 0_i32..3_i32 {
        game.update_player_movement(&mut input, &settings);
        highest = highest.max(game.player_floor_y);
    }
    // Stop beyond the top, while still above the authored room floor. Continuing
    // at 10 m/s for four seconds would leave the room and fall into the void.
    advance_frames(&mut game, 90);
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
                "format_version": 3,
                "id": "limit_riser",
                "name": "Limit Riser",
                "spawn": {{ "x": 0.5, "z": 0.5 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                           "height": 4.0, "floor_y": {floor_y} }}],
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
        set_eye_position(&mut game, Vec3::new(4.5, floor_y + EYE_HEIGHT, 1.9));
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
            "format_version": 3,
            "id": "max_slope_down",
            "name": "Max Slope Down",
            "spawn": { "x": 0.5, "z": 0.5 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 6.0 } ],
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
    set_eye_position(&mut game, Vec3::new(6.0, EYE_HEIGHT + 4.0, 6.4));
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
    for _ in 0_i32..30_i32 {
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
    set_eye_position(&mut game, Vec3::new(4.5, EYE_HEIGHT, 0.25));
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
    for _ in 0_i32..40_i32 {
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
    set_eye_position(&mut game, Vec3::new(54.0, -0.9 + EYE_HEIGHT, 13.6));
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
            "format_version": 3,
            "id": "smooth_stairs",
            "name": "Smooth Stairs",
            "spawn": { "x": 2.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 8.0, "height": 4.0 } ],
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
    set_eye_position(&mut game, Vec3::new(x, floor_y + EYE_HEIGHT, z));
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
    for _ in 0_i32..140_i32 {
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
    assert!(!on_stair.is_empty(), "on_stair must contain entries");
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
    for _ in 0_i32..150_i32 {
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
                "format_version": 3,
                "id": "stair_sandwich",
                "name": "Stair Sandwich",
                "spawn": {{ "x": 0.5, "z": 0.5 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 6.0 }} ],
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
            "format_version": 3,
            "id": "no_climb",
            "name": "No Climb",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 8.0, "height": 4.0 } ],
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
    set_eye_position(&mut game, Vec3::new(4.0, EYE_HEIGHT, 4.0));
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
    let mut wall_game = game_for(&level);
    set_eye_position(&mut wall_game, Vec3::new(10.0, EYE_HEIGHT, 4.0));
    wall_game.player_floor_y = 0.0;
    wall_game.player_yaw = 90.0_f32.to_radians();
    walk_forward(&mut wall_game, 60);
    assert!(
        wall_game.player_floor_y.abs() < 1e-4,
        "a wall is not climbed: {}",
        wall_game.player_floor_y
    );
    assert!(
        wall_game.player_position.x <= 12.0 - PLAYER_RADIUS + 1e-3,
        "the player stops at the wall face: {:?}",
        wall_game.player_position
    );
}

// ---------------------------------------------------------------------------
// Gravity, jumping and swimming
// ---------------------------------------------------------------------------

/// A fresh game on `level` at a known floor, walking a fixed frame delta.
fn play_at(game: &mut Game, x: f32, floor_y: f32, z: f32, yaw_degrees: f32, delta: f32) {
    game.set_app_state(AppState::Playing);
    game.eye_offset_current = EYE_HEIGHT;
    game.feet_y = floor_y;
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
            "format_version": 3,
            "id": "pool",
            "name": "Pool",
            "spawn": { "x": 1.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 6.0 } ],
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
            "format_version": 3,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 1.0 } } ]
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
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut input, &settings);
        assert!(game.grounded, "the player stays on the floor");
        assert_exact(game.player_position.y, game.player_floor_y + EYE_HEIGHT);
        assert!(game.player_floor_y.is_finite());
        assert_exact(game.vertical_velocity, 0.0);
    }
}

/// A spawn outside every room stands on the global ground plane at the
/// spawn's own height: it never falls into the void, and a jump from it lands
/// back on the same line.
#[test]
fn off_room_spawn_stands_on_the_ground_plane() {
    let mut game = Game::new(
        Vec3::new(2.0, EYE_HEIGHT, 3.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    let settings = Settings::default();
    let mut input = InputState::default();
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut input, &settings);
        assert_exact(game.player_position.y, EYE_HEIGHT);
    }
    assert!(game.grounded);

    let mut jump = InputState::holding(&[Control::Jump]);
    let mut highest = game.player_position.y;
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut jump, &settings);
        highest = highest.max(game.player_position.y);
    }
    assert!(
        highest > EYE_HEIGHT + 0.5,
        "the jump works off-room too: {highest}"
    );
    assert!(game.grounded, "and it lands back on the ground plane");
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
    for _ in 0_i32..30_i32 {
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
    for _ in 0_i32..300_i32 {
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
    let mut airborne_game = game_for(&level);
    play_at(&mut airborne_game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    airborne_game.update_player_movement(&mut held, &settings); // launch
    let mut released = InputState::default();
    airborne_game.update_player_movement(&mut released, &settings); // release
    assert!(!airborne_game.grounded);
    airborne_game.vertical_velocity = -1.0; // descending mid-air
    let mut pressed = InputState::holding(&[Control::Jump]);
    airborne_game.update_player_movement(&mut pressed, &settings);
    assert!(
        airborne_game.vertical_velocity < 0.0,
        "a mid-air jump must be rejected: {}",
        airborne_game.vertical_velocity
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
    for _ in 0_i32..600_i32 {
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

/// The jump apex is the sizing target at every frame rate: vertical motion runs
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
            "the apex clears the {KITCHEN_COUNTER_TOP_M} m counter by its margin: {apex}"
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
            "format_version": 3,
            "id": "low_room",
            "name": "Low Room",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 2.2 } ]
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
    for _ in 0_i32..180_i32 {
        game.update_player_movement(&mut input, &settings);
        highest = highest.max(game.player_position.y);
        assert!(
            game.player_position.y <= max_eye + 1e-4,
            "the head stays under the ceiling: {}",
            game.player_position.y
        );
        if game.ceiling_contact_this_frame {
            bumped = true;
            assert!(
                game.vertical_velocity <= 0.0,
                "upward impact velocity is consumed"
            );
        }
    }
    assert!(bumped, "the jump reaches the ceiling: {highest}");
    assert!(
        (highest - max_eye).abs()
            <= (0.5 * GRAVITY * game.sim_delta_seconds)
                .mul_add(game.sim_delta_seconds, CONTACT_EPS),
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
    set_eye_y(&mut game, 1.0);
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::default();
    let mut switched = false;
    for _ in 0_i32..300_i32 {
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
    for _ in 0_i32..600_i32 {
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
    set_eye_y(&mut game, -0.35);
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::Jump]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.is_swimming());

    let before = game.player_position.y;
    let mut released = InputState::default();
    let mut previous = before;
    for _ in 0_i32..180_i32 {
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
    set_eye_y(&mut game, -0.38);
    game.grounded = false;
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut stood_up = false;
    for _ in 0_i32..300_i32 {
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
    for _ in 0_i32..10_i32 {
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
    for _ in 0_i32..5_i32 {
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

    // A real wall refuses movement and stays idle; an unsupported edge must
    // instead let the player walk off and fall.
    let mut game = game_for(&level);
    game.walls.push(WallAabb::new(0.0, -0.2, 20.0, 0.2));
    game.collision_index = CollisionIndex::build(&game.walls);
    play_at(&mut game, 1.0, 0.0, PLAYER_RADIUS, 0.0, 1.0 / 144.0);
    let mut blocked = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..4_i32 {
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
            "format_version": 3,
            "id": "shallow_pool",
            "name": "Shallow Pool",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 8.0,
                      "height": 4.2, "floor_y": -1.5 } ],
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
    for _ in 0_i32..600_i32 {
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
// ledge falls, prop tops, doorways, stance, water and ladders
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
    for _ in 0_i32..90_i32 {
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
    for _ in 0_i32..120_i32 {
        walker.update_player_movement(&mut walk, &settings);
    }
    assert!(walker.grounded && walker.player_floor_y.abs() < 1e-3);
    assert!(
        walker.player_position.z >= 6.15 + PLAYER_RADIUS - 1e-3,
        "the desk face stops the walk one radius short: {:?}",
        walker.player_position
    );
}

/// The player jumps from the kitchen floor onto each 0.9 m counter-run top
/// (a base cabinet, the stove and the sink deck), walks along the run onto a
/// neighbouring top, and steps off the front edge back to the floor: real
/// landings at the surface the model actually has (world 0.0 over the -0.9 m
/// kitchen floor), no embedding, and no upward correction larger than a
/// walkable step.
#[test]
fn jumping_onto_the_demo_kitchen_counter_run_lands_walks_and_leaves() {
    let settings = Settings::default();
    // The front faces are at z 3.75 (cabinets, stove) and 3.70 (sink); a
    // take-off 1.45 m out lands inside the 1.18..1.78 m window the 1.0 m apex
    // and the 3.0 m/s walk give.
    for (name, x) in [("base cabinet", 55.4), ("stove", 56.0), ("sink", 54.8)] {
        let mut game = demo_game_at(x, -0.9, 5.2, 0.0, 1.0 / 60.0);
        let mut jump = InputState::holding(&[Control::MoveForward, Control::Jump]);
        let mut landed = false;
        let mut highest_feet = game.feet_y();
        let mut previous_feet = game.feet_y();
        for _ in 0_i32..120_i32 {
            game.update_player_movement(&mut jump, &settings);
            highest_feet = highest_feet.max(game.feet_y());
            assert!(
                game.feet_y() <= previous_feet + PLAYER_STEP_HEIGHT + STEP_EPS,
                "{name}: no upward correction larger than a step: {} then {}",
                previous_feet,
                game.feet_y()
            );
            previous_feet = game.feet_y();
            if game.grounded && game.player_floor_y.abs() < 1e-3 {
                landed = true;
                break;
            }
        }
        assert!(
            landed,
            "{name}: the jump lands on the 0.9 m top: floor {} at {:?}",
            game.player_floor_y, game.player_position
        );
        assert!(
            (game.feet_y() - game.player_floor_y).abs() < 1e-3,
            "{name}: the feet stand on the landed surface: {}",
            game.feet_y()
        );
        assert!(
            highest_feet <= -0.9 + JUMP_APEX_M + 1e-3,
            "{name}: the apex stays the ordinary jump, never a mantle: {highest_feet}"
        );

        // Walk along the run: strafing crosses onto the neighbouring top at
        // the same height with no step, no snag and no fall.
        let start_x = game.player_position.x;
        let mut strafe = InputState::holding(&[Control::StrafeLeft]);
        for frame in 0_i32..20_i32 {
            game.update_player_movement(&mut strafe, &settings);
            assert!(
                game.grounded,
                "{name}: the run stays walkable (frame {frame})"
            );
            assert!(
                game.player_floor_y.abs() < 1e-3,
                "{name}: still on the 0.0 m tops (frame {frame}): {}",
                game.player_floor_y
            );
        }
        let walked = (game.player_position.x - start_x).abs();
        assert!(
            walked > 0.8,
            "{name}: the player walks along the run: {walked}"
        );

        // Leave: walking off the front edge is a real drop to the -0.9 m
        // kitchen floor, and the landing is grounded, never embedded.
        let mut back = InputState::holding(&[Control::MoveBackward]);
        let mut left = false;
        for _ in 0_i32..120_i32 {
            game.update_player_movement(&mut back, &settings);
            if game.grounded && (game.player_floor_y + 0.9).abs() < 1e-3 {
                left = true;
                break;
            }
        }
        assert!(
            left,
            "{name}: the player steps off and lands on the floor: {:?}",
            game.player_position
        );
        assert!((game.feet_y() - game.player_floor_y).abs() < 1e-3);
    }
}

/// The counter jump lands at 30, 60 and 144 fps: the fixed vertical substep
/// keeps the apex frame-rate independent, exactly like the desk jump.
#[test]
fn the_counter_jump_lands_at_30_60_and_144_fps() {
    let settings = Settings::default();
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
        let mut game = demo_game_at(56.0, -0.9, 5.2, 0.0, delta);
        let mut jump = InputState::holding(&[Control::MoveForward, Control::Jump]);
        let mut landed = false;
        for _ in 0_i32..240_i32 {
            game.update_player_movement(&mut jump, &settings);
            if game.grounded && game.player_floor_y.abs() < 1e-3 {
                landed = true;
                break;
            }
        }
        assert!(
            landed,
            "the stove jump lands at delta {delta}: floor {} at {:?}",
            game.player_floor_y, game.player_position
        );
    }
}

/// The raised jump apex still cannot mount the 1.05 m pool guardrail: the
/// rails top out at -0.45 over the -1.5 m deck, above the 1.0 m apex, so the
/// jump is not a universal mantle and the rails keep guarding the deck.
#[test]
fn the_jump_never_mantles_onto_the_pool_guardrail() {
    let settings = Settings::default();
    // The north deck (z 7..10, floor -1.5) south of the rail at z 9.9.
    let mut game = demo_game_at(14.0, -1.5, 8.6, 180.0, 1.0 / 60.0);
    let mut jump = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut highest_feet = game.feet_y();
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut jump, &settings);
        highest_feet = highest_feet.max(game.feet_y());
    }
    assert!(
        highest_feet <= -0.45 + 1e-3,
        "the feet never reach the rail top: {highest_feet}"
    );
    assert!(
        (game.player_floor_y + 1.5).abs() < 1e-3,
        "the deck stays the floor: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.z <= 9.9 - PLAYER_RADIUS + 1e-3,
        "the rail stops the jump one radius short: {:?}",
        game.player_position
    );
}

/// A 12x12 room with a 2x2 m solid table whose top is at 0.75 m
/// (x 5..7, z 3..5, y 0..0.75).
fn table_top_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "table_top",
            "name": "Table Top",
            "spawn": { "x": 6.0, "z": 7.5 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 4.0 } ],
            "props": [
                { "model": "core:crate", "x": 6.0, "z": 4.0, "y": 0.0,
                  "size": [2.0, 0.75, 2.0], "solid": true }
            ]
        }"#,
    )
    .expect("the table-top level parses")
}

/// Standing on a solid prop top, walking stays on the top: the highest support
/// under the candidate is the top itself, so the player never re-anchors to the
/// room floor below and never gets depenetrated sideways.
#[test]
fn walking_on_a_prop_top_stays_on_the_top() {
    let level = table_top_level();
    let mut game = game_for(&level);
    play_at(&mut game, 6.0, OFFICE_DESK_TOP_M, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut walk = InputState::holding(&[Control::MoveForward]);
    let start_z = game.player_position.z;
    for _ in 0_i32..10_i32 {
        game.update_player_movement(&mut walk, &settings);
        assert!(game.grounded, "the table top is walked");
        assert!(
            (game.player_floor_y - OFFICE_DESK_TOP_M).abs() < 1e-4,
            "the floor is the top, not the room floor: {}",
            game.player_floor_y
        );
        assert!(
            (game.feet_y() - OFFICE_DESK_TOP_M).abs() < 1e-4,
            "the feet never sink into the top: {}",
            game.feet_y()
        );
        assert!(
            (game.player_position.y - (OFFICE_DESK_TOP_M + EYE_HEIGHT)).abs() < 1e-4,
            "the eye keeps the grounded line: {}",
            game.player_position.y
        );
    }
    assert!(
        game.player_position.z <= start_z - 0.3,
        "the player actually walked: {} to {}",
        start_z,
        game.player_position.z
    );
}

/// Walking off a prop top is a real drop, and a fall that ends within a
/// walkable step of the top lands back on it (the bounded step-up), never
/// through its side.
#[test]
fn prop_tops_are_left_by_falling_and_rejoined_by_a_bounded_step() {
    let level = table_top_level();
    let settings = Settings::default();

    // Walk off the north edge: the drop is real and lands on the room floor.
    let mut game = game_for(&level);
    play_at(&mut game, 6.0, OFFICE_DESK_TOP_M, 3.6, 0.0, 1.0 / 60.0);
    let mut walk = InputState::holding(&[Control::MoveForward]);
    let mut airborne = false;
    let mut landed = false;
    for _ in 0_i32..90_i32 {
        game.update_player_movement(&mut walk, &settings);
        if !game.grounded {
            airborne = true;
        } else if airborne {
            landed = true;
            break;
        }
    }
    assert!(airborne, "the table edge is a real drop");
    assert!(landed, "the fall lands on the room floor");
    assert!(
        game.player_floor_y.abs() < 1e-3,
        "floor {}",
        game.player_floor_y
    );

    // A descent from above crosses the top and lands on it. A body already
    // below the top must never be lifted onto it by the walking allowance.
    let mut below_top_game = game_for(&level);
    play_at(
        &mut below_top_game,
        6.0,
        OFFICE_DESK_TOP_M,
        4.0,
        0.0,
        1.0 / 60.0,
    );
    below_top_game.grounded = false;
    below_top_game.player_floor_y = 0.0;
    set_eye_y(&mut below_top_game, OFFICE_DESK_TOP_M + EYE_HEIGHT + 0.3);
    below_top_game.vertical_velocity = -1.0;
    let mut idle = InputState::default();
    let mut landed_on_top = false;
    for _ in 0_i32..60_i32 {
        below_top_game.update_player_movement(&mut idle, &settings);
        if below_top_game.grounded {
            landed_on_top = true;
            break;
        }
    }
    assert!(landed_on_top, "the fall lands");
    assert!(
        (below_top_game.player_floor_y - OFFICE_DESK_TOP_M).abs() < 1e-4,
        "the landing is the prop top, not the room floor: {}",
        below_top_game.player_floor_y
    );
    assert!((below_top_game.feet_y() - OFFICE_DESK_TOP_M).abs() < 1e-4);
}

/// Jump onto the table, walk around on it, walk off the edge and fall, four
/// times over: every landing is on the top, every edge is a fall, and the
/// player stands stably on the room floor afterwards.
#[test]
fn repeated_jumps_onto_a_prop_top_land_and_stand_stably() {
    let level = table_top_level();
    let settings = Settings::default();
    for round in 0_i32..4_i32 {
        let mut game = game_for(&level);
        play_at(&mut game, 6.0, 0.0, 6.6, 0.0, 1.0 / 60.0);
        let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
        let mut landed_on_top = false;
        for _ in 0_i32..120_i32 {
            game.update_player_movement(&mut input, &settings);
            if game.grounded && (game.player_floor_y - OFFICE_DESK_TOP_M).abs() < 1e-3 {
                landed_on_top = true;
                break;
            }
        }
        assert!(
            landed_on_top,
            "round {round} lands on the table: floor {} at {:?}",
            game.player_floor_y, game.player_position
        );
        assert!((game.feet_y() - OFFICE_DESK_TOP_M).abs() < 1e-3);

        // Walk around on the top for a moment: the eye line never dips.
        let mut walk = InputState::holding(&[Control::MoveForward]);
        for _ in 0_i32..8_i32 {
            game.update_player_movement(&mut walk, &settings);
            assert!(game.grounded, "round {round} walks on the top");
            assert!(
                (game.player_position.y - (OFFICE_DESK_TOP_M + EYE_HEIGHT)).abs() < 1e-3,
                "round {round} stays on the top line: {}",
                game.player_position.y
            );
        }

        // Walk off the north edge and fall back to the room floor.
        let mut airborne = false;
        let mut back_on_floor = false;
        for _ in 0_i32..90_i32 {
            game.update_player_movement(&mut walk, &settings);
            if !game.grounded {
                airborne = true;
            } else if airborne {
                back_on_floor = true;
                break;
            }
        }
        assert!(airborne, "round {round} walks off the edge");
        assert!(back_on_floor, "round {round} falls to the room floor");
        assert!(
            game.player_floor_y.abs() < 1e-3 && game.grounded,
            "round {round} stands stably on the floor: {}",
            game.player_floor_y
        );
    }
}

/// A solid prop under a jumping head blocks the rise, its side blocks a
/// standing body, and a crouched body passes underneath it.
#[test]
fn a_prop_underside_blocks_the_head_and_a_crouch_fits_under() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "low_beam",
            "name": "Low Beam",
            "spawn": { "x": 2.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 } ],
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
    for _ in 0_i32..60_i32 {
        game.update_player_movement(&mut walk, &settings);
    }
    assert!(
        game.player_position.x >= 6.0 + PLAYER_RADIUS - 1e-3,
        "the standing body is blocked by the beam edge: {:?}",
        game.player_position
    );

    // Crouched, the same body fits under the 1.0 m beam.
    let mut crouched_game = game_for(&level);
    play_at(&mut crouched_game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    force_stance(&mut crouched_game, Stance::Crouched);
    let mut jump = InputState::holding(&[Control::Jump]);
    let mut highest = crouched_game.player_position.y;
    let mut bumped = false;
    for _ in 0_i32..120_i32 {
        crouched_game.update_player_movement(&mut jump, &settings);
        highest = highest.max(crouched_game.player_position.y);
        if crouched_game.ceiling_contact_this_frame {
            bumped = true;
            assert!(
                crouched_game.vertical_velocity <= 0.0,
                "ceiling removed upward velocity"
            );
        }
    }
    let max_eye = 1.0 - (CROUCH_HEIGHT - CROUCH_EYE_HEIGHT) - CONTACT_EPS + 1e-3;
    assert!(
        highest <= max_eye,
        "the crouched head stops under the beam: {highest} vs {max_eye}"
    );
    assert!(bumped, "the beam underside consumes the upward velocity");
    assert!(
        crouched_game.grounded,
        "the bumped jump falls back to the floor"
    );
}

/// Pressing C crouches and pressing it again stands: the stance is exactly half
/// the standing height, the feet are anchored through the eased transition, and
/// holding the key never repeats the toggle.
#[test]
fn crouch_toggles_and_anchors_the_feet() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let feet = game.feet_y();
    let standing_eye = game.player_position.y;

    let mut pressed = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Crouched);
    assert!((game.body_height() - CROUCH_HEIGHT).abs() < 1e-6);
    assert!((CROUCH_HEIGHT - PLAYER_HEIGHT / 2.0).abs() < 1e-6);
    assert!((game.feet_y() - feet).abs() < 1e-5, "the feet do not move");
    assert!(
        game.player_position.y < standing_eye && game.player_position.y > feet + CROUCH_EYE_HEIGHT,
        "the eye is eased, not snapped: {}",
        game.player_position.y
    );
    assert!(game.eye_offset() > CROUCH_EYE_HEIGHT && game.eye_offset() < EYE_HEIGHT);

    // The transition completes in about `CROUCH_TRANSITION_SECONDS`; the
    // crouched eye then sits exactly the crouched offset above the feet. At the
    // test's 60 Hz delta that is 9 frames; two more settle it.
    let frames = 12_usize;
    let mut previous = game.player_position.y;
    for _ in 0..frames {
        game.update_player_movement(&mut pressed, &settings);
        assert!(
            game.player_position.y <= previous + 1e-6,
            "the crouch never rises: {previous} then {}",
            game.player_position.y
        );
        previous = game.player_position.y;
    }
    assert_exact(game.eye_offset(), CROUCH_EYE_HEIGHT);
    assert!((game.feet_y() - feet).abs() < 1e-5, "the feet do not move");
    assert!((game.player_position.y - (feet + CROUCH_EYE_HEIGHT)).abs() < 1e-5);

    // Holding the key is still one toggle.
    for _ in 0_i32..30_i32 {
        game.update_player_movement(&mut pressed, &settings);
    }
    assert!(game.is_crouched(), "a held key never repeats the toggle");

    // Release, then press again: stand back up with the feet in the same place.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    for _ in 0..frames {
        game.update_player_movement(&mut pressed, &settings);
    }
    assert_exact(game.eye_offset(), EYE_HEIGHT);
    assert!((game.feet_y() - feet).abs() < 1e-5, "the feet do not move");
    assert!((game.player_position.y - (feet + EYE_HEIGHT)).abs() < 1e-5);
}

/// The eye offset eases at the fixed [`CROUCH_TRANSITION_SECONDS`] rate: a
/// single frame never applies the whole offset, the motion is monotonic, the
/// ends snap exactly and the rendered eye never leaves the feet invariant.
#[test]
fn crouch_eye_eases_smoothly_and_snaps_at_the_exact_ends() {
    let level = step_rule_level();
    let mut game = game_for(&level);
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let feet = game.feet_y();
    let full_offset = EYE_HEIGHT - CROUCH_EYE_HEIGHT;
    let per_frame = full_offset / CROUCH_TRANSITION_SECONDS / 60.0;

    let mut pressed = InputState::holding(&[Control::Crouch]);
    let mut previous = game.player_position.y;
    // Two more frames than the transition needs, to pin the snap.
    for frame in 0_i32..12_i32 {
        game.update_player_movement(&mut pressed, &settings);
        let eye = game.player_position.y;
        assert_exact(eye, game.feet_y() + game.eye_offset());
        assert!(
            eye <= previous + 1e-6,
            "frame {frame} rises: {previous} then {eye}"
        );
        assert!(
            previous - eye <= per_frame + 1e-4,
            "frame {frame} snaps: {} m in one frame",
            previous - eye
        );
        assert!(
            eye >= feet + CROUCH_EYE_HEIGHT - 1e-6,
            "never overshoots the crouched eye: {eye}"
        );
        previous = eye;
    }
    assert_exact(game.eye_offset(), CROUCH_EYE_HEIGHT);
    assert!((game.player_position.y - (feet + CROUCH_EYE_HEIGHT)).abs() < 1e-5);

    // Standing eases back up with the same bound and snaps at the top.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    previous = game.player_position.y;
    for frame in 0_i32..12_i32 {
        game.update_player_movement(&mut pressed, &settings);
        let eye = game.player_position.y;
        assert_exact(eye, game.feet_y() + game.eye_offset());
        assert!(
            eye >= previous - 1e-6,
            "frame {frame} falls: {previous} then {eye}"
        );
        assert!(
            eye - previous <= per_frame + 1e-4,
            "frame {frame} snaps: {} m in one frame",
            eye - previous
        );
        assert!(
            eye <= feet + EYE_HEIGHT + 1e-6,
            "never overshoots the standing eye: {eye}"
        );
        previous = eye;
    }
    assert_exact(game.eye_offset(), EYE_HEIGHT);
    assert!((game.player_position.y - (feet + EYE_HEIGHT)).abs() < 1e-5);
}

/// Uncrouching is clearance-checked: under a 1.0 m beam the request is refused
/// and the player stays crouched without being moved through the box.
#[test]
fn blocked_uncrouch_keeps_the_crouched_body() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "blocked_uncrouch",
            "name": "Blocked Uncrouch",
            "spawn": { "x": 2.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 } ],
            "props": [
                { "model": "core:crate", "x": 5.0, "z": 5.0, "y": 1.0,
                  "size": [2.0, 0.3, 2.0], "solid": true }
            ]
        }"#,
    )
    .expect("the blocked-uncrouch level parses");
    let mut game = game_for(&level);
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    force_stance(&mut game, Stance::Crouched);
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

    // Once clear of the beam (disc included), the second press stands, and the
    // eye eases back to the standing line.
    game.player_position.x = 2.0;
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut pressed, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    for _ in 0_i32..12_i32 {
        game.update_player_movement(&mut pressed, &settings);
    }
    assert_exact(game.eye_offset(), EYE_HEIGHT);
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
    for _ in 0_i32..90_i32 {
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
        if game.ceiling_contact_this_frame {
            bumped = true;
            assert!(
                game.vertical_velocity <= 0.0,
                "ceiling removed upward velocity"
            );
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
    for round in 0_i32..4_i32 {
        game.update_player_movement(&mut idle, &settings);
        let mut highest = game.player_position.y;
        for _ in 0_i32..90_i32 {
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
    for _ in 0_i32..120_i32 {
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
    for _ in 0_i32..220_i32 {
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
    for _ in 0_i32..220_i32 {
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
    for _ in 0_i32..240_i32 {
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
    for _ in 0_i32..240_i32 {
        game.update_player_movement(&mut surface_input, &settings);
    }
    assert!(
        (game.player_position.y - (-1.65 + FLOAT_EYE_MARGIN)).abs() <= SWIM_BOB_AMPLITUDE + 1e-3,
        "the float line holds at the surface: {}",
        game.player_position.y
    );
}

/// The demo pool deck is 0.15 m above the waterline: the player can swim down,
/// surface, swim at the surface into the deck rim with Jump held, step up onto
/// the real deck, walk away, jump back in, and exit a second time. Standing on
/// the deck afterwards never oscillates back into the water.
#[test]
fn the_demo_pool_exits_over_the_deck_rim_and_re_enters_cleanly() {
    let settings = Settings::default();
    // The east rim at z = 14, clear of the ladder at z 11.4..12.0.
    let mut game = demo_game_at(12.0, -3.0, 14.0, 90.0, 1.0 / 60.0);
    let mut swim_east = InputState::holding(&[Control::MoveForward, Control::Jump]);

    // Phase 0: swim down. The standing basin floor is deep water, so the first
    // frames enter the swim state and sink toward the float clearance line.
    let mut idle = InputState::default();
    let mut deepest = game.player_position.y;
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut idle, &settings);
        deepest = deepest.min(game.player_position.y);
    }
    assert!(game.is_swimming(), "the basin water is swum");
    assert!(
        game.is_underwater(),
        "the player submerges before surfacing: {deepest}"
    );

    // Phase 1: surface and swim east across the basin to the deck rim.
    let mut exited = false;
    for _ in 0_i32..400_i32 {
        game.update_player_movement(&mut swim_east, &settings);
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the swimmer never passes the basin floor: {}",
            game.player_position.y
        );
        if game.grounded && game.player_floor_y > -1.6 {
            exited = true;
            break;
        }
    }
    assert!(
        exited,
        "the swimmer exits onto the deck: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(
        (game.player_floor_y - (-1.5)).abs() < 1e-3,
        "the exit floor is the real deck: {}",
        game.player_floor_y
    );
    assert!(
        game.player_position.x > 20.0,
        "the player stands past the rim: {:?}",
        game.player_position
    );
    assert!(!game.is_swimming(), "the exit left the swim state");

    // Walk away from the edge: the exit is stable, never re-entering.
    let mut walk = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..60_i32 {
        game.update_player_movement(&mut walk, &settings);
        assert!(game.grounded, "the deck is walked");
        assert!(!game.is_swimming(), "the deck never re-enters swimming");
    }
    assert!(
        game.player_position.x > 21.0,
        "the player walks away from the rim: {:?}",
        game.player_position
    );

    // Phase 2: walk back to the rim's edge, release the held Jump and leap in.
    // The leap starts above the band with deep water below, the exact case
    // that used to oscillate: the entry waits until the eye falls into the
    // band.
    game.player_yaw = 270.0_f32.to_radians();
    let mut walk_back = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..90_i32 {
        game.update_player_movement(&mut walk_back, &settings);
        if game.player_position.x <= 20.7 {
            break;
        }
    }
    assert!(
        game.player_position.x <= 20.7 && game.grounded,
        "the player walks back to the rim: {:?}",
        game.player_position
    );
    game.update_player_movement(&mut idle, &settings); // release the jump latch
    let mut leap = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut re_entered = false;
    for _ in 0_i32..240_i32 {
        game.update_player_movement(&mut leap, &settings);
        if game.is_swimming() {
            re_entered = true;
            break;
        }
    }
    assert!(re_entered, "jumping off the deck falls back into the water");
    assert!(!game.grounded);

    // Phase 3: exit again, the same way.
    let mut exited_again = false;
    for _ in 0_i32..400_i32 {
        game.update_player_movement(&mut swim_east, &settings);
        if game.grounded && game.player_floor_y > -1.6 {
            exited_again = true;
            break;
        }
    }
    assert!(
        exited_again,
        "the second exit works: floor {} at {:?}",
        game.player_floor_y, game.player_position
    );
    assert!((game.player_floor_y - (-1.5)).abs() < 1e-3);

    // Standing still on the deck never oscillates back into the water.
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut idle, &settings);
        assert!(game.grounded, "the exited player stays grounded");
        assert!(!game.is_swimming(), "no surface oscillation after the exit");
        assert_exact(game.vertical_velocity, 0.0);
    }
}

/// A pool whose rim is a real wall well above the [`WATER_EXIT_STEP_M`]
/// allowance cannot be exited: the swimmer is stopped by the solid rim, stays
/// in the deep water, and never snaps up onto the high platform.
#[test]
fn a_high_walled_pool_cannot_be_exited() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "high_walled_pool",
            "name": "High Walled Pool",
            "spawn": { "x": 1.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 5.0 } ],
            "floor_regions": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0, "offset_y": -3.0 },
                { "x": 10.0, "z": 1.0, "width": 2.0, "depth": 6.0, "offset_y": 0.5 }
            ],
            "water": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0,
                  "surface_y": -0.5, "bottom_y": -3.0 }
            ]
        }"#,
    )
    .expect("the high-walled pool parses");
    let settings = Settings::default();
    let mut game = game_for(&level);
    play_at(&mut game, 8.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut highest_eye = game.player_position.y;
    for _ in 0_i32..400_i32 {
        game.update_player_movement(&mut input, &settings);
        highest_eye = highest_eye.max(game.player_position.y);
        assert!(
            !game.grounded,
            "the high rim never becomes an exit: {:?} floor {}",
            game.player_position, game.player_floor_y
        );
        assert!(
            game.player_position.x < 10.0,
            "the wall stops the swimmer: {:?}",
            game.player_position
        );
        assert!(
            game.player_floor_y <= -2.0,
            "the swimmer stays over the basin: {}",
            game.player_floor_y
        );
    }
    assert!(game.is_swimming(), "the swimmer is still in the water");
    assert!(
        highest_eye <= -0.5 + FLOAT_EYE_MARGIN + SWIM_BOB_AMPLITUDE + 1e-3,
        "the swimmer never pops up the wall: {highest_eye}"
    );
}

/// The water-exit step-up applies only to real walkable floors: a solid prop
/// whose top is within the allowance at the water's edge is still a wall.
#[test]
fn a_solid_edge_prop_is_never_a_water_exit() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "solid_edge_pool",
            "name": "Solid Edge Pool",
            "spawn": { "x": 1.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 5.0 } ],
            "floor_regions": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0, "offset_y": -3.0 }
            ],
            "water": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0,
                  "surface_y": -0.5, "bottom_y": -3.0 }
            ],
            "props": [
                { "model": "core:crate", "x": 10.4, "z": 4.0, "y": -0.5,
                  "size": [0.8, 0.5, 3.0], "solid": true }
            ]
        }"#,
    )
    .expect("the solid-edge pool parses");
    let settings = Settings::default();
    let mut game = game_for(&level);
    play_at(&mut game, 8.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    for _ in 0_i32..300_i32 {
        game.update_player_movement(&mut input, &settings);
        assert!(
            game.player_position.x < 10.0,
            "the solid prop blocks the surface swimmer: {:?}",
            game.player_position
        );
        assert!(!game.grounded, "a solid edge is never a step-up exit");
    }
    assert!(game.is_swimming());
}

/// The water-exit step-up only applies at the surface: a deep swimmer is
/// stopped at a low ledge until they rise to it, then steps up.
#[test]
fn the_water_exit_step_up_only_applies_at_the_surface() {
    // Basin x 4..10 at -3.0; a ledge x 10..12 at -0.2, which is 0.3 above the
    // -0.5 surface (inside the allowance).
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "surface_only_exit",
            "name": "Surface Only Exit",
            "spawn": { "x": 1.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 5.0 } ],
            "floor_regions": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0, "offset_y": -3.0 },
                { "x": 10.0, "z": 1.0, "width": 2.0, "depth": 6.0, "offset_y": -0.2 }
            ],
            "water": [
                { "x": 4.0, "z": 1.0, "width": 6.0, "depth": 6.0,
                  "surface_y": -0.5, "bottom_y": -3.0 }
            ]
        }"#,
    )
    .expect("the surface-only exit pool parses");
    let settings = Settings::default();
    let mut game = game_for(&level);
    play_at(&mut game, 8.0, -3.0, 4.0, 90.0, 1.0 / 60.0);

    // Sink without Jump: the eye drops more than `EXIT_EYE_MARGIN` below the
    // surface and the low ledge stays refused.
    let mut forward = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..180_i32 {
        game.update_player_movement(&mut forward, &settings);
    }
    assert!(game.is_swimming());
    assert!(
        game.player_position.y < -0.5 - EXIT_EYE_MARGIN,
        "the swimmer is deep: {}",
        game.player_position.y
    );
    assert!(
        game.player_position.x < 10.0 && !game.grounded,
        "the deep swimmer cannot cross the ledge: {:?} floor {}",
        game.player_position,
        game.player_floor_y
    );

    // Hold Jump to rise to the surface: the same ledge is now a real exit.
    let mut surface_input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut stood = false;
    for _ in 0_i32..300_i32 {
        game.update_player_movement(&mut surface_input, &settings);
        if game.grounded {
            stood = true;
            break;
        }
    }
    assert!(
        stood,
        "the surface swimmer steps onto the ledge: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(
        (game.player_floor_y - (-0.2)).abs() < 1e-3,
        "the exit floor is the low ledge: {}",
        game.player_floor_y
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
    for _ in 0_i32..240_i32 {
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
    for _ in 0_i32..240_i32 {
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
    for _ in 0_i32..300_i32 {
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
            "format_version": 3,
            "id": "ladder_sides",
            "name": "Ladder Sides",
            "spawn": { "x": 2.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 } ],
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
    for _ in 0_i32..90_i32 {
        game.update_player_movement(&mut input, &settings);
        max_eye = max_eye.max(game.player_position.y);
        assert!(!game.is_climbing(), "the exit side never attaches");
    }
    assert!(
        max_eye <= EYE_HEIGHT + 1e-3,
        "no rise from the exit side: {max_eye}"
    );

    // Behind the centre, the same intent attaches.
    let mut behind_game = game_for(&level);
    play_at(&mut behind_game, 4.5, 0.0, 5.0, 90.0, 1.0 / 60.0);
    behind_game.update_player_movement(&mut input, &settings);
    assert!(behind_game.is_climbing(), "the approach side attaches");
}

/// An overhead obstruction stops a climb without pushing the climber down or
/// detaching them.
#[test]
fn a_ladder_obstruction_holds_the_climber_in_place() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "ladder_obstruction",
            "name": "Ladder Obstruction",
            "spawn": { "x": 2.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 } ],
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
    for _ in 0_i32..30_i32 {
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
        let mut game = demo_game_at(19.5, -3.0, 12.0, 90.0, 1.0 / 60.0);
        game.grounded = false;
        set_eye_y(&mut game, -1.55);
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
    for _ in 0_i32..30_i32 {
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
    let mut jump_game = make_attached();
    set_eye_y(&mut jump_game, -0.9);
    jump_game.vertical_velocity = 0.0;
    let before = jump_game.player_position.y;
    let mut jump = InputState::holding(&[Control::Jump]);
    jump_game.update_player_movement(&mut jump, &settings);
    assert!(!jump_game.is_climbing(), "jump releases the ladder");
    assert!(
        jump_game.player_position.y > before,
        "the jump launch rises off the ladder: {before} then {}",
        jump_game.player_position.y
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
    set_eye_y(&mut game, -2.4);
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
    for _ in 0_i32..12_i32 {
        game.update_player_movement(&mut crouch, &settings);
    }
    assert!(
        (game.feet_y() - (-1.85)).abs() < 1e-3,
        "the feet stay on the step"
    );
    assert!(
        (game.eye_offset() - CROUCH_EYE_HEIGHT).abs() < 1e-3,
        "the eased offset settles at the crouch height: {}",
        game.eye_offset()
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
            "format_version": 3,
            "id": "mid_depth",
            "name": "Mid Depth",
            "spawn": { "x": 1.0, "z": 4.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 8.0, "height": 4.0 } ],
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

    // Crouching lowers the eye into the swim band over the eased transition:
    // the pose floats once the crouched eye reaches the band.
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    assert!(game.is_crouched());
    let mut crouched_into_water = false;
    for _ in 0_i32..30_i32 {
        game.update_player_movement(&mut idle, &settings);
        if game.is_swimming() {
            crouched_into_water = true;
            break;
        }
    }
    assert!(
        crouched_into_water,
        "the crouched eye reaches the swim band: {}",
        game.player_position.y
    );

    // Standing back up recovers the wading pose on the real floor.
    let mut released = InputState::default();
    game.update_player_movement(&mut released, &settings);
    game.update_player_movement(&mut crouch, &settings);
    assert_eq!(game.stance(), Stance::Standing);
    for _ in 0_i32..30_i32 {
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
    set_eye_y(&mut game, -0.8);
    game.swimming = false;
    let mut idle = InputState::default();
    let mut deepest = game.player_position.y;
    for _ in 0_i32..30_i32 {
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
    set_eye_y(&mut game, -1.65 + FLOAT_EYE_MARGIN);
    game.swimming = true;
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut min_eye = game.player_position.y;
    for _ in 0_i32..240_i32 {
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

/// The Pit's source: the local drop-in when installed, else the tracked
/// fixture, so the normal suite stays hermetic on a fresh checkout.
fn pit_level_source() -> String {
    std::fs::read_to_string("levels/level0_pit.json")
        .or_else(|_| std::fs::read_to_string("tests/fixtures/levels/level0_pit.json"))
        .expect("The Pit source or its tracked fixture is present")
}

/// The Pit's real carpet holes are 3.2 m deep recesses: walking in loses
/// support and falls, and the hole's reset trigger returns the player to the
/// authored spawn instead of leaving them at the bottom. The full 15-hole
/// sweep and the safe carpet between holes are covered by
/// `the_pit_carpet_holes_reset_and_the_carpet_is_safe`; this is the original
/// single-hole walk-in regression, updated for the run-2 reset trigger.
#[test]
fn the_pit_carpet_hole_is_a_real_fall() {
    let content = pit_level_source();
    let level = LevelDef::from_json(&content).expect("The Pit parses");
    let mut game = game_for(&level);
    let spawn = spawn_position(&level);
    // Walk south from the hall floor into the first 1.6 m hole (x 9.6..11.2,
    // z -23.0..-21.4), 3.2 m below the hall.
    play_at(&mut game, 10.4, 0.0, -20.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut airborne = false;
    for _ in 0_i32..180_i32 {
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

    // Mid-jump: the crouch eases the eye down but keeps the feet on their
    // ballistic line, and the landing is still the rendered tread.
    let mut airborne_game = game_for(&level);
    play_at(&mut airborne_game, x, pitch, z, 90.0, 1.0 / 60.0);
    let mut released = InputState::default();
    let mut jump = InputState::holding(&[Control::Jump]);
    airborne_game.update_player_movement(&mut jump, &settings);
    assert!(!airborne_game.grounded);
    let feet_before = airborne_game.feet_y();
    let eye_before = airborne_game.player_position.y;
    airborne_game.update_player_movement(&mut pressed, &settings);
    assert!(airborne_game.is_crouched());
    assert!(
        airborne_game.feet_y() > feet_before && airborne_game.feet_y() < feet_before + 0.1,
        "the airborne feet keep rising on their ballistic line: {} vs {feet_before}",
        airborne_game.feet_y()
    );
    assert!(
        airborne_game.player_position.y < eye_before,
        "the eye eases down with the stance, not the body: {} vs {eye_before}",
        airborne_game.player_position.y
    );
    assert!(
        airborne_game.eye_offset() < EYE_HEIGHT && airborne_game.eye_offset() > CROUCH_EYE_HEIGHT,
        "the eye offset is mid-transition, not snapped: {}",
        airborne_game.eye_offset()
    );
    // The eased offset settles at the crouch height while the feet keep the
    // ballistic line; the rendered eye never leaves the invariant.
    for _ in 0_i32..12_i32 {
        airborne_game.update_player_movement(&mut released, &settings);
        assert_exact(
            airborne_game.player_position.y,
            airborne_game.feet_y() + airborne_game.eye_offset(),
        );
    }
    assert_exact(airborne_game.eye_offset(), CROUCH_EYE_HEIGHT);
    for _ in 0_i32..120_i32 {
        airborne_game.update_player_movement(&mut released, &settings);
        if airborne_game.grounded {
            break;
        }
    }
    assert!(airborne_game.grounded, "the crouched jump still lands");

    // On the demo ladder: crouching anchors the feet and keeps the attachment.
    let mut ladder_game = demo_game_at(19.5, -3.0, 12.0, 90.0, 1.0 / 60.0);
    ladder_game.grounded = false;
    set_eye_y(&mut ladder_game, -1.9);
    ladder_game.swimming = false;
    let mut forward = InputState::holding(&[Control::MoveForward]);
    ladder_game.update_player_movement(&mut forward, &settings);
    assert!(ladder_game.is_climbing(), "the fixture attaches");
    let ladder_feet = ladder_game.feet_y();
    let mut ladder_release = InputState::default();
    ladder_game.update_player_movement(&mut ladder_release, &settings);
    ladder_game.update_player_movement(&mut pressed, &settings);
    assert!(ladder_game.is_crouched());
    assert!(ladder_game.is_climbing(), "the crouch keeps the attachment");
    assert!(
        (ladder_game.feet_y() - ladder_feet).abs() < 1e-4,
        "the ladder feet stay anchored: {} vs {ladder_feet}",
        ladder_game.feet_y()
    );
    assert!((ladder_game.body_height() - CROUCH_HEIGHT).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// interactions, labels and area triggers
// ---------------------------------------------------------------------------

/// A 20x20 room with two aimable plants on one clear line of sight, plus an
/// optional low beam occluder and optional authored volumes.
fn interaction_level(props_json: &str, walls_json: &str, volumes_json: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "interaction_test",
            "name": "Interaction Test",
            "spawn": {{ "x": 2.0, "z": 5.0, "yaw_degrees": 0.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }} ],
            "walls": {walls_json},
            "props": {props_json},
            "volumes": {volumes_json}
        }}"#
    ))
    .expect("valid interaction test json")
}

/// Two same-model plants on one clear line of sight: one 1.4 m in front of the
/// spawn, one 2.7 m out, both tall enough to intersect a flat view ray.
fn two_plant_level(walls_json: &str, volumes_json: &str) -> LevelDef {
    interaction_level(
        r#"[
            { "id": "near_plant", "display_name": "Near Plant", "model": "core:plant",
              "x": 3.4, "z": 5.0, "size": [0.6, 1.8, 0.6],
              "components": [ { "component": "interactable", "prompt": "Toggle name" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label" }] } ] },
            { "id": "far_plant", "display_name": "Far Plant", "model": "core:plant",
              "x": 4.7, "z": 5.0, "size": [0.6, 1.8, 0.6],
              "components": [ { "component": "interactable", "prompt": "Toggle name" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label" }] } ] }
        ]"#,
        walls_json,
        volumes_json,
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
    assert_eq!(report.actions_run, 1, "the interact binding ran once");
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
    let rearmed_report = game.dispatch_interaction().expect("a target");
    assert_eq!(
        rearmed_report.actions_run, 1,
        "one press is one binding fire"
    );
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
    let report = game.dispatch_interaction().expect("the near plant");
    assert_eq!(report.actions_run, 1);
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
    let far_report = game.dispatch_interaction().expect("the far plant");
    assert_eq!(far_report.actions_run, 1);
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
    let mut level = interaction_level(
        r#"[
            { "id": "far_plant", "display_name": "Far Plant", "model": "core:plant",
              "x": 4.6, "z": 5.0,
              "components": [ { "component": "interactable" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label" }] } ] }
        ]"#,
        "[]",
        "[]",
    );
    level.props.push(crate::level::PropDef {
        id: Some("beam".into()),
        display_name: None,
        model: "core:beam".into(),
        x: 3.2,
        y: 1.0,
        z: 5.0,
        rotation_degrees: 0.0,
        scale: 1.0,
        size: Some([0.8, 0.4, 0.8]),
        solid: true,
        occludes: true,
        components: Vec::new(),
        bindings: Vec::new(),
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

    // Crouch to completion, and the same aim from the lower eye passes under
    // the beam.
    let settings = Settings::default();
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    let mut released = InputState::default();
    for _ in 0_i32..12_i32 {
        game.update_player_movement(&mut released, &settings);
    }
    assert!(game.is_crouched());
    assert!(
        (game.eye_offset() - CROUCH_EYE_HEIGHT).abs() < 1e-6,
        "the aim origin is the settled crouched eye"
    );
    assert!(
        (game.player_position.y - (game.feet_y() + CROUCH_EYE_HEIGHT)).abs() < 1e-6,
        "the rendered eye sits the crouched offset above the feet"
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
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    aim_at(&mut game, 3.4, 5.0, 0.9);
    let report = game.dispatch_interaction().expect("the near plant");
    assert_eq!(report.actions_run, 1);
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

    let report = game
        .entities_mut()
        .dispatch_interaction(Some(north))
        .expect("north fires");
    assert_eq!(report.actions_run, 1);
    assert!(game.is_label_visible(north));
    assert!(!game.is_label_visible(south));
    assert!(!game.is_label_visible(spooner));

    let _ = game.entities_mut().dispatch_interaction(Some(south));
    let _ = game.entities_mut().dispatch_interaction(Some(spooner));
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
              "components": [ { "component": "interactable" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label" }] } ] }
        ]"#,
        "[]",
        r#"[
            { "id": "cooldown_pad", "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "bindings": [ { "on": "enter_volume", "cooldown_seconds": 1.0,
                              "actions": [{ "action": "toggle_label",
                                            "target": "pad_plant" }] } ] },
            { "id": "once_pad", "x": 8.0, "z": 12.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "bindings": [ { "on": "enter_volume", "once": true,
                              "actions": [{ "action": "toggle_label",
                                            "target": "pad_plant" }] } ] }
        ]"#,
    );
    let mut game = game_for(&level);
    assert_eq!(game.volume_count(), 2);
    let plant = game.interactables().index_of("pad_plant").expect("plant");

    // Standing inside the cooldown pad does not repeat, even as time passes.
    play_at(&mut game, 5.0, 0.0, 5.0, 0.0, 1.0 / 60.0);
    for _ in 0_i32..30_i32 {
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
    for _ in 0_i32..70_i32 {
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
    let once_enter = |subject: &mut Game| {
        play_at(subject, 9.0, 0.0, 13.0, 0.0, 1.0 / 60.0);
        subject.update_player_movement(&mut InputState::default(), &Settings::default());
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
              "components": [ { "component": "interactable" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label" }] } ] }
        ]"#,
        "[]",
        r#"[
            { "id": "band", "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 0.1,
              "bindings": [ { "on": "enter_volume",
                              "actions": [{ "action": "toggle_label",
                                            "target": "band_plant" }] } ] }
        ]"#,
    );
    let mut game = game_for(&level);
    let plant = game.interactables().index_of("band_plant").expect("plant");

    // One MAX_SIM_DELTA frame at 25 m/s: the feet move from 1.0 m to below the
    // 10 cm band in a single update.
    game.set_app_state(AppState::Playing);
    set_eye_position(&mut game, Vec3::new(5.0, 1.0 + EYE_HEIGHT, 5.0));
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
    let mut level = interaction_level(
        "[]",
        "[]",
        r#"[
            { "id": "spawn_pad", "x": 0.5, "z": 0.5, "width": 1.0, "depth": 1.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "bindings": [ { "on": "enter_volume",
                              "actions": [{ "action": "reset_to_start" }] } ] },
            { "id": "far_pad", "x": 8.0, "z": 8.0, "width": 2.0, "depth": 2.0,
              "bottom_y": 0.0, "top_y": 1.5,
              "bindings": [ { "on": "enter_volume",
                              "actions": [{ "action": "reset_to_start" }] } ] }
        ]"#,
    );
    // Move the spawn into the first pad's volume: the enter semantics must not
    // fire on load, and a reset must not fire it either.
    level.spawn.x = 1.0;
    level.spawn.z = 1.0;
    let mut game = game_for(&level);
    let spawn = game.spawn_position();
    let settings = Settings::default();

    for _ in 0_i32..10_i32 {
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
    let lifted_eye = game.player_position.y + 0.3;
    set_eye_y(&mut game, lifted_eye);
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
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    assert_eq!(
        game.reset_count(),
        1,
        "the spawn volume never re-fires while the player stands in it"
    );

    // The teleport did not sweep the pads between spawn and the far pad: the
    // only fire was the far pad itself.
    assert_eq!(game.volume_count(), 2);
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
              "bindings": [ { "on": "enter_volume",
                              "actions": [{ "action": "reset_to_start" }] } ] }
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
    for _ in 0_i32..10_i32 {
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

    // A `set_light` on an entity that carries no light is reported as
    // unsupported instead of being silently applied or silently dropped.
    let unsupported = [ActionDef::SetLight {
        target: Some("near_plant".into()),
        on: false,
    }];
    let report = game.dispatch_actions(&unsupported, None);
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.actions_run, 0);

    let missing = [ActionDef::ToggleLabel {
        target: Some("ghost".into()),
    }];
    let missing_report = game.dispatch_actions(&missing, None);
    assert_eq!(missing_report.missing_targets, 1);

    // An explicit target addresses another instance, independently of the
    // acting one: the near plant's interaction toggles the far plant.
    let cross = [ActionDef::ToggleLabel {
        target: Some("far_plant".into()),
    }];
    let cross_report = game.dispatch_actions(&cross, Some(0));
    assert_eq!(cross_report.labels_shown, 1);
    assert!(game.is_label_visible(1));
    assert!(
        !game.is_label_visible(0),
        "the actor's own label is untouched"
    );
    let _dispatch_report = game.dispatch_actions(&cross, Some(0));

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
    let unresolved_report = game.dispatch_actions(&unresolved, Some(0));
    assert_eq!(unresolved_report.missing_targets, 2);
    assert_eq!(unresolved_report.labels_toggled(), 0);
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
    let reset_report = game.dispatch_actions(&batch, Some(0));
    assert_eq!(reset_report.actions_run, 2);
    assert!(reset_report.player_reset);
    assert_eq!(reset_report.labels_shown, 1);
    assert_eq!(reset_report.labels_hidden, 0);
    assert!(
        !game.is_label_visible(1),
        "the action after the reset was deferred"
    );

    // Programmatically oversized batches are truncated to the bound (validation
    // rejects them for real maps): 10 toggles run 8.
    let oversized: Vec<ActionDef> = (0_i32..10_i32)
        .map(|_| ActionDef::ToggleLabel {
            target: Some("near_plant".into()),
        })
        .collect();
    let oversized_report = game.dispatch_actions(&oversized, None);
    assert_eq!(
        oversized_report.actions_run,
        crate::level::MAX_ACTIONS_PER_SOURCE,
        "the dispatcher never runs more than the bound"
    );
}

/// Every intended carpet hole in The Pit resets the player, and the carpet
/// between the holes stays safe to walk on.
#[test]
fn the_pit_carpet_holes_reset_and_the_carpet_is_safe() {
    let content = pit_level_source();
    let level = LevelDef::from_json(&content).expect("The Pit parses");
    validate_pit_level(&level);
    // The volume entities carry the hole triggers; each has an `enter_volume`
    // binding whose actions reset the player.
    let world = crate::entities::EntityWorld::from_level(&level);
    let volumes: Vec<(String, [f32; 4])> = world
        .components()
        .volumes
        .iter()
        .map(|(handle, volume)| {
            (
                world
                    .id_of(handle)
                    .expect("every volume has an authored id")
                    .to_string(),
                volume.bounds,
            )
        })
        .collect();
    assert_eq!(
        volumes.len(),
        17,
        "one trigger per hole (15) plus the two Pit-gate triggers"
    );
    let ids: Vec<&str> = volumes.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"pit_hole_1"));
    assert!(ids.contains(&"pit_hole_15"));
    assert!(ids.contains(&"pit_gate_approach"));
    assert!(ids.contains(&"pit_gate_passed"));

    let settings = Settings::default();
    let spawn = spawn_position(&level);
    let spawn_yaw = level.spawn.yaw_degrees.to_radians();
    for (id, bounds) in volumes.iter().filter(|(id, _)| id.starts_with("pit_hole_")) {
        let mut game = Game::new(spawn, spawn_yaw, CollisionWorld::from_level(&level));
        let cx = f32::midpoint(bounds[0], bounds[1]);
        let cz = f32::midpoint(bounds[2], bounds[3]);
        // Start on the carpet 0.8 m north of the hole and walk south into it.
        play_at(&mut game, cx, 0.0, cz - 0.8, 180.0, 1.0 / 60.0);
        let mut input = InputState::holding(&[Control::MoveForward]);
        let mut reset = false;
        for _ in 0_i32..180_i32 {
            game.update_player_movement(&mut input, &settings);
            if game.reset_count() >= 1 {
                reset = true;
                break;
            }
        }
        assert!(reset, "walking into {id} resets the player");
        assert!(
            (game.player_position.x - spawn.x).abs() < 1e-4
                && (game.player_position.z - spawn.z).abs() < 1e-4,
            "{id} returns the player to the authored spawn"
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
    for _ in 0_i32..300_i32 {
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
    let first = volumes.first().expect("the first hole");
    for expected in 1..=2 {
        let cx = f32::midpoint(first.1[0], first.1[1]);
        let cz = f32::midpoint(first.1[2], first.1[3]);
        play_at(&mut repeat, cx, 0.0, cz - 0.8, 180.0, 1.0 / 60.0);
        let mut walk = InputState::holding(&[Control::MoveForward]);
        let mut settled = false;
        for _ in 0_i32..180_i32 {
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
              "x": 8.0, "z": 8.0,
              "components": [ { "component": "interactable", "enabled": false } ] },
            { "id": "switch", "display_name": "Switch", "model": "core:switch",
              "x": 4.0, "z": 5.0,
              "components": [ { "component": "interactable", "prompt": "Switch" } ],
              "bindings": [ { "on": "interact",
                              "actions": [{ "action": "toggle_label",
                                            "target": "lamp" }] } ] }
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
        !game.interactables().get(lamp).expect("lamp").enabled,
        "a label-only target is never aimable"
    );
    // Aiming at the lamp finds nothing: the disabled instance is skipped.
    play_at(&mut game, 7.0, 0.0, 8.0, 0.0, 1.0 / 60.0);
    aim_at(&mut game, 8.0, 8.0, 0.5);
    assert_eq!(
        game.interaction_target(),
        None,
        "the label-only instance is not a target"
    );

    let _ = game.entities_mut().dispatch_interaction(Some(switch));
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
            "format_version": 3,
            "id": "swim_stance_trigger",
            "name": "Swim Stance Trigger",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0,
                      "floor_y": -2.0 } ],
            "water": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "surface_y": 0.0, "bottom_y": -2.0 }
            ],
            "props": [
                { "id": "band_plant", "display_name": "Band Plant", "model": "core:plant",
                  "x": 1.0, "z": 1.0,
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle_label" }] } ] }
            ],
            "volumes": [
                { "id": "band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": -1.5, "top_y": -1.0,
                  "bindings": [ { "on": "enter_volume",
                                  "actions": [{ "action": "toggle_label",
                                                "target": "band_plant" }] } ] }
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
    set_eye_position(&mut game, Vec3::new(4.0, 0.0, 4.0));
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

/// When one frame crosses two trigger volumes, both crossings dispatch in
/// that frame: each band fires exactly once, and neither is lost to the other.
#[test]
fn a_swept_crossing_of_two_trigger_bands_fires_each_exactly_once() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "stacked_triggers",
            "name": "Stacked Triggers",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 6.0,
                      "floor_y": -3.0 } ],
            "props": [
                { "id": "upper", "display_name": "Upper", "model": "core:plant",
                  "x": 1.0, "z": 1.0,
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle_label" }] } ] },
                { "id": "lower", "display_name": "Lower", "model": "core:plant",
                  "x": 2.0, "z": 1.0,
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle_label" }] } ] }
            ],
            "volumes": [
                { "id": "upper_band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": 0.0, "top_y": 0.2,
                  "bindings": [ { "on": "enter_volume",
                                  "actions": [{ "action": "toggle_label",
                                                "target": "upper" }] } ] },
                { "id": "lower_band", "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "bottom_y": -1.0, "top_y": -0.8,
                  "bindings": [ { "on": "enter_volume",
                                  "actions": [{ "action": "toggle_label",
                                                "target": "lower" }] } ] }
            ]
        }"#,
    )
    .expect("the stacked trigger level parses");
    let mut game = game_for(&level);
    let upper = game.interactables().index_of("upper").expect("upper");
    let lower = game.interactables().index_of("lower").expect("lower");

    // One MAX_SIM_DELTA frame at 25 m/s crosses both bands.
    game.set_app_state(AppState::Playing);
    set_eye_position(&mut game, Vec3::new(4.0, 1.0 + EYE_HEIGHT, 4.0));
    game.player_floor_y = -3.0;
    game.grounded = false;
    game.vertical_velocity = -25.0;
    game.sim_delta_seconds = MAX_SIM_DELTA;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert_eq!(
        game.last_world_tick().events_processed,
        2,
        "both swept crossings became enter edges"
    );
    assert!(
        game.is_label_visible(upper),
        "the first band fires this frame"
    );
    assert!(
        game.is_label_visible(lower),
        "the later band's crossing is dispatched in the same sweep, never dropped"
    );

    // The player has already fallen past both bands: neither re-fires.
    game.sim_delta_seconds = 1.0 / 60.0;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.is_label_visible(upper) && game.is_label_visible(lower),
        "each band's crossing fired exactly once"
    );
}

/// A level with one routed, interactable, non-solid entity in a walled room:
/// the entity walks from (2, 2) toward (4, 2), the wall at x = 5..5.2 blocks
/// the far side.
fn routed_entity_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "routed_entity",
            "name": "Routed Entity",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
            "walls": [
                { "x": 5.0, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.5 }
            ],
            "props": [
                { "id": "walker", "display_name": "Walker", "model": "entity:x",
                  "x": 2.0, "z": 2.0, "size": [0.4, 0.5, 0.4],
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle_label" }] } ] }
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
    for _ in 0_i32..45_i32 {
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
            PoseCue::Idle
                | PoseCue::Walk { speed_mps: _ }
                | PoseCue::Clip {
                    name: _,
                    once: _,
                    paused: _
                }
        ),
        "a supported cue: {:?}",
        frame.cue
    );
}

#[test]
fn a_route_never_walks_the_entity_through_a_wall() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "blocked_route",
            "name": "Blocked Route",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
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
    for _ in 0_i32..100_i32 {
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

/// A fade-only entity authors no route and no AI, so its frame has no
/// transform; the opacity must still change between two update ticks.
#[test]
fn a_fading_entity_changes_opacity_between_ticks() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "fading_ghost",
            "name": "Fading Ghost",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
            "props": [
                { "id": "ghost", "model": "entity:sheet-ghost", "x": 2.0, "z": 2.0,
                  "components": [ { "component": "fade", "period_seconds": 2.0,
                                    "phase": 0.0, "min_opacity": 0.0,
                                    "max_opacity": 1.0 } ] }
            ]
        }"#,
    )
    .expect("the fading level parses");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let settings = Settings::default();
    let mut input = InputState::default();

    let opacity = |subject: &Game| -> f32 {
        let frame = subject
            .entity_frames()
            .iter()
            .find(|frame| frame.instance_id == "ghost")
            .expect("the fade-only ghost has a frame");
        assert!(frame.transform.is_none(), "nothing moves the ghost");
        frame.opacity
    };
    let first = opacity(&game);
    game.update_player_movement(&mut input, &settings);
    let second = opacity(&game);
    game.update_player_movement(&mut input, &settings);
    let third = opacity(&game);
    assert!(
        (second - first).abs() > 1e-4,
        "the fade advanced between ticks: {first} -> {second}"
    );
    assert!(
        (third - second).abs() > 1e-4,
        "and keeps advancing: {second} -> {third}"
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
    let _dispatch_report = game.dispatch_actions(&looping, None);
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
    let missing_clip_report = game.dispatch_actions(&ghost, Some(index));
    assert_eq!(missing_clip_report.missing_targets, 1);
    assert_eq!(missing_clip_report.animations_started, 0);

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
    for _ in 0_i32..40_i32 {
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
            PoseCue::Clip { ref name, once: true, paused: _ } if name == "pose_arms_forward"
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
        .find(|seated_skeleton| seated_skeleton.instance_id == "skeleton_chair")
        .expect("the seated skeleton has a frame");
    assert!(
        matches!(
            seated.cue,
            PoseCue::Clip { ref name, once: _, paused: _ } if name == "pose_stand"
        ),
        "the chair cycle moved to standing: {:?}",
        seated.cue
    );

    // Its interactable resets the pose exactly like the map authors it.
    let index = game
        .interactables()
        .index_of("mannequin_pose")
        .expect("the mannequin is interactable");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(index))
        .expect("the mannequin fires");
    assert_eq!(report.actions_run, 1);
    assert_eq!(
        game.animation_override("mannequin_pose"),
        Some(&PoseCue::Clip {
            name: "pose_stand".into(),
            once: true,
            paused: false
        })
    );
}

/// The Halloween fixture runs all three entities through the shared
/// components: the pumpkin hops its walkway on a route, the ghosts float
/// bounded routes while fading on independently phased cycles with a
/// fade-coupled glow, and the pumpkin-head skeleton wanders the tree zone
/// through the baked navigation.
#[test]
fn the_halloween_fixture_runs_jumping_pumpkins_fading_ghosts_and_wandering_skeletons() {
    let content = std::fs::read_to_string("tests/fixtures/levels/halloween_entities.json")
        .expect("the Halloween fixture is present");
    let level = LevelDef::from_json(&content).expect("the Halloween fixture parses");
    crate::loader::validate_level(&level).expect("the Halloween fixture validates");
    let mut game = game_for(&level);
    assert_eq!(
        game.routes().len(),
        5,
        "the pumpkin, three ghosts and the ghost cat each carry a route"
    );

    game.set_app_state(AppState::Playing);
    let settings = Settings::default();
    game.sim_delta_seconds = MAX_SIM_DELTA;
    for _ in 0_i32..40_i32 {
        game.update_player_movement(&mut InputState::default(), &settings);
    }

    // The pumpkin hops north along the walkway at its authored 0.5 m/s; four
    // seconds of frames move it 2 m from its spawn at z = -2.5 with no
    // lateral drift.
    let pumpkin = game
        .route_state("pumpkin_hopper")
        .expect("the pumpkin route resolves")
        .position;
    assert!(
        (pumpkin.z + 0.5).abs() < 0.15 && pumpkin.x.abs() < 0.05,
        "the pumpkin hopped the walkway: {pumpkin:?}"
    );
    // The ghost floats its route at its authored 0.3 m/s float speed.
    let ghost = game
        .route_state("ghost_a")
        .expect("the ghost route resolves")
        .position;
    assert!(
        (ghost.x - 6.2).abs() < 0.15 && (ghost.z + 8.0).abs() < 0.05,
        "the ghost floated its route: {ghost:?}"
    );

    // The frames carry the attachment contracts the renderer needs.
    let frames = game.entity_frames();
    let pumpkin_frame = frames
        .iter()
        .find(|frame| frame.instance_id == "pumpkin_hopper")
        .expect("the pumpkin has a frame");
    let glow = pumpkin_frame
        .glow
        .as_ref()
        .expect("the pumpkin carries a glow cue");
    assert_eq!(glow.socket.as_deref(), Some("flame"));
    assert!((pumpkin_frame.opacity - 1.0).abs() < f32::EPSILON);

    let ghost_frame = frames
        .iter()
        .find(|frame| frame.instance_id == "ghost_a")
        .expect("the ghost has a frame");
    let ghost_glow = ghost_frame
        .glow
        .as_ref()
        .expect("the ghost carries a glow cue");
    assert_eq!(ghost_glow.socket.as_deref(), Some("body"));
    assert!(ghost_glow.fade_with_opacity, "the glow follows the fade");
    let before = ghost_frame.opacity;
    assert!(
        (0.05..=0.85).contains(&before),
        "the authored opacity cycle bounds hold: {before}"
    );

    // Half of ghost_a's 7 s cycle later the opacity has crossed to the other
    // half: the fade is a real cycle, not a static value.
    game.sim_delta_seconds = 3.5;
    game.update_player_movement(&mut InputState::default(), &settings);
    let after = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "ghost_a")
        .expect("the ghost still has a frame")
        .opacity;
    assert!(
        (after - before).abs() > 0.3,
        "the ghost fades on its authored cycle: {before} -> {after}"
    );
    assert!(
        (0.05..=0.85).contains(&after),
        "the second half stays inside the bounds: {after}"
    );

    // The skeleton wanders through the AI runtime with a real transform.
    let walker = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "pumpkin_walker")
        .expect("the pumpkin-head skeleton has an AI frame");
    let (position, yaw) = walker.transform.expect("the skeleton has a transform");
    assert!(
        position.is_finite() && yaw.is_finite(),
        "the wandering skeleton stays finite: {position:?}"
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
    for _ in 0_i32..20_i32 {
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
            "format_version": 3,
            "id": "cue_only_target",
            "name": "Cue Only Target",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
            "props": [
                { "id": "actor", "model": "core:crate", "x": 2.0, "z": 2.0,
                  "size": [0.4, 0.4, 0.4],
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "play_animation", "target": "dummy",
                        "clip": "pose_sit_chair" } ] } ] },
                { "id": "dummy", "model": "skeleton", "x": 4.0, "z": 2.0,
                  "size": [0.42, 1.7, 0.24],
                  "components": [ { "component": "animation",
                                    "clip": "pose_sit_chair" } ] }
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
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(actor))
        .expect("the actor fires");
    assert_eq!(report.actions_run, 1);
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
            "format_version": 3,
            "id": "turning_route",
            "name": "Turning Route",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
            "props": [
                { "id": "turner", "display_name": "Turner", "model": "entity:x",
                  "x": 4.0, "z": 2.0, "rotation_degrees": 0.0,
                  "size": [0.2, 0.5, 0.8],
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle_label" }] } ] }
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
    for _ in 0_i32..10_i32 {
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
            "format_version": 3,
            "id": "switch_test",
            "name": "Switch Test",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "switch_a", "display_name": "Switch A", "model": "home:wall_switch",
                  "x": 2.0, "z": 0.2, "size": [0.18, 0.18, 0.1],
                  "components": [ { "component": "interactable", "prompt": "Switch" },
                                  { "component": "animation", "clip": "toggle",
                                    "looped": false, "playing": false } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "toggle_animation", "clip": "toggle" },
                      { "action": "toggle_label" } ] } ] },
                { "id": "switch_b", "display_name": "Switch B", "model": "home:wall_switch",
                  "x": 4.0, "z": 0.2, "size": [0.18, 0.18, 0.1],
                  "components": [ { "component": "interactable", "prompt": "Switch" },
                                  { "component": "animation", "clip": "toggle",
                                    "looped": false, "playing": false } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "toggle_animation", "clip": "toggle" },
                      { "action": "toggle_label" } ] } ] }
            ]
        }"#,
    )
    .expect("the switch level parses")
}

/// one press flips exactly one switch's lever target, composes with the
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
    // An unrouted switch's cue reaches the renderer through a cue-only frame:
    // the renderer matches frames to the characters it claimed, so a rigid
    // animated prop needs a frame exactly like a routed character does.
    let switch_frame = game
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "switch_a")
        .expect("the cued switch has a frame");
    assert!(
        switch_frame.transform.is_none(),
        "the cue-only frame carries no transform"
    );
    assert!(
        game.entity_frames()
            .iter()
            .all(|frame| frame.instance_id != "switch_b"),
        "switch B has no cue and therefore no frame"
    );
    assert!(
        game.animation_override("switch_b").is_none(),
        "switch B has no cue at all"
    );

    // A second press flips the target back toward the rest end rather than
    // restarting at it; the label toggles off again.
    let second_report = game.dispatch_actions(
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
    assert_eq!(second_report.labels_hidden, 1);
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
    let _dispatch_report = game.dispatch_actions(
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

/// a toggle needs a clip name and a target that resolves on its own.
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
    let missing_target_report = game.dispatch_actions(
        &[ActionDef::ToggleAnimation {
            target: Some("switch_a".into()),
            clip: Some("   ".into()),
        }],
        None,
    );
    assert_eq!(
        missing_target_report.unsupported, 1,
        "a blank clip is unsupported"
    );
    assert_eq!(game.animation_override("switch_a"), None);
}

/// A reset returns every switch to its authored rest pose: both a toggle
/// scrub and a playing animation override are cleared.
#[test]
fn reset_clears_toggle_scrubs_and_playing_animations() {
    let level = switch_level();
    let mut game = game_for(&level);
    let index = game
        .interactables()
        .index_of("switch_a")
        .expect("switch A is interactable");
    let _dispatch_report = game.dispatch_actions(
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
        None,
        "a reset clears the toggle override, returning the prop to its authored start"
    );
    let _dispatch_report_2 = game.dispatch_actions(
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

#[test]
fn demo_spoonerman_completes_six_seated_destinations_without_blocking() {
    let level = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("demo parses");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = MAX_SIM_DELTA;
    let settings = Settings::default();
    let mut visited = std::collections::HashSet::new();
    let mut destinations = Vec::new();
    let mut previous = 0;
    let mut completed = false;
    for _ in 0_i32..30_000_i32 {
        game.update_player_movement(&mut InputState::default(), &settings);
        let state = game.route_state("spooner_man").expect("demo route");
        assert!(
            !state.blocked,
            "blocked at step {}: {:?}",
            state.step, state.position
        );
        if state.cue.clip_name() == Some("sit_idle") && visited.insert(state.step) {
            destinations.push(state.position);
        }
        if state.step < previous {
            completed = true;
            break;
        }
        previous = state.step;
    }
    assert!(completed, "route did not loop; reached step {previous}");
    assert_eq!(visited.len(), 6, "six distinct seated destinations");
    for (x, z) in [
        (5.6, 8.4),
        (13.0, 3.6),
        (4.0, 3.6),
        (29.5, 13.2),
        (56.0, 6.5),
        (60.5, 12.5),
    ] {
        assert!(
            destinations
                .iter()
                .any(|point| (point.x - x).abs() < 0.05 && (point.z - z).abs() < 0.05),
            "missing seated destination ({x}, {z})"
        );
    }
}

#[test]
fn gameplay_consumes_fast_taps_once_without_a_held_frame() {
    use crate::input::InputHandler;
    use sdl3::event::Event;
    use sdl3::keyboard::{Keycode, Mod};

    fn tap(handler: &mut InputHandler, key: Keycode, settings: &Settings) {
        for event in [
            Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(key),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            },
            Event::KeyUp {
                timestamp: 0,
                window_id: 0,
                keycode: Some(key),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            },
        ] {
            handler.handle_gameplay_event(&event, &settings.bindings);
        }
    }

    let mut game = game_for(&step_rule_level());
    play_at(&mut game, 1.0, 0.0, 4.0, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut handler = InputHandler::new();
    for crouched in [true, false] {
        tap(&mut handler, Keycode::C, &settings);
        game.update_player_movement(handler.state_mut(), &settings);
        assert_eq!(game.is_crouched(), crouched);
        game.update_player_movement(handler.state_mut(), &settings);
        assert_eq!(game.is_crouched(), crouched, "one tap is one toggle");
    }
    for _ in 0_i32..2_i32 {
        tap(&mut handler, Keycode::E, &settings);
        game.update_player_movement(handler.state_mut(), &settings);
        assert!(game.take_interact_press());
        game.update_player_movement(handler.state_mut(), &settings);
        assert!(!game.take_interact_press());
    }
    game.set_app_state(AppState::Paused);
    tap(&mut handler, Keycode::E, &settings);
    game.update_player_movement(handler.state_mut(), &settings);
    game.set_app_state(AppState::Playing);
    game.update_player_movement(handler.state_mut(), &settings);
    assert!(
        !game.take_interact_press(),
        "paused taps cannot leak into play"
    );
    game.sim_delta_seconds = 1.0 / 60.0;
    tap(&mut handler, Keycode::Space, &settings);
    game.update_player_movement(handler.state_mut(), &settings);
    assert!(game.vertical_velocity > 0.0, "a short jump tap still jumps");
    assert!(!game.grounded);
}

/// A switch can toggle a switchable light fixture through the same declarative
/// `toggle` action that drives a door, and the renderer's hand-off reports the
/// change exactly once.
#[test]
#[expect(clippy::too_many_lines, reason = "one cohesive end-to-end scenario")] // one cohesive end-to-end scenario
fn a_switch_toggles_a_switchable_light_fixture() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "switch_light",
            "name": "Switch Light",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "ceiling_lights": [
                { "id": "room_light", "fixture": "core:fluorescent_panel_01",
                  "x": 4.0, "z": 4.0, "switchable": true },
                { "id": "always_on", "fixture": "core:fluorescent_panel_01",
                  "x": 6.0, "z": 6.0 }
            ],
            "props": [
                { "id": "room_switch", "model": "home:wall_switch", "x": 4.0, "y": 1.2, "z": 1.0,
                  "solid": false,
                  "components": [ { "component": "interactable", "prompt": "Switch" } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "toggle", "target": "room_light" }
                  ] } ] }
            ]
        }"#,
    )
    .expect("the switch level parses");
    crate::loader::validate_level(&level).expect("the switch level validates");

    let mut game = game_for(&level);
    let switch = game
        .interactables()
        .index_of("room_switch")
        .expect("the switch is aimable");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(switch))
        .expect("the switch fires");
    assert_eq!(report.actions_run, 1, "one toggle action ran");
    let light = game
        .entities()
        .handle_of("room_light")
        .expect("the fixture entity");
    assert!(
        !game
            .entities()
            .components()
            .lights
            .get(light)
            .expect("the light component")
            .enabled,
        "the switchable fixture flipped off"
    );
    assert_eq!(
        game.take_light_toggles(),
        vec![(0, false)],
        "the change is handed to the renderer exactly once"
    );
    assert!(game.take_light_toggles().is_empty(), "and never repeats");

    // A second press flips it back on.
    let second_report = game
        .entities_mut()
        .dispatch_interaction(Some(switch))
        .expect("the switch fires again");
    assert_eq!(second_report.actions_run, 1);
    assert_eq!(game.take_light_toggles(), vec![(0, true)]);

    // A `toggle` that names a fixture which never switches is rejected twice:
    // the loader refuses the map, and a hand-built world (bypassing
    // validation) still refuses at runtime rather than reporting a pretend
    // change.
    let bad = r#"{
        "format_version": 3,
        "id": "bad_switch",
        "name": "Bad Switch",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
        "ceiling_lights": [
            { "id": "always_on", "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
        ],
        "props": [
            { "id": "room_switch", "model": "home:wall_switch", "x": 4.0, "y": 1.2, "z": 1.0,
              "solid": false,
              "components": [ { "component": "interactable" } ],
              "bindings": [ { "on": "interact", "actions": [
                  { "action": "toggle", "target": "always_on" }
              ] } ] }
        ]
    }"#;
    let bad_level = LevelDef::from_json(bad).expect("the bad switch level parses");
    let err = crate::loader::validate_level(&bad_level)
        .expect_err("a non-switchable fixture is not a toggle target");
    assert!(
        err.contains("switchable ceiling fixture"),
        "the error names the requirement: {err}"
    );
    let mut bad_game = game_for(&bad_level);
    let unsupported_report = bad_game.dispatch_actions(
        &[ActionDef::Toggle {
            target: Some("always_on".into()),
        }],
        None,
    );
    assert_eq!(
        unsupported_report.unsupported, 1,
        "a baked fixture is refused, never reported as switched"
    );
    assert_eq!(unsupported_report.actions_run, 0);
    assert!(
        bad_game.take_light_toggles().is_empty(),
        "a refused toggle reaches the renderer as nothing"
    );
    assert_eq!(
        bad_game.light_states(),
        vec![(0, true)],
        "the baked fixture stays on"
    );
}

/// A closed door blocks the player, an open one lets them through, and the
/// same door can be driven by a switch's declarative action batch.
#[test]
fn a_door_blocks_the_player_when_closed_and_passes_when_open() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "door_collision",
            "name": "Door Collision",
            "spawn": { "x": 4.0, "z": 2.0, "yaw_degrees": 180.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "doors": [
                { "id": "barrier", "x": 2.0, "y": 0.0, "z": 4.0,
                  "rotation_degrees": 0.0, "width": 4.0, "height": 2.2,
                  "thickness": 0.06, "open_direction": "right",
                  "swing_degrees": 90.0, "initial_state": "closed" }
            ],
            "props": [
                { "id": "gate_switch", "model": "home:wall_switch", "x": 0.6, "y": 1.2, "z": 4.0,
                  "solid": false,
                  "components": [ { "component": "interactable", "prompt": "Gate" } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "open", "target": "barrier" } ] } ] }
            ]
        }"#,
    )
    .expect("the door collision level parses");
    crate::loader::validate_level(&level).expect("the door collision level validates");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);

    // Walk into the closed leaf: the player stops at it, never through it.
    walk_forward(&mut game, 120);
    assert!(
        game.player_position.z < 4.0,
        "the closed door blocks passage: z={}",
        game.player_position.z
    );
    assert!(game.grounded);

    // A switch drives it: the interact binding opens the leaf, and the frames
    // after it advance the hinge.
    let gate_switch = game
        .interactables()
        .index_of("gate_switch")
        .expect("the switch is aimable");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(gate_switch))
        .expect("the switch fires");
    assert_eq!(report.actions_run, 1, "the switch opened the door");
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "opening");
    let settings = Settings::default();
    let mut idle = InputState::default();
    for _ in 0_i32..80_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "open");
    assert!((game.doors().get(0).expect("door").angle().abs() - 90.0).abs() < 1.0e-3);

    // The open leaf lies along +Z from the hinge at x = 2, clear of the
    // player's lane at x = 4: they walk through.
    walk_forward(&mut game, 120);
    assert!(
        game.player_position.z > 5.0,
        "the open door lets the player through: z={}",
        game.player_position.z
    );
}

/// A manually interactable door is an aimable interactable: interaction
/// dispatch toggles it, and an externally controlled door is not aimable.
#[test]
fn a_manual_door_is_an_interaction_target_and_an_external_one_is_not() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "door_interaction",
            "name": "Door Interaction",
            "spawn": { "x": 4.0, "z": 2.0, "yaw_degrees": 180.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "doors": [
                { "id": "manual_door", "x": 3.0, "z": 3.2, "rotation_degrees": 0.0,
                  "width": 1.2, "height": 2.1, "open_direction": "left",
                  "components": [ { "component": "interactable", "prompt": "Door" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [{ "action": "toggle" }] } ] },
                { "id": "external_door", "x": 5.5, "z": 3.2, "rotation_degrees": 0.0,
                  "width": 1.2, "height": 2.1 }
            ]
        }"#,
    )
    .expect("the door interaction level parses");
    crate::loader::validate_level(&level).expect("the door interaction level validates");
    let mut game = game_for(&level);
    let ids: Vec<&str> = game
        .interactables()
        .items()
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert!(ids.contains(&"manual_door"));
    assert!(!ids.contains(&"external_door"));

    // Interact with the manual door: it starts opening and its prompt turns
    // into the closing prompt once it is open.
    let index = game
        .interactables()
        .index_of("manual_door")
        .expect("manual door target");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(index))
        .expect("the door fires");
    assert_eq!(report.actions_run, 1);
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "opening");
}

/// Action batches are finite by construction: an action targets an entity (a
/// door, a label, an animation), never another action list, so a self-referential
/// switch cannot recurse and a dispatch always terminates within the batch
/// bound.
#[test]
fn self_referential_actions_dispatch_once_and_terminate() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "self_actions",
            "name": "Self Actions",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "doors": [
                { "id": "self_door", "x": 1.0, "z": 3.0, "rotation_degrees": 0.0,
                  "width": 1.0, "height": 2.1 }
            ],
            "props": [
                { "id": "self_switch", "model": "home:wall_switch", "x": 1.0, "y": 1.2, "z": 2.0,
                  "solid": false,
                  "components": [ { "component": "interactable", "prompt": "Self" } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "toggle_label", "target": "self_switch" },
                      { "action": "toggle", "target": "self_door" },
                      { "action": "toggle_label", "target": "self_switch" }
                  ] } ] }
            ]
        }"#,
    )
    .expect("the self-action level parses");
    crate::loader::validate_level(&level).expect("the self-action level validates");
    let mut game = game_for(&level);
    let switch = game
        .interactables()
        .index_of("self_switch")
        .expect("the switch is aimable");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(switch))
        .expect("the switch fires");
    assert_eq!(
        report.actions_run, 3,
        "the batch runs exactly its own actions, never a chain"
    );
    assert_eq!(
        game.doors().get(0).expect("door").phase().name(),
        "opening",
        "the door toggle ran inside the batch"
    );
    assert!(
        !game.is_label_visible(switch),
        "two self-label toggles run in order and both are counted"
    );
}

// ---------------------------------------------------------------------------
// the entity runtime contract: components, bindings, volumes, timers,
// sequences, spawns and locks
// ---------------------------------------------------------------------------

/// A disabled interactable keeps its place in the aiming table but is never
/// targetable; `enable` restores it.
#[test]
fn a_disabled_interactable_is_not_aimable_until_enabled() {
    let level = interaction_level(
        r#"[
            { "id": "off_prop", "display_name": "Off Prop", "model": "core:plant",
              "x": 3.0, "z": 5.0,
              "components": [ { "component": "interactable", "enabled": false,
                                "prompt": "Off" } ] }
        ]"#,
        "[]",
        "[]",
    );
    crate::loader::validate_level(&level).expect("the disabled level validates");
    let mut game = game_for(&level);
    let off = game
        .interactables()
        .index_of("off_prop")
        .expect("the instance stays in the table");
    game.set_app_state(AppState::Playing);
    aim_at(&mut game, 3.0, 5.0, 0.9);
    assert!(
        !game.interactables().get(off).expect("prop").enabled,
        "the instance starts disabled"
    );
    assert_eq!(
        game.interaction_target(),
        None,
        "a disabled interactable is never aimable"
    );

    let report = game.dispatch_actions(
        &[ActionDef::Enable {
            target: Some("off_prop".into()),
        }],
        None,
    );
    assert_eq!(report.actions_run, 1);
    assert!(game.interactables().get(off).expect("prop").enabled);
    aim_at(&mut game, 3.0, 5.0, 0.9);
    assert_eq!(game.interaction_target(), Some(off));
}

/// A switch entity whose `interact` binding calls `set_light` flips the
/// fixture's light component and hands exactly one change to the renderer.
#[test]
fn a_switch_sets_a_switchable_fixture_light_off() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "interact_light",
            "name": "Interact Light",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "ceiling_lights": [
                { "id": "room_light", "fixture": "core:fluorescent_panel_01",
                  "x": 4.0, "z": 4.0, "switchable": true }
            ],
            "props": [
                { "id": "room_switch", "model": "home:wall_switch", "x": 4.0, "y": 1.2, "z": 1.0,
                  "solid": false,
                  "components": [ { "component": "interactable", "prompt": "Switch" } ],
                  "bindings": [ { "on": "interact", "actions": [
                      { "action": "set_light", "target": "room_light", "on": false }
                  ] } ] }
            ]
        }"#,
    )
    .expect("the interaction-light level parses");
    crate::loader::validate_level(&level).expect("the interaction-light level validates");

    let mut game = game_for(&level);
    let switch = game
        .interactables()
        .index_of("room_switch")
        .expect("the switch is aimable");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(switch))
        .expect("the switch fires");
    assert_eq!(report.actions_run, 1, "the set_light binding ran");
    let light = game
        .entities()
        .handle_of("room_light")
        .expect("the fixture entity");
    assert!(
        !game
            .entities()
            .components()
            .lights
            .get(light)
            .expect("the light component")
            .enabled,
        "the fixture's light component flipped off"
    );
    assert_eq!(
        game.take_light_toggles(),
        vec![(0, false)],
        "the change is handed to the renderer exactly once"
    );
    assert!(game.take_light_toggles().is_empty(), "and never repeats");
    assert_eq!(game.light_states(), vec![(0, false)]);

    // Writing the same state again is not a change: the binding still runs,
    // but nothing is reported to the renderer.
    let unchanged_report = game
        .entities_mut()
        .dispatch_interaction(Some(switch))
        .expect("the switch fires again");
    assert_eq!(unchanged_report.actions_run, 1);
    assert!(
        game.take_light_toggles().is_empty(),
        "game.take_light_toggles() must be empty"
    );
}

/// `set_state` emits an `object_state` event, and a second binding on the same
/// entity runs only while its `enabled` condition holds: both branches.
#[test]
fn an_object_state_binding_runs_only_when_its_condition_holds() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "state_condition",
            "name": "State Condition",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "doors": [
                { "id": "gate", "x": 6.5, "z": 1.0, "rotation_degrees": 0.0,
                  "width": 1.0, "height": 2.1,
                  "components": [ { "component": "interactable" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [ { "action": "toggle" } ] } ] }
            ],
            "props": [
                { "id": "panel", "display_name": "Panel", "model": "home:wall_switch",
                  "x": 3.0, "z": 2.0,
                  "components": [ { "component": "interactable", "prompt": "Arm" },
                                  { "component": "state", "name": "on", "value": false } ],
                  "bindings": [
                      { "on": "interact",
                        "actions": [ { "action": "set_state", "name": "on",
                                       "value": true } ] },
                      { "on": "object_state", "key": "on",
                        "when": [ { "check": "enabled", "target": "gate" } ],
                        "actions": [ { "action": "toggle_label" } ] }
                  ] }
            ]
        }"#,
    )
    .expect("the state-condition level parses");
    crate::loader::validate_level(&level).expect("the state-condition level validates");

    // True branch: the door is enabled, so the conditioned binding runs.
    let mut game = game_for(&level);
    let panel = game.interactables().index_of("panel").expect("panel");
    assert!(
        game.entities()
            .condition_holds(&crate::level::ConditionDef::Enabled {
                target: "gate".into()
            }),
        "the condition starts true"
    );
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(panel))
        .expect("the panel fires");
    assert_eq!(
        report.actions_run, 2,
        "the state write and the conditioned toggle both ran"
    );
    assert_eq!(state_of(&game, "panel", "on"), Some(StateValue::Bool(true)));
    assert!(
        game.is_label_visible(panel),
        "the condition held: the label toggled"
    );

    // False branch: disabling the door makes the same event skip the binding.
    let mut blocked = game_for(&level);
    let _dispatch_report = blocked.dispatch_actions(
        &[ActionDef::Disable {
            target: Some("gate".into()),
        }],
        None,
    );
    assert!(
        !blocked
            .entities()
            .condition_holds(&crate::level::ConditionDef::Enabled {
                target: "gate".into()
            }),
        "the disabled door fails the condition"
    );
    let blocked_panel = blocked.interactables().index_of("panel").expect("panel");
    let blocked_report = blocked
        .entities_mut()
        .dispatch_interaction(Some(blocked_panel))
        .expect("the panel fires");
    assert_eq!(
        blocked_report.actions_run, 1,
        "only the state write ran; the false-condition binding was skipped"
    );
    assert_eq!(
        state_of(&blocked, "panel", "on"),
        Some(StateValue::Bool(true)),
        "the state is written in both branches"
    );
    assert!(
        !blocked.is_label_visible(blocked_panel),
        "the false-condition binding never ran"
    );
}

/// A volume `enter_volume` binding with `once: true` fires on its first entry,
/// refuses the second, and is re-armed by a reset.
#[test]
fn a_once_volume_binding_fires_once_and_re_arms_on_reset() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "once_volume",
            "name": "Once Volume",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 3.0 } ],
            "props": [
                { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
                  "x": 9.0, "z": 9.0,
                  "components": [ { "component": "interactable", "enabled": false } ] }
            ],
            "volumes": [
                { "id": "pad", "x": 5.0, "z": 5.0, "width": 2.0, "depth": 2.0,
                  "bottom_y": 0.0, "top_y": 1.5,
                  "bindings": [ { "on": "enter_volume", "once": true,
                                  "actions": [ { "action": "toggle_label",
                                                 "target": "lamp" } ] } ] }
            ]
        }"#,
    )
    .expect("the once-volume level parses");
    crate::loader::validate_level(&level).expect("the once-volume level validates");

    let mut game = game_for(&level);
    let lamp = game.interactables().index_of("lamp").expect("lamp");
    assert_eq!(game.volume_count(), 1);

    let enter = |subject: &mut Game| {
        play_at(subject, 6.0, 0.0, 6.0, 0.0, 1.0 / 60.0);
        subject.update_player_movement(&mut InputState::default(), &Settings::default());
    };
    enter(&mut game);
    assert!(
        game.is_label_visible(lamp),
        "the once binding fires on the first entry"
    );

    // Leave and return: the second entry is refused by `once`.
    play_at(&mut game, 1.0, 0.0, 1.0, 0.0, 1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    enter(&mut game);
    assert!(
        game.is_label_visible(lamp),
        "the second entry is refused by `once`"
    );

    // A reset re-arms the binding: the next entry fires again.
    game.reset_to_spawn();
    enter(&mut game);
    assert!(
        !game.is_label_visible(lamp),
        "a reset re-arms the once binding"
    );
}

/// A one-shot timer fires exactly once, after its full period and never again.
#[test]
fn a_one_shot_timer_fires_exactly_once() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "one_shot_timer",
            "name": "One Shot Timer",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
                  "x": 9.0, "z": 9.0,
                  "components": [ { "component": "interactable", "enabled": false } ] }
            ],
            "timers": [
                { "id": "tick_once", "seconds": 0.5, "repeat": false, "autostart": false,
                  "bindings": [ { "on": "timer", "key": "tick_once",
                                  "actions": [ { "action": "toggle_label",
                                                 "target": "lamp" } ] } ] }
            ]
        }"#,
    )
    .expect("the one-shot timer level parses");
    crate::loader::validate_level(&level).expect("the one-shot timer level validates");

    let mut game = game_for(&level);
    let lamp = game.interactables().index_of("lamp").expect("lamp");
    let report = game.dispatch_actions(
        &[ActionDef::StartTimer {
            target: Some("tick_once".into()),
            seconds: None,
            repeat: None,
        }],
        None,
    );
    assert_eq!(report.timers_changed, 1);
    assert_eq!(report.missing_targets, 0);

    advance_frames(&mut game, 29);
    assert_eq!(
        game.entities()
            .timers()
            .get("tick_once")
            .expect("timer")
            .fires,
        0,
        "not due before its period"
    );
    assert!(!game.is_label_visible(lamp));

    advance_frames(&mut game, 2);
    let timer = game.entities().timers().get("tick_once").expect("timer");
    assert_eq!(timer.fires, 1, "exactly one fire at the period");
    assert!(!timer.running, "a one-shot stops after its fire");
    assert!(game.is_label_visible(lamp), "the fire ran its binding");

    advance_frames(&mut game, 120);
    let later_timer = game.entities().timers().get("tick_once").expect("timer");
    assert_eq!(later_timer.fires, 1, "a one-shot never fires again");
    assert!(
        game.is_label_visible(lamp),
        "no second fire toggled the label back"
    );
}

/// A repeating timer re-arms at its full period and fires again.
#[test]
fn a_repeating_timer_fires_again_each_period() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "repeating_timer",
            "name": "Repeating Timer",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
                  "x": 9.0, "z": 9.0,
                  "components": [ { "component": "interactable", "enabled": false } ] }
            ],
            "timers": [
                { "id": "pulse", "seconds": 0.25, "repeat": true, "autostart": false,
                  "bindings": [ { "on": "timer", "key": "pulse",
                                  "actions": [ { "action": "toggle_label",
                                                 "target": "lamp" } ] } ] }
            ]
        }"#,
    )
    .expect("the repeating timer level parses");
    crate::loader::validate_level(&level).expect("the repeating timer level validates");

    let mut game = game_for(&level);
    let lamp = game.interactables().index_of("lamp").expect("lamp");
    let report = game.dispatch_actions(
        &[ActionDef::StartTimer {
            target: Some("pulse".into()),
            seconds: None,
            repeat: None,
        }],
        None,
    );
    assert_eq!(report.timers_changed, 1);

    advance_frames(&mut game, 14);
    assert_eq!(
        game.entities().timers().get("pulse").expect("timer").fires,
        0
    );
    advance_frames(&mut game, 2);
    assert_eq!(
        game.entities().timers().get("pulse").expect("timer").fires,
        1
    );
    assert!(game.is_label_visible(lamp));

    // One full period later the timer has fired again and re-armed.
    advance_frames(&mut game, 16);
    let timer = game.entities().timers().get("pulse").expect("timer");
    assert_eq!(timer.fires, 2, "a repeating timer fires again");
    assert!(timer.running, "and stays armed");
    assert!(
        !game.is_label_visible(lamp),
        "the second fire toggled the label"
    );

    advance_frames(&mut game, 16);
    assert_eq!(
        game.entities().timers().get("pulse").expect("timer").fires,
        3
    );
    assert!(game.is_label_visible(lamp));
}

/// An `interact` binding starts a sequence; its steps wait, write a state and
/// emit, and completion fires the `sequence_complete` binding exactly once.
#[test]
fn an_interact_starts_a_sequence_that_completes_once() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "sequence_test",
            "name": "Sequence Test",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "actor", "display_name": "Actor", "model": "core:crate",
                  "x": 4.0, "z": 4.0,
                  "components": [ { "component": "interactable", "prompt": "Run" },
                                  { "component": "state", "name": "phase",
                                    "value": "idle" } ],
                  "bindings": [
                      { "on": "interact",
                        "actions": [ { "action": "start_sequence",
                                       "sequence": "lamp_on" } ] },
                      { "on": "sequence_complete",
                        "actions": [ { "action": "toggle_label" } ] }
                  ] }
            ],
            "sequences": [
                { "id": "lamp_on", "steps": [
                    { "step": "wait", "seconds": 0.05 },
                    { "step": "set_state", "name": "phase", "value": "lit" },
                    { "step": "emit", "on": "object_state", "key": "done" }
                ] }
            ]
        }"#,
    )
    .expect("the sequence level parses");
    crate::loader::validate_level(&level).expect("the sequence level validates");

    let mut game = game_for(&level);
    let actor = game.interactables().index_of("actor").expect("actor");
    let report = game
        .entities_mut()
        .dispatch_interaction(Some(actor))
        .expect("the actor fires");
    assert_eq!(report.actions_run, 1, "the interact binding started it");
    let handle = game.entities().handle_of("actor").expect("actor");
    let control = game
        .entities()
        .components()
        .sequences
        .get(handle)
        .expect("a sequence control");
    assert!(control.running);
    assert_eq!(control.sequence, "lamp_on");

    // Two frames are inside the 0.05 s wait: nothing has run yet.
    advance_frames(&mut game, 2);
    assert_eq!(
        state_of(&game, "actor", "phase"),
        Some(StateValue::Text("idle".into())),
        "the wait step defers the state write"
    );
    assert!(!game.is_label_visible(actor));

    // The wait completes, then the immediate set_state and emit steps run and
    // the sequence completes in the same tick.
    advance_frames(&mut game, 3);
    assert_eq!(
        state_of(&game, "actor", "phase"),
        Some(StateValue::Text("lit".into()))
    );
    assert!(
        !game
            .entities()
            .components()
            .sequences
            .get(handle)
            .expect("control")
            .running,
        "the sequence completed"
    );
    assert!(
        game.is_label_visible(actor),
        "sequence_complete ran its binding"
    );

    advance_frames(&mut game, 30);
    assert!(
        game.is_label_visible(actor),
        "exactly one completion, never a second"
    );
}

/// A second `start_sequence` on the same entity replaces the first: only the
/// replacement's steps run.
#[test]
fn a_second_sequence_replaces_the_first_on_the_same_entity() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "sequence_replace",
            "name": "Sequence Replace",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "actor", "display_name": "Actor", "model": "core:crate",
                  "x": 4.0, "z": 4.0,
                  "components": [ { "component": "state", "name": "phase",
                                    "value": "idle" } ] }
            ],
            "sequences": [
                { "id": "slow", "steps": [
                    { "step": "wait", "seconds": 10.0 },
                    { "step": "action", "action": { "action": "set_state", "target": "actor",
                                                    "name": "phase", "value": "slow" } }
                ] },
                { "id": "fast", "steps": [
                    { "step": "wait", "seconds": 0.05 },
                    { "step": "action", "action": { "action": "set_state", "target": "actor",
                                                    "name": "phase", "value": "fast" } }
                ] }
            ]
        }"#,
    )
    .expect("the sequence-replace level parses");
    crate::loader::validate_level(&level).expect("the sequence-replace level validates");

    let mut game = game_for(&level);
    let started = game.dispatch_actions(
        &[ActionDef::StartSequence {
            sequence: "slow".into(),
            target: Some("actor".into()),
        }],
        None,
    );
    assert_eq!(started.sequences_started, 1);
    advance_frames(&mut game, 3);
    assert_eq!(
        state_of(&game, "actor", "phase"),
        Some(StateValue::Text("idle".into())),
        "the slow run is still waiting"
    );

    let replaced = game.dispatch_actions(
        &[ActionDef::StartSequence {
            sequence: "fast".into(),
            target: Some("actor".into()),
        }],
        None,
    );
    assert_eq!(replaced.sequences_started, 1);
    let handle = game.entities().handle_of("actor").expect("actor");
    assert_eq!(
        game.entities()
            .components()
            .sequences
            .get(handle)
            .expect("control")
            .sequence,
        "fast",
        "the second start replaced the first"
    );

    advance_frames(&mut game, 6);
    assert_eq!(
        state_of(&game, "actor", "phase"),
        Some(StateValue::Text("fast".into())),
        "the replacement's step ran"
    );

    // Ten seconds later the replaced run would have written its own state;
    // it never does.
    advance_frames(&mut game, 601);
    assert_eq!(
        state_of(&game, "actor", "phase"),
        Some(StateValue::Text("fast".into())),
        "the replaced sequence never ran"
    );
}

/// A despawned sequence owner is cancelled: its later steps never run and it
/// never completes, with no panic.
#[test]
fn despawning_a_sequence_owner_cancels_it_without_completing() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "sequence_despawn",
            "name": "Sequence Despawn",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "props": [
                { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
                  "x": 9.0, "z": 9.0,
                  "components": [ { "component": "interactable", "enabled": false } ] }
            ],
            "spawn_templates": [
                { "id": "victim_template", "model": "core:crate", "scale": 1.0,
                  "bindings": [ { "on": "sequence_complete",
                                  "actions": [ { "action": "toggle_label",
                                                 "target": "lamp" } ] } ] }
            ],
            "spawn_points": [
                { "id": "victim_pad", "x": 5.0, "z": 5.0, "template": "victim_template" }
            ],
            "sequences": [
                { "id": "waiter", "steps": [
                    { "step": "wait", "seconds": 1.0 },
                    { "step": "action", "action": { "action": "toggle_label",
                                                    "target": "lamp" } }
                ] }
            ]
        }"#,
    )
    .expect("the sequence-despawn level parses");
    crate::loader::validate_level(&level).expect("the sequence-despawn level validates");

    let mut game = game_for(&level);
    let lamp = game.interactables().index_of("lamp").expect("lamp");
    let spawned = game.dispatch_actions(
        &[ActionDef::SpawnEntity {
            template: None,
            point: Some("victim_pad".into()),
            group: None,
            name: Some("victim".into()),
        }],
        None,
    );
    assert_eq!(spawned.spawned, 1);
    let started = game.dispatch_actions(
        &[ActionDef::StartSequence {
            sequence: "waiter".into(),
            target: Some("victim".into()),
        }],
        None,
    );
    assert_eq!(started.sequences_started, 1);
    advance_frames(&mut game, 5);
    assert!(
        !game.is_label_visible(lamp),
        "the sequence is still inside its wait"
    );

    let despawned = game.dispatch_actions(
        &[ActionDef::DespawnEntity {
            target: "victim".into(),
        }],
        None,
    );
    assert_eq!(despawned.despawned, 1);
    assert!(game.entities().handle_of("victim").is_none());

    // Past the wait and past where the run would have completed: the later
    // step never runs and no completion event reaches the (dead) binding.
    advance_frames(&mut game, 120);
    assert!(
        !game.is_label_visible(lamp),
        "the cancelled sequence never ran its later step"
    );
}

/// A spawned entity lives its authored lifetime and despawns itself; the
/// frame loop receives the spawn and despawn render commands.
#[test]
fn a_spawned_entity_despawns_when_its_lifetime_elapses() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "spawn_lifetime",
            "name": "Spawn Lifetime",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "spawn_templates": [
                { "id": "spark", "model": "core:crate", "scale": 1.0,
                  "lifetime_seconds": 0.5 }
            ],
            "spawn_points": [
                { "id": "spark_pad", "x": 5.0, "z": 5.0, "template": "spark" }
            ]
        }"#,
    )
    .expect("the spawn-lifetime level parses");
    crate::loader::validate_level(&level).expect("the spawn-lifetime level validates");

    let mut game = game_for(&level);
    let report = game.dispatch_actions(
        &[ActionDef::SpawnEntity {
            template: None,
            point: Some("spark_pad".into()),
            group: None,
            name: Some("spark".into()),
        }],
        None,
    );
    assert_eq!(report.spawned, 1);
    assert_eq!(report.missing_targets, 0);
    assert_eq!(report.unsupported, 0);
    assert_eq!(game.entities().live_spawns().len(), 1);
    assert!(game.entities().handle_of("spark").is_some());
    let commands = game.entities_mut().take_commands();
    assert!(
        commands.iter().any(|command| matches!(
            command,
            WorldCommand::SpawnDynamic {
                entity: _,
                model: _,
                position: _,
                yaw_degrees: _,
                scale: _
            }
        )),
        "the spawn hands a render command to the frame loop"
    );

    // Before the lifetime ends the entity is still alive.
    advance_frames(&mut game, 29);
    assert!(
        game.entities().handle_of("spark").is_some(),
        "the lifetime has not elapsed yet"
    );

    // Past the lifetime it despawns itself and releases its render object.
    advance_frames(&mut game, 2);
    assert!(game.entities().handle_of("spark").is_none());
    assert!(
        game.entities().live_spawns().is_empty(),
        "game.entities().live_spawns() must be empty"
    );
    let despawn_commands = game.entities_mut().take_commands();
    assert!(
        despawn_commands.iter().any(|command| matches!(
            command,
            WorldCommand::DespawnDynamic {
                entity: _,
                instance_id: _
            }
        )),
        "the despawn hands a render command to the frame loop"
    );
}

/// An `at_most_one_active` spawn group admits one member, refuses a second
/// while the first lives, and is released by the member's despawn.
#[test]
fn an_at_most_one_spawn_group_admits_one_member_and_releases_it() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "spawn_group",
            "name": "Spawn Group",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 } ],
            "spawn_templates": [
                { "id": "member", "model": "core:crate", "scale": 1.0 }
            ],
            "spawn_points": [
                { "id": "group_pad", "x": 5.0, "z": 5.0, "template": "member",
                  "group": "solo" }
            ],
            "spawn_groups": [
                { "id": "solo", "at_most_one_active": true }
            ]
        }"#,
    )
    .expect("the spawn-group level parses");
    crate::loader::validate_level(&level).expect("the spawn-group level validates");

    let mut game = game_for(&level);
    let spawn = |subject: &mut Game, name: &str| {
        subject.dispatch_actions(
            &[ActionDef::SpawnEntity {
                template: None,
                point: Some("group_pad".into()),
                group: None,
                name: Some(name.into()),
            }],
            None,
        )
    };
    let first = spawn(&mut game, "member_a");
    assert_eq!(first.spawned, 1);
    let second = spawn(&mut game, "member_b");
    assert_eq!(second.spawned, 0, "a second live member is refused");
    assert_eq!(
        second.unsupported, 1,
        "the refusal is reported, never silent"
    );
    assert_eq!(game.entities().live_spawns().len(), 1);
    let group = game.entities().spawn_groups().get("solo").expect("group");
    assert_eq!(group.spawns, 1);
    assert!(group.live.is_some());

    // Despawning the live member releases the group, and a later spawn works.
    let released = game.dispatch_actions(
        &[ActionDef::DespawnEntity {
            target: "member_a".into(),
        }],
        None,
    );
    assert_eq!(released.despawned, 1);
    assert!(
        game.entities()
            .spawn_groups()
            .get("solo")
            .expect("group")
            .live
            .is_none(),
        "the group is released by the despawn"
    );
    let third = spawn(&mut game, "member_c");
    assert_eq!(third.spawned, 1);
    assert_eq!(
        game.entities()
            .spawn_groups()
            .get("solo")
            .expect("group")
            .spawns,
        2
    );

    // Two same-frame requests into a fresh group admit exactly one.
    let mut batch_game = game_for(&level);
    let batch = batch_game.dispatch_actions(
        &[
            ActionDef::SpawnEntity {
                template: None,
                point: Some("group_pad".into()),
                group: None,
                name: Some("member_d".into()),
            },
            ActionDef::SpawnEntity {
                template: None,
                point: Some("group_pad".into()),
                group: None,
                name: Some("member_e".into()),
            },
        ],
        None,
    );
    assert_eq!(batch.spawned, 1, "exactly one request is admitted");
    assert_eq!(batch.unsupported, 1, "the other is refused");
    assert_eq!(batch_game.entities().live_spawns().len(), 1);
}

/// A locked door refuses the interaction that would open it; unlocking through
/// another entity's `unlock` binding lets the same interaction open it.
#[test]
fn a_locked_door_refuses_interaction_until_unlocked_by_a_binding() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "locked_door",
            "name": "Locked Door",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "doors": [
                { "id": "gate", "x": 6.5, "z": 1.0, "rotation_degrees": 0.0,
                  "width": 1.0, "height": 2.1, "locked": true,
                  "components": [ { "component": "interactable", "prompt": "Gate" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [ { "action": "toggle" } ] } ] }
            ],
            "props": [
                { "id": "key_switch", "model": "home:wall_switch", "x": 1.0, "y": 1.2, "z": 1.0,
                  "solid": false,
                  "components": [ { "component": "interactable", "prompt": "Key" } ],
                  "bindings": [ { "on": "interact",
                                  "actions": [ { "action": "unlock",
                                                 "target": "gate" } ] } ] }
            ]
        }"#,
    )
    .expect("the locked-door level parses");
    crate::loader::validate_level(&level).expect("the locked-door level validates");

    let mut game = game_for(&level);
    let door_target = game
        .interactables()
        .index_of("gate")
        .expect("the door is a target");
    let key = game
        .interactables()
        .index_of("key_switch")
        .expect("the key is a target");
    assert_eq!(game.doors().is_locked("gate"), Some(true));

    let report = game
        .entities_mut()
        .dispatch_interaction(Some(door_target))
        .expect("the door fires");
    assert_eq!(
        report.actions_run, 0,
        "a locked door refuses the toggle action"
    );
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "closed");
    assert!(game.doors().get(0).expect("door").angle().abs() < 1e-6);

    // The key's own interact binding unlocks it.
    let unlock_report = game
        .entities_mut()
        .dispatch_interaction(Some(key))
        .expect("the key fires");
    assert_eq!(unlock_report.actions_run, 1);
    assert_eq!(game.doors().is_locked("gate"), Some(false));

    // The same door interaction now opens the leaf, and the frames advance it.
    let opened_report = game
        .entities_mut()
        .dispatch_interaction(Some(door_target))
        .expect("the door fires");
    assert_eq!(opened_report.actions_run, 1);
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "opening");
    advance_frames(&mut game, 80);
    assert_eq!(game.doors().get(0).expect("door").phase().name(), "open");
    assert!((game.doors().get(0).expect("door").angle().abs() - 90.0).abs() < 1e-3);
}

/// The shipped Demo's authored chain is real: stepping into the sauna volume
/// arms a timer, the timer starts a sequence on the door, and the sequence's
/// wait/state/emit steps land on the door's own typed state.
#[test]
fn the_demo_sauna_chain_runs_end_to_end() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/levels/places_demo.json"
    ))
    .expect("the demo source is readable");
    let level = LevelDef::from_json(&source).expect("the demo source parses");
    crate::loader::validate_level(&level).expect("the demo source validates");
    let mut game = game_for(&level);
    game.set_app_state(AppState::Playing);
    let settings = Settings::default();

    // Stand in the sauna warmup volume with the frame's start sweeps outside.
    let zone = level
        .volumes
        .iter()
        .find(|volume| volume.id.as_deref() == Some("sauna_warmup_zone"))
        .expect("the sauna zone exists");
    let (x0, x1, z0, z1) = zone.bounds();
    let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
    let floor_y = crate::level::LevelSurfaces::new(&level)
        .floor_y_at(cx, cz)
        .expect("the sauna floor");
    set_eye_position(&mut game, Vec3::new(cx, floor_y + EYE_HEIGHT, cz));
    game.sim_delta_seconds = 1.0 / 60.0;
    game.update_player_movement(&mut InputState::default(), &settings);

    // Two seconds of simulation: volume edge, 1.5 s timer, 0.5 s wait.
    for _ in 0_i32..140_i32 {
        game.sim_delta_seconds = 1.0 / 60.0;
        game.update_player_movement(&mut InputState::default(), &settings);
    }
    let door = game
        .entities()
        .handle_of("sauna_door")
        .expect("the door entity");
    let phase = game
        .entities()
        .components()
        .states
        .get(door)
        .and_then(|state| state.get("phase").cloned());
    assert_eq!(
        phase,
        Some(crate::entities::components::StateValue::Text("warm".into())),
        "the sequence wrote the door's phase"
    );
    // The sequence's emitted `warm` cue is consumed by the door's own binding,
    // which turns the sauna lamp off: the chain has an observable end.
    let lamp = game.entities().handle_of("sauna_light").expect("the lamp");
    let enabled = game
        .entities()
        .components()
        .lights
        .get(lamp)
        .is_some_and(|light| light.enabled);
    assert!(!enabled, "the warm cue switched the sauna lamp off");
}

// ---- Baked navigation and the home encounter ----

/// Bakes the real Demo with the compiler's own navigation entry point and
/// loads it through the package record codec, exactly as a package install
/// does. Returns the mesh plus the bake warnings.
fn demo_navigation(level: &LevelDef) -> (crate::nav::NavMesh, Vec<String>) {
    let mut warnings = Vec::new();
    let (bytes, report) = crate::compiler::bake_navigation(level, 1, &mut warnings)
        .expect("the demo navigation bakes");
    assert!(
        warnings.is_empty(),
        "the demo has no navigation placement warnings: {warnings:?}"
    );
    assert!(
        report.walkable_cells.first().copied().unwrap_or(0) > 1000,
        "the demo bakes a real mesh"
    );
    assert_eq!(
        report.portals, 7,
        "the demo's seven doors (hall, pool-side sauna, shower-side sauna, corridor-side sauna, \
         study, front, house) are portal links"
    );
    let reencoded_bytes = crate::package::navigation::write_navigation(
        &crate::package::navigation::read_navigation(&bytes).expect("the record decodes"),
    )
    .expect("the record re-encodes");
    let grid =
        crate::package::navigation::read_navigation(&reencoded_bytes).expect("the record decodes");
    let mesh = crate::nav::NavMesh::from_record(grid).expect("the mesh validates");
    (mesh, warnings)
}

/// The real Demo with its real baked navigation installed.
fn demo_game_with_navigation(delta: f32) -> Game {
    let level = demo_level();
    let (mesh, _) = demo_navigation(&level);
    let mut game = Game::new(
        spawn_position(&level),
        level.spawn.yaw_degrees.to_radians(),
        CollisionWorld::from_level_with_navigation(&level, mesh),
    );
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = delta;
    game
}

/// Presses an authored interactable by id, through the ordinary dispatch.
fn press(game: &mut Game, id: &str) -> DispatchReport {
    let index = game
        .interactables()
        .index_of(id)
        .unwrap_or_else(|| panic!("`{id}` is interactable"));
    game.world_mut()
        .dispatch_interaction(Some(index))
        .unwrap_or_else(|| panic!("`{id}` dispatch"))
}

/// Live AI agents named `instance_id`.
fn agents_named(game: &Game, instance_id: &str) -> usize {
    game.entities()
        .ai()
        .agents()
        .iter()
        .filter(|agent| agent.instance_id == instance_id)
        .count()
}

/// Advances fixed frames while checking that both actors stay on walkable
/// navigation and outside every static box.
fn advance_checked(game: &mut Game, frames: usize, actors: &[&str]) {
    let settings = Settings::default();
    let mut input = InputState::default();
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
        for actor in actors {
            let Some(agent) = game.entities().ai().agent(actor) else {
                continue;
            };
            let position = agent.position;
            for wall in game.walls() {
                let inside = position.x > wall.min_x - 1.0e-3
                    && position.x < wall.max_x + 1.0e-3
                    && position.z > wall.min_z - 1.0e-3
                    && position.z < wall.max_z + 1.0e-3
                    && wall.blocks_body(position.y, agent.profile.height);
                assert!(
                    !inside,
                    "`{actor}` at {position:?} is inside {wall:?} in {:?}",
                    agent.state
                );
            }
            // The agent must remain on a baked cell for its own class.
            let Some(mesh) = game.navigation() else {
                panic!("the demo game has navigation");
            };
            let Some(class) = mesh.class_index(&agent.profile.class()) else {
                panic!("`{actor}` has a baked class");
            };
            let Some(point) = mesh.nearest(
                class,
                position,
                0.75,
                1.0,
                game.doors(),
                agent.profile.can_open_doors,
            ) else {
                panic!("`{actor}` at {position:?} left the baked mesh");
            };
            assert!(
                point.position.distance(position) < 0.75,
                "`{actor}` is not on baked navigation"
            );
        }
    }
}

#[test]
#[expect(clippy::too_many_lines, reason = "one end-to-end encounter, in order")] // one end-to-end encounter, in order
fn demo_home_encounter_repeats_end_to_end() {
    const DELTA: f32 = 1.0 / 60.0;
    let mut game = demo_game_with_navigation(DELTA);
    let rat = "encounter_rat";
    let cat = "spooner_man_home";
    assert_eq!(agents_named(&game, cat), 1, "the home cat exists");
    assert_eq!(agents_named(&game, rat), 0, "no rat before the switch");

    // 1. The authored switch spawns exactly one rat; a same-frame second press
    //    and a press while the rat is alive are both refused by the group.
    let report = press(&mut game, "rat_release_switch");
    assert_eq!(report.spawned, 1, "the switch spawns one rat");
    assert_eq!(agents_named(&game, rat), 1);
    let _ = press(&mut game, "rat_release_switch");
    assert_eq!(agents_named(&game, rat), 1, "the group admits one rat");
    drop(game.entities_mut().take_commands());

    // 2. The rat flees and the cat pursues, both through the baked mesh.
    advance_checked(&mut game, 180, &[rat, cat]);
    let (rat_state, rat_goal, rat_complete) = {
        let agent = game.entities().ai().agent(rat).expect("rat");
        (agent.state, agent.path_goal, agent.path.complete)
    };
    let cat_state = game.entities().ai().agent(cat).expect("cat").state;
    assert!(
        matches!(
            rat_state,
            crate::ai::AiState::Flee {
                threat: _,
                destination: _
            }
        ),
        "the rat flees, got {rat_state:?}"
    );
    assert!(
        matches!(cat_state, crate::ai::AiState::Pursue { target: _ }),
        "the cat pursues, got {cat_state:?}"
    );
    assert!(
        rat_goal.is_some() && rat_complete,
        "the rat's flee destination is a real navigated route"
    );

    // 3. The cat catches within a generous bounded simulation period.
    let mut caught_at = None;
    for frame in 0_i32..(60_i32 * 90_i32) {
        advance_checked(&mut game, 1, &[rat, cat]);
        if game
            .entities()
            .ai()
            .agent(rat)
            .is_some_and(|agent| agent.caught)
        {
            caught_at = Some(frame);
            break;
        }
    }
    assert!(
        caught_at.is_some(),
        "the cat must catch the rat; rat at {:?} in {:?}, cat at {:?} in {:?}",
        game.entities().ai().agent(rat).map(|a| a.position),
        game.entities().ai().agent(rat).map(|a| a.state),
        game.entities().ai().agent(cat).map(|a| a.position),
        game.entities().ai().agent(cat).map(|a| a.state),
    );
    let cat_agent = game.entities().ai().agent(cat).expect("cat");
    assert!(
        matches!(cat_agent.state, crate::ai::AiState::Catch { target: _ }),
        "the catch state owns the presentation, got {:?}",
        cat_agent.state
    );

    // 4. The authored pounce/consume sequence despawns the rat at its end and
    //    returns the cat to ordinary AI; the switch works again.
    let mut despawned = false;
    for _ in 0_i32..(60_i32 * 30_i32) {
        advance_checked(&mut game, 1, &[cat]);
        if game.entities().handle_of(rat).is_none() {
            despawned = true;
            break;
        }
    }
    assert!(despawned, "the catch presentation despawns the rat");
    assert_eq!(agents_named(&game, rat), 0, "no stale rat agent");
    // The presentation keeps running for its authored consume/stand-up steps;
    // the cat must return to ordinary AI when the sequence ends.
    let mut released = false;
    for _ in 0_i32..(60_i32 * 20_i32) {
        advance_checked(&mut game, 1, &[cat]);
        let state = game.entities().ai().agent(cat).expect("cat").state;
        if !matches!(state, crate::ai::AiState::Catch { target: _ }) {
            released = true;
            break;
        }
    }
    assert!(
        released,
        "the cat returns to ordinary AI after the presentation"
    );

    // 5. The predator returns to its post, then a second activation repeats
    //    the whole encounter.
    let home = game.entities().ai().agent(cat).expect("cat").home;
    let mut returned = false;
    for _ in 0_i32..(60_i32 * 90_i32) {
        advance_checked(&mut game, 1, &[cat]);
        let agent = game.entities().ai().agent(cat).expect("cat");
        if agent.position.distance(home) < 2.0 && matches!(agent.state, crate::ai::AiState::Idle) {
            returned = true;
            break;
        }
    }
    assert!(returned, "the predator walks back to its post");
    let repeat_report = press(&mut game, "rat_release_switch");
    assert_eq!(repeat_report.spawned, 1, "the released group spawns again");
    assert_eq!(agents_named(&game, rat), 1, "a fresh rat");
    let mut chased = false;
    for _ in 0_i32..(60_i32 * 30_i32) {
        advance_checked(&mut game, 1, &[rat, cat]);
        let repeated_rat_state = game.entities().ai().agent(rat).expect("rat").state;
        let predator_state = game.entities().ai().agent(cat).expect("cat").state;
        if matches!(
            repeated_rat_state,
            crate::ai::AiState::Flee {
                threat: _,
                destination: _
            }
        ) && matches!(predator_state, crate::ai::AiState::Pursue { target: _ })
        {
            chased = true;
            break;
        }
    }
    assert!(
        chased,
        "the second activation repeats the flee/pursuit: rat {:?}, cat {:?}",
        game.entities().ai().agent(rat).map(|a| a.state),
        game.entities().ai().agent(cat).map(|a| a.state),
    );
}

#[test]
fn demo_home_encounter_survives_external_despawn_and_absence() {
    const DELTA: f32 = 1.0 / 60.0;
    let mut game = demo_game_with_navigation(DELTA);
    let rat = "encounter_rat";
    let cat = "spooner_man_home";
    let report = press(&mut game, "rat_release_switch");
    assert_eq!(report.spawned, 1);
    drop(game.entities_mut().take_commands());
    advance_checked(&mut game, 120, &[rat, cat]);
    assert!(game.entities().handle_of(rat).is_some());

    // An external despawn releases the group and the AI cleanly.
    let despawn_report = game.dispatch_actions(
        &[ActionDef::DespawnEntity {
            target: "home_rat_encounter".to_string(),
        }],
        None,
    );
    assert_eq!(despawn_report.despawned, 1, "the group's rat despawns");
    assert!(game.entities().handle_of(rat).is_none());
    assert_eq!(agents_named(&game, rat), 0, "the AI released the rat");
    advance_checked(&mut game, 240, &[cat]);
    let cat_agent = game.entities().ai().agent(cat).expect("cat");
    assert!(
        !matches!(
            cat_agent.state,
            crate::ai::AiState::Pursue { target: _ } | crate::ai::AiState::Catch { target: _ }
        ),
        "the cat releases a despawned target, got {:?}",
        cat_agent.state
    );

    // The switch is reusable after the failure.
    let rearmed_report = press(&mut game, "rat_release_switch");
    assert_eq!(
        rearmed_report.spawned, 1,
        "the switch re-arms after a despawn"
    );
    assert_eq!(agents_named(&game, rat), 1);

    // Without navigation the AI cannot move but never panics: a package with
    // no mesh cannot be installed (the record is mandatory), so the honest
    // check is that the level resolves without an AI crash.
    let level = demo_level();
    let world = CollisionWorld::from_level(&level);
    assert!(world.navigation.is_none(), "from_level never bakes");
    let mut entity_world = world.world;
    let walls = level.collision_aabbs();
    let index = crate::collision_index::CollisionIndex::build(&walls);
    let floor = crate::level::WalkableFloor::from_level(&level);
    let ctx = WorldContext {
        delta_seconds: DELTA,
        feet_from: Vec3::ZERO,
        feet: Vec3::ZERO,
        eye: Vec3::Y,
        body_height: 1.8,
        walls: &walls,
        index: &index,
        floor: &floor,
        nav: None,
    };
    let tick = entity_world.tick(&ctx);
    assert_eq!(tick.events_dropped, 0, "a meshless world still ticks");
}

// ---------------------------------------------------------------------------
// The demo's walkable height transitions, audited
// ---------------------------------------------------------------------------

/// One audited fixed-step walking phase: the observed motion plus a count of
/// the frames that would violate a shared traversal invariant.
///
/// Every walkable transition asserts the same contract: the player advances
/// (no refusal), stays grounded (no lost-support flicker), moves at most the
/// walk speed per frame (no teleport), changes height by at most one authored
/// riser plus one frame of the local pitch slope (no double-step), and keeps
/// the vertical velocity at rest (no oscillation).
#[derive(Debug, Default)]
struct WalkAudit {
    frames: u32,
    airborne_frames: u32,
    stalled_frames: u32,
    discrete_steps: u32,
    max_frame_step: f32,
    max_eye_step: f32,
    max_floor_step: f32,
    reached: bool,
}

impl WalkAudit {
    /// Asserts the phase was a clean walkable traversal that reached `stop`.
    #[track_caller]
    fn assert_walkable(&self, label: &str) {
        assert!(self.frames > 0, "{label}: the phase ran at least one frame");
        assert!(
            self.reached,
            "{label}: the player reached the phase target (top/bottom blockage?)"
        );
        assert_eq!(
            self.airborne_frames, 0,
            "{label}: a walkable surface never loses support"
        );
        assert_eq!(
            self.stalled_frames, 0,
            "{label}: no frame is refused (a stall means a blocked transition)"
        );
    }

    /// Asserts the phase was a clean traversal with no floor-height change at
    /// all, for the flat door thresholds and corridor legs.
    #[track_caller]
    fn assert_flat(&self, label: &str) {
        self.assert_walkable(label);
        assert!(
            self.max_eye_step <= 1e-6,
            "{label}: the eye never changes height on a flat floor ({})",
            self.max_eye_step
        );
        assert!(
            self.max_floor_step <= 1e-6,
            "{label}: the floor never changes height on a flat floor ({})",
            self.max_floor_step
        );
    }
}

/// Walks fixed 60 Hz frames holding forward, asserting the shared per-frame
/// traversal invariants and stopping once `stop` fires.
///
/// `max_vertical_step` bounds the eye and the walking floor for one frame: one
/// authored riser plus one frame of the local pitch slope, plus tolerance.
/// When `discrete_threshold` is set, frames whose eye step exceeds it are
/// counted in [`WalkAudit::discrete_steps`], so a test can prove a flight has
/// exactly one discrete riser per authored boundary and none in between.
fn audited_walk(
    game: &mut Game,
    max_frames: usize,
    max_vertical_step: f32,
    discrete_threshold: Option<f32>,
    mut stop: impl FnMut(&Game) -> bool,
) -> WalkAudit {
    let settings = Settings::default();
    let delta = 1.0 / 60.0;
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = delta;
    let mut audit = WalkAudit::default();
    let mut previous = game.player_position;
    let mut previous_floor = game.player_floor_y;
    let mut previous_velocity = game.vertical_velocity;
    for _ in 0..max_frames {
        game.update_player_movement(&mut input, &settings);
        let position = game.player_position;
        let floor = game.player_floor_y;
        let frame_step = Vec2::new(position.x - previous.x, position.z - previous.z).length();
        audit.frames = audit.frames.saturating_add(1);
        audit.max_frame_step = audit.max_frame_step.max(frame_step);
        assert!(
            frame_step <= settings.walk_speed.mul_add(delta, 1e-3),
            "no teleport: {frame_step} m in one frame at ({}, {})",
            position.x,
            position.z
        );
        let eye_dy = position.y - previous.y;
        audit.max_eye_step = audit.max_eye_step.max(eye_dy.abs());
        audit.max_floor_step = audit.max_floor_step.max((floor - previous_floor).abs());
        assert!(
            eye_dy.abs() <= max_vertical_step,
            "the eye step {eye_dy} exceeds one riser plus a frame of pitch slope \
             ({max_vertical_step}) at ({}, {})",
            position.x,
            position.z
        );
        if discrete_threshold.is_some_and(|threshold| eye_dy.abs() > threshold) {
            audit.discrete_steps = audit.discrete_steps.saturating_add(1);
        }
        if game.grounded {
            assert_exact(game.vertical_velocity, 0.0);
            assert!(
                (position.y - (game.feet_y() + game.eye_offset())).abs() <= 1e-4,
                "the grounded eye keeps the feet line: {} vs {}",
                position.y,
                game.feet_y() + game.eye_offset()
            );
        } else {
            audit.airborne_frames = audit.airborne_frames.saturating_add(1);
            assert!(
                game.vertical_velocity <= previous_velocity + 1e-6,
                "the airborne vertical velocity never rises: {} then {}",
                previous_velocity,
                game.vertical_velocity
            );
        }
        previous_velocity = game.vertical_velocity;
        previous = position;
        previous_floor = floor;
        if stop(game) {
            audit.reached = true;
            break;
        }
        if frame_step <= 1e-9 {
            audit.stalled_frames = audit.stalled_frames.saturating_add(1);
        }
    }
    audit
}

/// Holds forward for `frames` fixed 60 Hz frames against a barrier and returns
/// the largest per-frame horizontal step, so a solidity check can prove the
/// walk never teleports through what should stop it.
fn pushed_walk(game: &mut Game, frames: usize) -> f32 {
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    let mut previous = game.player_position;
    let mut max_step = 0.0_f32;
    for frame in 0..frames {
        game.update_player_movement(&mut input, &settings);
        let position = game.player_position;
        let step = Vec2::new(position.x - previous.x, position.z - previous.z).length();
        max_step = max_step.max(step);
        assert!(
            step <= settings.walk_speed / 60.0 + 1e-3,
            "no teleport against a solid: {step} m on frame {frame} at {position:?}"
        );
        previous = position;
    }
    max_step
}

/// Office floor 0.0 -> the wall-5 doorway (opening z 0.3..1.5, floor 0.0) ->
/// top landing region 0 (z 0..1.6, world 0.0) -> the five 0.3 m region steps
/// (z 1.6..4.8) -> stair hall floor -1.5, walked both ways.
#[test]
fn demo_office_floor_door_and_stair_hall_steps_walk_both_ways() {
    let riser = 0.3_f32;
    let max_step = riser + 1e-3;

    // East from the office, through the threshold, onto the landing.
    let mut game = demo_game_at(17.2, 0.0, 0.9, 90.0, 1.0 / 60.0);
    let approach = audited_walk(&mut game, 160, 1e-3, None, |subject| {
        subject.player_position.x > 22.0
    });
    approach.assert_flat("office door approach");
    assert!((game.player_floor_y - 0.0).abs() < 1e-3);

    // South down all five steps onto the hall floor.
    game.player_yaw = 180.0_f32.to_radians();
    let descent = audited_walk(&mut game, 200, max_step, Some(riser * 0.9), |subject| {
        subject.player_floor_y < -1.45
    });
    descent.assert_walkable("hall steps descent");
    assert_eq!(descent.discrete_steps, 5, "one riser per step boundary");
    assert!(
        (game.player_floor_y - (-1.5)).abs() < 1e-3,
        "the descent ends on the hall floor: {}",
        game.player_floor_y
    );
    assert!(game.player_position.z > 4.8, "past the last step");

    // North back up the steps to the landing and into the doorway's lane.
    game.player_yaw = 0.0;
    let ascent = audited_walk(&mut game, 220, max_step, Some(riser * 0.9), |subject| {
        subject.player_floor_y > -0.05 && subject.player_position.z < 1.0
    });
    ascent.assert_walkable("hall steps ascent");
    assert_eq!(ascent.discrete_steps, 5, "one riser per step boundary");
    assert!((game.player_floor_y - 0.0).abs() < 1e-3);

    // West through the doorway back into the office.
    game.player_yaw = 270.0_f32.to_radians();
    let exit = audited_walk(&mut game, 200, 1e-3, None, |subject| {
        subject.player_position.x < 18.3
    });
    exit.assert_flat("office door exit");
    assert!((game.player_floor_y - 0.0).abs() < 1e-3);

    // The landing is closed by wall 3 on the north and wall 4 on the east:
    // each stops the walk on the landing plane, never through it.
    let mut landing_game = demo_game_at(22.0, 0.0, 0.6, 0.0, 1.0 / 60.0);
    let _pushed_walk_status = pushed_walk(&mut landing_game, 60);
    assert!(
        landing_game.player_position.z > 0.4,
        "wall 3 stops the landing walk: {:?}",
        landing_game.player_position
    );
    assert!(landing_game.grounded && landing_game.player_floor_y.abs() < 1e-3);

    let mut side_game = demo_game_at(22.5, 0.0, 0.9, 90.0, 1.0 / 60.0);
    let _pushed_walk_status_2 = pushed_walk(&mut side_game, 60);
    assert!(
        side_game.player_position.x < 23.6,
        "wall 4 stops the landing walk: {:?}",
        side_game.player_position
    );
    assert!(side_game.grounded && side_game.player_floor_y.abs() < 1e-3);
}

/// A 45-degree crossing of the five 0.3 m steps that passes through the wall-5
/// doorway at an angle, plus a start/stop held on the third step.
#[test]
fn demo_office_steps_diagonal_and_stop_on_a_step() {
    let riser = 0.3_f32;
    let max_step = riser + 1e-3;

    let mut game = demo_game_at(23.0, -1.5, 4.9, 315.0, 1.0 / 60.0);
    let diagonal = audited_walk(&mut game, 260, max_step, Some(riser * 0.9), |subject| {
        subject.player_position.x < 18.4
    });
    diagonal.assert_walkable("hall steps diagonal");
    assert_eq!(diagonal.discrete_steps, 5, "each boundary is one riser");
    assert!((game.player_floor_y - 0.0).abs() < 1e-3);

    // Stopping on the third step holds the pose: no drift, no oscillation,
    // still grounded; resuming climbs the remaining risers.
    let mut paused_game = demo_game_at(22.0, -0.9, 3.6, 0.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut idle = InputState::default();
    let held = paused_game.player_position;
    for _ in 0_i32..60_i32 {
        paused_game.update_player_movement(&mut idle, &settings);
        assert!(paused_game.grounded, "the step holds the player");
        assert!((paused_game.player_floor_y - (-0.9)).abs() < 1e-4);
        assert!(
            (paused_game.player_position - held).length() <= 1e-6,
            "no drift while stopped on a step"
        );
        assert_exact(paused_game.vertical_velocity, 0.0);
    }
    let resumed = audited_walk(
        &mut paused_game,
        220,
        max_step,
        Some(riser * 0.9),
        |subject| subject.player_floor_y > -0.05,
    );
    resumed.assert_walkable("hall steps resume");
    assert_eq!(resumed.discrete_steps, 3, "the three risers above step 3");
}

/// The pool's submerged walk-in step (region 6, world -1.85): the 0.35 m edge
/// from the deck is waded down and up, crossed lengthwise, and approached
/// diagonally, with no airborne frame and no swim state.
#[test]
fn demo_pool_walk_in_step_up_down_and_diagonal() {
    let riser = 0.35_f32;
    let max_step = riser + 1e-3;

    // South deck (-1.5) north onto the step (-1.85).
    let mut game = demo_game_at(13.0, -1.5, 17.6, 0.0, 1.0 / 60.0);
    let down = audited_walk(&mut game, 80, max_step, Some(riser * 0.9), |subject| {
        (subject.player_floor_y - (-1.85)).abs() < 1e-3
    });
    down.assert_walkable("pool deck to walk-in step");
    assert_eq!(down.discrete_steps, 1, "one 0.35 m drop edge");
    assert!(!game.is_swimming(), "0.2 m of water is waded, not swum");
    assert!(
        (game.player_position.y - (-1.85 + EYE_HEIGHT)).abs() < 1e-3,
        "the wading eye is the step plus the eye height: {}",
        game.player_position.y
    );

    // Stopped on the submerged step: the wading pose holds with no drift and
    // no flicker back into the swim state.
    let settings = Settings::default();
    let mut idle = InputState::default();
    let held = game.player_position;
    for _ in 0_i32..60_i32 {
        game.update_player_movement(&mut idle, &settings);
        assert!(game.grounded, "the submerged step holds the body");
        assert!(!game.is_swimming(), "0.2 m of water never swims");
        assert!((game.player_floor_y - (-1.85)).abs() < 1e-4);
        assert!(
            (game.player_position - held).length() <= 1e-6,
            "no drift while stopped on the step"
        );
        assert_exact(game.vertical_velocity, 0.0);
    }

    // Turn around: the same 0.35 m edge back up onto the deck.
    game.player_yaw = 180.0_f32.to_radians();
    let up = audited_walk(&mut game, 80, max_step, Some(riser * 0.9), |subject| {
        (subject.player_floor_y - (-1.5)).abs() < 1e-3 && subject.player_position.z > 16.95
    });
    up.assert_walkable("walk-in step to deck");
    assert_eq!(up.discrete_steps, 1, "one 0.35 m rise edge");

    // The step spans the basin's south rim: cross it west to east from the
    // deck beside the pool, dropping and climbing the same edge.
    let mut across_game = demo_game_at(17.6, -1.5, 16.45, 270.0, 1.0 / 60.0);
    let across = audited_walk(
        &mut across_game,
        180,
        max_step,
        Some(riser * 0.9),
        |subject| subject.player_position.x < 9.7,
    );
    across.assert_walkable("walk-in step deck to deck");
    assert_eq!(across.discrete_steps, 2, "down and back up the same edge");
    assert!((across_game.player_floor_y - (-1.5)).abs() < 1e-3);

    // A diagonal approach lands on the step and stays grounded on it.
    let mut diagonal_game = demo_game_at(16.9, -1.5, 17.7, 315.0, 1.0 / 60.0);
    let diagonal = audited_walk(
        &mut diagonal_game,
        60,
        max_step,
        Some(riser * 0.9),
        |subject| {
            (subject.player_floor_y - (-1.85)).abs() < 1e-3 && subject.player_position.z < 16.8
        },
    );
    diagonal.assert_walkable("walk-in step diagonal");
    assert_eq!(
        diagonal.discrete_steps, 1,
        "one 0.35 m edge on the diagonal"
    );
}

/// The pool basin: the walk-in step's north rim is a real 1.15 m drop into the
/// deep water, the body never passes the basin floor, and a surfaced swimmer
/// climbs back onto the submerged step and wades out to the deck.
#[test]
fn demo_pool_basin_drops_from_the_step_and_wades_back_out() {
    let settings = Settings::default();
    let mut game = demo_game_at(13.0, -1.85, 16.6, 0.0, 1.0 / 60.0);
    let mut forward = InputState::holding(&[Control::MoveForward]);
    let mut previous = game.player_position;
    let mut airborne = false;
    let mut swimming = false;
    for _ in 0_i32..240_i32 {
        game.update_player_movement(&mut forward, &settings);
        let position = game.player_position;
        assert!(
            Vec2::new(position.x - previous.x, position.z - previous.z).length()
                <= settings.walk_speed / 60.0 + 1e-3,
            "no teleport into the basin"
        );
        previous = position;
        assert!(
            position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the body never passes the basin floor: {}",
            position.y
        );
        if !game.grounded {
            airborne = true;
        }
        if game.is_swimming() {
            swimming = true;
            break;
        }
    }
    assert!(airborne, "the 1.15 m basin rim is a real drop with no jump");
    assert!(swimming, "the drop ends in the basin water");
    assert!(
        game.player_floor_y <= -2.9,
        "the basin floor is the support under the swimmer: {}",
        game.player_floor_y
    );

    // Surface and swim back south to the step, then wade south to the deck.
    game.player_yaw = 180.0_f32.to_radians();
    let mut exit = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut stood_on_step = false;
    let mut exit_previous = game.player_position;
    for _ in 0_i32..400_i32 {
        let frame_start_eye = game.player_position.y;
        game.update_player_movement(&mut exit, &settings);
        let position = game.player_position;
        assert!(
            Vec2::new(position.x - exit_previous.x, position.z - exit_previous.z).length()
                <= settings.walk_speed / 60.0 + 1e-3,
            "no teleport out of the basin"
        );
        exit_previous = position;
        assert!(
            position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the body never passes the basin floor: {}",
            position.y
        );
        if game.is_swimming() {
            // The old instant stand-up wrote the standing line in the frame it
            // left the water, so the eye could never pass the float line while
            // `is_swimming()`. The bounded climb now legitimately rises above
            // the line over the frames it takes to stand: what must hold
            // instead is that every frame moves at most the climb rate.
            assert!(
                (position.y - frame_start_eye).abs() <= WATER_EXIT_CLIMB_SPEED / 60.0 + 1e-3,
                "the exit climbs at a bounded rate: {}",
                position.y - frame_start_eye
            );
        }
        if game.grounded && (game.player_floor_y - (-1.85)).abs() < 1e-3 {
            stood_on_step = true;
            break;
        }
    }
    assert!(
        stood_on_step,
        "the surfaced swimmer stands on the submerged step: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(!game.is_swimming(), "the step exit leaves the swim state");

    // The wade out to the deck is the ordinary 0.35 m walkable edge.
    game.player_yaw = 180.0_f32.to_radians();
    let wade = audited_walk(&mut game, 80, 0.35 + 1e-3, Some(0.35 * 0.9), |subject| {
        (subject.player_floor_y - (-1.5)).abs() < 1e-3 && subject.player_position.z > 17.0
    });
    wade.assert_walkable("walk-in step wade to deck");
    assert_eq!(wade.discrete_steps, 1, "one 0.35 m rise edge");
}

/// A fall from the demo deck enters the swim state as soon as the basin water
/// is deep enough to swim in — well above the basin floor — instead of
/// grounding on the pool bottom and waiting for the eye band.
#[test]
fn falling_into_deep_water_starts_swimming_near_the_surface() {
    let settings = Settings::default();
    let mut game = demo_game_at(21.0, -1.5, 12.0, 270.0, 1.0 / 60.0);
    let mut walk = InputState::holding(&[Control::MoveForward]);
    let mut grounded_on_basin = false;
    let mut first_swim_feet = f32::MAX;
    for _ in 0_i32..240_i32 {
        game.update_player_movement(&mut walk, &settings);
        if game.is_swimming() {
            first_swim_feet = game.feet_y();
            break;
        }
        // The basin floor is -3.0; the deck (-1.5) is where the walk starts.
        if game.grounded && game.player_floor_y <= -2.5 {
            grounded_on_basin = true;
        }
    }
    assert!(game.is_swimming(), "the deck fall ends in swimming");
    assert!(
        !grounded_on_basin,
        "the swim state begins before the player grounds on the basin floor"
    );
    // The entry threshold is the surface minus WADE_DEPTH (-2.2 m in the demo
    // basin): the first swimming frame sits just below it, not on the basin
    // floor (-3.0). The old eye-band gate instead put the first swimming frame
    // at feet <= surface - 1.23 = -2.88, essentially the floor.
    assert!(
        first_swim_feet <= -1.65 - WADE_DEPTH + 1e-3,
        "the water is deeper than WADE_DEPTH at the feet: {first_swim_feet}"
    );
    assert!(
        first_swim_feet > -2.5,
        "the entry happens near the surface, not on the basin floor: {first_swim_feet}"
    );
    assert!(!game.grounded, "the entry clears the grounded flag");
}

/// Walking off the demo deck and jumping in with Jump held never snap the eye:
/// every frame moves it at most the fall rate, and the entry seeds a bounded
/// descending velocity instead of zeroing the plunge.
#[test]
fn entering_the_pool_never_snaps_the_eye() {
    let settings = Settings::default();
    // The deck fall reaches ~4 m/s at the entry (0.067 m per 1/60 frame); the
    // jump-in apex makes it ~5.8 m/s (0.097 m per frame). 0.12 m bounds both.
    // The old path instead wrote the eye straight to the float line when the
    // band was crossed, a step of up to 0.25 m.
    let bound = 0.12_f32;
    for (label, jump) in [("walking off the deck", false), ("jumping in", true)] {
        let mut game = demo_game_at(21.0, -1.5, 12.0, 270.0, 1.0 / 60.0);
        let mut input = if jump {
            InputState::holding(&[Control::MoveForward, Control::Jump])
        } else {
            InputState::holding(&[Control::MoveForward])
        };
        let mut previous_eye = game.player_position.y;
        let mut entry_velocity = None;
        for _ in 0_i32..240_i32 {
            let before_velocity = game.vertical_velocity;
            game.update_player_movement(&mut input, &settings);
            let eye = game.player_position.y;
            assert!(
                (eye - previous_eye).abs() <= bound + 1e-4,
                "{label}: the eye jumped {} m in one frame at {eye}",
                eye - previous_eye
            );
            previous_eye = eye;
            if game.is_swimming() && entry_velocity.is_none() {
                let velocity = game.vertical_velocity;
                // The entry keeps the plunge as a bounded swim velocity (the
                // old path zeroed it): it stays negative and within twice the
                // swim rise speed.
                let plunge_bound = SWIM_RISE_SPEED.mul_add(-2.0, -1e-4);
                assert!(
                    (plunge_bound..0.0).contains(&velocity),
                    "{label}: the entry velocity is the bounded plunge, not zero: {velocity}"
                );
                assert!(
                    before_velocity < 0.0,
                    "{label}: the frame before the entry is falling: {before_velocity}"
                );
                entry_velocity = Some(velocity);
            }
        }
        assert!(
            entry_velocity.is_some(),
            "{label}: the fall reaches the water"
        );
        assert!(game.is_swimming(), "{label}: still swimming at the end");
    }
}

/// The demo deck exit is a bounded, cancellable climb: the eye rises at most
/// the climb speed per frame onto the real deck floor, and reversing back over
/// the water mid-climb cancels to swimming with the same bound.
#[test]
fn exiting_the_demo_pool_is_a_bounded_climb() {
    let settings = Settings::default();
    let bound = WATER_EXIT_CLIMB_SPEED / 60.0 + 1e-3;

    // The exit completes standing on the deck floor (-1.5), not with an
    // instant stand-up: the old path moved the eye from the float line to the
    // standing line in a single frame (1.63 m).
    let mut game = demo_game_at(12.0, -3.0, 14.0, 90.0, 1.0 / 60.0);
    let mut swim = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut previous_eye = game.player_position.y;
    let mut exited = false;
    for _ in 0_i32..400_i32 {
        game.update_player_movement(&mut swim, &settings);
        let eye = game.player_position.y;
        assert!(
            (eye - previous_eye).abs() <= bound,
            "the exit climb is bounded: {} m in one frame",
            eye - previous_eye
        );
        previous_eye = eye;
        if game.grounded
            && (game.player_floor_y - (-1.5)).abs() < 1e-3
            && game.player_position.x > 20.0
        {
            exited = true;
            break;
        }
    }
    assert!(
        exited,
        "the swimmer climbs onto the deck: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(
        !game.is_swimming(),
        "the completed climb leaves the swim state"
    );
    assert!(
        (game.feet_y() - (-1.5)).abs() < 1e-3,
        "the feet stand on the deck"
    );

    // Reversing mid-climb: face back west over the water and release Jump. The
    // recorded support stops being standable and the climb cancels back to
    // swimming, with no snap on the way.
    let mut side_exit_game = demo_game_at(12.0, -3.0, 14.0, 90.0, 1.0 / 60.0);
    let mut side_swim = InputState::holding(&[Control::MoveForward, Control::Jump]);
    for _ in 0_i32..400_i32 {
        side_exit_game.update_player_movement(&mut side_swim, &settings);
        if side_exit_game.water_exit.is_some() {
            break;
        }
    }
    assert!(
        side_exit_game.water_exit.is_some(),
        "the swimmer reaches the rim and climbs"
    );
    // A few frames of climb put the eye above the float line, so the reversal
    // is a genuine mid-climb cancel.
    for _ in 0_i32..4_i32 {
        side_exit_game.update_player_movement(&mut side_swim, &settings);
    }
    assert!(
        side_exit_game.water_exit.is_some(),
        "the climb is still in progress"
    );
    side_exit_game.player_yaw = 270.0_f32.to_radians();
    let mut reverse = InputState::holding(&[Control::MoveForward]);
    let mut reverse_previous = side_exit_game.player_position.y;
    let mut previous_position = side_exit_game.player_position;
    let mut cancelled = false;
    let horizontal_bound = settings.walk_speed * SWIM_SPEED_FACTOR / 60.0 + 1e-3;
    for _ in 0_i32..120_i32 {
        side_exit_game.update_player_movement(&mut reverse, &settings);
        let eye = side_exit_game.player_position.y;
        assert!(
            (eye - reverse_previous).abs() <= bound,
            "the cancelled climb is bounded: {} m in one frame",
            eye - reverse_previous
        );
        reverse_previous = eye;
        // The cancel must not hand the body to the airborne pass while the rim
        // is beside it: the horizontal step stays at the swim speed, never a
        // rim depenetration lurch (the verifier's D1).
        let step = Vec2::new(
            side_exit_game.player_position.x - previous_position.x,
            side_exit_game.player_position.z - previous_position.z,
        )
        .length();
        assert!(
            step <= horizontal_bound,
            "the cancelled climb never lurches horizontally: {step} m in one frame"
        );
        previous_position = side_exit_game.player_position;
        cancelled |= side_exit_game.water_exit.is_none();
    }
    assert!(cancelled, "reversing back over the water cancels the climb");
    assert!(
        side_exit_game.swimming && side_exit_game.is_swimming(),
        "the cancelled climb returns to the swim pose"
    );
    assert!(
        (side_exit_game.player_floor_y - (-3.0)).abs() < 1e-3,
        "the support is the basin floor again: {}",
        side_exit_game.player_floor_y
    );
    assert!(
        side_exit_game.player_position.x < 20.0,
        "the reversed player is back over the basin: {:?}",
        side_exit_game.player_position
    );
}

/// Three full enter/exit cycles at the demo rim and at the walk-in step stay
/// bounded and never flap: each cycle is exactly one entry and one completed
/// exit, with no teleport and no per-frame step beyond the documented bounds.
#[test]
fn repeated_pool_entry_and_exit_stays_bounded() {
    let settings = Settings::default();
    let horizontal_bound = settings.walk_speed / 60.0 + 1e-3;
    // The fall into the water reaches ~4 m/s (0.067 m per frame) and the
    // bounded climb is 0.037 m per frame; 0.12 bounds both. The walk-in cycle's
    // ordinary 0.35 m walkable step gets the `PLAYER_STEP_HEIGHT` bound below.
    let water_bound = 0.12_f32;

    // The demo rim: walk west off the deck, surface east, climb out.
    let mut game = demo_game_at(21.0, -1.5, 14.0, 270.0, 1.0 / 60.0);
    let mut phase = 0_u8;
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut previous = game.player_position;
    let mut before_swimming = game.swimming;
    let mut toggles = 0_u32;
    let mut cycles = 0_u32;
    for _ in 0_i32..4_000_i32 {
        if phase == 0 && game.swimming {
            game.player_yaw = 90.0_f32.to_radians();
            input = InputState::holding(&[Control::MoveForward, Control::Jump]);
            phase = 1;
        } else if phase == 1 && game.grounded && (game.player_floor_y - (-1.5)).abs() < 1e-3 {
            cycles = cycles.saturating_add(1);
            if cycles >= 3 {
                break;
            }
            game.player_yaw = 270.0_f32.to_radians();
            input = InputState::holding(&[Control::MoveForward]);
            phase = 0;
        }
        game.update_player_movement(&mut input, &settings);
        let position = game.player_position;
        assert!(
            Vec2::new(position.x - previous.x, position.z - previous.z).length()
                <= horizontal_bound,
            "rim cycle: no horizontal teleport"
        );
        assert!(
            (position.y - previous.y).abs() <= water_bound,
            "rim cycle: the eye stepped {} m in one frame",
            position.y - previous.y
        );
        assert!(
            !(game.swimming || game.water_exit.is_some())
                || (position.y - previous.y).abs() <= water_bound,
            "rim cycle: the water states stay bounded"
        );
        previous = position;
        if game.swimming != before_swimming {
            toggles = toggles.saturating_add(1);
            before_swimming = game.swimming;
        }
    }
    assert_eq!(cycles, 3, "three full rim cycles complete");
    assert_eq!(toggles, 6, "one entry and one completed exit per cycle");

    // The walk-in step: wade north off the step, sink and surface, climb back
    // out over the step's submerged floor (-1.85).
    let mut step_game = demo_game_at(13.0, -1.85, 16.4, 0.0, 1.0 / 60.0);
    let mut step_phase = 0_u8;
    let mut step_input = InputState::holding(&[Control::MoveForward]);
    let mut step_previous = step_game.player_position;
    let mut step_was_swimming = step_game.swimming;
    let mut step_toggles = 0_u32;
    let mut step_cycles = 0_u32;
    for _ in 0_i32..4_000_i32 {
        if step_phase == 0 && step_game.swimming {
            step_game.player_yaw = 180.0_f32.to_radians();
            step_input = InputState::holding(&[Control::MoveForward, Control::Jump]);
            step_phase = 1;
        } else if step_phase == 1
            && step_game.grounded
            && (step_game.player_floor_y - (-1.85)).abs() < 1e-3
        {
            step_cycles = step_cycles.saturating_add(1);
            if step_cycles >= 3 {
                break;
            }
            step_game.player_yaw = 0.0;
            step_input = InputState::holding(&[Control::MoveForward]);
            step_phase = 0;
        }
        step_game.update_player_movement(&mut step_input, &settings);
        let position = step_game.player_position;
        assert!(
            Vec2::new(position.x - step_previous.x, position.z - step_previous.z).length()
                <= horizontal_bound,
            "step cycle: no horizontal teleport"
        );
        assert!(
            (position.y - step_previous.y).abs() <= PLAYER_STEP_HEIGHT + 1e-3,
            "step cycle: the eye stepped {} m in one frame",
            position.y - step_previous.y
        );
        assert!(
            !(step_game.swimming || step_game.water_exit.is_some())
                || (position.y - step_previous.y).abs() <= water_bound,
            "step cycle: the water states stay bounded"
        );
        step_previous = position;
        if step_game.swimming != step_was_swimming {
            step_toggles = step_toggles.saturating_add(1);
            step_was_swimming = step_game.swimming;
        }
    }
    assert_eq!(step_cycles, 3, "three full step cycles complete");
    assert_eq!(step_toggles, 6, "one water entry and exit per step cycle");
}

/// The demo rim entry and exit are frame-rate independent: 30, 60 and 144 fps
/// move the eye at most the same delta-scaled fall/climb bound and complete on
/// the deck.
#[test]
fn water_transitions_hold_at_30_60_and_144_fps() {
    let settings = Settings::default();
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
        // The water transitions never move the eye faster than the ordinary
        // jump's take-off speed: the deck fall reaches ~4 m/s at the entry and
        // every other water state is slower (the climb is 2.2 m/s), and every
        // step is delta-scaled, so one bound holds at each rate.
        let bound = JUMP_VELOCITY.mul_add(delta, 1e-3);
        let mut game = demo_game_at(21.0, -1.5, 14.0, 270.0, delta);
        let mut phase = 0_u8;
        let mut input = InputState::holding(&[Control::MoveForward]);
        let mut previous = game.player_position;
        let mut exited = false;
        // 20 s at 144 fps covers the whole cycle at every rate; the loop
        // breaks as soon as the exit completes.
        let frames = 144_i32 * 20_i32;
        for _ in 0_i32..frames {
            if phase == 0 && game.swimming {
                game.player_yaw = 90.0_f32.to_radians();
                input = InputState::holding(&[Control::MoveForward, Control::Jump]);
                phase = 1;
            }
            game.update_player_movement(&mut input, &settings);
            let position = game.player_position;
            let dy = (position.y - previous.y).abs();
            assert!(
                dy <= bound,
                "at {:.0} fps the eye stepped {dy} m in one frame",
                1.0 / delta
            );
            assert!(
                Vec2::new(position.x - previous.x, position.z - previous.z).length()
                    <= settings.walk_speed.mul_add(delta, 1e-3),
                "at {:.0} fps the player teleported horizontally",
                1.0 / delta
            );
            previous = position;
            if phase == 1
                && game.grounded
                && (game.player_floor_y - (-1.5)).abs() < 1e-3
                && game.player_position.x > 20.0
            {
                exited = true;
                break;
            }
        }
        assert!(
            exited,
            "at {:.0} fps the exit completes on the deck: {:?} floor {}",
            1.0 / delta,
            game.player_position,
            game.player_floor_y
        );
    }
}

/// A swimmer over the submerged walk-in step stands up with no jump at all —
/// the step's floor is below the surface — in a bounded climb that ends
/// grounded on the step floor (-1.85) or, once the body has crossed it, the
/// deck past it (-1.5).
#[test]
fn a_swimmer_over_the_walk_in_step_stands_up_continuously() {
    let settings = Settings::default();
    let mut game = demo_game_at(13.0, -3.0, 14.5, 180.0, 1.0 / 60.0);
    let mut idle = InputState::default();
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert!(game.is_swimming(), "the basin water is swum");
    assert!(
        game.player_position.y < -1.65 - SWIM_FLOOR_CLEARANCE + 1e-3,
        "the swimmer is deep: {}",
        game.player_position.y
    );

    // Swim south over the step with no Jump: the support below the surface is
    // a standable exit, so the bounded climb starts by itself.
    let mut forward = InputState::holding(&[Control::MoveForward]);
    let mut previous_eye = game.player_position.y;
    let mut stood = false;
    for _ in 0_i32..400_i32 {
        game.update_player_movement(&mut forward, &settings);
        let eye = game.player_position.y;
        assert!(
            (eye - previous_eye).abs() <= WATER_EXIT_CLIMB_SPEED / 60.0 + 1e-3,
            "the stand-up is bounded: {} m in one frame",
            eye - previous_eye
        );
        previous_eye = eye;
        if game.grounded
            && ((game.player_floor_y - (-1.85)).abs() < 1e-3
                || (game.player_floor_y - (-1.5)).abs() < 1e-3)
        {
            stood = true;
            break;
        }
    }
    assert!(
        stood,
        "the swimmer stands on the submerged step or the deck past it: {:?} floor {}",
        game.player_position, game.player_floor_y
    );
    assert!(!game.is_swimming(), "the step exit leaves the swim state");
}

/// The pool deck's raised sauna landing: region 7/8/9/10 build two 0.3 m steps
/// (-1.2 then -0.9) approached from the north, west, and south, plus a
/// 45-degree diagonal across the corner.
#[test]
fn demo_sauna_landing_steps_from_three_sides_and_diagonal() {
    let riser = 0.3_f32;
    let max_step = riser + 1e-3;
    let approaches: [(f32, f32, f32, &str); 4] = [
        // x24.6 is already inside the raised shower-passage region. Approach
        // from the deck beside it so the initial feet are on the stated floor.
        (23.7, 10.3, 180.0, "north"),
        (21.7, 13.6, 90.0, "west"),
        (24.6, 16.9, 0.0, "south"),
        (21.8, 17.0, 45.0, "diagonal"),
    ];
    for (x, z, yaw, label) in approaches {
        let mut game = demo_game_at(x, -1.5, z, yaw, 1.0 / 60.0);
        let audit = audited_walk(&mut game, 160, max_step, Some(riser * 0.9), |subject| {
            (subject.player_floor_y - (-0.9)).abs() < 1e-3
        });
        audit.assert_walkable(&format!("sauna landing {label}"));
        assert_eq!(
            audit.discrete_steps, 2,
            "sauna landing {label}: two 0.3 m risers"
        );
        assert!(
            (game.player_floor_y - (-0.9)).abs() < 1e-3,
            "sauna landing {label} ends on the landing plane"
        );
    }
}

/// The sauna doorway (wall 14): the authored-open leaf passes the flush -0.9
/// floor into the sauna; a closed leaf blocks the walker without any teleport.
/// The leaf must reach both ends: the hinge sits on the opening's edge by
/// authoring contract, so the sweep must tolerate its own rebate.
#[test]
fn demo_sauna_door_passes_open_and_blocks_closed() {
    let settings = Settings::default();

    // The authored leaf starts open: cross region 8 into the sauna, flush.
    let mut game = demo_game_at(25.0, -0.9, 13.3, 90.0, 1.0 / 60.0);
    let open_pass = audited_walk(&mut game, 160, 1e-3, None, |subject| {
        subject.player_position.x > 27.2
    });
    open_pass.assert_flat("sauna door open passage");
    assert!((game.player_floor_y - (-0.9)).abs() < 1e-3);

    // Stopped inside the aperture: the doorway floor holds, no drift.
    let mut aperture_game = demo_game_at(25.9, -0.9, 13.3, 90.0, 1.0 / 60.0);
    let held = aperture_game.player_position;
    let mut idle = InputState::default();
    for _ in 0_i32..60_i32 {
        aperture_game.update_player_movement(&mut idle, &settings);
        assert!(aperture_game.grounded, "the doorway floor holds the body");
        assert!((aperture_game.player_floor_y - (-0.9)).abs() < 1e-4);
        assert!(
            (aperture_game.player_position - held).length() <= 1e-6,
            "no drift while stopped in the doorway"
        );
        assert_exact(aperture_game.vertical_velocity, 0.0);
    }

    // Close the leaf with the player clear of its sweep, then walk into it.
    let mut closed_game = demo_game_at(24.0, -0.9, 13.3, 90.0, 1.0 / 60.0);
    let leaf = closed_game
        .doors()
        .index_of("sauna_door")
        .expect("the demo authors the sauna door");
    assert!(
        closed_game
            .world_mut()
            .doors_mut()
            .request_close("sauna_door"),
        "the authored-open leaf accepts a close request"
    );
    let mut closing_idle = InputState::default();
    for _ in 0_i32..80_i32 {
        closed_game.update_player_movement(&mut closing_idle, &settings);
    }
    assert_eq!(
        closed_game
            .doors()
            .get(leaf)
            .expect("sauna leaf")
            .phase()
            .name(),
        "closed",
        "the authored-open leaf reaches its closed end"
    );
    let max_step = pushed_walk(&mut closed_game, 120);
    assert!(
        max_step <= settings.walk_speed / 60.0 + 1e-3,
        "the closed leaf never teleports the walker"
    );
    assert!(
        closed_game.player_position.x < 26.0,
        "the closed leaf stops the walker short of the sauna: {:?}",
        closed_game.player_position
    );
    assert!(closed_game.grounded);
    assert!((closed_game.player_floor_y - (-0.9)).abs() < 1e-3);

    // Back off, open it again, and cross: the doorway's floor is flush.
    closed_game.player_yaw = 270.0_f32.to_radians();
    let mut away = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..14_i32 {
        closed_game.update_player_movement(&mut away, &settings);
    }
    assert!(
        closed_game
            .world_mut()
            .doors_mut()
            .request_open("sauna_door"),
        "the closed leaf opens again"
    );
    for _ in 0_i32..120_i32 {
        closed_game.update_player_movement(&mut closing_idle, &settings);
    }
    assert_eq!(
        closed_game
            .doors()
            .get(leaf)
            .expect("sauna leaf")
            .phase()
            .name(),
        "open"
    );
    closed_game.player_yaw = 90.0_f32.to_radians();
    let through = audited_walk(&mut closed_game, 160, 1e-3, None, |subject| {
        subject.player_position.x > 27.2
    });
    through.assert_flat("sauna door reopened passage");
}

/// The Home staircase (stairs[0], 8 x 0.2625 risers at world -0.9..1.2):
/// bottom approach through the pool-hall gap, ascent, start/stop on a tread,
/// balcony landing, descent, and a 45-degree approach hugging the handrail.
#[test]
fn demo_home_staircase_audit_up_down_diagonal_and_stops() {
    let level = demo_level();
    let stair = level.stairs.first().expect("the demo has one staircase");
    let riser = stair.riser_height();
    let settings = Settings::default();
    let pitch_step = riser / stair.tread_depth() * (settings.walk_speed / 60.0);
    let max_step = riser + pitch_step + 1e-3;

    // Bottom approach through the wall-32/33 gap at x53 (header at 2.1) and up
    // the flight to the balcony region 11 at 1.2.
    let mut game = demo_game_at(52.4, -0.9, 13.3, 90.0, 1.0 / 60.0);
    let up = audited_walk(&mut game, 260, max_step, Some(riser * 0.9), |subject| {
        subject.player_position.x > 58.35
    });
    up.assert_walkable("home staircase ascent");
    assert_eq!(
        up.discrete_steps, 1,
        "only the foot's real riser is discrete"
    );
    assert!(
        (game.player_floor_y - 1.2).abs() < 1e-3,
        "the top lands flush on the balcony: {}",
        game.player_floor_y
    );

    // Start/stop on a tread: holding the pose never drifts.
    let pitch = WalkableFloor::from_level(&level)
        .walk_height_at(56.6, 13.6)
        .expect("mid-flight");
    let mut midflight_game = game_for(&level);
    play_at(&mut midflight_game, 56.6, pitch, 13.6, 90.0, 1.0 / 60.0);
    let mut idle = InputState::default();
    let held = midflight_game.player_position;
    for _ in 0_i32..60_i32 {
        midflight_game.update_player_movement(&mut idle, &settings);
        assert!(midflight_game.grounded, "a tread holds the player");
        assert!((midflight_game.player_floor_y - pitch).abs() < 1e-4);
        assert!(
            (midflight_game.player_position - held).length() <= 1e-6,
            "no drift while stopped on a tread"
        );
        assert_exact(midflight_game.vertical_velocity, 0.0);
    }

    // Resume to the balcony: every remaining tread boundary is smoothed, so
    // no further discrete riser may appear.
    let resumed = audited_walk(
        &mut midflight_game,
        160,
        max_step,
        Some(riser * 0.9),
        |subject| subject.player_position.x > 58.35,
    );
    resumed.assert_walkable("home staircase resume");
    assert_eq!(resumed.discrete_steps, 0, "no fake riser mid-flight");
    assert!((midflight_game.player_floor_y - 1.2).abs() < 1e-3);

    // Walk back down: the foot's real riser is the one discrete drop.
    midflight_game.player_yaw = 270.0_f32.to_radians();
    let down = audited_walk(
        &mut midflight_game,
        260,
        max_step,
        Some(riser * 0.9),
        |subject| subject.player_floor_y < -0.85 && subject.player_position.x < 54.4,
    );
    down.assert_walkable("home staircase descent");
    assert_eq!(
        down.discrete_steps, 1,
        "only the foot's real riser is discrete"
    );
    assert!(
        (midflight_game.player_floor_y - (-0.9)).abs() < 1e-3,
        "the descent returns to the hall floor: {}",
        midflight_game.player_floor_y
    );

    // The balcony landing is one flat plane at 1.2 north of the armchair.
    // The guardrail ends at (58.035, 13.0). Keep the radius clear of its
    // rounded corner; x58.3 initially intersects it by about 17 mm.
    let mut landing_game = demo_game_at(58.35, 1.2, 13.1, 90.0, 1.0 / 60.0);
    let landing = audited_walk(&mut landing_game, 140, 1e-3, None, |subject| {
        subject.player_position.x > 60.4
    });
    landing.assert_flat("home balcony landing");

    // A 45-degree approach: enter the lane diagonally, hug the south handrail
    // up the flight, and top out on the balcony.
    let mut diagonal_game = demo_game_at(54.2, -0.9, 12.5, 135.0, 1.0 / 60.0);
    let diagonal = audited_walk(
        &mut diagonal_game,
        260,
        max_step,
        Some(riser * 0.9),
        |subject| subject.player_position.x > 58.2,
    );
    diagonal.assert_walkable("home staircase diagonal");
    assert!(
        (diagonal_game.player_floor_y - 1.2).abs() < 1e-3,
        "the diagonal tops out on the balcony: {}",
        diagonal_game.player_floor_y
    );
}

/// The Home staircase's sides are real: the handrail and the balcony's west
/// rim together refuse a side step from the hall onto the flight, and the
/// balcony edge itself is one-way (solid from below, walked off from above).
#[test]
fn demo_home_staircase_sides_are_solid_and_rails_guard_the_lane() {
    let settings = Settings::default();

    // No lane from the hall reaches the flight's side: mid-run the north
    // handrail stops the step, and beside the top tread the balcony's west rim
    // takes over. Every lane keeps the hall floor.
    for lane_x in [56.5_f32, 57.3, 57.7] {
        let mut game = demo_game_at(lane_x, -0.9, 12.3, 180.0, 1.0 / 60.0);
        let step = pushed_walk(&mut game, 90);
        assert!(
            step <= settings.walk_speed / 60.0 + 1e-3,
            "no teleport on the lane at x {lane_x}"
        );
        assert!(
            game.player_position.z < 13.0,
            "the lane at x {lane_x} never admits a step onto the flight: {:?}",
            game.player_position
        );
        assert!(game.grounded, "the lane at x {lane_x} stays grounded");
        assert!(
            (game.player_floor_y - (-0.9)).abs() < 1e-3,
            "the lane at x {lane_x} stays on the hall floor: {}",
            game.player_floor_y
        );
    }

    // The balcony's west edge north of the flight mouth is guarded by the
    // balcony rail: walking at it stops on the balcony plane, never a drop
    // into the hall below.
    let mut game = demo_game_at(58.6, 1.2, 12.5, 270.0, 1.0 / 60.0);
    let step = pushed_walk(&mut game, 90);
    assert!(
        step <= settings.walk_speed / 60.0 + 1e-3,
        "no teleport into the balcony rail"
    );
    assert!(
        game.player_position.x > 58.2,
        "the balcony rail stops the walk: {:?}",
        game.player_position
    );
    assert!(game.grounded, "the balcony rail never loses support");
    assert!(
        (game.player_floor_y - 1.2).abs() < 1e-3,
        "the guarded walk stays on the balcony plane: {}",
        game.player_floor_y
    );
}

/// The four corridor legs of the Home lower floor are one flat plane at -0.9:
/// the north leg, the east leg, the long south leg, and the study passage
/// through the wall-36 opening.
#[test]
fn demo_home_lower_floor_corridor_legs_are_flat() {
    let legs: [(f32, f32, f32, usize, f32, f32, &str); 4] = [
        // x, z, yaw, frames, stop_z (negative = ignore), stop_x, label
        (
            60.9,
            -13.0,
            180.0,
            420,
            2.3,
            f32::NEG_INFINITY,
            "corridor north leg",
        ),
        (
            70.2,
            -13.0,
            270.0,
            240,
            f32::NEG_INFINITY,
            63.0,
            "corridor east leg",
        ),
        (
            70.0,
            4.5,
            0.0,
            480,
            -12.4,
            f32::NEG_INFINITY,
            "corridor south leg",
        ),
        (
            66.0,
            4.8,
            270.0,
            140,
            f32::NEG_INFINITY,
            63.2,
            "study passage",
        ),
    ];
    for (x, z, yaw, frames, stop_z, stop_x, label) in legs {
        let mut game = demo_game_at(x, -0.9, z, yaw, 1.0 / 60.0);
        let audit = audited_walk(&mut game, frames, 1e-3, None, |subject| {
            subject.player_position.z < stop_z || subject.player_position.x < stop_x
        });
        audit.assert_flat(label);
        assert!(
            (game.player_floor_y - (-0.9)).abs() < 1e-3,
            "{label} stays on the -0.9 lower floor"
        );
    }
}

/// The Home hall door (wall 35, passage x 60.3..61.7): the authored-closed
/// leaf blocks the corridor, and once opened the -0.9 passage is flush both
/// ways.
///
/// KNOWN DEFECT (reported 2026-09-27, `B-report.md`): on the current demo
/// source and engine, `hall_door` cannot open. Its hinge sits exactly on the
/// wall-35 opening edge, so the hinge-edge thickness corner samples inside the
/// west jamb as soon as the leaf rotates and `entities::door_pose_hits_static`
/// refuses every candidate pose. The open half of this test runs its real
/// assertions once the leaf can open; until then it proves the precise
/// geometric blocker, so the finding stays visible and the test self-upgrades
/// when the hinge or the sweep is fixed.
#[test]
fn demo_home_hall_door_blocks_closed_and_passes_open() {
    let settings = Settings::default();

    let mut game = demo_game_at(61.0, -0.9, 4.2, 0.0, 1.0 / 60.0);
    let leaf = game
        .doors()
        .index_of("hall_door")
        .expect("the demo authors the hall door");
    assert_eq!(
        game.doors().get(leaf).expect("hall leaf").phase().name(),
        "closed",
        "the hall door starts closed"
    );
    let _pushed_walk_status = pushed_walk(&mut game, 120);
    assert!(
        game.player_position.z > 3.2,
        "the closed hall door stops the walker: {:?}",
        game.player_position
    );
    assert!(game.grounded);
    assert!((game.player_floor_y - (-0.9)).abs() < 1e-3);

    // Step back clear of the leaf's own sweep before opening it (the door
    // refuses to sweep into the player's body), then cross into the corridor
    // and walk back into the living room.
    game.player_yaw = 180.0_f32.to_radians();
    let mut away = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..40_i32 {
        game.update_player_movement(&mut away, &settings);
    }
    assert!(
        game.world_mut().doors_mut().request_open("hall_door"),
        "the closed hall door accepts an open request"
    );
    let mut idle = InputState::default();
    for _ in 0_i32..80_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert_eq!(
        game.doors().get(leaf).expect("hall leaf").phase().name(),
        "open",
        "the hall leaf reaches its open end once the walker is clear"
    );

    game.player_yaw = 0.0;
    let through = audited_walk(&mut game, 200, 1e-3, None, |subject| {
        subject.player_position.z < 2.0
    });
    through.assert_flat("hall door passage");

    game.player_yaw = 180.0_f32.to_radians();
    let back = audited_walk(&mut game, 200, 1e-3, None, |subject| {
        subject.player_position.z > 4.0
    });
    back.assert_flat("hall door return");
}

/// One phase of the developer capture: a start pose, a yaw, the frame count,
/// the riser this phase crosses, and the input held.
struct CapturePhase {
    name: &'static str,
    x: f32,
    floor_y: f32,
    z: f32,
    yaw_degrees: f32,
    frames: usize,
    riser: f32,
    forward: bool,
    jump: bool,
}

/// One CSV of the developer capture: a transition id and its ordered phases.
struct CaptureTransition {
    id: &'static str,
    phases: Vec<CapturePhase>,
}

/// Phases building one capture phase row.
const fn capture_phase(
    name: &'static str,
    x: f32,
    floor_y: f32,
    z: f32,
    yaw_degrees: f32,
    frames: usize,
    riser: f32,
) -> CapturePhase {
    CapturePhase {
        name,
        x,
        floor_y,
        z,
        yaw_degrees,
        frames,
        riser,
        forward: true,
        jump: false,
    }
}

/// The complete developer capture inventory: every walkable height transition
/// of the real demo, phase by phase, mirroring the audit tests.
// one inventory table, read top to bottom
fn demo_capture_transitions() -> Vec<CaptureTransition> {
    vec![
        CaptureTransition {
            id: "office_door_and_hall_steps",
            phases: vec![
                capture_phase("office_to_landing", 17.2, 0.0, 0.9, 90.0, 130, 0.0),
                capture_phase("landing_down_to_hall", 22.0, 0.0, 0.9, 180.0, 160, 0.3),
                capture_phase("hall_up_to_landing", 22.0, -1.5, 5.6, 0.0, 170, 0.3),
                capture_phase("landing_to_office", 22.5, 0.0, 0.9, 270.0, 150, 0.0),
            ],
        },
        CaptureTransition {
            id: "office_steps_diagonal",
            phases: vec![capture_phase(
                "diagonal_to_office",
                23.0,
                -1.5,
                4.9,
                315.0,
                200,
                0.3,
            )],
        },
        CaptureTransition {
            id: "pool_walk_in_step",
            phases: vec![
                capture_phase("deck_to_step", 13.0, -1.5, 17.6, 0.0, 30, 0.35),
                capture_phase("step_to_deck", 13.0, -1.85, 16.5, 180.0, 40, 0.35),
                capture_phase("step_crossing", 17.6, -1.5, 16.45, 270.0, 160, 0.35),
            ],
        },
        CaptureTransition {
            id: "pool_basin",
            phases: vec![
                capture_phase("step_to_basin_drop", 13.0, -1.85, 16.6, 0.0, 120, 1.15),
                {
                    let mut phase =
                        capture_phase("basin_exit_and_wade", 13.0, -3.0, 14.5, 180.0, 260, 1.15);
                    phase.jump = true;
                    phase
                },
            ],
        },
        CaptureTransition {
            id: "sauna_landing_steps",
            phases: vec![
                capture_phase("landing_from_north", 24.6, -1.5, 10.3, 180.0, 90, 0.3),
                capture_phase("landing_from_west", 21.7, -1.5, 13.6, 90.0, 90, 0.3),
                capture_phase("landing_from_south", 24.6, -1.5, 16.9, 0.0, 90, 0.3),
                capture_phase("landing_diagonal", 21.8, -1.5, 17.0, 45.0, 90, 0.3),
            ],
        },
        CaptureTransition {
            id: "sauna_door_passage",
            phases: vec![capture_phase(
                "landing_to_sauna",
                25.0,
                -0.9,
                13.3,
                90.0,
                120,
                0.0,
            )],
        },
        CaptureTransition {
            id: "home_staircase",
            phases: vec![
                capture_phase("hall_gap_to_balcony", 52.4, -0.9, 13.3, 90.0, 200, 0.2625),
                capture_phase("balcony_down_to_hall", 58.5, 1.2, 13.6, 270.0, 200, 0.2625),
                capture_phase(
                    "diagonal_hugging_rail",
                    54.2,
                    -0.9,
                    12.5,
                    135.0,
                    200,
                    0.2625,
                ),
                capture_phase("balcony_lane", 58.3, 1.2, 13.1, 90.0, 120, 0.0),
            ],
        },
        CaptureTransition {
            id: "home_lower_floor",
            phases: vec![
                capture_phase("corridor_north_leg", 60.9, -0.9, -13.0, 180.0, 330, 0.0),
                capture_phase("corridor_east_leg", 70.2, -0.9, -13.0, 270.0, 160, 0.0),
                capture_phase("corridor_south_leg", 70.0, -0.9, 4.5, 0.0, 360, 0.0),
                capture_phase("study_passage", 66.0, -0.9, 4.8, 270.0, 90, 0.0),
                capture_phase("hall_door_passage", 60.9, -0.9, 4.0, 0.0, 120, 0.0),
            ],
        },
    ]
}

/// Developer capture: walk every demo height transition at a fixed 60 Hz and
/// write one CSV per transition plus a `summary.txt` reporting each phase's
/// largest per-frame eye step against the transition's authored riser.
///
/// Ignored by default because it only produces evidence; run it with the
/// output directory in the environment:
///
/// ```text
/// PLACES_STAIR_TRACE_DIR=/abs/path/traces \
///     cargo test --lib capture_demo_stair_inventory -- --ignored
/// ```
#[test]
#[ignore = "developer diagnostic; writes CSVs when PLACES_STAIR_TRACE_DIR is set"]
fn capture_demo_stair_inventory() {
    let Ok(dir) = std::env::var("PLACES_STAIR_TRACE_DIR") else {
        return;
    };
    let trace_path = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&trace_path).expect("the trace directory is creatable");
    let level = demo_level();
    let settings = Settings::default();
    let mut summary = String::new();
    for transition in demo_capture_transitions() {
        let mut csv = String::from("phase,frame,x,z,eye_y,floor_y,render_y,step_dy\n");
        let mut per_phase = Vec::new();
        for phase in &transition.phases {
            let mut game = game_for(&level);
            play_at(
                &mut game,
                phase.x,
                phase.floor_y,
                phase.z,
                phase.yaw_degrees,
                1.0 / 60.0,
            );
            let mut controls = Vec::new();
            if phase.forward {
                controls.push(Control::MoveForward);
            }
            if phase.jump {
                controls.push(Control::Jump);
            }
            let mut input = InputState::holding(&controls);
            let mut previous_eye = game.player_position.y;
            let mut max_step = 0.0_f32;
            for frame in 0..phase.frames {
                game.update_player_movement(&mut input, &settings);
                let x = game.player_position.x;
                let z = game.player_position.z;
                let eye = game.player_position.y;
                let floor = game.player_floor_y;
                let render = game.floor.height_at(x, z).unwrap_or(f32::NAN);
                let step_dy = eye - previous_eye;
                previous_eye = eye;
                max_step = max_step.max(step_dy.abs());
                writeln!(
                    csv,
                    "{},{frame},{x:.5},{z:.5},{eye:.5},{floor:.5},{render:.5},{step_dy:.6}",
                    phase.name
                )
                .expect("the trace builds in memory");
            }
            per_phase.push((phase, max_step, game.player_floor_y));
        }
        std::fs::write(trace_path.join(format!("{}.csv", transition.id)), &csv)
            .expect("the trace CSV is writable");
        for (phase, max_step, end_floor) in per_phase {
            writeln!(
                summary,
                "{} {} riser={:.4} max|step_dy|={max_step:.5} end_floor={end_floor:.4}",
                transition.id, phase.name, phase.riser
            )
            .expect("the summary builds in memory");
        }
    }
    std::fs::write(trace_path.join("summary.txt"), &summary).expect("the summary is writable");
}

// ---- The night route: the outdoor extension ---------------------------------
//
// The coordinates below mirror tools/levels/build_outdoor_route.py, which owns
// the outdoor slice: the gravel route runs north (-Z) at x = 4.5 from the front
// door, the concrete walkway is parallel at x = 13.5, the connector's centre is
// 87.4 m from the threshold, and the yard is the rectangle the invisible
// containment colliders bound.

/// The gravel route's centre line, which is also the front doorway's centre.
const NIGHT_ROUTE_X: f32 = 4.5;
/// The parallel concrete walkway's centre line.
const NIGHT_WALKWAY_X: f32 = 13.5;
/// The connector's centre: ~30 s of walking from the threshold.
const NIGHT_CONNECTOR_Z: f32 = -87.4;
/// The yard's authored outside faces: west, east and north.
const NIGHT_YARD_WEST: f32 = -0.15;
const NIGHT_YARD_EAST: f32 = 24.15;
const NIGHT_YARD_NORTH: f32 = -91.85;
/// The source house's north face: the yard's south edge.
const NIGHT_FACADE_Z: f32 = -0.15;
/// The destination house's back-wall inner face.
const NIGHT_HOUSE_BACK_Z: f32 = -96.8;

/// Walks a fully held forward input from `(x, z)` on the yard floor, one fixed
/// 1/60 s frame at a time, and stops once `stop` says so or `max_steps` frames
/// elapse. Returns the game and the simulated seconds it took.
fn demo_night_walk(
    x: f32,
    z: f32,
    yaw_degrees: f32,
    max_steps: usize,
    mut stop: impl FnMut(&Game) -> bool,
) -> (Game, f32) {
    let settings = Settings::default();
    let delta = 1.0 / 60.0;
    let mut game = demo_game_at(x, 0.0, z, yaw_degrees, delta);
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut seconds = 0.0f32;
    for _ in 0..max_steps {
        game.sim_delta_seconds = delta;
        game.update_player_movement(&mut input, &settings);
        seconds += delta;
        if stop(&game) {
            break;
        }
    }
    (game, seconds)
}

/// One minute of fixed 1/60 s frames: the timed walk's generous upper bound.
const NIGHT_WALK_STEPS: usize = 60 * 60;

/// The night route is ordinary forward walking at the shipped speed: from the
/// front-door threshold to the connector's centre is 87.4 m, measured at 29.1 s,
/// inside the requested 27-33 s window, with no lateral drift and no clutter.
#[test]
fn demo_night_route_is_a_thirty_second_walk_from_the_front_door() {
    let settings = Settings::default();
    assert!(
        (settings.walk_speed - 3.0).abs() < f32::EPSILON,
        "the shipped ordinary walk speed is 3.0 m/s, found {}",
        settings.walk_speed
    );
    // Stand just outside the threshold, facing north up the gravel route.
    let start_z = -0.2f32;
    let (game, seconds) = demo_night_walk(NIGHT_ROUTE_X, start_z, 0.0, NIGHT_WALK_STEPS, |game| {
        game.player_position.z <= NIGHT_CONNECTOR_Z
    });
    assert!(
        game.player_position.z <= NIGHT_CONNECTOR_Z,
        "the walk reaches the connector: z {} after {seconds:.2} s",
        game.player_position.z
    );
    assert!(
        (27.0..=33.0).contains(&seconds),
        "the gravel route to the connector is ~30 s of walking, measured {seconds:.2} s"
    );
    assert!(
        (game.player_position.x - NIGHT_ROUTE_X).abs() < 0.05,
        "the route is straight: x drifted to {}",
        game.player_position.x
    );
    assert!(game.grounded, "the route is walked, never fallen off");
    assert!(
        (game.player_floor_y).abs() < 1e-3,
        "the whole route is at the authored y=0: floor {}",
        game.player_floor_y
    );
    let walked = start_z - game.player_position.z;
    assert!(
        (walked - (87.4 - 0.2)).abs() < 0.6,
        "measured distance {walked:.2} m matches the authored route length"
    );
}

/// The two connections the route depends on: walking out of the front door into
/// the yard, and walking off the walkway through the destination doorway into
/// the enclosed entry room and back out.
#[test]
fn demo_night_route_walks_out_of_one_house_and_into_the_other() {
    // Out of the source house: the front door is open, so ordinary forward
    // walking from inside the front room reaches the yard.
    let (out, _) = demo_night_walk(NIGHT_ROUTE_X, 0.6, 0.0, 4 * 60, |game| {
        game.player_position.z <= -1.0
    });
    assert!(
        out.player_position.z <= -1.0,
        "the front door is walkable outward: z {}",
        out.player_position.z
    );
    assert!(out.player_floor_y.abs() < 1e-3, "the yard floor is y=0");

    // Into the destination house: the doorway is open and the entry room's own
    // floor continues the yard's y=0.
    let (inside, _) = demo_night_walk(NIGHT_WALKWAY_X, -90.4, 0.0, 4 * 60, |game| {
        game.player_position.z <= -93.0
    });
    assert!(
        inside.player_position.z <= -93.0,
        "the destination doorway is walkable inward: z {}",
        inside.player_position.z
    );
    let (settled, _) = demo_night_walk(
        NIGHT_WALKWAY_X,
        inside.player_position.z,
        0.0,
        3 * 60,
        |_| false,
    );
    assert!(
        settled.player_position.z > NIGHT_HOUSE_BACK_Z,
        "the entry room's back wall stops the walk: z {}",
        settled.player_position.z
    );
    assert!(
        settled.player_floor_y.abs() < 1e-3,
        "the entry room is at the same y=0: floor {}",
        settled.player_floor_y
    );

    // Back out again: the return route is the same door.
    let (back, _) = demo_night_walk(NIGHT_WALKWAY_X, -95.0, 180.0, 6 * 60, |game| {
        game.player_position.z >= -90.0
    });
    assert!(
        back.player_position.z >= -90.0,
        "the way back out is walkable: z {}",
        back.player_position.z
    );
}

/// The invisible containment holds against walking and jumping in every
/// direction, with no ground gap and no invisible staircase: the feet never
/// rise onto a boundary top, and the player never leaves the yard.
#[test]
fn demo_night_route_containment_holds_walking_and_jumping() {
    let settings = Settings::default();
    let delta = 1.0 / 60.0;
    let cases: [(f32, f32, f32, &str); 6] = [
        (12.0, -45.0, 90.0, "east"),
        (12.0, -45.0, 270.0, "west"),
        (12.0, -88.0, 0.0, "north"),
        (NIGHT_ROUTE_X, -88.0, 0.0, "north along the gravel route"),
        (24.0, -1.0, 135.0, "the north-east corner"),
        (0.3, -1.0, 225.0, "the north-west corner"),
    ];
    for (start_x, start_z, yaw, label) in cases {
        let mut game = demo_game_at(start_x, 0.0, start_z, yaw, delta);
        let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
        let mut max_feet = f32::MIN;
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;
        for _ in 0_i32..(60_i32 * 25_i32) {
            game.sim_delta_seconds = delta;
            game.update_player_movement(&mut input, &settings);
            max_feet = max_feet.max(game.feet_y());
            min_x = min_x.min(game.player_position.x);
            max_x = max_x.max(game.player_position.x);
            min_z = min_z.min(game.player_position.z);
            max_z = max_z.max(game.player_position.z);
        }
        let margin = 0.05;
        assert!(
            min_x >= NIGHT_YARD_WEST && max_x <= NIGHT_YARD_EAST,
            "{label}: the player stays inside the yard in x ({min_x}..{max_x})"
        );
        assert!(
            min_z >= NIGHT_YARD_NORTH && max_z <= NIGHT_FACADE_Z,
            "{label}: the player stays inside the yard in z ({min_z}..{max_z})"
        );
        // The feet may reach the ordinary jump apex above the y = 0 ground but
        // never an invisible top: the containment is solid, not a staircase.
        // The bound is the jump apex (1.0 m = the 0.9 m kitchen counter plus
        // its 0.10 m clearance), not the old 0.9 m boundary top; containment
        // itself is proven by the x/z bounds above.
        assert!(
            max_feet <= JUMP_APEX_M + margin,
            "{label}: no invisible staircase: the feet reached {max_feet}"
        );
        assert!(
            game.grounded && game.player_floor_y.abs() < 1e-3,
            "{label}: the player ends standing on the y=0 ground"
        );
    }
}

/// Every maintained door completes a full close and a full open.
///
/// The sweep is driven through the real runtime: the same
/// `Game::update_player_movement` the player runs, the same obstruction test
/// (`entities::door_pose_hits_static`), the same `Doors` state machine. A leaf
/// that cannot reach either end is a door the player can never operate, so each
/// one is driven to the closed end, back to the open end, and closed again.
#[test]
fn every_demo_door_completes_a_full_close_and_a_full_open() {
    let settings = Settings::default();
    let doors = [
        "hall_door",
        "sauna_door",
        "study_door",
        "sauna_shower_door",
        "night_source_door",
        "night_house_door",
    ];
    for id in doors {
        // The yard is empty ground, 5 m from every leaf: no door can be
        // obstructed by the player while it swings.
        let mut game = demo_game_at(2.0, 0.0, -5.0, 0.0, 1.0 / 60.0);
        let leaf = game
            .doors()
            .index_of(id)
            .expect("the demo authors the door");
        let start = game.doors().get(leaf).expect("leaf").phase();
        assert!(
            matches!(
                start,
                crate::door::DoorPhase::Open | crate::door::DoorPhase::Closed
            ),
            "{id} starts at an end: {start:?}"
        );
        let mut idle = InputState::default();
        // Two full cycles from wherever the map starts it: open the leaf, close
        // it, and repeat. Each request moves the leaf off the end it is on, so
        // every call is meaningful.
        // The first request always moves the leaf off the end it starts on.
        let first_open = matches!(start, crate::door::DoorPhase::Closed);
        for _ in 0_i32..2_i32 {
            for open in [first_open, !first_open] {
                let expected = if open { "open" } else { "closed" };
                // The request is made inside the loop: issuing both before the
                // leaf moves would reverse it before a single frame ran.
                let requested = if open {
                    game.world_mut().doors_mut().request_open(id)
                } else {
                    game.world_mut().doors_mut().request_close(id)
                };
                assert!(requested, "{id} accepts the request");
                for _ in 0_i32..600_i32 {
                    game.update_player_movement(&mut idle, &settings);
                    if game.doors().get(leaf).expect("leaf").phase().name() == expected {
                        break;
                    }
                }
                assert_eq!(
                    game.doors().get(leaf).expect("leaf").phase().name(),
                    expected,
                    "{id} reaches its {expected} end (angle {:.1})",
                    game.doors().get(leaf).expect("leaf").angle()
                );
            }
        }
        assert!(!game.doors().get(leaf).expect("leaf").is_moving());
    }
}

/// Holds the backward control for `frames` frames at the demo's fixed delta.
fn step_back(game: &mut Game, frames: usize) {
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveBackward]);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
    }
}

/// A closed destination entrance blocks the player; the same leaf opened passes.
///
/// Both night-route leaves start open, so the closed half of the cycle had
/// never been walked before: the collider follows the leaf, and the opening
/// itself is unchanged.
#[test]
fn a_closed_destination_entrance_blocks_and_an_open_one_passes() {
    let settings = Settings::default();
    let mut game = demo_game_at(13.5, 0.0, -94.2, 180.0, 1.0 / 60.0);
    let leaf = game
        .doors()
        .index_of("night_house_door")
        .expect("the demo authors the destination leaf");
    assert!(
        game.world_mut()
            .doors_mut()
            .request_close("night_house_door")
    );
    let mut idle = InputState::default();
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut idle, &settings);
        if game.doors().get(leaf).expect("leaf").phase().name() == "closed" {
            break;
        }
    }
    assert_eq!(
        game.doors().get(leaf).expect("leaf").phase().name(),
        "closed"
    );
    let max_step = pushed_walk(&mut game, 180);
    assert!(max_step < 0.1, "no teleport at the closed leaf: {max_step}");
    assert!(
        game.player_position.z < -92.05,
        "the closed leaf stops the walker inside the room: {:?}",
        game.player_position
    );
    assert!(game.grounded && game.player_floor_y.abs() < 1e-3);

    // Opened again, the same walk crosses the threshold and the yard lies
    // beyond. The walker steps back first: the leaf swings into the room and
    // refuses to sweep through a body standing in the opening.
    step_back(&mut game, 60);
    assert!(
        game.world_mut()
            .doors_mut()
            .request_open("night_house_door")
    );
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut idle, &settings);
        if game.doors().get(leaf).expect("leaf").phase().name() == "open" {
            break;
        }
    }
    assert_eq!(game.doors().get(leaf).expect("leaf").phase().name(), "open");
    // The walk crosses the doorway's 0.24 m stoop, so the eye's bounded step
    // is the stoop height plus a frame of slack, not the flat 1 mm.
    let stoop_step = 0.24 + 1e-3;
    let through = audited_walk(&mut game, 240, stoop_step, None, |subject| {
        subject.player_position.z > -90.5
    });
    through.assert_walkable("open night doorway");
    assert!(
        through.max_eye_step <= stoop_step + 1e-3,
        "the walk over the stoop never steps more than the stoop height: {}",
        through.max_eye_step
    );
    assert!(
        game.player_position.z > -90.5,
        "the open leaf leaves a traversable opening: {:?}",
        game.player_position
    );
}

/// The source entrance closes into the doorway and blocks the yard walker.
#[test]
fn a_closed_source_entrance_blocks_and_an_open_one_passes() {
    let settings = Settings::default();
    let mut game = demo_game_at(4.5, 0.0, -2.4, 180.0, 1.0 / 60.0);
    let leaf = game
        .doors()
        .index_of("night_source_door")
        .expect("the demo authors the source leaf");
    assert!(
        game.world_mut()
            .doors_mut()
            .request_close("night_source_door")
    );
    let mut idle = InputState::default();
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut idle, &settings);
        if game.doors().get(leaf).expect("leaf").phase().name() == "closed" {
            break;
        }
    }
    assert_eq!(
        game.doors().get(leaf).expect("leaf").phase().name(),
        "closed"
    );
    let max_step = pushed_walk(&mut game, 180);
    assert!(max_step < 0.1, "no teleport at the closed leaf: {max_step}");
    assert!(
        game.player_position.z < 0.15,
        "the closed source leaf stops the walker at the doorway: {:?}",
        game.player_position
    );
    assert!(game.grounded && game.player_floor_y.abs() < 1e-3);

    // This leaf swings out into the yard, so the walker clears it before it
    // opens.
    step_back(&mut game, 60);
    assert!(
        game.world_mut()
            .doors_mut()
            .request_open("night_source_door")
    );
    for _ in 0_i32..600_i32 {
        game.update_player_movement(&mut idle, &settings);
        if game.doors().get(leaf).expect("leaf").phase().name() == "open" {
            break;
        }
    }
    assert_eq!(game.doors().get(leaf).expect("leaf").phase().name(), "open");
    let through = audited_walk(&mut game, 240, 1e-3, None, |subject| {
        subject.player_position.z > 2.0
    });
    through.assert_flat("open source doorway");
    assert!(
        game.player_position.z > 2.0,
        "the open source leaf lets the player through: {:?}",
        game.player_position
    );
}

/// Every demo door frames the tunnel it is actually installed in.
///
/// The frame's liner and casings are placed from the resolved wall, so a leaf
/// whose opening the resolver misses would draw a standalone liner buried
/// inside a thick wall — the pool-side sauna leaf's raised sill used to do
/// exactly that. Each entry pins the depth and the liner's offset from the leaf
/// plane for the shipped demo.
#[test]
fn every_demo_door_frames_the_wall_it_sits_in() {
    let level = demo_level();
    let expected: [(&str, f32, f32); 7] = [
        ("hall_door", 0.3, 0.0),
        ("sauna_door", 0.3, 0.08),
        ("study_door", 0.3, 0.0),
        ("sauna_shower_door", 0.3, 0.0),
        // The additional corridor-side sauna leaf sits in wall 17's own
        // 0.30 m opening, so its liner resolves to that wall.
        ("sauna_hall_door", 0.3, 0.0),
        // The two night-route entrances author the façade's visible tunnel: the
        // 0.30 m wall plus the 0.40 m doorway panel in front of it.
        ("night_source_door", 0.7, -0.2),
        ("night_house_door", 0.7, 0.2),
    ];
    assert_eq!(
        level.doors.len(),
        expected.len(),
        "the demo's door inventory"
    );
    for (id, depth, center) in expected {
        let def = level
            .doors
            .iter()
            .find(|door| door.id == id)
            .unwrap_or_else(|| panic!("the demo authors `{id}`"));
        let frame = level.door_frame(def);
        assert!(
            (frame.depth - depth).abs() < 1e-4,
            "{id} liner depth {} (expected {depth})",
            frame.depth
        );
        assert!(
            (frame.center - center).abs() < 1e-4,
            "{id} liner offset {} (expected {center})",
            frame.center
        );
    }
}

/// Distance from `point` to the axis-aligned box `[min, max]`, in metres;
/// zero when the point is inside.
fn point_box_distance(point: Vec3, min: [f32; 3], max: [f32; 3]) -> f32 {
    let dx = point.x - point.x.clamp(min[0], max[0]);
    let dy = point.y - point.y.clamp(min[1], max[1]);
    let dz = point.z - point.z.clamp(min[2], max[2]);
    (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt()
}

/// Distance from `point` to a door leaf's oriented box, in metres; zero when
/// the point is inside the leaf.
fn point_leaf_distance(door: &crate::collision::DoorCollider, point: Vec3) -> f32 {
    let dx = point.x - door.hinge_x;
    let dz = point.z - door.hinge_z;
    let u = dx.mul_add(door.dir_x, dz * door.dir_z);
    let v = dx.mul_add(-door.dir_z, dz * door.dir_x);
    let half_thickness = door.thickness * 0.5;
    let du = u - u.clamp(0.0, door.width);
    let dv = v - v.clamp(-half_thickness, half_thickness);
    let dy = point.y - point.y.clamp(door.hinge_y, door.hinge_y + door.height);
    (du.mul_add(du, dv.mul_add(dv, dy * dy))).sqrt()
}

/// Describes how `game`'s rendered camera sits outside the walkable world, if
/// it does: inside a collision box, inside a door leaf, below the walkable
/// floor under it, above the local ceiling, or within the projection near
/// plane ([`SCENE_NEAR_M`]) of any of those surfaces.
///
/// The near-plane distance is the rendered guarantee: geometry closer than it
/// is clipped by the projection, so the camera must never be within one near
/// plane of a wall, a leaf, the floor under it or the local ceiling. A camera
/// exactly on a surface plane is a see-through failure, not a margin case.
fn camera_outside_world(game: &Game) -> Option<String> {
    let eye = game.player_position;
    let margin = 1.0e-3;
    let clearance = SCENE_NEAR_M - margin;
    for wall in game.walls() {
        if eye.x > wall.min_x + margin
            && eye.x < wall.max_x - margin
            && eye.y > wall.min_y + margin
            && eye.y < wall.max_y - margin
            && eye.z > wall.min_z + margin
            && eye.z < wall.max_z - margin
        {
            return Some(format!("inside wall box {wall:?}"));
        }
        let distance = point_box_distance(
            eye,
            [wall.min_x, wall.min_y, wall.min_z],
            [wall.max_x, wall.max_y, wall.max_z],
        );
        if distance < clearance {
            return Some(format!("eye within {distance:.3} m of wall box {wall:?}"));
        }
    }
    for (index, door) in game.door_colliders().iter().enumerate() {
        if door.contains_point(eye.x, eye.y, eye.z) {
            return Some(format!("inside door leaf {index}"));
        }
        let distance = point_leaf_distance(door, eye);
        if distance < clearance {
            return Some(format!("eye within {distance:.3} m of door leaf {index}"));
        }
    }
    if let Some(surface) = game.floor.walk_height_at(eye.x, eye.z) {
        if eye.y < surface - margin {
            return Some(format!(
                "eye {:.3} below the walkable floor {surface:.3} at ({:.2},{:.2})",
                eye.y, eye.x, eye.z
            ));
        }
        if eye.y - surface < clearance {
            return Some(format!(
                "eye {:.3} above the walkable floor {surface:.3} at ({:.2},{:.2}): \
                 nearer than the {SCENE_NEAR_M} m near plane",
                eye.y - surface,
                eye.x,
                eye.z
            ));
        }
    }
    if let Some(ceiling) = game.ceiling.ceiling_y_at(eye.x, eye.z) {
        if eye.y > ceiling + margin {
            return Some(format!("eye {:.3} above the ceiling {ceiling:.3}", eye.y));
        }
        if ceiling - eye.y < clearance {
            return Some(format!(
                "eye {:.3} below the ceiling {ceiling:.3}: nearer than the \
                 {SCENE_NEAR_M} m near plane",
                ceiling - eye.y
            ));
        }
    }
    // The pull-up holds the eye one near plane above a raised rim until the
    // swim step carries the centre over it: the body climbing past the hold
    // while it is still in the water is the old unbounded climb, which ran
    // the camera up at the climb speed for as long as the key stayed down.
    if let Some(exit) = game.water_exit
        && exit.support_y > exit.surface_y
        && !game
            .floor
            .walk_height_at(eye.x, eye.z)
            .is_some_and(|support| (support - exit.support_y).abs() <= margin)
        && eye.y > exit.support_y + SCENE_NEAR_M + PLAYER_RADIUS
    {
        return Some(format!(
            "the pull-up rose past its hold line: eye {:.3}, rim {:.3}",
            eye.y, exit.support_y
        ));
    }
    None
}

/// Runs `frames` Playing frames, asserting every frame that the rendered
/// camera stays inside the walkable world. Returns true when `stop` fired.
fn run_camera_clear(
    game: &mut Game,
    input: &mut InputState,
    settings: &Settings,
    frames: usize,
    mut stop: impl FnMut(&Game) -> bool,
) -> bool {
    for _ in 0..frames {
        game.update_player_movement(input, settings);
        if let Some(what) = camera_outside_world(game) {
            panic!(
                "the camera left the world: {what}; eye {:?}; feet {:.3}; floor {:.3}; \
                 swimming {} exit {}",
                game.player_position,
                game.feet_y(),
                game.player_floor_y,
                game.swimming,
                game.water_exit.is_some()
            );
        }
        if stop(game) {
            return true;
        }
    }
    false
}

/// The demo pool exit never puts the rendered camera inside geometry.
///
/// Straight and diagonal approaches, a submerged start, a crouched exit and
/// the walk-in step are all sampled frame by frame, and the camera must stay
/// clear of every collision box, door leaf, the floor under it and the local
/// ceiling by the projection near plane. The old climb carried the centre
/// onto the deck (and the walk-in step) while the eye was still below the rim
/// top, so the camera sat inside the rim's collision volume for up to ~0.35 s;
/// the bounded pull-up lifts the eye at the rim first and the swim step
/// crosses only once the camera has cleared it. This test fails on that old
/// code.
#[test]
fn the_demo_pool_exit_never_puts_the_camera_inside_geometry() {
    let settings = Settings::default();
    // Out of the basin and standing on the -1.5 m deck: a diagonal approach
    // legitimately leaves over whichever rim it reaches first, so the stop is
    // "not over the basin", not a specific x.
    let over_basin = |x: f32, z: f32| (8.0..20.0).contains(&x) && (10.0..16.0).contains(&z);
    let on_deck = |game: &Game| {
        game.grounded
            && (game.player_floor_y - (-1.5)).abs() < 1e-3
            && !over_basin(game.player_position.x, game.player_position.z)
    };
    let mut idle = InputState::default();

    // Surface and submerged approaches into the east rim, straight and both
    // diagonals, at every supported frame rate.
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
        for (x, z, yaw) in [(12.0, 14.0, 90.0), (12.0, 13.0, 45.0), (12.0, 15.0, 135.0)] {
            let mut game = demo_game_at(x, -3.0, z, yaw, delta);
            for _ in 0_i32..240_i32 {
                game.update_player_movement(&mut idle, &settings);
            }
            assert!(
                game.is_swimming(),
                "the approach starts deep in the basin: {:?}",
                game.player_position
            );
            let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
            assert!(
                run_camera_clear(&mut game, &mut input, &settings, 900, on_deck),
                "the exit at ({x},{z}) yaw {yaw} completes at {:.0} fps: {:?} floor {}",
                1.0 / delta,
                game.player_position,
                game.player_floor_y
            );
        }
    }

    // Crouched: the crouched swim pose and climb must clear the rim too.
    let mut game = demo_game_at(12.0, -3.0, 14.0, 90.0, 1.0 / 60.0);
    let mut crouch = InputState::holding(&[Control::Crouch]);
    game.update_player_movement(&mut crouch, &settings);
    for _ in 0_i32..30_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert!(game.is_crouched(), "the crouched pose settles");
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    assert!(
        run_camera_clear(&mut game, &mut input, &settings, 900, |subject| {
            subject.grounded
                && (subject.player_floor_y - (-1.5)).abs() < 1e-3
                && subject.player_position.x > 20.0
                && subject.is_crouched()
        }),
        "the crouched exit completes: {:?} floor {}",
        game.player_position,
        game.player_floor_y
    );

    // The submerged walk-in step, with no Jump: the pull-up lifts the eye at
    // the step edge before the body crosses it, and the climb then stands the
    // body up on the deck past the step (a higher standable support under the
    // centre is adopted, never cancelled with the body embedded in it).
    let mut step_exit_game = demo_game_at(13.0, -3.0, 14.5, 180.0, 1.0 / 60.0);
    for _ in 0_i32..240_i32 {
        step_exit_game.update_player_movement(&mut idle, &settings);
    }
    assert!(
        step_exit_game.is_swimming(),
        "the step approach starts deep"
    );
    let mut forward = InputState::holding(&[Control::MoveForward]);
    assert!(
        run_camera_clear(
            &mut step_exit_game,
            &mut forward,
            &settings,
            600,
            |subject| {
                subject.grounded
                    && subject.player_position.z > 16.0
                    && subject.player_floor_y >= -1.85 - 1e-3
            }
        ),
        "the walk-in step exit completes: {:?} floor {}",
        step_exit_game.player_position,
        step_exit_game.player_floor_y
    );
    assert!(
        (step_exit_game.player_floor_y + 1.85).abs() < 1e-3
            || (step_exit_game.player_floor_y + 1.5).abs() < 1e-3,
        "the walk-in exit ends on the submerged step or the deck: floor {}",
        step_exit_game.player_floor_y
    );
}

/// The demo pool ladder climbs to the deck without ever putting the camera
/// inside the deck's collision volume.
#[test]
fn the_demo_pool_ladder_exit_never_puts_the_camera_inside_geometry() {
    let settings = Settings::default();
    let mut game = demo_game_at(18.0, -3.0, 12.0, 90.0, 1.0 / 60.0);
    let mut idle = InputState::default();
    for _ in 0_i32..240_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert!(game.is_swimming(), "the approach starts in the water");
    let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
    let mut attached = false;
    assert!(
        run_camera_clear(&mut game, &mut input, &settings, 600, |subject| {
            attached |= subject.is_climbing();
            attached && subject.grounded && (subject.player_floor_y - (-1.5)).abs() < 1e-3
        }),
        "the ladder climb tops out on the deck: {:?} floor {}",
        game.player_position,
        game.player_floor_y
    );
}

/// Runs the released-Jump sink on the pool level at `delta`, returning
/// `(seconds from the float line to the -1.5 m target, terminal rate over the
/// measured segment)`. Releasing Jump is the only input; the held float pose
/// is seeded directly, so this is the sink path, not the entry.
fn released_sink_segment(delta: f32) -> (f32, f32) {
    let settings = Settings::default();
    let start_eye = -0.5 + FLOAT_EYE_MARGIN;
    // Past this depth the sink has long reached its terminal: gravity 4.5
    // spends 0.24 s and 0.13 m reaching 1.1 m/s, so a 0.42 m drop is terminal.
    let terminal_eye = -0.8_f32;
    let target_eye = -1.5_f32;
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 7.0, -3.0, 4.0, 0.0, delta);
    game.swimming = true;
    game.grounded = false;
    game.float_hold = false;
    game.bob_phase = 0.0;
    game.vertical_velocity = 0.0;
    game.vertical_accumulator = 0.0;
    set_eye_y(&mut game, start_eye);

    let mut idle = InputState::default();
    let mut seconds = 0.0_f32;
    let mut reached_terminal: Option<f32> = None;
    let mut reached = false;
    for _ in 0_i32..2_000_i32 {
        game.update_player_movement(&mut idle, &settings);
        seconds += delta;
        assert!(
            game.player_position.y >= -3.0 + SWIM_FLOOR_CLEARANCE - 1e-3,
            "the swimmer never passes the basin floor: {}",
            game.player_position.y
        );
        if game.player_position.y <= terminal_eye && reached_terminal.is_none() {
            reached_terminal = Some(seconds);
        }
        if game.player_position.y <= target_eye {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "the released swimmer sinks past {target_eye} at {:.0} fps: {}",
        1.0 / delta,
        game.player_position.y
    );
    assert!(
        game.player_position.y >= SWIM_SINK_TERMINAL.mul_add(-delta, target_eye) - 1e-3,
        "the sink stops at the target, not past it: {}",
        game.player_position.y
    );
    let Some(terminal_time) = reached_terminal else {
        panic!(
            "the sink never reached its terminal rate at {:.0} fps",
            1.0 / delta
        );
    };
    let rate = (terminal_eye - target_eye) / (seconds - terminal_time);
    // One coarse frame of sampling error at either end of the measured
    // segment; the old 0.5 m/s terminal is far outside it.
    assert!(
        (0.9..=1.35).contains(&rate),
        "the terminal sink is the new 1.1 m/s rate, not a crawl or a snap: \
         {rate:.3} m/s at {:.0} fps",
        1.0 / delta
    );
    (seconds, rate)
}

/// Releasing Jump sinks under the stronger underwater gravity at the same
/// trajectory at 30, 60 and 144 fps: the fixed substep shares one integration
/// sequence, so the time from the float line to a given depth matches within
/// one coarse frame, the terminal rate is the new 1.1 m/s, and the descent is
/// materially faster than the old 0.5 m/s crawl.
#[test]
fn sinking_in_deep_water_is_frame_rate_independent() {
    let settings = Settings::default();
    let start_eye = -0.5 + FLOAT_EYE_MARGIN;
    let target_eye = -1.5_f32;
    let mut times = Vec::new();
    let mut rates = Vec::new();
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
        let (time, rate) = released_sink_segment(delta);
        times.push(time);
        rates.push(rate);
    }
    let low = times.iter().copied().fold(f32::INFINITY, f32::min);
    let high = times.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        high - low <= 1.0 / 30.0 + 1e-3,
        "the sink time is frame-rate independent: {times:?}"
    );
    // The old constants (gravity -2.2, terminal 0.5) take this 1.12 m drop in
    // about 2.35 s; the stronger sink must finish in well under three quarters
    // of that, which no 0.5 m/s terminal could.
    let old_time = {
        let accel = 2.2_f32;
        let old_terminal = 0.5_f32;
        let drop = start_eye - target_eye;
        let spin_up = old_terminal / accel;
        let spun = 0.5 * accel * spin_up * spin_up;
        spin_up + (drop - spun) / old_terminal
    };
    assert!(
        high < old_time * 0.75,
        "the unassisted sink is materially faster than the old {old_time:.2} s crawl: \
         {high:.3} s of {old_time:.2} s"
    );
    // The measured segment rates confirm the same terminal at every frame
    // rate, not just a matching overall time.
    assert!(
        rates
            .iter()
            .all(|rate| (rate - SWIM_SINK_TERMINAL).abs() < 0.25),
        "every frame rate sinks at the authored terminal: {rates:?}"
    );

    // And the sink stays smooth: no frame moves the eye more than the
    // terminal speed times the frame.
    let level = pool_level();
    let mut game = game_for(&level);
    play_at(&mut game, 7.0, -3.0, 4.0, 0.0, 1.0 / 60.0);
    game.swimming = true;
    game.grounded = false;
    game.float_hold = false;
    set_eye_y(&mut game, start_eye);
    let mut idle = InputState::default();
    let mut previous = game.player_position.y;
    for _ in 0_i32..200_i32 {
        game.update_player_movement(&mut idle, &settings);
        let eye = game.player_position.y;
        assert!(
            (eye - previous).abs() <= SWIM_SINK_TERMINAL / 60.0 + 1e-3,
            "the sink is bounded per frame: {previous} then {eye}"
        );
        previous = eye;
    }
}

/// A scripted `interact@…` press closes the open hall door, exactly as the
/// capture harness drives it.
///
/// This mirrors the real frame order (`update_player_movement` latches the
/// rising edge, `take_interact_press` consumes it, `dispatch_interaction`
/// resolves the aim) minus the SDL focus gate, so a failure here is an engine
/// fault and a pass points at the window layer.
#[test]
fn a_scripted_interact_press_closes_the_open_hall_door() {
    let settings = Settings::default();
    let mut game = demo_game_at(61.6, -0.9, 1.2, 216.0, 1.0 / 60.0);
    let leaf = game.doors().index_of("hall_door").expect("door");
    // The wall switch beside the door opens it, the way PLACES_INTERACT does.
    let switch = game
        .world()
        .interactables()
        .index_of("hall_switch")
        .expect("the demo authors the hall switch");
    let report = game.world_mut().dispatch_interaction(Some(switch));
    assert!(report.is_some(), "the switch dispatches");
    let mut input = InputState::default();
    let (script, rejected) = crate::input::parse_move_script("interact@3.0-3.1");
    assert!(rejected.is_empty(), "{rejected:?}");
    let mut pressed_frames = 0_i32;
    let mut prompt_at_press: Option<String> = None;
    for frame in 0_u16..240 {
        let seconds = f32::from(frame) / 60.0;
        input.apply_move_script(&script, seconds);
        game.update_player_movement(&mut input, &settings);
        if game.take_interact_press() {
            pressed_frames += 1_i32;
            let target = game.interaction_target();
            prompt_at_press = target
                .and_then(|index| game.world().interactables().get(index))
                .map(|item| item.prompt.clone());
            let _dispatch_report = game.dispatch_interaction();
        }
    }
    assert_eq!(pressed_frames, 1_i32, "one press edge");
    assert_eq!(
        prompt_at_press.as_deref(),
        Some("Hall door"),
        "the press aims at the hall door"
    );
    assert_eq!(
        game.doors().get(leaf).expect("leaf").phase().name(),
        "closed",
        "the press closes the open leaf (angle {:.1})",
        game.doors().get(leaf).expect("leaf").angle()
    );
}

/// One Interact press while aiming at the demo's `sauna_switch` plays its own
/// lever clip and flips its switchable lamp; the authored-open sauna leaf
/// keeps its own interaction and is never driven by the switch.
#[test]
fn demo_sauna_switch_toggles_only_its_lamp() {
    let settings = Settings::default();
    let mut game = demo_game_at(27.0, -0.9, 14.3, 0.0, 1.0 / 60.0);
    let switch = game
        .interactables()
        .index_of("sauna_switch")
        .expect("the demo authors the sauna switch");
    let leaf = game
        .doors()
        .index_of("sauna_door")
        .expect("the demo authors the sauna door");
    let lamp = game
        .entities()
        .handle_of("sauna_light")
        .expect("the demo authors the sauna lamp");
    let lamp_on = |subject: &Game| {
        subject
            .entities()
            .components()
            .lights
            .get(lamp)
            .expect("the light component")
            .enabled
    };
    assert!(lamp_on(&game), "the sauna lamp starts on");

    aim_at(&mut game, 26.16, 14.3, 0.39);
    assert_eq!(
        game.interaction_target(),
        Some(switch),
        "the switch is the aimed target on the door's opposite side"
    );
    let mut input = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.take_interact_press(), "the press latches");
    let report = game
        .dispatch_interaction()
        .expect("the switch fires its own bindings");
    assert_eq!(report.actions_run, 2, "the lever clip and the lamp toggle");
    assert!(!lamp_on(&game), "the lamp flipped off");
    let toggles = game.take_light_toggles();
    assert_eq!(toggles.len(), 1, "exactly one fixture changed: {toggles:?}");
    assert!(
        !toggles.first().expect("one toggle").1,
        "the sauna lamp is the fixture that changed"
    );
    assert!(game.take_light_toggles().is_empty(), "and never repeats");
    assert_eq!(
        game.doors().get(leaf).expect("sauna leaf").phase().name(),
        "open",
        "the switch never drives the door"
    );
}

/// One Interact press while aiming at the open `sauna_door` toggles the leaf
/// and never touches the switch's lamp: the leaf's own binding is the only one
/// that runs.
#[test]
fn demo_sauna_door_toggles_only_itself() {
    let settings = Settings::default();
    // The eye stays outside the leaf's swing arc: the leaf is aimed, but the
    // closing sweep never meets the body (or the leaf would stop, as authored).
    let mut game = demo_game_at(27.6, -0.9, 12.0, 0.0, 1.0 / 60.0);
    let leaf = game
        .doors()
        .index_of("sauna_door")
        .expect("the demo authors the sauna door");
    let door_target = game
        .interactables()
        .index_of("sauna_door")
        .expect("the open leaf is an interaction target");
    let lamp = game
        .entities()
        .handle_of("sauna_light")
        .expect("the demo authors the sauna lamp");
    let lamp_on = |subject: &Game| {
        subject
            .entities()
            .components()
            .lights
            .get(lamp)
            .expect("the light component")
            .enabled
    };
    assert!(lamp_on(&game), "the sauna lamp starts on");

    aim_at(&mut game, 27.0, 12.42, 0.15);
    assert_eq!(
        game.interaction_target(),
        Some(door_target),
        "the open leaf is aimed, not the switch"
    );
    let mut input = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.take_interact_press(), "the press latches");
    let report = game
        .dispatch_interaction()
        .expect("the leaf fires its own binding");
    assert_eq!(report.actions_run, 1, "one toggle action ran");
    assert_eq!(
        game.doors().get(leaf).expect("sauna leaf").phase().name(),
        "closing",
        "the leaf toggled from open"
    );
    assert!(
        game.take_light_toggles().is_empty(),
        "the leaf never drives the lamp"
    );
    assert!(lamp_on(&game), "the lamp stays on");

    let mut idle = InputState::default();
    for _ in 0_i32..120_i32 {
        game.update_player_movement(&mut idle, &settings);
    }
    assert_eq!(
        game.doors().get(leaf).expect("sauna leaf").phase().name(),
        "closed",
        "the leaf reaches its closed end"
    );
    assert!(lamp_on(&game), "the lamp is still a separate target");
}

/// The aimed target respects occlusion, reach and a nearer door leaf on the
/// line: from the sauna the relay switch resolves, the pool-deck wall occludes
/// it, its 1.6 m reach is honoured, and the open leaf's conservative bound
/// never steals a clear line to the switch on the door's opposite side.
#[test]
fn demo_sauna_switch_aim_respects_occlusion_and_reach() {
    let switch = {
        let level = demo_level();
        let game = game_for(&level);
        game.interactables()
            .index_of("sauna_switch")
            .expect("the demo authors the sauna switch")
    };
    let door_target = {
        let level = demo_level();
        let game = game_for(&level);
        game.interactables()
            .index_of("sauna_door")
            .expect("the demo authors the sauna door")
    };

    // In the sauna south of the doorway, the line to the switch is clear: the
    // open leaf is north of it, so nothing may steal the aim.
    let mut game = demo_game_at(26.9, -0.9, 14.3, 0.0, 1.0 / 60.0);
    aim_at(&mut game, 26.16, 14.3, 0.39);
    let ray_origin = game.player_position;
    let ray_direction = game.view_direction();
    let leaf_index = game
        .doors()
        .index_of("sauna_door")
        .expect("the demo authors the sauna door");
    let leaf = *game
        .door_colliders()
        .get(leaf_index)
        .expect("the sauna leaf collider");
    assert_eq!(
        leaf.ray_entry(ray_origin, ray_direction, 4.0),
        None,
        "the precondition: the open leaf is not on the line"
    );
    let door_item = game
        .interactables()
        .get(door_target)
        .expect("the door item");
    assert_eq!(
        crate::collision::ray_aabb_entry(
            ray_origin,
            ray_direction,
            door_item.bounds.min,
            door_item.bounds.max
        ),
        None,
        "the precondition: not even the leaf's conservative bound is crossed"
    );
    assert_eq!(
        game.interaction_target(),
        Some(switch),
        "the switch resolves with the open leaf beside it"
    );

    // From the pool deck the sauna wall is between the eye and the switch:
    // occluded targets are never aimed even inside their reach.
    let mut occluded_game = demo_game_at(25.4, -0.9, 14.3, 0.0, 1.0 / 60.0);
    aim_at(&mut occluded_game, 26.16, 14.3, 0.39);
    assert_eq!(
        occluded_game.interaction_target(),
        None,
        "the wall occludes the switch from the deck"
    );

    // The switch's authored reach is 1.6 m: from further back it is not a
    // candidate at all.
    let mut distant_game = demo_game_at(28.6, -0.9, 14.3, 0.0, 1.0 / 60.0);
    aim_at(&mut distant_game, 26.16, 14.3, 0.39);
    assert_eq!(
        distant_game.interaction_target(),
        None,
        "the switch is out of reach from 2.4 m"
    );

    // Aiming at the open leaf from just inside the doorway resolves the leaf,
    // and the switch stays a separate target.
    let mut nearby_door_game = demo_game_at(27.4, -0.9, 12.9, 0.0, 1.0 / 60.0);
    aim_at(&mut nearby_door_game, 27.0, 12.45, 0.39);
    assert_eq!(
        nearby_door_game.interaction_target(),
        Some(door_target),
        "the open leaf is the aimed target when it is on the line"
    );
}

// ---------------------------------------------------------------------------
// reply-02 content: the house guards, the hot tub and the sauna steam switch
// ---------------------------------------------------------------------------

/// Holds one control for a fixed number of frames at the test delta.
fn hold_for(game: &mut Game, control: Control, frames: usize) {
    let settings = Settings::default();
    let mut input = InputState::holding(&[control]);
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
    }
}

/// The guards' animation state and per-clip progress, by instance id.
fn guard_state(game: &Game, guards: &[&str]) -> Vec<(bool, f32)> {
    guards
        .iter()
        .map(|id| {
            game.entities()
                .handle_of(id)
                .and_then(|handle| game.entities().components().animations.get(handle))
                .map_or((false, 0.0), |animation| {
                    (animation.playing, animation.progress)
                })
        })
        .collect()
}

/// The three waiting skeletons inside the destination house start their shared
/// `collapse_reassemble` clip together on the first entry and refuse to restart
/// while the clip runs (leave and re-enter mid-clip).
#[test]
fn demo_house_guards_wake_once_per_entry_without_restarting_mid_clip() {
    const DELTA: f32 = 1.0 / 60.0;
    let guards = ["night_guard_a", "night_guard_b", "night_guard_c"];

    // Stand on the stoop outside the open doorway, facing into the house.
    let mut game = demo_game_at(13.5, 0.24, -90.6, 0.0, DELTA);
    assert_eq!(
        guard_state(&game, &guards)
            .iter()
            .filter(|(playing, _)| *playing)
            .count(),
        0,
        "the guards wait before entry"
    );
    assert!(
        guards.iter().all(|id| game
            .entities()
            .handle_of(id)
            .and_then(|handle| game.entities().components().transforms.get(handle))
            .is_some_and(|transform| transform.position.y.abs() < 1e-3)),
        "the guards stand on the house floor"
    );

    let settings = Settings::default();
    let mut inside = InputState::holding(&[Control::MoveForward]);
    let mut woken = false;
    for _ in 0_i32..180_i32 {
        game.update_player_movement(&mut inside, &settings);
        if guard_state(&game, &guards)
            .iter()
            .all(|(playing, _)| *playing)
        {
            woken = true;
            break;
        }
    }
    assert!(
        woken,
        "all three guards start their clips together: {:?}",
        game.player_position
    );
    assert_eq!(
        guard_state(&game, &guards)
            .iter()
            .filter(|(playing, _)| *playing)
            .count(),
        3
    );

    // Leave while the clip runs, then re-enter: the running clip is never
    // restarted (its progress never drops) and the sequence does not re-run.
    let before = guard_state(&game, &guards);
    hold_for(&mut game, Control::MoveBackward, 150);
    assert!(
        game.player_position.z > -91.9,
        "the player is back outside: {:?}",
        game.player_position
    );
    hold_for(&mut game, Control::MoveForward, 90);
    let after = guard_state(&game, &guards);
    for ((_, a), (_, b)) in after.iter().zip(before.iter()) {
        assert!(
            *b <= a + 1e-4,
            "the running clip was never restarted: {before:?} -> {after:?}"
        );
    }
}

/// After the player exits and the shared sequence completes, a fresh entry
/// re-arms the guards: the sequence is idle and the three clips start again.
#[test]
fn demo_house_guards_rearm_after_exit_and_completion() {
    const DELTA: f32 = 1.0 / 60.0;
    let guards = ["night_guard_a", "night_guard_b", "night_guard_c"];
    let mut game = demo_game_at(13.5, 0.24, -90.6, 0.0, DELTA);
    let zone = game
        .entities()
        .handle_of("night_guard_zone")
        .expect("the entry zone entity");
    let sequence_running = |subject: &Game| {
        subject
            .entities()
            .components()
            .sequences
            .get(zone)
            .is_some_and(|sequence| sequence.running)
    };
    hold_for(&mut game, Control::MoveForward, 180);
    assert!(
        guard_state(&game, &guards)
            .iter()
            .all(|(playing, _)| *playing),
        "the entry starts the guards"
    );
    assert!(sequence_running(&game), "the shared sequence is running");

    // Step outside and let the shared sequence's holds finish.
    hold_for(&mut game, Control::MoveBackward, 150);
    assert!(game.player_position.z > -91.9, "the player is outside");
    let settings = Settings::default();
    let mut idle = InputState::default();
    let mut completed = false;
    for _ in 0_i32..(60_i32 * 16_i32) {
        game.update_player_movement(&mut idle, &settings);
        if !sequence_running(&game) {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "the shared sequence ran to completion at rest: running={}",
        sequence_running(&game)
    );

    // A fresh entry after the exit and completion re-arms the guards.
    hold_for(&mut game, Control::MoveForward, 240);
    assert!(
        guard_state(&game, &guards)
            .iter()
            .all(|(playing, _)| *playing),
        "a fresh entry re-arms all three guards"
    );
    assert!(
        sequence_running(&game),
        "the sequence restarted for the entry"
    );
}

/// The hot tub's water is a disc, not a square: the centre and the axis points
/// are water, the bounding box's corners are dry, the basin floor is the strip
/// recess, and a walk-off entry can swim back out over the rim.
#[test]
fn demo_hot_tub_water_is_circular_and_the_basin_exits() {
    const DELTA: f32 = 1.0 / 60.0;
    let settings = Settings::default();
    let mut game = demo_game_at(3.6, -1.5, 10.9, 0.0, DELTA);

    let wet = |subject: &Game, x: f32, z: f32| subject.water.sample(x, z, -2.0).is_some();
    assert!(wet(&game, 3.6, 8.7), "the disc centre is water");
    assert!(wet(&game, 4.6, 8.7), "1.0 m east of centre is water");
    assert!(
        !wet(&game, 4.85, 8.7),
        "the bounding box's east edge is dry"
    );
    assert!(!wet(&game, 2.35, 7.45), "the bounding box's corner is dry");
    assert!(!wet(&game, 3.6, 9.95), "the south corner is dry");

    // Walk north off the deck: the plunge enters the water.
    let mut forward = InputState::holding(&[Control::MoveForward]);
    let mut entered = false;
    for _ in 0_i32..300_i32 {
        game.update_player_movement(&mut forward, &settings);
        if game.is_swimming() {
            entered = true;
            break;
        }
    }
    assert!(
        entered,
        "the walk off the rim enters the water: {:?}",
        game.player_position
    );
    assert!(
        (game.player_floor_y - (-3.0)).abs() < 1e-3,
        "the basin floor is the strip recess: {}",
        game.player_floor_y
    );

    // Keep swimming into the far rim: the bounded climb stands up on the deck.
    let mut climb = InputState::holding(&[Control::MoveForward]);
    let mut out = false;
    for _ in 0_i32..900_i32 {
        game.update_player_movement(&mut climb, &settings);
        if !game.is_swimming()
            && game.water_exit.is_none()
            && game.grounded
            && (game.feet_y() - (-1.5)).abs() < 1e-3
        {
            out = true;
            break;
        }
    }
    assert!(
        out,
        "the swimmer climbs out onto the deck: {:?}",
        game.player_position
    );
    assert!(
        game.player_position.z < 7.6,
        "the exit lands north of the tub: {:?}",
        game.player_position
    );
}

/// One press on the sauna steam switch fills the sauna (both emitters), a
/// second clears it, and neither press touches the lamp switch, the lamp or a
/// door.
#[test]
fn demo_sauna_steam_switch_toggles_only_the_steam() {
    let settings = Settings::default();
    let mut game = demo_game_at(27.6, -0.9, 13.6, 180.0, 1.0 / 60.0);
    let steam = |subject: &Game, id: &str| {
        let handle = subject
            .entities()
            .handle_of(id)
            .unwrap_or_else(|| panic!("`{id}` effect entity"));
        subject
            .entities()
            .components()
            .steam
            .get(handle)
            .unwrap_or_else(|| panic!("`{id}` steam component"))
            .enabled
    };
    let lamp = game
        .entities()
        .handle_of("sauna_light")
        .expect("the sauna lamp");
    let lamp_on = |subject: &Game| {
        subject
            .entities()
            .components()
            .lights
            .get(lamp)
            .expect("the light component")
            .enabled
    };
    let leaf = game
        .doors()
        .index_of("sauna_hall_door")
        .expect("the additional hallway sauna door");
    assert!(
        !steam(&game, "sauna_steam_a") && !steam(&game, "sauna_steam_b"),
        "the sauna starts dry"
    );
    assert!(lamp_on(&game), "the sauna lamp starts on");

    // Aim at the switch on the south wall, press, and the steam fills.
    aim_at(&mut game, 27.6, 14.8, 0.39);
    let switch = game
        .interactables()
        .index_of("sauna_steam_switch")
        .expect("the demo authors the steam switch");
    assert_eq!(
        game.interaction_target(),
        Some(switch),
        "the steam switch is the aimed target"
    );
    let mut input = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut input, &settings);
    assert!(game.take_interact_press(), "the press latches");
    let report = game
        .dispatch_interaction()
        .expect("the steam switch fires its own binding");
    assert_eq!(report.actions_run, 4, "the lever clip and both emitters");
    assert!(
        steam(&game, "sauna_steam_a") && steam(&game, "sauna_steam_b"),
        "both sauna emitters are on"
    );
    assert!(lamp_on(&game), "the lamp switch is untouched");
    assert_eq!(
        game.doors().get(leaf).expect("hall door").phase().name(),
        "closed",
        "the hallway sauna leaf stays closed"
    );

    // A second press clears the steam again; the key must be released first
    // (the press is edge-latched).
    let mut released = InputState::default();
    for _ in 0_i32..2_i32 {
        game.update_player_movement(&mut released, &settings);
    }
    let mut second = InputState::holding(&[Control::Interact]);
    game.update_player_movement(&mut second, &settings);
    assert!(game.take_interact_press(), "the second press latches");
    let off_report = game
        .dispatch_interaction()
        .expect("the second press fires the other branch");
    assert_eq!(
        off_report.actions_run, 4,
        "the lever and both emitters disable"
    );
    assert!(
        !steam(&game, "sauna_steam_a") && !steam(&game, "sauna_steam_b"),
        "the sauna clears"
    );

    // The light switch keeps its own target and never drives the steam; the
    // key is released first so the next press is a fresh edge.
    advance_frames(&mut game, 2);
    let mut press_light = InputState::holding(&[Control::Interact]);
    aim_at(&mut game, 26.16, 14.3, 0.39);
    let light_switch = game
        .interactables()
        .index_of("sauna_switch")
        .expect("the demo authors the sauna light switch");
    assert_eq!(game.interaction_target(), Some(light_switch));
    game.update_player_movement(&mut press_light, &settings);
    assert!(game.take_interact_press(), "the light press latches");
    let _dispatch_report = game.dispatch_interaction().expect("the light switch fires");
    assert!(!lamp_on(&game), "the lamp flipped");
    assert!(
        !steam(&game, "sauna_steam_a") && !steam(&game, "sauna_steam_b"),
        "the light switch never fills the steam"
    );
}

/// The steam switch's interaction bound protrudes from the south wall face
/// (its back plane sits on the slab's inner plane at z 14.85). A bound buried
/// inside the slab lets the wall itself win the aimed ray, because the target
/// entry is measured at the first point the ray crosses the instance's box.
#[test]
fn demo_sauna_steam_switch_bound_protrudes_from_the_wall() {
    let game = demo_game_at(27.6, -0.9, 13.4, 180.0, 1.0 / 60.0);
    let switch = game
        .interactables()
        .index_of("sauna_steam_switch")
        .expect("the demo authors the steam switch");
    let bound = game
        .interactables()
        .get(switch)
        .expect("the steam switch item")
        .bounds;
    assert!(
        bound.min[2] < 14.85 && bound.max[2] <= 14.85 + 1e-3,
        "the bound spans the wall face instead of sitting inside the slab: {bound:?}"
    );
    assert!(
        bound.max[2] - bound.min[2] > 0.05,
        "the bound has real depth"
    );
}

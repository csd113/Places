//! Bounded controller regressions originally found while authoring showcases.
// Test fixtures use indexed arrays and small bounded arithmetic deliberately.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::panic,
    reason = "Test fixtures use indexed arrays and small bounded arithmetic deliberately."
)]

use super::*;

fn at(level: &LevelDef, x: f32, z: f32) -> Game {
    let mut game = Game::new(
        spawn_position(level),
        level.spawn.yaw_degrees.to_radians(),
        CollisionWorld::from_level(level),
    );
    let floor = WalkableFloor::from_level(level)
        .height_at(x, z)
        .expect("audit starts on a real floor");
    game.reset_spawn_point(Vec3::new(x, floor + EYE_HEIGHT, z), 0.0);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / 60.0;
    game
}

fn advance(game: &mut Game, controls: &[Control], frames: usize) {
    let settings = Settings::default();
    let mut input = InputState::holding(controls);
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
    }
}

fn walk_to(game: &mut Game, x: f32, z: f32) {
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    let mut last = game.player_position;
    for _ in 0_i32..300_i32 {
        let delta = Vec2::new(x - game.player_position.x, z - game.player_position.z);
        if delta.length() < 0.10 {
            return;
        }
        game.player_yaw = delta.x.atan2(-delta.y);
        game.update_player_movement(&mut input, &settings);
        let next = game.player_position;
        assert!(
            Vec2::new(next.x - last.x, next.z - last.z).length() <= 0.07,
            "walk has no horizontal teleport"
        );
        assert!(
            (next.y - last.y).abs() <= 0.41,
            "walk stays within step bounds"
        );
        last = next;
    }
    panic!("route to ({x},{z}) blocked at {:?}", game.player_position);
}

/// Two adjacent low risers used to expose porch support disagreement. Three
/// short walks over two regions retain the controller regression in a 6 m room.
#[test]
fn two_level_stoop_walks_down_to_the_ground() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "two_level_stoop", "name": "Two Level Stoop",
            "spawn": { "x": 3.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 3.0, "offset_y": 0.2304 },
                { "x": 2.0, "z": 1.4, "width": 2.0, "depth": 0.6, "offset_y": 0.1152 }
            ]
        }"#,
    )
    .expect("the stoop fixture parses");
    crate::loader::validate_level(&level).expect("the stoop fixture validates");
    let mut game = at(&level, 3.0, 4.0);
    walk_to(&mut game, 3.0, 2.5);
    assert!((game.feet_y - 0.2304).abs() < 1e-3);
    walk_to(&mut game, 3.0, 1.7);
    assert!((game.feet_y - 0.1152).abs() < 1e-3);
    walk_to(&mut game, 3.0, 1.0);
    assert!(game.feet_y.abs() < 1e-3 && game.grounded);
}

/// At 0.665 m depth the ordinary crouched body remains supported; an adjacent
/// 0.325 m submerged step must then lead to dry ground without swim recapture.
/// One basin and one step replace the whole forest-map route campaign.
#[test]
fn shallow_pond_keeps_crouched_posture_and_a_stepped_exit() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "shallow_pond_exit", "name": "Shallow Pond Exit",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }],
            "floor_regions": [
                { "x": 1.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.65 },
                { "x": 3.0, "z": 2.0, "width": 0.6, "depth": 2.0, "offset_y": -0.325 }
            ],
            "water": [{ "x": 1.0, "z": 2.0, "width": 2.6, "depth": 2.0,
                        "surface_y": 0.015, "bottom_y": -0.65 }]
        }"#,
    )
    .expect("the pond fixture parses");
    crate::loader::validate_level(&level).expect("the pond fixture validates");
    let mut game = at(&level, 2.0, 3.0);
    advance(&mut game, &[Control::Crouch], 1);
    advance(&mut game, &[], 90);
    assert!(!game.swimming && game.grounded);
    assert!((game.feet_y() + 0.65).abs() < CONTACT_EPS);
    assert!((game.eye_offset() - CROUCH_EYE_HEIGHT).abs() < CONTACT_EPS);
    assert!((game.body_height() - CROUCH_HEIGHT).abs() < CONTACT_EPS);
    walk_to(&mut game, 4.0, 3.0);
    advance(&mut game, &[], 30);
    assert!(!game.swimming && game.grounded);
    assert!(game.feet_y >= -0.01 && game.is_crouched());
}

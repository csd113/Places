//! Real fixed-step player-controller audits for Lantern Hollow's authored map.
// Test fixtures use indexed arrays and small bounded arithmetic deliberately.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

fn level() -> LevelDef {
    LevelDef::from_json(include_str!("../assets/levels/lantern_hollow.json"))
        .expect("Lantern Hollow parses")
}

fn at(level: &LevelDef, x: f32, z: f32) -> Game {
    let mut game = Game::new(
        spawn_position(level),
        level.spawn.yaw_degrees.to_radians(),
        CollisionWorld::from_level(level),
    );
    let floor = WalkableFloor::from_level(level)
        .height_at(x, z)
        .expect("audit starts on authored floor");
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
    for _ in 0..2400 {
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

#[test]
fn showcase_all_four_cottages_allow_road_entry_bedroom_and_return() {
    let level = level();
    for cx in [-27.0, -9.0, 9.0, 27.0] {
        let mut game = at(&level, cx, 21.8);
        walk_to(&mut game, cx, 9.0);
        walk_to(&mut game, cx, 4.2);
        walk_to(&mut game, cx + 2.1, 4.2);
        walk_to(&mut game, cx + 2.1, 3.8);
        walk_to(&mut game, cx + 2.1, 4.2);
        walk_to(&mut game, cx, 4.2);
        walk_to(&mut game, cx, 21.8);
        assert!(game.grounded && !game.swimming);
    }
}

#[test]
fn showcase_every_front_door_blocks_closed_and_allows_open_passage() {
    let level = level();
    for (index, cx) in [-27.0, -9.0, 9.0, 27.0].into_iter().enumerate() {
        let mut game = at(&level, cx, 9.0);
        let identity = format!("house_{index}_door");
        game.dispatch_actions(
            &[ActionDef::Close {
                target: Some(identity.clone()),
            }],
            None,
        );
        advance(&mut game, &[], 180);
        game.player_yaw = 0.0;
        advance(&mut game, &[Control::MoveForward], 160);
        assert!(
            game.player_position.z > 6.1,
            "closed door {identity} blocks"
        );
        game.dispatch_actions(
            &[ActionDef::Open {
                target: Some(identity),
            }],
            None,
        );
        advance(&mut game, &[], 180);
        walk_to(&mut game, cx, 4.2);
    }
}

#[test]
fn showcase_all_five_forest_trails_reach_the_seated_skeleton_clearing() {
    let level = level();
    let routes: &[&[(f32, f32)]] = &[
        &[
            (-36.0, 10.2),
            (-36.0, -4.0),
            (-26.0, -8.0),
            (-25.0, -24.0),
            (-18.0, -34.0),
        ],
        &[
            (-18.0, 10.2),
            (-18.0, -5.0),
            (-12.0, -16.0),
            (-18.0, -27.0),
            (-18.0, -34.0),
        ],
        &[
            (0.0, 10.2),
            (0.0, -6.0),
            (-8.0, -19.0),
            (-18.0, -27.0),
            (-18.0, -34.0),
        ],
        &[
            (18.0, 10.2),
            (18.0, -5.0),
            (11.0, -13.0),
            (2.0, -24.0),
            (-18.0, -27.0),
            (-18.0, -34.0),
        ],
        &[
            (36.0, 10.2),
            (36.0, -7.0),
            (23.0, -19.0),
            (2.0, -24.0),
            (-18.0, -27.0),
            (-18.0, -34.0),
        ],
    ];
    for route in routes {
        let mut game = at(&level, route[0].0, route[0].1);
        for &(x, z) in &route[1..] {
            walk_to(&mut game, x, z);
        }
        assert!(game.grounded && !game.swimming);
    }
}

#[test]
fn showcase_pond_supports_real_crouched_swimming_and_a_safe_exit() {
    let level = level();
    let mut game = at(&level, -25.0, -15.0);
    walk_to(&mut game, -32.0, -15.0);
    advance(&mut game, &[Control::Crouch], 1);
    advance(&mut game, &[], 90);
    assert!(
        game.swimming,
        "pond basin uses the existing swim controller"
    );
    walk_to(&mut game, -25.0, -15.0);
    advance(&mut game, &[], 30);
    assert!(
        !game.swimming && game.grounded,
        "stepped eastern shore exits to dry ground"
    );
    assert!(game.feet_y >= -0.01);
}

#[test]
fn showcase_84_jump_challenges_cannot_escape_visible_rock_or_gate_edges() {
    let level = level();
    for index in 0..21 {
        let across = -39.0 + index as f32 * 3.9;
        let along = -40.0 + index as f32 * 3.4;
        for (x, z, yaw) in [
            (across, -40.8, 0.0_f32),
            (across, 27.0, 180.0),
            (-38.7, along, -90.0),
            (38.7, along, 90.0),
        ] {
            let mut game = at(&level, x, z);
            game.player_yaw = yaw.to_radians();
            for _ in 0..6 {
                advance(&mut game, &[Control::MoveForward, Control::Jump], 50);
                advance(&mut game, &[Control::MoveForward], 10);
            }
            let p = game.player_position;
            assert!(
                p.x > -43.9 && p.x < 43.9 && p.z > -45.9 && p.z < 31.9,
                "visible boundary failed from ({x},{z}) yaw {yaw}: {p:?}"
            );
        }
    }
}

#[test]
fn showcase_both_road_gates_and_their_rock_joints_are_closed() {
    let level = level();
    for side in [-1.0_f32, 1.0] {
        for z in [12.7, 13.0, 16.6, 20.3, 20.5, 21.1] {
            let mut game = at(&level, side * 38.7, z);
            game.player_yaw = (side * 90.0).to_radians();
            advance(&mut game, &[Control::MoveForward, Control::Jump], 240);
            assert!(
                game.player_position.x.abs() < 42.0,
                "road gate joint leaked at z={z}"
            );
        }
    }
}

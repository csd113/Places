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
fn showcase_each_porch_walks_its_real_two_level_stoop() {
    let level = level();
    for cx in [-27.0_f32, -9.0, 9.0, 27.0] {
        // Onto the raised platform, down the lower front tread, onto the path.
        let mut game = at(&level, cx, 12.0);
        walk_to(&mut game, cx, 9.0);
        walk_to(&mut game, cx, 7.0);
        assert!(
            (game.feet_y - 0.2304).abs() < 1e-3,
            "platform top at {cx}: {}",
            game.feet_y
        );
        walk_to(&mut game, cx + 0.9, 7.5);
        assert!(
            (game.feet_y - 0.1152).abs() < 1e-3,
            "front tread top at {cx}: {}",
            game.feet_y
        );
        walk_to(&mut game, cx + 0.9, 8.1);
        assert!(
            game.feet_y.abs() < 1e-3,
            "porch meets the ground path at {cx}: {}",
            game.feet_y
        );
    }
}

#[test]
fn showcase_facades_keep_the_camera_out_of_their_cladding() {
    let level = level();
    for cx in [-27.0_f32, -9.0, 9.0, 27.0] {
        // Beside the door, press into the wall for five seconds. The doorway
        // surround protrudes to z = 6.45; the camera must stay outside it.
        let mut game = at(&level, cx + 1.0, 8.5);
        game.player_yaw = 0.0;
        advance(&mut game, &[Control::MoveForward], 300);
        assert!(
            game.player_position.z > 6.46,
            "camera entered the doorway surround at {cx}: {}",
            game.player_position.z
        );
    }
}

#[test]
fn showcase_large_decorative_props_block_the_player() {
    let level = level();
    // The campfire is solid: the player stops at its collider edge.
    let mut fire = at(&level, -18.0, -30.5);
    fire.player_yaw = 0.0;
    advance(&mut fire, &[Control::MoveForward], 300);
    assert!(
        fire.player_position.z > -33.1,
        "player entered the campfire: {:?}",
        fire.player_position
    );
    // A trail lamp post is solid: the player stops at its base.
    let mut lamp = at(&level, 17.4, -4.4);
    lamp.player_yaw = 90.0_f32.to_radians();
    advance(&mut lamp, &[Control::MoveForward], 300);
    assert!(
        lamp.player_position.x < 18.95,
        "player passed through a solid trail lamp: {:?}",
        lamp.player_position
    );
    // A garden railing blocks a walk through it towards the house.
    let mut rail = at(&level, -30.6, 11.0);
    rail.player_yaw = 0.0;
    advance(&mut rail, &[Control::MoveForward], 300);
    assert!(
        rail.player_position.z > 9.9,
        "player passed through a garden railing: {:?}",
        rail.player_position
    );
}

#[test]
fn showcase_all_four_cottages_allow_road_entry_bedroom_and_return() {
    let level = level();
    for cx in [-27.0, -9.0, 9.0, 27.0] {
        let mut game = at(&level, cx, 21.8);
        walk_to(&mut game, cx, 9.0);
        walk_to(&mut game, cx, 4.2);
        walk_to(&mut game, cx, 4.5);
        walk_to(&mut game, cx - 3.05, 4.5);
        walk_to(&mut game, cx - 3.05, 2.85);
        walk_to(&mut game, cx - 2.5, 2.85);
        walk_to(&mut game, cx - 3.05, 2.85);
        walk_to(&mut game, cx - 3.05, 4.5);
        walk_to(&mut game, cx, 4.5);
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
            (-17.85, -31.65),
        ],
        &[
            (-18.0, 10.2),
            (-18.0, -5.0),
            (-12.0, -16.0),
            (-18.0, -27.0),
            (-17.85, -31.65),
        ],
        &[
            (0.0, 10.2),
            (0.0, -6.0),
            (-8.0, -19.0),
            (-18.0, -27.0),
            (-17.85, -31.65),
        ],
        &[
            (18.0, 10.2),
            (18.0, -5.0),
            (11.0, -13.0),
            (2.0, -24.0),
            (-18.0, -27.0),
            (-17.85, -31.65),
        ],
        &[
            (36.0, 10.2),
            (36.0, -7.0),
            (23.0, -19.0),
            (2.0, -24.0),
            (-18.0, -27.0),
            (-17.85, -31.65),
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
        let across = (index as f32).mul_add(3.9, -39.0);
        let along = (index as f32).mul_add(3.4, -40.0);
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
    for (x, z, yaw) in [
        (-38.7, -40.8, -45.0_f32),
        (38.7, -40.8, 45.0),
        (-38.7, 27.0, -135.0),
        (38.7, 27.0, 135.0),
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
            "visible diagonal corner failed from ({x},{z}): {p:?}"
        );
    }
}

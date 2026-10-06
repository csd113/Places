//! Traversal contracts for the shipped Winter foundation, using the real controller.
use super::*;

fn winter() -> LevelDef {
    LevelDef::from_json(include_str!("../../../assets/levels/winter.json"))
        .expect("shipped Winter source parses")
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Validated fixture coordinates use finite, bounded floating-point vector arithmetic."
)]
fn at(level: &LevelDef, x: f32, z: f32) -> Game {
    let floor = WalkableFloor::from_level(level)
        .height_at(x, z)
        .expect("audit starts on a real floor");
    let mut game = Game::new(
        Vec3::new(x, floor, z) + Vec3::Y * EYE_HEIGHT,
        0.0,
        CollisionWorld::from_level(level),
    );
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

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Subtracting finite fixture positions produces bounded movement vectors, with no integer overflow."
)]
fn walk_to(game: &mut Game, x: f32, z: f32) {
    let settings = Settings::default();
    let mut input = InputState::holding(&[Control::MoveForward]);
    for _ in 0_u32..2_400_u32 {
        let before = game.player_position;
        let delta = Vec2::new(x, z) - Vec2::new(before.x, before.z);
        if delta.length() < 0.1 {
            assert!(
                game.grounded && !game.swimming,
                "route ends grounded on a solid floor"
            );
            return;
        }
        game.player_yaw = delta.x.atan2(-delta.y);
        game.update_player_movement(&mut input, &settings);
        let step = game.player_position - before;
        assert!(
            Vec2::new(step.x, step.z).length() <= 0.07,
            "no horizontal teleport"
        );
        assert!(step.y.abs() <= 0.41, "no vertical teleport: {step:?}");
    }
    panic!(
        "Winter route to ({x},{z}) blocked at {:?}",
        game.player_position
    );
}

#[test]
fn winter_lodge_stairs_ramp_shelter_and_interior_are_connected() {
    let level = winter();
    let mut game = at(&level, -18.2, -9.0);
    // The existing controller uses a continuous walk pitch across stair treads.
    // Verify the first/second tread bands, then the exact deck landing.
    walk_to(&mut game, -17.0, -9.0);
    advance(&mut game, &[], 30);
    assert!(
        game.feet_y > 0.19 && game.feet_y < 0.4,
        "first stair tread: {}",
        game.feet_y
    );
    walk_to(&mut game, -16.2, -9.0);
    advance(&mut game, &[], 30);
    assert!(
        game.feet_y > 0.39 && game.feet_y < 0.6,
        "second stair tread: {}",
        game.feet_y
    );
    walk_to(&mut game, -14.2, -9.0);
    assert!(game.feet_y > 0.59 && game.feet_y < 0.61, "deck landing");
    walk_to(&mut game, -11.5, -9.0);
    walk_to(&mut game, -11.5, -12.0);
    walk_to(&mut game, -9.7, -12.0);
    walk_to(&mut game, -11.5, -12.0);
    walk_to(&mut game, -11.5, -9.0);
    walk_to(&mut game, -11.5, -4.0);
    assert!(game.feet_y.abs() < 0.01, "returns to level snow ground");
    walk_to(&mut game, -11.5, -9.0);
    walk_to(&mut game, -14.2, -9.0);
    walk_to(&mut game, -18.2, -9.0);
    assert!(game.feet_y.abs() < 0.01, "returns to level snow ground");
}

#[test]
fn winter_all_entrances_and_operable_doors_work() {
    let level = winter();
    for (id, x, front, outside) in [
        ("home_0_door", -11.5, -10.0, -9.0),
        ("home_1_door", 12.5, -24.0, -22.8),
        ("home_2_door", -12.5, 10.0, 11.2),
    ] {
        let mut game = at(&level, x, outside);
        let _closed = game.dispatch_actions(
            &[ActionDef::Close {
                target: Some(id.to_owned()),
            }],
            None,
        );
        advance(&mut game, &[], 180);
        advance(&mut game, &[Control::MoveForward], 120);
        assert!(game.player_position.z > front, "closed {id} blocks");
        let _opened = game.dispatch_actions(
            &[ActionDef::Open {
                target: Some(id.to_owned()),
            }],
            None,
        );
        advance(&mut game, &[], 180);
        let inside = front.mul_add(1.0, -2.0);
        walk_to(&mut game, x, inside);
        walk_to(&mut game, x, outside);
    }
}

#[test]
fn winter_pond_is_walkable_jumpable_and_returns_to_snow() {
    let level = winter();
    let mut game = at(&level, 4.0, -10.0);
    walk_to(&mut game, 11.0, -10.0);
    assert!(
        game.feet_y > -0.17 && game.feet_y < -0.15,
        "stands on physical ice floor"
    );
    advance(&mut game, &[Control::Jump], 1);
    assert!(!game.grounded, "jump leaves ice surface");
    advance(&mut game, &[], 120);
    assert!(
        game.grounded && !game.swimming,
        "route ends grounded on a solid floor"
    );
    assert!(
        game.feet_y > -0.17 && game.feet_y < -0.15,
        "stands on physical ice floor"
    );
    walk_to(&mut game, 4.0, -10.0);
    assert!(game.feet_y.abs() < 0.01, "returns to level snow ground");
    walk_to(&mut game, 11.0, -10.0);
    walk_to(&mut game, 11.0, -2.0);
    assert!(game.feet_y.abs() < 0.01, "returns to level snow ground");
}

#[test]
fn winter_forest_spine_and_cottage_paths_stay_clear() {
    let level = winter();
    let mut game = at(&level, 0.0, 16.0);
    walk_to(&mut game, 0.0, -36.0);
    walk_to(&mut game, 0.0, -20.7);
    walk_to(&mut game, 12.5, -20.7);
    walk_to(&mut game, 12.5, -26.0);
    walk_to(&mut game, 12.5, -20.7);
    walk_to(&mut game, 0.0, -20.7);
    walk_to(&mut game, 0.0, 13.1);
    walk_to(&mut game, -12.5, 13.1);
    walk_to(&mut game, -12.5, 8.0);
}

#[test]
fn winter_visible_rocks_and_railings_block_walks() {
    let level = winter();
    for (x, z, yaw, axis, limit) in [
        (15.2, -10.0, 90.0_f32, true, 16.6),
        (5.0, -18.0, 90.0, true, 6.1),
        (-13.5, -9.0, 180.0, false, -7.9),
    ] {
        let mut game = at(&level, x, z);
        game.player_yaw = yaw.to_radians();
        advance(&mut game, &[Control::MoveForward], 300);
        let position = game.player_position;
        assert!(
            if axis {
                position.x < limit
            } else {
                position.z < limit
            },
            "solid failed: {position:?}"
        );
        assert!(
            game.feet_y > -0.2 && game.feet_y < 0.61,
            "solid collision preserves floor height"
        );
    }
}

#[test]
fn winter_boundary_jumps_cannot_escape_or_fall_through() {
    let level = winter();
    for (x, z, yaw) in [
        (0.0, -37.0, 0.0_f32),
        (0.0, 17.0, 180.0),
        (-21.0, -32.0, -90.0),
        (21.0, -32.0, 90.0),
        (-20.0, 17.0, -135.0),
        (20.0, -37.0, 45.0),
    ] {
        let mut game = at(&level, x, z);
        game.player_yaw = yaw.to_radians();
        for _ in 0_u32..6_u32 {
            advance(&mut game, &[Control::MoveForward, Control::Jump], 50);
            advance(&mut game, &[Control::MoveForward], 10);
        }
        let position = game.player_position;
        assert!(
            position.x > -24.0 && position.x < 24.0 && position.z > -40.0 && position.z < 20.0,
            "boundary failed: {position:?}"
        );
        assert!(position.y > 1.3, "fell through ground: {position:?}");
    }
}

//! Bounded snow/ice contact controls, using the real controller.
use super::*;

fn winter() -> LevelDef {
    LevelDef::from_json(include_str!(
        "../../../tests/fixtures/unit/winter_contacts.json"
    ))
    .expect("bounded snow/ice fixture parses")
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
        CollisionWorld::from_level(level)
            .with_ground_materials(level, &super::ice::materials(level)),
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
fn stairs_and_ramp_share_a_continuous_raised_landing() {
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
fn a_recessed_ice_floor_is_walkable_jumpable_and_returns_to_snow() {
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

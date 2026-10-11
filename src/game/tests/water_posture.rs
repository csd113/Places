//! Upright water posture and physical body clearance regressions.

use super::*;

fn tick(game: &mut Game, controls: &[Control], frames: usize) {
    let settings = Settings::default();
    let mut input = InputState::holding(controls);
    for _ in 0..frames {
        game.update_player_movement(&mut input, &settings);
        assert!(game.feet_y() >= -3.0 - CONTACT_EPS);
        assert!((game.player_position.y - game.feet_y() - game.eye_offset()).abs() < CONTACT_EPS);
    }
}

#[test]
fn deep_water_preserves_standing_height_while_rising_and_sinking() {
    let mut game = game_for(&pool_level());
    play_at(&mut game, 6.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    tick(&mut game, &[], 120);
    assert!(game.is_swimming() && game.is_underwater());
    assert_exact(game.feet_y(), -3.0);
    assert_exact(game.body_height(), PLAYER_HEIGHT);
    assert!((game.player_position.y - (-3.0 + EYE_HEIGHT)).abs() < CONTACT_EPS);

    tick(&mut game, &[Control::Jump], 180);
    assert!(
        (game.player_position.y - (-0.5 + FLOAT_EYE_MARGIN)).abs()
            <= SWIM_BOB_AMPLITUDE + CONTACT_EPS
    );
    assert_exact(game.body_height(), PLAYER_HEIGHT);
    assert_exact(game.eye_offset(), EYE_HEIGHT);
    assert!(game.feet_y() > -3.0 + 0.5);

    tick(&mut game, &[], 300);
    assert_exact(game.feet_y(), -3.0);
    assert_exact(game.eye_offset(), EYE_HEIGHT);
    assert!(game.is_swimming() && game.is_underwater());
}

#[test]
fn underwater_crouch_keeps_feet_supported_and_standing_is_explicit() {
    let mut game = game_for(&pool_level());
    play_at(&mut game, 6.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    tick(&mut game, &[], 120);
    game.toggle_stance();
    tick(&mut game, &[], 30);
    assert_exact(game.feet_y(), -3.0);
    assert_exact(game.body_height(), CROUCH_HEIGHT);
    assert_exact(game.eye_offset(), CROUCH_EYE_HEIGHT);
    game.toggle_stance();
    tick(&mut game, &[], 30);
    assert_exact(game.feet_y(), -3.0);
    assert_exact(game.body_height(), PLAYER_HEIGHT);
    assert_exact(game.eye_offset(), EYE_HEIGHT);
}

#[test]
fn changing_stance_at_the_float_line_moves_feet_only_at_swim_speed() {
    let mut game = game_for(&pool_level());
    play_at(&mut game, 6.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    tick(&mut game, &[Control::Jump], 300);
    assert!(game.float_hold);
    let settings = Settings::default();
    let mut jump = InputState::holding(&[Control::Jump]);
    for target in [Stance::Crouched, Stance::Standing] {
        game.toggle_stance();
        assert_eq!(game.stance(), target);
        for _ in 0_i32..120_i32 {
            let previous_feet = game.feet_y();
            game.update_player_movement(&mut jump, &settings);
            assert!(
                (game.feet_y() - previous_feet).abs()
                    <= SWIM_RISE_SPEED.mul_add(game.sim_delta_seconds, CONTACT_EPS),
                "stance animation must not displace the buoyant body: {} -> {}",
                previous_feet,
                game.feet_y()
            );
        }
        assert!(
            (game.player_position.y - (-0.5 + FLOAT_EYE_MARGIN)).abs()
                <= SWIM_BOB_AMPLITUDE + CONTACT_EPS
        );
    }
}

#[test]
fn underwater_beam_blocks_standing_and_clamps_crouched_rise() {
    let mut game = game_for(&pool_level());
    let mut walls = game.walls.clone();
    walls.push(WallAabb::with_y(6.0, -1.8, 3.0, 2.0, 0.3, 2.0));
    game.set_walls(walls);
    play_at(&mut game, 5.0, -3.0, 4.0, 90.0, 1.0 / 60.0);
    tick(&mut game, &[Control::MoveForward], 90);
    assert!(game.player_position.x <= 6.0 - PLAYER_RADIUS + CONTACT_EPS);
    game.toggle_stance();
    tick(&mut game, &[], 30);
    tick(&mut game, &[Control::MoveForward], 45);
    assert!((6.0..8.0).contains(&game.player_position.x));
    game.toggle_stance();
    assert!(game.is_crouched(), "standing is refused under the beam");
    tick(&mut game, &[Control::Jump], 180);
    assert!(game.feet_y() + game.body_height() <= -1.8 + CONTACT_EPS);
    assert!(game.player_position.y <= -1.8 - SCENE_NEAR_M + CONTACT_EPS);
    tick(&mut game, &[], 180);
    assert_exact(game.feet_y(), -3.0);
}

#[test]
fn sinking_upright_body_lands_on_a_submerged_solid() {
    let mut game = game_for(&pool_level());
    let mut walls = game.walls.clone();
    walls.push(WallAabb::with_y(6.0, -2.8, 3.0, 2.0, 0.4, 2.0));
    game.set_walls(walls);
    play_at(&mut game, 7.0, -2.0, 4.0, 90.0, 1.0 / 60.0);
    tick(&mut game, &[], 300);
    assert!((game.feet_y() - (-2.4)).abs() < CONTACT_EPS);
    assert!((game.player_position.y - (-2.4 + EYE_HEIGHT)).abs() < CONTACT_EPS);
    assert_exact(game.body_height(), PLAYER_HEIGHT);
    assert!(game.is_swimming());
}

#[test]
fn positive_surface_bob_settles_onto_the_standing_depth_boundary() {
    let mut level = pool_level();
    level
        .floor_regions
        .first_mut()
        .expect("basin region")
        .offset_y = -1.98;
    level.water.first_mut().expect("basin water").bottom_y = Some(-1.98);
    let mut game = game_for(&level);
    play_at(&mut game, 6.0, -1.98, 4.0, 90.0, VERTICAL_SUBSTEP);
    set_eye_y(&mut game, -0.5 + FLOAT_EYE_MARGIN + SWIM_BOB_AMPLITUDE);
    game.swimming = true;
    game.grounded = false;
    game.float_hold = true;
    game.bob_phase = std::f32::consts::FRAC_PI_2;
    let settings = Settings::default();
    let mut jump = InputState::holding(&[Control::Jump]);
    for _ in 0_i32..30_i32 {
        let previous_feet = game.feet_y();
        game.update_player_movement(&mut jump, &settings);
        assert!(
            (game.feet_y() - previous_feet).abs()
                <= WATER_EXIT_CLIMB_SPEED.mul_add(VERTICAL_SUBSTEP, CONTACT_EPS)
        );
    }
    assert!(game.grounded && !game.swimming && game.water_exit.is_none());
    assert!((game.feet_y() + 1.98).abs() < CONTACT_EPS);
}

#[test]
fn the_upright_swimmer_follows_the_real_beach_ramp_back_to_shore() {
    let level = LevelDef::from_json(include_str!("../../../assets/levels/beach_demo.json"))
        .expect("the actual Beach parses");
    let mut game = game_for(&level);
    play_at(&mut game, 0.0, -2.2, -8.0, 180.0, 1.0 / 60.0);
    let settings = Settings::default();
    let mut forward = InputState::holding(&[Control::MoveForward]);
    for _ in 0_i32..900_i32 {
        let previous = game.player_position;
        let was_in_water = game.swimming || game.water_exit.is_some();
        game.update_player_movement(&mut forward, &settings);
        let bound = if was_in_water || game.swimming || game.water_exit.is_some() {
            WATER_EXIT_CLIMB_SPEED / 60.0 + CONTACT_EPS
        } else {
            PLAYER_STEP_HEIGHT + CONTACT_EPS
        };
        assert!(
            (game.player_position.y - previous.y).abs() <= bound,
            "bounded shore transition {previous:?} -> {:?}, grounded {}, swim {}, exit {:?}",
            game.player_position,
            game.grounded,
            game.swimming,
            game.water_exit
        );
        assert_exact(game.body_height(), PLAYER_HEIGHT);
        assert_exact(game.eye_offset(), EYE_HEIGHT);
        let floor = game
            .floor
            .walk_height_at(game.player_position.x, game.player_position.z)
            .expect("on Beach");
        assert!(
            game.feet_y() >= floor - CONTACT_EPS,
            "feet {}, floor {floor}, position {:?}, grounded {}, swim {}, exit {:?}",
            game.feet_y(),
            game.player_position,
            game.grounded,
            game.swimming,
            game.water_exit
        );
        if game.player_position.z >= 2.0 && game.grounded && !game.swimming {
            break;
        }
    }
    assert!(
        game.player_position.z >= 2.0 && game.grounded && !game.swimming,
        "the full body climbs the submerged ramp and reaches dry shore: {:?}",
        game.player_position
    );
}

#[test]
fn default_standing_demo_pool_exits_repeatedly_with_full_body_clearance() {
    let settings = Settings::default();
    for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
        let mut game = demo_game_at(12.0, -3.0, 14.0, 90.0, delta);
        let mut input = InputState::holding(&[Control::MoveForward]);
        let mut previous = game.player_position;
        let mut exited = false;
        for _ in 0_i32..1_440_i32 {
            game.update_player_movement(&mut input, &settings);
            if game.water_exit.is_some() {
                assert!(!game.grounded, "a pull-up is unsupported until arrival");
            }
            assert_exact(game.body_height(), PLAYER_HEIGHT);
            assert_exact(game.eye_offset(), EYE_HEIGHT);
            assert!(
                game.feet_y()
                    >= game
                        .floor
                        .walk_height_at(game.player_position.x, game.player_position.z)
                        .expect("inside pool/deck")
                        - CONTACT_EPS
            );
            assert!(
                (game.player_position.y - previous.y).abs()
                    <= WATER_EXIT_CLIMB_SPEED.mul_add(delta, CONTACT_EPS)
            );
            assert!(
                game.player_position.distance(previous)
                    <= (settings.walk_speed.hypot(WATER_EXIT_CLIMB_SPEED))
                        .mul_add(delta, CONTACT_EPS)
            );
            assert!(camera_outside_world(&game).is_none());
            previous = game.player_position;
            if game.grounded
                && game.player_position.x > 20.0
                && (game.feet_y() + 1.5).abs() < CONTACT_EPS
            {
                exited = true;
                break;
            }
        }
        assert!(
            exited,
            "standing exit completes at delta {delta}: {:?}",
            game.player_position
        );
    }
}

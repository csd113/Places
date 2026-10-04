//! Deterministic traversal of the permanent QA map, including irregular frames.

use super::*;

const PATTERNS: &[&[f32]] = &[
    &[1.0 / 30.0],
    &[1.0 / 60.0],
    &[1.0 / 144.0],
    &[MAX_SIM_DELTA],
    &[0.004, 0.013, 0.041, 0.1, 0.007],
];

fn level() -> LevelDef {
    LevelDef::from_json(include_str!("../../../assets/levels/movement_test.json"))
        .expect("the permanent movement map parses")
}

fn run(game: &mut Game, controls: &[Control], seconds: f32, pattern: &[f32]) -> bool {
    let mut input = InputState::holding(controls);
    let settings = Settings::default();
    let mut elapsed = 0.0;
    let mut always_grounded = true;
    for delta in pattern.iter().cycle() {
        if elapsed >= seconds {
            break;
        }
        game.sim_delta_seconds = delta.min(seconds - elapsed);
        game.update_player_movement(&mut input, &settings);
        elapsed += game.sim_delta_seconds;
        always_grounded &= game.grounded;
        assert!(game.player_position.is_finite());
        assert!(game.vertical_velocity.is_finite());
        assert!((game.player_position.y - game.feet_y() - game.eye_offset_current).abs() < 1e-4);
    }
    always_grounded
}

#[test]
fn movement_map_is_valid_and_rejects_over_limit_slopes() {
    let mut level = level();
    crate::loader::validate_level(&level).expect("all QA stations are valid geometry");
    let ramp = level.ramps.last_mut().expect("the map has ramps");
    ramp.rise = ramp.length() * (crate::level::MAX_RAMP_SLOPE + 0.01);
    let error = crate::loader::validate_level(&level).expect_err("an over-limit slope is rejected");
    assert!(error.contains("too steep"), "{error}");
}

#[test]
fn measured_lane_has_equal_cardinal_and_diagonal_speed_with_irregular_frames() {
    let level = level();
    for pattern in PATTERNS {
        for controls in [
            &[Control::MoveForward][..],
            &[Control::MoveBackward][..],
            &[Control::StrafeLeft][..],
            &[Control::StrafeRight][..],
            &[Control::MoveForward, Control::StrafeRight][..],
        ] {
            let mut game = game_for(&level);
            play_at(&mut game, 15.0, 0.0, 10.0, 90.0, 0.0);
            let start = game.player_position;
            let _run_status = run(&mut game, controls, 1.0, pattern);
            assert!((game.player_position.distance(start) - 3.0).abs() < 0.002);
            assert!(game.grounded);
        }
    }
}

#[test]
fn diagnostic_steps_accept_the_limit_and_refuse_above_it() {
    let level = level();
    for pattern in PATTERNS {
        for (z, height) in [
            (5.7, 0.1),
            (7.7, 0.3),
            (9.7, 0.39),
            (11.7, 0.4),
            (13.7, 0.41),
            (15.7, 0.6),
        ] {
            let mut game = game_for(&level);
            play_at(&mut game, 38.0, 0.0, z, 90.0, 0.0);
            let _run_status = run(&mut game, &[Control::MoveForward], 1.2, pattern);
            if height <= PLAYER_STEP_HEIGHT {
                assert!(game.player_position.x > 41.0, "step {height}, {pattern:?}");
                assert!((game.feet_y() - height).abs() < STEP_EPS);
            } else {
                assert!(game.player_position.x < 40.0, "step {height}, {pattern:?}");
                assert!(game.feet_y().abs() < STEP_EPS);
            }
            assert!(game.grounded);
        }
    }
}

#[test]
fn diagnostic_header_requires_crouch_and_refuses_standing_on_the_step() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 48.0, 0.0, 8.0, 90.0, 0.0);
        let _run_status = run(&mut game, &[Control::MoveForward], 1.0, pattern);
        assert!(game.player_position.x < 50.0 && game.feet_y().abs() < STEP_EPS);
        force_stance(&mut game, Stance::Crouched);
        let _run_status_2 = run(&mut game, &[Control::MoveForward], 1.0, pattern);
        assert!(game.player_position.x > 51.0);
        game.toggle_stance();
        assert_eq!(game.stance, Stance::Crouched);
        assert!(game.feet_y() + game.body_height() <= 2.0);
    }
}

#[test]
fn diagnostic_ledges_fall_land_and_have_no_off_room_plane() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 56.0, 1.5, 34.0, 90.0, 0.0);
        let _run_status = run(&mut game, &[Control::MoveForward], 2.0, pattern);
        assert!(game.player_position.x > 61.9 && game.grounded);
        assert!(game.feet_y().abs() < STEP_EPS);
        let _run_status_2 = run(&mut game, &[], 0.5, pattern);
        assert!(game.grounded && game.vertical_velocity == 0.0);

        let mut outside_game = game_for(&level);
        play_at(&mut outside_game, 70.0, 1.5, 35.0, 90.0, 0.0);
        let _run_status_3 = run(&mut outside_game, &[Control::MoveForward], 1.6, pattern);
        assert!(outside_game.player_position.x > 74.7);
        assert!(!outside_game.grounded && outside_game.feet_y() < -1.0);
        assert_eq!(outside_game.support_at(74.0, 35.0, 1.5), None);
    }
}

#[test]
fn diagnostic_stairs_and_ramps_join_their_landings_at_all_frame_patterns() {
    let level = level();
    for pattern in PATTERNS {
        for (x, start_z, end_z, height) in [
            (4.4, 27.5, 35.0, 0.9),
            (7.7, 27.5, 35.0, 1.5),
            (10.6, 27.5, 31.5, 1.6),
            (13.5, 27.5, 35.0, 1.5),
            (18.5, 27.5, 33.0, 1.2),
            (28.6, 27.5, 34.5, 0.6),
            (31.6, 27.5, 34.5, 1.8),
            (34.6, 27.5, 31.5, 3.0),
            (37.4, 27.5, 30.5, 3.8),
            (40.4, 27.5, 30.5, 4.0),
        ] {
            let mut game = game_for(&level);
            play_at(&mut game, x, 0.0, start_z, 180.0, 0.0);
            assert!(
                run(
                    &mut game,
                    &[Control::MoveForward],
                    (end_z - start_z) / 3.0,
                    pattern,
                ),
                "lost support at {x}, {pattern:?}"
            );
            assert!(
                (game.player_position.z - end_z).abs() < 0.01,
                "{x}, {pattern:?}: {:?}",
                game.player_position
            );
            assert!(game.grounded && (game.feet_y() - height).abs() < STEP_EPS);
            assert!(
                run(
                    &mut game,
                    &[Control::MoveBackward],
                    (end_z - start_z) / 3.0,
                    pattern,
                ),
                "lost support at {x}, {pattern:?}"
            );
            assert!(
                game.grounded && game.feet_y().abs() < STEP_EPS,
                "descent {x}, {pattern:?}: {:?}, feet {}, grounded {}",
                game.player_position,
                game.feet_y(),
                game.grounded
            );
        }
    }
}

#[test]
fn diagnostic_crouch_passage_clears_smoothly_and_blocks_standing() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 37.5, 0.0, 48.0, 180.0, 0.0);
        let _run_status = run(&mut game, &[Control::MoveForward], 1.0, pattern);
        assert!(game.player_position.z < 50.0);
        game.toggle_stance();
        let _run_status_2 = run(&mut game, &[Control::MoveForward], 1.0, pattern);
        game.toggle_stance();
        assert_eq!(game.stance, Stance::Crouched);
        let _run_status_3 = run(&mut game, &[Control::MoveBackward], 1.0, pattern);
        let eye = game.player_position.y;
        game.toggle_stance();
        assert_eq!(game.stance, Stance::Standing);
        assert_exact(game.player_position.y, eye);
        let _run_status_4 = run(&mut game, &[], 1.0, pattern);
        assert!((game.player_position.y - EYE_HEIGHT).abs() < STEP_EPS);
    }
}

#[test]
fn diagnostic_pool_surfaces_and_exits_flat_and_raised_rims_without_reentry() {
    let level = level();
    for pattern in PATTERNS {
        for (x, z, yaw, exit_height) in [(10.0, 74.0, 0.0, 0.0), (17.0, 75.0, 90.0, 0.2)] {
            let mut game = game_for(&level);
            play_at(&mut game, x, -2.4, z, yaw, 0.0);
            let _run_status = run(&mut game, &[Control::Jump], 4.0, pattern);
            assert!(game.swimming && game.player_position.y > -0.15);
            let _run_status_2 = run(
                &mut game,
                &[Control::Jump, Control::MoveForward],
                2.0,
                pattern,
            );
            assert!(
                !game.swimming && game.water_exit.is_none() && game.grounded,
                "{yaw}, {pattern:?}: {:?}",
                game.player_position
            );
            assert!(
                (game.feet_y() - exit_height).abs() < STEP_EPS,
                "exit {yaw}, {pattern:?}: {:?}, feet {}",
                game.player_position,
                game.feet_y()
            );
            let _run_status_3 = run(&mut game, &[], 0.5, pattern);
            assert!(game.grounded && !game.swimming);
        }
    }
}

#[test]
fn diagnostic_doorway_jump_has_no_horizontal_blip_or_sideways_push() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 58.9, 0.0, 48.0, 180.0, 0.0);
        let mut input = InputState::holding(&[Control::MoveForward, Control::Jump]);
        let settings = Settings::default();
        let mut elapsed = 0.0;
        for delta in pattern.iter().cycle() {
            if elapsed >= 2.0 {
                break;
            }
            game.sim_delta_seconds = delta.min(2.0 - elapsed);
            let before = game.player_position;
            game.update_player_movement(&mut input, &settings);
            let travelled = Vec2::new(
                game.player_position.x - before.x,
                game.player_position.z - before.z,
            )
            .length();
            assert!(travelled <= 3.0f32.mul_add(game.sim_delta_seconds, 1e-3));
            assert!((game.player_position.x - 58.9).abs() < 1e-3);
            elapsed += game.sim_delta_seconds;
        }
        // The header may briefly stop the airborne approach. Once grounded,
        // ordinary walking must finish crossing without a position correction.
        let _run_status = run(&mut game, &[Control::MoveForward], 0.5, pattern);
        assert!(game.player_position.z > 53.9 && game.grounded);
    }
}

#[test]
fn diagnostic_closed_door_blocks_then_opens_for_ordinary_movement() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 52.6, 0.0, 56.5, 180.0, 0.0);
        let _run_status = run(&mut game, &[Control::MoveForward], 1.0, pattern);
        assert!(game.player_position.z < 58.0);
        // Clear the leaf's swing before opening; otherwise its obstruction
        // rule correctly holds against the player standing in front of it.
        let _run_status_2 = run(&mut game, &[Control::MoveBackward], 0.4, pattern);
        let target = game
            .interactables()
            .index_of("closed_door")
            .expect("manual QA door");
        let _dispatch_report = game
            .entities_mut()
            .dispatch_interaction(Some(target))
            .expect("toggle the leaf");
        let _run_status_3 = run(&mut game, &[], 1.0, pattern);
        let _run_status_4 = run(&mut game, &[Control::MoveForward], 1.5, pattern);
        assert!(game.player_position.z > 59.0 && game.grounded);
    }
}

#[test]
fn diagnostic_props_and_opposing_walls_do_not_tunnel_or_jitter() {
    let level = level();
    for pattern in PATTERNS {
        for (x, z, yaw, blocked_z) in [
            (30.0, 72.0, 180.0, 73.35),
            (34.0, 72.0, 180.0, 73.45),
            (38.0, 72.0, 180.0, 73.4),
            (42.0, 72.0, 180.0, 73.525),
            (55.0, 72.0, 180.0, 72.7),
        ] {
            let mut game = game_for(&level);
            play_at(&mut game, x, 0.0, z, yaw, 0.0);
            let _run_status = run(&mut game, &[Control::MoveForward], 1.0, pattern);
            assert!(game.player_position.z <= blocked_z + 1e-3);
            let contact = game.player_position;
            let _run_status_2 = run(&mut game, &[Control::MoveForward], 1.0, pattern);
            assert!(game.player_position.distance(contact) < 1e-3);
            assert!(game.grounded && game.feet_y().abs() < STEP_EPS);
        }
    }
}

#[test]
fn diagnostic_jump_lands_on_desk_and_head_collision_consumes_rise() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 7.0, 0.0, 57.5, 180.0, 0.0);
        let _run_status = run(
            &mut game,
            &[Control::MoveForward, Control::Jump],
            0.55,
            pattern,
        );
        let _run_status_2 = run(&mut game, &[], 1.0, pattern);
        assert!(
            game.grounded && (game.feet_y() - 0.75).abs() < STEP_EPS,
            "{pattern:?}: {:?}",
            game.player_position
        );

        let mut ceiling_game = game_for(&level);
        play_at(&mut ceiling_game, 41.0, 0.0, 60.0, 0.0, 0.0);
        let _run_status_3 = run(&mut ceiling_game, &[Control::Jump], 0.2, pattern);
        assert!(ceiling_game.feet_y() + PLAYER_HEIGHT <= 2.05 + CONTACT_EPS);
        assert!(ceiling_game.vertical_velocity <= 0.0);
        assert!((ceiling_game.player_position.x - 41.0).abs() < 1e-3);
        let _run_status_4 = run(&mut ceiling_game, &[], 1.0, pattern);
        assert!(ceiling_game.grounded && ceiling_game.feet_y().abs() < STEP_EPS);
    }
}

#[test]
fn diagnostic_too_high_jump_cannot_leave_the_body_inside_the_ledge_face() {
    let level = level();
    for pattern in PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, 16.0, 0.0, 49.5, 180.0, 0.0);
        let _run_status = run(
            &mut game,
            &[Control::Jump, Control::MoveForward],
            0.7,
            pattern,
        );
        let _run_status_2 = run(&mut game, &[], 1.0, pattern);
        assert!(
            game.player_position.z <= 51.0 - PLAYER_RADIUS + CONTACT_EPS,
            "{pattern:?}: {:?}",
            game.player_position
        );
        assert!(game.grounded && game.feet_y().abs() < STEP_EPS);
    }
}

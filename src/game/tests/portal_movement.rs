//! Essential regressions for the new command scheduler, momentum/jump ordering,
//! duck hull/eye ownership, and continuous square-hull contacts. Four small
//! deterministic CPU fixtures run a few thousand ticks without assets or bake.
use crate::engine::collision::{PLAYER_HEIGHT, PLAYER_RADIUS, WallAabb};
use crate::engine::level::LevelDef;
use crate::game::input::{Control, InputState, ScriptedHold};
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game, Settings, Stance, VERTICAL_SUBSTEP};
use glam::{Vec2, Vec3};

fn flat() -> Result<Game, String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"command_test","name":"Command test","spawn":{"x":20,"z":30},"rooms":[{"x":0,"z":0,"width":40,"depth":60,"height":12,"ceiling":{"kind":"open"}}]}"#).map_err(|error|error.to_string())?;
    let mut game = Game::new(
        Vec3::new(20.0, EYE_HEIGHT, 30.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    game.set_app_state(AppState::Playing);
    Ok(game)
}
fn input(controls: &[Control]) -> InputState {
    let script: Vec<_> = controls
        .iter()
        .map(|control| ScriptedHold {
            control: *control,
            first_seconds: 0.0,
            last_seconds: 1.0,
        })
        .collect();
    let mut input = InputState::default();
    input.apply_move_script(&script, 0.5);
    input.clear_presses();
    input
}
fn tick(game: &mut Game, command: &InputState) {
    game.sim_delta_seconds = VERTICAL_SUBSTEP;
    let _origin = game.update_playing_frame(command, &Settings::default(), (0.0, 0.0));
}

fn timed_replay(hz: u16) -> Result<Vec3, String> {
    let script = [
        ScriptedHold {
            control: Control::MoveForward,
            first_seconds: 0.0,
            last_seconds: 0.4999,
        },
        ScriptedHold {
            control: Control::MoveForward,
            first_seconds: 1.0,
            last_seconds: 1.4999,
        },
        ScriptedHold {
            control: Control::StrafeRight,
            first_seconds: 0.5,
            last_seconds: 0.9999,
        },
        ScriptedHold {
            control: Control::MoveBackward,
            first_seconds: 2.0,
            last_seconds: 2.4999,
        },
        ScriptedHold {
            control: Control::Jump,
            first_seconds: 1.0,
            last_seconds: 1.2,
        },
        ScriptedHold {
            control: Control::Jump,
            first_seconds: 2.0,
            last_seconds: 2.2,
        },
        ScriptedHold {
            control: Control::Crouch,
            first_seconds: 1.5,
            last_seconds: 1.9999,
        },
    ];
    let mut replay = flat()?;
    let mut command = InputState::default();
    for frame in 0..hz.saturating_mul(3) {
        command.apply_move_script(&script, f32::from(frame) / f32::from(hz));
        if frame == hz / 2 {
            command.accumulate_mouse_motion(75.0, -12.0);
        }
        replay.set_benchmark_simulation_delta(1.0 / f32::from(hz));
        replay.update_player_movement(&mut command, &Settings::default());
    }
    Ok(replay.player_position)
}

#[test]
fn archived_velocity_landmarks_and_render_independent_momentum() -> Result<(), String> {
    let forward = input(&[Control::MoveForward]);
    let empty = input(&[]);
    let mut game = flat()?;
    tick(&mut game, &forward);
    assert!(
        (game.horizontal_velocity.length() / 0.025 - 22.5).abs() < 0.001,
        "first ground command must match the archived 22.5-unit acceleration"
    );
    for _ in 0_u8..40 {
        tick(&mut game, &forward);
    }
    assert!(
        (game.horizontal_velocity.length() - 3.75).abs() < 0.0001,
        "ground wish speed must settle at 150 units"
    );
    for _ in 0_u8..30 {
        tick(&mut game, &empty);
    }
    assert!(
        game.horizontal_velocity.length() < 0.0001,
        "stop-speed friction must finish a released ground run"
    );
    let mut endpoints = Vec::new();
    for hz in [30_u16, 60, 144, 240] {
        let mut replay = flat()?;
        for _ in 0..hz.saturating_mul(3) {
            replay.sim_delta_seconds = 1.0 / f32::from(hz);
            let _origin = replay.update_playing_frame(&forward, &Settings::default(), (0.0, 0.0));
        }
        endpoints.push(replay.player_position);
    }
    assert!(
        endpoints.windows(2).all(|pair| pair
            .first()
            .zip(pair.get(1))
            .is_some_and(|(a, b)| a.distance(*b) < 0.0001)),
        "identical held commands must simulate identically across render rates"
    );
    let mut diagonal = flat()?;
    for _ in 0_u8..40 {
        tick(
            &mut diagonal,
            &input(&[Control::MoveForward, Control::StrafeRight]),
        );
    }
    assert!(
        (diagonal.horizontal_velocity.length() - 3.75).abs() < 0.0001,
        "diagonal input must normalize its wish speed"
    );
    let scripted_endpoints = [30_u16, 60, 144, 240]
        .into_iter()
        .map(timed_replay)
        .collect::<Result<Vec<_>, _>>()?;
    assert!(
        scripted_endpoints.windows(2).all(|pair| pair
            .first()
            .zip(pair.get(1))
            .is_some_and(|(a, b)| a.distance(*b) < 0.0001)),
        "a timed keyboard/mouse command replay must retain its jump, duck and turn trajectory across render rates"
    );
    Ok(())
}

#[test]
fn jump_fixtures_release_debounce_and_projection_air_strafes() -> Result<(), String> {
    let mut game = flat()?;
    game.horizontal_velocity = Vec2::new(0.0, -3.75);
    let jump = input(&[Control::MoveForward, Control::Jump]);
    tick(&mut game, &jump);
    assert!(
        (game.horizontal_velocity.y + 5.625).abs() < 0.0001,
        "running jump adds the archived 75-unit boost before air acceleration"
    );
    for expected in [151.0_f32, 142.0, 133.0, 124.0, 115.0, 106.0, 97.0] {
        assert!(
            (game.vertical_velocity / 0.025 - expected).abs() < 0.002,
            "15 ms gravity trace must match the archived build-5135 velocity landmarks"
        );
        tick(&mut game, &jump);
    }
    for _ in 0_u8..60 {
        tick(&mut game, &jump);
    }
    assert!(game.grounded, "held Jump must not auto-hop on landing");
    tick(&mut game, &input(&[]));
    let speed = game.horizontal_velocity.length();
    tick(&mut game, &jump);
    assert!(
        !game.grounded && game.horizontal_velocity.length() >= speed,
        "released then pressed Jump must launch before ground friction"
    );
    let mut velocities = Vec::new();
    for right in [false, true] {
        let mut air = flat()?;
        air.set_feet_y(4.0);
        air.grounded = false;
        air.horizontal_velocity = Vec2::new(0.0, -5.0);
        tick(
            &mut air,
            &input(&[if right {
                Control::StrafeRight
            } else {
                Control::StrafeLeft
            }]),
        );
        velocities.push(air.horizontal_velocity);
    }
    let left = velocities.first().ok_or("missing left trace")?;
    let right = velocities.get(1).ok_or("missing right trace")?;
    assert!(
        (left.x + right.x).abs() < 0.0001
            && (left.y + 5.0).abs() < 0.0001
            && (right.y + 5.0).abs() < 0.0001,
        "mirrored air strafes preserve existing perpendicular momentum"
    );
    Ok(())
}

#[test]
fn duck_hull_eye_anchor_blocked_expansion_and_short_press() -> Result<(), String> {
    let mut game = flat()?;
    let duck = input(&[Control::Crouch]);
    let empty = input(&[]);
    for _ in 0_u8..27 {
        tick(&mut game, &duck);
    }
    assert!(
        game.is_crouched()
            && game.feet_y.abs() < 0.0001
            && (game.eye_offset() - 0.7).abs() < 0.0001,
        "ground duck holds feet and reaches the 28-unit eye"
    );
    game.set_walls(vec![WallAabb::with_y(19.0, 1.1, 29.0, 2.0, 0.2, 2.0)]);
    tick(&mut game, &empty);
    assert!(
        game.is_crouched(),
        "low ceiling must refuse full standing expansion"
    );
    game.set_walls(Vec::new());
    for _ in 0_u8..14 {
        tick(&mut game, &empty);
    }
    assert!(
        game.stance == Stance::Standing && (game.eye_offset() - EYE_HEIGHT).abs() < 0.0001,
        "release stands when clear after the 0.2 s eye transition"
    );
    game.set_feet_y(4.0);
    game.grounded = false;
    let top = game.feet_y + PLAYER_HEIGHT;
    let eye = game.player_position.y;
    game.update_crouch(&duck);
    game.advance_eye_offset(VERTICAL_SUBSTEP);
    assert!(
        (game.feet_y + game.body_height() - top).abs() < 0.0001
            && (game.player_position.y - eye).abs() < 0.0001,
        "air duck must lift feet while preserving hull top and eye"
    );
    let mut short = flat()?;
    let mut press = input(&[]);
    press.apply_move_script(
        &[ScriptedHold {
            control: Control::Jump,
            first_seconds: 0.0,
            last_seconds: 1.0,
        }],
        0.5,
    );
    short.sim_delta_seconds = 1.0 / 240.0;
    let _press_origin = short.update_playing_frame(&press, &Settings::default(), (0.0, 0.0));
    for _ in 0_u8..3 {
        short.sim_delta_seconds = 1.0 / 240.0;
        let _origin = short.update_playing_frame(&empty, &Settings::default(), (0.0, 0.0));
    }
    assert!(
        !short.grounded && short.vertical_velocity > 0.0,
        "a press released between movement ticks must not disappear"
    );
    Ok(())
}

#[test]
fn square_corner_clearance_and_fast_sweep_do_not_tunnel() -> Result<(), String> {
    let mut game = flat()?;
    game.set_walls(vec![WallAabb::with_y(23.0, 0.0, 28.0, 0.1, 4.0, 4.0)]);
    assert!(
        !game.body_fits_with_height(Vec3::new(22.65, 0.0, 27.65), PLAYER_HEIGHT),
        "the square corner must collide where the former cylinder missed"
    );
    assert!(
        (PLAYER_RADIUS - 0.4).abs() < f32::EPSILON,
        "32-unit hull width must use the same uniform unit scale"
    );
    game.horizontal_velocity = Vec2::new(500.0, 0.0);
    tick(&mut game, &input(&[]));
    assert!(
        game.player_position.x <= 22.6001 && game.horizontal_velocity.length() < 0.1,
        "continuous high-speed collision must stop at the wall without retained launch pressure"
    );
    let mut overhead = flat()?;
    overhead.player_position.x = 22.65;
    overhead.player_position.z = 27.65;
    overhead.set_walls(vec![WallAabb::with_y(23.0, 2.0, 28.0, 0.1, 0.2, 0.1)]);
    let jump = input(&[Control::Jump]);
    for _ in 0_u8..30 {
        tick(&mut overhead, &jump);
        assert!(
            overhead.feet_y + overhead.body_height() < 2.0001,
            "square-corner head contact must stop a jump where the former circular overhead query missed"
        );
    }
    for hz in [30_u16, 144] {
        let mut constrained = flat()?;
        constrained.player_yaw = std::f32::consts::FRAC_PI_2;
        let mut forward = input(&[Control::MoveForward]);
        for _ in 0..hz.saturating_mul(2) {
            constrained.set_benchmark_simulation_delta(1.0 / f32::from(hz));
            constrained.update_player_movement_constrained(
                &mut forward,
                &Settings::default(),
                |state| {
                    Vec3::new(
                        state.player_position.x.min(22.0),
                        state.player_position.y,
                        state.player_position.z,
                    )
                },
            );
        }
        let wall_position = constrained.player_position;
        let mut released = input(&[]);
        constrained.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
        constrained.update_player_movement(&mut released, &Settings::default());
        assert!(
            constrained.player_position.distance(wall_position) < 0.0001,
            "a stopped live-body constraint must spend momentum on command ticks without a release launch at any render rate"
        );
    }
    Ok(())
}

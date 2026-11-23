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

fn spawn_jump_sphere(
    items: &mut crate::engine::physics::ItemWorld,
    key: u64,
    center: Vec3,
    pickup: bool,
) -> Result<(), String> {
    use crate::engine::physics::{BodyMotion, PhysicsDef, ShapeDef};

    let radius = 0.108;
    items.spawn(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup,
            shape: Some(ShapeDef::Sphere { radius }),
            ..PhysicsDef::default()
        },
        (Vec3::splat(-radius), Vec3::splat(radius)),
        center,
        0.0,
        1.0,
    )
}

#[derive(Clone, Copy)]
enum JumpWitness {
    Flight,
    Release,
    Push,
}

fn replay_carry_jump(
    held: bool,
    hitch: Option<u8>,
    witness: JumpWitness,
) -> Result<(f32, f32), String> {
    use crate::engine::physics::{FIXED_STEP, ItemWorld};

    let script = [
        ScriptedHold {
            control: Control::MoveForward,
            first_seconds: 0.0,
            last_seconds: 0.90,
        },
        ScriptedHold {
            control: Control::Jump,
            first_seconds: 0.30,
            last_seconds: 0.40,
        },
    ];
    let mut game = flat()?;
    let mut items = ItemWorld::default();
    let player = game.physical_player_context();
    if held {
        spawn_jump_sphere(
            &mut items,
            1,
            player.view.mul_add(
                Vec3::splat(crate::engine::physics::HOLD_DISTANCE),
                player.eye,
            ),
            true,
        )?;
        items.sync_context(player, &[]);
        assert!(
            items.pickup(1, player.eye, player.view, player.yaw),
            "the fixture sphere must be held before the run-up"
        );
    }
    let mut command = InputState::default();
    let mut highest_feet = game.feet_y();
    let mut elapsed = 0.0;
    for frame in 0_u8..109 {
        command.apply_move_script(&script, elapsed);
        let hitch_frame = Some(frame) == hitch;
        let raw_delta = if hitch_frame { 0.175 } else { FIXED_STEP };
        if hitch_frame && matches!(witness, JumpWitness::Push) {
            let center = items
                .snapshots()
                .first()
                .ok_or("missing held sphere")?
                .world_center_of_mass;
            let obstacle = Vec3::new(center.x, center.y + 0.08, center.z - 0.29);
            spawn_jump_sphere(&mut items, 2, obstacle, false)?;
        }
        game.set_benchmark_simulation_delta(raw_delta);
        game.update_player_movement_with_items(&mut command, &Settings::default(), &mut items);
        let context = game.physical_player_context();
        if hitch_frame && matches!(witness, JumpWitness::Release) {
            items.sync_context(context, &[]);
            assert!(
                game.drop_carried_item(&mut items),
                "release before item physics must see the swept carry pose"
            );
            return Ok((game.player_position.z, game.feet_y()));
        }
        items.step_with_player_velocity(raw_delta, context, &[], game.airborne_velocity());
        if hitch_frame && matches!(witness, JumpWitness::Push) {
            assert!(
                items
                    .contact_impulses()
                    .iter()
                    .any(|row| row.pair == [1, 2] && row.impulse > 0.0),
                "the carried sphere must push the loose one in a real solver tick"
            );
            return Ok((game.player_position.z, game.feet_y()));
        }
        elapsed += game.sim_delta_seconds();
        highest_feet = highest_feet.max(game.feet_y());
        assert_eq!(
            items.held(),
            held.then_some(1),
            "pickup must persist through landing"
        );
    }
    assert!(game.grounded, "the replay must end after landing");
    if held {
        assert!(items.drop(), "the held sphere must still release normally");
    }
    Ok::<_, String>((game.player_position.z, highest_feet))
}

// Subsecond, asset-free replays protect the interaction between the movement
// command clock and the carry solver. A hitch raises the otherwise invisible
// stale-collider failure, the pre-solver release check covers the E ordering,
// and a second sphere detects loss of physical pushback inside the hitch.
#[test]
fn forward_jump_keeps_its_trajectory_while_carrying_in_clear_space() -> Result<(), String> {
    for hitch in [None, Some(44)] {
        let (empty_end, empty_apex) = replay_carry_jump(false, hitch, JumpWitness::Flight)?;
        let (held_end, held_apex) = replay_carry_jump(true, hitch, JumpWitness::Flight)?;
        assert!(
            empty_apex > 0.5,
            "the reference jump must reach its ordinary apex"
        );
        assert!(
            (held_end - empty_end).abs() < 0.01,
            "carry changed forward travel ({hitch:?}): held {held_end}, empty {empty_end}"
        );
        assert!(
            (held_apex - empty_apex).abs() < 0.005,
            "carry shortened the jump ({hitch:?}): held {held_apex}, empty {empty_apex}"
        );
    }
    let _release = replay_carry_jump(true, Some(44), JumpWitness::Release)?;
    let _push = replay_carry_jump(true, Some(44), JumpWitness::Push)?;
    Ok(())
}

// One command per case protects side tangents and two genuine undersides
// (direct contact and an obstruction encountered during a side slide).
// Clear jumps exercise neither contact path. No item solver steps or assets.
#[test]
fn held_body_contact_keeps_side_tangents_and_stops_ascent_at_an_underside() -> Result<(), String> {
    use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef};

    let side = Vec3::new(0.0, 1.6, -0.62);
    let underside = Vec3::new(0.0, PLAYER_HEIGHT + 0.23, 0.0);
    for (relative, aim, velocity, blocks_slide) in [
        (side, Vec3::NEG_Z, Vec2::new(1.0, -3.0), false),
        (underside, Vec3::Y, Vec2::X, false),
        (side, Vec3::NEG_Z, Vec2::new(1.0, -3.0), true),
    ] {
        let mut game = flat()?;
        game.set_feet_y(0.2);
        game.grounded = false;
        game.horizontal_velocity = velocity;
        game.vertical_velocity = 3.0;
        let player = game.physical_player_context();
        let half = Vec3::new(0.35, 0.2, 0.2);
        let mut items = ItemWorld::default();
        let carry_box = PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Cuboid {
                half_extents: half.to_array(),
            }),
            ..PhysicsDef::default()
        };
        items.spawn(
            1,
            &carry_box,
            (-half, half),
            player.feet + relative,
            0.0,
            1.0,
        )?;
        if blocks_slide {
            items.spawn_authored(
                2,
                &PhysicsDef {
                    pickup: false,
                    ..carry_box
                },
                (-half, half),
                player.feet + underside,
                0.0,
                1.0,
            )?;
        }
        items.sync_context(player, &[]);
        assert!(items.pickup(1, player.eye, aim, player.yaw));
        game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
        game.update_player_movement_with_items(
            &mut InputState::default(),
            &Settings::default(),
            &mut items,
        );
        assert!((game.horizontal_velocity.x - 1.0).abs() < 1.0e-4);
        if !blocks_slide {
            assert!((game.player_position.x - player.eye.x - VERTICAL_SUBSTEP).abs() < 1.0e-4);
        }
        if aim == Vec3::Y || blocks_slide {
            assert!(game.ceiling_contact_this_frame);
            assert_eq!(game.vertical_velocity, 0.0);
            assert!(game.feet_y() < 0.23);
        } else {
            assert!(!game.ceiling_contact_this_frame);
            assert!(game.vertical_velocity > 2.7);
            assert!(game.feet_y() > 0.24);
            assert!(game.horizontal_velocity.y.abs() < 1.0e-4);
            assert!(game.player_position.z > player.eye.z - 0.02);
        }
        assert_eq!(items.held(), Some(1));
    }
    Ok(())
}

// Two short walks and one takeoff on the existing flat fixture check the
// rig sample after item resolution. The existing stop checks only inspect
// physics. A single cuboid and fixed commands need no assets or bake.
#[test]
fn player_rig_uses_accepted_motion_and_resolved_support() -> Result<(), String> {
    use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef};
    use crate::engine::scene::LocomotionState;

    for blocked in [false, true] {
        let mut game = flat()?;
        let mut items = ItemWorld::default();
        if blocked {
            let half = Vec3::new(0.6, 0.9, 0.2);
            items.spawn(
                1,
                &PhysicsDef {
                    motion: BodyMotion::Dynamic,
                    shape: Some(ShapeDef::Cuboid {
                        half_extents: half.to_array(),
                    }),
                    ..PhysicsDef::default()
                },
                (-half, half),
                Vec3::new(20.0, 0.9, 29.0),
                0.0,
                1.0,
            )?;
            items.pause();
        }
        for _ in 0_u8..45 {
            items.sync_context(game.physical_player_context(), &[]);
            game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
            game.update_player_movement_with_items(
                &mut input(&[Control::MoveForward]),
                &Settings::default(),
                &mut items,
            );
        }
        items.sync_context(game.physical_player_context(), &[]);
        let before = game.player_position;
        game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
        game.update_player_movement_with_items(
            &mut input(&[Control::MoveForward]),
            &Settings::default(),
            &mut items,
        );
        let accepted = (game.player_position.x - before.x).hypot(game.player_position.z - before.z);
        let snapshot = game.locomotion_snapshot();
        assert!(game.grounded);
        assert!((snapshot.speed - accepted / VERTICAL_SUBSTEP).abs() < 1.0e-5);
        if blocked {
            assert!(game.player_position.z > 29.49);
            assert!(snapshot.speed < 0.001);
            assert_eq!(snapshot.state, LocomotionState::Idle);
        } else {
            assert!(snapshot.speed > 3.0);
            assert_eq!(snapshot.state, LocomotionState::Walking);

            let launch = game.player_position;
            game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
            game.update_player_movement_with_items(
                &mut input(&[Control::MoveForward, Control::Jump]),
                &Settings::default(),
                &mut items,
            );
            let distance =
                (game.player_position.x - launch.x).hypot(game.player_position.z - launch.z);
            let airborne = game.locomotion_snapshot();
            assert!(!game.grounded && game.feet_y() > 0.0);
            assert_eq!(airborne.state, LocomotionState::Airborne);
            assert!((airborne.speed - distance / VERTICAL_SUBSTEP).abs() < 1.0e-5);
        }
    }
    Ok(())
}

// Both frontends release before the item solver. Two takeoffs distinguish
// an ordinary substep-short frame from a timing reset such as refocus or
// closing the console, and also exercise the blocked release retry.
#[test]
fn drop_before_item_step_inherits_accepted_takeoff() -> Result<(), String> {
    use crate::engine::physics::ItemWorld;

    for timing_reset in [false, true] {
        let mut game = flat()?;
        let mut items = ItemWorld::default();
        let player = game.physical_player_context();
        let center = player.eye + Vec3::NEG_Z;
        spawn_jump_sphere(&mut items, 1, center, true)?;
        items.sync_context(player, &[]);
        assert!(items.pickup(1, player.eye, player.view, player.yaw));

        game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
        game.update_player_movement_with_items(
            &mut input(&[Control::MoveForward, Control::Jump]),
            &Settings::default(),
            &mut items,
        );
        let accepted = game.physical_player_context();
        let inherited = ((accepted.feet - player.feet) / VERTICAL_SUBSTEP).clamp_length_max(2.0);
        assert!(
            inherited.y > 0.0 && inherited.z < 0.0,
            "control must move forward and upward"
        );

        if timing_reset {
            game.reset_timing();
        }
        game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP * 0.25);
        game.update_player_movement_with_items(
            &mut InputState::default(),
            &Settings::default(),
            &mut items,
        );
        assert_eq!(game.physical_player_context().feet, accepted.feet);
        items.sync_context(
            crate::engine::physics::PlayerContext {
                feet: Vec3::new(center.x, player.feet.y, center.z),
                eye: center,
                ..accepted
            },
            &[],
        );
        assert!(
            !game.drop_carried_item(&mut items),
            "player penetration must block release"
        );
        assert_eq!(items.held(), Some(1));
        items.sync_context(accepted, &[]);
        assert!(
            game.drop_carried_item(&mut items),
            "returning to accepted feet must clear the release"
        );
        let (_, actual) = items
            .body_pose_velocity(1)
            .ok_or("missing released sphere")?;
        let expected = if timing_reset { Vec3::ZERO } else { inherited };
        assert!(
            actual.distance(expected) < 1.0e-5,
            "timing reset {timing_reset}: actual {actual:?}, expected {expected:?}"
        );
    }
    Ok(())
}

// One movement command against one wall protects both accepted slide crossings
// and rejection of what the controller requested past the wall. No native map
// build or device is involved.
#[test]
fn trigger_sampling_follows_the_solved_wall_slide() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"slide_trigger","name":"Slide trigger",
        "spawn":{"x":20,"z":30},
        "rooms":[{"x":0,"z":0,"width":40,"depth":60,"height":12,"ceiling":{"kind":"open"}}],
        "volumes":[
            {"id":"slide","x":20.19,"z":30.23,"width":0.015,"depth":0.05,"bottom_y":0,"top_y":2,
                "bindings":[{"on":"exit_volume","actions":[{"action":"show_message","title":"slide crossed","lines":["marker"]}]}]},
            {"id":"blocked","x":20.22,"z":30.23,"width":0.1,"depth":0.10,"bottom_y":0,"top_y":2,
                "bindings":[{"on":"enter_volume","actions":[{"action":"show_message","title":"blocked","lines":["marker"]}]}]}
        ]
    }"#).map_err(|error| error.to_string())?;
    let mut game = Game::new(
        Vec3::new(20.0, EYE_HEIGHT + 0.5, 30.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    game.set_app_state(AppState::Playing);
    game.walls
        .push(WallAabb::with_y(20.6, 0.0, 29.5, 1.0, 4.0, 3.0));
    game.collision_index = crate::engine::collision::index::CollisionIndex::build(&game.walls);
    game.set_feet_y(0.5);
    game.grounded = false;
    game.horizontal_velocity = Vec2::new(30.0, 33.0);
    game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    game.update_volume_occupants(&[]);
    assert!(
        game.player_position.x < 20.21 && game.player_position.z > 30.3,
        "the hull stops its horizontal advance and slides along the wall"
    );
    let messages: Vec<_> = game
        .world
        .take_commands()
        .into_iter()
        .filter_map(|command| {
            if let crate::engine::entities::WorldCommand::ShowMessage { title, lines: _ } = command
            {
                Some(title)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        messages,
        ["slide crossed"],
        "only the actual slide passes through a trigger"
    );
    Ok(())
}

// One real movement command in each case detects reversals hidden by the
// endpoint chord, including a ceiling that prevents the predicted exit.
#[test]
fn trigger_sampling_keeps_only_accepted_vertical_peaks() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"peak_trigger","name":"Peak trigger",
        "spawn":{"x":2,"z":2},
        "rooms":[{"width":4,"depth":4,"height":6,"ceiling":{"kind":"open"}}],
        "volumes":[{"id":"zone","x":1,"z":1,"width":2,"depth":2,"bottom_y":0,"top_y":0.5002,
            "bindings":[
                {"on":"exit_volume","actions":[{"action":"show_message","title":"exit","lines":["marker"]}]},
                {"on":"enter_volume","actions":[{"action":"show_message","title":"reentry","lines":["marker"]}]}
            ]}]
    }"#).map_err(|error| error.to_string())?;
    let cases: [(Option<f32>, &[&str]); 3] = [
        (None, &["exit", "reentry"]),
        (Some(2.30045), &["exit", "reentry"]),
        (Some(2.3002), &[]),
    ];
    for (ceiling, expected) in cases {
        let mut game = Game::new(
            Vec3::new(2.0, EYE_HEIGHT + 0.5, 2.0),
            0.0,
            CollisionWorld::from_level(&level),
        );
        game.set_app_state(AppState::Playing);
        if let Some(underside) = ceiling {
            game.walls
                .push(WallAabb::with_y(1.0, underside, 1.0, 2.0, 0.2, 2.0));
            game.collision_index =
                crate::engine::collision::index::CollisionIndex::build(&game.walls);
        }
        game.set_feet_y(0.5);
        game.grounded = false;
        game.vertical_velocity = if ceiling.is_some() {
            0.15
        } else {
            super::super::GRAVITY * VERTICAL_SUBSTEP * 0.5
        };
        game.world.seed_volumes(Vec3::new(2.0, 0.5, 2.0));
        game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
        game.update_player_movement(&mut InputState::default(), &Settings::default());
        game.update_volume_occupants(&[]);
        assert!(
            game.feet_y() <= 0.5002,
            "both command endpoints are inside the trigger"
        );
        let messages: Vec<_> = game
            .world
            .take_commands()
            .into_iter()
            .filter_map(|command| {
                if let crate::engine::entities::WorldCommand::ShowMessage { title, lines: _ } =
                    command
                {
                    Some(title)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            messages, expected,
            "trace must follow only the accepted peak under {ceiling:?}"
        );
    }
    Ok(())
}

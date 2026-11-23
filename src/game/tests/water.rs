//! Essential CPU regressions for water depth/input, entry momentum, exits and
//! timestep stability. One 12 m basin and a short bank, no assets or map bake;
//! a few thousand controller ticks replace a costly full-map test campaign.
use crate::engine::level::{LadderDef, LevelDef};
use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef};
use crate::game::input::{Control, ScriptedHold};
use crate::game::{
    AppState, CollisionWorld, EYE_HEIGHT, Game, InputState, Settings, VERTICAL_SUBSTEP, WaterExit,
};
use glam::{Vec2, Vec3};

fn basin() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"water_test","name":"Water test",
      "spawn":{"x":1,"z":1},"rooms":[{"x":0,"z":0,"width":14,"depth":14,"height":8,"ceiling":{"kind":"open"}}],
      "floor_regions":[{"x":2,"z":2,"width":10,"depth":10,"offset_y":-3}],
      "ramps":[{"x":8,"z":10,"width":2,"depth":3,"offset_y":-3,"rise":3}],
      "water":[{"x":2,"z":2,"width":10,"depth":10,"surface_y":-0.15,"bottom_y":-3}]}"#)
      .map_err(|error| error.to_string())
}

fn placed(level: &LevelDef, x: f32, feet: f32, z: f32, yaw: f32) -> Game {
    let mut game = Game::new(
        Vec3::new(x, feet + EYE_HEIGHT, z),
        yaw,
        CollisionWorld::from_level(level),
    );
    game.set_app_state(AppState::Playing);
    game.player_position.x = x;
    game.player_position.z = z;
    game.set_feet_y(feet);
    game.grounded = false;
    game
}

fn controls(held: &[Control]) -> InputState {
    let mut input = InputState::default();
    let script: Vec<_> = held
        .iter()
        .map(|control| ScriptedHold {
            control: *control,
            first_seconds: 0.0,
            last_seconds: 100.0,
        })
        .collect();
    input.apply_move_script(&script, 1.0);
    input.clear_presses();
    input
}

fn tick(game: &mut Game, input: &InputState, delta: f32) {
    game.sim_delta_seconds = delta;
    let _origin = game.update_playing_frame(input, &Settings::default(), (0.0, 0.0));
}

#[test]
fn waist_eye_depth_view_directed_swimming_and_drag_are_stable() -> Result<(), String> {
    let level = basin()?;
    let mut endpoints = Vec::new();
    for hz in [30_u16, 60, 144] {
        let delta = 1.0 / f32::from(hz);
        let mut game = placed(&level, 6.0, -1.4, 6.0, 0.0);
        assert_eq!(
            game.water_depth_level(),
            2,
            "waist wet, eye dry must swim even over a standable floor"
        );
        game.player_pitch = -0.5;
        let forward = controls(&[Control::MoveForward]);
        for _ in 0..hz {
            tick(&mut game, &forward, delta);
        }
        assert!(
            game.is_underwater() && game.feet_y < -1.6,
            "view-directed forward input must dive without crouch"
        );
        assert!(
            game.horizontal_velocity.length() < 2.41,
            "3D wish normalization must bound swim speed"
        );
        let speed = game.horizontal_velocity.length();
        for _ in 0..hz {
            tick(&mut game, &InputState::default(), delta);
        }
        assert!(
            game.horizontal_velocity.length() < speed * 0.05,
            "released swim input must coast under fluid friction"
        );
        endpoints.push(game.player_position);
    }
    // A counter-current stop can resolve to zero planar speed. It must not
    // normalize zero merely because the surrounding fluid is still moving.
    let mut current_level = level;
    let current = &mut current_level
        .water
        .first_mut()
        .ok_or("missing water")?
        .motion;
    current.flow_speed = 0.8;
    current.direction_degrees = 90.0;
    let mut stopped = placed(&current_level, 6.0, -1.4, 6.0, 0.0);
    stopped.swimming = true;
    let before = stopped.player_position;
    let sample = stopped.water.sample(6.0, 6.0, stopped.feet_y);
    let distance =
        stopped.move_on_surface(Vec3::ZERO, &Settings::default(), 1.0 / 120.0, sample, false);
    assert!(
        stopped.player_position.is_finite()
            && stopped.player_position.distance(before) < 0.0001
            && distance < 0.0001,
        "cancelled current momentum must preserve a finite stationary pose"
    );
    let first = endpoints.first().ok_or("missing trajectory")?;
    assert!(
        endpoints.iter().all(|point| point.distance(*first) < 0.07),
        "30/60/144 Hz must share a bounded trajectory"
    );
    Ok(())
}

#[test]
fn plunge_preserves_momentum_and_rise_resurfaces_without_a_pose_snap() -> Result<(), String> {
    let level = basin()?;
    let mut game = placed(&level, 6.0, -1.1, 6.0, 0.0);
    game.vertical_velocity = -6.0;
    tick(
        &mut game,
        &InputState::default(),
        crate::game::VERTICAL_SUBSTEP,
    );
    assert!(
        game.vertical_velocity < -5.5 && game.vertical_velocity > -6.0,
        "entry must dissipate plunge momentum through drag instead of clamping it to a swim cap"
    );
    game.set_feet_y(-2.5);
    game.vertical_velocity = 0.0;
    let jump = controls(&[Control::Jump]);
    for _ in 0_u16..240 {
        let before = game.feet_y;
        tick(&mut game, &jump, 1.0 / 120.0);
        assert!(
            (game.feet_y - before).abs() < 0.06,
            "water rise/surface transition must be continuous"
        );
    }
    assert!(
        !game.is_underwater(),
        "held rise must reach breathable water"
    );
    assert!(
        game.feet_y < 0.0 && game.feet_y > -1.3,
        "surface transition must remain around the waist waterline without a launch"
    );
    Ok(())
}

#[test]
fn ramp_and_checked_ledge_exit_use_real_support_without_reentry_or_reset() -> Result<(), String> {
    let level = basin()?;
    let forward = controls(&[Control::MoveForward]);
    let mut ramp = placed(&level, 9.0, -3.0, 9.6, std::f32::consts::PI);
    ramp.grounded = true;
    for _ in 0_u16..180 {
        tick(&mut ramp, &forward, 1.0 / 120.0);
    }
    assert!(
        ramp.player_position.z > 13.0 && ramp.grounded && ramp.feet_y.abs() < 0.002,
        "bottom contact must walk continuously up the real bank: position={:?} feet={} grounded={}",
        ramp.player_position,
        ramp.feet_y,
        ramp.grounded
    );
    let mut ledge = placed(&level, 6.0, -1.1, 11.6, std::f32::consts::PI);
    assert!(
        ledge
            .exit_probe_floor(glam::Vec2::new(6.0, 12.04), -0.15)
            .is_some(),
        "real deck probe must be eligible: head={:?} walls={:?}",
        ledge.head_limit(0.0),
        ledge
            .walls
            .iter()
            .filter(|wall| wall.overlaps_disc(6.0, 12.04, 0.0001))
            .collect::<Vec<_>>()
    );
    let rising = controls(&[Control::MoveForward, Control::Jump]);
    let mut exit_seen = false;
    for _ in 0_u16..360 {
        tick(&mut ledge, &rising, 1.0 / 120.0);
        exit_seen |= ledge.water_exit.is_some();
        if ledge.player_position.z > 12.8 {
            break;
        }
    }
    for _ in 0_u16..90 {
        tick(&mut ledge, &controls(&[Control::Jump]), 1.0 / 120.0);
    }
    assert!(
        exit_seen && ledge.player_position.z > 12.1 && ledge.grounded,
        "forward surface approach must complete the checked water jump onto the real deck: seen={exit_seen} pos={:?} feet={} grounded={} depth={} vv={}",
        ledge.player_position,
        ledge.feet_y,
        ledge.grounded,
        ledge.water_depth_level(),
        ledge.vertical_velocity
    );
    assert!(
        ledge.feet_y.abs() < 0.002 && ledge.reset_count() == 0,
        "held jump must not bounce on exit or trigger recovery"
    );
    let mut roof_source = serde_json::to_value(&level).map_err(|error| error.to_string())?;
    let room = roof_source.pointer_mut("/rooms/0").ok_or("missing room")?;
    room["height"] = serde_json::json!(1.0_f32);
    room["ceiling"] = serde_json::json!({"kind":"flat"});
    let covered_level: LevelDef =
        serde_json::from_value(roof_source).map_err(|error| error.to_string())?;
    let mut covered = placed(&covered_level, 6.0, -1.1, 11.59, std::f32::consts::PI);
    for _ in 0_u16..120 {
        tick(&mut covered, &rising, 1.0 / 120.0);
        assert!(
            covered.water_exit.is_none(),
            "low overhead space must refuse the water jump"
        );
    }
    assert!(
        covered.player_position.z < 11.61 && covered.feet_y + covered.body_height() <= 1.001,
        "a refused exit must preserve the actual rim and head contacts"
    );
    let before_dive = covered.feet_y;
    covered.player_pitch = -0.8;
    for _ in 0_u16..60 {
        tick(&mut covered, &forward, 1.0 / 120.0);
    }
    assert!(
        covered.water_exit.is_none() && covered.feet_y < before_dive - 0.2,
        "after the exit is refused, looking down and swimming must still dive"
    );
    Ok(())
}

// The native flow-bank reproduction exposed a side-current launch. This tiny
// bank reproduces the directional-contact distinction in 240 CPU ticks.
#[test]
fn sideways_current_cannot_turn_a_walkable_bank_into_a_water_jump() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"current_bank","name":"Current bank","spawn":{"x":1,"z":1},
      "rooms":[{"x":0,"z":0,"width":12,"depth":15,"height":8,"ceiling":{"kind":"open"}}],
      "floor_regions":[{"x":2,"z":2,"width":8,"depth":8,"offset_y":-2.4}],
      "ramps":[{"x":8,"z":10,"width":1.4,"depth":4,"offset_y":-2.4,"rise":2.4}],
      "water":[{"x":2,"z":2,"width":8,"depth":8,"surface_y":-0.15,"bottom_y":-2.4,"motion":{"flow_speed":0.8,"direction_degrees":90}},
      {"x":8,"z":10,"width":1.4,"depth":3.7,"surface_y":-0.15,"bottom_y":-2.4,"motion":{"flow_speed":0.8,"direction_degrees":90}}]}"#).map_err(|error|error.to_string())?;
    for speed in [0.8_f32, 4.0] {
        let mut source = level.clone();
        for water in &mut source.water {
            water.motion.flow_speed = speed;
        }
        let mut game = placed(&source, 8.7, -2.4, 9.6, std::f32::consts::PI);
        game.grounded = true;
        let input = controls(&[Control::MoveForward]);
        for _ in 0_u16..240 {
            tick(&mut game, &input, 1.0 / 120.0);
            assert!(
                game.water_exit.is_none(),
                "current-driven side contact must never start a forward water jump"
            );
            if game.player_position.z > 13.5 {
                break;
            }
        }
        assert!(
            game.player_position.z > 13.5 && game.grounded && game.reset_count() == 0,
            "the real bank must remain continuously traversable under sideways flow"
        );
    }
    Ok(())
}

// Protect the documented 0.5 m rim limit with the standing hull. This needs
// only one metadata edit and 300 CPU ticks; the former fixed arc fell short.
#[test]
fn maximum_authored_rim_remains_reachable_by_a_standing_swimmer() -> Result<(), String> {
    let mut source = serde_json::to_value(basin()?).map_err(|error| error.to_string())?;
    let room = source.pointer_mut("/rooms/0").ok_or("missing room")?;
    room["floor_y"] = serde_json::json!(0.35_f32);
    let level: LevelDef = serde_json::from_value(source).map_err(|error| error.to_string())?;
    let mut game = placed(&level, 6.0, -1.1, 11.59, std::f32::consts::PI);
    let input = controls(&[Control::MoveForward, Control::Jump]);
    for _ in 0_u16..240 {
        tick(&mut game, &input, 1.0 / 120.0);
        if game.player_position.z > 12.8 {
            break;
        }
    }
    for _ in 0_u16..90 {
        tick(&mut game, &controls(&[Control::Jump]), 1.0 / 120.0);
    }
    assert!(
        game.grounded && game.player_position.z > 12.1 && (game.feet_y - 0.35).abs() < 0.002,
        "the standing swimmer must clear the documented maximum waterline rim"
    );
    Ok(())
}

fn spawn_dormant_box(
    items: &mut ItemWorld,
    key: u64,
    center: Vec3,
    half: Vec3,
) -> Result<(), String> {
    items.spawn_authored(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            shape: Some(ShapeDef::Cuboid {
                half_extents: half.to_array(),
            }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-half.x, -half.y, -half.z), half),
        center,
        0.0,
        1.0,
    )
}

fn tick_with_items(game: &mut Game, items: &mut ItemWorld, mut input: InputState) {
    items.sync_context(game.physical_player_context(), &[]);
    game.sim_delta_seconds = VERTICAL_SUBSTEP;
    game.update_player_movement_with_items(&mut input, &Settings::default(), items);
}

// One command per case, with one or two dormant boxes. A side hit precedes
// the possible underside, exercising the accepted path that a room-roof
// clamp or a single diagonal cast cannot distinguish. No solver or map bake.
#[test]
fn water_ascent_reaches_only_ceilings_over_the_accepted_footprint() -> Result<(), String> {
    let level = basin()?;
    for exiting in [false, true] {
        for (side, roof, ceiling) in [
            (true, None, false),
            (true, Some(0.0), true),
            (true, Some(0.014), false),
            (false, Some(0.0), true),
        ] {
            let mut game = placed(&level, 6.0, -1.25, 6.0, 0.0);
            let start = game.physical_player_context();
            let mut items = ItemWorld::default();
            if side {
                let half = Vec3::new(0.2, 0.75, 0.3);
                spawn_dormant_box(
                    &mut items,
                    1,
                    start.feet + Vec3::new(start.half + 0.009 + half.x, 0.65, 0.0),
                    half,
                )?;
            }
            let underside_rise = 0.025;
            if let Some(overhang) = roof {
                let half = Vec3::new(0.2, 0.15, 0.3);
                let x = if overhang > 0.0 {
                    start.half + overhang + half.x
                } else {
                    0.0
                };
                spawn_dormant_box(
                    &mut items,
                    2,
                    start.feet + Vec3::new(x, start.height + underside_rise + half.y, 0.0),
                    half,
                )?;
            }
            game.horizontal_velocity = Vec2::new(2.0, 0.0);
            game.vertical_velocity = 4.0;
            if exiting {
                game.water_exit = Some(WaterExit {
                    support_y: 0.0,
                    surface_y: -0.15,
                    direction: Vec2::X,
                    remaining: 2.0,
                });
            }
            tick_with_items(
                &mut game,
                &mut items,
                controls(&[Control::StrafeRight, Control::Jump]),
            );
            let rise = game.feet_y - start.feet.y;
            assert_eq!(
                game.ceiling_contact_this_frame, ceiling,
                "wrong ceiling at exiting={exiting}, side={side}, roof={roof:?}, rise={rise}"
            );
            if side {
                assert!(
                    game.player_position.x < start.feet.x + 0.008,
                    "the near side must block the planar approach"
                );
            }
            if ceiling {
                assert!(
                    (0.02..=underside_rise).contains(&rise)
                        && game.vertical_velocity <= 0.0
                        && game.water_exit.is_none(),
                    "reachable underside must consume ascent and end any exit, rise={rise}"
                );
            } else {
                assert!(
                    rise > underside_rise && game.vertical_velocity > 0.0,
                    "side contact must retain available ascent, rise={rise}"
                );
                assert_eq!(game.water_exit.is_some(), exiting);
            }
        }
    }
    Ok(())
}

// A real rim admits the first exit. A single dormant box appears during
// the arc, so repeated forward input must remain a dive after the impact.
// One basin and at most 42 commands exercise re-admission without a bake.
#[test]
fn dormant_overhang_at_a_real_rim_does_not_relaunch_a_diving_swimmer() -> Result<(), String> {
    let level = basin()?;
    let mut game = placed(&level, 6.0, -1.18, 11.59, std::f32::consts::PI);
    let mut items = ItemWorld::default();
    tick_with_items(
        &mut game,
        &mut items,
        controls(&[Control::MoveForward, Control::Jump]),
    );
    assert!(
        game.water_exit.is_some(),
        "the open bank must admit an ordinary exit"
    );

    let player = game.physical_player_context();
    let half = Vec3::new(0.6, 0.15, 0.65);
    spawn_dormant_box(
        &mut items,
        1,
        player.feet + Vec3::new(0.0, player.height + 0.025 + half.y, 0.0),
        half,
    )?;
    let forward = controls(&[Control::MoveForward]);
    tick_with_items(&mut game, &mut items, forward);
    assert!(
        game.ceiling_contact_this_frame
            && game.water_exit.is_none()
            && game.vertical_velocity <= 0.0
            && game.water_depth_level() == 2,
        "the active exit must hit the dormant underside and return to waist-deep water"
    );

    let impact_feet = game.feet_y;
    game.player_pitch = -0.8;
    for _ in 0..40_u8 {
        tick_with_items(&mut game, &mut items, forward);
        assert!(
            game.water_exit.is_none()
                && !game.ceiling_contact_this_frame
                && game.vertical_velocity < 0.0,
            "continued forward dive must not rearm the blocked exit"
        );
    }
    assert!(
        items.body_is_dormant(1) && game.feet_y < impact_feet - 0.45 && game.is_underwater(),
        "the swimmer must dive back underwater with the same overhang still present"
    );
    Ok(())
}

// Two commands with an oblique approach distinguish a clipped path from a
// held-item slide. A second box tests the newly reachable footprint. Dropping
// before any solver step checks release inheritance from accepted movement.
#[test]
fn held_swim_ascent_slides_then_releases_with_accepted_velocity() -> Result<(), String> {
    let level = basin()?;
    for roof in [false, true] {
        let mut game = placed(&level, 6.0, -1.5, 6.0, 0.0);
        let start = game.physical_player_context();
        let mut items = ItemWorld::default();
        let half = Vec3::new(0.35, 0.2, 0.2);
        items.spawn(
            1,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                pickup: true,
                shape: Some(ShapeDef::Cuboid {
                    half_extents: half.to_array(),
                }),
                ..PhysicsDef::default()
            },
            (Vec3::new(-half.x, -half.y, -half.z), half),
            start.feet + Vec3::new(0.0, 1.6, -(start.half + half.z + 0.02)),
            0.0,
            1.0,
        )?;
        if roof {
            let roof_half = Vec3::new(0.2, 0.15, 0.3);
            spawn_dormant_box(
                &mut items,
                2,
                start.feet
                    + Vec3::new(
                        start.half + 0.017 + roof_half.x,
                        start.height + 0.025 + roof_half.y,
                        0.0,
                    ),
                roof_half,
            )?;
        }
        items.sync_context(start, &[]);
        assert!(
            items.pickup(1, start.eye, start.view, start.yaw),
            "fixture box must be held before the oblique approach"
        );
        game.horizontal_velocity = Vec2::new(1.8, -3.0);
        tick_with_items(&mut game, &mut items, controls(&[Control::Jump]));

        let accepted = game.physical_player_context();
        let travel = accepted.feet - start.feet;
        assert!(
            travel.x > 0.02 && (-0.019..-0.005).contains(&travel.z),
            "held side contact must keep the tangent and block inward motion: {travel:?}"
        );
        assert!(
            items.player_pose_clear(accepted),
            "the slide must remain clear"
        );
        assert_eq!(
            game.ceiling_contact_this_frame, roof,
            "only the underside reached by the accepted slide may stop ascent"
        );
        if roof {
            assert!(
                (0.02..0.025).contains(&travel.y) && game.vertical_velocity <= 0.0,
                "the roof reached after sliding must stop ascent: {travel:?}"
            );
        } else {
            assert!(
                travel.y > 0.03 && game.vertical_velocity > 0.0,
                "a held side contact alone must allow ascent: {travel:?}"
            );
        }

        items.sync_context(accepted, &[]);
        assert!(
            game.drop_carried_item(&mut items),
            "the clear held box must release"
        );
        let (_, velocity) = items.body_pose_velocity(1).ok_or("missing released box")?;
        let inherited = (travel / VERTICAL_SUBSTEP).clamp_length_max(2.0);
        assert!(
            velocity.x > 0.8 && velocity.distance(inherited) < 1.0e-5,
            "release must inherit the accepted tangent and ascent: {velocity:?}, {inherited:?}"
        );
    }
    Ok(())
}

// A landing followed by a jump on one dormant box protects support across
// movement commands. No solver steps, assets or external fixture are needed.
#[test]
fn dormant_prop_support_allows_the_next_jump() -> Result<(), String> {
    let level = basin()?;
    let mut game = placed(&level, 1.0, 1.035, 1.0, 0.0);
    let mut items = ItemWorld::default();
    spawn_dormant_box(
        &mut items,
        1,
        Vec3::new(1.0, 0.5, 1.0),
        Vec3::new(0.4, 0.5, 0.4),
    )?;
    game.vertical_velocity = -3.0;
    tick_with_items(&mut game, &mut items, InputState::default());
    assert!(
        game.grounded && (game.feet_y - 1.0).abs() < 0.01,
        "the dormant prop must catch the fall"
    );
    let landed = game.feet_y;
    tick_with_items(&mut game, &mut items, controls(&[Control::Jump]));
    assert!(
        !game.grounded && game.feet_y > landed + 0.02 && game.vertical_velocity > 0.0,
        "the jump must use the dormant support from the preceding command"
    );
    Ok(())
}

// A ladder inside the basin distinguishes climbing from swimming ascent.
// Five commands check forward, idle, reverse and jump with no builds or GPU.
#[test]
fn ladder_controls_keep_priority_over_water_ascent() -> Result<(), String> {
    let mut level = basin()?;
    level.ladders.push(LadderDef {
        x: 5.7,
        z: 5.5,
        width: 0.6,
        depth: 0.2,
        bottom_y: -3.0,
        top_y: 1.0,
        facing_degrees: 0.0,
    });
    let mut game = placed(&level, 6.0, -1.8, 5.9, 0.0);
    let mut items = ItemWorld::default();
    let before = game.feet_y;
    tick_with_items(&mut game, &mut items, controls(&[Control::MoveForward]));
    assert!(
        game.climbing.is_some() && game.feet_y > before && !game.swimming,
        "forward input must climb the submerged ladder"
    );
    let attached = game.feet_y;
    tick_with_items(&mut game, &mut items, InputState::default());
    assert!(
        game.climbing.is_some() && (game.feet_y - attached).abs() < 0.0001,
        "idle input must hold the attached height"
    );
    tick_with_items(&mut game, &mut items, controls(&[Control::MoveBackward]));
    assert!(game.climbing.is_none(), "backward input must detach");
    tick_with_items(&mut game, &mut items, controls(&[Control::MoveForward]));
    assert!(game.climbing.is_some(), "forward input must reattach");
    tick_with_items(&mut game, &mut items, controls(&[Control::Jump]));
    assert!(game.climbing.is_none(), "jump must detach the climber");
    Ok(())
}

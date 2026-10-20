//! Essential CPU regressions for water depth/input, entry momentum, exits and
//! timestep stability. One 12 m basin and a short bank, no assets or map bake;
//! a few thousand controller ticks replace a costly full-map test campaign.
use crate::engine::level::LevelDef;
use crate::game::input::{Control, ScriptedHold};
use crate::game::{CollisionWorld, EYE_HEIGHT, Game, InputState, Settings};
use glam::Vec3;

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
    tick(&mut game, &InputState::default(), 1.0 / 120.0);
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
    let mut ramp = placed(&level, 9.0, -3.0, 10.0, std::f32::consts::PI);
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
    let mut covered = placed(&covered_level, 6.0, -1.1, 11.69, std::f32::consts::PI);
    for _ in 0_u16..120 {
        tick(&mut covered, &rising, 1.0 / 120.0);
        assert!(
            covered.water_exit.is_none(),
            "low overhead space must refuse the water jump"
        );
    }
    assert!(
        covered.player_position.z < 11.71 && covered.feet_y + covered.body_height() <= 1.001,
        "a refused exit must preserve the actual rim and head contacts"
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
    let mut game = placed(&level, 6.0, -1.1, 11.69, std::f32::consts::PI);
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

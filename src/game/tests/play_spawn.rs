//! Protects strict launch parsing and safe storey/body placement. Tiny in-memory
//! geometry, no assets, subprocesses or baking; these boundary cases are unique
//! to the ephemeral editor launch path and run in milliseconds.
use crate::engine::level::LevelDef;
use crate::game::{CollisionWorld, EYE_HEIGHT, Game, MAX_PITCH, play_spawn::Pose};
use glam::Vec3;

#[test]
fn play_here_pose_rejects_invalid_tuples_and_clamps_pitch() -> Result<(), String> {
    for invalid in [
        "1,NaN,2,90,0",
        "1,2,3,inf,0",
        "1,2,3,90,NaN",
        "1,2,bad,90,0",
        "1,2,3,90",
        "1,2,3,90,0,1",
    ] {
        assert!(
            Pose::parse(invalid).is_none(),
            "malformed tuple must be rejected whole: {invalid}"
        );
    }
    let pose = Pose::parse("1,6.6,2,450,90").ok_or("valid pose rejected")?;
    assert!(
        (pose.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.00001,
        "yaw wraps in radians"
    );
    assert!(
        (pose.pitch - MAX_PITCH).abs() < 0.00001,
        "pitch stays in the normal player range"
    );
    let roundtrip = Pose::parse(&pose.encode()).ok_or("roundtrip rejected")?;
    assert!(
        roundtrip.eye.distance(pose.eye) < 0.00001,
        "explicit camera eye survives encoding"
    );
    Ok(())
}

#[test]
fn play_here_rejects_void_obstructions_and_wrong_storey_without_recovery() -> Result<(), String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"launch","name":"Launch","spawn":{"x":1,"z":1},
    "rooms":[{"width":12,"depth":12,"height":3},{"width":12,"depth":12,"floor_y":5,"height":3},{"x":14,"z":0,"width":3,"depth":3,"height":1.6}],
    "doors":[{"id":"closed","x":4,"z":2,"width":1,"height":2,"thickness":0.05}],
    "floor_regions":[{"x":4,"z":4,"width":2,"depth":2,"offset_y":-1}],
    "walls":[{"x":2,"z":2,"width":1,"depth":1,"height":3}],
    "props":[{"model":"core:chair","x":7,"z":2,"solid":true,"size":[1,2,1]}],
    "stairs":[{"x":8,"z":4,"width":2,"depth":6,"rise":2,"steps":10}]}"#).map_err(|error|error.to_string())?;
    let mut game = Game::new(
        Vec3::new(1.0, EYE_HEIGHT, 1.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    for floor in [
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(1.0, 5.0, 1.0),
        Vec3::new(5.0, -1.0, 5.0),
        Vec3::new(9.0, 1.0, 6.5),
    ] {
        let pose = game.play_pose_on_floor(floor, 0.7, -0.3)?;
        game.apply_play_pose(pose)?;
        assert!(
            game.player_position.distance(pose.eye) < 0.0001,
            "safe launch may not silently recover to another position"
        );
        assert!(
            (game.player_pitch + 0.3).abs() < 0.0001,
            "pitch must survive launch"
        );
    }
    for eye in [
        Vec3::new(20.0, EYE_HEIGHT, 20.0),
        Vec3::new(1.0, 4.0, 1.0),
        Vec3::new(2.5, EYE_HEIGHT, 2.5),
        Vec3::new(7.0, EYE_HEIGHT, 2.0),
        Vec3::new(1.0, 2.9, 1.0),
        Vec3::new(4.5, EYE_HEIGHT, 2.0),
        Vec3::new(15.0, EYE_HEIGHT, 1.0),
    ] {
        let before = game.player_position;
        assert!(
            game.validate_play_pose(Pose::new(eye, 0.0, 0.0)?).is_err(),
            "unsafe point must fail: {eye:?}"
        );
        assert!(
            game.player_position.distance(before) < 0.0001,
            "validation must not move the player"
        );
    }
    Ok(())
}

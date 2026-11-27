//! One small solver witness: yaw the player around the same carry pivot.
use super::{Bell, FIRM, SOFT};
use crate::engine::{
    level::LevelDef,
    physics::{BodyMotion, FIXED_STEP, HOLD_DISTANCE, ItemWorld, PhysicsDef, PlayerContext},
};
use glam::Vec3;

fn orbit(center: Vec3, yaw: f32) -> PlayerContext {
    let view = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
    let eye = Vec3::splat(-HOLD_DISTANCE).mul_add(view, center);
    PlayerContext {
        eye,
        feet: Vec3::new(eye.x, 0.0, eye.z),
        height: 1.72,
        half: 0.26,
        view,
        yaw,
        active: true,
    }
}

// The solver, not a synthetic sound velocity, accepts rotation while the
// handle's center stays put. One cuboid in an empty room, no graphics or SDL.
#[test]
fn accepted_open_space_rotation_drives_a_soft_jingle_at_near_zero_translation() -> Result<(), String>
{
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"bell_rotation","name":"Bell rotation","spawn":{"x":4,"z":4},"rooms":[{"width":8,"depth":8}]}"#,
    ).map_err(|error| error.to_string())?;
    let center = Vec3::new(4.0, 1.62, 3.0);
    let key = 17;
    let mut world = ItemWorld::from_level(&level, &[]);
    world.spawn(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.19, -0.03, -0.04), Vec3::new(0.19, 0.03, 0.04)),
        center,
        0.0,
        1.0,
    )?;
    let initial = orbit(center, 0.0);
    assert!(world.pickup(key, initial.eye, initial.view, initial.yaw));
    for i in 1_u8..=8 {
        world.step(FIXED_STEP, orbit(center, f32::from(i) * 0.020), &[]);
    }
    let (_, linear, angular) = world
        .resolved_body_motion(key)
        .ok_or("Held bell motion missing")?;
    assert!(
        linear.length() < 0.05 && angular.length() > 1.5,
        "{linear:?} {angular:?}"
    );
    assert_eq!(world.held(), Some(key));
    let mut bell = Bell {
        key,
        cooldown: 0.0,
        moving: false,
    };
    assert!(
        bell.advance(Vec3::splat(0.012), Vec3::splat(0.08), true, FIXED_STEP)
            .is_none()
    );
    let gentle = bell
        .advance(linear, angular, true, FIXED_STEP)
        .ok_or("Accepted swivel did not jingle")?;
    let vigorous = bell
        .advance(Vec3::X, Vec3::ZERO, false, 0.5)
        .ok_or("Free movement did not jingle")?;
    assert_eq!(gentle.cue, SOFT);
    assert_eq!(vigorous.cue, FIRM);
    assert!(vigorous.gain > gentle.gain && vigorous.interval < gentle.interval);
    assert!(bell.advance(Vec3::ZERO, Vec3::ZERO, false, 0.5).is_none());
    Ok(())
}

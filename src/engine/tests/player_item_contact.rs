//! One sphere exercises dormant contact wakeup and fixed-body separation.
//! A wall-free metadata world and a handful of fixed steps cost milliseconds;
//! this specifically protects the all-dormant early-return activation gap.

use crate::engine::level::LevelDef;
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};
use glam::Vec3;

#[test]
fn player_contact_activates_dormant_movable_bodies_but_preserves_fixed_bodies() -> Result<(), String>
{
    for motion in [BodyMotion::Dynamic, BodyMotion::Fixed] {
        let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"contact","name":"Contact","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
        let mut world = ItemWorld::from_level(&level, &[]);
        world.spawn_authored(
            1,
            &PhysicsDef {
                motion,
                shape: Some(ShapeDef::Sphere { radius: 0.2 }),
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.2), Vec3::splat(0.2)),
            Vec3::new(0.0, 0.9, -0.7),
            0.0,
            1.0,
        )?;
        let player = PlayerContext {
            eye: Vec3::new(0.0, 1.6, 0.0),
            feet: Vec3::ZERO,
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: true,
        };
        world.sync_context(player, &[]);
        let desired = PlayerContext {
            eye: Vec3::new(0.0, 1.6, -0.4),
            feet: Vec3::new(0.0, 0.0, -0.4),
            ..player
        };
        let eye = world.constrain_player_eye(desired);
        let contact = PlayerContext {
            eye,
            feet: Vec3::new(eye.x, eye.y - 1.6, eye.z),
            ..player
        };
        world.step(FIXED_STEP, contact, &[]);
        if motion == BodyMotion::Dynamic {
            assert!(
                !world.body_is_dormant(1),
                "a controller sweep contact must wake the first movable body"
            );
        } else {
            assert_eq!(
                world.body_motion(1),
                Some(BodyMotion::Fixed),
                "static definitions must stay fixed"
            );
        }
    }
    Ok(())
}

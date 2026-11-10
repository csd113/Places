//! Protects tool contact wakeup, light/heavy throw-strength response and fixed
//! ownership. Three one-sphere worlds, no solver ticks, assets or map builds.
use crate::engine::{
    level::LevelDef,
    physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef},
};
use glam::Vec3;

#[test]
fn crowbar_contact_wakes_light_props_with_throw_strength_but_preserves_fixed_bodies()
-> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"hit","name":"Hit","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
    for (mass, motion) in [
        (1.0_f32, BodyMotion::Dynamic),
        (16.0, BodyMotion::Dynamic),
        (1.0, BodyMotion::Fixed),
    ] {
        let mut world = ItemWorld::from_level(&level, &[]);
        world.spawn_authored(
            1,
            &PhysicsDef {
                motion,
                mass_kg: mass,
                shape: Some(ShapeDef::Sphere { radius: 0.2 }),
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.2), Vec3::splat(0.2)),
            Vec3::new(0.0, 2.0, -1.0),
            0.0,
            1.0,
        )?;
        let accepted = world.strike_body(1, Vec3::NEG_Z, Vec3::new(0.0, 2.0, -1.0));
        if motion == BodyMotion::Dynamic {
            assert!(
                accepted && !world.body_is_dormant(1),
                "tool contact activates dormant bodies"
            );
            let body = world
                .snapshots()
                .into_iter()
                .next()
                .ok_or("missing struck body")?;
            assert!(
                (body.linear_velocity.z + 8.0 / mass.sqrt()).abs() < 1.0e-4,
                "same mass curve as throw"
            );
        } else {
            assert!(!accepted, "fixed bodies never become movable tools");
            assert_eq!(world.body_motion(1), Some(BodyMotion::Fixed));
        }
        assert!(
            world.held().is_none(),
            "contact does not claim carry ownership"
        );
    }
    Ok(())
}

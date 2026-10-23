//! One floor and one sphere protect release at ordinary resting contact.
//! Pickup followed by immediate set-down must not be rejected as penetration;
//! a genuine overlapping player must still refuse release. No assets or GPU.

use crate::engine::level::LevelDef;
use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, PlayerContext, ShapeDef};
use glam::Vec3;

#[test]
fn resting_contact_allows_release_but_player_penetration_retains_ownership() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"release","name":"Release","spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4}]}"#).map_err(|error| error.to_string())?;
    let mut world = ItemWorld::from_level(&level, &[]);
    world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.1 }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        Vec3::new(0.0, -0.00001, -1.0),
        0.0,
        1.0,
    )?;
    let player = PlayerContext {
        half: crate::engine::collision::PLAYER_RADIUS,
        eye: Vec3::new(0.0, 1.6, 0.0),
        feet: Vec3::ZERO,
        height: 1.8,
        view: Vec3::new(0.0, -1.5, -1.0).normalize(),
        yaw: 0.0,
        active: true,
    };
    world.sync_context(player, &[]);
    assert!(
        world.pickup(1, player.eye, player.view, 0.0),
        "floor item is reachable"
    );
    assert!(
        world.can_drop(),
        "solver-scale resting contact is a valid release pose"
    );
    world.sync_context(
        PlayerContext {
            eye: Vec3::new(0.0, 1.6, -1.0),
            feet: Vec3::new(0.0, 0.0, -1.0),
            ..player
        },
        &[],
    );
    assert!(!world.drop(), "real body penetration must remain blocked");
    assert_eq!(world.held(), Some(1), "a blocked drop retains custody");
    world.sync_context(player, &[]);
    assert!(
        world.drop(),
        "retry after separation succeeds at resting contact"
    );
    assert!(
        world.held().is_none(),
        "one successful release clears custody"
    );
    Ok(())
}

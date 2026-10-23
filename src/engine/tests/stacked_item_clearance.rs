//! Two tiny rooms and one sphere protect a real campaign blocker: a lower
//! ceiling's artificial backing trapped an item on the next floor. Partial
//! overlap also protects the uncovered ceiling and the upper floor. No assets,
//! lighting, GPU or map bake; sixty small fixed steps cost milliseconds.

use glam::Vec3;

use crate::engine::level::LevelDef;
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};

#[test]
fn shared_storey_plane_keeps_upper_items_carryable_and_unshared_ceiling_solid() -> Result<(), String>
{
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"stacked_item","name":"Stacked item",
        "spawn":{"x":1,"z":3.5},"rooms":[
          {"x":0,"z":0,"width":4,"depth":4,"floor_y":0,"height":3.2},
          {"x":0,"z":0,"width":2,"depth":4,"floor_y":3.2,"height":3.2,
           "ceiling":{"kind":"open"}}
        ]}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut world = ItemWorld::from_level(&level, &[]);
    let player = PlayerContext {
        half: crate::engine::collision::PLAYER_RADIUS,
        eye: Vec3::new(1.0, 4.8, 3.5),
        feet: Vec3::new(1.0, 3.2, 3.5),
        height: 1.8,
        view: (Vec3::new(1.0, 3.308, 2.0) - Vec3::new(1.0, 4.8, 3.5)).normalize(),
        yaw: 0.0,
        active: true,
    };
    world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.108 }),
            ..PhysicsDef::default()
        },
        (
            Vec3::new(-0.108, 0.0, -0.108),
            Vec3::new(0.108, 0.216, 0.108),
        ),
        Vec3::new(1.0, 3.2, 2.0),
        0.0,
        1.0,
    )?;
    assert!(
        world
            .body_penetration_depth(1)
            .is_some_and(|depth| depth <= 0.002),
        "exact shared-plane contact is clear for dormant items"
    );
    world.step(FIXED_STEP, player, &[]);
    assert!(world.pickup(1, player.eye, player.view, player.yaw));
    let carrying = PlayerContext {
        view: Vec3::NEG_Z,
        ..player
    };
    for _ in 0_u8..60 {
        world.step(FIXED_STEP, carrying, &[]);
    }
    let sphere = world
        .snapshots()
        .into_iter()
        .next()
        .ok_or("missing sphere")?;
    assert!(
        sphere.pose.position.y > 3.6,
        "the held sphere must lift out of the shared floor/ceiling plane"
    );
    assert!(
        !world.held_movement_blocked(),
        "ordinary upper-floor carrying must not report a sustained blockage"
    );
    assert!(world.throw(Vec3::NEG_Z), "a clear held sphere must release");
    assert!(world.held().is_none());
    let ceiling_hit = world
        .camera_sweep(Vec3::new(3.0, 2.5, 2.0), Vec3::new(3.0, 3.8, 2.0), 0.05)
        .ok_or("invalid ceiling sweep")?;
    assert!(ceiling_hit.y < 3.2, "the uncovered half stays solid below");
    let floor_hit = world
        .camera_sweep(Vec3::new(1.0, 4.0, 1.0), Vec3::new(1.0, 2.5, 1.0), 0.05)
        .ok_or("invalid upper-floor sweep")?;
    assert!(floor_hit.y > 3.2, "the upper floor stays solid from above");
    Ok(())
}

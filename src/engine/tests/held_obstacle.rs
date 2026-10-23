//! One held sphere protects the paused-carry obstacle-pose contract consumed
//! by moving doors and AI. Existing carry tests check ownership and
//! release, not the obstacle pose. No assets, GPU or map bake; a few CPU steps.

use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, PlayerContext, ShapeDef};
use glam::Vec3;

#[test]
fn held_obstacle_tracks_actual_body_while_solver_is_paused() -> Result<(), String> {
    let mut world = ItemWorld::default();
    world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.1 }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        Vec3::new(0.0, 1.4, -1.0),
        0.0,
        1.0,
    )?;
    let player = PlayerContext {
        half: crate::engine::collision::PLAYER_RADIUS,
        eye: Vec3::new(0.0, 1.5, 0.0),
        feet: Vec3::ZERO,
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: true,
    };
    world.sync_context(player, &[]);
    assert!(
        world.pickup(1, player.eye, player.view, 0.0),
        "sphere is reachable"
    );
    world.set_time_scale(0.0)?;
    for _ in 0_u8..10 {
        world.step(1.0 / 60.0, player, &[]);
    }
    let state = world
        .snapshots()
        .into_iter()
        .next()
        .ok_or("missing held sphere")?;
    let obstacle = world
        .navigation_obstacles()
        .next()
        .ok_or("missing held obstacle")?;
    assert!(state.held, "query must not change custody");
    assert!(
        (f32::midpoint(obstacle.min_z, obstacle.max_z) - state.world_center_of_mass.z).abs()
            < 1.0e-5,
        "door/AI obstacle bounds follow the held body, without waiting for collider propagation"
    );
    Ok(())
}

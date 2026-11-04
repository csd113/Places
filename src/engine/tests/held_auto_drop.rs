//! One sphere and one slab protect the new size/distance/grace release boundary,
//! safe retained pose, fresh checkpoint ownership and ordinary following. These
//! are absent from manual-drop coverage; bounded CPU steps, no assets/bake/GPU.
use crate::engine::collision::DoorCollider;
use crate::engine::physics::{
    BodyMotion, BodySnapshot, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};
use glam::Vec3;

fn player(eye: Vec3) -> PlayerContext {
    PlayerContext {
        half: 0.4,
        feet: Vec3::new(eye.x, eye.y - 1.8, eye.z),
        eye,
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: true,
    }
}

fn held_world(radius: f32) -> Result<ItemWorld, String> {
    let mut world = ItemWorld::default();
    world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius }),
            ..PhysicsDef::default()
        },
        (Vec3::splat(-radius), Vec3::splat(radius)),
        Vec3::new(0.0, 2.0, -1.2 - radius),
        0.0,
        1.0,
    )?;
    let context = player(Vec3::new(0.0, 2.0, 0.0));
    world.sync_context(context, &[]);
    assert!(world.pickup(1, context.eye, context.view, context.yaw));
    world.pause();
    Ok(world)
}

fn body(world: &ItemWorld) -> Result<BodySnapshot, String> {
    world
        .snapshots()
        .into_iter()
        .next()
        .ok_or_else(|| "Missing sphere".into())
}

fn barrier() -> DoorCollider {
    DoorCollider::from_pose([-1.0, 0.0, -1.05], [1.0, 0.0], 2.0, 0.1, 4.0)
}

fn steps(world: &mut ItemWorld, context: PlayerContext, doors: &[DoorCollider], count: u8) {
    for _ in 0_u8..count {
        world.step(0.05, context, doors);
    }
}

#[test]
fn held_auto_drop_uses_size_aware_boundary_and_preserves_pose_without_impulse() -> Result<(), String>
{
    for radius in [0.1, 0.5] {
        let mut world = held_world(radius)?;
        let origin = player(Vec3::new(0.0, 2.0, 0.0));
        steps(&mut world, origin, &[barrier()], 20);
        assert!(world.held_movement_blocked());
        assert_eq!(world.held(), Some(1), "nearby obstructions retain custody");
        let limit = world.held_auto_drop_distance().ok_or("No limit")?;
        assert!((limit - (2.7 + radius)).abs() < 0.0001);
        let retained = body(&world)?.pose;
        let below = player(retained.position + Vec3::Z * (limit - 0.01));
        steps(&mut world, below, &[barrier()], 4);
        assert_eq!(world.held(), Some(1), "inside the limit must stay held");
        let above = player(retained.position + Vec3::Z * (limit + 0.01));
        world.step(0.05, above, &[barrier()]);
        let dropped = body(&world)?;
        assert_eq!(world.held(), None);
        assert_eq!(world.stats().auto_dropped_held, Some(1));
        assert_eq!(dropped.pose, retained, "release must not teleport the body");
        assert_eq!(dropped.render_pose, retained);
        assert_eq!(dropped.linear_velocity, Vec3::ZERO);
        assert_eq!(dropped.angular_velocity, Vec3::ZERO);
        assert!(!world.pickup(1, above.eye, above.view, above.yaw));
        world.step(0.05, above, &[barrier()]);
        assert_eq!(world.stats().auto_dropped_held, None);
        assert_eq!(world.snapshots().len(), 1, "release preserves one body");
    }
    Ok(())
}

#[test]
fn held_auto_drop_revalidates_grace_after_resume_and_saves_released_ownership() -> Result<(), String>
{
    let selected = [(1, "crate".into(), "fixture".into())];
    let mut original = held_world(0.1)?;
    original.resume();
    let saved = original.checkpoint(&selected)?;
    let encoded = serde_json::to_vec(&saved).map_err(|error| error.to_string())?;
    let records: Vec<(String, crate::engine::physics::checkpoint::Item)> =
        serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
    let mut restored = held_world(0.1)?;
    restored.restore_checkpoint(&selected, &records)?;
    let far = player(Vec3::new(0.0, 2.0, 2.0));
    steps(&mut restored, far, &[barrier()], 4);
    assert_eq!(
        restored.held(),
        Some(1),
        "fresh obstruction needs its grace"
    );
    steps(&mut restored, far, &[barrier()], 12);
    assert_eq!(restored.held(), None);
    restored.resume();
    let released = restored.checkpoint(&selected)?;
    let mut again = held_world(0.1)?;
    again.restore_checkpoint(&selected, &released)?;
    assert_eq!(again.held(), None, "resume must not reclaim a dropped item");
    assert_eq!(body(&again)?.pose, body(&restored)?.pose);
    Ok(())
}

#[test]
fn held_auto_drop_leaves_ordinary_following_and_manual_release_intact() -> Result<(), String> {
    let mut world = held_world(0.1)?;
    let mut context = player(Vec3::new(0.0, 2.0, 0.0));
    for _ in 0_u8..100 {
        context.eye.x += 0.1;
        context.feet.x += 0.1;
        world.step(0.05, context, &[]);
        assert_eq!(world.held(), Some(1));
        assert_eq!(world.stats().auto_dropped_held, None);
    }
    assert!(body(&world)?.pose.position.distance(context.eye) < 1.0);
    assert!(world.drop());
    assert_eq!(world.held(), None);
    Ok(())
}

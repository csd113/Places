//! A native cargo run exposed dormancy restoring the old pose one tick after
//! transport. This small solver fixture protects that boundary, unchanged
//! neighbours and held/velocity rotation without any assets, renderer or bake.
use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, PlayerContext};
use glam::{Quat, Vec3};

fn body(world: &ItemWorld, key: u64) -> Result<crate::engine::physics::BodySnapshot, String> {
    world
        .snapshots()
        .into_iter()
        .find(|b| b.key == key)
        .ok_or_else(|| "Missing body".into())
}

fn spawn(world: &mut ItemWorld, key: u64, position: Vec3) -> Result<(), String> {
    world.spawn_authored(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        position,
        0.0,
        1.0,
    )
}

#[test]
fn transported_dormant_body_keeps_its_new_rest_anchor() -> Result<(), String> {
    let mut world = ItemWorld::default();
    spawn(&mut world, 1, Vec3::new(0.0, 0.0, 1.2))?;
    spawn(&mut world, 2, Vec3::new(-3.0, 0.0, 0.0))?;
    let outside = body(&world, 2)?.pose;
    let selected = vec![(1_u64, "crate".into(), "fixture".into())];
    let exported = world.transport_checkpoint(&selected)?;
    let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let records: Vec<_> = exported
        .iter()
        .map(|(id, state)| {
            (
                id.clone(),
                state.transported(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), turn),
            )
        })
        .collect();
    world.restore_transport(&selected, &records)?;
    let player = PlayerContext {
        half: 0.4,
        eye: Vec3::new(-5.0, 1.6, 0.0),
        feet: Vec3::new(-5.0, 0.0, 0.0),
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: false,
    };
    for _ in 0_u8..16 {
        world.step(1.0 / 60.0, player, &[]);
    }
    let moved = body(&world, 1)?;
    assert!((moved.pose.position.x - 5.2).abs() < 0.0001);
    assert!(moved.pose.position.z.abs() < 0.0001);
    assert!(moved.dormant);
    assert_eq!(moved.pose, moved.render_pose);
    assert_eq!(outside, body(&world, 2)?.pose);
    let saved = world.checkpoint(&[
        (1, "crate".into(), "fixture".into()),
        (2, "outside".into(), "fixture".into()),
    ])?;
    let mut restored = ItemWorld::default();
    spawn(&mut restored, 91, Vec3::new(0.0, 0.0, 1.2))?;
    spawn(&mut restored, 92, Vec3::new(-3.0, 0.0, 0.0))?;
    restored.restore_checkpoint(
        &[
            (91, "crate".into(), "fixture".into()),
            (92, "outside".into(), "fixture".into()),
        ],
        &saved,
    )?;
    restored.step(1.0 / 60.0, player, &[]);
    assert_eq!(body(&restored, 91)?.pose, moved.pose);
    Ok(())
}

#[test]
fn cargo_rotates_velocity_and_refuses_a_blocked_batch() -> Result<(), String> {
    let mut world = ItemWorld::default();
    spawn(&mut world, 1, Vec3::new(0.0, 1.0, 0.0))?;
    spawn(&mut world, 2, Vec3::new(3.0, 1.0, 0.0))?;
    assert!(world.set_fixture_velocity(1, Vec3::X, Vec3::Z));
    let selected = vec![(1_u64, "crate".into(), "fixture".into())];
    let before = body(&world, 1)?;
    let exported = world.transport_checkpoint(&selected)?;
    let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let blocked: Vec<_> = exported
        .iter()
        .map(|(id, state)| {
            (
                id.clone(),
                state.transported(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0), turn),
            )
        })
        .collect();
    assert!(world.restore_transport(&selected, &blocked).is_err());
    assert_eq!(body(&world, 1)?.pose, before.pose);
    let clear: Vec<_> = exported
        .iter()
        .map(|(id, state)| {
            (
                id.clone(),
                state.transported(Vec3::ZERO, Vec3::new(5.0, 0.0, 0.0), turn),
            )
        })
        .collect();
    // Resident NPCs have no rigid proxy: candidate cargo must still reject
    // their occupied space without moving either side of the batch.
    assert!(!world.transport_actor_clear(&selected, &clear, Vec3::new(5.0, 0.0, 0.0), 1.8, 0.4)?);
    assert!(world.transport_actor_clear(&selected, &clear, Vec3::new(7.0, 0.0, 0.0), 1.8, 0.4)?);
    assert_eq!(body(&world, 1)?.pose, before.pose);
    world.restore_transport(&selected, &clear)?;
    let after = body(&world, 1)?;
    assert!((after.linear_velocity.z + 1.0).abs() < 0.0001);
    assert!((after.angular_velocity.x - 1.0).abs() < 0.0001);
    Ok(())
}

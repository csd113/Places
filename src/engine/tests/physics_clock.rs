//! Tiny CPU-only regressions for exact stepping and independent carry control.

use glam::Vec3;

use crate::engine::physics::{
    BodyMotion, BodySnapshot, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext,
};

fn player() -> PlayerContext {
    PlayerContext {
        half: crate::engine::collision::PLAYER_RADIUS,
        eye: Vec3::new(0.0, 2.0, 0.0),
        feet: Vec3::new(0.0, 0.2, 0.0),
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: true,
    }
}

fn dynamic_profile() -> PhysicsDef {
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        linear_damping: 0.0,
        angular_damping: 0.0,
        ..PhysicsDef::default()
    }
}

fn spawn(world: &mut ItemWorld, key: u64, position: Vec3) -> Result<(), String> {
    world.spawn(
        key,
        &dynamic_profile(),
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        position,
        0.0,
        1.0,
    )
}

fn body(world: &ItemWorld, key: u64) -> Result<BodySnapshot, String> {
    world
        .snapshots()
        .into_iter()
        .find(|body| body.key == key)
        .ok_or_else(|| format!("body {key} disappeared"))
}

#[test]
fn exact_step_ignores_scale_discards_fraction_and_stays_paused() -> Result<(), String> {
    let mut world = ItemWorld::default();
    let context = player();
    spawn(&mut world, 1, Vec3::new(2.0, 3.0, -2.0))?;
    let initial = body(&world, 1)?.pose;
    world.step(FIXED_STEP * 0.75, context, &[]);
    assert_eq!(
        world.stats().substeps,
        0,
        "partial time must wait for a fixed interval"
    );
    world.step_once(context, &[])?;
    assert!(
        world.simulation_state().paused,
        "single step must finish paused"
    );
    assert_eq!(
        world.simulation_state().fixed_steps,
        1,
        "single step must advance exactly once"
    );
    assert_eq!(
        world.stats().substeps,
        1,
        "explicit stepping must run one solver interval"
    );
    let advanced = body(&world, 1)?;
    assert!(
        advanced.pose.position.y < initial.position.y,
        "the explicit interval must apply gravity immediately"
    );
    assert!(
        advanced
            .render_pose
            .position
            .distance(advanced.pose.position)
            < 1.0e-6,
        "single-step rendering must publish the solved pose"
    );
    world.step(0.1, context, &[]);
    assert!(
        body(&world, 1)?
            .pose
            .position
            .distance(advanced.pose.position)
            < 1.0e-6,
        "paused frames must preserve the solved pose"
    );
    world.set_time_scale(1.0)?;
    world.resume();
    world.step(FIXED_STEP * 0.25, context, &[]);
    assert_eq!(
        world.stats().substeps,
        0,
        "explicit stepping must discard the old fractional remainder"
    );
    world.set_time_scale(0.0)?;
    world.step_once(context, &[])?;
    assert_eq!(
        world.simulation_state().fixed_steps,
        2,
        "explicit stepping must ignore zero time scale"
    );
    assert!(
        world.simulation_state().paused,
        "zero-scale step must also finish paused"
    );
    let before_rejection = world.simulation_state();
    for scale in [f32::NAN, f32::INFINITY, -0.1, 4.1] {
        assert!(
            world.set_time_scale(scale).is_err(),
            "invalid scales must be rejected"
        );
        assert_eq!(
            world.simulation_state(),
            before_rejection,
            "scale rejection must not mutate the clock"
        );
    }
    world.pause();
    world.set_time_scale(4.0)?;
    world.resume();
    world.step(FIXED_STEP, context, &[]);
    assert_eq!(
        world.stats().substeps,
        4,
        "fourfold time must advance four fixed intervals"
    );
    Ok(())
}

#[test]
fn frozen_carry_sweeps_dynamic_obstacles_and_releases_with_real_velocity() -> Result<(), String> {
    for paused in [false, true] {
        let mut world = ItemWorld::default();
        let mut context = player();
        spawn(&mut world, 1, Vec3::new(0.0, 2.0, -1.3))?;
        world.sync_context(context, &[]);
        assert!(
            world.pickup(1, context.eye, context.view, context.yaw),
            "fixture must be pickable before freezing"
        );
        if paused {
            world.pause();
        } else {
            world.set_time_scale(0.0)?;
        }
        for _ in 0_u8..20_u8 {
            world.step(0.01, context, &[]);
        }
        let carried = body(&world, 1)?;
        assert!(
            carried.pose.position.z > -1.0,
            "unscaled carry must reach the hand while world time is stopped"
        );
        spawn(&mut world, 2, Vec3::new(0.45, 2.0, carried.pose.position.z))?;
        let obstacle = body(&world, 2)?.pose;
        for _ in 0_u8..80_u8 {
            context.eye.x += 0.01;
            context.feet.x += 0.01;
            world.step(0.01, context, &[]);
        }
        assert!(
            body(&world, 1)?.pose.position.x < 0.26,
            "held shape must stop before the frozen dynamic obstacle"
        );
        assert!(
            body(&world, 2)?.pose.position.distance(obstacle.position) < 1.0e-6,
            "frozen obstacles must not receive carry impulses or gravity"
        );
        assert!(
            world.remove_movable(2),
            "the fixture obstacle must be removable"
        );
        assert!(
            world.release_held(),
            "clear checked carry pose must release"
        );
        let released = body(&world, 1)?;
        assert!(
            (released.linear_velocity.x - 1.0).abs() < 1.0e-4,
            "release must inherit real player velocity regardless of world scale"
        );
        assert!(
            released.angular_velocity.length() < 1.0e-5,
            "frozen carrying must not leak stale kinematic angular velocity"
        );
        for _ in 0_u8..10_u8 {
            world.step(0.01, context, &[]);
        }
        assert!(
            body(&world, 1)?
                .pose
                .position
                .distance(released.pose.position)
                < 1.0e-6,
            "released bodies must stay frozen until world time resumes"
        );
        world.set_time_scale(1.0)?;
        world.resume();
        world.step(FIXED_STEP, context, &[]);
        let resumed = body(&world, 1)?;
        assert!(
            resumed.pose.position.x > released.pose.position.x,
            "resume must preserve the clean release velocity"
        );
        assert!(
            resumed.pose.position.y < released.pose.position.y,
            "resume must restore ordinary gravity"
        );
    }
    Ok(())
}

/// Protects empty-handed walking, falling and jumping through stopped dynamic
/// bodies. One cube and six casts cost no solver steps, maps or GPU resources.
#[test]
fn stopped_world_blocks_player_capsule_in_all_axes_without_a_held_item() -> Result<(), String> {
    for paused in [false, true] {
        let mut world = ItemWorld::default();
        let initial = player();
        spawn(&mut world, 1, Vec3::new(1.0, 1.1, 0.0))?;
        world.sync_context(initial, &[]);
        if paused {
            world.pause();
        } else {
            world.set_time_scale(0.0)?;
        }
        let crossing = PlayerContext {
            eye: Vec3::new(2.0, initial.eye.y, 0.0),
            feet: Vec3::new(2.0, initial.feet.y, 0.0),
            ..initial
        };
        let constrained = world.constrain_player_eye(crossing);
        assert!(
            (constrained.x - (0.9 - crate::engine::collision::PLAYER_RADIUS)).abs() < 0.01,
            "an empty hand must not let the player cross a frozen dynamic body"
        );
        let above = PlayerContext {
            eye: Vec3::new(1.0, 3.8, 0.0),
            feet: Vec3::new(1.0, 2.0, 0.0),
            ..initial
        };
        world.sync_context(above, &[]);
        let falling = PlayerContext {
            eye: Vec3::new(1.0, 2.0, 0.0),
            feet: Vec3::new(1.0, 0.2, 0.0),
            ..initial
        };
        assert!(
            world.constrain_player_eye(falling).y > 2.99,
            "a downward capsule sweep must stop on the frozen body's top"
        );
        let below = PlayerContext {
            eye: Vec3::new(1.0, 0.8, 0.0),
            feet: Vec3::new(1.0, -1.0, 0.0),
            ..initial
        };
        world.sync_context(below, &[]);
        assert!(
            world.constrain_player_eye(above).y < 1.01,
            "an upward capsule sweep must stop below the frozen body"
        );
        assert_eq!(
            world.simulation_state().fixed_steps,
            0,
            "collision queries must leave frozen world time unchanged"
        );
    }
    let mut live = ItemWorld::default();
    let initial = player();
    live.spawn_authored(
        1,
        &dynamic_profile(),
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        Vec3::new(1.0, 1.1, 0.0),
        0.0,
        1.0,
    )?;
    live.sync_context(initial, &[]);
    let crossing = PlayerContext {
        eye: Vec3::new(2.0, initial.eye.y, 0.0),
        feet: Vec3::new(2.0, initial.feet.y, 0.0),
        ..initial
    };
    assert!(
        live.constrain_player_eye(crossing).x < 0.61,
        "a dormant authored body must block an empty-handed player at live scale one"
    );
    Ok(())
}

#[test]
fn reset_detaches_at_the_retained_pose_even_when_new_player_context_overlaps() -> Result<(), String>
{
    let mut world = ItemWorld::default();
    let initial = player();
    spawn(&mut world, 1, Vec3::new(0.0, 2.0, -1.3))?;
    world.sync_context(initial, &[]);
    assert!(
        world.pickup(1, initial.eye, initial.view, initial.yaw),
        "fixture must be held before reset"
    );
    world.set_time_scale(0.0)?;
    world.step(0.05, initial, &[]);
    let retained = body(&world, 1)?.pose;
    let reset_context = PlayerContext {
        feet: Vec3::new(retained.position.x, 1.1, retained.position.z),
        eye: Vec3::new(retained.position.x, 2.9, retained.position.z),
        ..initial
    };
    world.sync_context(reset_context, &[]);
    assert!(
        !world.drop(),
        "ordinary release must reject the new overlapping player context"
    );
    let clock = world.simulation_state();
    assert!(
        world.release_held_for_reset(),
        "an authoritative reset must detach the already checked carry pose"
    );
    let released = body(&world, 1)?;
    assert!(!released.held, "reset detachment must clear ownership");
    assert!(
        released.pose.position.distance(retained.position) < 1.0e-6,
        "reset must preserve the original item pose"
    );
    assert!(
        released.linear_velocity.length() < 1.0e-6 && released.angular_velocity.length() < 1.0e-6,
        "reset detachment must clear inherited motion"
    );
    assert_eq!(
        world.simulation_state(),
        clock,
        "reset detachment must preserve the world clock"
    );
    world.step(0.05, reset_context, &[]);
    assert!(
        body(&world, 1)?.pose.position.distance(retained.position) < 1.0e-6,
        "a reset item must remain frozen at zero scale"
    );
    Ok(())
}

/// A become/authored body must not become dynamic around its player. One cube
/// checks the shared activation gate with no solver, map or GPU work.
#[test]
fn dormant_activation_waits_until_player_has_left_its_solid_volume() -> Result<(), String> {
    let mut world = ItemWorld::default();
    let initial = player();
    let profile = PhysicsDef {
        reset_group: Some("player-overlap".into()),
        ..dynamic_profile()
    };
    let position = Vec3::new(0.0, 1.1, 0.0);
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    world.spawn_authored(1, &profile, bounds, position, 0.0, 1.0)?;
    world.sync_context(initial, &[]);
    let retained = body(&world, 1)?.pose;
    assert!(
        !world.activate(1),
        "direct activation must refuse a body intersecting its enabled player"
    );
    assert_eq!(
        world.activate_group("player-overlap")?.activated_count,
        0,
        "group activation must use the same player-overlap guard"
    );
    assert!(
        !world.pickup(1, initial.eye, Vec3::NEG_Y, initial.yaw),
        "pickup must not bypass dormant activation safety"
    );
    let rejected = body(&world, 1)?;
    assert!(
        rejected.dormant && !rejected.held,
        "rejected activation must retain dormant ownership"
    );
    assert!(
        rejected.pose.position.distance(retained.position) < 1.0e-6,
        "rejected activation must retain the authored pose"
    );
    let touching = PlayerContext {
        eye: Vec3::new(
            crate::engine::collision::PLAYER_RADIUS + 0.1,
            initial.eye.y,
            0.0,
        ),
        feet: Vec3::new(
            crate::engine::collision::PLAYER_RADIUS + 0.1,
            initial.feet.y,
            0.0,
        ),
        ..initial
    };
    world.sync_context(touching, &[]);
    assert!(
        world.activate(1),
        "mere player contact must permit ordinary activation"
    );
    assert!(
        world.remove_movable(1),
        "the activated fixture body must be removable"
    );
    world.spawn_authored(2, &profile, bounds, position, 0.0, 1.0)?;
    world.sync_context(
        PlayerContext {
            active: false,
            ..initial
        },
        &[],
    );
    assert!(
        world.activate(2),
        "a disabled noclip player collider must not block activation"
    );
    Ok(())
}

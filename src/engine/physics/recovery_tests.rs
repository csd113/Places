//! Small escape fixtures; no renderer, assets, map compiler or production bake.

use glam::{Quat, Vec3};
use rapier3d::prelude::Vector;

use super::{EscapeReason, RecoveryOutcome, RecoverySpace, finite_pose};
use crate::engine::collision::WallAabb;
use crate::engine::level::LevelDef;
use crate::engine::physics::{BodyMotion, ItemPose, ItemWorld, PhysicsDef, PlayerContext};

fn level() -> Result<LevelDef, String> {
    serde_json::from_str(
        r#"{"format_version":3,"id":"prop_recovery","name":"prop recovery",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-1,"z":-1,"width":2,"depth":2,"height":3}],
        "defaults":{"floor":"core:carpet","ceiling":"core:ceiling_tile","wall":"core:wallpaper"}}"#,
    )
    .map_err(|error| error.to_string())
}

fn player() -> PlayerContext {
    PlayerContext {
        eye: Vec3::new(8.0, 1.6, 8.0),
        feet: Vec3::new(8.0, 0.0, 8.0),
        height: 1.8,
        view: -Vec3::Z,
        yaw: 0.0,
        active: false,
    }
}

fn definition() -> PhysicsDef {
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        ..PhysicsDef::default()
    }
}

fn spawn(world: &mut ItemWorld, key: u64, authored: bool, position: Vec3) -> Result<(), String> {
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    if authored {
        world.spawn_authored(key, &definition(), bounds, position, 0.0, 1.0)
    } else {
        world.spawn(key, &definition(), bounds, position, 0.0, 1.0)
    }
}

fn recover(world: &mut ItemWorld, interval: f32) -> Result<usize, String> {
    let runtime = world.runtime.as_mut().ok_or("missing recovery runtime")?;
    Ok(runtime.recover_escaped(&world.recovery_space, interval))
}

// Protects geometry-derived detection from accepting gaps through a global
// bounding box or restoring a legitimate lower storey/recess/open-sky throw.
// Five metadata-only areas and twelve membership queries; no solver steps.
#[test]
fn recovery_coverage_preserves_local_gaps_storeys_recesses_and_open_sky() -> Result<(), String> {
    let fixture: LevelDef = serde_json::from_str(
        r#"{"format_version":3,"id":"recovery_coverage","name":"coverage",
        "spawn":{"x":0,"z":0},"rooms":[
            {"x":0,"z":0,"width":2,"depth":2,"height":3},
            {"x":0,"z":0,"width":2,"depth":2,"floor_y":10,"height":3},
            {"x":4,"z":0,"width":2,"depth":2,"height":3,"ceiling":{"kind":"open"}},
            {"x":12,"z":0,"width":2,"depth":2,"floor_y":4,"height":3,"ceiling":{"kind":"gable","ridge":"x","ridge_rise":2}}],
        "floor_regions":[{"x":0,"z":0,"width":2,"depth":2,"offset_y":-4}],
        "water":[{"shape":"circle","x":8,"z":0,"radius":1,"surface_y":0,"bottom_y":-5}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let space = RecoverySpace::from_level(&fixture, &[]);
    for (center, inside) in [
        (Vec3::new(1.0, -4.0, 1.0), true),
        (Vec3::new(1.0, 10.1, 1.0), true),
        (Vec3::new(1.0, 15.2, 1.0), false),
        (Vec3::new(1.0, -6.2, 1.0), false),
        (Vec3::new(3.0, 0.1, 1.0), false),
        (Vec3::new(4.5, 100.0, 1.0), true),
        (Vec3::new(13.0, 8.9, 1.0), true),
        (Vec3::new(13.0, 11.2, 1.0), false),
        (Vec3::new(9.0, -5.0, 1.0), true),
        (Vec3::new(8.05, -5.0, 0.05), false),
    ] {
        assert_eq!(
            space.membership(center, 0.1, None).is_some(),
            inside,
            "physical center {center:?}"
        );
    }
    let member = space.membership(Vec3::new(1.0, 0.1, 1.0), 0.1, None);
    assert!(
        space
            .membership(Vec3::new(1.2, 0.1, 1.0), 0.1, member)
            .is_some(),
        "cached membership follows a body within its area"
    );
    assert_eq!(
        space.reason(Vec3::new(1.0, 15.2, 1.0), 0.1),
        EscapeReason::AbovePlayableSpace,
        "closed-space upward escapes use the highest overlapping authored ceiling"
    );
    Ok(())
}

// Protects dormant/fixed placements from load-time correction, then checks the
// actual escape record, full-shape recovery, cleared motion and held ownership.
// Four tiny cuboids; one second of manual grace and three invalid-motion checks.
#[test]
fn recovery_retains_dormancy_and_clears_escaped_or_invalid_active_motion() -> Result<(), String> {
    let fixture = level()?;
    let mut world = ItemWorld::from_level(&fixture, &[]);
    world.sync_context(player(), &[]);
    let initial = Vec3::new(0.0, 0.112, 0.0);
    spawn(&mut world, 1, true, initial)?;
    spawn(&mut world, 2, true, Vec3::new(0.7, -5.0, 0.0))?;
    world.spawn(
        3,
        &PhysicsDef::default(),
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        Vec3::new(-0.7, -5.0, 0.0),
        0.0,
        1.0,
    )?;
    assert_retained_placements(&mut world)?;
    assert_finite_escape(&mut world, initial)?;
    assert_invalid_active_motion(&mut world)?;
    assert_invalid_held_motion(&fixture)
}

fn assert_retained_placements(world: &mut ItemWorld) -> Result<(), String> {
    assert_eq!(
        recover(world, 2.0)?,
        0,
        "dormant and fixed bodies never auto-correct their authored pose"
    );
    let retained = world.snapshots();
    assert!(
        retained
            .iter()
            .any(|body| body.key == 2 && body.dormant && body.pose.position.y < -4.9),
        "imperfect dormant placement remains exact"
    );
    assert!(
        retained.iter().any(|body| body.key == 3
            && body.motion == BodyMotion::Fixed
            && body.pose.position.y < -4.9),
        "fixed scenery remains fixed"
    );
    Ok(())
}

fn assert_finite_escape(world: &mut ItemWorld, initial: Vec3) -> Result<(), String> {
    let escaped = ItemPose {
        position: Vec3::new(0.0, -5.0, 0.0),
        rotation: Quat::IDENTITY,
    };
    assert!(
        world.set_fixture_pose(1, escaped),
        "an explicit developer pose activates the test body"
    );
    assert!(
        world.set_fixture_velocity(1, Vec3::new(0.0, -8.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        "finite test velocity"
    );
    assert_eq!(
        recover(world, 0.5)?,
        0,
        "brief unsupported excursion receives grace"
    );
    assert_eq!(
        recover(world, 0.5)?,
        1,
        "the sustained escape returns to a supported clear initial pose"
    );
    let event = world
        .take_recoveries()
        .into_iter()
        .next()
        .ok_or("missing escape evidence")?;
    assert_eq!(
        event.reason,
        EscapeReason::BelowPlayableSpace,
        "records geometry cause"
    );
    assert_eq!(
        event.escaped_pose,
        Some(escaped),
        "recovery preserves the original escaped transform as evidence"
    );
    assert_eq!(
        event.outcome,
        RecoveryOutcome::Restored,
        "validated destination succeeded"
    );
    let restored = world
        .snapshots()
        .into_iter()
        .find(|body| body.key == 1)
        .ok_or("missing recovered body")?;
    assert!(
        restored.dormant && restored.pose.position.distance(initial) < 0.0001,
        "authored recovery at the exact initial pose restores dormancy"
    );
    assert!(
        restored.linear_velocity.length_squared() < f32::EPSILON
            && restored.angular_velocity.length_squared() < f32::EPSILON,
        "recovery clears linear and angular motion"
    );

    Ok(())
}

fn assert_invalid_active_motion(world: &mut ItemWorld) -> Result<(), String> {
    assert!(world.activate(1), "explicit activation remains available");
    let before_runtime = world.runtime.as_mut().ok_or("missing runtime")?;
    let body_handle = before_runtime.entries.get(&1).ok_or("missing entry")?.body;
    let corrupted_body = before_runtime
        .world
        .bodies
        .get_mut(body_handle)
        .ok_or("missing rigid body")?;
    corrupted_body.set_linvel(Vector::new(f32::NAN, 0.0, 0.0), true);
    corrupted_body.add_force(Vector::new(2.0, 3.0, 4.0), true);
    corrupted_body.add_torque(Vector::new(1.0, 2.0, 3.0), true);
    assert_eq!(
        recover(world, 0.0)?,
        1,
        "invalid motion is cleared before the solver without waiting for grace"
    );
    let invalid = world
        .take_recoveries()
        .into_iter()
        .next()
        .ok_or("missing invalid-motion evidence")?;
    assert_eq!(
        invalid.reason,
        EscapeReason::NonFiniteMotion,
        "reports invalid motion separately from tunnelling"
    );
    assert!(
        invalid.escaped_linear_velocity.is_none(),
        "nonfinite evidence remains valid JSON"
    );
    let cleared_runtime = world.runtime.as_ref().ok_or("missing recovered runtime")?;
    let cleared_body = cleared_runtime
        .world
        .bodies
        .get(body_handle)
        .ok_or("missing cleared body")?;
    assert!(
        cleared_body.user_force().length_squared() < f32::EPSILON
            && cleared_body.user_torque().length_squared() < f32::EPSILON,
        "recovery clears accumulated forces and torques"
    );

    Ok(())
}

fn assert_invalid_held_motion(fixture: &LevelDef) -> Result<(), String> {
    let mut held_world = ItemWorld::from_level(fixture, &[]);
    let eye = Vec3::new(0.0, 1.6, 0.8);
    let context = PlayerContext {
        eye,
        feet: Vec3::new(0.0, 0.0, 0.8),
        active: true,
        ..player()
    };
    held_world.sync_context(context, &[]);
    spawn(&mut held_world, 4, false, Vec3::new(0.0, 1.6, 0.0))?;
    assert!(
        held_world.pickup(4, eye, -Vec3::Z, 0.0),
        "normal bounded pickup"
    );
    let runtime = held_world
        .runtime
        .as_mut()
        .ok_or("missing carried runtime")?;
    let held_handle = runtime.entries.get(&4).ok_or("missing held entry")?.body;
    runtime
        .world
        .bodies
        .get_mut(held_handle)
        .ok_or("missing held body")?
        .set_translation(Vector::new(f32::NAN, 1.6, 0.0), true);
    assert!(
        runtime
            .world
            .bodies
            .get(held_handle)
            .ok_or("missing corrupted held body")?
            .translation()
            .x
            .is_nan(),
        "the fixture must actually corrupt the held pose: position-based kinematics ignore set_linvel"
    );
    assert_eq!(
        recover(&mut held_world, 0.0)?,
        1,
        "held invalid motion uses a clear destination"
    );
    let held_event = held_world
        .take_recoveries()
        .into_iter()
        .next()
        .ok_or("missing held recovery")?;
    assert!(
        held_event.released_held && held_world.held().is_none(),
        "held ownership clears atomically with recovery"
    );
    assert!(
        held_event.escaped_pose.is_none(),
        "the original invalid pose is retained honestly before sanitization"
    );
    assert_invalid_next_pose(&mut held_world, eye, held_handle)
}

fn assert_invalid_next_pose(
    held_world: &mut ItemWorld,
    eye: Vec3,
    held_handle: rapier3d::prelude::RigidBodyHandle,
) -> Result<(), String> {
    assert!(
        held_world.pickup(4, eye, -Vec3::Z, 0.0),
        "the recovered given body can be picked up again"
    );
    let runtime = held_world
        .runtime
        .as_mut()
        .ok_or("missing recovered held runtime")?;
    runtime
        .world
        .bodies
        .get_mut(held_handle)
        .ok_or("missing next-pose test body")?
        .set_next_kinematic_position(crate::engine::physics::collision::pose(
            Vec3::new(f32::NAN, 1.6, 0.0),
            Quat::IDENTITY,
        ));
    assert_eq!(
        recover(held_world, 0.0)?,
        1,
        "a malformed prescribed next pose is sanitized before a solver step"
    );
    let next_event = held_world
        .take_recoveries()
        .into_iter()
        .next()
        .ok_or("missing next-pose evidence")?;
    assert!(
        next_event.invalid_next_pose
            && next_event.escaped_pose.is_some()
            && held_world.held().is_none(),
        "diagnostics distinguish an invalid future target from a valid current transform"
    );
    Ok(())
}

// Protects invalid authored recovery anchors and permanently blocked destinations
// from teleport loops. Complete shape clearance chooses a nearby supported pose;
// an entirely obstructed 2 m room instead suspends once, preserving escape evidence.
// Two worlds, one movable cuboid each, and at most thirty destination candidates.
#[test]
fn recovery_uses_safe_fallback_or_suspends_once_without_a_teleport_loop() -> Result<(), String> {
    let fixture = level()?;
    assert_supported_fallback(&fixture)?;
    assert_permanent_suspension(&fixture)
}

fn assert_supported_fallback(fixture: &LevelDef) -> Result<(), String> {
    let obstruction = WallAabb::with_y(-0.3, 0.0, -0.3, 0.6, 2.0, 0.6);
    let mut world = ItemWorld::from_level(fixture, &[obstruction]);
    world.sync_context(player(), &[]);
    spawn(&mut world, 1, true, Vec3::new(0.0, 0.1, 0.0))?;
    assert_eq!(
        recover(&mut world, 2.0)?,
        0,
        "initial imperfect dormancy is retained"
    );
    assert!(
        world.set_fixture_pose(
            1,
            ItemPose {
                position: Vec3::new(0.0, -5.0, 0.0),
                rotation: Quat::IDENTITY
            }
        ),
        "activate and escape"
    );
    assert_eq!(
        recover(&mut world, 1.0)?,
        1,
        "a supported clear alternative is available"
    );
    let recovered = world
        .snapshots()
        .into_iter()
        .next()
        .ok_or("missing alternate recovery")?;
    assert!(
        finite_pose(recovered.pose)
            && !recovered.dormant
            && recovered
                .pose
                .position
                .x
                .abs()
                .max(recovered.pose.position.z.abs())
                > 0.5,
        "fallback uses the physical shape outside the obstructing box, retaining authored reset metadata"
    );
    assert_eq!(
        recover(&mut world, 2.0)?,
        0,
        "the supported fallback does not immediately recover again"
    );
    assert_eq!(
        world.take_recoveries().len(),
        1,
        "one escaped episode emits one event"
    );

    Ok(())
}

fn assert_permanent_suspension(fixture: &LevelDef) -> Result<(), String> {
    let blocked = WallAabb::with_y(-1.0, 0.0, -1.0, 2.0, 3.0, 2.0);
    let mut suspended = ItemWorld::from_level(fixture, &[blocked]);
    suspended.sync_context(player(), &[]);
    spawn(&mut suspended, 2, true, Vec3::new(0.0, 0.1, 0.0))?;
    let escape = ItemPose {
        position: Vec3::new(0.0, -5.0, 0.0),
        rotation: Quat::IDENTITY,
    };
    assert!(
        suspended.set_fixture_pose(2, escape),
        "blocked world escape"
    );
    assert_eq!(
        recover(&mut suspended, 1.0)?,
        0,
        "no fully clear supported destination exists"
    );
    let event = suspended
        .take_recoveries()
        .into_iter()
        .next()
        .ok_or("missing suspension event")?;
    assert_eq!(
        event.outcome,
        RecoveryOutcome::Suspended,
        "blocked recovery is explicit"
    );
    assert_eq!(
        event.escaped_pose,
        Some(escape),
        "suspension cannot hide the escape cause"
    );
    assert!(
        event.destination.is_none() && event.candidate_tests <= 30,
        "destination work is bounded and failed honestly"
    );
    for _ in 0..60_u32 {
        assert_eq!(
            recover(&mut suspended, 1.0)?,
            0,
            "suspended body cannot repeatedly teleport or retry queries"
        );
    }
    assert!(
        suspended.take_recoveries().is_empty(),
        "blocked event is emitted once"
    );
    let runtime = suspended
        .runtime
        .as_ref()
        .ok_or("missing suspended runtime")?;
    let entry = runtime.entries.get(&2).ok_or("missing suspended entry")?;
    assert!(
        entry.recovery.suspended,
        "explicit suspension state is retained"
    );
    assert!(
        !runtime
            .world
            .bodies
            .get(entry.body)
            .ok_or("missing suspended body")?
            .is_enabled(),
        "suspended bodies cannot advance in the solver"
    );
    assert!(
        !runtime
            .world
            .colliders
            .get(entry.collider)
            .ok_or("missing suspended collider")?
            .is_enabled(),
        "suspended collider cannot block the game world"
    );
    Ok(())
}

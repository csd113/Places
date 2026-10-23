//! Tiny CPU regressions protect sound spam limits, real contact speed,
//! full-frame reversals and rotating trigger bounds. Small rigid bodies and one room require
//! no asset loading, GPU, map baking or long simulation campaign.

use glam::{Quat, Vec3};

use super::ImpactBudget;
use crate::engine::collision::WallAabb;
use crate::engine::level::LevelDef;
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, ItemPose, ItemWorld, PhysicsDef, PlayerContext, SurfaceMaterial,
};

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{
            "schema": "places-level", "format_version": 1, "id": "impact_fixture", "name": "Impact fixture",
            "spawn": {"x": 0.0, "z": 0.0},
            "rooms": [{"x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0,
                       "height": 5.0, "material": "home:hardwood_oak_01"}],
            "floor_patches": [{"x": 1.0, "z": -1.0, "width": 2.0, "depth": 2.0,
                               "material": "core:carpet_beige_01"}]
        }"#,
    )
    .map_err(|error| error.to_string())
}

fn player() -> PlayerContext {
    PlayerContext {
        half: crate::engine::collision::PLAYER_RADIUS,
        eye: Vec3::new(4.0, 1.8, 4.0),
        feet: Vec3::new(4.0, 0.0, 4.0),
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: false,
    }
}

fn spawn(world: &mut ItemWorld, key: u64, position: Vec3) -> Result<(), String> {
    world.spawn(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            linear_damping: 0.0,
            angular_damping: 0.0,
            restitution: 0.0,
            ..PhysicsDef::default()
        },
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        position,
        0.0,
        1.0,
    )
}

#[test]
fn impact_budget_rejects_noise_and_limits_simultaneous_sources() {
    let mut budget = ImpactBudget::default();
    for (speed, impulse, cooldown) in [
        (0.1, 10.0, 0.0),
        (3.0, 0.001, 0.0),
        (3.0, 10.0, 0.01),
        (f32::NAN, 10.0, 0.0),
        (3.0, f32::INFINITY, 0.0),
    ] {
        assert!(
            budget.take(speed, impulse, cooldown).is_none(),
            "low speed, negligible force, cooldown and invalid values must stay silent"
        );
    }
    let accepted = (0_u8..20_u8)
        .filter_map(|_| budget.take(3.0, 1.0, 0.0))
        .count();
    assert_eq!(
        accepted, 8,
        "simultaneous sources must share a bounded burst"
    );
    for _ in 0_u8..6_u8 {
        budget.advance();
    }
    let recovered = budget.take(3.0, 1.0, 0.0);
    assert!(
        recovered.is_some_and(|strength| (0.1..=1.0).contains(&strength)),
        "fixed world time must replenish a bounded event strength"
    );
}

#[test]
fn dropped_body_reports_incoming_contact_while_resting_body_stays_silent() -> Result<(), String> {
    let mut world = ItemWorld::from_level(&level()?, &[]);
    spawn(&mut world, 1, Vec3::new(0.0, 1.0, 0.0))?;
    spawn(&mut world, 2, Vec3::new(2.0, 0.102, 0.0))?;
    let mut impacts = Vec::new();
    for _ in 0_u16..240_u16 {
        world.step(FIXED_STEP, player(), &[]);
        world.drain_impacts(|impact| impacts.push(impact));
    }
    assert_eq!(
        impacts.len(),
        1,
        "resting contacts must not produce chatter"
    );
    let impact = impacts.first().ok_or("drop produced no impact")?;
    assert_eq!(impact.key, 1, "only the dropped body must emit");
    assert!(
        impact.speed > 1.0 && impact.impulse > 0.01,
        "incoming speed must survive the solver stopping the dropped body"
    );
    assert_eq!(
        impact.surface,
        SurfaceMaterial::Wood,
        "the actual supporting authored floor must classify the impact"
    );
    let mut drained_again = 0_u8;
    world.drain_impacts(|_| drained_again = drained_again.saturating_add(1));
    assert_eq!(
        drained_again, 0,
        "draining must consume events exactly once"
    );
    Ok(())
}

#[test]
fn trigger_bounds_preserve_frame_start_and_material_queries_require_support() -> Result<(), String>
{
    let mut world = ItemWorld::from_level(&level()?, &[]);
    spawn(&mut world, 7, Vec3::new(-2.0, 2.0, 0.0))?;
    assert!(
        world.set_fixture_velocity(7, Vec3::new(12.0, 0.0, 0.0), Vec3::ZERO),
        "the fixture must accept bounded physical velocity"
    );
    world.step(FIXED_STEP * 4.0, player(), &[]);
    let mut snapshot = None;
    world.for_each_trigger_body(|body| snapshot = Some(body));
    let body = snapshot.ok_or("trigger body snapshot disappeared")?;
    assert!(
        (body.previous.x + 2.0).abs() < 1.0e-5 && body.current.x > -1.65,
        "the trigger sweep must span all four substeps"
    );
    assert!(
        body.half_extents.distance(Vec3::splat(0.1)) < 1.0e-5 && !body.held,
        "trigger geometry must retain the physical collider bounds and carry state"
    );
    assert_eq!(
        world.floor_material_at(Vec3::new(2.0, 0.0, 0.0)),
        Some("core:carpet_beige_01"),
        "the last floor patch must override the room material"
    );
    assert_eq!(
        world.floor_material_at(Vec3::new(2.0, 1.0, 0.0)),
        None,
        "a prop top or airborne body must not inherit the floor far below"
    );
    Ok(())
}

#[test]
fn trigger_path_retains_a_bounce_crossing_hidden_by_frame_endpoints() -> Result<(), String> {
    // One wall and twelve solver intervals expose the reversal without a full
    // map or an extended performance run. The old endpoint chord missed it.
    let wall = WallAabb::with_y(0.9, 0.0, -1.0, 0.1, 4.0, 2.0);
    let mut world = ItemWorld::from_level(&level()?, &[wall]);
    world.spawn(
        8,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            linear_damping: 0.0,
            angular_damping: 0.0,
            friction: 0.0,
            restitution: 1.0,
            reset_group: Some("bounce".into()),
            ..PhysicsDef::default()
        },
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        Vec3::new(0.0, 2.0, 0.0),
        0.0,
        1.0,
    )?;
    assert!(
        world.set_fixture_velocity(8, Vec3::new(20.0, 0.0, 0.0), Vec3::ZERO),
        "the fixture must accept its bounded collision velocity"
    );
    world.step(0.1, player(), &[]);
    let mut snapshot = None;
    world.for_each_trigger_body(|body| snapshot = Some(body));
    let body = snapshot.ok_or("bouncing trigger body disappeared")?;
    assert!(
        body.previous.x + body.half_extents.x < 0.55 && body.current.x + body.half_extents.x < 0.55,
        "frame endpoints must both lie on the near side of the test trigger"
    );
    assert!(
        body.path().iter().any(|sample| sample.x > 0.6),
        "the captured substep path must retain the outward crossing before the bounce"
    );
    assert!(
        body.path().len() <= 14,
        "the full maximum-substep frame must retain bounded path storage"
    );
    let _reset = world.reset_group("bounce")?;
    world.for_each_trigger_body(|reset_body| {
        assert_eq!(
            reset_body.path().len(),
            1,
            "an authoritative reset must discard the old path instead of crossing triggers"
        );
    });
    Ok(())
}

#[test]
fn rotating_trigger_bounds_enclose_the_intermediate_diagonal() -> Result<(), String> {
    // A single rectangular kinematic body turns ninety degrees in one step.
    // Both endpoint AABBs stop at one metre; the diagonal exceeds that bound.
    let mut world = ItemWorld::from_level(&level()?, &[]);
    let initial = Vec3::new(0.0, 2.0, 2.0);
    world.spawn(
        9,
        &PhysicsDef {
            motion: BodyMotion::Kinematic,
            ..PhysicsDef::default()
        },
        (Vec3::new(-1.0, -0.05, -0.5), Vec3::new(1.0, 0.05, 0.5)),
        initial,
        0.0,
        1.0,
    )?;
    assert!(
        world.set_kinematic_pose(
            9,
            ItemPose {
                position: initial,
                rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            }
        ),
        "the rectangular fixture must accept a prescribed quarter-turn"
    );
    world.step(FIXED_STEP, player(), &[]);
    let mut snapshot = None;
    world.for_each_trigger_body(|body| snapshot = Some(body));
    let body = snapshot.ok_or("rotating trigger body disappeared")?;
    assert!(
        body.half_extents.x > 1.05 && body.half_extents.z > 1.05,
        "rotation bounds must enclose the diagonal beyond both endpoint AABBs"
    );
    assert!(
        body.path()
            .iter()
            .all(|center| center.distance(initial) < 1.0e-5),
        "pure rotation must retain its rigid origin throughout the trigger path"
    );
    Ok(())
}

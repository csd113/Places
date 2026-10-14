//! Deterministic water fixtures: a few small bodies, no shipped-map preparation.
#![allow(
    clippy::expect_used,
    reason = "Invalid deterministic fixture setup is a test failure"
)]

use glam::{Quat, Vec3};

use super::{
    BodyMotion, BodySnapshot, FIXED_STEP, GRAVITY, ItemPose, ItemWorld, PhysicsDef, PlayerContext,
    ShapeDef, WaterPhysicsDef,
};
use crate::level::{LevelDef, WaterVolumes};

fn level(water: &str) -> LevelDef {
    serde_json::from_str(&format!(
        r#"{{"format_version":3,"id":"water_physics_fixture","name":"water physics",
        "spawn":{{"x":0,"z":0}},
        "rooms":[{{"x":-3,"z":-3,"width":6,"depth":6,"height":6,"floor_y":-2}}],
        "water":[{water}]}}"#
    ))
    .expect("small water fixture")
}

fn pool() -> LevelDef {
    level(r#"{"x":-2,"z":-2,"width":4,"depth":4,"surface_y":0,"bottom_y":-2}"#)
}

fn context() -> PlayerContext {
    PlayerContext {
        eye: Vec3::new(2.5, 1.6, 2.5),
        feet: Vec3::new(2.5, 0.0, 2.5),
        height: 1.8,
        view: -Vec3::Z,
        yaw: 0.0,
        active: false,
    }
}

fn profile(mass_kg: f32, volume_m3: Option<f32>) -> PhysicsDef {
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        shape: Some(ShapeDef::Cuboid {
            half_extents: [0.1; 3],
        }),
        mass_kg,
        linear_damping: 0.0,
        angular_damping: 0.0,
        pickup: true,
        water: Some(WaterPhysicsDef {
            displaced_volume_m3: volume_m3,
            linear_drag: 4.0,
            angular_drag: 2.5,
        }),
        ..PhysicsDef::default()
    }
}

fn spawn(world: &mut ItemWorld, key: u64, definition: &PhysicsDef, position: Vec3) {
    world
        .spawn(
            key,
            definition,
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            position,
            0.0,
            1.0,
        )
        .expect("valid water body");
}

fn snapshot(world: &ItemWorld, key: u64) -> BodySnapshot {
    world
        .snapshots()
        .into_iter()
        .find(|body| body.key == key)
        .expect("existing water fixture body")
}

// Protects the phase split: all immersion reads finish before stable-key body
// mutations. Sixty-four tiny tilted floats take one fixed step, about 9,216
// primitive rays; two explicit pools avoid changing the process-wide executor.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn parallel_immersion_preserves_exact_poses_velocities_and_contacts() -> Result<(), String> {
    let level = pool();
    let build = |workers| -> Result<ItemWorld, String> {
        let worker_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map(std::sync::Arc::new)
            .map_err(|error| error.to_string())?;
        let mut world = ItemWorld::from_level(&level, &[]);
        world.set_worker_pool(worker_pool);
        let definition = profile(4.0, Some(0.008));
        for index in 0..64_u16 {
            let key = u64::from(index).saturating_add(1);
            let position = Vec3::new(
                f32::from(index % 8).mul_add(0.35, -1.3),
                -0.01,
                f32::from(index / 8).mul_add(0.35, -1.3),
            );
            world.spawn(
                key,
                &definition,
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                position,
                0.0,
                1.0,
            )?;
            assert!(
                world.set_fixture_pose(
                    key,
                    ItemPose {
                        position,
                        rotation: Quat::from_rotation_z(0.7),
                    }
                ),
                "tilted partial immersion exercises quadrature"
            );
            assert!(
                world.set_fixture_velocity(
                    key,
                    Vec3::new(0.05, -0.03, 0.02),
                    Vec3::new(0.1, 0.0, 0.15)
                ),
                "finite initial velocities exercise both fluid decays"
            );
        }
        world.step(FIXED_STEP, context(), &[]);
        Ok(world)
    };
    let serial = build(1)?;
    let parallel = build(2)?;
    assert!(
        parallel
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.water_work.len() == 64),
        "the delayed immersion branch measured all sixty-four bodies"
    );
    let serial_snapshots = serial.snapshots();
    let parallel_snapshots = parallel.snapshots();
    assert!(
        parallel_snapshots
            .iter()
            .all(|body| body.submerged_fraction > 0.0 && body.submerged_fraction < 1.0),
        "every tilted float remains partially immersed"
    );
    let encode = |world: &ItemWorld, snapshots: Vec<BodySnapshot>| {
        serde_json::to_vec(&(snapshots, world.contact_pairs(), world.contact_impulses()))
            .map_err(|error| error.to_string())
    };
    assert_eq!(
        encode(&serial, serial_snapshots)?,
        encode(&parallel, parallel_snapshots)?,
        "worker count preserves exact poses, velocities, immersion and contacts"
    );
    Ok(())
}

// Protects profile round-tripping, bounded source/scaled lift and strict nested
// keys. Pure metadata plus one rejected spawn; no solver campaign is needed.
#[test]
fn water_profile_rejects_invalid_coefficients_and_unsafe_scaled_lift() {
    let definition = profile(1.0, Some(0.008));
    let encoded = serde_json::to_string(&definition).expect("water metadata serializes");
    let decoded: PhysicsDef = serde_json::from_str(&encoded).expect("water metadata round trips");
    assert_eq!(
        decoded, definition,
        "water contract survives semantics serialization"
    );
    for source in [
        r#"{"motion":"dynamic","water":{"unknown":1}}"#,
        r#"{"motion":"dynamic","water":{"linear_drag":null}}"#,
        r#"{"motion":"dynamic","water":{"displaced_volume_m3":"1"}}"#,
    ] {
        assert!(
            serde_json::from_str::<PhysicsDef>(source).is_err(),
            "strict water keys/types"
        );
    }
    for water in [
        WaterPhysicsDef {
            displaced_volume_m3: Some(0.0),
            ..WaterPhysicsDef::default()
        },
        WaterPhysicsDef {
            displaced_volume_m3: Some(f32::NAN),
            ..WaterPhysicsDef::default()
        },
        WaterPhysicsDef {
            linear_drag: -1.0,
            ..WaterPhysicsDef::default()
        },
        WaterPhysicsDef {
            angular_drag: f32::INFINITY,
            ..WaterPhysicsDef::default()
        },
    ] {
        assert!(
            water.validate().is_err(),
            "nonphysical water coefficient rejected"
        );
    }
    assert!(
        profile(0.001, Some(1.0)).validate().is_err(),
        "source lift is mass bounded"
    );
    assert!(
        definition.validate_scale(3.0).is_err(),
        "scale cubed cannot bypass lift bound"
    );
    let fixed = PhysicsDef {
        motion: BodyMotion::Fixed,
        pickup: false,
        ..definition
    };
    assert!(
        fixed.validate().is_err(),
        "fixed scenery cannot author fluid response"
    );
    let nullable: PhysicsDef = serde_json::from_str(r#"{"motion":"dynamic","water":null}"#)
        .expect("optional null profile follows serde Option contract");
    assert!(
        nullable.water.is_none(),
        "null means use default solid displacement"
    );
}

// Protects partial horizontal displacement, off-centre pressure torque, circle
// corners and finite bottoms with four one-step cubes, rather than a map bake.
#[test]
fn immersion_integrates_edges_circle_footprints_and_finite_depth() {
    let rect = level(r#"{"x":0,"z":-2,"width":2,"depth":4,"surface_y":0,"bottom_y":-1}"#);
    let mut edge = ItemWorld::from_level(&rect, &[]);
    spawn(
        &mut edge,
        1,
        &profile(1.0, None),
        Vec3::new(-0.05, 0.0, 0.0),
    );
    edge.step(FIXED_STEP, context(), &[]);
    let edge_body = snapshot(&edge, 1);
    assert!(
        (edge_body.submerged_fraction - 0.125).abs() < 1.0e-5,
        "quarter footprint times half height"
    );
    assert!(
        edge_body.angular_velocity.z > 0.01,
        "pressure on submerged side produces torque"
    );
    assert!(
        edge_body.linear_velocity.y.abs() < 1.0e-4,
        "one litre displaced balances one kilogram without a scripted waterline"
    );

    let circle =
        level(r#"{"shape":"circle","x":-1,"z":-1,"radius":1,"surface_y":0,"bottom_y":-1}"#);
    let mut corner = ItemWorld::from_level(&circle, &[]);
    spawn(
        &mut corner,
        1,
        &profile(1.0, None),
        Vec3::new(0.95, 0.0, 0.95),
    );
    corner.step(FIXED_STEP, context(), &[]);
    assert!(
        snapshot(&corner, 1).submerged_fraction.abs() < 1.0e-7,
        "circle bounding-box corner is dry"
    );

    let shallow = level(r#"{"x":-2,"z":-2,"width":4,"depth":4,"surface_y":0,"bottom_y":-0.02}"#);
    let mut shallow_world = ItemWorld::from_level(&shallow, &[]);
    spawn(
        &mut shallow_world,
        1,
        &profile(1.0, None),
        Vec3::new(1.2345, 0.0, 1.7654),
    );
    shallow_world.step(FIXED_STEP, context(), &[]);
    assert!(
        (snapshot(&shallow_world, 1).submerged_fraction - 0.1).abs() < 1.0e-5,
        "water below authored bottom contributes no displaced volume"
    );
    assert!(
        snapshot(&shallow_world, 1).angular_velocity.length() < 1.0e-6,
        "symmetric partial immersion has no round-trip centroid torque"
    );
    let mut submerged = ItemWorld::from_level(&pool(), &[]);
    spawn(
        &mut submerged,
        1,
        &profile(1.0, None),
        Vec3::new(1.2345, -0.7, 1.7654),
    );
    submerged.step(FIXED_STEP, context(), &[]);
    let full = snapshot(&submerged, 1);
    assert!((full.submerged_fraction - 1.0).abs() < 1.0e-6);
    assert!(
        full.angular_velocity.length() < 1.0e-6,
        "full displacement acts at the volume centroid, without numerical torque"
    );
    assert!(
        (GRAVITY * 7.0)
            .mul_add(-FIXED_STEP, full.linear_velocity.y)
            .abs()
            < 1.0e-5,
        "full immersion gives exact eight-litre lift against one kilogram"
    );
}

// Protects real float/sink equilibrium and orientation restoring forces. Two
// boxes run six simulated seconds (720 bounded steps), with no assets or IO.
#[test]
fn light_body_bobs_and_rights_while_dense_open_body_sinks() {
    let mut world = ItemWorld::from_level(&pool(), &[]);
    let floating_profile = PhysicsDef {
        shape: Some(ShapeDef::Cuboid {
            half_extents: [0.2, 0.1, 0.15],
        }),
        ..profile(1.0, Some(0.008))
    };
    spawn(&mut world, 1, &floating_profile, Vec3::new(-0.6, 0.35, 0.0));
    spawn(
        &mut world,
        2,
        &profile(6.4, Some(0.0008)),
        Vec3::new(0.6, 0.35, 0.0),
    );
    assert!(
        world.set_fixture_pose(
            1,
            ItemPose {
                position: Vec3::new(-0.6, 0.35, 0.0),
                rotation: Quat::from_rotation_z(0.3),
            }
        ),
        "tilted release fixture"
    );
    let mut rose_after_entry = false;
    let mut entered_water = false;
    for _ in 0_u32..360_u32 {
        world.step(1.0 / 60.0, context(), &[]);
        let floating = snapshot(&world, 1);
        entered_water |= floating.submerged_fraction > 0.0;
        rose_after_entry |= entered_water && floating.linear_velocity.y > 0.02;
        assert!(
            floating.pose.position.is_finite() && floating.angular_velocity.is_finite(),
            "bob remains finite"
        );
        assert!(
            world.take_recoveries().is_empty(),
            "ordinary buoyancy needs no escape recovery"
        );
    }
    let floating = snapshot(&world, 1);
    let sinking = snapshot(&world, 2);
    assert!(rose_after_entry, "gravity entry causes a buoyant bob");
    assert!(
        (floating.pose.position.y - 0.075).abs() < 0.04,
        "equilibrium waterline follows mass/displacement: {floating:?}"
    );
    assert!(
        floating.pose.rotation.mul_vec3(Vec3::Y).dot(Vec3::Y) > 0.9,
        "distributed submerged volume restores a tipped broad float: {floating:?}"
    );
    assert!(
        sinking.pose.position.y < -1.8,
        "dense open body settles on real basin floor"
    );
}

// Protects authored dormancy, action toggles and reset clearing using one
// body and at most 24 steps. No water toggle may implicitly activate authored props.
#[test]
fn water_actions_preserve_dormancy_and_reset_clears_immersion() {
    let fixture = pool();
    let mut world = ItemWorld::from_level(&fixture, &[]);
    let definition = PhysicsDef {
        reset_group: Some("water_demo".into()),
        ..profile(1.0, Some(0.008))
    };
    world
        .spawn_authored(
            1,
            &definition,
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::ZERO,
            0.0,
            1.0,
        )
        .expect("authored wet prop");
    let mut volumes = WaterVolumes::from_level(&fixture);
    assert!(volumes.set_enabled(0, false), "disable authored water");
    world.sync_water(&volumes);
    world.step(0.1, context(), &[]);
    assert!(snapshot(&world, 1).dormant, "disable retains authored pose");
    assert!(volumes.set_enabled(0, true), "enable authored water");
    world.sync_water(&volumes);
    world.step(0.1, context(), &[]);
    assert!(snapshot(&world, 1).dormant, "enable retains authored pose");
    assert_eq!(
        world
            .activate_group("water_demo")
            .expect("activate group")
            .activated_count,
        1,
        "explicit activation releases dormant body"
    );
    world.step(FIXED_STEP, context(), &[]);
    assert!(
        snapshot(&world, 1).linear_velocity.y > 0.0,
        "active float receives pressure"
    );
    let _reset = world.reset_group("water_demo").expect("unblocked reset");
    let reset = snapshot(&world, 1);
    assert!(
        reset.dormant && reset.linear_velocity.length_squared() < 1.0e-8,
        "reset restores exact dormant motion"
    );
    assert!(
        reset.submerged_fraction.abs() < 1.0e-8,
        "reset clears previous water diagnostics"
    );
}

// Protects the minimum collider dimension against an otherwise finite but
// unstable buoyancy spring. One rejected profile and one 240-step millimetre
// fixture uniquely cover the supported numerical boundary.
#[test]
fn millimetre_float_rejects_excessive_stiffness_and_remains_stable_when_supported() {
    let mut world = ItemWorld::from_level(&pool(), &[]);
    let thin = PhysicsDef {
        shape: Some(ShapeDef::Cuboid {
            half_extents: [0.001; 3],
        }),
        ..profile(0.001, Some(0.0001))
    };
    assert!(
        world
            .spawn(
                1,
                &thin,
                (Vec3::splat(-0.001), Vec3::splat(0.001)),
                Vec3::ZERO,
                0.0,
                1.0
            )
            .is_err(),
        "finite but stiff lift is explicitly rejected"
    );
    assert!(
        world.snapshots().is_empty(),
        "rejected profile creates no partial body"
    );
    let supported = PhysicsDef {
        water: Some(WaterPhysicsDef {
            displaced_volume_m3: Some(0.000_002),
            linear_drag: 100.0,
            angular_drag: 100.0,
        }),
        ..thin
    };
    spawn(&mut world, 1, &supported, Vec3::new(0.0, 0.0005, 0.0));
    for _ in 0_u32..240_u32 {
        world.step(FIXED_STEP, context(), &[]);
        let body = snapshot(&world, 1);
        assert!(
            body.pose.position.y.abs() < 0.01 && body.linear_velocity.length() < 0.5,
            "supported millimetre float does not launch or chatter: {body:?}"
        );
        assert!(
            world.take_recoveries().is_empty(),
            "stable integration requires no recovery"
        );
    }
}

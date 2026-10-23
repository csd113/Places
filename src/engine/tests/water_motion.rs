//! Tiny source/collision roundtrip and two physical cubes protect current
//! direction/precedence, safe controls, and density-dependent flotation. No
//! renderer, assets or baked map; the solver runs only three simulated seconds.
use crate::engine::level::{LevelDef, WaterMotionDef};
use crate::engine::loader::validate_level;
use crate::engine::package::collision::{CompiledCollision, read_collision, write_collision};
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};
use crate::game::CollisionWorld;
use glam::Vec3;

fn level(speed: f32) -> Result<LevelDef, String> {
    LevelDef::from_json(&serde_json::json!({"schema":"places-level","format_version":1_u32,"id":"flow_test","name":"Flow test",
      "spawn":{"x":0_u32,"z":0_u32},"rooms":[{"x":0_u32,"z":0_u32,"width":12_u32,"depth":12_u32,"height":6_u32,"floor_y":-3.0}],
      "water":[{"x":1_u32,"z":1_u32,"width":10_u32,"depth":10_u32,"surface_y":0.0,"bottom_y":-3.0,
       "motion":{"speed":0.0,"strength":0.0,"flow_speed":speed,"direction_degrees":90_u32}}]}).to_string()).map_err(|error|error.to_string())
}

#[test]
fn motion_controls_validate_and_restore_beside_unchanged_collision_records() -> Result<(), String> {
    let source = level(0.8)?;
    validate_level(&source)?;
    let encoded = write_collision(&CompiledCollision::from_level(&source))?;
    let world = CollisionWorld::from_compiled(&source, read_collision(&encoded)?, None);
    let sample = world.water.sample(5.0, 5.0, -1.0).ok_or("missing water")?;
    assert!(
        (sample.flow[0] - 0.8).abs() < 0.0001 && sample.flow[1].abs() < 0.0001,
        "source controls must hydrate compiled geometry with Places yaw convention"
    );
    for invalid in [
        WaterMotionDef {
            speed: f32::NAN,
            ..WaterMotionDef::default()
        },
        WaterMotionDef {
            flow_speed: 4.1,
            ..WaterMotionDef::default()
        },
        WaterMotionDef {
            strength: -0.1,
            ..WaterMotionDef::default()
        },
    ] {
        assert!(
            !invalid.is_valid(),
            "invalid controls cannot enter CPU/GPU water"
        );
    }
    assert!(
        serde_json::from_str::<WaterMotionDef>(r#"{"flow_speeed":1}"#).is_err(),
        "a typo must not silently produce still water"
    );
    let mut malformed = source;
    malformed
        .water
        .first_mut()
        .ok_or("missing source")?
        .motion
        .direction_degrees = f32::INFINITY;
    assert!(
        validate_level(&malformed).is_err(),
        "production validation must enforce the shared bounds"
    );
    Ok(())
}

#[test]
fn relative_fluid_drag_moves_floating_and_dense_objects_without_changing_density()
-> Result<(), String> {
    for speed in [0.0, 0.8] {
        let source = level(speed)?;
        let mut world = ItemWorld::from_level(&source, &[]);
        for (key, mass, x) in [(1_u64, 1.0, 4.0), (2, 16.0, 7.0)] {
            world.spawn(
                key,
                &PhysicsDef {
                    motion: BodyMotion::Dynamic,
                    mass_kg: mass,
                    shape: Some(ShapeDef::Cuboid {
                        half_extents: [0.1; 3],
                    }),
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    ..PhysicsDef::default()
                },
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                Vec3::new(x, -0.15, 6.0),
                0.0,
                1.0,
            )?;
        }
        let player = PlayerContext {
            half: crate::engine::collision::PLAYER_RADIUS,
            eye: Vec3::new(0.0, 1.6, 0.0),
            feet: Vec3::ZERO,
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: true,
        };
        for _ in 0_u16..360 {
            world.step(FIXED_STEP, player, &[]);
        }
        let bodies = world.snapshots();
        let light = bodies
            .iter()
            .find(|body| body.key == 1)
            .ok_or("missing float")?;
        let dense = bodies
            .iter()
            .find(|body| body.key == 2)
            .ok_or("missing sink")?;
        assert!(
            light.pose.position.y > -0.15 && dense.pose.position.y < -2.0,
            "the authored mass/displacement ratio must still decide float versus sink"
        );
        if speed > 0.0 {
            assert!(
                light.pose.position.x > 5.0 && dense.pose.position.x > 7.1,
                "both physical densities must receive the authored +X current"
            );
        } else {
            assert!(
                (light.pose.position.x - 4.0).abs() < 0.01
                    && (dense.pose.position.x - 7.0).abs() < 0.01,
                "still water must inject no horizontal motion"
            );
        }
    }
    Ok(())
}

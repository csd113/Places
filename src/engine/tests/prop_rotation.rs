//! Saved arbitrary prop orientations must agree across rendering, collision and
//! dormant rigid physics. One tiny source + one CPU body guards the previous
//! yaw-only drift without loading assets, baking a map or widening broad tests.

use glam::Vec3;

use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef};

#[test]
fn sideways_embedded_prop_retains_full_rotation_and_collision_bounds() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
        "schema":"places-level","format_version":1,"id":"tilted","name":"Tilted desk",
        "spawn":{"x":1,"z":1},"rooms":[{"width":6,"depth":6,"height":4}],
        "props":[{"id":"sideways","model":"core:desk","x":3,"y":-0.4,"z":3,
          "rotation_degrees":31,"pitch_degrees":90,"roll_degrees":17,"scale":1.25,
          "solid":true,"size":[1.4,0.8,0.7]}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    let saved = serde_json::to_string(&level).map_err(|error| error.to_string())?;
    let reopened = LevelDef::from_json(&saved).map_err(|error| error.to_string())?;
    let prop = reopened.props.first().ok_or("Missing prop")?;
    assert_eq!(
        (
            prop.rotation_degrees,
            prop.pitch_degrees,
            prop.roll_degrees,
            prop.y
        ),
        (31.0, 90.0, 17.0, -0.4)
    );
    let base = LevelSurfaces::new(&reopened)
        .floor_y_at(prop.x, prop.z)
        .ok_or("Missing floor")?;
    let bounds = prop.placement_bounds(base, prop.size.ok_or("Missing size")?);
    let collider = reopened
        .collision_aabbs()
        .into_iter()
        .find(|wall| wall.min_y < 0.0)
        .ok_or("Missing embedded prop collider")?;
    for (actual, expected) in [
        collider.min_x,
        collider.min_y,
        collider.min_z,
        collider.max_x,
        collider.max_y,
        collider.max_z,
    ]
    .into_iter()
    .zip(bounds.min.into_iter().chain(bounds.max))
    {
        assert!(
            (actual - expected).abs() < 0.00001,
            "collision encloses the exact rendered rotated box"
        );
    }
    assert!(
        bounds.min[1] < -0.4 && bounds.max[1] > 0.0,
        "intentional floor penetration remains represented"
    );
    let mut physics = ItemWorld::default();
    physics.spawn_authored_oriented(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.7, 0.0, -0.35), Vec3::new(0.7, 0.8, 0.35)),
        Vec3::new(prop.x, prop.y, prop.z),
        prop.orientation(),
        prop.scale,
    )?;
    physics.step(
        crate::engine::physics::FIXED_STEP,
        crate::engine::physics::PlayerContext {
            half: crate::engine::collision::PLAYER_RADIUS,
            eye: Vec3::new(20.0, 1.8, 20.0),
            feet: Vec3::new(20.0, 0.0, 20.0),
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: true,
        },
        &[],
    );
    let body = physics
        .snapshots()
        .into_iter()
        .next()
        .ok_or("Missing rigid body")?;
    assert!(
        body.pose
            .position
            .distance(Vec3::new(prop.x, prop.y, prop.z))
            < 0.00001,
        "dormant solver steps retain the saved embedded origin"
    );
    assert!(
        body.pose.rotation.dot(prop.orientation()).abs() > 0.99999,
        "authored rigid body starts at the identical quaternion"
    );
    assert!(
        matches!(body.state, crate::engine::physics::BodyState::Dormant),
        "arbitrary rotation retains frozen-on-load policy"
    );
    Ok(())
}

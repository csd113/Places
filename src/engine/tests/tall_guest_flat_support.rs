//! Regression value: an ankle above a flat floor does not prove that its
//! skinned sole/calf clears the plane. Existing stair coverage only exercises
//! uneven terrain. One native skin and two bounded IK samples take milliseconds;
//! no navigation bake, simulation campaign, GPU or audio. This is a component
//! candidate, not a reconstruction of the missing old native joint pose.
use crate::engine::{
    collision_index::CollisionIndex,
    entities::WorldContext,
    level::{LevelDef, WalkableFloor},
};
use glam::{Quat, Vec3};

#[test]
fn flat_floor_support_checks_skin_below_an_above_floor_ankle() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
      "schema":"places-level","format_version":1,"id":"flat_leg_support",
      "name":"Flat leg support","spawn":{"x":100,"z":100},
      "rooms":[{"x":0,"z":0,"width":20,"depth":20,"height":3.8}],
      "props":[{"id":"guest","model":"tall-guest","x":10,"z":10,
        "solid":false,"tall_guest":{}}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let feet = Vec3::new(100.0, 0.0, 100.0);
    let ctx = WorldContext {
        delta_seconds: 1.0 / 120.0,
        player_walk_speed: 3.75,
        feet_from: feet,
        feet,
        eye: feet + Vec3::Y * 1.6,
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let mut guest = crate::engine::tall_guest::load(&level)?
        .pop()
        .ok_or("missing Guest")?;
    guest.sample_floor(&ctx);
    *guest.state.plants.get_mut(1).ok_or("missing left stance")? = None;
    let foot = guest.socket("foot_left").ok_or("missing foot")?;
    let (_, rotation, position) = foot.to_scale_rotation_translation();
    let target = guest
        .transform()
        .inverse()
        .transform_point3(position - Vec3::Y * 0.032);
    guest.solve_leg(
        "left",
        target,
        Quat::from_rotation_y(-guest.state.yaw) * rotation,
    );
    let before = guest
        .sole_support(1, &ctx)
        .ok_or("missing skinned left leg")?
        .0;
    let ankle_before = guest
        .socket("foot_left")
        .ok_or("missing foot")?
        .transform_point3(Vec3::ZERO);
    assert!(
        ankle_before.y > 0.0 && before < -0.005,
        "candidate must have an above-floor ankle and penetrating skin: ankle={ankle_before:?}, clearance={before}"
    );
    guest.seat_terrain_soles(&ctx);
    let after = guest
        .sole_support(1, &ctx)
        .ok_or("missing skinned left leg")?
        .0;
    let ankle_after = guest
        .socket("foot_left")
        .ok_or("missing foot")?
        .transform_point3(Vec3::ZERO);
    assert!(after >= -0.005, "Flat-floor skin clearance remains{after}m");
    assert!(
        glam::Vec2::new(ankle_before.x, ankle_before.z)
            .distance(glam::Vec2::new(ankle_after.x, ankle_after.z))
            < 0.002,
        "support correction must retain horizontal foot placement"
    );
    assert!(
        guest.state.plants.get(1).is_some_and(Option::is_none),
        "a corrected swing must not become a planted contact"
    );
    Ok(())
}

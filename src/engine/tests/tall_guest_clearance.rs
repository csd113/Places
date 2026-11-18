//! Two small CPU fixtures cover bulk-query boundaries and stale proxies on
//! restore. Neither needs navigation baking, a renderer, or a campaign map.
use glam::{Vec2, Vec3};

use crate::engine::level::{LevelDef, MeshClearance, WalkableCeiling, WalkableFloor};
use crate::engine::tall_guest;

#[test]
fn bulk_clearance_preserves_room_edges_gables_and_stacked_surfaces() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
          "schema":"places-level","format_version":1,"id":"pose_room_clearance",
          "name":"Pose room clearance","spawn":{"x":1,"z":1},
          "rooms":[
            {"x":0,"z":0,"width":8,"depth":6,"height":3,
             "ceiling":{"kind":"gable","ridge":"x","ridge_rise":1}},
            {"x":4,"z":0,"width":4,"depth":6,"floor_y":2,"height":3},
            {"x":20,"z":0,"width":4,"depth":6,"height":3}
          ]
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let ceiling = WalkableCeiling::from_level(&level);
    let rules = MeshClearance {
        disc_radius: 0.002,
        head_margin: 0.03,
        ground_slop: 0.005,
        ground_reach: 0.45,
    };
    let safe = Vec3::new(1.0, 2.0, 3.0);
    for (name, feet, point, expected) in [
        ("open gable interior", 0.0, safe, true),
        ("sloped eave disc", 0.0, Vec3::new(1.0, 2.98, -0.001), false),
        ("outside eave disc", 0.0, Vec3::new(1.0, 2.98, -0.003), true),
        (
            "raised floor underside",
            0.0,
            Vec3::new(4.001, 1.99, 3.0),
            false,
        ),
        (
            "ground seam tolerance",
            0.0,
            Vec3::new(-0.005, -0.01, 3.0),
            false,
        ),
        (
            "outside ground tolerance",
            0.0,
            Vec3::new(-0.02, -0.01, 3.0),
            true,
        ),
        (
            "ground under raised pose",
            2.0,
            Vec3::new(6.0, 1.99, 3.0),
            false,
        ),
    ] {
        let probe = [safe, point];
        let reference = probe.iter().all(|p| {
            ceiling
                .ceiling_above_disc(p.x, p.z, rules.disc_radius, feet)
                .is_none_or(|height| p.y + rules.head_margin < height)
                && floor
                    .lowest_room_underside(Vec2::new(p.x, p.z), rules.disc_radius, feet)
                    .is_none_or(|height| p.y + rules.head_margin < height)
                && floor
                    .height_below(p.x, p.z, feet + rules.ground_reach)
                    .is_none_or(|height| p.y + rules.ground_slop >= height)
        });
        assert_eq!(
            reference, expected,
            "{name} must exercise the intended surface"
        );
        assert_eq!(
            rules.is_clear(&floor, &ceiling, &probe, feet),
            reference,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn restored_guest_publishes_proxies_from_the_restored_pose() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
          "schema":"places-level","format_version":1,"id":"guest_proxy_restore",
          "name":"Guest proxy restore","spawn":{"x":1,"z":1},
          "rooms":[{"x":0,"z":0,"width":24,"depth":12,"height":4}],
          "props":[{"id":"guest","model":"tall-guest","x":4,"z":6,
                    "solid":false,"tall_guest":{}}]
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let mut guests = tall_guest::load(&level)?;
    let guest = guests.first_mut().ok_or("Missing Tall Guest fixture")?;
    let saved = guest.checkpoint();
    let original = guest.body_colliders();
    assert!(
        !original.is_empty(),
        "Fixture must publish posed collision proxies"
    );

    let mut moved = saved.clone();
    moved.position.x = 16.0;
    guest.restore(&moved)?;
    let shifted = guest.body_colliders();
    assert_eq!(
        shifted.len(),
        original.len(),
        "All joint clusters must still be present"
    );
    assert!(
        shifted
            .iter()
            .zip(&original)
            .all(|(after, before)| after.hinge_x > before.hinge_x + 10.0),
        "Restoring a different position must update every proxy"
    );

    guest.restore(&saved)?;
    assert_eq!(
        guest.body_colliders(),
        original,
        "Restoring the original pose must restore every proxy"
    );
    assert!(
        guest.set_enabled(false),
        "The active fixture must become disabled"
    );
    assert!(
        guest.body_colliders().is_empty(),
        "A disabled pose must publish no proxies"
    );
    Ok(())
}

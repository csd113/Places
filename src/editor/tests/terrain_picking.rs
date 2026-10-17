//! Essential regression: a recessed/ramped cell must pick its emitted surface,
//! never the obsolete original room plane. No renderer, assets, bake or I/O;
//! one tiny scene and four rays cost well below a millisecond.

use super::terrain_objects;
use crate::editor::document::ObjectId;
use crate::engine::level::LevelDef;
use glam::Vec3;

#[test]
fn picking_tracks_recess_depth_and_signed_ramp_surface() -> Result<(), String> {
    let level: LevelDef = serde_json::from_value(serde_json::json!({
        "schema":"places-level", "format_version":1_u32,"id":"pick","name":"Pick", "spawn":{"x":0.5_f32,"z":0.5_f32},
        "rooms":[{"width":6.0_f32,"depth":6.0_f32,"height":4.0_f32}],
        "floor_regions":[{"x":1.0_f32,"z":1.0_f32,"width":2.0_f32,"depth":2.0_f32,"offset_y":-1.5_f32}],
        "ramps":[{"x":3.0_f32,"z":1.0_f32,"width":2.0_f32,"depth":1.0_f32,"offset_y":1.0_f32,"rise":-1.0_f32}]
    })).map_err(|error|error.to_string())?;
    let objects = terrain_objects(&level);
    for (x, z, expected, distance) in [
        (2.0_f32, 2.0_f32, ObjectId::FloorRegion(0), 11.5_f32),
        (3.5_f32, 1.5_f32, ObjectId::Ramp(0), 9.25_f32),
        (4.5_f32, 1.5_f32, ObjectId::Ramp(0), 9.75_f32),
        (0.5_f32, 0.5_f32, ObjectId::Room(0), 10.0_f32),
    ] {
        let found = objects
            .iter()
            .filter_map(|object| {
                object
                    .distance(Vec3::new(x, 10.0_f32, z), -Vec3::Y)
                    .map(|hit| (object.id, hit))
            })
            .min_by(|first, second| first.1.total_cmp(&second.1))
            .ok_or("No surface picked")?;
        assert_eq!(
            found.0, expected,
            "Picking must select the emitted terrain owner"
        );
        assert!(
            (found.1 - distance).abs() < 0.0001_f32,
            "Picking must respect actual surface height"
        );
    }
    Ok(())
}

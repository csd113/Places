//! Static shutters need positive headroom for fixed housing and swept banks.
//! This probe uses just the real floor/roof patch builder; no bake or render.

use super::{include_pose, intersects, structure};
use crate::engine::collision::DoorCollider;
use crate::engine::level::LevelDef;

fn room(roof_y: f32, overhead: &str) -> Result<LevelDef, String> {
    LevelDef::from_json(&format!(
        r#"{{"schema":"places-level","format_version":1,
        "id":"shutter_support","name":"Shutter support","spawn":{{"x":1,"z":1}},
        "rooms":[{{"width":6,"depth":6,"height":{roof_y}}}{overhead}]}}"#
    ))
    .map_err(|error| error.to_string())
}

#[test]
fn shutter_envelope_rejects_fixed_housing_and_swept_floor_slabs() -> Result<(), String> {
    let housing = DoorCollider::from_pose([1.0, 2.4, 2.0], [1.0, 0.0], 2.6, 0.32, 0.84);
    let low_roof = room(3.0, "")?;
    assert!(
        structure(&low_roof)
            .iter()
            .any(|slab| intersects(slab, &housing))
    );
    let clear_roof = room(3.3, "")?;
    assert!(
        !structure(&clear_roof)
            .iter()
            .any(|slab| intersects(slab, &housing))
    );

    let mut swept = DoorCollider::from_pose([1.1, 0.0, 2.0], [1.0, 0.0], 2.4, 0.045, 0.8);
    let open = DoorCollider::from_pose([1.1, 2.42, 2.0], [1.0, 0.0], 2.4, 0.045, 0.8);
    include_pose(&mut swept, &open)?;
    let slab = room(
        7.0,
        r#",{"x":1,"z":1,"width":3,"depth":2,"floor_y":2.1,"height":3}"#,
    )?;
    let patches = structure(&slab);
    assert!(!patches.iter().any(|patch| intersects(patch, &open)));
    assert!(patches.iter().any(|patch| intersects(patch, &swept)));
    Ok(())
}

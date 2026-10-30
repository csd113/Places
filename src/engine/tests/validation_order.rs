//! One metadata-only fixture protects the cross-domain first-error contract.
//! Unlike isolated admission tests, simultaneous failures detect reordering of
//! the facade. The four checks need no assets, renderer, physics or map bake.

use crate::engine::level::LevelDef;
use crate::engine::loader::validate_level;

#[test]
fn simultaneous_domain_failures_keep_the_historical_first_error() -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"order","name":"Order","spawn":{"x":1,"z":1},"transitions":["next","next"],"rooms":[{"width":0,"depth":4}],"props":[{"model":""}]}"#,
    )
    .map_err(|error| error.to_string())?;
    level.schema = "wrong".into();
    for expected in [
        "Unsupported level schema; author this source as places-level revision 1",
        "Duplicate level transition destination",
        "Room 0 width, depth, and height must be positive",
        "Prop 0 must reference a non-empty model id",
    ] {
        let problem = validate_level(&level)
            .err()
            .ok_or("a simultaneous invalid-domain fixture was admitted")?;
        assert_eq!(
            problem, expected,
            "first-error ordering and text must stay stable"
        );
        match expected {
            "Unsupported level schema; author this source as places-level revision 1" => {
                level.schema = "places-level".into();
            }
            "Duplicate level transition destination" => level.transitions.clear(),
            "Room 0 width, depth, and height must be positive" => {
                level.rooms.first_mut().ok_or("missing fixture room")?.width = 4.0;
            }
            _ => level.props.clear(),
        }
    }
    validate_level(&level)
}

/// Sharing collision preparation must preserve both consumers and their error
/// order after an edit. One room, wall, door and two-metre route cost under
/// 16 KiB and milliseconds; no assets, GPU, bake or full authored map.
#[test]
fn shared_motion_geometry_checks_doors_routes_and_fresh_edits() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"motion_order","name":"Motion order",
        "spawn":{"x":1,"z":1},"rooms":[{"width":8,"depth":6,"height":3}],
        "walls":[{"x":5,"z":0,"width":0.2,"depth":6,"height":3}],
        "doors":[{"id":"door","x":1,"z":1,"width":1,"height":2}],
        "props":[{"id":"actor","model":"core:plant","x":1,"z":4,"size":[0.2,1.6,0.2],"solid":false}],
        "routes":[{"id":"actor","steps":[{"step":"move_to","x":3,"z":4,"speed":1}]}]
    }"#).map_err(|error| error.to_string())?;
    validate_level(&level)?;
    level.walls.first_mut().ok_or("missing wall")?.x = 2.0_f32;
    let route_problem = validate_level(&level)
        .err()
        .ok_or("blocked route admitted")?;
    assert!(
        route_problem.starts_with("Entity route"),
        "a valid door must not suppress the route sweep: {route_problem}"
    );
    level.doors.first_mut().ok_or("missing door")?.x = 2.05_f32;
    let door_problem = validate_level(&level).err().ok_or("buried door admitted")?;
    assert!(
        door_problem.starts_with("Door 0 leaf starts inside solid geometry"),
        "the door remains the first motion error: {door_problem}"
    );
    level.walls.first_mut().ok_or("missing wall")?.x = 5.0_f32;
    validate_level(&level)
}

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

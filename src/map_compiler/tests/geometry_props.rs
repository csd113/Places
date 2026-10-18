//! One tiny source protects the real anchored-bouquet false positive and the
//! eligibility boundary for other runtime poses. A genuine static duplicate
//! remains the sole finding. No GLB loading, map geometry build, GPU or bake.

use super::Checker;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::loader::PropCatalog;

#[test]
fn duplicate_props_require_static_pose_and_complete_appearance_identity() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
      "schema":"places-level","format_version":1,"id":"duplicate_props","name":"Tiny duplicate props",
      "spawn":{"x":0,"z":0},"props":[
        {"id":"static_a","model":"core:crate"},{"id":"static_b","model":"core:crate"},
        {"id":"bouquet_a","model":"arcade:party_balloon","x":1,"components":[{"component":"balloon","offset":[0.5,0,0],"color":"pink"}]},
        {"id":"bouquet_b","model":"arcade:party_balloon","x":1,"components":[{"component":"balloon","offset":[-0.5,0,0],"color":"orange","string_length":1.4,"phase":0.4}]},
        {"id":"route_a","model":"core:crate","x":2},{"id":"route_b","model":"core:crate","x":2},
        {"id":"physics_a","model":"core:crate","x":3,"physics":{"motion":"dynamic"}},
        {"id":"physics_b","model":"core:crate","x":3,"physics":{"motion":"kinematic"}},
        {"id":"ai_a","model":"core:crate","x":4,"components":[{"component":"ai","behavior":"idler"}]},
        {"id":"ai_b","model":"core:crate","x":4,"components":[{"component":"ai","behavior":"idler"}]},
        {"id":"animated_a","model":"core:crate","x":5,"components":[{"component":"animation","clip":"idle"}]},
        {"id":"animated_b","model":"core:crate","x":5,"components":[{"component":"animation","clip":"idle"}]},
        {"id":"tilt_a","model":"core:crate","x":6,"pitch_degrees":90},
        {"id":"tilt_b","model":"core:crate","x":6,"pitch_degrees":0},
        {"id":"material_a","model":"core:crate","x":7,"material_overrides":{"0":"core:first"}},
        {"id":"material_b","model":"core:crate","x":7,"material_overrides":{"0":"core:second"}}
      ],"routes":[{"id":"route_a","steps":[]}]
    }"#).map_err(|error| error.to_string())?;
    let mut checker = Checker::new(&level);
    checker.check_props(&PropCatalog::builtin(), &LevelSurfaces::new(&level));
    let duplicates: Vec<_> = checker
        .findings
        .iter()
        .filter(|finding| finding.check == "prop-duplicate")
        .collect();
    assert_eq!(
        duplicates.len(),
        1,
        "only truly identical static placement and appearance qualifies; live shared anchors are not rendered pose identity"
    );
    assert!(duplicates.first().is_some_and(|finding| finding.id == "static_a" && finding.message.contains("static_b")), "the actual static duplicate remains actionable");
    Ok(())
}

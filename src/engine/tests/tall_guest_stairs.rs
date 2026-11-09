//! Regression value: the real native skin used to freeze at the first19cm
//! riser, and a newly planted descending foot was rejected before its reach
//! could lower the support posture. One tiny navigation-only fixture exercises
//! a complete ascent/descent; no lighting bake, GPU, device audio or full map.
//! At most2400 fixed ticks, sampled state every12 ticks, with early success.
use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext};
use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor};
use crate::engine::nav::NavMesh;
use glam::Vec3;

#[test]
fn native_stair_contacts_acquire_support_before_validation() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
      "schema":"places-level","format_version":1,"id":"native_stair_contact",
      "name":"Native stair contact","spawn":{"x":63,"z":61},
      "rooms":[{"x":50,"z":59.2,"width":14,"depth":4.8,"height":3.8},{"x":50,"z":49,"width":14,"depth":10.2,"height":8},{"x":50,"z":59.2,"width":14,"depth":4.8,"floor_y":4.2,"height":3.8}],
      "floor_regions":[{"x":57,"z":50,"width":6,"depth":2,"offset_y":2.1}],
      "stairs":[{"x":58.5,"z":52,"width":3,"depth":6,"rise":2.1,"steps":11,"reverse":true}],
      "props":[{"id":"guest","model":"tall-guest","x":52,"z":60,
        "rotation_degrees":90,"solid":false,
        "tall_guest":{"patrol":[[52,0,60],[60,0,60],[60,0,58],[60,2.1,51],[60,0,60]]}}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let ceiling = WalkableCeiling::from_level(&level);
    let doors = crate::engine::door::Doors::from_level(&level);
    let classes = [crate::engine::tall_guest::navigation_class()];
    let (grid, _) = crate::map_compiler::navigation::bake(
        &crate::map_compiler::navigation::NavBakeInput {
            level: &level,
            walls: &[],
            floor: &floor,
            ceiling: &ceiling,
            doors: &doors,
            classes: &classes,
            obstacles: &[],
            walk_proxies: &[],
        },
        &crate::map_compiler::navigation::NavBakeOptions {
            cell_m: 0.2,
            workers: 1,
        },
    )?;
    let nav = NavMesh::from_record(grid)?;
    let mut path = crate::engine::nav::Path::new();
    let _result = nav.path_into(
        &crate::engine::nav::PathQuery {
            class: 0,
            start: Vec3::new(60.0, 0.0, 60.0),
            goal: Vec3::new(60.0, 2.1, 51.0),
            can_open_doors: false,
            max_expansions: 4096,
            doors: &crate::engine::nav::NoDoors,
        },
        &mut crate::engine::nav::NavScratch::new(),
        &mut path,
    );
    assert!(path.complete, "stair route is incomplete: {path:?}");
    let index = CollisionIndex::empty();
    let distant = Vec3::new(90.0, 0.0, 90.0);
    let mut world = EntityWorld::from_level(&level);
    let mut reached_landing = false;
    let mut last_position = Vec3::ZERO;
    for tick in 0_u16..2400 {
        let _report = world.tick(&WorldContext {
            delta_seconds: 1.0 / 60.0,
            player_walk_speed: 3.75,
            feet_from: distant,
            feet: distant,
            eye: Vec3::new(90.0, 1.6, 90.0),
            body_height: 1.8,
            body_half: 0.4,
            form_model: None,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: Some(&nav),
        });
        let _commands = world.take_commands();
        let _sounds = world.take_guest_sounds();
        if !tick.is_multiple_of(12) {
            continue;
        }
        let diagnostic = world.guest_diagnostics();
        let state = diagnostic
            .pointer("/0/state")
            .ok_or("native Guest absent")?;
        let position: [f32; 3] =
            serde_json::from_value(state.get("position").cloned().ok_or("missing position")?)
                .map_err(|error| error.to_string())?;
        last_position = Vec3::from(position);
        reached_landing |= last_position.y >= 2.09 && last_position.z < 52.0;
        if reached_landing && last_position.y < 0.05 && last_position.z >= 59.9 {
            return Ok(());
        }
    }
    Err(format!(
        "Native stair traversal stalled at {last_position:?}; landing reached={reached_landing}"
    ))
}

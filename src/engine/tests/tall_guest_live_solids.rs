//! Regression value and bounded cost: one flat navigation-only native controller
//! fixture reproduces the low-item replan deadlock. At most2400ticks with early
//! success; no full map, light bake, GPU or audio device.
use crate::engine::{
    collision::WallAabb,
    collision_index::CollisionIndex,
    entities::{EntityWorld, WorldContext},
    level::{LevelDef, WalkableCeiling, WalkableFloor},
    nav::NavMesh,
};
use glam::Vec3;
#[test]
fn native_guest_live_solids_follow_movement_and_removal() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
          "schema":"places-level","format_version":1,"id":"guest_item_nav",
          "name":"Guest item navigation","spawn":{"x":100,"z":100},
          "rooms":[{"x":8,"z":36,"width":22,"depth":16,"height":3.8}],
          "props":[{"id":"guest","model":"tall-guest","x":16.9,"z":43.2184,
            "rotation_degrees":90,"solid":false,
            "tall_guest":{"patrol":[[26,0,43.2184],[16.9,0,43.2184]]}}]
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
            cell_m: 0.4,
            workers: 1,
        },
    )?;
    let nav = NavMesh::from_record(grid)?;
    let index = CollisionIndex::empty();
    let mut world = EntityWorld::from_level(&level);
    let bottle = WallAabb::with_y(18.342, 0.002, 43.802_03, 0.116_001, 0.265_5, 0.095_939);
    let feet = Vec3::new(100.0, 0.0, 100.0);
    let mut end = serde_json::Value::Null;
    let mut blocked = 0;
    let moved_bottle = WallAabb::with_y(22.342, 0.002, 43.802_03, 0.116_001, 0.265_5, 0.095_939);
    let mut phase = 0_u8;
    let mut vacated_lane_seen = false;
    for tick in 0_u16..2400 {
        // Publish movement/removal exactly through the runtime's existing per-tick seam.
        world.set_navigation_obstacles(
            match phase {
                0 => Some(bottle),
                1 => Some(moved_bottle),
                _ => None,
            }
            .into_iter(),
        );
        let _report = world.tick(&WorldContext {
            delta_seconds: 1.0 / 60.0,
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
            nav: Some(&nav),
        });
        let _c = world.take_commands();
        let _s = world.take_guest_sounds();
        if tick.is_multiple_of(30) {
            end = world.guest_diagnostics();
            blocked = end
                .pointer("/0/events")
                .and_then(serde_json::Value::as_array)
                .map_or(0, |v| v.iter().filter(|e| e["type"] == "blocked").count());
            let position: [f32; 3] = serde_json::from_value(
                end.pointer("/0/state/position")
                    .cloned()
                    .ok_or("Missing native position")?,
            )
            .map_err(|error| error.to_string())?;
            if phase == 0 && position[0] > 25.8 {
                phase = 1;
            } else if phase == 1 && position[0] < 17.1 {
                phase = 2;
            } else if phase == 2 {
                vacated_lane_seen |= moved_bottle.overlaps_disc(position[0], position[2], 1.0);
                if position[0] > 25.8 {
                    assert!(vacated_lane_seen, "Removed item left its old lane blocked");
                    return Ok(());
                }
            }
        }
    }
    Err(format!(
        "Guest stalled in live-item phase{phase}: {end}; blocked events={blocked}"
    ))
}

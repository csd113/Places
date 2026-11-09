//! Essential regression: a Guest's conservative posed proxies must not enter
//! the stationary player square and strand all normal movement. One flat room,
//! one native skin, coarse CPU navigation and two600tick approaches; no lighting/GPU.
use crate::engine::collision::square_overlaps_door;
use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext};
use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor};
use crate::engine::nav::NavMesh;
use glam::{Vec2, Vec3};

fn approach(start_x: f32, expect_attack: bool) -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{
      "schema":"places-level","format_version":1,"id":"guest_player_proxy",
      "name":"Guest player proxy","spawn":{"x":8.946465,"z":42.96285},
      "rooms":[{"x":2,"z":36,"width":18,"depth":14,"height":3.8}],
      "props":[{"id":"guest","model":"tall-guest","x":18,"z":42,
        "rotation_degrees":-90,"solid":false,"tall_guest":{"patrol":[[8,0,42],[14,0,42]]}}]
    }"#,
    )
    .map_err(|e| e.to_string())?;
    level.props.first_mut().ok_or("Missing Guest fixture")?.x = start_x;
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
    let feet = Vec3::new(8.946_465, 0.0, 42.962_85);
    let mut world = EntityWorld::from_level(&level);
    let mut approached = false;
    let mut attacked = false;
    for _ in 0_u16..600 {
        let _report = world.tick(&WorldContext {
            delta_seconds: 1.0 / 60.0,
            player_walk_speed: 3.75,
            feet_from: feet,
            feet,
            eye: Vec3::new(feet.x, 1.6, feet.z),
            body_height: 1.8,
            body_half: 0.4,
            form_model: None,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: Some(&nav),
        });
        assert!(
            !world
                .door_colliders()
                .iter()
                .any(|body| body.overlaps_body_y(0.0, 1.8)
                    && square_overlaps_door(Vec2::new(feet.x, feet.z), 0.399, body)),
            "Native Guest published an overlapping player collider"
        );
        let diagnostic = world.guest_diagnostics();
        let at: [f32; 3] = serde_json::from_value(
            diagnostic
                .pointer("/0/state/position")
                .cloned()
                .ok_or("Missing native position")?,
        )
        .map_err(|e| e.to_string())?;
        attacked |= diagnostic
            .pointer("/0/events")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    event.get("type").and_then(serde_json::Value::as_str)
                        == Some("attack_requested")
                })
            });
        approached |= Vec3::from(at).distance(feet) < 2.0;
        let _commands = world.take_commands();
        let _sounds = world.take_guest_sounds();
    }
    assert!(approached, "Control must actually approach the player");
    if expect_attack {
        assert!(
            attacked,
            "Fresh close-contact evidence must retain the existing attack"
        );
    }
    Ok(())
}

#[test]
fn native_guest_approach_never_encloses_player_in_published_proxies() -> Result<(), String> {
    // The shorter sideways pass reproduces the reported unconfirmed patrol
    // contact. The longer approach permits two echoes and must still attack.
    approach(14.0, false)?;
    approach(18.0, true)
}

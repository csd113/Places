//! Synthetic capacity and far-coordinate regressions.
//!
//! The numerical boundary case must exceed the former limits to catch a cap
//! regression; it builds only collision and takes one controller step. The
//! four-room coordinate case protects indexed collision, route and volume
//! calculations kilometres from the origin. Authored showroom layout and
//! generated stress-map/package campaigns belong in explicit developer tools.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use crate::collision_index::CollisionIndex;
use crate::game::{CollisionWorld, Game};
use crate::level::LevelDef;

#[test]
fn collision_and_routes_work_kilometres_from_the_origin() {
    // Preserve the retired sparse map's CPU coordinate checks without keeping
    // a playable source that cannot compile its navigation grid.
    let mut rooms = Vec::new();
    let mut walls = Vec::new();
    for (cx, cz) in [
        (2_000.0_f64, 2_000.0_f64),
        (-2_000.0_f64, 2_000.0_f64),
        (-2_000.0_f64, -2_000.0_f64),
        (2_000.0_f64, -2_000.0_f64),
    ] {
        rooms.push(serde_json::json!({"x": cx - 13.0_f64, "z": cz - 10.0_f64,
            "width": 26.0_f64, "depth": 20.0_f64, "height": 4.0_f64}));
        for (x, z, width, depth) in [
            (cx - 13.4_f64, cz - 10.4_f64, 26.8_f64, 0.4_f64),
            (cx - 13.4_f64, cz + 10.0_f64, 26.8_f64, 0.4_f64),
            (cx - 13.4_f64, cz - 10.0_f64, 0.4_f64, 20.0_f64),
            (cx + 13.0_f64, cz - 10.0_f64, 0.4_f64, 20.0_f64),
        ] {
            walls.push(serde_json::json!({"x": x, "z": z, "width": width, "depth": depth}));
        }
    }
    let document = serde_json::json!({
        "format_version": 3_i32, "id": "far_coordinate_test", "name": "Far coordinate test",
        "spawn": {"x": 2_000.0_f64, "z": 1_994.0_f64}, "rooms": rooms, "walls": walls,
        "props": [{"id": "far_rat", "model": "rat", "x": 1_990.0_f64, "z": 2_006.0_f64, "rotation_degrees": 0.0_f64, "size": [0.2_f64, 0.2_f64, 0.7_f64]}],
        "water": [{"x": 2_002.0_f64, "z": 1_998.0_f64, "width": 6.0_f64, "depth": 4.0_f64, "surface_y": 0.35_f64, "bottom_y": 0.0_f64, "swimming": false}],
        "volumes": [{"id": "far_trigger", "x": -2_002.0_f64, "z": -2_002.0_f64, "width": 4.0_f64, "depth": 4.0_f64, "bindings": [{"on": "enter_volume", "actions": [{"action": "reset_to_start"}], "cooldown_seconds": 0.5_f64}]}],
        "routes": [{"id": "far_rat", "loop": true, "steps": [{"step": "move_to", "x": 1_998.0_f64, "z": 2_006.0_f64, "speed": 0.198_5_f64}, {"step": "wait", "seconds": 0.5_f64}, {"step": "move_to", "x": 1_990.0_f64, "z": 2_006.0_f64, "speed": 0.573_1_f64}, {"step": "wait", "seconds": 0.5_f64}]}],
    });
    let level = LevelDef::from_json(&document.to_string()).expect("the coordinate case parses");
    crate::loader::validate_level(&level).expect("the coordinate case validates");
    let world = CollisionWorld::from_level(&level);
    // Every room's floor, collision, water, trigger and route is a satellite
    // at ±2 km; the controller must stand, walk and collide there.
    assert!(level.props.iter().any(|prop| prop.x.abs() > 1_900.0));
    assert!(!level.water.is_empty());
    assert!(
        !level.volumes.is_empty(),
        "level.volumes must contain entries"
    );
    assert!(
        !level.routes.is_empty(),
        "level.routes must contain entries"
    );
    for room in level.room_iter() {
        let cx = room.x + room.width * 0.5;
        let cz = room.z + room.depth * 0.5;
        assert!(
            world.floor.walk_height_at(cx, cz).is_some(),
            "the walkable floor must resolve at ({cx}, {cz})"
        );
        assert!(
            world.walls.iter().any(|wall| {
                wall.overlaps_disc(room.x, room.z, 0.5)
                    || wall.overlaps_disc(room.x + room.width, room.z + room.depth, 0.5)
            }),
            "the room's own shell must be in the collision world at ({cx}, {cz})"
        );
    }
    // A rat walk far from the origin: the route must advance, not stall.
    let routes = world.world.routes();
    let route = routes
        .get("far_rat")
        .expect("the coordinate case routes a rat");
    let mut state = route.new_state();
    let index = CollisionIndex::build(&world.walls);
    let route_world = crate::entity::RouteWorld {
        walls: &world.walls,
        floor: &world.floor,
        index: &index,
    };
    let start = state.position;
    for _ in 0_i32..600_i32 {
        route.advance(&mut state, 1.0 / 60.0, &route_world);
    }
    assert!(
        (state.position - start).length() > 1.0,
        "the 2 km route must actually move; travelled {}",
        (state.position - start).length()
    );
    assert!(!state.blocked, "the 2 km route must not stall: {state:?}");
    // The reset volume in the opposite quadrant must be resolved and bounded.
    let volumes = &world.world.components().volumes;
    let far = volumes
        .iter()
        .find(|(handle, _)| world.world.id_of(*handle) == Some("far_trigger"))
        .map(|(_, volume)| *volume)
        .expect("the coordinate case's reset volume is named far_trigger");
    let mid_y = f32::midpoint(far.bottom_y, far.top_y);
    assert!(
        far.contains(-2002.0, -2002.0, mid_y),
        "the reset volume must cover its authored footprint: {far:?}"
    );
}

#[test]
fn the_2026_raised_caps_accept_content_past_their_former_boundary() {
    // The 2026 capacity pass raised the loader caps again: 2000 rooms,
    // 20 000 walls, 20 000 ceiling lights and 20 000 props became 8000 /
    // 60 000 / 50 000 / 100 000 (see `src/level.rs`). A level one past the
    // *former* value of each must parse, validate, collide and step the
    // controller: the raised values are only real if the engine handles
    // content the previous release refused.
    let mut rooms: Vec<String> = Vec::new();
    let mut walls: Vec<String> = Vec::new();
    let mut props: Vec<String> = Vec::new();
    let mut lights: Vec<String> = Vec::new();
    for index in 0_i32..20_001_i32 {
        let x = crate::test_support::exact_f32(index % 71_i32) * 2.0;
        let z = crate::test_support::exact_f32(index / 71_i32) * 2.0;
        props.push(format!(
            r#"{{"id": "p{index}", "model": "core:crate", "x": {x}, "z": {z}, "size": [0.6, 0.6, 0.6], "solid": false}}"#
        ));
    }
    for index in 0_i32..20_001_i32 {
        let x = crate::test_support::exact_f32(index % 71_i32) * 2.0 + 1.0;
        let z = crate::test_support::exact_f32(index / 71_i32) * 2.0 + 1.0;
        lights.push(format!(
            r#"{{"id": "l{index}", "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}}}"#
        ));
    }
    for index in 0_i32..20_001_i32 {
        let x = crate::test_support::exact_f32(index % 71_i32) * 2.0 + 0.5;
        let z = crate::test_support::exact_f32(index / 71_i32) * 2.0 + 0.5;
        walls.push(format!(
            r#"{{"x": {x}, "z": {z}, "width": 0.3, "depth": 0.3, "height": 2.0}}"#
        ));
    }
    for index in 0_i32..2_001_i32 {
        let x = crate::test_support::exact_f32(index % 45_i32) * 6.0;
        let z = crate::test_support::exact_f32(index / 45_i32) * 6.0;
        rooms.push(format!(
            r#"{{"x": {x}, "z": {z}, "width": 5.0, "depth": 5.0, "height": 3.0}}"#
        ));
    }
    let json = format!(
        r#"{{"format_version": 3, "id": "beyond_2026_caps", "name": "Beyond 2026 Caps",
            "spawn": {{"x": 1.0, "z": 1.0}},
            "rooms": [{}], "walls": [{}], "ceiling_lights": [{}], "props": [{}]}}"#,
        rooms.join(","),
        walls.join(","),
        lights.join(","),
        props.join(",")
    );
    let level = LevelDef::from_json(&json).expect("a level past the 2026 caps parses");
    crate::loader::validate_level(&level).expect("a level past the 2026 caps validates");
    let world = CollisionWorld::from_level(&level);
    assert!(world.walls.len() > 20_000);
    let index = CollisionIndex::build(&world.walls);
    assert_eq!(index.len(), world.walls.len());
    // A 0.3 m wall stub at (0.5, 0.5)..(0.8, 0.8) must resolve support through
    // both the linear and the indexed query.
    assert_eq!(
        crate::collision::highest_support_top(0.6, 0.6, 3.0, &world.walls),
        crate::collision::highest_support_top_indexed(&index, 0.6, 0.6, 3.0, &world.walls),
    );
    assert!(
        crate::collision::highest_support_top(0.6, 0.6, 3.0, &world.walls).is_some(),
        "the indexed support query must answer in a 20 001-wall world"
    );
    // A game built on it takes its first movement step without panicking.
    let mut game = Game::new(crate::game::spawn_position(&level), 0.0, world);
    game.set_app_state(crate::game::AppState::Playing);
    let mut input = crate::input::InputState::default();
    let settings = crate::settings::Settings::default();
    game.update_player_movement(&mut input, &settings);
}

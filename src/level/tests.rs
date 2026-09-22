//! Unit tests for the level schema, surfaces and walkable floor.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use crate::test_support::assert_exact;

#[test]
fn test_parse_single_room_level() {
    let json = r#"{
        "format_version": 1,
        "id": "test_room",
        "name": "Test Room",
        "spawn": { "x": 0.0, "z": 0.0 },
        "room": { "x": -6.0, "z": -12.0, "width": 12.0, "depth": 16.0, "height": 3.5 }
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let rooms: Vec<&RoomDef> = level.room_iter().collect();
    assert_eq!(rooms.len(), 1);
    assert_exact(rooms[0].width, 12.0);
    assert_exact(rooms[0].height, 3.5);
}

#[test]
fn test_parse_multi_room_level_with_walls() {
    let json = r#"{
        "format_version": 1,
        "id": "multi_room",
        "name": "Connected Rooms",
        "spawn": { "x": 2.0, "z": 2.0, "yaw_degrees": 90.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 },
            { "x": 10.0, "z": 2.0, "width": 8.0, "depth": 6.0 }
        ],
        "walls": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 0.35 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid multi room json");
    assert_eq!(level.room_iter().count(), 2);
    assert_eq!(level.walls.len(), 1);
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 1);
    assert_exact(aabbs[0].max_x, 10.0);
}

#[test]
fn test_parse_variable_wall_properties() {
    let json = r#"{
        "format_version": 1,
        "id": "variable_walls",
        "name": "Variable Walls Test",
        "spawn": { "x": 0.0, "z": 0.0 },
        "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 },
        "walls": [
            { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 0.2 },
            { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 0.0, "height": 1.5 },
            { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 2.5, "height": 1.5 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid variable walls json");
    assert_eq!(level.walls.len(), 3);

    // Wall 0: y omitted (defaults to 0.0), height omitted (defaults to room height 4.0)
    assert_exact(level.walls[0].y, 0.0);
    assert_eq!(level.walls[0].height, None);
    assert_exact(level.walls[0].resolved_height(4.0), 4.0);

    // Wall 1: window sill (half-height)
    assert_exact(level.walls[1].y, 0.0);
    assert_eq!(level.walls[1].height, Some(1.5));

    // Wall 2: window header (raised)
    assert_exact(level.walls[2].y, 2.5);
    assert_eq!(level.walls[2].height, Some(1.5));

    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 3);
    assert_exact(aabbs[0].min_y, 0.0);
    assert_exact(aabbs[0].max_y, 4.0);
    assert_exact(aabbs[1].min_y, 0.0);
    assert_exact(aabbs[1].max_y, 1.5);
    assert_exact(aabbs[2].min_y, 2.5);
    assert_exact(aabbs[2].max_y, 4.0);
}

#[test]
fn test_estimate_geometry_scales_with_rooms_not_area() {
    // A 100x100 m room must stay a bounded number of floor/ceiling quads,
    // and a 400x400 m room must not cost any more: the baked-lighting grid
    // is capped per axis, so geometry never scales with floor area.
    let json = r#"{
        "format_version": 1,
        "id": "big",
        "name": "Big",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let estimate = level.estimate_geometry();
    let cap =
        u64::from(crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS);
    assert!(
        estimate.floor_quads <= cap,
        "floor geometry must stay bounded, got {} quads",
        estimate.floor_quads
    );
    assert!(estimate.floor_quads > 1, "a large room is subdivided");
    assert_eq!(estimate.ceiling_quads, estimate.floor_quads);
    assert_eq!(estimate.floor_area_m2, 10_000);

    let huge = r#"{
        "format_version": 1,
        "id": "huge",
        "name": "Huge",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 400.0, "depth": 400.0, "height": 3.5 }]
    }"#;
    let huge = LevelDef::from_json(huge).expect("valid json");
    assert_eq!(huge.estimate_geometry().floor_quads, estimate.floor_quads);

    // A small room that needs no lighting resolution stays a single quad.
    let small = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "small",
            "name": "Small",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 2.0, "depth": 2.0 }]
        }"#,
    )
    .expect("valid json");
    let small = small.estimate_geometry();
    assert_eq!(small.floor_quads, 1);
    assert_eq!(small.ceiling_quads, 1);
}

#[test]
fn test_estimate_geometry_saturates_on_extreme_input() {
    // Direct construction with absurd dimensions must not overflow or panic.
    let level = LevelDef {
        format_version: 1,
        id: "extreme".into(),
        name: "Extreme".into(),
        author: String::new(),
        room: None,
        rooms: vec![RoomDef {
            x: f32::MAX,
            z: -f32::MAX,
            width: 1.0e30,
            depth: 1.0e30,
            height: 3.5,
            floor_y: 0.0,
            ceiling: CeilingProfileDef::Flat,
            material: None,
            ceiling_material: None,
        }],
        spawn: SpawnDef {
            x: 0.0,
            z: 0.0,
            yaw_degrees: 0.0,
        },
        defaults: LevelDefaults::default(),
        walls: Vec::new(),
        floor_patches: Vec::new(),
        floor_regions: Vec::new(),
        decals: Vec::new(),
        ceiling_lights: Vec::new(),
        props: Vec::new(),
    };
    let estimate = level.estimate_geometry();
    // Values are clamped before multiplication, so no wrap-around occurs and
    // the absurd area is still reported as over budget.
    assert!(estimate.floor_area_m2 >= MAX_LEVEL_FLOOR_AREA_M2);
    assert!(estimate.floor_quads >= 1);
    assert!(estimate.total_vertices < MAX_LEVEL_VERTICES);
}

fn wall_with_openings(openings_json: &str) -> WallDef {
    let json = format!(
        r#"{{
            "x": 0.0, "y": 0.0, "z": 0.0,
            "width": 4.0, "depth": 0.4, "height": 3.5,
            "openings": {openings_json}
        }}"#
    );
    serde_json::from_str(&json).expect("valid wall json")
}

#[test]
fn test_parse_wall_with_doorway_opening() {
    let wall = wall_with_openings(r#"[{ "offset": 1.0, "width": 1.0, "height": 2.1 }]"#);
    assert_eq!(wall.openings.len(), 1);
    let door = &wall.openings[0];
    // `kind` defaults to "door" and `sill` to a walk-through doorway.
    assert_eq!(door.kind, "door");
    assert_exact(door.sill, 0.0);
    assert!(door.is_door());
    assert!(door.reaches_floor());
    assert_exact(door.end(), 2.0);
    assert_exact(door.bottom(0.0), 0.0);
    assert_exact(door.top(0.0), 2.1);
    assert_exact(door.bottom(1.0), 1.0);
}

#[test]
fn test_wall_axis_and_length_helpers() {
    let x_wall = wall_with_openings("[]");
    assert_eq!(x_wall.axis(), WallAxis::X);
    assert_exact(x_wall.length(), 4.0);
    assert_exact(x_wall.thickness(), 0.4);
    assert_eq!(x_wall.min_corner(), (0.0, 0.0));
    assert_eq!(x_wall.length_origin(), (0.0, 0.0));

    let json = r#"{
        "x": 5.0, "z": -3.0, "width": 0.4, "depth": 6.0, "height": 3.5
    }"#;
    let z_wall: WallDef = serde_json::from_str(json).expect("valid z wall");
    assert_eq!(z_wall.axis(), WallAxis::Z);
    assert_exact(z_wall.length(), 6.0);
    assert_exact(z_wall.thickness(), 0.4);
    assert_eq!(z_wall.length_origin(), (5.0, -3.0));

    // Negative dimensions still expose a positive length from the min corner.
    let negative: WallDef =
        serde_json::from_str(r#"{ "x": 4.0, "z": 1.0, "width": -4.0, "depth": -0.4 }"#)
            .expect("valid negative wall");
    assert_eq!(negative.axis(), WallAxis::X);
    assert_exact(negative.length(), 4.0);
    assert_eq!(negative.min_corner(), (0.0, 0.6));
}

#[test]
fn test_wall_solid_slices_without_openings_is_one_full_slice() {
    let wall = wall_with_openings("[]");
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(
        slices,
        vec![WallSlice {
            start: 0.0,
            end: 4.0,
            bottom: 0.0,
            top: 3.5,
        }]
    );
}

#[test]
fn test_wall_solid_slices_with_doorway() {
    let wall =
        wall_with_openings(r#"[{ "kind": "door", "offset": 1.0, "width": 1.0, "height": 2.1 }]"#);
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(slices.len(), 3);
    // Left jamb, door header, right jamb.
    assert_exact(slices[0].start, 0.0);
    assert_exact(slices[0].end, 1.0);
    assert_eq!((slices[0].bottom, slices[0].top), (0.0, 3.5));
    assert_exact(slices[1].start, 1.0);
    assert_exact(slices[1].end, 2.0);
    assert_eq!((slices[1].bottom, slices[1].top), (2.1, 3.5));
    assert_exact(slices[2].start, 2.0);
    assert_exact(slices[2].end, 4.0);
    assert_eq!((slices[2].bottom, slices[2].top), (0.0, 3.5));
}

#[test]
fn test_wall_solid_slices_with_window_above_floor() {
    // A window spanning the whole wall leaves only a sill and a header.
    let json = r#"{
        "x": 0.0, "z": 0.0, "width": 3.0, "depth": 0.4, "height": 3.5,
        "openings": [{ "kind": "window", "offset": 0.0, "width": 3.0, "height": 1.2, "sill": 1.0 }]
    }"#;
    let wall: WallDef = serde_json::from_str(json).expect("valid wall");
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(slices.len(), 2);
    assert_eq!((slices[0].bottom, slices[0].top), (0.0, 1.0));
    assert_eq!((slices[1].bottom, slices[1].top), (2.2, 3.5));
    assert!(!wall.openings[0].reaches_floor());
}

#[test]
fn test_wall_solid_slices_with_two_openings() {
    let wall = wall_with_openings(
        r#"[
            { "kind": "door", "offset": 1.0, "width": 1.0, "height": 2.1 },
            { "kind": "window", "offset": 2.5, "width": 1.0, "height": 1.0, "sill": 1.0 }
        ]"#,
    );
    let slices = wall_solid_slices(&wall, 3.5);
    // [0,1] full, [1,2] header, [2,2.5] full, [2.5,3.5] sill+header, [3.5,4] full.
    assert_eq!(slices.len(), 6);
    let sill = slices
        .iter()
        .find(|s| (s.start - 2.5).abs() < 1e-4 && s.top <= 1.0 + 1e-4)
        .expect("window sill slice");
    assert_eq!((sill.bottom, sill.top), (0.0, 1.0));
    let header = slices
        .iter()
        .find(|s| (s.start - 2.5).abs() < 1e-4 && s.bottom >= 2.0 - 1e-4)
        .expect("window header slice");
    assert_eq!((header.bottom, header.top), (2.0, 3.5));
    // Slices are ordered by start.
    assert!(slices.windows(2).all(|w| w[0].start <= w[1].start));
}

#[test]
fn test_wall_solid_slices_with_opening_flush_to_wall_end() {
    let wall = wall_with_openings(
        r#"[{ "kind": "passage", "offset": 0.0, "width": 1.0, "height": 2.1 }]"#,
    );
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(slices.len(), 2);
    assert_eq!((slices[0].start, slices[0].end), (0.0, 1.0));
    assert_eq!((slices[0].bottom, slices[0].top), (2.1, 3.5));
    assert_eq!((slices[1].start, slices[1].end), (1.0, 4.0));
    assert_eq!((slices[1].bottom, slices[1].top), (0.0, 3.5));
    assert!(wall.openings[0].is_door());
}

#[test]
fn test_wall_solid_slices_ignores_out_of_range_openings() {
    // Beyond the wall end, NaN values, a zero width and a sill above the
    // wall top must all be ignored without panicking.
    let wall = wall_with_openings(
        r#"[
            { "offset": 10.0, "width": 1.0, "height": 2.1 },
            { "offset": 0.0, "width": 0.0, "height": 2.1 },
            { "offset": 0.5, "width": 1.0, "height": 2.1, "sill": 100.0 }
        ]"#,
    );
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(
        slices,
        vec![WallSlice {
            start: 0.0,
            end: 4.0,
            bottom: 0.0,
            top: 3.5,
        }]
    );

    let nan_wall =
        wall_with_openings(r#"[{ "offset": 1.0, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#);
    let mut nan_wall = nan_wall;
    nan_wall.openings[0].offset = f32::NAN;
    assert_eq!(wall_solid_slices(&nan_wall, 3.5).len(), 1);

    // A wall with no length produces no slices at all.
    let mut empty = nan_wall.clone();
    empty.openings.clear();
    empty.width = 0.0;
    empty.depth = 0.0;
    assert!(wall_solid_slices(&empty, 3.5).is_empty());
}

#[test]
fn test_wall_solid_slices_clamps_oversized_opening() {
    // An opening larger than the wall removes it entirely from collision.
    let wall = wall_with_openings(r#"[{ "offset": -1.0, "width": 10.0, "height": 10.0 }]"#);
    assert!(wall_solid_slices(&wall, 3.5).is_empty());
}

#[test]
fn test_wall_solid_slices_z_axis_wall() {
    // A wall whose length runs along Z uses depth as its length.
    let json = r#"{
        "x": 4.8, "z": 0.0, "width": 0.4, "depth": 6.0, "height": 3.5,
        "openings": [{ "kind": "door", "offset": 2.0, "width": 1.0, "height": 2.1 }]
    }"#;
    let wall: WallDef = serde_json::from_str(json).expect("valid z wall");
    let slices = wall_solid_slices(&wall, 3.5);
    assert_eq!(slices.len(), 3);
    assert_eq!((slices[0].start, slices[0].end), (0.0, 2.0));
    assert_eq!((slices[0].bottom, slices[0].top), (0.0, 3.5));
    assert_eq!((slices[1].start, slices[1].end), (2.0, 3.0));
    assert_eq!((slices[1].bottom, slices[1].top), (2.1, 3.5));
    assert_eq!((slices[2].start, slices[2].end), (3.0, 6.0));
}

#[test]
fn test_estimate_geometry_accounts_for_openings_and_props() {
    let json = r#"{
        "format_version": 1,
        "id": "estimate",
        "name": "Estimate",
        "spawn": { "x": 0.0, "z": 0.0 },
        "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
        "walls": [{
            "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.4,
            "openings": [{ "offset": 1.0, "width": 1.0, "height": 2.1 }]
        }],
        "props": [{ "model": "core:crate", "x": 1.0, "z": 1.0 }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let estimate = level.estimate_geometry();
    assert_eq!(estimate.prop_quads, MAX_PROP_QUADS);
    // The wall estimate follows the real solid slices: three slices (left
    // jamb, door header, right jamb), each one segment long, plus the
    // boundary reveals. It must bound what the builder emits.
    assert!(
        estimate.wall_quads >= 6,
        "a wall with one door must account for its slices and reveals, got {}",
        estimate.wall_quads
    );
    assert!(estimate.wall_quads <= 64, "estimate unexpectedly loose");
    // The estimate must bound the geometry that is actually generated.
    let mesh = crate::render::build_level_geometry(&level);
    assert!(
        u64::try_from(mesh.batches.wall_batch.count.max(0)).unwrap_or(0) <= estimate.wall_quads * 6
    );
    let expected_quads = estimate.floor_quads
        + estimate.ceiling_quads
        + estimate.wall_quads
        + estimate.light_quads
        + estimate.prop_quads
        + estimate.decal_quads;
    assert_eq!(estimate.total_vertices, expected_quads * 6);
}

#[test]
fn test_collision_aabbs_for_z_axis_wall_follow_depth() {
    // A wall running along Z: the slice spans must follow depth, not width.
    let json = r#"{
        "format_version": 1,
        "id": "z_wall",
        "name": "Z Wall",
        "spawn": { "x": 5.0, "z": 5.0 },
        "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
        "walls": [{
            "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.5,
            "openings": [{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]
        }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 3);

    // The door header only spans the door's Z range, full thickness in X.
    let header = aabbs
        .iter()
        .find(|a| a.min_y > 2.0)
        .expect("door header slice");
    assert!((header.min_x - 4.8).abs() < 1e-4);
    assert!((header.max_x - 5.2).abs() < 1e-4);
    assert_eq!((header.min_z, header.max_z), (4.0, 6.0));
    assert!(!header.intersects_player_y(0.0));
}

#[test]
fn test_collision_aabbs_include_solid_props_only() {
    let json = r#"{
        "format_version": 1,
        "id": "props",
        "name": "Props",
        "spawn": { "x": 0.0, "z": 0.0 },
        "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
        "props": [
            { "model": "core:crate", "x": 2.0, "y": 0.0, "z": 3.0, "size": [1.0, 1.0, 1.0], "solid": true },
            { "model": "core:rug", "x": 5.0, "z": 5.0, "solid": false }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 1);
    assert_exact(aabbs[0].min_x, 1.5);
    assert_exact(aabbs[0].max_x, 2.5);
    assert_exact(aabbs[0].min_y, 0.0);
    assert_exact(aabbs[0].max_y, 1.0);
    assert_exact(aabbs[0].min_z, 2.5);
    assert_exact(aabbs[0].max_z, 3.5);
}

// ------------------------------------------------- vertical geometry (4.0)

#[test]
fn test_legacy_room_gets_zero_elevation_flat_ceiling_and_the_new_default_height() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "legacy",
            "name": "Legacy",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": -6.0, "z": -6.0, "width": 12.0, "depth": 12.0 }
        }"#,
    )
    .expect("legacy json");
    let room = level.room_iter().next().expect("one room");
    assert_exact(room.height, DEFAULT_CEILING_HEIGHT_M);
    assert_exact(room.floor_y, 0.0);
    assert_eq!(room.ceiling, CeilingProfileDef::Flat);
    assert_exact(room.eave_y(), DEFAULT_CEILING_HEIGHT_M);
    assert_exact(room.ceiling_y_at(0.0, 0.0), DEFAULT_CEILING_HEIGHT_M);
}

#[test]
fn test_room_elevation_moves_floor_and_ceiling_together() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "raised",
            "name": "Raised",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                  "height": 3.0, "floor_y": 2.0 },
                { "x": 20.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                  "height": 3.0, "floor_y": -1.0 }
            ]
        }"#,
    )
    .expect("vertical json");
    let surfaces = LevelSurfaces::new(&level);
    assert_eq!(surfaces.floor_y_at(5.0, 5.0), Some(2.0));
    assert_exact(surfaces.ceiling_y_at(5.0, 5.0), 5.0);
    assert_eq!(surfaces.floor_y_at(25.0, 5.0), Some(-1.0));
    assert_exact(surfaces.ceiling_y_at(25.0, 5.0), 2.0);
    // Outside every room the historical first-room fallback still applies.
    assert_eq!(surfaces.floor_y_at(100.0, 100.0), None);
    assert_exact(surfaces.ceiling_y_at(100.0, 100.0), 5.0);
}

#[test]
fn test_gable_ceiling_interpolates_eave_to_ridge_on_both_axes() {
    let ridge_x = CeilingProfileDef::Gable {
        ridge: WallAxis::X,
        ridge_rise: 2.0,
    };
    let bounds = (0.0, 10.0, 0.0, 8.0);
    // Ridge runs along X, so the ceiling slopes across Z.
    assert_exact(
        ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 0.0),
        3.0,
    );
    assert_exact(
        ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 4.0),
        5.0,
    );
    assert_exact(
        ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 2.0),
        4.0,
    );
    assert_exact(
        ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 0.0, 8.0),
        3.0,
    );

    let ridge_z = CeilingProfileDef::Gable {
        ridge: WallAxis::Z,
        ridge_rise: 1.5,
    };
    // Ridge runs along Z, so the ceiling slopes across X.
    assert_exact(
        ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 0.0, 4.0),
        4.0,
    );
    assert_exact(
        ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 5.0, 4.0),
        5.5,
    );
    assert_exact(
        ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 2.5, 4.0),
        4.75,
    );
}

#[test]
fn test_malformed_gable_profiles_degrade_to_the_eave_plane() {
    let bounds = (0.0, 10.0, 0.0, 8.0);
    for rise in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let profile = CeilingProfileDef::Gable {
            ridge: WallAxis::X,
            ridge_rise: rise,
        };
        assert_exact(
            ceiling_y_for_volume(bounds, 0.0, 3.0, profile, 5.0, 4.0),
            3.0,
        );
    }
    // A degenerate footprint has no ridge to interpolate towards.
    assert_exact(
        ceiling_y_for_volume(
            (0.0, 10.0, 4.0, 4.0),
            0.0,
            3.0,
            CeilingProfileDef::Gable {
                ridge: WallAxis::X,
                ridge_rise: 2.0,
            },
            5.0,
            4.0,
        ),
        3.0,
    );
}

#[test]
fn test_gable_room_helpers_report_the_ridge() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 5.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0,
                      "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } }
        }"#,
    )
    .expect("gable json");
    let room = level.room_iter().next().expect("one room");
    assert_eq!(room.ceiling.ridge_axis(), Some(WallAxis::X));
    assert_exact(room.ridge_across().expect("ridge line"), 4.0);
    assert_exact(room.ridge_y().expect("ridge height"), 5.0);
    assert_exact(room.ceiling_y_at(5.0, 4.0), 5.0);
}

#[test]
fn test_floor_region_offsets_resolve_inside_outside_and_last_wins() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "regions",
            "name": "Regions",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0 },
                { "x": 3.0, "z": 3.0, "width": 1.0, "depth": 1.0, "offset_y": -2.0 }
            ]
        }"#,
    )
    .expect("region json");
    let surfaces = LevelSurfaces::new(&level);
    assert_eq!(surfaces.floor_y_at(0.5, 0.5), Some(0.0));
    assert_eq!(surfaces.floor_y_at(2.5, 2.5), Some(-1.0));
    // Overlapping regions: the later authored one wins.
    assert_eq!(surfaces.floor_y_at(3.5, 3.5), Some(-2.0));
    assert_eq!(
        surfaces
            .region_at(3.5, 3.5)
            .and_then(|r| r.material.as_deref()),
        None
    );
}

#[test]
fn test_floor_grid_cuts_at_region_edges_and_carries_offsets() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "grid",
            "name": "Grid",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5 }
            ]
        }"#,
    )
    .expect("grid json");
    let surfaces = LevelSurfaces::new(&level);
    let room = level.room_iter().next().expect("one room");
    let grid = surfaces.floor_grid(room);
    assert!(grid.xs.contains(&2.0), "region edge is a cut line");
    assert!(grid.xs.contains(&4.0));
    for iz in 0..grid.cells_z() {
        for ix in 0..grid.cells_x() {
            let x = f32::midpoint(grid.xs[ix], grid.xs[ix + 1]);
            let z = f32::midpoint(grid.zs[iz], grid.zs[iz + 1]);
            let inside = (2.0..4.0).contains(&x) && (2.0..4.0).contains(&z);
            let expected = if inside { -0.5 } else { 0.0 };
            assert_exact(grid.offset_at(ix, iz), expected);
        }
    }
}

#[test]
fn test_floor_region_rims_are_solid_only_for_unwalkable_steps() {
    let deep = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "deep",
            "name": "Deep",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -1.2 }
            ]
        }"#,
    )
    .expect("deep json");
    let deep_surfaces = LevelSurfaces::new(&deep);
    let room = deep.room_iter().next().expect("one room");
    let mut rims = Vec::new();
    deep_surfaces
        .floor_grid(room)
        .push_region_rims(room, &mut rims);
    assert!(!rims.is_empty(), "a 1.2 m recess needs solid walls");
    for rim in &rims {
        assert_exact(rim.min_y, -1.2);
        assert_exact(rim.max_y, 0.0);
    }

    // A step the controller can walk is deliberately not a wall.
    let shallow = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "shallow",
            "name": "Shallow",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.3 }
            ]
        }"#,
    )
    .expect("shallow json");
    let shallow_surfaces = LevelSurfaces::new(&shallow);
    let room = shallow.room_iter().next().expect("one room");
    let mut rims = Vec::new();
    shallow_surfaces
        .floor_grid(room)
        .push_region_rims(room, &mut rims);
    assert!(rims.is_empty(), "a walkable step must stay walkable");
}

#[test]
fn test_walkable_floor_matches_the_surface_queries() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "walkable",
            "name": "Walkable",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "height": 4.0, "floor_y": 2.0 },
                { "x": 8.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "height": 3.0, "floor_y": 0.0 }
            ],
            "floor_regions": [
                { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "offset_y": -0.35 }
            ]
        }"#,
    )
    .expect("walkable json");
    let surfaces = LevelSurfaces::new(&level);
    let floor = WalkableFloor::from_level(&level);
    for x in [-1.0_f32, 1.5, 3.0, 7.9, 8.5, 12.0, 17.0] {
        for z in [-1.0_f32, 1.5, 5.0, 7.9, 12.0] {
            assert_eq!(
                floor.height_at(x, z),
                surfaces.floor_y_at(x, z),
                "walkable floor disagrees at ({x}, {z})"
            );
        }
    }
    assert_eq!(floor.height_at(1.5, 1.5), Some(1.65));
    assert_eq!(floor.height_at(4.0, 4.0), Some(2.0));
    assert_eq!(floor.height_at(9.0, 4.0), Some(0.0));
    assert_eq!(floor.height_at(-5.0, -5.0), None);
}

#[test]
fn test_estimate_geometry_accounts_for_regions_and_gables() {
    let plain = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "plain",
            "name": "Plain",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }
        }"#,
    )
    .expect("plain json");
    let plain_estimate = plain.estimate_geometry();

    let complex = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "complex",
            "name": "Complex",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
            "floor_regions": [
                { "x": 4.0, "z": 4.0, "width": 4.0, "depth": 3.0, "offset_y": -1.5 }
            ]
        }"#,
    )
    .expect("complex json");
    let complex_estimate = complex.estimate_geometry();

    assert!(
        complex_estimate.floor_quads > plain_estimate.floor_quads,
        "a recess adds transition faces and cut lines"
    );
    assert!(
        complex_estimate.ceiling_quads >= plain_estimate.ceiling_quads,
        "the ridge cut line never lowers the ceiling cell count"
    );
    // The estimate remains an upper bound on what a build emits.
    let mesh = crate::render::build_level_geometry(&complex);
    assert!(
        u64::try_from(mesh.vertex_count).unwrap_or(u64::MAX) <= complex_estimate.total_vertices
    );
}

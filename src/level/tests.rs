//! Unit tests for the level schema, surfaces and walkable floor.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]

use super::*;
use crate::test_support::{assert_exact, assert_exact_named};

#[test]
fn test_parse_single_room_level() {
    let json = r#"{
        "format_version": 3,
        "id": "test_room",
        "name": "Test Room",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": -6.0, "z": -12.0, "width": 12.0, "depth": 16.0, "height": 3.5 } ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let rooms: Vec<&RoomDef> = level.room_iter().collect();
    assert_eq!(rooms.len(), 1);
    assert_exact(rooms[0].width, 12.0);
    assert_exact(rooms[0].height, 3.5);
}

#[test]
fn test_parse_sky_defaults_brightness_and_ambient() {
    let json = r#"{
        "format_version": 3,
        "id": "sky_level",
        "name": "Sky Level",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 } ],
        "sky": { "texture": "outdoor:tex_sky_stars_01" }
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let sky = level.sky.expect("sky parses");
    assert_eq!(sky.texture, "outdoor:tex_sky_stars_01");
    assert_exact(sky.brightness, 1.0);
    assert_exact(sky.ambient, 0.0);

    let json = r#"{
        "format_version": 3,
        "id": "sky_tuned",
        "name": "Sky Tuned",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 } ],
        "sky": { "texture": "outdoor:tex_sky_stars_01", "brightness": 0.5, "ambient": 0.08 }
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let sky = level.sky.expect("sky parses");
    assert_exact(sky.brightness, 0.5);
    assert_exact(sky.ambient, 0.08);
}

#[test]
fn test_parse_multi_room_level_with_walls() {
    let json = r#"{
        "format_version": 3,
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
        "format_version": 3,
        "id": "variable_walls",
        "name": "Variable Walls Test",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 } ],
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
        "format_version": 3,
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
        "format_version": 3,
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
            "format_version": 3,
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
        sky: None,
        doors: Vec::new(),
        effects: Vec::new(),
        routes: Vec::new(),
        format_version: LEVEL_FORMAT_VERSION,
        id: "extreme".into(),
        name: "Extreme".into(),
        author: String::new(),
        rooms: vec![RoomDef {
            x: f32::MAX,
            z: -f32::MAX,
            width: 1.0e30,
            depth: 1.0e30,
            height: 3.5,
            floor_y: 0.0,
            ceiling: CeilingProfileDef::Flat,
            material: None,
            shine: None,
            ceiling_material: None,
            ceiling_shine: None,
            ceiling_tile_origin: None,
            ceiling_tile_rotation_degrees: None,
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
        water: Vec::new(),
        ladders: Vec::new(),
        ramps: Vec::new(),
        stairs: Vec::new(),
        half_walls: Vec::new(),
        columns: Vec::new(),
        archways: Vec::new(),
        guardrails: Vec::new(),
        thresholds: Vec::new(),
        baseboards: Vec::new(),
        decals: Vec::new(),
        ceiling_lights: Vec::new(),
        props: Vec::new(),
        volumes: Vec::new(),
        timers: Vec::new(),
        sequences: Vec::new(),
        spawn_templates: Vec::new(),
        spawn_points: Vec::new(),
        spawn_groups: Vec::new(),
        animated_emissions: Vec::new(),
        arc_walls: Vec::new(),
        pillars: Vec::new(),
        void_walls: Vec::new(),
        geometry_intent: Vec::new(),
        fog_regions: Vec::new(),
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
fn test_a_door_resolves_the_wall_opening_it_fills() {
    // Wall at z 2.85..3.15 (0.3 m thick) with a passage x 3.0..4.4, and the
    // leaf hinged exactly on the passage's near edge and centred in the wall.
    let json = r#"{
        "format_version": 3,
        "id": "frame_resolve",
        "name": "Frame Resolve",
        "spawn": { "x": 3.0, "z": 4.0 },
        "rooms": [ { "x": 0.0, "z": 2.0, "width": 6.0, "depth": 4.0, "height": 2.7 } ],
        "walls": [ { "x": 0.0, "z": 2.85, "width": 6.0, "depth": 0.3,
                     "openings": [ { "kind": "passage", "offset": 3.0, "width": 1.4,
                                     "height": 2.1, "sill": 0.0 } ] } ],
        "doors": [
            { "id": "in_the_wall", "x": 3.0, "z": 3.0, "width": 1.4, "height": 2.1 },
            { "id": "off_the_wall", "x": 3.0, "z": 5.0, "width": 1.4, "height": 2.1 },
            { "id": "explicit", "x": 3.0, "z": 4.0, "width": 1.4, "height": 2.1,
              "frame_depth": 0.7, "frame_center": -0.2 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("level parses");
    let frame = level.door_frame(&level.doors[0]);
    assert_exact(frame.depth, 0.3);
    assert_exact(frame.center, 0.0);
    // A leaf no wall opening claims keeps the standalone liner.
    let standalone = level.door_frame(&level.doors[1]);
    assert!((standalone.depth - STANDALONE_DOOR_FRAME_DEPTH_M).abs() < 1e-6);
    assert_exact(standalone.center, 0.0);
    // Authored values win over the resolution.
    let authored = level.door_frame(&level.doors[2]);
    assert_exact(authored.depth, 0.7);
    assert_exact(authored.center, -0.2);
    let (lo, hi) = authored.span();
    assert!(
        (lo + 0.55).abs() < 1e-6 && (hi - 0.15).abs() < 1e-6,
        "{lo}..{hi}"
    );

    // A Z-axis wall resolves the same way, and a door whose leaf normal runs
    // along the wall's length still measures the tunnel across its thickness.
    let json = r#"{
        "format_version": 3,
        "id": "frame_resolve_z",
        "name": "Frame Resolve Z",
        "spawn": { "x": 3.0, "z": 3.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.7 } ],
        "walls": [ { "x": 2.85, "z": 0.0, "width": 0.3, "depth": 6.0,
                     "openings": [ { "kind": "door", "offset": 2.0, "width": 1.4,
                                     "height": 2.1, "sill": 0.0 } ] } ],
        "doors": [ { "id": "z_door", "x": 3.08, "z": 2.0, "rotation_degrees": 270.0,
                     "width": 1.4, "height": 2.1 } ]
    }"#;
    let level = LevelDef::from_json(json).expect("z level parses");
    let frame = level.door_frame(&level.doors[0]);
    assert!((frame.depth - 0.3).abs() < 1e-5, "{}", frame.depth);
    // The leaf sits 80 mm east of the wall centre and its normal points west.
    assert!((frame.center - 0.08).abs() < 1e-5, "{}", frame.center);

    // A raised doorway whose sill meets the leaf's own raised floor still
    // frames the leaf: the pool-side sauna door is the maintained example.
    let json = r#"{
        "format_version": 3,
        "id": "frame_resolve_sill",
        "name": "Frame Resolve Sill",
        "spawn": { "x": 3.0, "z": 3.0 },
        "rooms": [ { "x": 0.0, "z": 2.0, "width": 6.0, "depth": 4.0, "height": 2.7 } ],
        "walls": [ { "x": 0.0, "z": 2.85, "width": 6.0, "depth": 0.3, "y": -0.6,
                     "openings": [ { "kind": "door", "offset": 0.7, "width": 1.4,
                                     "height": 2.1, "sill": 0.6 } ] } ],
        "doors": [ { "id": "raised", "x": 0.7, "y": 0.6, "z": 3.0,
                     "width": 1.4, "height": 2.1 } ]
    }"#;
    let level = LevelDef::from_json(json).expect("raised level parses");
    let frame = level.door_frame(&level.doors[0]);
    assert!((frame.depth - 0.3).abs() < 1e-5, "{}", frame.depth);
    assert!(frame.center.abs() < 1e-5, "{}", frame.center);
    // A window that happens to span the leaf's height is never a frame.
    let json = r#"{
        "format_version": 3,
        "id": "frame_resolve_window",
        "name": "Frame Resolve Window",
        "spawn": { "x": 3.0, "z": 3.0 },
        "rooms": [ { "x": 0.0, "z": 2.0, "width": 6.0, "depth": 4.0, "height": 2.7 } ],
        "walls": [ { "x": 0.0, "z": 2.85, "width": 6.0, "depth": 0.3,
                     "openings": [ { "kind": "window", "offset": 0.7, "width": 1.4,
                                     "height": 2.1, "sill": 0.0 } ] } ],
        "doors": [ { "id": "in_front_of_a_window", "x": 0.7, "z": 3.0,
                     "width": 1.4, "height": 2.1 } ]
    }"#;
    let level = LevelDef::from_json(json).expect("window level parses");
    let frame = level.door_frame(&level.doors[0]);
    assert!(
        (frame.depth - STANDALONE_DOOR_FRAME_DEPTH_M).abs() < 1e-6,
        "{}",
        frame.depth
    );
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
        "format_version": 3,
        "id": "estimate",
        "name": "Estimate",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
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
        "format_version": 3,
        "id": "z_wall",
        "name": "Z Wall",
        "spawn": { "x": 5.0, "z": 5.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
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
        "format_version": 3,
        "id": "props",
        "name": "Props",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 } ],
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

// ------------------------------------------------------- vertical geometry

#[test]
fn test_a_room_without_elevation_gets_flat_ceiling_and_the_default_height() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "minimal_room",
            "name": "Minimal Room",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": -6.0, "z": -6.0, "width": 12.0, "depth": 12.0 } ]
        }"#,
    )
    .expect("minimal room json");
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
            "format_version": 3,
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
            "format_version": 3,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 5.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0,
                      "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } } ]
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
            "format_version": 3,
            "id": "regions",
            "name": "Regions",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 } ],
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
            "format_version": 3,
            "id": "grid",
            "name": "Grid",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 } ],
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
            "format_version": 3,
            "id": "deep",
            "name": "Deep",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 } ],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -1.2 }
            ]
        }"#,
    )
    .expect("deep json");
    let deep_surfaces = LevelSurfaces::new(&deep);
    let room = deep.room_iter().next().expect("one room");
    let mut rims = Vec::new();
    let deep_grid = deep_surfaces.floor_grid(room);
    deep_grid.push_region_rims(|x, z| deep_grid.height_at(room, x, z), &mut rims);
    assert!(!rims.is_empty(), "a 1.2 m recess needs solid walls");
    for rim in &rims {
        assert_exact(rim.min_y, -1.2);
        assert_exact(rim.max_y, 0.0);
    }

    // A step the controller can walk is deliberately not a wall.
    let shallow = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "shallow",
            "name": "Shallow",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 } ],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.3 }
            ]
        }"#,
    )
    .expect("shallow json");
    let shallow_surfaces = LevelSurfaces::new(&shallow);
    let room = shallow.room_iter().next().expect("one room");
    let mut rims = Vec::new();
    let shallow_grid = shallow_surfaces.floor_grid(room);
    shallow_grid.push_region_rims(|x, z| shallow_grid.height_at(room, x, z), &mut rims);
    assert!(rims.is_empty(), "a walkable step must stay walkable");
}

#[test]
fn test_walkable_floor_matches_the_surface_queries() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
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
            "format_version": 3,
            "id": "plain",
            "name": "Plain",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 } ]
        }"#,
    )
    .expect("plain json");
    let plain_estimate = plain.estimate_geometry();

    let complex = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "complex",
            "name": "Complex",
            "spawn": { "x": 5.0, "z": 5.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } } ],
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

// ------------------------------------------------------- surface shine

/// A level with a shine override on every surface carrier.
fn shine_level_json() -> &'static str {
    r#"{
        "format_version": 3,
        "id": "shine",
        "name": "Shine",
        "spawn": { "x": 4.0, "z": 4.0 },
        "defaults": { "wall": "core:wallpaper_yellow_01",
                      "floor": "core:carpet_beige_01",
                      "ceiling": "core:ceiling_panel_01",
                      "wall_shine": 0.0, "floor_shine": 0.1, "ceiling_shine": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                     "material": "core:linoleum_polished_01", "shine": 0.05,
                     "ceiling_material": "core:ceiling_panel_01",
                     "ceiling_shine": 0.0 } ],
        "walls": [
            { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3, "height": 3.0,
              "material": "core:metal_brushed_01", "shine": 0.3,
              "faces": { "south": "core:metal_brushed_01" },
              "face_shine": { "south": 0.6 },
              "openings": [
                  { "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0,
                    "sill": 1.0, "glass": "core:glass_window_clear_01",
                    "glass_shine": 0.85 }
              ] }
        ],
        "floor_patches": [
            { "x": 0.0, "z": 0.0, "width": 1.0, "depth": 1.0,
              "material": "core:carpet_damp_01", "shine": 0.0 }
        ],
        "floor_regions": [
            { "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5,
              "material": "core:pool_tile_basin_01", "shine": 0.3,
              "edge_material": "core:pool_tile_wall_01", "edge_shine": 0.28 }
        ]
    }"#
}

#[test]
fn every_authored_shine_survives_a_json_round_trip() {
    let level = LevelDef::from_json(shine_level_json()).expect("shine level parses");
    let rewritten = serde_json::to_string(&level).expect("level serialises");
    let again = LevelDef::from_json(&rewritten).expect("rewritten level parses");

    assert_eq!(again.defaults.floor_shine, Some(0.1));
    let room = &again.rooms[0];
    assert_eq!(room.shine, Some(0.05));
    assert_eq!(room.ceiling_shine, Some(0.0));
    let wall = &again.walls[0];
    assert_eq!(wall.shine, Some(0.3));
    assert_eq!(wall.face_shine.get("south"), Some(&0.6));
    assert_eq!(wall.openings[0].glass_shine, Some(0.85));
    assert_eq!(again.floor_patches[0].shine, Some(0.0));
    assert_eq!(again.floor_regions[0].shine, Some(0.3));
    assert_eq!(again.floor_regions[0].edge_shine, Some(0.28));
}

#[test]
fn a_level_without_shine_keeps_every_override_empty() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "plain", "name": "Plain",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ]
        }"#,
    )
    .expect("plain level parses");
    assert!(level.defaults.floor_shine.is_none());
    assert!(level.room_iter().all(|room| room.shine.is_none()));
    assert!(level.walls.is_empty());
}

#[test]
fn material_references_carry_the_surface_override() {
    let level = LevelDef::from_json(shine_level_json()).expect("shine level parses");
    let room = &level.rooms[0];
    let floor = room.floor_ref().expect("the room overrides its floor");
    assert_eq!(floor.id, "core:linoleum_polished_01");
    assert_eq!(floor.shine, Some(0.05));
    assert!(floor.has_shine());

    // The default is a reference too, with the level-wide override.
    let default_floor = level.defaults.floor_ref();
    assert_eq!(default_floor.shine, Some(0.1));
    assert_eq!(level.defaults.wall_ref().shine, Some(0.0));

    // A pane carries its own override.
    let glass = level.walls[0].openings[0]
        .glass_ref()
        .expect("the opening is glazed");
    assert_eq!(glass.id, "core:glass_window_clear_01");
    assert_eq!(glass.shine, Some(0.85));

    // A patch and a region resolve theirs.
    assert_eq!(level.floor_patches[0].material_ref().shine, Some(0.0));
    let region = &level.floor_regions[0];
    assert_eq!(region.floor_ref().expect("region floor").shine, Some(0.3));
    assert_eq!(region.edge_ref().expect("region edge").shine, Some(0.28));
}

#[test]
fn wall_face_shine_resolves_face_then_wall_then_material() {
    let level = LevelDef::from_json(shine_level_json()).expect("shine level parses");
    let wall = &level.walls[0];
    // The face's own override wins.
    assert_eq!(wall.face_ref("south").expect("south face").shine, Some(0.6));
    // A face that only falls back to the wall's material keeps the wall shine.
    assert_eq!(wall.face_ref("north").expect("north face").shine, Some(0.3));
    // The wall's own reference carries it too.
    assert_eq!(wall.material_ref().expect("wall material").shine, Some(0.3));

    // A face with a different material does not inherit the wall's override:
    // its own material default applies.
    let json = r#"{
        "format_version": 3, "id": "faces", "name": "Faces",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ],
        "walls": [
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.3, "height": 3.0,
              "material": "core:metal_brushed_01", "shine": 0.6,
              "faces": { "north": "core:wallpaper_yellow_01" } }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("faces level parses");
    let wall = &level.walls[0];
    assert_eq!(
        wall.face_ref("north").expect("north").id,
        "core:wallpaper_yellow_01"
    );
    assert_eq!(
        wall.face_ref("north").expect("north").shine,
        None,
        "a face with its own material keeps that material's default"
    );
    assert_eq!(
        wall.face_ref("south").expect("south").shine,
        Some(0.6),
        "a face falling back to the wall material keeps the wall override"
    );
}

// ------------------------------------------------- generic architecture

/// A ramp's surface is a straight line between its ends, and the sign of `rise`
/// decides which end is high.
#[test]
fn test_ramp_offset_is_linear_and_signed() {
    let json = r#"{
        "format_version": 3,
        "id": "ramps",
        "name": "Ramps",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 4.0 } ],
        "ramps": [
            { "x": 1.0, "z": 1.0, "width": 1.0, "depth": 4.0, "rise": 1.0 },
            { "x": 5.0, "z": 1.0, "width": 1.0, "depth": 4.0, "offset_y": 0.5, "rise": -0.5 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("ramp json");
    let rise = &level.ramps[0];
    // The run is the longer axis (Z), and the rise climbs toward its far end.
    assert_eq!(rise.axis(), WallAxis::Z);
    assert_exact(rise.length(), 4.0);
    assert_exact(rise.offset_at(1.5, 1.0), 0.0);
    assert_exact(rise.offset_at(1.5, 3.0), 0.5);
    assert_exact(rise.offset_at(1.5, 5.0), 1.0);
    // Outside the footprint the fraction clamps, so the query never shoots past
    // the ends; containment is what decides whether a point is on the ramp.
    assert_exact(rise.offset_at(1.5, 0.0), 0.0);
    assert!(!rise.contains(1.5, 0.0));
    assert!(rise.contains(1.5, 3.0));
    assert_exact(rise.low_offset(), 0.0);
    assert_exact(rise.high_offset(), 1.0);
    let (high_x, high_z) = rise.end_point(true);
    assert_exact(high_x, 1.5);
    assert_exact(high_z, 5.0);
    // A negative rise descends toward the far end from the authored offset.
    let descend = &level.ramps[1];
    assert_exact(descend.offset_at(6.0, 1.0), 0.5);
    assert_exact(descend.offset_at(6.0, 5.0), 0.0);
}

/// A staircase climbs one riser per tread and steps out at the top; every step
/// is within the player's walkable step.
#[test]
fn test_staircase_offset_steps_over_its_risers() {
    let json = r#"{
        "format_version": 3,
        "id": "stairs",
        "name": "Stairs",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 4.0 } ],
        "stairs": [
            { "x": 1.0, "z": 1.0, "width": 1.0, "depth": 2.0, "rise": 0.8, "steps": 4 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("stair json");
    let stair = &level.stairs[0];
    assert_eq!(stair.axis(), WallAxis::Z);
    assert_exact(stair.riser_height(), 0.2);
    assert_exact(stair.tread_depth(), 0.5);
    assert_exact(stair.offset_at(1.5, 1.0), 0.2);
    assert_exact(stair.offset_at(1.5, 1.49), 0.2);
    assert_exact(stair.offset_at(1.5, 1.5), 0.4);
    assert_exact(stair.offset_at(1.5, 2.99), 0.8);
    assert_exact(stair.top_offset(), 0.8);
    assert!(stair.riser_height() <= PLAYER_STEP_HEIGHT + 1e-6);
}

/// The walkable floor answers with the ramp's slope and the staircase's steps,
/// and agrees with [`LevelSurfaces::floor_y_at`] everywhere.
#[test]
fn test_walkable_floor_follows_ramps_and_stairs() {
    let json = r#"{
        "format_version": 3,
        "id": "walkable_architecture",
        "name": "Walkable Architecture",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 }
        ],
        "ramps": [
            { "x": 1.0, "z": 1.0, "width": 1.0, "depth": 4.0, "rise": 1.0 }
        ],
        "stairs": [
            { "x": 5.0, "z": 1.0, "width": 1.0, "depth": 2.0, "rise": 0.8, "steps": 4 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("architecture json");
    let surfaces = LevelSurfaces::new(&level);
    let floor = WalkableFloor::from_level(&level);
    for (x, z) in [
        (1.5f32, 1.0f32),
        (1.5, 2.5),
        (1.5, 5.0),
        (5.5, 1.0),
        (5.5, 1.75),
        (5.5, 2.99),
        (9.0, 9.0),
    ] {
        assert_exact_named(
            floor.height_at(x, z).unwrap_or(f32::NAN),
            surfaces.floor_y_at(x, z).unwrap_or(f32::NAN),
            format!("({x}, {z})"),
        );
    }
    assert_exact(floor.height_at(1.5, 2.5).unwrap_or(f32::NAN), 0.375);
    assert_exact(floor.height_at(5.5, 2.99).unwrap_or(f32::NAN), 0.8);
    // A ramp wins over a floor region that overlaps it, so the sloped surface
    // is the one underfoot.
    assert_exact(surfaces.walkable_offset_at(1.5, 4.0), 0.75);
}

/// The solid pieces become real boxes for collision and the lighting bake; the
/// walking surfaces and the trim are deliberately not solid.
#[test]
fn test_architecture_solids_cover_walls_piers_rails_and_not_trim() {
    let json = r#"{
        "format_version": 3,
        "id": "solids",
        "name": "Solids",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 14.0, "depth": 10.0, "height": 4.0 } ],
        "ramps": [
            { "x": 1.0, "z": 1.0, "width": 1.0, "depth": 2.0, "rise": 0.5 }
        ],
        "stairs": [
            { "x": 3.0, "z": 1.0, "width": 1.0, "depth": 2.0, "rise": 0.6, "steps": 3 }
        ],
        "half_walls": [
            { "x": 5.0, "z": 5.0, "width": 2.0, "depth": 0.2, "height": 1.05 }
        ],
        "columns": [
            { "x": 8.0, "z": 5.0, "width": 0.3, "depth": 0.3 }
        ],
        "archways": [
            { "x": 10.0, "z": 3.0, "width": 0.3, "depth": 1.4, "height": 3.0,
              "opening_width": 1.0, "opening_height": 2.1, "arch_rise": 0.25 }
        ],
        "guardrails": [
            { "x": 1.0, "z": 7.0, "length": 2.0, "rotation_degrees": 0.0, "height": 1.0 }
        ],
        "thresholds": [
            { "x": 2.0, "z": 9.0, "length": 1.0, "material": "core:carpet_beige_01" }
        ],
        "baseboards": [
            { "x": 0.0, "z": 0.0, "length": 3.0, "material": "core:wallpaper_yellow_01" }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("solids json");
    let solids = level.architecture_solids();
    // Two half-wall/column boxes, three archway boxes (two piers and the
    // spandrel) and one guardrail barrier.
    assert_eq!(solids.len(), 6, "{solids:?}");
    // Nothing on the walking surfaces or the trim.
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), solids.len(), "only the solid pieces collide");
    let column = solids
        .iter()
        .find(|solid| (solid.min[0] - 8.0).abs() < 1e-4)
        .expect("the column is a solid box");
    assert_exact(column.min[1], 0.0);
    assert_exact_named(
        column.max[1],
        4.0,
        "a column without a height reaches the ceiling",
    );
    let half_wall = solids
        .iter()
        .find(|solid| (solid.min[0] - 5.0).abs() < 1e-4)
        .expect("the half wall is a solid box");
    assert_exact(half_wall.max[1], 1.05);
}

#[test]
fn test_water_volumes_resolve_surface_bottom_and_the_dry_default() {
    let dry_level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "dry",
            "name": "Dry",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ]
        }"#,
    )
    .expect("dry level");
    assert!(
        dry_level.water.is_empty(),
        "a level with no water volumes is dry"
    );
    let dry = WaterVolumes::from_level(&dry_level);
    assert!(dry.is_empty());
    assert!(dry.sample(1.0, 1.0, -50.0).is_none());

    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "wet",
            "name": "Wet",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 },
                { "x": 0.0, "z": 8.0, "width": 8.0, "depth": 4.0, "height": 3.0, "floor_y": -2.0 }
            ],
            "floor_regions": [
                { "x": 1.0, "z": 8.0, "width": 6.0, "depth": 4.0, "offset_y": -1.0 }
            ],
            "water": [
                { "x": 1.0, "z": 8.0, "width": 6.0, "depth": 4.0, "surface_y": -1.75,
                  "material": "core:water_pool_01", "opacity": 0.4 }
            ]
        }"#,
    )
    .expect("water level");
    let volumes = WaterVolumes::from_level(&level);
    assert_eq!(volumes.len(), 1);
    let volume = &volumes.volumes()[0];
    assert_exact(volume.surface_y, -1.75);
    assert_exact(volume.bottom_y, -3.0);
    assert_exact(volume.depth(), 1.25);
    assert_exact(volume.opacity, 0.4);
    assert_eq!(volume.material_id(), "core:water_pool_01");
    assert!(volume.swimming);

    let sample = volumes.sample(4.0, 10.0, -2.5).expect("submerged point");
    assert_exact(sample.surface_y, -1.75);
    assert_exact(sample.bottom_y, -3.0);
    assert!(
        volumes.sample(4.0, 10.0, -1.0).is_none(),
        "above the surface"
    );
    assert!(
        volumes.sample(7.5, 10.0, -2.5).is_none(),
        "outside the footprint"
    );
    assert!(
        volumes.sample(4.0, 10.0, -1.75).is_some(),
        "the surface itself is water"
    );
}

/// A circular volume resolves as a disc: membership excludes the bounding box
/// corners a square would wrongly include, the bottom samples the circle's own
/// interior, and a swimmer's sample reports the round pool.
#[test]
fn test_circular_water_volumes_are_discs_not_bounding_boxes() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "round_pool",
            "name": "Round Pool",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "floor_regions": [
                { "x": 1.0, "z": 1.0, "width": 1.5, "depth": 1.5, "offset_y": -1.5 }
            ],
            "water": [
                { "shape": "circle", "x": 2.0, "z": 2.0, "radius": 2.0,
                  "surface_y": 0.5, "swimming": true }
            ]
        }"#,
    )
    .expect("circular water level");
    let volumes = WaterVolumes::from_level(&level);
    assert_eq!(volumes.len(), 1);
    let volume = &volumes.volumes()[0];
    assert_eq!(volume.shape, WaterShape::Circle);
    assert_exact(volume.radius, 2.0);
    assert_exact(volume.x0, 2.0);
    assert_exact(volume.x1, 6.0);
    assert_exact(volume.z0, 2.0);
    assert_exact(volume.z1, 6.0);
    assert_eq!(volume.center(), (4.0, 4.0));

    // A square would include these four bounding box corners; the circle must
    // not.
    for (x, z) in [(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)] {
        assert!(
            !volume.contains(x, z),
            "a bounding box corner is not water: ({x}, {z})"
        );
        assert!(
            volumes.sample(x, z, 0.0).is_none(),
            "the swimmer's sample misses the corner: ({x}, {z})"
        );
        assert!(
            volumes.surface_y_at(x, z).is_none(),
            "the surface query misses the corner: ({x}, {z})"
        );
    }
    // The centre and points strictly inside the disc are water; the rim
    // itself (the wall line) is dry, which is what keeps the four bounding box
    // axis extremes from reading as water.
    for (x, z) in [(4.0, 4.0), (5.9, 4.0), (4.0, 2.1), (5.4, 5.4)] {
        assert!(volume.contains(x, z), "inside the disc: ({x}, {z})");
    }
    for (x, z) in [(6.0, 4.0), (4.0, 2.0)] {
        assert!(
            !volume.contains(x, z),
            "the rim is the wall line and is dry: ({x}, {z})"
        );
    }
    let sample = volumes
        .sample(4.0, 4.0, 0.2)
        .expect("a swimming player's sample reports the circular volume");
    assert_exact(sample.surface_y, 0.5);
    assert!(sample.swimming);
    assert_exact(volumes.surface_y_at(5.4, 5.4).expect("surface"), 0.5);

    // The resolved bottom samples the disc's own interior. The only recessed
    // region covers the (1.5, 1.5) bounding box corner, which is outside the
    // circle: a corner-sampling implementation would resolve -1.0, the disc
    // resolves the room's own floor.
    assert_exact(volume.bottom_y, 0.0);

    // A floating disc fits the round pool; one that would poke past the rim
    // does not, however well it fits the bounding box.
    assert!(volumes.contains_disc(4.0, 4.0, 1.5));
    assert!(!volumes.contains_disc(5.4, 4.0, 0.8), "past the rim");
    assert!(
        volumes.contains_disc(4.0, 4.0, 2.0),
        "the exact inscribed disc fits"
    );
}

/// The first `fade` component of the level's first prop.
fn first_fade(level: &LevelDef) -> &FadeDef {
    let prop = level.props.first().expect("the level authors a prop");
    prop.components
        .iter()
        .find_map(|component| {
            if let ComponentDef::Fade(def) = component {
                Some(def)
            } else {
                None
            }
        })
        .expect("the prop authors a fade")
}

/// `fade` proximity fields: the radii activate the mode and the times resolve
/// their documented defaults.
#[test]
fn test_fade_proximity_fields_resolve_defaults() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "fade_proximity",
            "name": "Fade Proximity",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "props": [
                { "id": "ghost", "model": "entity:sheet-ghost", "x": 2.0, "z": 2.0,
                  "components": [ { "component": "fade", "near_radius": 3.0,
                                    "far_radius": 6.0 } ] }
            ]
        }"#,
    )
    .expect("the proximity fade parses");
    let fade = first_fade(&level);
    let proximity = fade.proximity().expect("both radii activate the mode");
    assert_exact(proximity.near_radius, 3.0);
    assert_exact(proximity.far_radius, 6.0);
    assert_exact(proximity.fade_out_seconds, DEFAULT_FADE_OUT_SECONDS);
    assert_exact(proximity.fade_in_seconds, DEFAULT_FADE_IN_SECONDS);
    assert_exact(fade.period_seconds, DEFAULT_FADE_PERIOD_SECONDS);
    assert!(
        fade.proximity()
            .is_some_and(|proximity| proximity.far_radius > proximity.near_radius),
        "the hysteresis band is positive"
    );

    // A partial contract is not a proximity fade.
    let partial = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "fade_partial",
            "name": "Fade Partial",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "props": [
                { "id": "ghost", "model": "entity:sheet-ghost", "x": 2.0, "z": 2.0,
                  "components": [ { "component": "fade", "near_radius": 3.0 } ] }
            ]
        }"#,
    )
    .expect("a partial contract still parses");
    assert!(
        first_fade(&partial).proximity().is_none(),
        "a lone near_radius is not a proximity fade"
    );
    // The loader rejects it by name; the parser alone is permissive.
    let error = crate::loader::validate_level(&partial).expect_err("partial contract");
    assert!(
        error.contains("far_radius") && error.contains("near_radius"),
        "the refusal names both radii: {error}"
    );
}

/// Every malformed proximity contract is refused by name.
#[test]
fn test_fade_proximity_rejects_malformed_contracts() {
    let cases = [
        (
            r#"{ "component": "fade", "near_radius": 6.0, "far_radius": 3.0 }"#,
            "far_radius",
            "an inverted band",
        ),
        (
            r#"{ "component": "fade", "near_radius": 0.0, "far_radius": 3.0 }"#,
            "near_radius",
            "a zero near radius",
        ),
        (
            r#"{ "component": "fade", "near_radius": 3.0, "far_radius": 6.0,
                "fade_out_seconds": 601.0 }"#,
            "fade_out_seconds",
            "a fade time above the cap",
        ),
        (
            r#"{ "component": "fade", "fade_in_seconds": 1.0 }"#,
            "fade_in_seconds",
            "a fade time without radii",
        ),
    ];
    for (component, needle, label) in cases {
        let level = LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "fade_case",
                "name": "Fade Case",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }} ],
                "props": [ {{ "id": "ghost", "model": "entity:sheet-ghost", "x": 2.0, "z": 2.0,
                              "components": [ {component} ] }} ]
            }}"#
        ))
        .expect("the case parses");
        let error = crate::loader::validate_level(&level).expect_err(label);
        assert!(error.contains(needle), "{label}: {error}");
    }
}

/// The round-pool/proximity-fade fixture validates end to end and resolves
/// the circular volume and the proximity fade the way the runtime does.
#[test]
fn test_round_pool_and_proximity_fade_fixture_validates() {
    let level = LevelDef::from_json(include_str!(
        "../../tests/fixtures/levels/round_pool_and_proximity_fade.json"
    ))
    .expect("the fixture parses");
    crate::loader::validate_level(&level).expect("the fixture validates");

    let volumes = WaterVolumes::from_level(&level);
    assert_eq!(volumes.len(), 1);
    let pool = &volumes.volumes()[0];
    assert_eq!(pool.shape, WaterShape::Circle);
    assert_eq!(pool.center(), (6.0, 6.0));
    assert_exact(pool.radius, 2.0);
    assert!(pool.contains(6.0, 6.0), "the centre is water");
    assert!(
        !pool.contains(4.0, 4.0),
        "a bounding box corner is dry deck"
    );
    assert!(
        volumes.contains_disc(6.0, 6.0, 0.1),
        "the floating duck's disc fits the round pool"
    );
    assert!(
        !volumes.contains_disc(7.9, 6.0, 0.2),
        "a float drifting to the rim would not fit"
    );

    let cat = level
        .props
        .iter()
        .find(|prop| prop.id.as_deref() == Some("proximity_cat"))
        .expect("the fixture authors the ghost cat");
    let fade = cat
        .components
        .iter()
        .find_map(|component| {
            if let ComponentDef::Fade(def) = component {
                Some(def)
            } else {
                None
            }
        })
        .expect("the cat authors a fade");
    let proximity = fade.proximity().expect("the cat authors a proximity fade");
    assert_exact(proximity.near_radius, 1.0);
    assert_exact(proximity.far_radius, 2.2);
    assert_exact(proximity.fade_out_seconds, 0.8);
    assert_exact(proximity.fade_in_seconds, 1.6);
}

/// Water volume shapes: the loader accepts the documented rectangle and
/// circle forms and refuses every malformed combination by name.
#[test]
fn test_water_shape_validation_names_bad_combinations() {
    let level = |water: &str| {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "water_validation",
                "name": "Water Validation",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }} ],
                "water": [ {water} ]
            }}"#
        ))
        .expect("the case parses")
    };

    // The documented forms validate.
    crate::loader::validate_level(&level(
        r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "surface_y": 0.5 }"#,
    ))
    .expect("a rectangle validates");
    crate::loader::validate_level(&level(
        r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "radius": 2.0, "surface_y": 0.5 }"#,
    ))
    .expect("a circle with no width/depth validates");
    crate::loader::validate_level(&level(
        r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "width": 4.0, "depth": 4.0,
            "radius": 2.0, "surface_y": 0.5 }"#,
    ))
    .expect("a circle may author its own bounding box verbatim");

    let cases = [
        (
            r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "surface_y": 0.5 }"#,
            "must author radius",
            "a circle without a radius",
        ),
        (
            r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "radius": 0.0, "surface_y": 0.5 }"#,
            "radius must be a finite number greater than 0",
            "a zero radius",
        ),
        (
            r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "radius": -2.0, "surface_y": 0.5 }"#,
            "radius must be a finite number greater than 0",
            "a negative radius",
        ),
        (
            r#"{ "shape": "circle", "x": 1.0, "z": 1.0, "width": 4.0, "radius": 1.5,
                "surface_y": 0.5 }"#,
            "must be absent or equal",
            "a circle whose box contradicts its radius",
        ),
        (
            r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "radius": 1.5,
                "surface_y": 0.5 }"#,
            "authors radius on a rectangle",
            "a rectangle with a radius",
        ),
        (
            r#"{ "x": 1.0, "z": 1.0, "depth": 2.0, "surface_y": 0.5 }"#,
            "must author width",
            "a rectangle without a width",
        ),
        (
            r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "surface_y": 0.5 }"#,
            "must author depth",
            "a rectangle without a depth",
        ),
    ];
    for (water, needle, label) in cases {
        let error = crate::loader::validate_level(&level(water)).expect_err(label);
        assert!(error.contains(needle), "{label}: {error}");
    }
}

// ------------------------------------------------------- fixture grid alignment

/// A synthetic catalog with a 2 m and a 1 m ceiling material, enough to resolve
/// a logical material table without touching the filesystem.
fn alignment_catalog() -> crate::assets::AssetCatalog {
    crate::assets::AssetCatalog::from_json_str(
        r#"{
            "format_version": 3,
            "assets": [
                { "id": "test:tex_ceiling", "asset_class": "environment",
                  "asset_type": "texture", "source": "file",
                  "model": "test/ceiling.png", "surface": "ceiling" },
                { "id": "test:ceiling_2m", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "ceiling", "texture": "test:tex_ceiling",
                  "tile_metres": 2.0 },
                { "id": "test:ceiling_1m", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "ceiling", "texture": "test:tex_ceiling",
                  "tile_metres": 1.0 },
                { "id": "test:ceiling_2m_grid1m", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "ceiling", "texture": "test:tex_ceiling",
                  "tile_metres": 2.0, "grid_metres": 1.0 }
            ]
        }"#,
    )
    .expect("synthetic alignment catalog parses")
}

fn alignment_table(level: &LevelDef) -> crate::materials::MaterialTable {
    crate::materials::MaterialTable::logical(level, &alignment_catalog(), None)
}

/// A flat 10 x 10 m room with a 2 m ceiling grid and one fixture.
fn alignment_level(fixture: &str, ceiling_material: &str, ceiling: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "align",
            "name": "Align",
            "spawn": {{ "x": 2.0, "z": 2.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                         "height": 3.0, "ceiling_material": "{ceiling_material}",
                         "ceiling": {ceiling} }}],
            "ceiling_lights": [{fixture}]
        }}"#
    ))
    .expect("alignment level parses")
}

/// A 2 m panel ceiling snaps the fixture centre to the nearest cell centre on
/// both axes, and nothing but `x`/`z` changes.
#[test]
fn test_grid_alignment_snaps_a_fluorescent_panel_to_cell_centres() {
    let mut level = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2,
             "rotation_degrees": 90.0, "brightness": 0.34, "emission": 1.0,
             "color": [1.0, 0.2, 0.15] }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    let moved = level.align_ceiling_fixtures(&alignment_table(&level));
    assert_eq!(moved, 1, "one panel was off its ceiling grid");
    let light = &level.ceiling_lights[0];
    assert_exact(light.x, 3.0);
    assert_exact(light.z, 3.0);
    // Everything else is untouched.
    assert_exact(light.rotation_degrees, 90.0);
    assert_eq!(light.brightness, Some(0.34));
    assert_eq!(light.emission, Some(1.0));
    assert_eq!(
        light.color,
        Some(crate::lighting::LightColor::rgb(1.0, 0.2, 0.15))
    );
}

/// `"align": "none"` is honoured exactly; a fixture already on a cell centre
/// does not count as moved.
#[test]
fn test_grid_alignment_none_keeps_the_authored_position() {
    let mut level = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2,
             "align": "none" }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(level.align_ceiling_fixtures(&alignment_table(&level)), 0);
    assert_exact(level.ceiling_lights[0].x, 2.5);
    assert_exact(level.ceiling_lights[0].z, 2.2);

    let mut settled = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(
        settled.align_ceiling_fixtures(&alignment_table(&settled)),
        0
    );
    assert_exact(settled.ceiling_lights[0].x, 3.0);
}

/// Round downlights are not grid panels, and a gable ceiling has no single
/// plane or grid to align to.
#[test]
fn test_grid_alignment_leaves_round_downlights_and_gable_ceilings_alone() {
    let mut round = alignment_level(
        r#"{ "fixture": "core:pool_light_round", "x": 2.5, "z": 2.2 }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(round.align_ceiling_fixtures(&alignment_table(&round)), 0);
    assert_exact(round.ceiling_lights[0].x, 2.5);

    let mut gable = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2 }"#,
        "test:ceiling_2m",
        r#"{ "kind": "gable", "ridge": "x", "ridge_rise": 2.0 }"#,
    );
    assert_eq!(gable.align_ceiling_fixtures(&alignment_table(&gable)), 0);
    assert_exact(gable.ceiling_lights[0].x, 2.5);
    assert_exact(gable.ceiling_lights[0].z, 2.2);
}

/// No resolvable ceiling material period means no grid: a blank default and a
/// fixture outside every room both stay exactly where they were authored.
#[test]
fn test_grid_alignment_without_a_ceiling_period_stays_put() {
    let mut blank = level_from_json_with_defaults(
        r#"{ "wall": "test:ceiling_2m", "floor": "test:ceiling_2m", "ceiling": "" }"#,
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2 }"#,
    );
    assert_eq!(blank.align_ceiling_fixtures(&alignment_table(&blank)), 0);
    assert_exact(blank.ceiling_lights[0].x, 2.5);
    assert_exact(blank.ceiling_lights[0].z, 2.2);

    let mut outside = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 25.0, "z": 25.0 }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(
        outside.align_ceiling_fixtures(&alignment_table(&outside)),
        0
    );
    assert_exact(outside.ceiling_lights[0].x, 25.0);
}

/// The returned count is the number of fixtures whose centre actually moved,
/// and every qualifying panel snaps to a cell centre of its own period.
#[test]
fn test_grid_alignment_counts_and_periods_match() {
    let mut level = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2 }
           ,{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 }
           ,{ "fixture": "core:pool_light_round", "x": 6.5, "z": 5.4 }
           ,{ "fixture": "core:fluorescent_panel_01", "x": 6.5, "z": 5.4, "align": "none" }"#,
        "test:ceiling_2m",
        r#"{ "kind": "flat" }"#,
    );
    let moved = level.align_ceiling_fixtures(&alignment_table(&level));
    assert_eq!(moved, 1, "only the off-grid default-aligned panel moves");
    assert_exact(level.ceiling_lights[0].x, 3.0);
    assert_exact(level.ceiling_lights[0].z, 3.0);
    assert_exact(level.ceiling_lights[2].x, 6.5);
    assert_exact(level.ceiling_lights[3].x, 6.5);

    let mut one_metre = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2 }"#,
        "test:ceiling_1m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(
        one_metre.align_ceiling_fixtures(&alignment_table(&one_metre)),
        1
    );
    // A 1 m grid's cell centres are the half-metre marks.
    assert_exact(one_metre.ceiling_lights[0].x, 2.5);
    assert_exact(one_metre.ceiling_lights[0].z, 2.5);
}

/// A level with an authored `defaults` block and one fixture.
fn level_from_json_with_defaults(defaults: &str, fixture: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "align_defaults",
            "name": "Align Defaults",
            "spawn": {{ "x": 2.0, "z": 2.0 }},
            "defaults": {defaults},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                         "height": 3.0 }}],
            "ceiling_lights": [{fixture}]
        }}"#
    ))
    .expect("defaults alignment level parses")
}

/// A ceiling sheet that paints four 1 m panels inside a 2 m texture repeat
/// aligns fixtures to panel centres, not to the sheet's repeat centres.
#[test]
fn test_grid_alignment_uses_the_panel_module_not_the_sheet_repeat() {
    let mut level = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.5, "z": 2.2 }"#,
        "test:ceiling_2m_grid1m",
        r#"{ "kind": "flat" }"#,
    );
    let moved = level.align_ceiling_fixtures(&alignment_table(&level));
    assert_eq!(moved, 1, "the panel moves from the T-bar to a panel centre");
    // Panel centres are the half-metre marks, not odd metres.
    assert_exact(level.ceiling_lights[0].x, 2.5);
    assert_exact(level.ceiling_lights[0].z, 2.5);

    // A fixture already on a panel centre does not count as moved.
    let mut settled = alignment_level(
        r#"{ "fixture": "core:fluorescent_panel_01", "x": 4.5, "z": 5.5 }"#,
        "test:ceiling_2m_grid1m",
        r#"{ "kind": "flat" }"#,
    );
    assert_eq!(
        settled.align_ceiling_fixtures(&alignment_table(&settled)),
        0
    );
    assert_exact(settled.ceiling_lights[0].x, 4.5);
    assert_exact(settled.ceiling_lights[0].z, 5.5);
}

// ---------------------------------------------------------------------------
// instance identity, actions and area triggers
// ---------------------------------------------------------------------------

/// Actions are a closed, internally tagged set: a known tag parses to its
/// variant, an unknown tag is a parse error (never an ignored key), and a
/// round-trip preserves the authored shape.
#[test]
fn test_action_defs_parse_as_tagged_objects() {
    let toggle: ActionDef =
        serde_json::from_str(r#"{ "action": "toggle_label" }"#).expect("toggle_label parses");
    assert_eq!(toggle, ActionDef::ToggleLabel { target: None });
    assert_eq!(toggle.kind(), "toggle_label");
    assert_eq!(toggle.target(), None);

    let targeted: ActionDef =
        serde_json::from_str(r#"{ "action": "toggle_label", "target": "plant_1" }"#)
            .expect("a targeted toggle parses");
    assert_eq!(
        targeted,
        ActionDef::ToggleLabel {
            target: Some("plant_1".into())
        }
    );
    assert_eq!(targeted.target(), Some("plant_1"));

    let reset: ActionDef =
        serde_json::from_str(r#"{ "action": "reset_to_start" }"#).expect("reset_to_start parses");
    assert_eq!(reset, ActionDef::ResetToStart);

    // The v3 action set replaced the reserved `play_audio` tag with
    // `play_sound`; the removed tag is an unknown action now, never a silently
    // accepted alias.
    assert!(
        serde_json::from_str::<ActionDef>(r#"{ "action": "play_audio", "sound": "beep" }"#)
            .is_err(),
        "the removed play_audio tag must not parse"
    );
    let sound: ActionDef = serde_json::from_str(r#"{ "action": "play_sound", "sound": "beep" }"#)
        .expect("the play_sound action parses");
    assert_eq!(
        sound,
        ActionDef::PlaySound {
            target: None,
            sound: Some("beep".into()),
            looped: false
        }
    );
    assert_eq!(sound.kind(), "play_sound");
    let animation: ActionDef =
        serde_json::from_str(r#"{ "action": "play_animation", "clip": "wave" }"#)
            .expect("the reserved animation action parses");
    assert_eq!(animation.kind(), "play_animation");

    assert!(
        serde_json::from_str::<ActionDef>(r#"{ "action": "launch_missiles" }"#).is_err(),
        "an unknown action is a parse error, not an ignored key"
    );

    // The serialized form keeps the tag and its fields.
    let serialized = serde_json::to_string(&targeted).expect("serialize");
    assert!(serialized.contains("\"action\":\"toggle_label\""));
    assert!(serialized.contains("\"target\":\"plant_1\""));
}

/// Instance ids are authored when present, deterministic when not: the default
/// counts placements per model short name in array order.
#[test]
fn test_prop_instance_ids_default_deterministically() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "ids",
            "name": "Ids",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "props": [
                { "model": "core:chair", "x": 1.0, "z": 1.0 },
                { "id": "nook_chair", "model": "core:chair", "x": 2.0, "z": 1.0 },
                { "model": "core:chair", "x": 3.0, "z": 1.0 },
                { "model": "core:plant", "x": 4.0, "z": 1.0 }
            ]
        }"#,
    )
    .expect("the id level parses");
    assert_eq!(
        level.prop_instance_ids(),
        vec!["chair_1", "nook_chair", "chair_2", "plant_1"]
    );
    // Deterministic across parses: the same document yields the same ids.
    assert_eq!(
        level.prop_instance_ids(),
        level.prop_instance_ids(),
        "id resolution never depends on call order"
    );

    let light_level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "light_ids",
            "name": "Light Ids",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 1.0 }
            ]
        }"#,
    )
    .expect("the light level parses");
    assert_eq!(
        light_level.light_instance_ids(),
        vec!["fluorescent_panel_01_1", "fluorescent_panel_01_2"]
    );
}

/// Area trigger volumes resolve authored ids, default ids, authored vertical
/// bounds and floor-derived bounds, and `contains` is a real volume test.
#[test]
fn test_area_triggers_resolve_ids_and_bounds() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "triggers",
            "name": "Triggers",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0,
                      "floor_y": -1.0 } ],
            "floor_regions": [
                { "x": 6.0, "z": 6.0, "width": 2.0, "depth": 2.0, "offset_y": -1.0 }
            ],
            "volumes": [
                { "id": "authored", "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0,
                  "bottom_y": -1.0, "top_y": -0.1,
                  "bindings": [{ "on": "enter_volume",
                                 "actions": [{ "action": "reset_to_start" }] }] },
                { "x": 6.0, "z": 6.0, "width": 2.0, "depth": 2.0,
                  "bindings": [{ "on": "enter_volume",
                                 "actions": [{ "action": "reset_to_start" }] }] }
            ]
        }"#,
    )
    .expect("the trigger level parses");
    assert_eq!(level.volume_instance_ids(), vec!["authored", "trigger_2"]);

    let world = crate::entities::EntityWorld::from_level(&level);
    let volumes: Vec<_> = world.components().volumes.iter().collect();
    assert_eq!(volumes.len(), 2);
    let (first_handle, first) = volumes[0];
    assert_eq!(world.id_of(first_handle), Some("authored"));
    assert!((first.bottom_y - (-1.0)).abs() < 1e-6);
    assert!((first.top_y - (-0.1)).abs() < 1e-6);
    assert!(first.contains(1.5, 1.5, -0.5));
    assert!(!first.contains(1.5, 3.5, -0.5));
    assert!(!first.contains(1.5, 1.5, -1.2));

    // The second trigger's vertical bounds default to the floor under its
    // centre (the recess at -2.0) plus the documented 2.0 m height.
    let (_, second) = volumes[1];
    assert!(
        (second.bottom_y - (-2.0)).abs() < 1e-6,
        "{}",
        second.bottom_y
    );
    assert!((second.top_y - 0.0).abs() < 1e-6, "{}", second.top_y);
}

/// A trigger with a non-finite footprint or an inverted authored band is
/// skipped at resolution time, exactly like a malformed water volume or ladder;
/// the loader rejects it before a real level ever reaches here. (A zero-area
/// footprint is no longer filtered at resolution — validation owns that check —
/// so the malformed case here is a non-finite one.)
#[test]
fn test_malformed_area_triggers_are_skipped_at_resolution() {
    let mut level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "bad_triggers",
            "name": "Bad Triggers",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "volumes": [
                { "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0,
                  "bottom_y": 0.0, "top_y": 1.0,
                  "bindings": [{ "on": "enter_volume",
                                 "actions": [{ "action": "reset_to_start" }] }] },
                { "x": 2.0, "z": 2.0, "width": 1.0, "depth": 1.0,
                  "bottom_y": 1.0, "top_y": 0.0,
                  "bindings": [{ "on": "enter_volume",
                                 "actions": [{ "action": "reset_to_start" }] }] },
                { "x": 3.0, "z": 3.0, "width": 1.0, "depth": 1.0,
                  "bottom_y": 0.0, "top_y": 1.0,
                  "bindings": [{ "on": "enter_volume",
                                 "actions": [{ "action": "reset_to_start" }] }] }
            ]
        }"#,
    )
    .expect("the malformed trigger level parses");
    // A non-finite footprint cannot come from strict JSON, so it is authored
    // directly on the first record.
    level.volumes[0].width = f32::INFINITY;
    let world = crate::entities::EntityWorld::from_level(&level);
    assert_eq!(
        world.components().volumes.len(),
        1,
        "only the last trigger is well formed"
    );
    let (handle, _) = world
        .components()
        .volumes
        .iter()
        .next()
        .expect("one trigger");
    assert_eq!(world.id_of(handle), Some("trigger_3"));
}

/// A `float` block is optional decoration on the prop schema: every old map
/// keeps parsing with no float at all, and a block that only names what it
/// changes fills the rest in from [`DEFAULT_FLOAT_PERIOD_S`].
#[test]
fn test_prop_float_is_optional_and_defaults_fill_in() {
    // An untouched prop schema: no `float` key anywhere.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "float_defaults",
            "name": "Float Defaults",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ],
            "props": [
                { "model": "core:chair", "x": 1.0, "z": 1.0 },
                { "model": "core:rubber_duck", "x": 2.0, "z": 2.0 }
            ]
        }"#,
    )
    .expect("a level without floats still parses");
    assert!(level.props[0].float.is_none(), "an old map stays unchanged");
    assert!(level.props[1].float.is_none());

    // A float block only has to name what it changes: the periods fall back to
    // the shared default and the phase stays unauthored.
    let float: PropFloatDef =
        serde_json::from_str(r#"{ "draft": 0.03, "bob": 0.012, "heel_degrees": 3.0 }"#)
            .expect("a partial float block parses");
    assert_exact(float.draft, 0.03);
    assert_exact(float.bob, 0.012);
    assert_exact(float.heel_degrees, 3.0);
    assert_exact(float.bob_seconds, DEFAULT_FLOAT_PERIOD_S);
    assert_exact(float.heel_seconds, DEFAULT_FLOAT_PERIOD_S);
    assert_eq!(float.phase, None);

    // The same defaults apply when the block is parsed inside a level.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "float_defaults",
            "name": "Float Defaults",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ],
            "props": [
                { "model": "core:rubber_duck", "x": 2.0, "z": 2.0,
                  "float": { "draft": 0.03 } }
            ]
        }"#,
    )
    .expect("a level with a float block parses");
    let block = level.props[0].float.expect("the float block parsed");
    assert_exact(block.bob_seconds, DEFAULT_FLOAT_PERIOD_S);
    assert_exact(block.heel_seconds, DEFAULT_FLOAT_PERIOD_S);
    assert_eq!(block.phase, None);
}

/// `contains_disc` proves the swept-footprint guarantee a float relies on:
/// one volume must contain the whole disc, and the boundary itself is inside.
#[test]
fn test_water_contains_disc_requires_one_volume_to_hold_the_whole_disc() {
    // Two adjacent 10x10 basins sharing the x = 10 seam, so the "one volume"
    // half of the rule is testable.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "disc_test",
            "name": "Disc Test",
            "spawn": { "x": 0.0, "z": 0.0 },
            "water": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "surface_y": 1.0 },
                { "x": 10.0, "z": 0.0, "width": 10.0, "depth": 10.0, "surface_y": 1.0 }
            ]
        }"#,
    )
    .expect("the disc test level parses");
    let water = WaterVolumes::from_level(&level);
    assert_eq!(water.len(), 2);

    // A disc well inside one basin.
    assert!(water.contains_disc(5.0, 5.0, 4.0));
    // A radius exactly touching an edge and a corner is still contained...
    assert!(water.contains_disc(4.5, 5.0, 4.5), "edge-touching disc");
    assert!(water.contains_disc(4.5, 4.5, 4.5), "corner-touching disc");
    // ... and one hair more is not: the containment is inclusive at the rim.
    assert!(!water.contains_disc(4.5, 5.0, 4.5 + 1e-5));

    // A disc that straddles the seam between two volumes is contained by
    // neither the left nor the right volume, however much water it covers.
    assert!(!water.contains_disc(10.0, 5.0, 0.5));
    // A disc that starts exactly on the seam belongs to the second volume.
    assert!(water.contains_disc(10.5, 5.0, 0.5));

    // Outside every footprint, an oversized disc, the empty set and malformed
    // input never contain.
    assert!(!water.contains_disc(20.5, 5.0, 0.4));
    assert!(!water.contains_disc(5.0, 5.0, 5.01));
    assert!(!WaterVolumes::new().contains_disc(5.0, 5.0, 0.5));
    assert!(!water.contains_disc(f32::NAN, 5.0, 1.0));
    assert!(!water.contains_disc(5.0, f32::NAN, 1.0));
    assert!(!water.contains_disc(5.0, 5.0, f32::NAN));
    assert!(!water.contains_disc(5.0, 5.0, -1.0));
}

// ---------------------------------------------------------------------------
// round architecture and the ceiling tile frame
// ---------------------------------------------------------------------------

#[test]
fn test_round_primitives_parse_with_sensible_defaults() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "round_parse",
            "name": "Round Parse",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 } ],
            "arc_walls": [
                { "x": 5.0, "z": 5.0, "radius": 2.0 }
            ],
            "pillars": [
                { "x": 3.0, "z": 3.0, "radius": 0.4 }
            ]
        }"#,
    )
    .expect("round primitives parse");
    let arc = &level.arc_walls[0];
    assert_exact(arc.thickness, DEFAULT_ARC_WALL_THICKNESS_M);
    assert_exact(arc.sweep_degrees, DEFAULT_ARC_SWEEP_DEGREES);
    assert_exact(arc.start_degrees, 0.0);
    assert!(arc.segments.is_none());
    // A 90 degree default sweep resolves to a quarter of the 24-segment ring.
    assert_eq!(arc.resolved_segments(), 6);
    assert!(!arc.is_full_ring());
    assert_exact(arc.inner_radius(), 2.0 - DEFAULT_ARC_WALL_THICKNESS_M * 0.5);
    assert_exact(arc.outer_radius(), 2.0 + DEFAULT_ARC_WALL_THICKNESS_M * 0.5);

    let pillar = &level.pillars[0];
    assert_eq!(pillar.resolved_segments(), ROUND_SEGMENTS_DEFAULT);
    assert_eq!(pillar.polygon_points().len(), 24);
    assert_eq!(round_segments_for(360.0, Some(4)), 4);
    assert_eq!(round_segments_for(360.0, Some(2)), ROUND_SEGMENTS_DEFAULT);
    assert_eq!(
        round_segments_for(360.0, Some(1024)),
        ROUND_SEGMENTS_DEFAULT
    );
    // A full ring has no ends; a quarter arc does.
    let mut ring = arc.clone();
    ring.sweep_degrees = 360.0;
    assert!(ring.is_full_ring());
    assert_eq!(ring.resolved_segments(), 24);
}

#[test]
fn test_ceiling_tile_frame_round_trips_and_rotates() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "tile_frame",
            "name": "Tile Frame",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                  "ceiling_tile_origin": [1.0, 2.0],
                  "ceiling_tile_rotation_degrees": 90.0 }
            ]
        }"#,
    )
    .expect("the tile frame parses");
    let room = &level.rooms[0];
    let (origin_x, origin_z, rotation) = room.ceiling_tile_frame();
    assert_exact(origin_x, 1.0);
    assert_exact(origin_z, 2.0);
    assert_exact(rotation, 90.0);
    // Rotation is about the origin in the (x, z) plane; the round trip is exact.
    let (local_x, local_z) = room.ceiling_tile_local(3.25, 1.75);
    assert!((local_x - 0.25).abs() < 1.0e-4, "local x {local_x}");
    assert!((local_z - 2.25).abs() < 1.0e-4, "local z {local_z}");
    let (world_x, world_z) = room.ceiling_tile_world(local_x, local_z);
    assert!((world_x - 3.25).abs() < 1.0e-4, "world x {world_x}");
    assert!((world_z - 1.75).abs() < 1.0e-4, "world z {world_z}");
    // A lattice point of the rotated frame is a fixed point of the frame.
    let (fixed_x, fixed_z) = room.ceiling_tile_world(-1.5, 2.5);
    let (back_x, back_z) = room.ceiling_tile_local(fixed_x, fixed_z);
    assert!((back_x + 1.5).abs() < 1.0e-4, "fixed local x {back_x}");
    assert!((back_z - 2.5).abs() < 1.0e-4, "fixed local z {back_z}");
}

#[test]
fn test_ceiling_uvs_follow_the_room_tile_frame() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "tile_uv",
            "name": "Tile Uv",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "ceiling_tile_origin": [1.0, 2.0],
                  "ceiling_tile_rotation_degrees": 90.0 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("the tiled level parses");
    let room = &level.rooms[0];
    let materials = crate::render::logical_materials(&level);
    let tile = materials
        .entry_of("core:ceiling_panel_01")
        .expect("the default ceiling")
        .tile_metres;
    let mesh = crate::render::build_level_geometry_with_materials(&level, &materials);
    let mut checked = 0;
    let mut differs_from_world_tiling = false;
    for range in &mesh.ranges {
        if range.key.kind != crate::render::SurfaceKind::Ceiling {
            continue;
        }
        for vertex in &range.vertices {
            let (x, z) = (vertex.pos[0], vertex.pos[2]);
            let expected = room.ceiling_tile_local(x, z);
            assert_exact_named(
                vertex.uv[0],
                crate::render::tiled_uv(expected.0, expected.1, tile)[0],
                "ceiling u",
            );
            assert_exact_named(
                vertex.uv[1],
                crate::render::tiled_uv(expected.0, expected.1, tile)[1],
                "ceiling v",
            );
            if (vertex.uv[0] - x / tile).abs() > 1.0e-3 || (vertex.uv[1] - z / tile).abs() > 1.0e-3
            {
                differs_from_world_tiling = true;
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "the ceiling emitted vertices");
    assert!(
        differs_from_world_tiling,
        "the authored frame must move the ceiling UVs off the world grid"
    );
}

#[test]
fn test_ceiling_grid_decals_snap_in_the_rooms_own_frame() {
    let mut level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "tile_decal",
            "name": "Tile Decal",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                  "ceiling_tile_origin": [1.0, 2.0],
                  "ceiling_tile_rotation_degrees": 90.0 }
            ],
            "decals": [
                { "x": 3.4, "y": 0.0, "z": 2.2, "width": 0.6, "height": 0.6,
                  "material": "core:decal_ceiling_vent_01", "surface": "ceiling",
                  "align": "ceiling_grid" },
                { "x": 4.0, "y": 0.0, "z": 4.0, "width": 0.6, "height": 0.6,
                  "material": "core:decal_ceiling_vent_01", "surface": "ceiling" }
            ]
        }"#,
    )
    .expect("the decal level parses");
    let materials = crate::render::logical_materials(&level);
    let moved = level.snap_ceiling_decals(&materials);
    assert_eq!(moved, 1, "only the aligned decal moves");
    // Unaligned decal: authored exactly, no rotation composition.
    assert_exact(level.decals[1].x, 4.0);
    assert_exact(level.decals[1].z, 4.0);
    assert_exact(level.ceiling_decal_rotation(&level.decals[1]), 0.0);
    // Aligned decal: snapped to a lattice point of the rotated frame and its
    // rotation composed with the room's.
    let snapped = &level.decals[0];
    let (local_x, local_z) = level.rooms[0].ceiling_tile_local(snapped.x, snapped.z);
    for value in <[f32; 2]>::from((local_x, local_z)) {
        let remainder = (value - 0.5).round();
        assert!(
            (value - (remainder + 0.5)).abs() < 1.0e-3,
            "snapped local {value} is not a 1 m panel centre"
        );
    }
    assert_exact(level.ceiling_decal_rotation(snapped), 90.0);
    // The pass is idempotent: snapping again moves nothing.
    assert_eq!(level.snap_ceiling_decals(&materials), 0);
}

#[test]
fn raised_region_keeps_its_height_through_the_room_edge_tolerance() {
    let level = LevelDef::from_json(
        r#"{
        "format_version": 3, "id": "join", "name": "Join", "spawn": {"x": 1, "z": 1},
        "rooms": [
            {"x": 0, "z": 0, "width": 4, "depth": 4, "height": 3, "floor_y": -1.5},
            {"x": 4, "z": 0, "width": 4, "depth": 4, "height": 3, "floor_y": -0.9}
        ],
        "floor_regions": [{"x": 2, "z": 0, "width": 2, "depth": 4, "offset_y": 0.6}]
    }"#,
    )
    .expect("joined rooms");
    let floor = WalkableFloor::from_level(&level);
    for x in [3.99, 4.0, 4.0001, 4.001, 4.01] {
        for y in [floor.height_at(x, 2.0), floor.walk_height_at(x, 2.0)] {
            assert!((y.expect("floor") + 0.9).abs() < 1.0e-5, "join at {x}");
        }
    }
}

#[test]
fn the_switchable_fixture_material_base_starts_after_the_family_sheets() {
    // The fixture-face slot table is `families ++ switchable fixtures`; if a
    // family were added without moving the base, a switchable fixture would
    // overwrite a family's sheet.
    assert_eq!(
        usize::try_from(crate::level::FIXTURE_SWITCHABLE_MATERIAL_BASE).unwrap_or(usize::MAX),
        crate::lighting::FixtureKind::ALL.len()
    );
    assert_eq!(
        crate::level::fixture_face_material_index(
            0,
            false,
            crate::lighting::FixtureKind::WallSconce
        ),
        u32::try_from(crate::lighting::FixtureKind::WallSconce.index()).expect("fits")
    );
    assert_eq!(
        crate::level::fixture_face_material_index(
            7,
            true,
            crate::lighting::FixtureKind::WallSconce
        ),
        crate::level::FIXTURE_SWITCHABLE_MATERIAL_BASE + 7
    );
}

#[test]
fn stair_treads_share_exact_boundaries_with_risers_and_landing() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "stair_joins", "name": "Stair joins",
            "spawn": {"x": 1.0, "z": 1.0},
            "rooms": [{"x": -100.0, "z": -100.0, "width": 200.0, "depth": 200.0}],
            "stairs": [
                {"x": 58.31, "z": 4.17, "width": 3.39, "depth": 1.3, "rise": 1.7, "steps": 13},
                {"x": -37.13, "z": -61.73, "width": 1.3, "depth": 7.73, "rise": 2.1, "steps": 17}
            ]
        }"#,
    )
    .expect("valid stairs");
    for stair in &level.stairs {
        for step in 1..stair.step_count() {
            assert_exact(stair.tread_span(step - 1).1, stair.tread_span(step).0);
        }
        let (_, x1, _, z1) = stair.bounds();
        let end = match stair.axis() {
            WallAxis::X => x1,
            WallAxis::Z => z1,
        };
        assert_exact(stair.tread_span(stair.step_count() - 1).1, end);
    }
}

/// A sink's collider is its standable deck, never the model's faucet post:
/// over the -0.9 m kitchen floor the authored 0.90 m `size` yields a box
/// topping out at exactly 0.0 with the real 0.6 x 0.55 m footprint, a
/// conservative 0.55 x 0.6 m box at 90 degrees, and a top that scales with
/// the placement.
#[test]
fn a_sink_collider_is_its_standable_deck_not_the_faucet_post() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "sink_decks",
            "name": "Sink Decks",
            "spawn": { "x": 54.8, "z": 3.4 },
            "rooms": [ { "x": 53.0, "z": 3.0, "width": 12.0, "depth": 12.0, "height": 4.5, "floor_y": -0.9 } ],
            "props": [
                { "model": "core:sink", "x": 54.8, "y": 0.0, "z": 3.425, "size": [0.6, 0.9, 0.55], "solid": true },
                { "model": "core:sink", "x": 56.0, "y": 0.0, "z": 3.425, "rotation_degrees": 90.0, "size": [0.6, 0.9, 0.55], "solid": true },
                { "model": "core:sink", "x": 57.5, "y": 0.0, "z": 3.425, "scale": 1.2, "size": [0.6, 0.9, 0.55], "solid": true }
            ]
        }"#,
    )
    .expect("the sink level parses");
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 3, "exactly the three solid sinks collide");

    // The plain deck: -0.9 + 0.0 .. -0.9 + 0.9, footprint 0.6 x 0.55.
    let plain = aabbs[0];
    assert_exact(plain.min_y, -0.9);
    assert_exact(plain.max_y, 0.0);
    assert!((plain.max_x - plain.min_x - 0.6).abs() < 1e-4);
    assert!((plain.max_z - plain.min_z - 0.55).abs() < 1e-4);
    assert!((plain.min_x + plain.max_x - 2.0 * 54.8).abs() < 1e-4);

    // Rotated 90 degrees: the same top, the conservative 0.55 x 0.6 m box
    // that contains the rotated rectangle's corners.
    let rotated = aabbs[1];
    assert_exact(rotated.max_y, 0.0);
    assert!((rotated.max_x - rotated.min_x - 0.55).abs() < 1e-4);
    assert!((rotated.max_z - rotated.min_z - 0.6).abs() < 1e-4);

    // Scaled 1.2: the top is the deck height times the scale.
    let scaled = aabbs[2];
    assert!((scaled.max_y - (-0.9 + 1.08)).abs() < 1e-5);
    assert!((scaled.max_x - scaled.min_x - 0.72).abs() < 1e-4);
    assert!((scaled.max_z - scaled.min_z - 0.66).abs() < 1e-4);
}

/// The shipped Places Demo kitchen counter run: every collider over the
/// counter footprint tops out at the 0.0 m counter surface (the -0.9 m floor
/// plus the 0.9 m cabinet/stove/sink size), and nothing occupies the
/// 0.55..0.9 m band above it, so the upper cabinets (now `solid: false`) can
/// never push a player standing on the counter off it. This is the surface
/// the raised jump apex can land on.
#[test]
fn the_shipped_demo_counters_are_clear_above_their_tops() {
    let content = std::fs::read_to_string("assets/levels/places_demo.json")
        .expect("the Places demo level is present");
    let level = LevelDef::from_json(&content).expect("the Places demo parses");

    // Four 0.6 m base cabinets, the stove and the sink along the kitchen's
    // north wall: x 53.3..56.9 by the base cabinets' 0.6 m depth at z 3.45.
    let (x0, x1, z0, z1) = (53.3_f32, 56.9_f32, 3.15_f32, 3.75_f32);
    let mut counter_boxes = 0_u32;
    for aabb in level.collision_aabbs() {
        let overlaps = aabb.max_x > x0 + 1e-4
            && aabb.min_x < x1 - 1e-4
            && aabb.max_z > z0 + 1e-4
            && aabb.min_z < z1 - 1e-4;
        if !overlaps {
            continue;
        }
        assert!(
            !(aabb.min_y < 0.9 - 1e-3 && aabb.max_y > 0.55 + 1e-3),
            "a collider spans the walkable band over the counter: {aabb:?}"
        );
        let inside = aabb.min_x > x0 - 1e-3
            && aabb.max_x < x1 + 1e-3
            && aabb.min_z > z0 - 1e-3
            && aabb.max_z < z1 + 1e-3;
        if inside {
            counter_boxes += 1;
            assert!(
                aabb.max_y.abs() < 1e-4,
                "a counter box tops out at {} instead of 0.0: {aabb:?}",
                aabb.max_y
            );
            assert!(
                (aabb.min_y + 0.9).abs() < 1e-4,
                "a counter box must start on the -0.9 m kitchen floor: {aabb:?}"
            );
        }
    }
    assert_eq!(
        counter_boxes, 6,
        "the counter run contributes four bases, the stove and the sink"
    );
}

// ---------------------------------------------------------------------------
// Regional fog
// ---------------------------------------------------------------------------

#[test]
fn fog_regions_parse_defaults_and_fogless_levels_serialize_without_the_key() {
    let json = r#"{
        "format_version": 3,
        "id": "foggy",
        "name": "Foggy",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0 } ],
        "fog_regions": [
            { "id": "yard_mist", "min": [0.0, -2.0, 0.0], "max": [10.0, 1.0, 10.0],
              "density": 0.05 },
            { "id": "stack", "min": [0.0, 0.0, 0.0], "max": [4.0, 3.0, 4.0],
              "density": 0.2, "color": [0.6, 0.63, 0.68], "falloff_m": 3.0,
              "ground_y": 0.5, "top_y": 2.5 }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid fog json");
    assert_eq!(level.fog_regions.len(), 2);
    let mist = &level.fog_regions[0];
    assert_eq!(mist.id, "yard_mist");
    assert_eq!(mist.min, [0.0, -2.0, 0.0]);
    assert_eq!(mist.max, [10.0, 1.0, 10.0]);
    assert_exact(mist.density, 0.05);
    assert_eq!(mist.color, None);
    assert_eq!(mist.falloff_m, None);
    assert_eq!(mist.ground_y, None);
    assert_eq!(mist.top_y, None);
    let stack = &level.fog_regions[1];
    assert_eq!(stack.color, Some([0.6, 0.63, 0.68]));
    assert_exact(stack.falloff_m.unwrap(), 3.0);
    assert_exact(stack.ground_y.unwrap(), 0.5);
    assert_exact(stack.top_y.unwrap(), 2.5);

    // The omitted optionals are not written back, so the level round-trips.
    let encoded = serde_json::to_string(&level).expect("fog level serializes");
    assert!(!encoded.contains("\"falloff_m\":null"));
    assert!(!encoded.contains("\"ground_y\":null"));
    assert!(!encoded.contains("\"top_y\":null"));
    assert!(!encoded.contains("\"color\":null"));
    assert!(encoded.contains("\"id\":\"yard_mist\""));

    // A level with no regions keeps the key out of the serialized document
    // entirely: fog-less levels and packages stay byte-identical.
    let clean = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "clear",
            "name": "Clear",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ]
        }"#,
    )
    .expect("clear level parses");
    let encoded = serde_json::to_string(&clean).expect("clear level serializes");
    assert!(!encoded.contains("fog_regions"));
    assert!(!encoded.contains("void_walls"));
}

// ---------------------------------------------------------------------------
// Void walls
// ---------------------------------------------------------------------------

/// `10 x 10 x 10` box at the origin with every optional field default.
fn test_void_wall(faces: VoidWallFaces) -> VoidWallDef {
    VoidWallDef {
        id: None,
        min: [0.0, 0.0, 0.0],
        max: [10.0, 10.0, 10.0],
        material: "core:wallpaper_yellow_01".into(),
        faces,
        solid: true,
        occludes: true,
    }
}

/// `a - b`.
fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The right-handed normal of the first triangle of a quad.
fn quad_normal(points: [[f32; 3]; 4]) -> [f32; 3] {
    let edge_a = sub3(points[1], points[0]);
    let edge_b = sub3(points[2], points[0]);
    [
        edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
        edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
        edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
    ]
}

/// The unit vector along `value`.
fn normalized3(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    [value[0] / length, value[1] / length, value[2] / length]
}

/// Two-sided Möller–Trumbore: the segment fraction `t` of a hit, or `None`.
fn segment_triangle_t(
    origin: [f32; 3],
    direction: [f32; 3],
    triangle: [[f32; 3]; 3],
) -> Option<f32> {
    let edge_a = sub3(triangle[1], triangle[0]);
    let edge_b = sub3(triangle[2], triangle[0]);
    let p = [
        direction[1] * edge_b[2] - direction[2] * edge_b[1],
        direction[2] * edge_b[0] - direction[0] * edge_b[2],
        direction[0] * edge_b[1] - direction[1] * edge_b[0],
    ];
    let det = edge_a[0] * p[0] + edge_a[1] * p[1] + edge_a[2] * p[2];
    if det.abs() < 1.0e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t_vec = sub3(origin, triangle[0]);
    let u = (t_vec[0] * p[0] + t_vec[1] * p[1] + t_vec[2] * p[2]) * inv;
    if !(-1.0e-6..=1.0 + 1.0e-6).contains(&u) {
        return None;
    }
    let q = [
        t_vec[1] * edge_a[2] - t_vec[2] * edge_a[1],
        t_vec[2] * edge_a[0] - t_vec[0] * edge_a[2],
        t_vec[0] * edge_a[1] - t_vec[1] * edge_a[0],
    ];
    let v = (direction[0] * q[0] + direction[1] * q[1] + direction[2] * q[2]) * inv;
    if v < -1.0e-6 || u + v > 1.0 + 1.0e-6 {
        return None;
    }
    let t = (edge_b[0] * q[0] + edge_b[1] * q[1] + edge_b[2] * q[2]) * inv;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// The nearest segment hit over a face list: `(face index, t, hit point)`.
fn nearest_face_hit(
    faces: &[VoidWallFace],
    from: [f32; 3],
    to: [f32; 3],
) -> Option<(usize, [f32; 3])> {
    let direction = sub3(to, from);
    let mut best: Option<(usize, f32)> = None;
    for (index, face) in faces.iter().enumerate() {
        let triangles = [
            [face.points[0], face.points[1], face.points[2]],
            [face.points[0], face.points[2], face.points[3]],
        ];
        for triangle in triangles {
            if let Some(t) = segment_triangle_t(from, direction, triangle)
                && best.is_none_or(|(_, current)| t < current)
            {
                best = Some((index, t));
            }
        }
    }
    best.map(|(index, t)| {
        (
            index,
            [
                from[0] + direction[0] * t,
                from[1] + direction[1] * t,
                from[2] + direction[2] * t,
            ],
        )
    })
}

/// True when some emitted face crosses `from -> to` with its front side
/// towards `from` (the authored normal points back at the origin).
fn front_facing_hit(faces: &[VoidWallFace], from: [f32; 3], to: [f32; 3]) -> bool {
    let direction = sub3(to, from);
    for face in faces {
        let triangles = [
            [face.points[0], face.points[1], face.points[2]],
            [face.points[0], face.points[2], face.points[3]],
        ];
        for triangle in triangles {
            let Some(t) = segment_triangle_t(from, direction, triangle) else {
                continue;
            };
            let hit = [
                from[0] + direction[0] * t,
                from[1] + direction[1] * t,
                from[2] + direction[2] * t,
            ];
            let to_origin = sub3(from, hit);
            if face.normal[0] * to_origin[0]
                + face.normal[1] * to_origin[1]
                + face.normal[2] * to_origin[2]
                > 0.0
            {
                return true;
            }
        }
    }
    false
}

#[test]
fn void_wall_face_modes_carry_the_authored_winding_and_normal() {
    let centre = [5.0, 5.0, 5.0];
    for faces in [
        VoidWallFaces::Inward,
        VoidWallFaces::Outward,
        VoidWallFaces::Both,
    ] {
        let piece = test_void_wall(faces);
        let emitted = piece.faces();
        assert_eq!(
            emitted.len(),
            if faces == VoidWallFaces::Both { 12 } else { 6 },
            "{faces:?} emits one quad per face, two for both"
        );
        let mut inward = 0usize;
        let mut outward = 0usize;
        for face in &emitted {
            // The winding normal is the authored normal.
            let winding = normalized3(quad_normal(face.points));
            for channel in 0..3 {
                assert!(
                    (winding[channel] - face.normal[channel]).abs() < 1.0e-5,
                    "{faces:?} winding {winding:?} vs normal {:?}",
                    face.normal
                );
            }
            // The authored normal points into or out of the box.
            let face_centre = [
                (face.points[0][0] + face.points[1][0] + face.points[2][0] + face.points[3][0])
                    * 0.25,
                (face.points[0][1] + face.points[1][1] + face.points[2][1] + face.points[3][1])
                    * 0.25,
                (face.points[0][2] + face.points[1][2] + face.points[2][2] + face.points[3][2])
                    * 0.25,
            ];
            let toward_centre = face.normal[0] * (centre[0] - face_centre[0])
                + face.normal[1] * (centre[1] - face_centre[1])
                + face.normal[2] * (centre[2] - face_centre[2]);
            assert!(toward_centre.abs() > 1.0, "a box face is not central");
            if toward_centre > 0.0 {
                inward += 1;
            } else {
                outward += 1;
            }
        }
        match faces {
            VoidWallFaces::Inward => assert_eq!((inward, outward), (6, 0)),
            VoidWallFaces::Outward => assert_eq!((inward, outward), (0, 6)),
            VoidWallFaces::Both => assert_eq!((inward, outward), (6, 6)),
        }
    }
}

#[test]
fn a_sight_ray_through_a_void_wall_is_blocked_and_front_facing_per_mode() {
    let inside = [5.0, 5.0, 5.0];
    let outside = [20.0, 5.0, 5.0];
    for faces in [
        VoidWallFaces::Inward,
        VoidWallFaces::Outward,
        VoidWallFaces::Both,
    ] {
        let piece = test_void_wall(faces);
        let emitted = piece.faces();
        // Both directions cross the +X face at x = 10: the surface blocks the
        // sight line because it is real geometry.
        let (from_inside, hit_inside) =
            nearest_face_hit(&emitted, inside, outside).expect("inside ray crosses the shell");
        assert!(
            hit_inside[0] >= 9.99 && hit_inside[0] <= 10.01,
            "{hit_inside:?}"
        );
        let (from_outside, hit_outside) =
            nearest_face_hit(&emitted, outside, inside).expect("outside ray crosses the shell");
        assert!(
            hit_outside[0] >= 9.99 && hit_outside[0] <= 10.01,
            "{hit_outside:?}"
        );

        // `both` is two coplanar quads per face, so the nearest-hit face is
        // whichever duplicate the scan reaches first; the honest question is
        // whether *a* front-facing quad exists on the hit plane from each side.
        // The authored facing decides that: the engine shades both sides, so
        // the mode is an orientation, not a visibility switch.
        let inside_front = front_facing_hit(&emitted, inside, outside);
        let outside_front = front_facing_hit(&emitted, outside, inside);
        match faces {
            VoidWallFaces::Inward => assert!(inside_front && !outside_front),
            VoidWallFaces::Outward => assert!(!inside_front && outside_front),
            VoidWallFaces::Both => assert!(inside_front && outside_front),
        }
        // The face each direction actually reaches is the +X plane either way.
        assert_eq!(
            from_inside, from_outside,
            "{faces:?} duplicates are coplanar"
        );
        assert!(hit_inside[0] >= 9.99 && hit_inside[0] <= 10.01);
        assert!(hit_outside[0] >= 9.99 && hit_outside[0] <= 10.01);
    }
}

#[test]
fn void_walls_parse_defaults_and_a_solid_box_reaches_collision() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "voided",
            "name": "Voided",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 } ],
            "void_walls": [
                { "id": "yard_shell_north", "min": [-1.0, 0.0, -1.0], "max": [1.0, 3.0, 15.0],
                  "material": "outdoor:dirt_gravel_01" },
                { "min": [4.0, 0.0, 4.0], "max": [6.0, 4.0, 6.0],
                  "material": "core:wallpaper_yellow_01", "faces": "outward",
                  "solid": false, "occludes": false }
            ]
        }"#,
    )
    .expect("valid void json");
    assert_eq!(level.void_walls.len(), 2);
    let shell = &level.void_walls[0];
    assert_eq!(shell.id.as_deref(), Some("yard_shell_north"));
    assert_eq!(shell.faces, VoidWallFaces::Inward);
    assert!(shell.solid && shell.occludes);
    let plate = &level.void_walls[1];
    assert_eq!(plate.id, None);
    assert_eq!(plate.faces, VoidWallFaces::Outward);
    assert!(!plate.solid && !plate.occludes);

    // Only the `solid` box collides; the plate is sight-only.
    let aabbs = level.collision_aabbs();
    assert_eq!(aabbs.len(), 1, "one solid void wall");
    assert_exact(aabbs[0].min_x, -1.0);
    assert_exact(aabbs[0].max_x, 1.0);
    assert_exact(aabbs[0].min_z, -1.0);
    assert_exact(aabbs[0].max_z, 15.0);
    assert_exact(aabbs[0].max_y, 3.0);

    // The estimate still bounds what the emitter produces: six quads per
    // face set, twelve for `both`.
    let estimate = level.estimate_geometry();
    assert!(
        estimate.wall_quads >= 2 * MAX_VOID_WALL_QUADS,
        "two void walls need at least {} wall quads, got {}",
        2 * MAX_VOID_WALL_QUADS,
        estimate.wall_quads
    );

    // Every optional default is omitted from the serialized form.
    let encoded = serde_json::to_string(&level).expect("void level serializes");
    assert!(!encoded.contains("\"faces\":\"inward\""));
    assert!(!encoded.contains("\"solid\":true"));
    assert!(!encoded.contains("\"occludes\":true"));
    assert!(encoded.contains("\"faces\":\"outward\""));
    assert!(encoded.contains("\"solid\":false"));
}

// --------------------------------------------- 2026 raised geometry budgets

#[test]
fn test_the_raised_floor_area_budget_accepts_its_limit_and_rejects_one_over() {
    // One 8 000 m × 8 000 m room is exactly `MAX_LEVEL_FLOOR_AREA_M2`.
    let build = |depth: f32| {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "area_cap",
                "name": "Area Cap",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 8000.0, "depth": {depth},
                              "height": 3.5 }} ]
            }}"#
        ))
        .expect("the area cap level parses")
    };
    let at_limit = build(8000.0);
    assert_eq!(
        at_limit.estimate_geometry().floor_area_m2,
        MAX_LEVEL_FLOOR_AREA_M2,
        "8 000 × 8 000 m must be exactly the raised area budget"
    );
    crate::loader::validate_level(&at_limit)
        .expect("a level at the raised floor-area budget must validate");
    let over = build(8001.0);
    let error = crate::loader::validate_level(&over)
        .expect_err("one square metre over the area budget must be rejected");
    assert!(error.contains("floor area is too large"), "{error}");
}

#[test]
fn test_the_raised_vertex_budget_rejects_an_over_budget_estimate() {
    // The estimate's floor grid is cut by every patch and region edge, so a
    // small room with the patch cap is a geometry-complexity case while its
    // floor area stays tiny: exactly the shape the vertex budget refuses.
    let patches: Vec<String> = (0..crate::level::MAX_LEVEL_FLOOR_PATCHES)
        .map(|index| {
            format!(
                r#"{{ "x": {}, "z": {}, "width": 0.4, "depth": 0.4,
                     "material": "core:carpet_beige_01" }}"#,
                index % 100,
                index / 100
            )
        })
        .collect();
    let level = LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "vertex_cap",
            "name": "Vertex Cap",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 1000.0, "depth": 1000.0,
                          "height": 3.5 }} ],
            "floor_patches": [{}]
        }}"#,
        patches.join(",")
    ))
    .expect("the vertex cap level parses");
    let estimate = level.estimate_geometry();
    assert!(
        estimate.total_vertices > MAX_LEVEL_VERTICES,
        "the cut-line estimate must exceed the raised vertex budget, got {}",
        estimate.total_vertices
    );
    assert!(
        estimate.floor_area_m2 <= MAX_LEVEL_FLOOR_AREA_M2,
        "the case must be refused for its vertex estimate, not its area"
    );
    let error = crate::loader::validate_level(&level)
        .expect_err("an over-budget geometry estimate must be rejected");
    assert!(error.contains("geometry is too complex"), "{error}");
}

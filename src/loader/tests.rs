//! Unit tests for level packs, validation and the level manager.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unwrap_in_result,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_clone,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::suboptimal_flops,
    clippy::unwrap_used,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use super::*;
use crate::level::{RoomDef, WallDef};
use crate::test_support::{assert_exact, assert_exact_array};

#[test]
fn route_sampling_rejects_counts_that_lose_adjacent_float_indices() {
    let error = route_path_is_clear(
        "oversized route",
        &[],
        &crate::collision_index::CollisionIndex::empty(),
        &crate::level::WalkableFloor::default(),
        glam::Vec3::ZERO,
        (1.0e10, 0.0),
        0.2,
        1.0,
        0.2,
    )
    .expect_err("reject the count before running the interpolation loop");
    assert!(
        error.contains("exact interpolation sample budget"),
        "expected a sample-count error, got {error}"
    );
}

#[test]
fn test_validate_level_success() {
    let level = LevelDef {
        sky: None,
        environment: None,
        weather: None,
        global_illuminators: Vec::new(),
        doors: Vec::new(),
        effects: Vec::new(),
        routes: Vec::new(),
        format_version: 3,
        id: "test_level".into(),
        name: "Test Level".into(),
        author: "Author".into(),
        rooms: vec![RoomDef {
            ceiling: crate::level::CeilingProfileDef::Flat,
            floor_y: 0.0,
            x: 0.0,
            z: 0.0,
            width: 20.0,
            depth: 20.0,
            height: 3.5,
            material: None,
            shine: None,
            ceiling_material: None,
            ceiling_shine: None,
            ceiling_tile_origin: None,
            ceiling_tile_rotation_degrees: None,
        }],
        spawn: crate::level::SpawnDef {
            x: 5.0,
            z: 5.0,
            yaw_degrees: 0.0,
        },
        defaults: crate::level::LevelDefaults::default(),
        walls: vec![WallDef {
            x: 10.0,
            y: 0.0,
            z: 10.0,
            width: 2.0,
            depth: 1.0,
            height: Some(3.5),
            faces: HashMap::new(),
            openings: Vec::new(),
            material: None,
            shine: None,
            face_shine: HashMap::new(),
        }],
        floor_patches: vec![],
        floor_regions: vec![],
        water: vec![],
        ladders: vec![],
        ramps: vec![],
        stairs: vec![],
        half_walls: vec![],
        columns: vec![],
        archways: vec![],
        guardrails: vec![],
        thresholds: vec![],
        baseboards: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
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
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_level_invalid_version() {
    let level = LevelDef {
        sky: None,
        environment: None,
        weather: None,
        global_illuminators: Vec::new(),
        doors: Vec::new(),
        effects: Vec::new(),
        routes: Vec::new(),
        format_version: 1,
        id: "test".into(),
        name: "Test".into(),
        author: String::new(),
        rooms: vec![],
        spawn: crate::level::SpawnDef {
            x: 0.0,
            z: 0.0,
            yaw_degrees: 0.0,
        },
        defaults: crate::level::LevelDefaults::default(),
        walls: vec![],
        floor_patches: vec![],
        floor_regions: vec![],
        water: vec![],
        ladders: vec![],
        ramps: vec![],
        stairs: vec![],
        half_walls: vec![],
        columns: vec![],
        archways: vec![],
        guardrails: vec![],
        thresholds: vec![],
        baseboards: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
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
    assert!(validate_level(&level).is_err());
}

#[test]
fn test_validate_sky_bounds_and_identifiers() {
    let base = |sky: &str| {
        format!(
            r#"{{
                "format_version": 3,
                "id": "sky_case",
                "name": "Sky Case",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }} ],
                "sky": {sky}
            }}"#
        )
    };
    let level = LevelDef::from_json(&base(
        r#"{ "texture": "outdoor:tex_sky_stars_01", "brightness": 2.0, "ambient": 1.0 }"#,
    ))
    .expect("valid json");
    assert!(validate_level(&level).is_ok());

    for (sky, expected) in [
        (
            r#"{ "texture": "outdoor:tex_sky_stars_01", "brightness": -1.0 }"#,
            "brightness",
        ),
        (
            r#"{ "texture": "outdoor:tex_sky_stars_01", "brightness": 9.0 }"#,
            "brightness",
        ),
        (
            r#"{ "texture": "outdoor:tex_sky_stars_01", "ambient": 2.0 }"#,
            "ambient",
        ),
        (
            r#"{ "texture": "outdoor:tex_sky_stars_01", "ambient": -0.5 }"#,
            "ambient",
        ),
        (
            r#"{ "texture": "", "brightness": 1.0 }"#,
            "names no texture",
        ),
        (
            r#"{ "texture": "not a:valid id", "brightness": 1.0 }"#,
            "well-formed",
        ),
    ] {
        let sky_level = LevelDef::from_json(&base(sky)).expect("valid json");
        let error = validate_level(&sky_level).expect_err("invalid sky must be rejected");
        assert!(
            error.contains(expected),
            "expected `{expected}` in: {error}"
        );
    }
}

#[test]
fn test_validate_level_preserves_overlapping_geometry() {
    // Overlapping walls and rooms are explicitly legal
    let level = LevelDef {
        sky: None,
        environment: None,
        weather: None,
        global_illuminators: Vec::new(),
        doors: Vec::new(),
        effects: Vec::new(),
        routes: Vec::new(),
        format_version: 3,
        id: "overlap".into(),
        name: "Overlap".into(),
        author: String::new(),
        rooms: vec![
            RoomDef {
                ceiling: crate::level::CeilingProfileDef::Flat,
                floor_y: 0.0,
                x: 0.0,
                z: 0.0,
                width: 10.0,
                depth: 10.0,
                height: 3.5,
                material: None,
                shine: None,
                ceiling_material: None,
                ceiling_shine: None,
                ceiling_tile_origin: None,
                ceiling_tile_rotation_degrees: None,
            },
            RoomDef {
                ceiling: crate::level::CeilingProfileDef::Flat,
                floor_y: 0.0,
                x: 5.0,
                z: 5.0,
                width: 10.0,
                depth: 10.0,
                height: 3.5,
                material: None,
                shine: None,
                ceiling_material: None,
                ceiling_shine: None,
                ceiling_tile_origin: None,
                ceiling_tile_rotation_degrees: None,
            },
        ],
        spawn: crate::level::SpawnDef {
            x: 1.0,
            z: 1.0,
            yaw_degrees: 0.0,
        },
        defaults: crate::level::LevelDefaults::default(),
        walls: vec![
            WallDef {
                x: 2.0,
                y: 0.0,
                z: 2.0,
                width: 4.0,
                depth: 4.0,
                height: Some(3.5),
                faces: HashMap::new(),
                openings: Vec::new(),
                material: None,
                shine: None,
                face_shine: HashMap::new(),
            },
            WallDef {
                x: 3.0,
                y: 0.0,
                z: 3.0,
                width: 4.0,
                depth: 4.0,
                height: Some(3.5),
                faces: HashMap::new(),
                openings: Vec::new(),
                material: None,
                shine: None,
                face_shine: HashMap::new(),
            },
        ],
        floor_patches: vec![],
        floor_regions: vec![],
        water: vec![],
        ladders: vec![],
        ramps: vec![],
        stairs: vec![],
        half_walls: vec![],
        columns: vec![],
        archways: vec![],
        guardrails: vec![],
        thresholds: vec![],
        baseboards: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
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
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_parse_materials_json() {
    let json = r#"{
        "materials": {
            "pack:custom_wall": {
                "texture": "textures/my_wall.png",
                "tile_metres": 3.0,
                "tint": [0.5, 0.6, 0.7]
            },
            "pack:carpet_gray": "textures/carpet.png"
        }
    }"#;
    let map = parse_materials_json(Some(json));
    let wall = map.get("pack:custom_wall").expect("object form");
    assert_eq!(wall.texture, "textures/my_wall.png");
    assert_eq!(wall.tile_metres, Some(3.0));
    assert_eq!(wall.tint, Some([0.5, 0.6, 0.7]));
    let carpet = map.get("pack:carpet_gray").expect("string form");
    assert_eq!(carpet.texture, "textures/carpet.png");
    assert_eq!(carpet.tile_metres(), crate::assets::DEFAULT_TILE_METRES);
    assert_eq!(carpet.tint(), crate::materials::DEFAULT_TINT);
    assert!(parse_materials_json(None).is_empty());
    assert!(parse_materials_json(Some("not json")).is_empty());
}

#[test]
fn test_missing_pack_materials_use_the_diagnostic_texture_with_an_error() {
    let level = LevelDef {
        sky: None,
        environment: None,
        weather: None,
        global_illuminators: Vec::new(),
        doors: Vec::new(),
        effects: Vec::new(),
        routes: Vec::new(),
        format_version: 3,
        id: "fallback_test".into(),
        name: "Fallback Test".into(),
        author: String::new(),
        rooms: vec![],
        spawn: crate::level::SpawnDef {
            x: 0.0,
            z: 0.0,
            yaw_degrees: 0.0,
        },
        defaults: crate::level::LevelDefaults {
            wall: "pack:missing_wall".into(),
            floor: "pack:missing_carpet".into(),
            ceiling: "core:ceiling_panel_01".into(),
            wall_shine: None,
            floor_shine: None,
            ceiling_shine: None,
        },
        walls: vec![],
        floor_patches: vec![],
        floor_regions: vec![],
        water: vec![],
        ladders: vec![],
        ramps: vec![],
        stairs: vec![],
        half_walls: vec![],
        columns: vec![],
        archways: vec![],
        guardrails: vec![],
        thresholds: vec![],
        baseboards: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
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

    // No custom textures supplied: the two `pack:` materials resolve to the
    // one diagnostic pattern with a named error, the built-in ceiling still
    // resolves from the catalog.
    let pack = crate::materials::PackMaterials::new("fallback_pack", None, HashMap::new());
    let catalog = crate::assets::AssetCatalog::load_default();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let mut cache = crate::materials::TextureCache::new();
    let table =
        crate::materials::resolve_materials(&level, &catalog, Some(&pack), Some(&root), &mut cache);

    let wall = table.entry_of("pack:missing_wall").expect("wall entry");
    assert_eq!(wall.origin, crate::materials::TextureOrigin::Missing);
    assert_eq!(wall.texture_key, crate::materials::MISSING_TEXTURE_KEY);
    let error = wall.error.as_deref().expect("named error");
    assert!(error.contains("pack:missing_wall"), "error: {error}");
    let ceiling = table
        .entry_of("core:ceiling_panel_01")
        .expect("ceiling entry");
    assert_eq!(ceiling.origin, crate::materials::TextureOrigin::Catalog);
    let image = wall.image.as_ref().expect("diagnostic image");
    assert_eq!((image.width, image.height), (64, 64));
    assert_eq!(
        image.rgba.len(),
        usize::try_from(64_i32 * 64_i32 * 4_i32).expect("fixture integer fits usize")
    );
}

/// The built-in material variants resolve to distinct images, so a level
/// that asks for the water-damaged surfaces really draws them.
#[test]
fn test_damaged_material_variants_resolve() {
    let checksum = |image: &RawImage| -> u64 {
        image
            .rgba
            .iter()
            .enumerate()
            .fold(0x811c_9dc5u64, |hash, (index, byte)| {
                (hash
                    ^ (u64::from(*byte) + u64::try_from(index).expect("fixture integer fits u64")))
                .wrapping_mul(0x0100_0000_01b3)
            })
    };
    // (maintained id, damaged id)
    let variants = [
        ("core:wallpaper_yellow_01", "core:wallpaper_stained_01"),
        ("core:carpet_beige_01", "core:carpet_damp_01"),
        ("core:ceiling_panel_01", "core:ceiling_stained_01"),
    ];
    let catalog = crate::assets::AssetCatalog::load_default();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    for (maintained, damaged) in variants {
        let sample = |material: &str| {
            let mut level = LevelDef::from_json(
                r#"{"format_version": 3, "id": "x", "name": "x", "spawn": {"x": 0.0, "z": 0.0}}"#,
            )
            .expect("minimal level");
            level.defaults.wall = material.into();
            level.defaults.floor = material.into();
            level.defaults.ceiling = material.into();
            let mut cache = crate::materials::TextureCache::new();
            let table = crate::materials::resolve_materials(
                &level,
                &catalog,
                None,
                Some(&root),
                &mut cache,
            );
            assert!(
                table.errors().is_empty(),
                "{material} errors: {:?}",
                table.errors()
            );
            let image = table
                .entry_of(material)
                .and_then(|entry| entry.image.clone())
                .expect("decoded image");
            (image.width, image.height, checksum(&image))
        };
        let plain = sample(maintained);
        let worn = sample(damaged);
        assert_ne!(
            plain, worn,
            "{damaged} must resolve to its own PNG, not {maintained}'s"
        );
    }
}

fn level_from_rooms_json(rooms_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "budget_test",
            "name": "Budget Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": {rooms_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

#[test]
fn test_validate_accepts_moderately_large_level() {
    // 10 rooms of 100x100 m = 100,000 m^2, comfortably under the budget.
    let rooms: Vec<String> = (0_i32..10_i32)
        .map(|i| {
            format!(
                r#"{{ "x": {}, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }}"#,
                crate::test_support::exact_f32(i) * 100.0
            )
        })
        .collect();
    let level = level_from_rooms_json(&format!("[{}]", rooms.join(",")));
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_rejects_pathological_huge_room() {
    // 8001 x 8001 m is 64 016 001 m^2: past the raised 64 000 000 m^2 budget
    // (the historical budget was 16 000 000 m^2) while each edge stays inside
    // the room-extent cap, so the rejection names the floor area.
    let level = level_from_rooms_json(
        r#"[{ "x": 0.0, "z": 0.0, "width": 8001.0, "depth": 8001.0, "height": 3.5 }]"#,
    );
    let err = validate_level(&level).expect_err("huge room must be rejected");
    assert!(
        err.contains("floor area") || err.contains("complex"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_validate_permits_overlapping_rooms_within_budget() {
    // 50 fully-overlapping 100x100 m rooms = 500,000 m^2: overlapping is
    // intentional and allowed, and the total is within budget.
    let rooms: Vec<String> = (0_i32..50_i32)
        .map(|_| {
            r#"{ "x": 0.0, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }"#.to_string()
        })
        .collect();
    let level = level_from_rooms_json(&format!("[{}]", rooms.join(",")));
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_rejects_huge_geometry_without_overflowing() {
    // Finite but absurd dimensions must be handled by saturating arithmetic
    // (no panic/overflow) and rejected.
    let level = level_from_rooms_json(&format!(
        r#"[{{ "x": 0.0, "z": 0.0, "width": {}, "depth": {}, "height": 3.5 }}]"#,
        f32::MAX,
        f32::MAX
    ));
    assert!(validate_level(&level).is_err());
}

// ------------------------------------------------------- vertical geometry

/// A level with one room plus whatever extra JSON keys the test supplies.
fn vertical_level(room_extra: &str, level_extra: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "vertical",
            "name": "Vertical",
            "spawn": {{ "x": 4.0, "z": 4.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0{room_extra} }}]{level_extra}
        }}"#
    ))
    .expect("valid vertical json")
}

#[test]
fn test_validate_accepts_flat_and_vertical_rooms() {
    // Flat: no elevation, no profile.
    assert!(validate_level(&vertical_level("", "")).is_ok());
    // Elevated with a gable ceiling and a recessed region.
    let level = vertical_level(
        r#", "floor_y": 2.0, "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 1.5 }"#,
        r#", "floor_regions": [
            { "x": 1.0, "z": 1.0, "width": 3.0, "depth": 2.0, "offset_y": -1.0 }
        ]"#,
    );
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_rejects_non_finite_room_elevation() {
    let level = vertical_level(r#", "floor_y": 1.0e30"#, "");
    // Finite but absurd is still finite: the elevation itself is accepted,
    // but an infinite one must not be.
    assert!(validate_level(&level).is_ok());
    let very_high_level = vertical_level(r#", "floor_y": 1.0e38"#, "");
    let infinite_level = {
        // serde_json cannot express infinity, so build it programmatically.
        let mut mutated_level = very_high_level;
        mutated_level.rooms.push(crate::level::RoomDef {
            x: 20.0,
            z: 0.0,
            width: 4.0,
            depth: 4.0,
            height: 3.0,
            floor_y: f32::INFINITY,
            ceiling: crate::level::CeilingProfileDef::Flat,
            material: None,
            shine: None,
            ceiling_material: None,
            ceiling_shine: None,
            ceiling_tile_origin: None,
            ceiling_tile_rotation_degrees: None,
        });
        mutated_level
    };
    let err = validate_level(&infinite_level).expect_err("non-finite elevation must be rejected");
    assert!(err.contains("floor elevation"), "unexpected error: {err}");
}

#[test]
fn test_validate_rejects_impossible_gable_definitions() {
    for (extra, expected) in [
        (
            r#", "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 0.0 }"#,
            "above the eave",
        ),
        (
            r#", "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": -2.0 }"#,
            "above the eave",
        ),
        (
            r#", "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 51.0 }"#,
            "maximum limit",
        ),
    ] {
        let level = vertical_level(extra, "");
        let err = validate_level(&level).expect_err("impossible gable must be rejected");
        assert!(err.contains(expected), "unexpected error: {err}");
    }
    // A non-finite rise cannot come from JSON, so it is built directly.
    let mut level = vertical_level("", "");
    level.rooms.push(crate::level::RoomDef {
        x: 20.0,
        z: 0.0,
        width: 4.0,
        depth: 4.0,
        height: 3.0,
        floor_y: 0.0,
        ceiling: crate::level::CeilingProfileDef::Gable {
            ridge: crate::level::WallAxis::X,
            ridge_rise: f32::NAN,
        },
        material: None,
        shine: None,
        ceiling_material: None,
        ceiling_shine: None,
        ceiling_tile_origin: None,
        ceiling_tile_rotation_degrees: None,
    });
    let err = validate_level(&level).expect_err("NaN ridge must be rejected");
    assert!(err.contains("ridge rise"), "unexpected error: {err}");
}

// ------------------------------------------------------- surface shine

/// A level that authors `shine` on every carrier, with `value` spliced in.
fn shine_level(value: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "shine",
            "name": "Shine",
            "spawn": {{ "x": 4.0, "z": 4.0 }},
            "defaults": {{ "wall": "core:wallpaper_yellow_01",
                          "floor": "core:carpet_beige_01",
                          "ceiling": "core:ceiling_panel_01",
                          "wall_shine": {value}, "floor_shine": {value},
                          "ceiling_shine": {value} }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                       "material": "core:linoleum_polished_01", "shine": {value},
                       "ceiling_material": "core:ceiling_panel_01",
                       "ceiling_shine": {value} }}],
            "walls": [
                {{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3, "height": 3.0,
                   "material": "core:metal_brushed_01", "shine": {value},
                   "face_shine": {{ "south": {value} }},
                   "openings": [
                       {{ "kind": "window", "offset": 1.0, "width": 1.0,
                          "height": 1.0, "sill": 1.0,
                          "glass": "core:glass_window_clear_01",
                          "glass_shine": {value} }}
                   ] }}
            ],
            "floor_patches": [
                {{ "x": 0.0, "z": 0.0, "width": 1.0, "depth": 1.0,
                   "material": "core:carpet_damp_01", "shine": {value} }}
            ],
            "floor_regions": [
                {{ "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5,
                   "material": "core:pool_tile_basin_01", "shine": {value},
                   "edge_material": "core:pool_tile_wall_01", "edge_shine": {value} }}
            ]
        }}"#
    ))
    .expect("shine level parses")
}

#[test]
fn test_validate_accepts_a_level_without_any_shine() {
    // The shipped shape: no `shine` key anywhere. Every override is `None`.
    let level = vertical_level("", "");
    assert!(validate_level(&level).is_ok());
    assert!(level.defaults.wall_shine.is_none());
    assert!(level.rooms.iter().all(|room| room.shine.is_none()));

    // And the shipped demo, which mixes explicit and default shine.
    let demo = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("the shipped demo parses");
    assert!(validate_level(&demo).is_ok());
    assert_eq!(demo.floor_patches[0].shine, Some(0.05));
}

#[test]
fn test_validate_accepts_every_shine_value_in_range() {
    for value in ["0.0", "0.05", "0.5", "0.999", "1.0"] {
        let level = shine_level(value);
        assert!(
            validate_level(&level).is_ok(),
            "shine {value} must be accepted"
        );
    }
}

#[test]
fn test_validate_rejects_malformed_shine_by_surface_name() {
    for (value, expected) in [
        ("-0.1", "shine must be a finite number"),
        ("1.1", "shine must be a finite number"),
    ] {
        let err = validate_level(&shine_level(value))
            .expect_err("an out-of-range shine must be rejected");
        assert!(err.contains(expected), "unexpected error: {err}");
    }

    // A non-finite value cannot come from JSON, so it is built directly; the
    // loader still rejects it by name rather than letting a NaN reach the
    // renderer.
    let mut level = shine_level("0.5");
    level.floor_patches[0].shine = Some(f32::NAN);
    let err = validate_level(&level).expect_err("a NaN shine must be rejected");
    assert!(
        err.contains("Floor patch 0") && err.contains("shine"),
        "unexpected error: {err}"
    );

    // Malformed JSON (a string where a number belongs) is a load error, not a
    // panic: the level loader reports it and the level is skipped.
    let json = r#"{
        "format_version": 3, "id": "bad_shine", "name": "Bad Shine",
        "spawn": { "x": 0.0, "z": 0.0 },
        "floor_patches": [ { "x": 0.0, "z": 0.0, "width": 1.0, "depth": 1.0,
                             "material": "core:carpet_beige_01", "shine": "very" } ]
    }"#;
    assert!(LevelDef::from_json(json).is_err());
}

#[test]
fn test_validate_rejects_malformed_floor_regions() {
    for (region, expected) in [
        (
            r#"{ "x": 1.0, "z": 1.0, "width": 0.0, "depth": 2.0 }"#,
            "width and depth",
        ),
        (
            r#"{ "x": 20.0, "z": 20.0, "width": 2.0, "depth": 2.0 }"#,
            "outside every room",
        ),
        (
            r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "offset_y": 3.0 }"#,
            "at or above the ceiling",
        ),
        (
            r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "material": "" }"#,
            "non-empty",
        ),
    ] {
        let level = vertical_level("", &format!(r#", "floor_regions": [{region}]"#));
        let err = validate_level(&level).expect_err("malformed region must be rejected");
        assert!(err.contains(expected), "unexpected error: {err}");
    }
}

#[test]
fn test_validate_rejects_ceiling_decals_on_a_gable() {
    let level = vertical_level(
        r#", "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 1.5 }"#,
        r#", "decals": [
            { "x": 4.0, "y": 4.5, "z": 4.0, "width": 1.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "ceiling" }
        ]"#,
    );
    let err = validate_level(&level).expect_err("sloped ceiling decal must be rejected");
    assert!(err.contains("gable ceiling"), "unexpected error: {err}");

    // The same decal on a flat ceiling (elevated or not) stays valid.
    let flat_ceiling_level = vertical_level(
        r#", "floor_y": 2.0"#,
        r#", "decals": [
            { "x": 4.0, "y": 5.0, "z": 4.0, "width": 1.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "ceiling" }
        ]"#,
    );
    assert!(validate_level(&flat_ceiling_level).is_ok());
}

#[test]
fn test_validate_rejects_a_decal_that_straddles_a_height_change() {
    // The decal is centred beside the recess but its 2 m width reaches over
    // the edge, so it cannot lie on one plane.
    let level = vertical_level(
        "",
        r#", "floor_regions": [
            { "x": 4.0, "z": 3.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0 }
        ],
        "decals": [
            { "x": 4.0, "y": 0.0, "z": 5.0, "width": 2.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "floor" }
        ]"#,
    );
    let err = validate_level(&level).expect_err("straddling decal must be rejected");
    assert!(err.contains("height change"), "unexpected error: {err}");

    // The same decal fully inside the room floor is fine.
    let contained_decal_level = vertical_level(
        "",
        r#", "floor_regions": [
            { "x": 4.0, "z": 3.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0 }
        ],
        "decals": [
            { "x": 2.0, "y": 0.0, "z": 5.0, "width": 2.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "floor" }
        ]"#,
    );
    assert!(validate_level(&contained_decal_level).is_ok());
}

#[test]
fn test_validate_rejects_too_many_floor_patches() {
    let patches: Vec<String> = (0..=crate::level::MAX_LEVEL_FLOOR_PATCHES)
        .map(|_| {
            r#"{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0, "material": "core:carpet_beige_01" }"#
                .to_string()
        })
        .collect();
    let level = vertical_level(
        "",
        &format!(r#", "floor_patches": [{}]"#, patches.join(",")),
    );
    let err = validate_level(&level).expect_err("too many patches must be rejected");
    assert!(
        err.contains("too many floor patches"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_validate_rejects_too_many_wall_openings() {
    let openings: Vec<String> = (0..=crate::level::MAX_WALL_OPENINGS)
        .map(|_| r#"{ "kind": "door", "offset": 0.0, "width": 0.5, "height": 1.0 }"#.to_string())
        .collect();
    let level = LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "openings",
            "name": "Openings",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }}],
            "walls": [{{
                "x": 0.0, "z": 2.0, "width": 4.0, "depth": 0.2,
                "openings": [{}]
            }}]
        }}"#,
        openings.join(",")
    ))
    .expect("level parses");
    let err = validate_level(&level).expect_err("too many openings must be rejected");
    assert!(err.contains("too many openings"), "unexpected error: {err}");
}

#[test]
fn test_validate_rejects_too_many_floor_regions() {
    let regions: Vec<String> = (0..=crate::level::MAX_LEVEL_FLOOR_REGIONS)
        .map(|_| r#"{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0 }"#.to_string())
        .collect();
    let level = vertical_level(
        "",
        &format!(r#", "floor_regions": [{}]"#, regions.join(",")),
    );
    let err = validate_level(&level).expect_err("too many regions must be rejected");
    assert!(
        err.contains("too many floor regions"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_vertical_room_fields_round_trip() {
    let level = vertical_level(
        r#", "floor_y": -1.5, "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 }"#,
        r#", "floor_regions": [
            { "x": 1.0, "z": 1.0, "width": 3.0, "depth": 2.0, "offset_y": -0.75,
              "material": "core:carpet_damp_01" }
        ]"#,
    );
    let json = serde_json::to_string(&level).expect("serializes");
    let reparsed = LevelDef::from_json(&json).expect("round trips");
    let room = reparsed.room_iter().next().expect("one room");
    assert_exact(room.floor_y, -1.5);
    assert_exact(room.ceiling.ridge_rise_m(), 2.0);
    assert_eq!(room.ceiling.ridge_axis(), Some(crate::level::WallAxis::X));
    assert_eq!(reparsed.floor_regions.len(), 1);
    assert_exact(reparsed.floor_regions[0].offset(), -0.75);
    assert_eq!(
        reparsed.floor_regions[0].material.as_deref(),
        Some("core:carpet_damp_01")
    );
    assert!(validate_level(&reparsed).is_ok());
}

fn level_with_opening_json(opening_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "opening_test",
            "name": "Opening Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }} ],
            "walls": [{{
                "x": 0.0, "z": 4.8, "width": 10.0, "depth": 0.4, "height": 3.5,
                "openings": [{opening_json}]
            }}]
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

fn level_with_props_json(props_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "props_test",
            "name": "Props Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }} ],
            "props": {props_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

#[test]
fn test_validate_accepts_valid_door_and_window() {
    let door = level_with_opening_json(
        r#"{ "kind": "door", "offset": 4.0, "width": 1.0, "height": 2.1 }"#,
    );
    assert!(validate_level(&door).is_ok());

    let window = level_with_opening_json(
        r#"{ "kind": "window", "offset": 2.0, "width": 2.0, "height": 1.0, "sill": 1.2 }"#,
    );
    assert!(validate_level(&window).is_ok());
}

#[test]
fn test_validate_rejects_door_beyond_wall() {
    let level = level_with_opening_json(
        r#"{ "kind": "door", "offset": 9.5, "width": 1.0, "height": 2.1 }"#,
    );
    let err = validate_level(&level).expect_err("door must not extend past the wall");
    assert!(
        err.starts_with("Door opening extends beyond this wall"),
        "unexpected error: {err}"
    );
    assert!(err.contains("wall 0"), "missing wall index: {err}");
}

#[test]
fn test_validate_rejects_window_and_unknown_opening_beyond_wall() {
    let window = level_with_opening_json(
        r#"{ "kind": "window", "offset": 9.0, "width": 1.5, "height": 1.0, "sill": 1.0 }"#,
    );
    let err = validate_level(&window).expect_err("window must not extend past the wall");
    assert!(
        err.starts_with("Window opening extends beyond this wall"),
        "unexpected error: {err}"
    );

    let vent = level_with_opening_json(
        r#"{ "kind": "vent", "offset": 9.0, "width": 2.0, "height": 0.4 }"#,
    );
    let vent_error = validate_level(&vent).expect_err("vent must not extend past the wall");
    assert!(
        vent_error.starts_with("Opening extends beyond this wall"),
        "unexpected error: {vent_error}"
    );
}

#[test]
fn test_validate_rejects_invalid_opening_numbers() {
    let negative_sill = level_with_opening_json(
        r#"{ "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0, "sill": -0.5 }"#,
    );
    let err = validate_level(&negative_sill).expect_err("negative sill is invalid");
    assert!(
        err.contains("cannot have a negative sill height"),
        "unexpected error: {err}"
    );

    let negative_offset = level_with_opening_json(
        r#"{ "kind": "door", "offset": -1.0, "width": 1.0, "height": 2.1 }"#,
    );
    let negative_offset_error =
        validate_level(&negative_offset).expect_err("negative offset is invalid");
    assert!(
        negative_offset_error.contains("starts before the wall"),
        "unexpected error: {negative_offset_error}"
    );

    let zero_width = level_with_opening_json(
        r#"{ "kind": "door", "offset": 1.0, "width": 0.0, "height": 2.1 }"#,
    );
    let zero_width_error = validate_level(&zero_width).expect_err("zero width is invalid");
    assert!(
        zero_width_error.contains("must have a positive width and height"),
        "unexpected error: {zero_width_error}"
    );
}

#[test]
fn test_validate_accepts_sunk_and_intersecting_props() {
    // A prop sunk into the floor and a prop embedded in a wall are both
    // intentional and must not be rejected.
    let level = level_with_props_json(
        r#"[
            { "model": "core:rug", "x": 2.0, "y": -0.02, "z": 2.0 },
            { "model": "core:bookshelf", "x": 0.0, "y": 0.0, "z": 4.8, "scale": 1.2, "solid": true }
        ]"#,
    );
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_rejects_invalid_props() {
    let empty_model = level_with_props_json(r#"[{ "model": "  ", "x": 0.0, "z": 0.0 }]"#);
    assert!(
        validate_level(&empty_model)
            .expect_err("empty model id is invalid")
            .contains("model id")
    );

    let non_finite = level_with_props_json(r#"[{ "model": "core:crate", "x": 1.0, "z": 0.0 }]"#);
    let mut infinite_position = non_finite;
    infinite_position.props[0].x = f32::INFINITY;
    assert!(
        validate_level(&infinite_position)
            .expect_err("infinite x is invalid")
            .contains("finite")
    );

    let bad_scale =
        level_with_props_json(r#"[{ "model": "core:crate", "scale": 0.0, "x": 0.0, "z": 0.0 }]"#);
    assert!(
        validate_level(&bad_scale)
            .expect_err("zero scale is invalid")
            .contains("scale must be positive")
    );

    let bad_size = level_with_props_json(
        r#"[{ "model": "core:crate", "x": 0.0, "z": 0.0, "size": [1.0, -1.0, 1.0] }]"#,
    );
    assert!(
        validate_level(&bad_size)
            .expect_err("negative size is invalid")
            .contains("size must contain positive finite numbers")
    );
}

#[test]
fn test_parse_hex_color() {
    assert_eq!(
        parse_hex_color("#6b5f4a"),
        Some([
            crate::test_support::exact_f32(0x006b_i32) / 255.0,
            crate::test_support::exact_f32(0x005f_i32) / 255.0,
            crate::test_support::exact_f32(0x004a_i32) / 255.0
        ])
    );
    // The leading '#' is optional and plain greys parse too.
    assert_eq!(parse_hex_color("8a8a8a"), parse_hex_color("#8A8A8A"));
    assert_eq!(parse_hex_color("#fff"), None);
    assert_eq!(parse_hex_color("not-a-colour"), None);
}

#[test]
fn test_prop_catalog_from_json_str() {
    let json = r##"{
        "format_version": 3,
        "assets": [
            { "id": "core:couch", "display_name": "Couch", "asset_class": "core",
              "asset_type": "prop", "category": "Furniture",
              "size": [2.0, 0.9, 0.9], "color": "#6b5f4a", "solid": true },
            { "id": "core:lamp", "asset_class": "core",
              "asset_type": "prop" }
        ]
    }"##;
    let catalog = PropCatalog::from_json_str(json).expect("valid catalog");
    assert_eq!(catalog.len(), 2);
    assert!(!catalog.is_empty());

    let couch = catalog.get("core:couch");
    assert_eq!(couch.name, "Couch");
    assert_eq!(couch.category, "Furniture");
    assert_exact_array(couch.size, [2.0, 0.9, 0.9]);
    assert_exact_array(couch.color, parse_hex_color("#6b5f4a").unwrap());
    assert_eq!(couch.model, None);
    assert!(couch.solid);

    // Missing fields default to the neutral values.
    let lamp = catalog.get("core:lamp");
    assert_eq!(lamp.name, "core:lamp");
    assert_eq!(lamp.category, "Other");
    assert_exact_array(lamp.size, crate::level::PROP_FALLBACK_SIZE);
    assert_exact_array(lamp.color, parse_hex_color("#8a8a8a").unwrap());
    assert!(!lamp.solid);

    assert!(PropCatalog::from_json_str("not json").is_err());
}

#[test]
fn test_prop_catalog_falls_back_for_unknown_model() {
    let catalog = PropCatalog::builtin();
    assert!(catalog.is_empty());
    let fallback = catalog.get("core:does_not_exist");
    assert_eq!(fallback.name, "core:does_not_exist");
    assert_eq!(fallback.category, "Other");
    assert_exact_array(fallback.size, crate::level::PROP_FALLBACK_SIZE);
    assert_exact_array(fallback.color, parse_hex_color("#8a8a8a").unwrap());
    assert!(!fallback.solid);
}

#[test]
fn test_shipped_prop_catalog_parses() {
    let catalog = PropCatalog::from_json_str(include_str!("../../assets/catalog.json"))
        .expect("shipped props.json must parse");
    assert!(catalog.len() >= 16, "expected at least 16 props");
    let couch = catalog.get("core:couch");
    assert_eq!(couch.name, "Couch");
    assert!(couch.size.iter().all(|v| *v > 0.0));
    assert!(couch.solid);
}

#[test]
fn test_shipped_test_room_demonstrates_openings_and_props() {
    let json = include_str!("../../tests/fixtures/levels/test_room.json");
    let level = LevelDef::from_json(json).expect("valid test_room json");
    validate_level(&level).expect("the shipped sample level validates");

    let kinds: Vec<&str> = level
        .walls
        .iter()
        .flat_map(|wall| wall.openings.iter().map(|opening| opening.kind.as_str()))
        .collect();
    assert!(kinds.contains(&"door"), "the sample level shows a doorway");
    assert!(kinds.contains(&"window"), "the sample level shows a window");
    assert!(
        !level.props.is_empty(),
        "the sample level shows at least one prop"
    );

    // The sample is also a real geometry exercise: openings and props must
    // produce drawable batches.
    let mesh = crate::render::build_level_geometry(&level);
    assert!(mesh.batches.wall_batch.count > 0_i32);
    assert!(mesh.batches.prop_batch.count > 0_i32);
}

/// The official showcase is the level the README sends a new visitor to, so its
/// promises are pinned here: one continuous route that exercises the office
/// family, the openings, a coloured transition, a real elevation change, the
/// recessed pool and an external decal sheet.
#[test]
fn test_the_official_demo_exercises_every_showcased_feature() {
    let json = include_str!("../../assets/levels/places_demo.json");
    let level = LevelDef::from_json(json).expect("valid places_demo json");
    validate_level(&level).expect("the official demo validates");

    // Openings: doorways are the route, a window looks between lighting
    // families, and a passage joins the stair hall to the pool deck.
    let kinds: std::collections::HashSet<&str> = level
        .walls
        .iter()
        .flat_map(|wall| wall.openings.iter().map(|opening| opening.kind.as_str()))
        .collect();
    for kind in ["door", "window", "passage"] {
        assert!(kinds.contains(kind), "the demo must cut a {kind}");
    }
    // Every opening the player walks through is wide enough to walk through.
    for wall in &level.walls {
        for opening in &wall.openings {
            if opening.sill <= f32::EPSILON && opening.reaches_floor() {
                assert!(
                    opening.width >= 1.0,
                    "a walk-through opening is {} m wide",
                    opening.width
                );
            }
        }
    }

    // Vertical: two floor elevations one walkable staircase apart, a recessed
    // basin and a raised landing, all built from floor regions.
    let elevations: std::collections::BTreeSet<String> = level
        .room_iter()
        .map(|room| format!("{:.3}", room.floor_y))
        .collect();
    assert_eq!(
        elevations.len(),
        3,
        "the demo steps between three floor elevations, found {elevations:?}"
    );
    let recessed = level
        .floor_regions
        .iter()
        .filter(|region| region.offset() <= -1.0)
        .count();
    assert!(recessed >= 1, "the empty pool basin is a real recess");
    let raised = level
        .floor_regions
        .iter()
        .filter(|region| region.offset() > 0.0)
        .count();
    assert!(
        raised >= 5,
        "the stair and the landing platform are raised regions"
    );
    for region in &level.floor_regions {
        assert!(
            region.edge_material.is_some(),
            "a region in the demo must name its transition material"
        );
    }

    // Lighting: warm office, one strongly coloured transition and a cool pool,
    // so the demo can never silently collapse to a single colour temperature.
    let colours: std::collections::BTreeSet<String> = level
        .ceiling_lights
        .iter()
        .map(|light| {
            light.color.map_or_else(
                || "default".to_string(),
                |color| {
                    let (r, g, b) = (color.r, color.g, color.b);
                    format!("{:.2},{:.2},{:.2}", r, g, b)
                },
            )
        })
        .collect();
    assert!(
        colours.len() >= 4,
        "the demo shows several lighting conditions, found {colours:?}"
    );

    // The pool family, the office family and the external decal sheets are all
    // real placements, and every solid prop authors the box it collides with.
    let placed: std::collections::HashSet<&str> =
        level.props.iter().map(|prop| prop.model.as_str()).collect();
    for id in [
        "core:desk",
        "core:chair",
        "core:pool_ladder",
        "core:pool_guardrail_straight",
        "core:pool_curtain_straight",
        "core:pool_table",
    ] {
        assert!(placed.contains(id), "the demo must place {id}");
    }
    for prop in &level.props {
        if prop.solid {
            assert!(
                prop.size.is_some(),
                "solid prop {} must author its collision size",
                prop.model
            );
        }
    }
    let decals: Vec<&str> = level
        .decals
        .iter()
        .map(|decal| decal.material.as_str())
        .collect();
    assert!(
        decals.contains(&"core:decal_no_diving_01"),
        "the demo ends the pool with the external NO DIVING sign"
    );
    assert!(
        decals.contains(&"core:decal_stripes_01"),
        "the demo marks the step up with the external hazard sheet"
    );
    assert!(
        decals.contains(&"core:decal_ceiling_vent_01"),
        "the demo places the external ceiling-vent sheet"
    );

    // The spawn is inside a room, so the first frame is never the void.
    let spawn = &level.spawn;
    assert!(
        level
            .room_iter()
            .any(|room| room.contains(spawn.x, spawn.z)),
        "the demo spawn must be inside a room"
    );

    // And it all builds. The decals are external PNG sheets, so the geometry
    // needs the shipped catalogue rather than the built-in fallback.
    let props = PropCatalog::load_default();
    let assets = crate::assets::AssetCatalog::load_default();
    let mesh = crate::render::build_level_geometry_with_catalog(&level, &props);
    assert!(mesh.batches.wall_batch.count > 0_i32);
    assert!(mesh.batches.floor_batch.count > 0_i32);
    assert!(mesh.batches.light_batch.count > 0_i32);
    assert_eq!(
        mesh.batches.decal_batch.count,
        i32::try_from(level.decals.len() * 6).expect("decal quad count fits"),
        "one quad per placed decal"
    );
    let sheets = crate::render::decal_external_sheet_ids(&level, &assets);
    assert_eq!(
        sheets,
        vec![
            "core:decal_temptation_adam_eve_01".to_string(),
            "core:decal_no_diving_01".to_string(),
            "core:decal_stripes_01".to_string(),
            "core:decal_ceiling_vent_01".to_string(),
            "outdoor:decal_path_edge_01".to_string(),
            "outdoor:decal_path_end_01".to_string(),
            "outdoor:decal_path_corner_01".to_string(),
            "pool:decal_lane_01".to_string()
        ],
        "every decal sheet the demo places resolves as external PNG artwork"
    );
}

/// Compiled packages are discovered, validated and loaded; authoring sources
/// are not playable and imports point at the compiler instead.
#[test]
fn test_custom_packages_are_discovered_loaded_and_sources_rejected() {
    let root =
        std::env::temp_dir().join(format!("places-custom-level-test-{}", std::process::id()));
    let assets_dir = root.join("assets/levels");
    let levels_dir = root.join("levels");
    let import_dir = root.join("import");
    fs::create_dir_all(&assets_dir).expect("test assets dir");
    fs::create_dir_all(&levels_dir).expect("test levels dir");
    fs::create_dir_all(&import_dir).expect("test import dir");

    let level_json = r#"{
        "format_version": 3,
        "id": "community_room",
        "name": "Community Room",
        "author": "A Player",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
        "ceiling_lights": [{ "fixture": "core:ceiling_panel_01", "x": 2.0, "z": 2.0 }]
    }"#;
    let package = compile_fixture(&levels_dir, "community_room.json", level_json);
    assert!(package.exists(), "the fixture package was published");

    // A raw authoring source beside the package is not playable.
    fs::write(levels_dir.join("authored.json"), level_json).expect("write a raw source");

    let manager = LevelManager::with_paths(assets_dir, levels_dir.clone(), import_dir.clone());
    let entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == "community_room")
        .cloned()
        .expect("a drop-in package is discovered");
    assert_eq!(entry.source_type, LevelSourceType::Installed);
    assert_eq!(entry.name, "Community Room");
    assert!(
        !manager
            .entries()
            .iter()
            .any(|candidate_entry| candidate_entry.id == "authored"),
        "a raw source is never a playable row"
    );
    let loaded = manager
        .load_level(&entry)
        .expect("the drop-in package loads");
    assert_eq!(loaded.level.name, "Community Room");

    // Importing a raw source is rejected with the compiler command.
    let mut refreshed_manager = LevelManager::with_paths(
        root.join("assets/levels"),
        levels_dir.clone(),
        import_dir.clone(),
    );
    let error = refreshed_manager
        .import_file(&levels_dir.join("authored.json"))
        .expect_err("a raw source is not importable");
    assert!(
        error.contains("places-compile"),
        "the rejection names the compiler: {error}"
    );

    // Importing a package copies it into the installed directory.
    let incoming = root.join("incoming");
    fs::create_dir_all(&incoming).expect("incoming dir");
    let pack_json = level_json
        .replace("community_room", "pack_room")
        .replace("Community Room", "Pack Room");
    let incoming_package = compile_fixture(&incoming, "pack_room.json", &pack_json);
    let target = import_dir.join("pack_room.placesmap");
    let _expect_status = fs::copy(&incoming_package, &target).expect("stage the import");
    let imported = refreshed_manager.import_available().expect("import runs");
    assert_eq!(imported, 1, "one package imported");
    assert!(
        refreshed_manager
            .entries()
            .iter()
            .any(|candidate_entry| candidate_entry.id == "pack_room"),
        "the imported package is discovered"
    );

    fs::remove_dir_all(&root).expect("clean up the test directory");
}

/// Authoring sources are expected content in a level directory: a `.json` (with
/// or without a sibling package) and a `.zip` are skipped silently, never
/// become rows, and the explicit-open path still names the compiler.
#[test]
fn test_authoring_sources_are_skipped_silently() {
    let root = std::env::temp_dir().join(format!(
        "places-authoring-source-test-{}",
        std::process::id()
    ));
    let assets_dir = root.join("assets/levels");
    let levels_dir = root.join("levels");
    let import_dir = root.join("import");
    fs::create_dir_all(&assets_dir).expect("test assets dir");
    fs::create_dir_all(&levels_dir).expect("test levels dir");

    let level_json = r#"{
        "format_version": 3,
        "id": "source_room",
        "name": "Source Room",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
    }"#;
    drop(compile_fixture(&levels_dir, "source_room.json", level_json));
    // The repository layout: every source sits beside its compiled package.
    // The authoring workflow also leaves sources the player has never compiled
    // and zip bundles; all of them are silently skipped.
    fs::write(levels_dir.join("source_room.json"), level_json).expect("source beside package");
    fs::write(levels_dir.join("uncompiled_source.json"), level_json).expect("source only");
    fs::write(levels_dir.join("bundle.zip"), b"not a real archive").expect("zip source");

    let manager = LevelManager::with_paths(assets_dir, levels_dir.clone(), import_dir.clone());
    let rows: Vec<(&str, LevelSourceType, bool)> = manager
        .entries()
        .iter()
        .map(|entry| {
            (
                entry.id.as_str(),
                entry.source_type,
                entry.path.to_string_lossy().ends_with(".placesmap"),
            )
        })
        .collect();
    assert!(
        rows.iter().any(|(id, source, package)| *id == "source_room"
            && *source == LevelSourceType::Installed
            && *package),
        "the compiled package is a row: {rows:?}"
    );
    assert_eq!(
        rows.iter().filter(|(id, ..)| *id == "source_room").count(),
        1,
        "the sources beside the package add no duplicate row: {rows:?}"
    );
    for (id, source, package) in &rows {
        assert!(
            *package || *source == LevelSourceType::Embedded,
            "{id} is a package row or the embedded fallback"
        );
    }
    for source in ["source_room.json", "uncompiled_source.json", "bundle.zip"] {
        assert!(
            LevelManager::is_authoring_source(&levels_dir.join(source)),
            "{source} is classified as an authoring source"
        );
    }
    assert_eq!(
        LevelSourceType::Bundled.label(),
        "bundled",
        "the CLI label is stable"
    );

    // Explicitly opening a source stays actionable: it names the compiler.
    let mut refreshed_manager = LevelManager::with_paths(
        root.join("assets/levels"),
        levels_dir.clone(),
        import_dir.clone(),
    );
    for source in ["source_room.json", "uncompiled_source.json"] {
        let error = refreshed_manager
            .import_file(&levels_dir.join(source))
            .expect_err("a source is not importable");
        assert!(
            error.contains("places-compile"),
            "{source} rejection names the compiler: {error}"
        );
    }

    fs::remove_dir_all(&root).expect("clean up the test directory");
}

/// Compiles a tiny level source into a package, without GPU capture.
fn compile_fixture(
    dir: &std::path::Path,
    source_name: &str,
    level_json: &str,
) -> std::path::PathBuf {
    let source = dir.join(source_name);
    fs::write(&source, level_json).expect("write the fixture source");
    let out = source.with_extension("placesmap");
    let request = crate::compiler::BuildRequest {
        source: source.clone(),
        out: out.clone(),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Off],
        workers: 1,
        force: true,
        capture_probes: false,
    };
    drop(crate::compiler::build(&request).expect("the fixture package builds"));
    out
}

#[test]
fn test_the_default_level_is_the_shipped_demo() {
    let manager = LevelManager::new();
    // The Level Select menu renders the bundled showcases and diagnostic map;
    // any drop-in packages are Installed.
    let mut bundled: Vec<&str> = manager
        .entries()
        .iter()
        .filter(|entry| entry.source_type == LevelSourceType::Bundled)
        .map(|entry| entry.id.as_str())
        .collect();
    bundled.sort_unstable();
    assert_eq!(
        bundled,
        vec![
            "beach_demo",
            "frutiger_aero_demo",
            "lantern_hollow",
            "model_zoo",
            "movement_test",
            "places_demo",
            "winter"
        ],
        "the bundled levels include the showcases, Movement Test, Winter, Beach and Aero"
    );

    let loaded = manager.load_default().expect("the shipped demo loads");
    assert_eq!(loaded.entry.id, "places_demo");
    assert_eq!(loaded.entry.source_type, LevelSourceType::Bundled);
    assert_eq!(loaded.level.name, "Places Demo");
}

#[test]
fn test_ceiling_light_intensity_is_optional_and_sanitized() {
    let base = |lights: &str| {
        format!(
            r#"{{
                "format_version": 3,
                "id": "intensity",
                "name": "Intensity",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }} ],
                "ceiling_lights": {lights}
            }}"#
        )
    };

    // An omitted brightness is the standard fixture.
    let omitted = LevelDef::from_json(&base(
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }]"#,
    ))
    .expect("omitted brightness parses");
    validate_level(&omitted).expect("an omitted brightness validates");
    assert_eq!(omitted.ceiling_lights[0].brightness, None);
    assert_exact(omitted.ceiling_lights[0].intensity(), 1.0);

    // Authored brightness values load and sanitise.
    let both = LevelDef::from_json(&base(
        r#"[
            { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 0.8 },
            { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0, "brightness": 1.4 }
        ]"#,
    ))
    .expect("authored brightness parses");
    assert_exact(both.ceiling_lights[0].intensity(), 0.8);
    assert_exact(both.ceiling_lights[1].intensity(), 1.4);
    validate_level(&both).expect("authored intensities validate");

    // A negative intensity is malformed data; the loader says so instead of
    // producing negative lighting.
    let negative = LevelDef::from_json(&base(
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": -1.0 }]"#,
    ))
    .expect("negative intensity still parses");
    let error = validate_level(&negative).expect_err("negative intensity is rejected");
    assert!(error.contains("intensity"), "unexpected error: {error}");

    // Non-finite light coordinates are rejected like every other element.
    let mut non_finite = both;
    non_finite.ceiling_lights[0].x = f32::NAN;
    assert!(validate_level(&non_finite).is_err());

    // Very high intensities are allowed through validation (they saturate),
    // but sanitise to a finite, bounded value.
    let high = LevelDef::from_json(&base(
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 1000.0 }]"#,
    ))
    .expect("high intensity parses");
    validate_level(&high).expect("high intensity still loads");
    assert_exact(
        high.ceiling_lights[0].intensity(),
        crate::lighting::MAX_LIGHT_INTENSITY,
    );
}

#[test]
fn test_ceiling_light_colour_is_optional_validated_and_round_trips() {
    let base = |lights: &str| {
        format!(
            r#"{{
                "format_version": 3,
                "id": "colour",
                "name": "Colour",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }} ],
                "ceiling_lights": {lights}
            }}"#
        )
    };

    // Omitted colour: the fixture keeps loading and emits the documented
    // restrained warm default.
    let omitted = LevelDef::from_json(&base(
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }]"#,
    ))
    .expect("omitted colour parses");
    validate_level(&omitted).expect("an omitted colour validates");
    assert_eq!(omitted.ceiling_lights[0].color, None);
    assert_eq!(
        omitted.ceiling_lights[0].emitted_color(),
        crate::lighting::DEFAULT_LIGHT_COLOR
    );

    // Explicit colours, including the boundary values 0 and 1.
    let explicit = LevelDef::from_json(&base(
        r#"[
            { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0,
              "color": [1.0, 0.55, 0.0] },
            { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0,
              "color": [0.0, 0.0, 1.0] }
        ]"#,
    ))
    .expect("explicit colours parse");
    validate_level(&explicit).expect("boundary colours validate");
    assert_eq!(
        explicit.ceiling_lights[0].color,
        Some(crate::lighting::LightColor::rgb(1.0, 0.55, 0.0))
    );
    assert_eq!(
        explicit.ceiling_lights[1].emitted_color(),
        crate::lighting::LightColor::rgb(0.0, 0.0, 1.0)
    );

    // Serialisation round trip: the array shape survives a save round trip.
    let json = serde_json::to_value(&explicit).expect("level serialises");
    let restored: LevelDef = serde_json::from_value(json).expect("level deserialises");
    assert_eq!(
        restored.ceiling_lights[0].color,
        explicit.ceiling_lights[0].color
    );

    // Out-of-range and non-finite channels are rejected, exactly like a
    // negative intensity.
    for bad in [
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "color": [1.5, 0.0, 0.0] }]"#,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "color": [-0.1, 0.0, 0.0] }]"#,
    ] {
        let level = LevelDef::from_json(&base(bad)).expect("out-of-range colour still parses");
        let error = validate_level(&level).expect_err("out-of-range colour is rejected");
        assert!(error.contains("colour"), "unexpected error: {error}");
    }
    // Programmatic non-finite channels are rejected by the same rule.
    let mut non_finite = explicit.clone();
    non_finite.ceiling_lights[0].color = Some(crate::lighting::LightColor::rgb(f32::NAN, 0.5, 0.5));
    assert!(validate_level(&non_finite).is_err());
    // And sanitising them for baking can never produce negative or
    // non-finite illumination.
    assert_eq!(
        non_finite.ceiling_lights[0].emitted_color(),
        crate::lighting::LightColor::rgb(0.0, 0.5, 0.5)
    );
}

#[test]
fn test_shipped_prop_catalog_covers_shipped_levels() {
    let catalog = PropCatalog::load_from_path(Path::new("assets/catalog.json"))
        .expect("assets/catalog.json must load");
    assert!(!catalog.is_empty());

    let mut levels_checked = 0_i32;
    let mut props_checked = 0_i32;
    for entry in fs::read_dir("assets/levels")
        .expect("assets/levels exists")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("level file is readable");
        let level = LevelDef::from_json(&content)
            .unwrap_or_else(|e| panic!("{} is not a valid level: {e}", path.display()));
        validate_level(&level)
            .unwrap_or_else(|e| panic!("{} failed validation: {e}", path.display()));
        levels_checked += 1_i32;
        for prop in &level.props {
            assert!(
                catalog.contains(&prop.model),
                "{} uses prop '{}' which is missing from assets/catalog.json",
                path.display(),
                prop.model
            );
            props_checked += 1_i32;
        }
    }
    assert!(levels_checked >= 1_i32, "expected the shipped level file");
    assert!(props_checked >= 1_i32, "expected at least one placed prop");
}

/// Loads one engine regression fixture from `tests/fixtures/levels/`.
///
/// Fixtures exercise the loader, renderer and lighting model; they are not part
/// of the shipped game content.
fn fixture_level(name: &str) -> LevelDef {
    let path = format!("tests/fixtures/levels/{name}.json");
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    let level = LevelDef::from_json(&content)
        .unwrap_or_else(|error| panic!("{path} is not a valid level: {error}"));
    validate_level(&level).unwrap_or_else(|error| panic!("{path} failed validation: {error}"));
    level
}

// ---------------------------------------------------------------- decals

/// One decal in an otherwise valid room, with `body` replacing the decal
/// object so malformed fields can be injected.
fn decal_level(body: &str) -> String {
    format!(
        r#"{{
            "format_version": 3,
            "id": "decal_level",
            "name": "Decal Level",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }}],
            "decals": [{body}]
        }}"#
    )
}

#[test]
fn test_decals_parse_with_defaults_and_validate() {
    let json = decal_level(
        r#"{ "x": 2.0, "z": 2.0, "width": 1.0, "height": 0.5,
             "material": "core:decal_test_01", "surface": "floor" }"#,
    );
    let level = LevelDef::from_json(&json).expect("valid decal json");
    assert_eq!(level.decals.len(), 1);
    let decal = &level.decals[0];
    assert_exact(decal.y, 0.0);
    assert_exact(decal.rotation_degrees, 0.0);
    assert_eq!(decal.surface, crate::level::DecalSurface::Floor);
    validate_level(&level).expect("a well-formed decal validates");
    assert_eq!(level.estimate_geometry().decal_quads, 1);
}

#[test]
fn test_validate_level_rejects_malformed_decals() {
    let valid = decal_level(
        r#"{ "x": 2.0, "y": 1.0, "z": 0.0, "width": 1.0, "height": 0.5,
             "material": "core:decal_test_01", "surface": "wall_south" }"#,
    );
    let base = LevelDef::from_json(&valid).expect("valid decal json");
    validate_level(&base).expect("the base level validates");

    let mut level = base.clone();
    level.decals[0].x = f32::NAN;
    assert!(validate_level(&level).is_err(), "non-finite x");
    let mut negative_height = base.clone();
    negative_height.decals[0].height = -1.0;
    assert!(validate_level(&negative_height).is_err(), "negative height");
    let mut oversized_width = base.clone();
    oversized_width.decals[0].width = 11.0;
    assert!(validate_level(&oversized_width).is_err(), "oversized width");
    let mut empty_material = base.clone();
    empty_material.decals[0].material = "  ".into();
    assert!(validate_level(&empty_material).is_err(), "empty material");

    // An unknown surface name is a schema error, not a silently dropped
    // decal: the level does not parse at all.
    let unknown_surface = LevelDef::from_json(&decal_level(
        r#"{ "x": 1.0, "y": 1.0, "z": 0.0, "width": 1.0, "height": 0.5,
             "material": "core:decal_test_01", "surface": "wall_up" }"#,
    ));
    assert!(unknown_surface.is_err());
}

#[test]
fn test_the_decal_count_is_bounded() {
    let decal = r#"{ "x": 1.0, "y": 1.0, "z": 0.0, "width": 1.0, "height": 0.5,
                     "material": "core:decal_test_01", "surface": "wall_south" }"#;
    let decals = std::iter::repeat_n(
        decal,
        usize::try_from(crate::level::MAX_LEVEL_DECALS).expect("fixture integer fits usize") + 1,
    )
    .collect::<Vec<_>>()
    .join(",");
    let level = LevelDef::from_json(&decal_level(&decals)).expect("large decal list still parses");
    assert!(
        validate_level(&level).is_err(),
        "a level past the decal cap must be rejected"
    );
}

#[test]
fn test_rendering_diagnostic_level_shows_every_decal_sheet() {
    let level = fixture_level("rendering_diagnostic");
    assert_eq!(level.decals.len(), 8);
    let catalog = crate::assets::AssetCatalog::load_default();
    for decal in &level.decals {
        assert!(
            crate::render::decal_sheet_index(&level, &catalog, &decal.material).is_some(),
            "{} references an unknown decal sheet",
            decal.material
        );
    }
    let mesh = crate::render::build_level_geometry_with_catalog(
        &level,
        &crate::loader::PropCatalog::load_default(),
    );
    assert_eq!(
        mesh.batches.decal_batch.count,
        i32::try_from(level.decals.len()).unwrap_or(0_i32) * 6_i32,
        "every diagnostic decal must emit one quad"
    );
}

#[test]
fn test_vertical_diagnostic_level_exercises_the_new_geometry() {
    let level = fixture_level("vertical_diagnostic");
    assert!(
        validate_level(&level).is_ok(),
        "the vertical diagnostic level must validate"
    );

    // Area A keeps the standard default ceiling height.
    let ordinary = &level.rooms[0];
    assert_exact(ordinary.floor_y, 0.0);
    assert_exact(ordinary.height, crate::level::DEFAULT_CEILING_HEIGHT_M);
    assert!(ordinary.ceiling.is_flat());

    // Area B is a genuinely elevated room.
    let elevated = &level.rooms[1];
    assert_exact(elevated.floor_y, 2.0);

    // Area D is a real gable.
    let gable = &level.rooms[3];
    assert_eq!(gable.ceiling.ridge_axis(), Some(crate::level::WallAxis::X));
    assert_exact(gable.ceiling.ridge_rise_m(), 2.0);
    assert_exact(gable.ridge_y().expect("ridge"), 7.0);

    // Area C carries a walkable recess and a deep one.
    let surfaces = crate::level::LevelSurfaces::new(&level);
    assert_eq!(surfaces.floor_y_at(20.0, 5.0), Some(1.65), "shallow recess");
    assert_eq!(surfaces.floor_y_at(25.0, 5.0), Some(0.5), "deep recess");
    assert_eq!(
        surfaces.floor_y_at(30.0, 5.0),
        Some(2.0),
        "gable room floor"
    );

    // The staircase in area A climbs in walkable steps from 0 to 2.
    let mut previous = 0.0;
    for x in [6.7_f32, 7.3, 7.9, 8.5, 9.1, 9.7] {
        let step = surfaces.floor_y_at(x, 5.0).expect("inside room A");
        assert!(
            (step - previous).abs() <= crate::collision::PLAYER_STEP_HEIGHT + 1e-3,
            "staircase step at {x} is {step}, was {previous}"
        );
        previous = step;
    }
    assert!((previous - 2.0).abs() < 1e-4);

    // Every fixture hangs below its own ceiling, and the gable fixtures use
    // the local profile rather than one flat plane.
    let lighting = crate::lighting::LevelLighting::bake(&level);
    let eave_light = lighting.fixture_y(33.0, 1.0);
    let ridge_light = lighting.fixture_y(33.0, 5.0);
    assert!(
        (5.2..5.5).contains(&eave_light),
        "eave fixture: {eave_light}"
    );
    assert!(
        (ridge_light - (7.0 - 0.01)).abs() < 0.05,
        "ridge fixture: {ridge_light}"
    );
    assert!(ridge_light - eave_light > 1.4);

    // The collision geometry follows the recesses: a deep one has solid
    // walls, the shallow one does not.
    let rims = level
        .collision_aabbs()
        .iter()
        .filter(|aabb| aabb.min_y < 1.0 && aabb.max_y <= 2.0 + 1e-3)
        .count();
    assert!(rims > 0, "the deep recess keeps its retaining walls");

    // It still builds, with the decals and props it authors. The shipped
    // catalog is used so the level's external (PNG) decal sheet resolves
    // exactly as it does in game.
    let mesh =
        crate::render::build_level_geometry_with_catalog(&level, &PropCatalog::load_default());
    assert!(mesh.vertex_count > 0);
    assert_eq!(
        mesh.batches.decal_batch.count,
        i32::try_from(level.decals.len()).unwrap_or(0_i32) * 6_i32
    );
}

// ------------------------------------------------------------- fixture sheets

/// The visible face of a fixture family is ordinary external artwork: the
/// catalog names its PNG, the loader decodes it once through the shared session
/// cache, and every further light of that family reuses the same sheet.
#[test]
fn test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "fixture_sheets",
            "name": "Fixture Sheets",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 6.0, "height": 3.0 }],
            "ceiling_lights": [
                { "fixture": "core:pool_light_round", "x": 2.0, "z": 2.0 },
                { "fixture": "core:pool_light_round", "x": 4.0, "z": 2.0,
                  "id": "switchable_round", "switchable": true },
                { "fixture": "core:pool_light_wall", "x": 6.0, "z": 2.0,
                  "mount": "wall", "y": 1.7, "rotation_degrees": 180.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0 },
                { "fixture": "home:ceiling_light_round", "x": 10.0, "z": 2.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let catalog = crate::assets::AssetCatalog::load_default();
    let mut cache = TextureCache::new();
    let sheets = resolve_fixture_sheets(&level, &catalog, None, &mut cache);

    // One sheet per family (in family-slot order), plus one private slot for
    // the switchable fixture. The second round fixture adds nothing to the
    // family sheet.
    let kinds: Vec<crate::lighting::FixtureKind> = sheets.iter().map(|sheet| sheet.kind).collect();
    assert_eq!(
        kinds,
        [
            crate::lighting::FixtureKind::FluorescentPanel,
            crate::lighting::FixtureKind::RoundRecessed,
            crate::lighting::FixtureKind::WallSconce,
            crate::lighting::FixtureKind::FlushMount,
            crate::lighting::FixtureKind::RoundRecessed,
        ]
    );
    let slots: Vec<u32> = sheets.iter().map(|sheet| sheet.slot).collect();
    assert_eq!(
        slots,
        [
            0,
            1,
            2,
            3,
            crate::level::FIXTURE_SWITCHABLE_MATERIAL_BASE + 1,
        ],
        "the switchable fixture owns the private slot for its array position"
    );

    let sheet_for = |kind| {
        sheets
            .iter()
            .find(|sheet| sheet.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} must resolve a sheet"))
    };
    for sheet in &sheets {
        assert_eq!(
            sheet.origin,
            crate::materials::TextureOrigin::Catalog,
            "{:?} comes from the catalog",
            sheet.kind
        );
        assert!(
            sheet
                .key
                .split_once("#png-v1-")
                .is_some_and(|(source, _)| source.to_ascii_lowercase().ends_with(".png")
                    && !source.starts_with("assets/")),
            "{:?}: the key is the catalog-relative PNG path, found `{}`",
            sheet.kind,
            sheet.key
        );
        assert!(
            cache.get(&sheet.key).is_some(),
            "{:?}: the decoded image is shared with the session cache",
            sheet.kind
        );
        // Fixture faces are opaque surfaces; alpha would only cost sorting.
        for texel in sheet.image.rgba.as_chunks::<4>().0 {
            assert_eq!(texel[3], 255, "{:?} must be fully opaque", sheet.kind);
        }
    }

    // The shipped sheets keep the aspect each face is mapped with, so nothing
    // is stretched: the panel and the wall lens are 2:1, the round sheets 1:1.
    // The exact resolution is art policy (the shipped sheets are currently
    // authored at the 1024 hard budget), so the contract pinned here is the
    // aspect, the power-of-two edges the mip chain needs, and the engine limit.
    let dimensions = |kind| {
        let sheet = sheet_for(kind);
        (sheet.image.width, sheet.image.height)
    };
    let aspect = |kind| {
        let (width, height) = dimensions(kind);
        let divisor = gcd(width, height);
        if divisor == 0 {
            return (width, height);
        }
        (width / divisor, height / divisor)
    };
    assert_eq!(
        aspect(crate::lighting::FixtureKind::FluorescentPanel),
        (2, 1)
    );
    assert_eq!(aspect(crate::lighting::FixtureKind::RoundRecessed), (1, 1));
    assert_eq!(aspect(crate::lighting::FixtureKind::WallSconce), (2, 1));
    assert_eq!(aspect(crate::lighting::FixtureKind::FlushMount), (1, 1));
    for kind in [
        crate::lighting::FixtureKind::FluorescentPanel,
        crate::lighting::FixtureKind::RoundRecessed,
        crate::lighting::FixtureKind::WallSconce,
        crate::lighting::FixtureKind::FlushMount,
    ] {
        let (width, height) = dimensions(kind);
        assert!(
            width.is_power_of_two() && height.is_power_of_two(),
            "{kind:?}: fixture faces are drawn with mipmaps, so both edges must be \
             powers of two, found {width}x{height}"
        );
        assert!(
            width <= crate::assets::MAX_TEXTURE_DIMENSION
                && height <= crate::assets::MAX_TEXTURE_DIMENSION,
            "{kind:?}: fixture face {width}x{height} is over the {}x{} hard limit",
            crate::assets::MAX_TEXTURE_DIMENSION,
            crate::assets::MAX_TEXTURE_DIMENSION
        );
    }
}

/// Greatest common divisor of two non-zero sheet dimensions.
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// A fixture with no sheet to draw (an unknown id, or a `pack:` id whose pack
/// carries nothing) resolves to nothing at all: the family draws the shared
/// white sheet instead of borrowing some other fixture's artwork.
#[test]
fn test_fixtures_without_a_sheet_resolve_to_nothing() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "sheetless_fixtures",
            "name": "Sheetless Fixtures",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }],
            "ceiling_lights": [
                { "fixture": "core:future_panel", "x": 2.0, "z": 2.0 },
                { "fixture": "pack:my_panel", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let catalog = crate::assets::AssetCatalog::load_default();
    let mut cache = TextureCache::new();
    let sheets = resolve_fixture_sheets(&level, &catalog, None, &mut cache);
    assert!(
        sheets.is_empty(),
        "no sheet was named, so none may be resolved: {sheets:?}"
    );
}

/// The shipped demo places all three built-in families, and every one of them
/// reaches the loaded level with its catalog sheet attached: no fixture can
/// silently fall back to the untextured sheet on the level the game boots into.
#[test]
fn test_the_demo_loads_every_fixture_family_with_its_sheet() {
    let manager = LevelManager::new();
    let loaded = manager.load_default().expect("the demo loads");

    let families: Vec<crate::lighting::FixtureKind> = loaded
        .level
        .ceiling_lights
        .iter()
        .map(|light| crate::lighting::fixture_profile(&light.fixture).kind)
        .collect();
    for kind in [
        crate::lighting::FixtureKind::FluorescentPanel,
        crate::lighting::FixtureKind::RoundRecessed,
        crate::lighting::FixtureKind::WallSconce,
    ] {
        assert!(families.contains(&kind), "the demo places {kind:?}");
        let sheet = loaded
            .light_sheets
            .iter()
            .find(|sheet| sheet.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} must resolve its sheet"));
        assert!(!sheet.image.rgba.is_empty(), "{kind:?} sheet is empty");
    }
}

/// Places Demo is offered even when nothing is installed, and selecting the
/// offered entry loads the embedded copy.
#[test]
fn test_the_embedded_demo_is_always_listed_and_loadable() {
    let scratch = std::path::Path::new("target/agent-work/tests/levels-empty");
    crate::test_support::remove_dir_if_present(scratch);
    std::fs::create_dir_all(scratch).expect("scratch directory is writable");

    // `with_paths` never discovers anything under a fresh scratch directory.
    let manager = LevelManager::with_paths(
        scratch.join("assets/levels"),
        scratch.join("levels"),
        scratch.join("import"),
    );
    let entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == DEMO_LEVEL_ID)
        .expect("the embedded demo is always listed");
    assert_eq!(entry.source_type, LevelSourceType::Embedded);

    let loaded = manager.load_level(entry).expect("the embedded demo loads");
    assert_eq!(loaded.level.id, DEMO_LEVEL_ID);
    assert_eq!(loaded.entry.source_type, LevelSourceType::Embedded);

    crate::test_support::remove_dir_if_present(scratch);
}

// ------------------------------------------------- generic architecture

/// The committed Home showcase must satisfy the full loader contract: it
/// exercises every generic architectural piece and every Home material.
#[test]
fn test_validate_accepts_the_home_showcase_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/levels/home_showcase.json")
        .expect("the Home showcase fixture is present");
    let level = LevelDef::from_json(&content).expect("the Home showcase parses");
    if let Err(error) = validate_level(&level) {
        panic!("the Home showcase must validate: {error}");
    }
    for (name, empty) in [
        ("ramps", level.ramps.is_empty()),
        ("stairs", level.stairs.is_empty()),
        ("half_walls", level.half_walls.is_empty()),
        ("columns", level.columns.is_empty()),
        ("archways", level.archways.is_empty()),
        ("guardrails", level.guardrails.is_empty()),
        ("thresholds", level.thresholds.is_empty()),
        ("baseboards", level.baseboards.is_empty()),
    ] {
        assert!(!empty, "the showcase places at least one {name} entry");
    }
}

/// A level with one room and the given architecture arrays appended, validated.
fn architecture_level(extra: &str) -> Result<(), String> {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "architecture",
            "name": "Architecture",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 14.0, "depth": 10.0, "height": 4.0 }} ]{extra}
        }}"#
    );
    let level = LevelDef::from_json(&json).expect("architecture json parses");
    validate_level(&level)
}

#[test]
fn test_validate_rejects_malformed_ramps() {
    architecture_level(
        r#", "ramps": [{ "x": 1.0, "z": 1.0, "width": 0.4, "depth": 1.0, "rise": 2.0 }]"#,
    )
    .expect("a 2 m rise over a 1 m run is exactly at the walkable slope limit");

    let error = architecture_level(
        r#", "ramps": [{ "x": 1.0, "z": 1.0, "width": 0.4, "depth": 0.5, "rise": 2.0 }]"#,
    )
    .expect_err("two metres of rise over a half-metre run is a wall, not a ramp");
    assert!(error.contains("too steep"), "{error}");

    let zero_rise_error = architecture_level(
        r#", "ramps": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 2.0, "rise": 0.0 }]"#,
    )
    .expect_err("a ramp with no rise is a floor region");
    assert!(zero_rise_error.contains("no rise"), "{zero_rise_error}");

    let outside_error = architecture_level(
        r#", "ramps": [{ "x": 30.0, "z": 30.0, "width": 1.0, "depth": 2.0, "rise": 0.5 }]"#,
    )
    .expect_err("a ramp outside every room");
    assert!(
        outside_error.contains("outside every room"),
        "{outside_error}"
    );
}

#[test]
fn test_validate_rejects_malformed_staircases() {
    let error = architecture_level(
        r#", "stairs": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0,
                           "rise": 1.2, "steps": 2 }]"#,
    )
    .expect_err("a 0.6 m riser is taller than the walkable step");
    assert!(error.contains("riser"), "{error}");

    let short_tread_error = architecture_level(
        r#", "stairs": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 2.0,
                           "rise": 0.8, "steps": 20 }]"#,
    )
    .expect_err("a 0.1 m tread is not a step");
    assert!(short_tread_error.contains("tread"), "{short_tread_error}");

    let single_step_error = architecture_level(
        r#", "stairs": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0,
                           "rise": 0.4, "steps": 1 }]"#,
    )
    .expect_err("a single step is a floor region");
    assert!(
        single_step_error.contains("at least 2 steps"),
        "{single_step_error}"
    );
}

#[test]
fn test_validate_rejects_walking_surfaces_that_overlap() {
    let error = architecture_level(
        r#", "ramps": [{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 4.0, "rise": 0.5 }],
             "floor_regions": [{ "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": 1.0 }]"#,
    )
    .expect_err("a floor region inside a ramp has no single floor");
    assert!(error.contains("overlaps ramp"), "{error}");

    let staircase_overlap_error = architecture_level(
        r#", "ramps": [{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 4.0, "rise": 0.5 }],
             "stairs": [{ "x": 1.0, "z": 2.0, "width": 2.0, "depth": 2.0,
                           "rise": 0.4, "steps": 2 }]"#,
    )
    .expect_err("a staircase inside a ramp has no single floor");
    assert!(
        staircase_overlap_error.contains("overlaps staircase"),
        "{staircase_overlap_error}"
    );
}

#[test]
fn test_validate_rejects_malformed_archways() {
    let error = architecture_level(
        r#", "archways": [{ "x": 5.0, "z": 3.0, "width": 0.3, "depth": 1.0, "height": 3.0,
                             "opening_width": 0.95, "opening_height": 2.1, "arch_rise": 0.2 }]"#,
    )
    .expect_err("an opening with no pier left is not an archway");
    assert!(error.contains("too wide"), "{error}");

    let short_archway_error = architecture_level(
        r#", "archways": [{ "x": 5.0, "z": 3.0, "width": 0.3, "depth": 1.4, "height": 1.5,
                             "opening_width": 0.9, "opening_height": 2.1, "arch_rise": 0.2 }]"#,
    )
    .expect_err("the block must be at least as tall as its opening");
    assert!(
        short_archway_error.contains("shorter than its opening"),
        "{short_archway_error}"
    );

    let high_crown_error = architecture_level(
        r#", "archways": [{ "x": 5.0, "z": 3.0, "width": 0.3, "depth": 1.4, "height": 3.0,
                             "opening_width": 0.9, "opening_height": 1.0, "arch_rise": 1.2 }]"#,
    )
    .expect_err("the crown cannot sit at or below the springing line");
    assert!(high_crown_error.contains("arch rise"), "{high_crown_error}");
}

#[test]
fn test_validate_rejects_a_threshold_over_an_elevation_change() {
    let error = architecture_level(
        r#", "floor_regions": [{ "x": 4.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": 0.5 }],
             "thresholds": [{ "x": 5.0, "z": 4.0, "length": 1.0 }]"#,
    )
    .expect_err("a strip that spans the platform edge would float on one side");
    assert!(error.contains("height change"), "{error}");

    let outside_error =
        architecture_level(r#", "thresholds": [{ "x": 30.0, "z": 30.0, "length": 1.0 }]"#)
            .expect_err("a strip outside every room has no floor to sit on");
    assert!(
        outside_error.contains("outside every room"),
        "{outside_error}"
    );
}

#[test]
fn test_validate_rejects_malformed_trim_and_rails() {
    let error = architecture_level(
        r#", "guardrails": [{ "x": 1.0, "z": 1.0, "length": 2.0, "height": 3.5 }]"#,
    )
    .expect_err("a 3.5 m rail is not a guardrail");
    assert!(error.contains("height"), "{error}");

    let post_spacing_error = architecture_level(
        r#", "guardrails": [{ "x": 1.0, "z": 1.0, "length": 2.0, "post_spacing": 0.05 }]"#,
    )
    .expect_err("a 5 cm post spacing is a typo");
    assert!(
        post_spacing_error.contains("post spacing"),
        "{post_spacing_error}"
    );

    let baseboard_height_error = architecture_level(
        r#", "baseboards": [{ "x": 1.0, "z": 1.0, "length": 3.0, "height": 1.5 }]"#,
    )
    .expect_err("a 1.5 m skirting board is not trim");
    assert!(
        baseboard_height_error.contains("height"),
        "{baseboard_height_error}"
    );
}

#[test]
fn test_validate_water_accepts_a_valid_volume_and_rejects_bad_ones() {
    let json = |water: &str| -> String {
        format!(
            r#"{{
                "format_version": 3,
                "id": "water_gate",
                "name": "Water Gate",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }} ],
                "water": [{water}]
            }}"#
        )
    };
    let parse = |water: &str| LevelDef::from_json(&json(water)).expect("water json parses");

    let valid = parse(r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "surface_y": 0.5 }"#);
    validate_level(&valid).expect("a surface above the floor is valid");

    let buried = parse(r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "surface_y": -0.5 }"#);
    let error = validate_level(&buried).expect_err("a surface below the floor is rejected");
    assert!(error.contains("below the floor"), "{error}");

    let outside =
        parse(r#"{ "x": 30.0, "z": 30.0, "width": 2.0, "depth": 2.0, "surface_y": -0.5 }"#);
    let outside_error =
        validate_level(&outside).expect_err("a volume outside every room is rejected");
    assert!(
        outside_error.contains("outside every room"),
        "{outside_error}"
    );

    let bad_opacity = parse(
        r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "surface_y": -0.5, "opacity": 1.5 }"#,
    );
    let bad_opacity_error =
        validate_level(&bad_opacity).expect_err("opacity outside 0..=1 is rejected");
    assert!(bad_opacity_error.contains("opacity"), "{bad_opacity_error}");

    let bad_bottom = parse(
        r#"{ "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "surface_y": -0.5, "bottom_y": 0.0 }"#,
    );
    let bad_bottom_error =
        validate_level(&bad_bottom).expect_err("a bottom above the surface is rejected");
    assert!(bad_bottom_error.contains("bottom_y"), "{bad_bottom_error}");

    let zero_width =
        parse(r#"{ "x": 1.0, "z": 1.0, "width": 0.0, "depth": 2.0, "surface_y": -0.5 }"#);
    let zero_width_error =
        validate_level(&zero_width).expect_err("a zero-width volume is rejected");
    assert!(
        zero_width_error.contains("width and depth"),
        "{zero_width_error}"
    );
}

/// Ladders validate like water volumes: a real room overlap, positive
/// footprint, finite reach and a top above the bottom; the shipped demo's own
/// ladder passes and resolves to the +X climb direction.
#[test]
fn test_validate_ladders_accepts_the_demo_and_rejects_bad_reach() {
    let demo = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("the Places demo parses");
    validate_level(&demo).expect("the shipped demo validates");
    assert_eq!(demo.ladders.len(), 1, "the demo authors its pool ladder");
    let ladders = crate::level::Ladders::from_level(&demo);
    assert_eq!(ladders.len(), 1);
    let ladder = ladders.get(0).expect("one resolved ladder");
    assert!(
        ladder.facing_x > 0.99 && ladder.facing_z.abs() < 0.01,
        "facing 90 degrees climbs towards +X"
    );
    assert!(ladder.approach_side(19.0, 12.0), "the water side attaches");
    assert!(
        !ladder.approach_side(20.5, 12.0),
        "the deck side never attaches"
    );
    assert!(ladder.overlaps_disc(19.6, 12.0, 0.3));

    let json = |ladder_json: &str| -> String {
        format!(
            r#"{{
                "format_version": 3,
                "id": "ladder_gate",
                "name": "Ladder Gate",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }} ],
                "ladders": [{ladder_json}]
            }}"#
        )
    };
    let parse =
        |ladder_json: &str| LevelDef::from_json(&json(ladder_json)).expect("ladder json parses");

    let valid = parse(
        r#"{ "x": 1.0, "z": 1.0, "width": 0.6, "depth": 0.6,
             "bottom_y": 0.0, "top_y": 1.5, "facing_degrees": 0.0 }"#,
    );
    validate_level(&valid).expect("a valid ladder passes");

    let inverted = parse(
        r#"{ "x": 1.0, "z": 1.0, "width": 0.6, "depth": 0.6,
             "bottom_y": 1.5, "top_y": 0.0 }"#,
    );
    let error = validate_level(&inverted).expect_err("a top below the bottom is rejected");
    assert!(error.contains("top_y"), "{error}");

    let zero_width = parse(
        r#"{ "x": 1.0, "z": 1.0, "width": 0.0, "depth": 0.6,
             "bottom_y": 0.0, "top_y": 1.5 }"#,
    );
    let zero_width_error =
        validate_level(&zero_width).expect_err("a zero-width ladder is rejected");
    assert!(
        zero_width_error.contains("width and depth"),
        "{zero_width_error}"
    );

    let outside = parse(
        r#"{ "x": 30.0, "z": 30.0, "width": 0.6, "depth": 0.6,
             "bottom_y": 0.0, "top_y": 1.5 }"#,
    );
    let outside_error =
        validate_level(&outside).expect_err("a ladder outside every room is rejected");
    assert!(
        outside_error.contains("outside every room"),
        "{outside_error}"
    );
}

// ------------------------------------------------- level preparation

/// A synthetic catalog with an office-flavoured wall material declaring a
/// baseboard, a plain wall material without one, and the trim material itself.
fn baseboard_catalog() -> crate::assets::AssetCatalog {
    crate::assets::AssetCatalog::from_json_str(
        r#"{
            "format_version": 3,
            "assets": [
                { "id": "test:tex_wall", "asset_class": "environment",
                  "asset_type": "texture", "source": "file",
                  "model": "test/wall.png", "surface": "wall" },
                { "id": "test:tex_trim", "asset_class": "environment",
                  "asset_type": "texture", "source": "file",
                  "model": "test/trim.png", "surface": "wall" },
                { "id": "test:trim", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "wall", "texture": "test:tex_trim",
                  "tile_metres": 2.0 },
                { "id": "test:wall", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "wall", "texture": "test:tex_wall",
                  "tile_metres": 2.0, "baseboard": "test:trim" },
                { "id": "test:plain_wall", "asset_class": "environment",
                  "asset_type": "material", "source": "definition",
                  "surface": "wall", "texture": "test:tex_wall",
                  "tile_metres": 2.0 }
            ]
        }"#,
    )
    .expect("synthetic baseboard catalog parses")
}

/// One 5 x 5 m room and one X-axis wall spanning its south edge, with a door
/// and a window, plus whatever extra JSON the caller needs.
fn baseboard_level(extra: &str) -> LevelDef {
    let separator = if extra.trim().is_empty() { "" } else { "," };
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "baseboard_prep",
            "name": "Baseboard Prep",
            "spawn": {{ "x": 2.5, "z": 2.5 }},
            "defaults": {{ "wall": "test:plain_wall", "floor": "test:plain_wall",
                           "ceiling": "test:plain_wall" }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 5.0, "depth": 5.0, "height": 3.0 }}],
            "walls": [{{
                "x": 0.0, "z": 0.0, "width": 5.0, "depth": 0.3,
                "material": "test:wall",
                "openings": [
                    {{ "kind": "door", "offset": 1.0, "width": 1.0, "height": 2.1, "sill": 0.0 }},
                    {{ "kind": "window", "offset": 3.5, "width": 1.0, "height": 1.2, "sill": 1.0 }}
                ]
            }}]
            {separator}{extra}
        }}"#
    ))
    .expect("baseboard preparation level parses")
}

/// A face whose material declares a baseboard gains runs along the room-facing
/// side only, split around a floor-reaching door while a window with a sill
/// keeps its board.
#[test]
fn test_prepare_level_generates_office_baseboards_around_floor_openings() {
    let mut level = baseboard_level("");
    let catalog = baseboard_catalog();
    prepare_level(&mut level, &catalog, None);

    let generated: Vec<&crate::level::BaseboardDef> = level
        .baseboards
        .iter()
        .filter(|board| board.material.as_deref() == Some("test:trim"))
        .collect();
    assert_eq!(generated.len(), 2, "one run per side of the door");
    // The +Z face fronts the room: rotation 0 runs +X from the wall's face.
    let first = generated[0];
    assert_exact(first.x, 0.0);
    assert_exact(first.z, 0.3);
    assert_exact(first.length, 1.0);
    assert_exact(first.rotation_degrees, 0.0);
    assert_eq!(first.y, Some(0.0));
    assert_exact(first.height, crate::level::BASEBOARD_DEFAULT_HEIGHT_M);
    assert_exact(first.thickness, crate::level::BASEBOARD_DEFAULT_THICKNESS_M);
    let second = generated[1];
    assert_exact(second.x, 2.0);
    assert_exact(second.z, 0.3);
    assert_exact(second.length, 3.0);
    assert!(
        generated.iter().all(|board| board.rotation_degrees == 0.0),
        "the -Z face fronts no room and must not be trimmed: {generated:?}"
    );
    // The generated boards satisfy the authored contract.
    validate_level(&level).expect("the prepared level still validates");
}

/// A wall facing no room gains nothing, and the trim material itself must not
/// leak onto faces finished with a material that declares no baseboard.
#[test]
fn test_prepare_level_skips_faces_that_front_no_walkable_floor() {
    let mut level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "baseboard_void",
            "name": "Baseboard Void",
            "spawn": { "x": 2.5, "z": 2.5 },
            "defaults": { "wall": "test:wall", "floor": "test:wall", "ceiling": "test:wall" },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 5.0, "depth": 5.0, "height": 3.0 }],
            "walls": [{ "x": 20.0, "z": 20.0, "width": 5.0, "depth": 0.3 }]
        }"#,
    )
    .expect("void wall level parses");
    let catalog = baseboard_catalog();
    prepare_level(&mut level, &catalog, None);
    assert!(
        level.baseboards.is_empty(),
        "a wall over the void gets no trim: {:?}",
        level.baseboards
    );

    // A face whose resolved material has no `baseboard` gains nothing either.
    let mut plain = baseboard_level("");
    plain.walls[0].material = Some("test:plain_wall".to_string());
    let count_before = plain.baseboards.len();
    prepare_level(&mut plain, &catalog, None);
    assert_eq!(plain.baseboards.len(), count_before);
}

/// An authored run on the same plane suppresses the generated one, exactly so
/// the Home wing's hand-placed trim is never duplicated.
#[test]
fn test_prepare_level_lets_an_authored_run_suppress_the_generated_one() {
    let mut authored = baseboard_level("");
    authored.baseboards.push(crate::level::BaseboardDef {
        x: 0.0,
        z: 0.3,
        length: 5.0,
        rotation_degrees: 0.0,
        height: crate::level::BASEBOARD_DEFAULT_HEIGHT_M,
        thickness: crate::level::BASEBOARD_DEFAULT_THICKNESS_M,
        y: Some(0.0),
        material: Some("test:trim".to_string()),
        shine: None,
    });
    let catalog = baseboard_catalog();
    prepare_level(&mut authored, &catalog, None);
    assert_eq!(
        authored.baseboards.len(),
        1,
        "the authored run owns the face; no generated duplicate"
    );
    assert_exact(authored.baseboards[0].length, 5.0);
}

/// A floor that changes along the run is not one floor: the wall fronts a step
/// or a recessed area, and trim is skipped rather than floated over the edge.
#[test]
fn test_prepare_level_skips_a_face_whose_floor_changes_along_the_run() {
    let mut level = baseboard_level(
        r#""floor_regions": [{ "x": 0.0, "z": 0.0, "width": 2.5, "depth": 5.0,
                               "offset_y": -0.5, "material": "test:plain_wall",
                               "edge_material": "test:plain_wall" }]"#,
    );
    let catalog = baseboard_catalog();
    prepare_level(&mut level, &catalog, None);
    assert!(
        level
            .baseboards
            .iter()
            .all(|board| board.material.as_deref() != Some("test:trim")),
        "a stepped floor gets no generated trim: {:?}",
        level.baseboards
    );
}

/// The shipped demo is prepared before materials decode: its office and stair
/// hall walls gain office trim, its 13 fluorescent panels end on cell centres,
/// and the authored Home runs are untouched.
#[test]
fn test_prepared_demo_aligns_panels_and_gains_office_baseboards() {
    let raw = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("places_demo parses");
    let manager = LevelManager::new();
    let loaded = manager.load_default().expect("the demo loads");
    let prepared = &loaded.level;

    assert!(
        validate_level(prepared).is_ok(),
        "the prepared level still satisfies the authored contract"
    );

    // Every fluorescent panel lands on a panel centre of its ceiling material's
    // visible grid (the office art paints four 1 m panels inside its 2 m
    // repeat); there are 13, including the stair-hall reds and the east
    // corridor strip.
    let panels: Vec<&crate::level::LightFixtureDef> = prepared
        .ceiling_lights
        .iter()
        .filter(|light| {
            crate::lighting::fixture_profile(&light.fixture).kind
                == crate::lighting::FixtureKind::FluorescentPanel
        })
        .collect();
    assert_eq!(panels.len(), 13, "the demo's panel count");
    let surfaces = LevelSurfaces::new(prepared);
    for light in panels {
        let room = surfaces
            .room_at(light.x, light.z)
            .unwrap_or_else(|| panic!("panel at ({}, {}) is inside a room", light.x, light.z));
        let material_id = room
            .ceiling_material
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| prepared.defaults.ceiling.trim());
        let period = loaded
            .materials
            .entry_of(material_id)
            .unwrap_or_else(|| panic!("ceiling material `{material_id}` resolved"))
            .grid_metres;
        assert!(period.is_finite() && period > 0.0);
        let half = period * 0.5;
        for value in [light.x, light.z] {
            let snapped = period.mul_add(((value - half) / period).round(), half);
            assert!(
                (value - snapped).abs() < 1e-4,
                "panel at ({}, {}) is off its {period} m grid",
                light.x,
                light.z
            );
        }
    }

    // The office baseboard material resolved into the renderer's table, and
    // every run appended by preparation names it.
    assert!(
        loaded
            .materials
            .index_of("core:baseboard_office_01")
            .is_some()
    );
    let generated = &prepared.baseboards[raw.baseboards.len()..];
    assert!(
        !generated.is_empty(),
        "the office walls gain automatic trim"
    );
    for board in generated {
        assert_eq!(
            board.material.as_deref(),
            Some("core:baseboard_office_01"),
            "a generated run uses the office trim"
        );
        assert!(board.y.is_some(), "a generated run is pinned to its floor");
    }
    // The Home wing's authored runs are untouched: same entry, same position,
    // same material, and no generated run names a Home trim material.
    assert_eq!(
        prepared.baseboards.len(),
        raw.baseboards.len() + generated.len()
    );
    for (before, after) in raw.baseboards.iter().zip(prepared.baseboards.iter()) {
        assert_eq!(before.material, after.material);
        assert_exact(before.x, after.x);
        assert_exact(before.z, after.z);
        assert_exact(before.length, after.length);
        assert_eq!(before.y, after.y);
    }
    for board in generated {
        assert!(
            !board
                .material
                .as_deref()
                .is_some_and(|id| id.starts_with("home:")),
            "Home walls keep only their authored runs"
        );
    }
}

/// The prepared demo's office trim is real geometry: the trim material has
/// ranges, and every one of its vertices sits at one of the floors the boards
/// were pinned to.
#[test]
fn test_prepared_demo_baseboard_geometry_sits_at_floor_level() {
    let manager = LevelManager::new();
    let loaded = manager.load_default().expect("the demo loads");
    let prepared = &loaded.level;
    let materials = crate::render::logical_materials(prepared);
    let index = materials
        .index_of("core:baseboard_office_01")
        .expect("the trim material is referenced by generated runs");
    let mesh = crate::render::build_level_geometry_with_materials(prepared, &materials);

    let mut ranges = 0_i32;
    let mut vertices = 0_i32;
    for range in mesh.ranges.iter().filter(|range| {
        range.key.kind == crate::render::SurfaceKind::Wall && range.key.material == index
    }) {
        ranges += 1_i32;
        for vertex in &range.vertices {
            vertices += 1_i32;
            let at_a_floor = [0.0_f32, -0.9, -1.5].iter().any(|floor| {
                vertex.pos[1] >= floor - 1.0e-3
                    && vertex.pos[1] <= floor + crate::level::BASEBOARD_DEFAULT_HEIGHT_M + 1.0e-3
            });
            assert!(
                at_a_floor,
                "office trim vertex at y {} is not on a floor band",
                vertex.pos[1]
            );
        }
    }
    assert!(ranges > 0_i32, "the office trim emits real ranges");
    assert!(vertices > 0_i32);
}

// ---------------------------------------------------------------------------
// instance identity, components, bindings, volumes, sequences and spawns
// ---------------------------------------------------------------------------

/// A room plus arbitrary level arrays, for component/binding validation tests.
fn binding_level(extra: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "bindings_test",
            "name": "Bindings Test",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 4.0 }} ],
            {extra}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

/// One prop with an enabled `interactable` and an `on: "interact"` binding
/// whose actions are `actions`.
fn interact_prop_json(id: &str, actions: &str) -> String {
    format!(
        r#"{{ "id": "{id}", "model": "core:switch", "x": 2.0, "z": 2.0,
              "components": [ {{ "component": "interactable" }} ],
              "bindings": [ {{ "on": "interact", "actions": [{actions}] }} ] }}"#
    )
}

/// The positive path: a level that authors at least one of every new record --
/// components, bindings, conditions, sequences, timers, volumes, spawn
/// templates, spawn points, spawn groups and effects -- validates whole.
#[test]
fn test_validate_accepts_every_new_feature() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "every_feature",
            "name": "Every Feature",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 } ],
            "props": [
                { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
                  "components": [
                      { "component": "interactable", "prompt": "Press", "reach": 2.5 },
                      { "component": "state", "name": "pressed", "value": 0 }
                  ],
                  "bindings": [
                      { "on": "interact",
                        "when": [ { "check": "state", "target": "switch",
                                    "name": "pressed", "equals": 0 } ],
                        "actions": [
                            { "action": "set_state", "name": "pressed", "value": 1 },
                            { "action": "start_sequence", "sequence": "blink" }
                        ] }
                  ] },
                { "id": "lamp", "model": "core:lamp", "x": 4.0, "z": 2.0,
                  "display_name": "Lamp",
                  "components": [
                      { "component": "light", "enabled": true, "emission_scale": 1.0 },
                      { "component": "material",
                        "variants": [
                            { "name": "off", "emission_scale": 0.0 },
                            { "name": "on", "emission_scale": 1.0 }
                        ],
                        "current": "on" },
                      { "component": "state", "name": "level", "value": 1 }
                  ],
                  "bindings": [
                      { "on": "object_state", "key": "level",
                        "actions": [
                            { "action": "change_material", "target": "rat",
                              "variant": "off" },
                            { "action": "set_light", "target": "ceiling_panel", "on": false },
                            { "action": "toggle_label", "target": "lamp" }
                        ] }
                  ] },
                { "id": "radio", "model": "core:radio", "x": 6.0, "z": 2.0,
                  "components": [
                      { "component": "audio", "sound": "core:radio_hum" },
                      { "component": "animation", "clip": "idle" }
                  ],
                  "bindings": [
                      { "on": "animation_complete",
                        "actions": [
                            { "action": "play_sound", "target": "radio",
                              "sound": "core:radio_hum", "loop": true },
                            { "action": "toggle_animation", "target": "radio",
                              "clip": "idle" }
                        ] }
                  ] },
                { "id": "phone", "model": "core:phone", "x": 8.0, "z": 2.0,
                  "components": [
                      { "component": "interactable" },
                      { "component": "lifetime", "seconds": 30.0 }
                  ],
                  "bindings": [
                      { "on": "interact",
                        "actions": [ { "action": "despawn_entity", "target": "rats" } ] }
                  ] },
                { "id": "button", "model": "core:button", "x": 10.0, "z": 2.0,
                  "components": [ { "component": "interactable" } ],
                  "bindings": [
                      { "on": "interact",
                        "actions": [
                            { "action": "spawn_entity", "point": "rat_point" },
                            { "action": "start_timer", "target": "ticker" }
                        ] }
                  ] }
            ],
            "doors": [
                { "id": "front_door", "locked": true, "x": 12.0, "z": 2.0,
                  "width": 0.9, "height": 2.1, "rotation_degrees": 0.0,
                  "components": [ { "component": "interactable", "prompt": "Open" } ],
                  "bindings": [
                      { "on": "interact",
                        "when": [ { "check": "unlocked", "target": "front_door" } ],
                        "actions": [ { "action": "toggle" } ] }
                  ] }
            ],
            "ceiling_lights": [
                { "id": "ceiling_panel", "fixture": "core:ceiling_panel_01",
                  "x": 5.0, "z": 5.0, "switchable": true,
                  "bindings": [
                      { "on": "object_state",
                        "actions": [ { "action": "set_light", "on": false } ] }
                  ] }
            ],
            "volumes": [
                { "id": "pit", "x": 3.0, "z": 3.0, "width": 2.0, "depth": 2.0,
                  "bottom_y": -2.0, "top_y": 0.0,
                  "bindings": [
                      { "on": "enter_volume",
                        "actions": [ { "action": "reset_to_start" } ] },
                      { "on": "exit_volume",
                        "actions": [ { "action": "enable", "target": "switch" } ] }
                  ] }
            ],
            "timers": [ { "id": "ticker", "seconds": 2.0, "repeat": true, "autostart": true } ],
            "sequences": [
                { "id": "blink", "steps": [
                    { "step": "set_state", "name": "pressed", "value": 2 },
                    { "step": "wait", "seconds": 0.5 },
                    { "step": "action",
                      "action": { "action": "change_material", "target": "rat",
                                  "variant": "on" } },
                    { "step": "wait_animation", "clip": "idle", "timeout": 2.0 },
                    { "step": "emit", "on": "sequence_complete" },
                    { "step": "stop" }
                ] }
            ],
            "spawn_templates": [
                { "id": "rat", "model": "core:cardboard_box", "scale": 0.5,
                  "lifetime_seconds": 30.0,
                  "components": [ { "component": "state", "name": "phase", "value": "idle" },
                                  { "component": "material",
                                    "variants": [ { "name": "off", "emission_scale": 0.0 },
                                                  { "name": "on", "emission_scale": 1.0 } ],
                                    "current": "on" } ],
                  "bindings": [
                      { "on": "spawn",
                        "actions": [ { "action": "set_state", "name": "phase",
                                       "value": "active" } ] }
                  ] }
            ],
            "spawn_points": [
                { "id": "rat_point", "x": 4.0, "z": 8.0, "template": "rat", "group": "rats",
                  "bindings": [
                      { "on": "spawn",
                        "actions": [ { "action": "despawn_entity", "target": "phone" } ] }
                  ] }
            ],
            "spawn_groups": [ { "id": "rats", "at_most_one_active": true } ],
            "effects": [
                { "id": "steam", "kind": "steam", "x": 2.0, "z": 2.0,
                  "bindings": [
                      { "on": "spawn", "actions": [ { "action": "disable" } ] }
                  ] }
            ]
        }"#,
    )
    .expect("the every-feature level parses");
    validate_level(&level).expect("a level using every new feature validates");
    assert_eq!(level.volumes.len(), 1);
    assert_eq!(level.timers.len(), 1);
    assert_eq!(level.sequences.len(), 1);
    assert_eq!(level.spawn_templates.len(), 1);
    assert_eq!(level.spawn_points.len(), 1);
    assert_eq!(level.spawn_groups.len(), 1);
}

/// Duplicate and malformed instance ids are named errors; the default scheme
/// is included in the uniqueness set, so an authored `chair_2` cannot collide
/// with the second defaulted chair.
#[test]
fn test_validate_rejects_duplicate_and_malformed_instance_ids() {
    let duplicate = level_with_props_json(
        r#"[{ "id": "same", "model": "core:chair", "x": 1.0, "z": 1.0 },
            { "id": "same", "model": "core:chair", "x": 2.0, "z": 1.0 }]"#,
    );
    let err = validate_level(&duplicate).expect_err("duplicate ids are invalid");
    assert!(err.contains("duplicates"), "unexpected error: {err}");
    assert!(err.contains("same"), "the error names the id: {err}");

    let default_collision = level_with_props_json(
        r#"[{ "id": "chair_1", "model": "core:chair", "x": 1.0, "z": 1.0 },
            { "model": "core:chair", "x": 2.0, "z": 1.0 }]"#,
    );
    let default_collision_error =
        validate_level(&default_collision).expect_err("an authored id cannot shadow a default id");
    assert!(
        default_collision_error.contains("chair_1"),
        "the error names the id: {default_collision_error}"
    );

    let malformed =
        level_with_props_json(r#"[{ "id": "bad id", "model": "core:chair", "x": 1.0, "z": 1.0 }]"#);
    let malformed_error = validate_level(&malformed).expect_err("a spaced id is malformed");
    assert!(
        malformed_error.contains("well-formed"),
        "unexpected error: {malformed_error}"
    );
}

/// Every placeable record shares one instance-id namespace: a prop and a door
/// (or a timer, volume or spawn point) cannot share a name, while a sequence
/// has its own namespace whose duplicates are still named.
#[test]
fn test_validate_rejects_duplicate_ids_across_kinds() {
    let prop_door = binding_level(
        r#""props": [ { "id": "same", "model": "core:chair", "x": 2.0, "z": 2.0 } ],
           "doors": [ { "id": "same", "x": 4.0, "z": 4.0, "width": 0.9, "height": 2.1 } ]"#,
    );
    let err = validate_level(&prop_door).expect_err("a prop and a door cannot share an id");
    assert!(err.contains("duplicates"), "unexpected error: {err}");
    assert!(err.contains("`same`"), "the error names the id: {err}");

    let prop_timer = binding_level(
        r#""props": [ { "id": "tick", "model": "core:clock", "x": 2.0, "z": 2.0 } ],
           "timers": [ { "id": "tick", "seconds": 1.0 } ]"#,
    );
    let prop_timer_error =
        validate_level(&prop_timer).expect_err("a prop and a timer cannot share an id");
    assert!(
        prop_timer_error.contains("`tick`"),
        "the error names the id: {prop_timer_error}"
    );

    let duplicate_sequences = binding_level(
        r#""sequences": [ { "id": "same", "steps": [ { "step": "stop" } ] },
                          { "id": "same", "steps": [ { "step": "stop" } ] } ]"#,
    );
    let duplicate_sequences_error =
        validate_level(&duplicate_sequences).expect_err("sequence ids must be unique");
    assert!(
        duplicate_sequences_error.contains("sequence ids must be unique per level"),
        "unexpected error: {duplicate_sequences_error}"
    );
    assert!(
        duplicate_sequences_error.contains("`same`"),
        "the error names the id: {duplicate_sequences_error}"
    );

    let blank_spawn_point = binding_level(
        r#""spawn_templates": [ { "id": "rat", "model": "core:box" } ],
           "spawn_points": [ { "id": "  ", "x": 2.0, "z": 2.0, "template": "rat" } ]"#,
    );
    let blank_spawn_point_error =
        validate_level(&blank_spawn_point).expect_err("a blank id is malformed");
    assert!(
        blank_spawn_point_error.contains("well-formed"),
        "unexpected error: {blank_spawn_point_error}"
    );
}

/// Components are capability contracts: a non-repeatable kind may appear once
/// per entity, and only `state` may repeat with distinct names.
#[test]
fn test_validate_rejects_duplicate_component_kinds() {
    let two_interactables = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "interactable" } ] } ]"#,
    );
    let err = validate_level(&two_interactables).expect_err("two interactables are invalid");
    assert!(
        err.contains("more than one `interactable` component"),
        "unexpected error: {err}"
    );
    assert!(err.contains("`switch`"), "the error names the prop: {err}");

    let repeated_state = binding_level(
        r#""props": [ { "id": "panel", "model": "core:panel", "x": 2.0, "z": 2.0,
            "components": [ { "component": "state", "name": "phase", "value": 0 },
                            { "component": "state", "name": "phase", "value": 1 } ] } ]"#,
    );
    let repeated_state_error =
        validate_level(&repeated_state).expect_err("repeated state names are invalid");
    assert!(
        repeated_state_error.contains("two `state` components named `phase`"),
        "unexpected error: {repeated_state_error}"
    );

    // Distinct state names are the one legal repetition.
    let distinct_states = binding_level(
        r#""props": [ { "id": "panel", "model": "core:panel", "x": 2.0, "z": 2.0,
            "components": [ { "component": "state", "name": "a", "value": 0 },
                            { "component": "state", "name": "b", "value": 1 } ] } ]"#,
    );
    validate_level(&distinct_states).expect("distinct state names may repeat");
}

/// `fade` and `glow` accept the documented examples and every omitted field
/// resolves its documented default.
#[test]
fn test_validate_accepts_fade_and_glow_components() {
    let level = binding_level(
        r#""props": [ { "id": "ghost", "model": "entity:sheet-ghost", "x": 2.0, "z": 2.0,
            "components": [
                { "component": "fade", "period_seconds": 6.0, "phase": 0.25,
                  "min_opacity": 0.0, "max_opacity": 1.0, "enabled": true },
                { "component": "glow", "color": [0.45, 0.95, 1.0], "intensity": 0.6,
                  "range": 3.5, "socket": "flame", "offset": [0.0, 0.1, 0.0],
                  "fade": true } ] } ]"#,
    );
    validate_level(&level).expect("the documented fade/glow examples validate");

    // A plain prop may carry a glow; only the components' own values are
    // checked here (the runtime's character path decides where it updates).
    let defaults = binding_level(
        r#""props": [ { "id": "lamp_post", "model": "core:lamp", "x": 2.0, "z": 2.0,
            "components": [ { "component": "fade", "period_seconds": 2.0 },
                            { "component": "glow" } ] } ]"#,
    );
    validate_level(&defaults).expect("fade/glow defaults validate");
    let prop = defaults.props.first().expect("the prop parses");
    let fade = prop
        .components
        .iter()
        .find_map(|component| {
            if let crate::level::ComponentDef::Fade(def) = component {
                Some(def)
            } else {
                None
            }
        })
        .expect("the fade parses");
    assert_eq!(fade.period_seconds, 2.0);
    assert_eq!(fade.phase, None, "an omitted phase stays unresolved");
    assert_eq!(fade.min_opacity, 0.0);
    assert_eq!(fade.max_opacity, 1.0);
    assert!(fade.enabled);
    let glow = prop
        .components
        .iter()
        .find_map(|component| {
            if let crate::level::ComponentDef::Glow(def) = component {
                Some(def)
            } else {
                None
            }
        })
        .expect("the glow parses");
    assert_eq!(glow.intensity, 0.5);
    assert_eq!(glow.range, 3.0);
    assert_eq!(glow.socket, None);
    assert!(glow.fade);
    // Two instance ids get independent deterministic phases without one.
    let first = crate::level::default_fade_phase("ghost_a");
    let second = crate::level::default_fade_phase("ghost_b");
    assert!((0.0..1.0).contains(&first) && (0.0..1.0).contains(&second));
    assert_ne!(first.to_bits(), second.to_bits());
}

/// Every component value is checked by name: reach, lifetime, emission,
/// variants, state names, clips, sounds and nav metadata.
#[test]
fn test_validate_rejects_bad_component_values() {
    let cases = [
        (
            r#"{ "component": "interactable", "reach": 0.0 }"#,
            "`interactable` component 0 reach",
            "reach 0",
        ),
        (
            r#"{ "component": "interactable", "reach": 9.0 }"#,
            "`interactable` component 0 reach",
            "reach above the cap",
        ),
        (
            r#"{ "component": "interactable", "prompt": "  " }"#,
            "prompt must not be blank",
            "blank prompt",
        ),
        (
            r#"{ "component": "lifetime", "seconds": -1.0 }"#,
            "`lifetime` component 0 seconds",
            "negative lifetime",
        ),
        (
            r#"{ "component": "lifetime", "seconds": 7200.0 }"#,
            "`lifetime` component 0 seconds",
            "lifetime above the cap",
        ),
        (
            r#"{ "component": "light", "emission_scale": -1.0 }"#,
            "`light` component 0 emission_scale",
            "negative light emission",
        ),
        (
            r#"{ "component": "material", "variants": [] }"#,
            "must declare at least one variant",
            "no variants",
        ),
        (
            r#"{ "component": "material", "variants": [ { "name": "  " } ] }"#,
            "must name a non-empty variant",
            "blank variant name",
        ),
        (
            r#"{ "component": "material",
                "variants": [ { "name": "on" }, { "name": "on" } ] }"#,
            "declares variant `on` twice",
            "duplicate variant name",
        ),
        (
            r#"{ "component": "material",
                "variants": [ { "name": "off" } ], "current": "on" }"#,
            "current `on` is not one of its variants",
            "unknown current variant",
        ),
        (
            r#"{ "component": "material", "variants": [ { "name": "on", "emission_scale": -1.0 } ] }"#,
            "variant `on` emission_scale",
            "negative variant emission",
        ),
        (
            r#"{ "component": "state", "name": "  ", "value": 0 }"#,
            "must name a non-empty state",
            "blank state name",
        ),
        (
            r#"{ "component": "animation", "clip": "  " }"#,
            "`animation` component 0 clip must not be blank",
            "blank clip",
        ),
        (
            r#"{ "component": "audio", "sound": "  " }"#,
            "`audio` component 0 sound must not be blank",
            "blank sound",
        ),
        (
            r#"{ "component": "nav_agent", "radius": 0.0, "speed_mps": 1.0 }"#,
            "must be finite and positive",
            "zero nav radius",
        ),
        (
            r#"{ "component": "nav_agent", "radius": 0.5, "speed_mps": 0.0 }"#,
            "must be finite and positive",
            "zero nav speed",
        ),
        (
            r#"{ "component": "ai", "behavior": "prey" }"#,
            "authors an `ai` component without a `nav_agent` body",
            "an ai needs a body",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 0.0 }"#,
            "`fade` component 0 period_seconds",
            "zero fade period",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 3601.0 }"#,
            "`fade` component 0 period_seconds",
            "fade period above the cap",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 1.0, "phase": 1.5 }"#,
            "`fade` component 0 phase",
            "fade phase above one",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 1.0, "min_opacity": -0.1 }"#,
            "`fade` component 0 min_opacity",
            "negative min opacity",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 1.0, "max_opacity": 1.5 }"#,
            "`fade` component 0 max_opacity",
            "max opacity above one",
        ),
        (
            r#"{ "component": "fade", "period_seconds": 1.0,
                "min_opacity": 0.8, "max_opacity": 0.2 }"#,
            "must not exceed max_opacity",
            "inverted opacity range",
        ),
        (
            r#"{ "component": "glow", "color": [0.0, 1.5, 0.0] }"#,
            "`glow` component 0 color",
            "glow colour above one",
        ),
        (
            r#"{ "component": "glow", "intensity": 8.5 }"#,
            "`glow` component 0 intensity",
            "glow intensity above the cap",
        ),
        (
            r#"{ "component": "glow", "range": 0.01 }"#,
            "`glow` component 0 range",
            "glow range below the floor",
        ),
        (
            r#"{ "component": "glow", "range": 65.0 }"#,
            "`glow` component 0 range",
            "glow range above the cap",
        ),
        (
            r#"{ "component": "glow", "socket": "  " }"#,
            "socket must not be blank",
            "blank glow socket",
        ),
        (
            r#"{ "component": "glow", "offset": [0.0, 4.5, 0.0] }"#,
            "`glow` component 0 offset",
            "glow offset above the cap",
        ),
    ];
    for (component, expected, label) in cases {
        let level = binding_level(&format!(
            r#""props": [ {{ "id": "thing", "model": "core:thing", "x": 2.0, "z": 2.0,
                "components": [{component}] }} ]"#
        ));
        let err = validate_level(&level).expect_err(label);
        assert!(
            err.contains(expected),
            "{label}: expected `{expected}` in: {err}"
        );
        assert!(
            err.contains("`thing`"),
            "{label}: the error names the prop: {err}"
        );
    }
}

/// `open`, `close`, `lock`, `unlock`, `set_light`, `change_material`,
/// `play_animation`, `play_sound`, `move_object` and friends only fit the
/// target kinds that implement them.
#[test]
fn test_validate_rejects_action_target_mismatches() {
    let open_on_light = binding_level(
        r#""props": [ { "id": "lamp", "model": "core:lamp", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "light" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "open" } ] } ] } ]"#,
    );
    let err = validate_level(&open_on_light).expect_err("open needs a door");
    assert!(
        err.contains("`open` requires a door target"),
        "unexpected error: {err}"
    );
    assert!(err.contains("`lamp`"), "the error names the target: {err}");

    let lock_on_prop = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json("plant", r#"{ "action": "lock" }"#)
    ));
    let lock_on_prop_error = validate_level(&lock_on_prop).expect_err("lock needs a door");
    assert!(
        lock_on_prop_error.contains("requires a door target"),
        "unexpected error: {lock_on_prop_error}"
    );

    let static_material = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "plant",
            r#"{ "action": "change_material", "variant": "off" }"#
        )
    ));
    let static_material_error =
        validate_level(&static_material).expect_err("a static prop has no material");
    assert!(
        static_material_error.contains("a baked static prop's material is prepared geometry"),
        "the error names the rule: {static_material_error}"
    );
    assert!(
        static_material_error.contains("`plant`"),
        "the error names the target: {static_material_error}"
    );

    // A switchable `light` component on a prop is rejected: only a ceiling
    // fixture has prepared switchable layers.
    let switchable_prop_light = binding_level(
        r#""props": [ { "id": "lamp", "model": "core:lamp", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "light", "switchable": true } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "set_light", "on": true } ] } ] } ]"#,
    );
    let switchable_prop_light_error =
        validate_level(&switchable_prop_light).expect_err("a prop light cannot be switchable");
    assert!(
        switchable_prop_light_error
            .contains("only a ceiling fixture has prepared switchable lightmap layers"),
        "unexpected error: {switchable_prop_light_error}"
    );

    // A non-switchable light is not a `set_light` target either.
    let static_light_target = binding_level(
        r#""props": [ { "id": "lamp", "model": "core:lamp", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "light" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "set_light", "on": true } ] } ] } ]"#,
    );
    let static_light_target_error = validate_level(&static_light_target)
        .expect_err("a non-switchable light is not a set_light target");
    assert!(
        static_light_target_error.contains("a switchable light"),
        "unexpected error: {static_light_target_error}"
    );

    let bad_variant = binding_level(
        r#""spawn_templates": [ { "id": "screen", "model": "core:screen",
            "components": [ { "component": "material",
                              "variants": [ { "name": "off", "emission_scale": 0.0 } ] } ] } ],
           "props": [ { "id": "button", "model": "core:button", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "change_material", "target": "screen",
                               "variant": "missing" } ] } ] } ]"#,
    );
    let bad_variant_error = validate_level(&bad_variant).expect_err("the variant must exist");
    assert!(
        bad_variant_error.contains("no variant `missing`"),
        "unexpected error: {bad_variant_error}"
    );

    let no_animation = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "mannequin",
            r#"{ "action": "play_animation", "clip": "wave" }"#
        )
    ));
    let no_animation_error =
        validate_level(&no_animation).expect_err("play_animation needs an animation");
    assert!(
        no_animation_error.contains("requires a target with an `animation` component"),
        "unexpected error: {no_animation_error}"
    );

    let no_audio = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json("bell", r#"{ "action": "play_sound" }"#)
    ));
    let no_audio_error = validate_level(&no_audio).expect_err("play_sound needs audio");
    assert!(
        no_audio_error.contains("requires a target with an `audio` component"),
        "unexpected error: {no_audio_error}"
    );

    let static_move = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "crate",
            r#"{ "action": "move_object", "x": 3.0, "z": 3.0 }"#
        )
    ));
    let static_move_error =
        validate_level(&static_move).expect_err("a baked static prop cannot move");
    assert!(
        static_move_error.contains("a baked static prop and a door cannot move"),
        "unexpected error: {static_move_error}"
    );

    let label_on_volume = binding_level(
        r#""volumes": [ { "id": "pit", "x": 3.0, "z": 3.0, "width": 2.0, "depth": 2.0,
            "bindings": [ { "on": "enter_volume",
                "actions": [ { "action": "toggle_label" } ] } ] } ]"#,
    );
    let label_on_volume_error =
        validate_level(&label_on_volume).expect_err("a volume has no label");
    assert!(
        label_on_volume_error.contains("requires a placed prop target"),
        "unexpected error: {label_on_volume_error}"
    );
    assert!(
        label_on_volume_error.contains("trigger volume"),
        "the error names the actor: {label_on_volume_error}"
    );

    // A door moves through open/close/toggle, never through move_object.
    let move_door = binding_level(
        r#""doors": [ { "id": "front_door", "x": 4.0, "z": 4.0, "width": 0.9, "height": 2.1,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "move_object", "target": "front_door",
                               "x": 4.5, "z": 4.0, "speed": 1.0 } ] } ] } ]"#,
    );
    let move_door_error = validate_level(&move_door).expect_err("a door cannot be moved");
    assert!(
        move_door_error.contains("move_object") && move_door_error.contains("open"),
        "the error names the door rule: {move_door_error}"
    );

    let move_template = binding_level(
        r#""spawn_templates": [ { "id": "crate", "model": "core:cardboard_box" } ],
           "props": [ { "id": "lever", "model": "core:lever", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "move_object", "target": "crate",
                               "x": 5.0, "z": 5.0 } ] } ] } ]"#,
    );
    validate_level(&move_template).expect("a spawn template target can move");
}

/// Unknown action targets and unknown sequences, spawn points, spawn groups
/// and spawn templates are all named errors.
#[test]
fn test_validate_rejects_unknown_action_targets() {
    let unknown_entity = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "toggle_label", "target": "ghost" }"#
        )
    ));
    let err = validate_level(&unknown_entity).expect_err("the target must resolve");
    assert!(
        err.contains("unknown entity `ghost`"),
        "unexpected error: {err}"
    );
    assert!(err.contains("`switch`"), "the error names the actor: {err}");

    let unknown_sequence = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "start_sequence", "sequence": "ghost" }"#
        )
    ));
    let unknown_sequence_error =
        validate_level(&unknown_sequence).expect_err("the sequence must resolve");
    assert!(
        unknown_sequence_error.contains("unknown sequence `ghost`"),
        "unexpected error: {unknown_sequence_error}"
    );

    let unknown_point = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "spawn_entity", "point": "ghost" }"#
        )
    ));
    let unknown_point_error = validate_level(&unknown_point).expect_err("the point must resolve");
    assert!(
        unknown_point_error.contains("unknown spawn point `ghost`"),
        "unexpected error: {unknown_point_error}"
    );

    let unknown_timer = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "start_timer", "target": "ghost" }"#
        )
    ));
    let unknown_timer_error = validate_level(&unknown_timer).expect_err("the timer must resolve");
    assert!(
        unknown_timer_error.contains("unknown entity `ghost`"),
        "unexpected error: {unknown_timer_error}"
    );

    let unknown_despawn = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "despawn_entity", "target": "ghost" }"#
        )
    ));
    let unknown_despawn_error =
        validate_level(&unknown_despawn).expect_err("despawn needs a real target");
    assert!(
        unknown_despawn_error.contains("spawn group id"),
        "the error explains what despawn accepts: {unknown_despawn_error}"
    );
}

/// Conditions must resolve, `state` must name an authored state, and
/// door-only checks need a door.
#[test]
fn test_validate_rejects_unknown_condition_targets_and_states() {
    let unknown_target = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "when": [ { "check": "enabled", "target": "ghost" } ],
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let err = validate_level(&unknown_target).expect_err("the condition target must resolve");
    assert!(
        err.contains("condition 0 (`enabled`) targets unknown entity `ghost`"),
        "unexpected error: {err}"
    );

    let unknown_state = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "when": [ { "check": "state", "target": "switch",
                            "name": "missing", "equals": 0 } ],
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let unknown_state_error = validate_level(&unknown_state).expect_err("the state must exist");
    assert!(
        unknown_state_error.contains("reads state `missing` on `switch`"),
        "unexpected error: {unknown_state_error}"
    );

    let door_condition_on_prop = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "when": [ { "check": "locked", "target": "switch" } ],
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let door_condition_on_prop_error =
        validate_level(&door_condition_on_prop).expect_err("locked needs a door target");
    assert!(
        door_condition_on_prop_error.contains("need a door"),
        "unexpected error: {door_condition_on_prop_error}"
    );
    assert!(
        door_condition_on_prop_error.contains("`switch`"),
        "the error names the target: {door_condition_on_prop_error}"
    );
}

/// `set_state` is strict: it may only change a state the target already
/// authors (timers and trigger volumes are the runtime-owned exception).
#[test]
fn test_validate_rejects_invented_state_writes() {
    let invented = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "set_state", "name": "invented", "value": 1 }"#
        )
    ));
    let err = validate_level(&invented).expect_err("a new state name cannot be invented");
    assert!(
        err.contains("`set_state` requires an authored state named `invented`"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("`switch`"),
        "the error names the target: {err}"
    );

    let blank = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "set_state", "name": "  ", "value": 1 }"#
        )
    ));
    let blank_error = validate_level(&blank).expect_err("a blank state name is invalid");
    assert!(
        blank_error.contains("needs a non-empty state name"),
        "unexpected error: {blank_error}"
    );

    let authored = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "state", "name": "pressed", "value": 0 } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "set_state", "name": "pressed", "value": 1 } ] } ] } ]"#,
    );
    validate_level(&authored).expect("a state the target authors can be written");
}

/// A synthesized water/effect id shares the instance namespace: an authored id
/// cannot shadow the entity `enable`/`disable` addresses.
#[test]
fn test_validate_rejects_shadowed_water_and_effect_ids() {
    let shadowed_water = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "shadow_water",
            "name": "Shadow Water",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "water": [ { "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
                         "surface_y": 0.0, "bottom_y": -2.0 } ],
            "props": [ { "id": "water_1", "model": "core:crate", "x": 2.0, "z": 2.0 } ]
        }"#,
    )
    .expect("the shadow level parses");
    let err = validate_level(&shadowed_water).expect_err("the synthesized id is reserved");
    assert!(err.contains("water_1"), "the error names the id: {err}");

    let shadowed_effect = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "shadow_effect",
            "name": "Shadow Effect",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
            "effects": [ { "kind": "steam", "x": 4.0, "z": 4.0 } ],
            "props": [ { "id": "effect_1", "model": "core:crate", "x": 2.0, "z": 2.0 } ]
        }"#,
    )
    .expect("the shadow effect level parses");
    let shadowed_effect_error =
        validate_level(&shadowed_effect).expect_err("the synthesized id is reserved");
    assert!(
        shadowed_effect_error.contains("effect_1"),
        "the error names the id: {shadowed_effect_error}"
    );
}

/// A binding only validates on a record that can emit its event.
#[test]
fn test_validate_rejects_bindings_that_cannot_fire() {
    // A `timer` binding is legal on any record: a sequence's `emit` step can
    // publish the same cue, so a prop listening for one is a real pattern.
    let timer_on_prop = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json("switch", r#"{ "action": "reset_to_start" }"#)
            .replace(r#""on": "interact""#, r#""on": "timer""#)
    ));
    validate_level(&timer_on_prop).expect("a prop may listen for a timer cue");

    let volume_on_door = binding_level(
        r#""doors": [ { "id": "front_door", "x": 4.0, "z": 4.0, "width": 0.9, "height": 2.1,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "enter_volume",
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let err = validate_level(&volume_on_door).expect_err("a door emits no volume event");
    assert!(
        err.contains("listens for `enter_volume`") && err.contains("trigger volume"),
        "unexpected error: {err}"
    );

    let animation_without_component = binding_level(
        r#""props": [ { "id": "radio", "model": "core:radio", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "animation_complete",
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let animation_without_component_error = validate_level(&animation_without_component)
        .expect_err("animation_complete needs an animation component");
    assert!(
        animation_without_component_error.contains("has no `animation` component"),
        "unexpected error: {animation_without_component_error}"
    );

    let disabled_interactable = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable", "enabled": false } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let disabled_interactable_error = validate_level(&disabled_interactable)
        .expect_err("a disabled interactable never emits interact");
    assert!(
        disabled_interactable_error.contains("no enabled `interactable` component"),
        "unexpected error: {disabled_interactable_error}"
    );

    // Acceptance: the kinds that own an event may listen for it.
    let legal = binding_level(
        r#""volumes": [ { "id": "pit", "x": 3.0, "z": 3.0, "width": 2.0, "depth": 2.0,
              "bindings": [ { "on": "enter_volume",
                              "actions": [ { "action": "reset_to_start" } ] } ] } ],
           "props": [ { "id": "radio", "model": "core:radio", "x": 2.0, "z": 2.0,
              "components": [ { "component": "animation", "clip": "idle" } ],
              "bindings": [ { "on": "animation_complete",
                              "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    validate_level(&legal).expect("each event owner may listen for its own event");
}

/// Binding lists, action lists, cooldowns and ids are all bounded and
/// non-empty; a malformed one is a named error.
#[test]
fn test_validate_rejects_oversized_or_malformed_bindings() {
    let bindings: Vec<&str> = (0..=crate::level::MAX_BINDINGS_PER_ENTITY)
        .map(|_| r#"{ "on": "interact", "actions": [{ "action": "reset_to_start" }] }"#)
        .collect();
    let too_many_bindings = binding_level(&format!(
        r#""props": [ {{ "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ {{ "component": "interactable" }} ],
            "bindings": [{}] }} ]"#,
        bindings.join(",")
    ));
    let err = validate_level(&too_many_bindings).expect_err("an oversized binding list is invalid");
    assert!(err.contains("the limit is"), "unexpected error: {err}");
    assert!(
        err.contains("`switch`"),
        "the error names the record: {err}"
    );

    let actions: Vec<&str> = (0..=crate::level::MAX_ACTIONS_PER_SOURCE)
        .map(|_| r#"{ "action": "reset_to_start" }"#)
        .collect();
    let too_many_actions = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json("switch", &actions.join(","))
    ));
    let too_many_actions_error =
        validate_level(&too_many_actions).expect_err("an oversized action batch is invalid");
    assert!(
        too_many_actions_error.contains("the limit is"),
        "unexpected error: {too_many_actions_error}"
    );

    let empty_actions = binding_level(&format!(
        r#""props": [{}]"#,
        interact_prop_json("switch", "")
    ));
    let empty_actions_error =
        validate_level(&empty_actions).expect_err("a binding needs an action");
    assert!(
        empty_actions_error.contains("must declare at least one action"),
        "unexpected error: {empty_actions_error}"
    );

    let bad_cooldown = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact", "cooldown_seconds": -1.0,
                            "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let bad_cooldown_error =
        validate_level(&bad_cooldown).expect_err("a negative cooldown is invalid");
    assert!(
        bad_cooldown_error
            .contains("cooldown_seconds must be a finite number that is not negative"),
        "unexpected error: {bad_cooldown_error}"
    );

    let blank_binding_id = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "id": "  ", "on": "interact",
                            "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    let blank_binding_id_error =
        validate_level(&blank_binding_id).expect_err("a blank binding id is invalid");
    assert!(
        blank_binding_id_error.contains("id must not be blank when specified"),
        "unexpected error: {blank_binding_id_error}"
    );
}

/// Sequences are bounded, their delays are ranged and their step values are
/// finite; a `wait_animation` clip is only shape-checked because the loader does
/// not know a model's clip set (the runtime timeout bounds a missing clip).
#[test]
fn test_validate_rejects_malformed_sequences() {
    let cases = [
        (
            r#"{ "id": "empty", "steps": [] }"#,
            "declares no steps",
            "empty step list",
        ),
        (
            r#"{ "id": "bad_wait", "steps": [ { "step": "wait", "seconds": -1.0 } ] }"#,
            "between 0 and 600",
            "negative wait",
        ),
        (
            r#"{ "id": "bad_move", "steps": [ { "step": "move", "x": 2.0, "z": 2.0,
                "speed": 0.0 } ] }"#,
            "speed positive",
            "zero move speed",
        ),
        (
            r#"{ "id": "bad_animation", "steps": [
                { "step": "wait_animation", "clip": "idle", "timeout": 1000.0 } ] }"#,
            "between 0 and 120",
            "oversized animation timeout",
        ),
        (
            r#"{ "id": "blank_clip", "steps": [
                { "step": "wait_animation", "clip": "  ", "timeout": 1.0 } ] }"#,
            "clip must not be blank",
            "blank wait_animation clip",
        ),
        (
            r#"{ "id": "blank_emit", "steps": [
                { "step": "emit", "on": "timer", "key": "  " } ] }"#,
            "key must not be blank",
            "blank emit key",
        ),
        (
            r#"{ "id": "bad_state", "steps": [
                { "step": "set_state", "name": "  ", "value": 0 } ] }"#,
            "must name a non-empty state",
            "blank sequence state name",
        ),
    ];
    for (sequence, expected, label) in cases {
        let level = binding_level(&format!(r#""sequences": [{sequence}]"#));
        let err = validate_level(&level).expect_err(label);
        assert!(
            err.contains("Sequence 0"),
            "{label}: the error names the sequence: {err}"
        );
        assert!(
            err.contains(expected),
            "{label}: expected `{expected}` in: {err}"
        );
    }
}

/// The obvious zero-delay cycle is refused, while a cycle that passes through
/// a wait is legal.
#[test]
fn test_validate_rejects_zero_delay_cycles_but_allows_delayed_ones() {
    let cycle = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "start_sequence", "sequence": "loop" } ] } ] } ],
           "sequences": [ { "id": "loop", "steps": [
                { "step": "action",
                  "action": { "action": "start_sequence", "sequence": "loop" } } ] } ]"#,
    );
    let err = validate_level(&cycle).expect_err("an immediate self-restart is a zero-delay cycle");
    assert!(err.contains("Zero-delay cycle"), "unexpected error: {err}");
    assert!(
        err.contains("sequence `loop`"),
        "the cycle path names it: {err}"
    );
    assert!(
        err.contains("without consuming a wait"),
        "the error explains the cycle: {err}"
    );

    let delayed = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [ { "on": "interact",
                "actions": [ { "action": "start_sequence", "sequence": "loop" } ] } ] } ],
           "sequences": [ { "id": "loop", "steps": [
                { "step": "wait", "seconds": 1.0 },
                { "step": "action",
                  "action": { "action": "start_sequence", "sequence": "loop" } } ] } ]"#,
    );
    validate_level(&delayed).expect("a cycle behind a wait is legal repetition");

    // A sequence that completes immediately and is restarted by its own
    // `sequence_complete` binding is a zero-delay cycle...
    let completion_cycle = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" } ],
            "bindings": [
                { "on": "interact",
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] },
                { "on": "sequence_complete",
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] }
            ] } ],
           "sequences": [ { "id": "once", "steps": [ { "step": "stop" } ] } ]"#,
    );
    let completion_cycle_error = validate_level(&completion_cycle)
        .expect_err("an immediate completion that restarts itself is a zero-delay cycle");
    assert!(
        completion_cycle_error.contains("Zero-delay cycle"),
        "unexpected error: {completion_cycle_error}"
    );
    assert!(
        completion_cycle_error.contains("binding 1 on `switch`"),
        "the cycle path names the re-entering binding: {completion_cycle_error}"
    );

    // ... unless a cooldown or a condition bounds the repetition.
    let cooled = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "state", "name": "done", "value": 0 } ],
            "bindings": [
                { "on": "interact",
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] },
                { "on": "sequence_complete", "cooldown_seconds": 0.5,
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] }
            ] } ],
           "sequences": [ { "id": "once", "steps": [ { "step": "stop" } ] } ]"#,
    );
    validate_level(&cooled).expect("a cooldown bounds the repetition");

    let conditioned = binding_level(
        r#""props": [ { "id": "switch", "model": "core:switch", "x": 2.0, "z": 2.0,
            "components": [ { "component": "interactable" },
                            { "component": "state", "name": "done", "value": 0 } ],
            "bindings": [
                { "on": "interact",
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] },
                { "on": "sequence_complete",
                  "when": [ { "check": "state", "target": "switch",
                              "name": "done", "equals": 0 } ],
                  "actions": [ { "action": "start_sequence", "sequence": "once" } ] }
            ] } ],
           "sequences": [ { "id": "once", "steps": [ { "step": "stop" } ] } ]"#,
    );
    validate_level(&conditioned).expect("a condition bounds the repetition");
}

/// Spawn templates, points and groups must resolve and carry usable values.
#[test]
fn test_validate_rejects_malformed_spawns() {
    let missing_template = binding_level(
        r#""spawn_points": [ { "id": "point", "x": 2.0, "z": 2.0, "template": "ghost" } ]"#,
    );
    let err = validate_level(&missing_template).expect_err("the template must exist");
    assert!(
        err.contains("references unknown spawn template `ghost`"),
        "unexpected error: {err}"
    );
    assert!(err.contains("`point`"), "the error names the point: {err}");

    let missing_group = binding_level(
        r#""spawn_templates": [ { "id": "rat", "model": "core:box" } ],
           "spawn_points": [ { "id": "point", "x": 2.0, "z": 2.0, "template": "rat",
                               "group": "ghost" } ]"#,
    );
    let missing_group_error = validate_level(&missing_group).expect_err("the group must exist");
    assert!(
        missing_group_error.contains("references unknown spawn group `ghost`"),
        "unexpected error: {missing_group_error}"
    );

    let blank_model = binding_level(r#""spawn_templates": [ { "id": "rat", "model": "  " } ]"#);
    let blank_model_error =
        validate_level(&blank_model).expect_err("the template model is required");
    assert!(
        blank_model_error.contains("non-empty model id"),
        "unexpected error: {blank_model_error}"
    );
    assert!(
        blank_model_error.contains("Spawn template 0"),
        "the error names the template: {blank_model_error}"
    );

    let bad_scale = binding_level(
        r#""spawn_templates": [ { "id": "rat", "model": "core:box", "scale": 0.0 } ]"#,
    );
    let bad_scale_error = validate_level(&bad_scale).expect_err("the scale must be positive");
    assert!(
        bad_scale_error.contains("scale must be a finite positive number"),
        "unexpected error: {bad_scale_error}"
    );

    let negative_lifetime = binding_level(
        r#""spawn_templates": [ { "id": "rat", "model": "core:box",
                                  "lifetime_seconds": -1.0 } ]"#,
    );
    let negative_lifetime_error =
        validate_level(&negative_lifetime).expect_err("a negative lifetime is invalid");
    assert!(
        negative_lifetime_error.contains("lifetime_seconds must be a finite positive number"),
        "unexpected error: {negative_lifetime_error}"
    );

    let template_without_point = binding_level(&format!(
        r#""spawn_templates": [ {{ "id": "rat", "model": "core:box" }} ],
           "props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "spawn_entity", "template": "rat" }"#
        )
    ));
    let template_without_point_error = validate_level(&template_without_point)
        .expect_err("a template without a point cannot resolve");
    assert!(
        template_without_point_error.contains("names a `template` without a `point`"),
        "unexpected error: {template_without_point_error}"
    );

    let unknown_spawn_group = binding_level(&format!(
        r#""spawn_templates": [ {{ "id": "rat", "model": "core:box" }} ],
           "spawn_points": [ {{ "id": "point", "x": 2.0, "z": 2.0, "template": "rat" }} ],
           "props": [{}]"#,
        interact_prop_json(
            "switch",
            r#"{ "action": "spawn_entity", "point": "point", "group": "ghost" }"#
        )
    ));
    let unknown_spawn_group_error =
        validate_level(&unknown_spawn_group).expect_err("the spawn group must exist");
    assert!(
        unknown_spawn_group_error.contains("references unknown spawn group `ghost`"),
        "unexpected error: {unknown_spawn_group_error}"
    );
}

/// Trigger volumes are checked like the area triggers they replaced: a real
/// footprint, a real vertical band, an overlapping room and a bounded count.
#[test]
fn test_validate_rejects_malformed_volumes() {
    let cases = [
        (
            r#"[{ "x": 2.0, "z": 2.0, "width": 0.0, "depth": 1.0 }]"#,
            "width and depth must be positive",
        ),
        (
            r#"[{ "x": 2.0, "z": 2.0, "width": 1.0, "depth": 1.0,
                 "bottom_y": 0.5, "top_y": 0.2 }]"#,
            "must be above its bottom_y",
        ),
        (
            r#"[{ "x": 40.0, "z": 40.0, "width": 1.0, "depth": 1.0 }]"#,
            "lies outside every room section",
        ),
    ];
    for (volumes, expected) in cases {
        let level = binding_level(&format!(r#""volumes": {volumes}"#));
        let err = validate_level(&level).expect_err("the volume must be rejected");
        assert!(err.contains(expected), "expected `{expected}` in: {err}");
        assert!(
            err.contains("Trigger volume 0"),
            "the error names the volume: {err}"
        );
    }

    // A well-formed volume with an id default and bindings validates.
    let valid = binding_level(
        r#""volumes": [ { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0,
            "bottom_y": -1.0, "top_y": 0.0,
            "bindings": [ { "on": "enter_volume",
                            "actions": [ { "action": "reset_to_start" } ] } ] } ]"#,
    );
    validate_level(&valid).expect("a well-formed volume validates");
}

/// A `toggle_label` target may name a placed prop with no interaction of its
/// own; only an unknown id is an error.
#[test]
fn test_validate_accepts_a_label_only_target() {
    let level = binding_level(
        r#""props": [
            { "id": "lamp", "display_name": "Lamp", "model": "core:lamp",
              "x": 1.0, "z": 1.0 },
            { "id": "switch", "display_name": "Switch", "model": "core:switch",
              "x": 2.0, "z": 1.0,
              "components": [ { "component": "interactable" } ],
              "bindings": [ { "on": "interact",
                              "actions": [ { "action": "toggle_label",
                                             "target": "lamp" } ] } ] }
        ]"#,
    );
    validate_level(&level).expect("a label-only target is valid");
}

/// A level with no binding content validates, keeps its deterministic prop
/// ids and authors no volumes, sequences or spawns.
#[test]
fn test_levels_load_without_bindings_or_volumes() {
    let level = level_with_props_json(
        r#"[{ "model": "core:chair", "x": 1.0, "z": 1.0 },
            { "model": "core:chair", "x": 2.0, "z": 1.0 }]"#,
    );
    validate_level(&level).expect("a prop list is still valid");
    assert!(level.volumes.is_empty(), "the level has no trigger volumes");
    assert!(level.timers.is_empty(), "the level has no timers");
    assert!(level.sequences.is_empty(), "the level has no sequences");
    assert_eq!(level.prop_instance_ids(), vec!["chair_1", "chair_2"]);
}

/// A level with a prop and a set of routes, plus a wall at x = 5..5.2 that
/// splits the room so blocked paths are expressible.
fn level_with_routes_json(props_json: &str, routes_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "routes_test",
            "name": "Routes Test",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }} ],
            "walls": [{{ "x": 5.0, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.5 }}],
            "props": {props_json},
            "routes": {routes_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

#[test]
fn test_validate_accepts_a_clear_entity_route() {
    let level = level_with_routes_json(
        r#"[{ "id": "runner", "model": "core:crate", "x": 2.0, "z": 2.0,
             "size": [0.4, 0.4, 0.4] }]"#,
        r#"[{ "id": "runner", "loop": true, "steps": [
            { "step": "move_to", "x": 4.0, "z": 2.0, "speed": 0.5 },
            { "step": "face", "yaw_degrees": 180.0 },
            { "step": "wait", "seconds": 1.0 },
            { "step": "play", "clip": "idle", "seconds": 2.0, "loop": true }
        ] }]"#,
    );
    validate_level(&level).expect("a clear route validates");
}

#[test]
fn test_validate_rejects_malformed_entity_routes() {
    let props = r#"[{ "id": "runner", "model": "core:crate", "x": 2.0, "z": 2.0,
                     "size": [0.4, 0.4, 0.4] },
                    { "id": "solid_runner", "model": "core:crate", "x": 3.0, "z": 3.0,
                      "size": [0.4, 0.4, 0.4], "solid": true }]"#;
    let cases = [
        (
            r#"[{ "id": "ghost", "steps": [
                { "step": "wait", "seconds": 1.0 } ] }]"#,
            "unknown instance",
        ),
        (r#"[{ "id": "runner", "steps": [] }]"#, "declares no steps"),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "wait", "seconds": 1.0 } ] },
                { "id": "runner", "steps": [
                { "step": "wait", "seconds": 1.0 } ] }]"#,
            "duplicates instance",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "move_to", "x": 40.0, "z": 2.0, "speed": 0.5 } ] }]"#,
            "not on",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "move_to", "x": 8.0, "z": 2.0, "speed": 0.5 } ] }]"#,
            "blocked by geometry",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "move_to", "x": 4.0, "z": 2.0, "speed": 0.0 } ] }]"#,
            "speed must be between",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "move_to", "x": 4.0, "z": 2.0, "speed": 99.0 } ] }]"#,
            "speed must be between",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "wait", "seconds": 0.0 } ] }]"#,
            "seconds must be between",
        ),
        (
            r#"[{ "id": "runner", "steps": [
                { "step": "play", "clip": "", "seconds": 1.0 } ] }]"#,
            "needs a clip name",
        ),
        (
            r#"[{ "id": "solid_runner", "steps": [
                { "step": "wait", "seconds": 1.0 } ] }]"#,
            "solid: true",
        ),
    ];
    for (routes, expected) in cases {
        let level = level_with_routes_json(props, routes);
        let err = validate_level(&level).expect_err(&format!("must reject: {routes}"));
        assert!(err.contains(expected), "expected {expected:?} in {err:?}");
    }

    // Duplicate route ids are refused with the id in the message.
    let duplicate = level_with_routes_json(
        props,
        r#"[{ "id": "runner", "steps": [{ "step": "wait", "seconds": 1.0 }] },
             { "id": "runner", "steps": [{ "step": "wait", "seconds": 1.0 }] }]"#,
    );
    let err = validate_level(&duplicate).expect_err("duplicate routes are refused");
    assert!(err.contains("duplicates instance `runner`"), "{err}");
}

/// The runtime's minimum movement-disc radius is the validator's minimum too,
/// so a tiny prop can never pass validation and then stall on a wall the map
/// did not clear.
#[test]
fn validate_routes_uses_the_runtime_minimum_disc_radius() {
    let level = level_with_routes_json(
        r#"[{ "id": "tiny", "model": "core:crate", "x": 2.0, "z": 2.0,
             "size": [0.02, 0.02, 0.02] }]"#,
        r#"[{ "id": "tiny", "steps": [
            { "step": "move_to", "x": 2.5, "z": 2.0, "speed": 0.5 } ] }]"#,
    );
    validate_level(&level).expect("a tiny prop's route validates");
    let routes = crate::entity::EntityRoutes::from_level(&level);
    let route = routes.get("tiny").expect("the route resolves");
    assert!(
        (route.radius - crate::entity::ENTITY_MIN_RADIUS_M).abs() < 1.0e-6,
        "the runtime disc uses the shared minimum: {}",
        route.radius
    );
}

/// The demo duck, exactly as the shipped demo authors it: the pool
/// surface is at -1.65 m and the duck floats inside the basin's
/// 8..20 x 10..16 footprint.
const DEMO_DUCK_FLOAT: &str = r#"{ "model": "core:rubber_duck", "x": 10.5, "z": 10.4,
    "rotation_degrees": 180.0, "size": [0.10, 0.12, 0.14], "solid": false,
    "float": { "draft": 0.03, "bob": 0.012, "bob_seconds": 2.4,
               "heel_degrees": 3.0, "heel_seconds": 3.1 } }"#;

/// A 24x24 m basin room carrying the demo pool's water (8..20 x 10..16 at
/// -1.65 m, floor -3.0) and one authored prop, plus optional routes.
fn float_level_with_routes(prop_json: &str, routes_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "float_test",
            "name": "Float Test",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 24.0,
                         "height": 3.5, "floor_y": -3.0 }}],
            "water": [{{ "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0,
                         "surface_y": -1.65, "bottom_y": -3.0 }}],
            "props": [{prop_json}],
            "routes": {routes_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("the float test level parses")
}

/// [`float_level_with_routes`] with no routes.
fn float_level(prop_json: &str) -> LevelDef {
    float_level_with_routes(prop_json, "[]")
}

/// The float block of the level's only prop, for case construction.
fn float_mut(level: &mut LevelDef) -> &mut crate::level::PropFloatDef {
    level.props[0]
        .float
        .as_mut()
        .expect("the test level's only prop floats")
}

/// The demo duck validates, the swept disc really is inside the basin, and the
/// contract's inclusive boundaries (bob at half the height, heel at the cap,
/// phase at both ends) are accepted.
#[test]
fn test_validate_accepts_the_demo_duck_and_the_contract_boundaries() {
    let level = float_level(DEMO_DUCK_FLOAT);
    validate_level(&level).expect("the demo duck validates");

    // The duck sits 0.4 m from the north rim with a ~0.089 m swept radius.
    let water = crate::level::WaterVolumes::from_level(&level);
    assert!(water.contains_disc(10.5, 10.4, 0.09));

    // A minimal block only has to name its draft: zero bob and zero heel are
    // the default calm float.
    let minimal = float_level(
        r#"{ "model": "core:rubber_duck", "x": 10.5, "z": 10.4,
             "size": [0.10, 0.12, 0.14], "solid": false,
             "float": { "draft": 0.03 } }"#,
    );
    validate_level(&minimal).expect("a minimal float block validates");

    // The inclusive boundaries are valid: bob == 0.5 * height and
    // heel == MAX_FLOAT_HEEL_DEGREES are "at most", phase 0.0 and 1.0 are
    // inside the closed range.
    let mut boundary = level;
    {
        let float = float_mut(&mut boundary);
        float.bob = 0.06;
        float.heel_degrees = crate::level::MAX_FLOAT_HEEL_DEGREES;
        float.phase = Some(1.0);
    }
    validate_level(&boundary).expect("the inclusive float boundaries are valid");
    let mut zero_phase = float_level(DEMO_DUCK_FLOAT);
    float_mut(&mut zero_phase).phase = Some(0.0);
    validate_level(&zero_phase).expect("phase 0 is valid");
}

/// Every broken clause of the float contract is a named validation error.
#[test]
fn test_validate_rejects_malformed_float_props() {
    let base = float_level(DEMO_DUCK_FLOAT);
    validate_level(&base).expect("the unmodified demo duck validates");

    let mut solid = base.clone();
    solid.props[0].solid = true;
    let mut size_missing = base.clone();
    size_missing.props[0].size = None;
    let mut draft_zero = base.clone();
    float_mut(&mut draft_zero).draft = 0.0;
    let mut draft_negative = base.clone();
    float_mut(&mut draft_negative).draft = -0.03;
    let mut draft_at_height = base.clone();
    float_mut(&mut draft_at_height).draft = 0.12;
    let mut bob_over = base.clone();
    float_mut(&mut bob_over).bob = 0.060_001;
    let mut heel_over = base.clone();
    float_mut(&mut heel_over).heel_degrees = crate::level::MAX_FLOAT_HEEL_DEGREES + 1.0;
    let mut bob_period_zero = base.clone();
    float_mut(&mut bob_period_zero).bob_seconds = 0.0;
    let mut heel_period_negative = base.clone();
    float_mut(&mut heel_period_negative).heel_seconds = -2.4;
    let mut phase_over = base.clone();
    float_mut(&mut phase_over).phase = Some(1.5);
    let mut phase_under = base.clone();
    float_mut(&mut phase_under).phase = Some(-0.25);

    let cases = [
        ("solid float", solid, "solid: false"),
        ("missing size", size_missing, "must author `size`"),
        ("draft == 0", draft_zero, "draft must be"),
        ("draft < 0", draft_negative, "draft must be"),
        ("draft == height", draft_at_height, "draft must be"),
        ("bob > half height", bob_over, "bob must be"),
        ("heel > cap", heel_over, "heel_degrees must be"),
        ("bob_seconds == 0", bob_period_zero, "bob_seconds must be"),
        (
            "heel_seconds < 0",
            heel_period_negative,
            "heel_seconds must be",
        ),
        ("phase > 1", phase_over, "phase must be"),
        ("phase < 0", phase_under, "phase must be"),
    ];
    for (label, level, expected) in cases {
        let err = validate_level(&level).expect_err(label);
        assert!(
            err.contains(expected),
            "{label}: expected {expected:?} in {err:?}"
        );
    }

    // A float whose swept footprint pokes through the rim is refused: at
    // x = 8.02 the ~0.089 m swept radius leaves the basin.
    let mut at_rim = base.clone();
    at_rim.props[0].x = 8.02;
    let err = validate_level(&at_rim).expect_err("a disc off the rim is refused");
    assert!(
        err.contains("must be fully inside a water volume"),
        "unexpected rim error: {err}"
    );
}

/// A floating prop cannot also be addressed by an entity route.
#[test]
fn test_validate_rejects_a_route_on_a_floating_prop() {
    let level = float_level_with_routes(
        DEMO_DUCK_FLOAT,
        r#"[{ "id": "rubber_duck_1", "steps": [{ "step": "wait", "seconds": 1.0 }] }]"#,
    );
    let err = validate_level(&level).expect_err("a float cannot be routed");
    assert!(
        err.contains("is addressed by a route"),
        "unexpected route error: {err}"
    );
}

/// The shipped demo stays valid with its duck, and the duck is the non-solid,
/// sized float the contract requires.
#[test]
fn test_the_shipped_demo_duck_validates() {
    let level = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("the Places demo parses");
    validate_level(&level).expect("the shipped demo validates with its duck");
    let duck = level
        .props
        .iter()
        .find(|prop| prop.model == "core:rubber_duck")
        .expect("the shipped demo places the floating duck");
    assert!(duck.float.is_some(), "the demo duck authors a float block");
    assert!(!duck.solid, "a float cannot be solid");
    assert!(duck.size.is_some(), "a float must author its size");
}

// ---------------------------------------------------------------------------
// round-architecture validation diagnostics
// ---------------------------------------------------------------------------

/// Parses a level JSON with a `base` room and the given extra geometry blocks,
/// then validates it.
fn validate_with(extra: &str) -> Result<(), String> {
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "round_validation",
            "name": "Round Validation",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 }} ],
            {extra}
        }}"#
    );
    let level = LevelDef::from_json(&json).expect("the round validation document parses");
    validate_level(&level)
}

#[test]
fn test_round_dimension_diagnostics_name_the_constraint() {
    let cases: [(&str, &str); 9] = [
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 0.0 }]"#,
            "radius must be positive",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 0.5, "thickness": 1.0 }]"#,
            "thinner than twice its radius",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 1.0, "sweep_degrees": 0.0 }]"#,
            "sweep must be a non-zero angle",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 1.0, "sweep_degrees": 400.0 }]"#,
            "sweep must be a non-zero angle",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 1.0, "segments": 2 }]"#,
            "segments must be between",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 1.0, "height": 0.0 }]"#,
            "height must be a positive finite number",
        ),
        (
            r#""arc_walls": [{ "x": 5.0, "z": 5.0, "radius": 1.0, "material": "  " }]"#,
            "materials must be non-empty",
        ),
        (
            r#""pillars": [{ "x": 5.0, "z": 5.0, "radius": -1.0 }]"#,
            "radius must be positive",
        ),
        (
            r#""pillars": [{ "x": 5.0, "z": 5.0, "radius": 0.5, "segments": 200 }]"#,
            "segments must be between",
        ),
    ];
    for (extra, expected) in cases {
        let error = validate_with(extra).expect_err(&format!("{extra} must be rejected"));
        assert!(
            error.contains(expected),
            "unexpected diagnostic for {extra}: {error}"
        );
    }
}

#[test]
fn test_valid_round_primitives_pass_validation() {
    validate_with(
        r#""arc_walls": [
            { "x": 5.0, "z": 5.0, "radius": 2.0, "thickness": 0.3,
              "start_degrees": 45.0, "sweep_degrees": 360.0, "segments": 32,
              "material": "core:wallpaper_stained_01",
              "inner_material": "core:pool_tile_wall_01",
              "outer_material": "core:wallpaper_yellow_01",
              "cap_material": "core:baseboard_office_01",
              "end_material": "core:baseboard_office_01" }
        ],
        "pillars": [
            { "x": 2.0, "z": 2.0, "radius": 0.3, "height": 2.4, "segments": 12 },
            { "x": 7.0, "z": 7.0, "radius": 0.25 }
        ]"#,
    )
    .expect("well-formed curves validate");
}

#[test]
fn test_ceiling_tile_frame_validation() {
    let bad_origin = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "bad_frame",
            "name": "Bad Frame",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0,
                      "ceiling_tile_origin": [null, 0.0] } ]
        }"#,
    );
    assert!(bad_origin.is_err(), "a null origin is a parse error");
    let mut level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "frame",
            "name": "Frame",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ]
        }"#,
    )
    .expect("the frame document parses");
    level.rooms[0].ceiling_tile_origin = Some([f32::NAN, 0.0]);
    let error = validate_level(&level).expect_err("a non-finite origin is rejected");
    assert!(error.contains("ceiling tile origin"), "{error}");
    level.rooms[0].ceiling_tile_origin = Some([0.0, 0.0]);
    level.rooms[0].ceiling_tile_rotation_degrees = Some(f32::INFINITY);
    let level_error = validate_level(&level).expect_err("a non-finite rotation is rejected");
    assert!(
        level_error.contains("ceiling tile rotation"),
        "{level_error}"
    );
}

/// Every checked-in level parses and validates against the one final schema.
///
/// This is the repository-content gate: a map that still used a removed field
/// or action would fail here, so no level can be left on an obsolete
/// development format. The deliberately invalid checker fixture under
/// `tests/fixtures/levels/invalid/` is not part of the playable set and is
/// exercised by the geometry checker instead.
#[test]
fn every_checked_in_level_parses_and_validates_against_the_final_schema() {
    let mut checked = 0_usize;
    for directory in ["assets/levels", "levels", "tests/fixtures/levels"] {
        let entries =
            std::fs::read_dir(directory).unwrap_or_else(|e| panic!("read {directory}: {e}"));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("json") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let level =
                LevelDef::from_json(&content).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                level.format_version,
                crate::level::LEVEL_FORMAT_VERSION,
                "{} must use the current format",
                path.display()
            );
            validate_level(&level).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            checked = checked.saturating_add(1);
        }
    }
    assert!(
        checked >= 20,
        "expected every checked-in level, found {checked}"
    );
}

/// An edit that only changes an AI behavior reuses the previous package's
/// prepared world; a geometry edit does not.
#[test]
fn an_ai_only_edit_reuses_the_prepared_lighting() {
    let dir = std::env::temp_dir().join(format!("places-nav-reuse-{}", std::process::id()));
    crate::test_support::remove_dir_if_present(&dir);
    fs::create_dir_all(&dir).expect("create the reuse scratch directory");
    let source = dir.join("reuse_level.json");
    let level_json = |ai_speed: f32, room_width: f32| {
        format!(
            r#"{{
                "format_version": 3, "id": "nav_reuse", "name": "Reuse",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": {room_width}, "depth": 6.0, "height": 3.0 }} ],
                "props": [ {{ "id": "actor", "model": "rat", "x": 2.0, "z": 2.0,
                              "components": [
                                {{ "component": "nav_agent", "radius": 0.1, "height": 0.16,
                                   "speed_mps": 0.2, "step_height": 0.2, "max_slope": 2.6667 }},
                                {{ "component": "ai", "behavior": "idler", "walk_speed": {ai_speed} }}
                              ] }} ]
            }}"#
        )
    };
    let out = source.with_extension("placesmap");
    let request = |force: bool| crate::compiler::BuildRequest {
        source: source.clone(),
        out: out.clone(),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Off],
        workers: 1,
        force,
        capture_probes: false,
    };

    fs::write(&source, level_json(1.0, 8.0)).expect("write the base level");
    let first = crate::compiler::build(&request(true)).expect("the base package builds");
    assert!(first.rebuilt);
    let first_manifest = crate::compiler::inspect(&out).expect("inspect the base package");
    let first_variant = first_manifest
        .variant("off")
        .expect("the base variant exists")
        .clone();

    // AI-only edit: the stage fingerprint is unchanged, so the prepared
    // lighting/geometry blob names are reused.
    fs::write(&source, level_json(1.7, 8.0)).expect("write the AI-edited level");
    let second = crate::compiler::build(&request(false)).expect("the AI edit rebuilds");
    assert!(
        second.rebuilt,
        "the package is rewritten with new semantics"
    );
    assert!(
        second
            .warnings
            .iter()
            .any(|warning| warning.contains("reused prepared geometry")),
        "the AI edit reports lighting reuse: {:?}",
        second.warnings
    );
    let second_manifest = crate::compiler::inspect(&out).expect("inspect the edited package");
    let second_variant = second_manifest
        .variant("off")
        .expect("the edited variant exists");
    assert_eq!(
        first_variant.entries.lighting, second_variant.entries.lighting,
        "the baked lighting record is reused byte-for-byte"
    );
    assert_eq!(
        first_variant.entries.mesh, second_variant.entries.mesh,
        "the prepared geometry is reused"
    );
    assert_eq!(
        first_variant.entries.navigation, second_variant.entries.navigation,
        "an AI speed edit does not change the navigation record"
    );
    assert_ne!(
        first_manifest.compiler_fingerprint, second_manifest.compiler_fingerprint,
        "the package identity still tracks the source"
    );
    assert_eq!(
        first_manifest.lighting_fingerprint, second_manifest.lighting_fingerprint,
        "the stage fingerprint is unchanged by an AI-only edit"
    );

    // Geometry edit: the stage fingerprint changes and nothing is reused.
    fs::write(&source, level_json(1.7, 9.0)).expect("write the geometry-edited level");
    let third = crate::compiler::build(&request(false)).expect("the geometry edit rebuilds");
    assert!(
        !third
            .warnings
            .iter()
            .any(|warning| warning.contains("reused prepared geometry")),
        "a geometry edit prepares everything again: {:?}",
        third.warnings
    );
    let third_manifest = crate::compiler::inspect(&out).expect("inspect the rebuilt package");
    let third_variant = third_manifest
        .variant("off")
        .expect("the rebuilt variant exists");
    assert_ne!(
        first_manifest.lighting_fingerprint, third_manifest.lighting_fingerprint,
        "the stage fingerprint tracks the geometry"
    );
    assert_ne!(
        second_variant.entries.lighting, third_variant.entries.lighting,
        "a geometry edit produces a new lighting record"
    );
    crate::test_support::remove_dir_if_present(&dir);
}

/// The emitted-geometry revision is part of both build fingerprints, so an
/// emitter change that leaves the source bytes untouched still invalidates the
/// package: the stale mesh and the lightmaps baked against it are rebuilt
/// instead of being silently reused.
#[test]
fn the_geometry_revision_is_part_of_both_build_fingerprints() {
    let level = LevelDef::from_json(
        r#"{ "format_version": 3, "id": "revision", "name": "Revision",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }] }"#,
    )
    .expect("valid revision level");
    let variants = [crate::quality::LightmapQuality::Off];
    let dependencies: Vec<crate::package::manifest::PackageDependency> = Vec::new();
    let revision = crate::render::GEOMETRY_REVISION;

    let compiler_now =
        crate::compiler::fingerprint_with_revision("00", &variants, &dependencies, revision);
    let compiler_next =
        crate::compiler::fingerprint_with_revision("00", &variants, &dependencies, revision + 1);
    assert_ne!(
        compiler_now, compiler_next,
        "a geometry revision bump must change the package fingerprint"
    );

    let lighting_now = crate::compiler::lighting_fingerprint_with_revision(
        &level,
        &variants,
        &dependencies,
        revision,
    )
    .expect("the lighting stage input serialises");
    let lighting_next = crate::compiler::lighting_fingerprint_with_revision(
        &level,
        &variants,
        &dependencies,
        revision + 1,
    )
    .expect("the lighting stage input serialises");
    assert_ne!(
        lighting_now, lighting_next,
        "a geometry revision bump must change the lighting stage fingerprint"
    );

    // The same revision and inputs stay byte-stable, so nothing about the
    // bump weakens the reuse tests above.
    assert_eq!(
        compiler_now,
        crate::compiler::fingerprint_with_revision("00", &variants, &dependencies, revision),
        "the package fingerprint is a pure function of its inputs"
    );
    assert_eq!(
        lighting_now,
        crate::compiler::lighting_fingerprint_with_revision(
            &level,
            &variants,
            &dependencies,
            revision
        )
        .expect("the lighting stage input serialises"),
        "the stage fingerprint is a pure function of its inputs"
    );

    // The shipped revision is the one the fix introduced, not a placeholder.
    assert_eq!(
        revision, 8,
        "shared static-model chart UVs and physical density require geometry revision 8"
    );
}

// ---------------------------------------------------------------------------
// Regional fog and void wall validation
// ---------------------------------------------------------------------------

/// A minimal valid level with the given `fog_regions` array body.
fn fog_level(regions_json: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "fog_validation",
            "name": "Fog Validation",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 }} ],
            "fog_regions": {regions_json}
        }}"#
    ))
    .expect("fog validation level parses")
}

#[test]
fn fog_regions_at_the_cap_and_at_every_bound_are_accepted() {
    let full = fog_level(&format!(
        "[{}]",
        (0..crate::level::MAX_FOG_REGIONS)
            .map(|index| format!(
                r#"{{ "id": "r{index}", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                     "density": 0.5, "color": [1.0, 1.0, 1.0], "falloff_m": 0.0,
                     "ground_y": 0.0, "top_y": 1.0 }}"#
            ))
            .collect::<Vec<_>>()
            .join(",")
    ));
    assert!(
        validate_level(&full).is_ok(),
        "exactly 16 regions are legal"
    );
}

#[test]
fn fog_regions_beyond_every_bound_are_named_errors() {
    let cases: [(&str, &str, &str); 10] = [
        (
            "one over the cap",
            &format!("[{}]", vec![
                r#"{ "id": "r", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 }"#;
                crate::level::MAX_FOG_REGIONS + 1
            ].join(",")),
            "too many fog regions",
        ),
        (
            "empty id",
            r#"[{ "id": "  ", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 }]"#,
            "names no id",
        ),
        (
            "duplicate id",
            r#"[{ "id": "dup", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 },
                { "id": "dup", "min": [2.0, 0.0, 0.0], "max": [3.0, 1.0, 1.0], "density": 0.1 }]"#,
            "repeats an id",
        ),
        (
            "inverted bounds",
            r#"[{ "id": "inverted", "min": [1.0, 0.0, 0.0], "max": [0.0, 1.0, 1.0], "density": 0.1 }]"#,
            "min below max on every axis",
        ),
        (
            "non-finite bounds",
            r#"[{ "id": "nan", "min": [0.0, 0.0, 0.0], "max": [1.0, 1e40, 1.0], "density": 0.1 }]"#,
            "bounds must be finite numbers",
        ),
        (
            "density over the limit",
            r#"[{ "id": "yard_mist", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.9 }]"#,
            "has density 0.9 (limit 0.5)",
        ),
        (
            "negative density",
            r#"[{ "id": "negative", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": -0.1 }]"#,
            "has density",
        ),
        (
            "colour out of range",
            r#"[{ "id": "tinted", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1,
                 "color": [1.2, 0.0, 0.0] }]"#,
            "colour components must be between 0.0 and 1.0",
        ),
        (
            "negative falloff",
            r#"[{ "id": "hard", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1,
                 "falloff_m": -1.0 }]"#,
            "falloff_m",
        ),
        (
            "non-finite layer",
            r#"[{ "id": "layer", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1,
                 "top_y": 1e40 }]"#,
            "ground_y and top_y must be finite",
        ),
    ];
    for (case, regions, expected) in cases {
        let error = validate_level(&fog_level(regions)).expect_err(case);
        assert!(
            error.contains(expected),
            "expected `{expected}` in: {error}"
        );
    }
}

/// The named density error carries the authored position, exactly as the
/// authoring guide documents it.
#[test]
fn a_fog_region_error_names_the_index_and_id() {
    let level = fog_level(
        r#"[{ "id": "a", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 },
            { "id": "b", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 },
            { "id": "c", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "density": 0.1 },
            { "id": "yard_mist", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
              "density": 0.9 }]"#,
    );
    let error = validate_level(&level).expect_err("density over the limit");
    assert_eq!(
        error,
        "fog region 3 ('yard_mist') has density 0.9 (limit 0.5)"
    );
}

/// A minimal valid level with the given `void_walls` array body.
fn void_wall_level(walls_json: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "void_validation",
            "name": "Void Validation",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 }} ],
            "void_walls": {walls_json}
        }}"#
    ))
    .expect("void validation level parses")
}

#[test]
fn void_walls_at_the_cap_are_accepted_and_one_over_is_rejected() {
    let entry = r#"{ "id": "wall", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                     "material": "core:wallpaper_yellow_01" }"#;
    let full = void_wall_level(&format!(
        "[{}]",
        (0..crate::level::MAX_VOID_WALLS)
            .map(|index| entry.replace("\"wall\"", &format!("\"wall_{index}\"")))
            .collect::<Vec<_>>()
            .join(",")
    ));
    assert!(
        validate_level(&full).is_ok(),
        "exactly 256 void walls are legal"
    );
    let over = void_wall_level(&format!(
        "[{}]",
        (0..=crate::level::MAX_VOID_WALLS)
            .map(|index| entry.replace("\"wall\"", &format!("\"wall_{index}\"")))
            .collect::<Vec<_>>()
            .join(",")
    ));
    let error = validate_level(&over).expect_err("one over the cap");
    assert!(error.contains("too many void walls"), "{error}");
}

#[test]
fn void_walls_reject_named_bounds_ids_and_materials() {
    let cases: [(&str, &str, &str); 8] = [
        (
            "inverted bounds",
            r#"[{ "min": [1.0, 0.0, 0.0], "max": [0.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01" }]"#,
            "min below max on every axis",
        ),
        (
            "non-finite bounds",
            r#"[{ "min": [0.0, 0.0, 0.0], "max": [1.0, 1e40, 1.0],
                 "material": "core:wallpaper_yellow_01" }]"#,
            "bounds must be finite numbers",
        ),
        (
            "empty material",
            r#"[{ "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "material": "  " }]"#,
            "names no material",
        ),
        (
            "malformed material id",
            r#"[{ "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0], "material": "not a material" }]"#,
            "not a well-formed logical id",
        ),
        (
            "empty id",
            r#"[{ "id": "", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01" }]"#,
            "has an empty id",
        ),
        (
            "duplicate id",
            r#"[{ "id": "shell", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01" },
                { "id": "shell", "min": [2.0, 0.0, 0.0], "max": [3.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01" }]"#,
            "repeats an id",
        ),
        (
            "id too long",
            r#"[{ "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                 "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01" }]"#,
            "longer than",
        ),
        (
            "unknown face mode",
            r#"[{ "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0],
                 "material": "core:wallpaper_yellow_01", "faces": "sideways" }]"#,
            "unknown variant",
        ),
    ];
    for (case, walls, expected) in cases {
        let error = match LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "void_validation",
                "name": "Void Validation",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 }} ],
                "void_walls": {walls}
            }}"#
        )) {
            Ok(level) => validate_level(&level).expect_err(case),
            Err(parse) => parse.to_string(),
        };
        assert!(
            error.contains(expected),
            "expected `{expected}` in: {error}"
        );
    }
}

/// A fog/void level survives the authoring → `semantics.json` → reader path:
/// serialize the prepared `LevelDef`, parse it back and validate it, and the
/// new records are bit-identical.
#[test]
fn fog_regions_and_void_walls_round_trip_through_semantics_json() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "round_trip",
            "name": "Round Trip",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 } ],
            "fog_regions": [
                { "id": "mist", "min": [0.0, -1.0, 0.0], "max": [4.0, 1.0, 4.0],
                  "density": 0.05, "color": [0.2, 0.3, 0.4], "falloff_m": 1.5,
                  "ground_y": -0.5, "top_y": 0.5 }
            ],
            "void_walls": [
                { "id": "shell", "min": [-1.0, 0.0, -1.0], "max": [1.0, 3.0, 1.0],
                  "material": "outdoor:dirt_gravel_01", "faces": "both",
                  "solid": false, "occludes": true }
            ]
        }"#,
    )
    .expect("round-trip level parses");
    assert!(validate_level(&level).is_ok());
    let encoded = serde_json::to_string(&level).expect("prepared semantics serialize");
    let decoded = LevelDef::from_json(&encoded).expect("prepared semantics parse");
    assert!(validate_level(&decoded).is_ok());
    assert_eq!(decoded.fog_regions, level.fog_regions);
    assert_eq!(decoded.void_walls, level.void_walls);
    assert_eq!(
        serde_json::to_string(&decoded).expect("decoded semantics serialize"),
        encoded,
        "serialization is stable across a semantics round trip"
    );
}

// ------------------------------------------------- 2026 raised element caps
//
// The capacity pass raised every per-element count cap. Each raised cap is
// pinned twice here: a level at the limit validates, and one past it is
// rejected by name. The table shares the entry makers so the two halves can
// never test different content.

/// One raised element cap under test.
struct RaisedCapCase {
    /// The level field the entries live in.
    field: &'static str,
    /// Further top-level fields the case needs (spawn templates, say).
    extra: &'static str,
    /// Rooms the case authors; the geometry estimate rejects a level that
    /// stacks every piece in one room before the cap under test can name
    /// itself, so a case places one piece per room where the estimate needs it.
    rooms: usize,
    /// One entry, by authored index.
    make: fn(usize) -> String,
    /// The cap's value.
    limit: usize,
    /// Fragment the one-over rejection must contain.
    fragment: &'static str,
}

/// The room lattice a case's entries are placed on: 4 m × 4 m rooms on a 6 m
/// pitch, row-major, 90 per row.
fn cap_room_position(index: usize) -> (f32, f32) {
    (
        crate::test_support::exact_f32(index % 90) * 6.0,
        crate::test_support::exact_f32(index / 90) * 6.0,
    )
}

/// `count` rooms on the same lattice as [`cap_room_position`].
fn cap_rooms(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| {
            let (x, z) = cap_room_position(index);
            format!(r#"{{ "x": {x}, "z": {z}, "width": 4.0, "depth": 4.0, "height": 3.5 }}"#)
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// A level with `rooms` and the case's collection.
fn raised_cap_level(case: &RaisedCapCase, count: usize) -> LevelDef {
    let entries: Vec<String> = (0..count).map(case.make).collect();
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 3,
            "id": "raised_cap",
            "name": "Raised Cap",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": {}{},
            "{}": [{}]
        }}"#,
        cap_rooms(case.rooms),
        case.extra,
        case.field,
        entries.join(",")
    ))
    .unwrap_or_else(|error| panic!("the {} cap level parses: {error}", case.field))
}

/// Every raised element cap, its current constant and its one-entry JSON.
fn raised_cap_cases() -> Vec<RaisedCapCase> {
    vec![
        RaisedCapCase {
            field: "walls",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 1.0, "z": 1.0, "width": 0.4, "depth": 0.4, "height": 2.0 }"#.to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_WALLS)
                .expect("fixture integer fits usize"),
            fragment: "too many walls",
        },
        RaisedCapCase {
            field: "ceiling_lights",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }"#.to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_CEILING_LIGHTS)
                .expect("fixture integer fits usize"),
            fragment: "too many ceiling lights",
        },
        RaisedCapCase {
            field: "props",
            extra: "",
            rooms: 1,
            make: |_| r#"{ "model": "core:crate", "x": 2.0, "z": 2.0 }"#.to_string(),
            limit: usize::try_from(crate::level::MAX_LEVEL_PROPS)
                .expect("fixture integer fits usize"),
            fragment: "too many props",
        },
        RaisedCapCase {
            field: "decals",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "y": 0.0, "z": 2.0, "width": 0.5, "height": 0.5,
                     "material": "core:decal_test_01", "surface": "floor" }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_DECALS)
                .expect("fixture integer fits usize"),
            fragment: "too many decals",
        },
        RaisedCapCase {
            field: "floor_regions",
            extra: "",
            rooms: usize::try_from(crate::level::MAX_LEVEL_FLOOR_REGIONS)
                .expect("fixture integer fits usize"),
            make: |index| {
                let (x, z) = cap_room_position(index);
                format!(
                    r#"{{ "x": {}, "z": {}, "width": 1.0, "depth": 1.0, "offset_y": 0.1 }}"#,
                    x + 1.0,
                    z + 1.0
                )
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_FLOOR_REGIONS)
                .expect("fixture integer fits usize"),
            fragment: "too many floor regions",
        },
        RaisedCapCase {
            field: "floor_patches",
            extra: "",
            rooms: usize::try_from(crate::level::MAX_LEVEL_FLOOR_PATCHES)
                .expect("fixture integer fits usize"),
            make: |index| {
                let (x, z) = cap_room_position(index);
                format!(
                    r#"{{ "x": {}, "z": {}, "width": 1.0, "depth": 1.0,
                         "material": "core:carpet_beige_01" }}"#,
                    x + 1.0,
                    z + 1.0
                )
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_FLOOR_PATCHES)
                .expect("fixture integer fits usize"),
            fragment: "too many floor patches",
        },
        RaisedCapCase {
            field: "water",
            extra: "",
            rooms: usize::try_from(crate::level::MAX_LEVEL_WATER_VOLUMES)
                .expect("fixture integer fits usize"),
            make: |index| {
                let (x, z) = cap_room_position(index);
                format!(
                    r#"{{ "x": {}, "z": {}, "width": 1.0, "depth": 1.0, "surface_y": 0.2 }}"#,
                    x + 1.0,
                    z + 1.0
                )
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_WATER_VOLUMES)
                .expect("fixture integer fits usize"),
            fragment: "too many water volumes",
        },
        RaisedCapCase {
            field: "ladders",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 1.0, "z": 1.0, "width": 0.6, "depth": 0.2,
                     "bottom_y": 0.0, "top_y": 2.0, "facing_degrees": 0.0 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_LADDERS)
                .expect("fixture integer fits usize"),
            fragment: "too many ladders",
        },
        RaisedCapCase {
            field: "ramps",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 2.0,
                     "offset_y": 0.0, "rise": 0.25 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_RAMPS)
                .expect("fixture integer fits usize"),
            fragment: "too many ramps",
        },
        RaisedCapCase {
            field: "stairs",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.2,
                     "offset_y": 0.0, "rise": 0.4, "steps": 3 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_STAIRS)
                .expect("fixture integer fits usize"),
            fragment: "too many staircases",
        },
        RaisedCapCase {
            field: "half_walls",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 0.2, "height": 1.0 }"#.to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_HALF_WALLS)
                .expect("fixture integer fits usize"),
            fragment: "too many half walls",
        },
        RaisedCapCase {
            field: "columns",
            extra: "",
            rooms: 1,
            make: |_| r#"{ "x": 1.0, "z": 1.0, "width": 0.3, "depth": 0.3 }"#.to_string(),
            limit: usize::try_from(crate::level::MAX_LEVEL_COLUMNS)
                .expect("fixture integer fits usize"),
            fragment: "too many columns",
        },
        RaisedCapCase {
            field: "arc_walls",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "radius": 1.2, "thickness": 0.2,
                     "height": 2.4, "start_degrees": 0.0, "sweep_degrees": 90.0,
                     "segments": 8 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_ARC_WALLS)
                .expect("fixture integer fits usize"),
            fragment: "too many arc walls",
        },
        RaisedCapCase {
            field: "pillars",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "radius": 0.2, "height": 3.0, "segments": 8 }"#.to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_PILLARS)
                .expect("fixture integer fits usize"),
            fragment: "too many pillars",
        },
        RaisedCapCase {
            field: "archways",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "width": 2.4, "depth": 0.4, "height": 2.5,
                     "opening_width": 1.0, "opening_height": 2.0, "arch_rise": 0.2 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_ARCHWAYS)
                .expect("fixture integer fits usize"),
            fragment: "too many archways",
        },
        RaisedCapCase {
            field: "guardrails",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "length": 2.0, "rotation_degrees": 0.0,
                     "height": 0.9 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_GUARDRAILS)
                .expect("fixture integer fits usize"),
            fragment: "too many guardrails",
        },
        RaisedCapCase {
            field: "thresholds",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "length": 1.0, "thickness": 0.08,
                     "height": 0.02, "rotation_degrees": 90.0 }"#
                    .to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_THRESHOLDS)
                .expect("fixture integer fits usize"),
            fragment: "too many thresholds",
        },
        RaisedCapCase {
            field: "baseboards",
            extra: "",
            rooms: 1,
            make: |_| {
                r#"{ "x": 2.0, "z": 2.0, "length": 2.0, "rotation_degrees": 90.0 }"#.to_string()
            },
            limit: usize::try_from(crate::level::MAX_LEVEL_BASEBOARDS)
                .expect("fixture integer fits usize"),
            fragment: "too many baseboards",
        },
        RaisedCapCase {
            field: "volumes",
            extra: "",
            rooms: 1,
            make: |_| r#"{ "x": 2.0, "z": 2.0, "width": 1.0, "depth": 1.0 }"#.to_string(),
            limit: usize::try_from(crate::level::MAX_LEVEL_AREA_TRIGGERS)
                .expect("fixture integer fits usize"),
            fragment: "too many trigger volumes",
        },
        RaisedCapCase {
            field: "sequences",
            extra: "",
            rooms: 1,
            make: |index| {
                format!(
                    r#"{{ "id": "cap_sequence_{index}", "steps": [ {{ "step": "wait", "seconds": 0.1 }} ] }}"#
                )
            },
            limit: crate::entities::sequences::MAX_LEVEL_SEQUENCES,
            fragment: "too many sequences",
        },
        RaisedCapCase {
            field: "spawn_templates",
            extra: "",
            rooms: 1,
            make: |index| {
                format!(
                    r#"{{ "id": "cap_template_{index}", "model": "core:crate", "scale": 1.0 }}"#
                )
            },
            limit: crate::entities::spawn::MAX_LEVEL_SPAWN_TEMPLATES,
            fragment: "too many spawn templates",
        },
        RaisedCapCase {
            field: "spawn_points",
            extra: r#", "spawn_templates": [ { "id": "cap_template", "model": "core:crate" } ]"#,
            rooms: 1,
            make: |index| {
                format!(
                    r#"{{ "id": "cap_point_{index}", "x": 2.0, "z": 2.0, "template": "cap_template" }}"#
                )
            },
            limit: crate::entities::spawn::MAX_LEVEL_SPAWN_POINTS,
            fragment: "too many spawn points",
        },
        RaisedCapCase {
            field: "spawn_groups",
            extra: "",
            rooms: 1,
            make: |index| format!(r#"{{ "id": "cap_group_{index}" }}"#),
            limit: crate::entities::spawn::MAX_LEVEL_SPAWN_GROUPS,
            fragment: "too many spawn groups",
        },
    ]
}

#[test]
fn test_validate_accepts_every_raised_element_cap_at_its_limit() {
    for case in raised_cap_cases() {
        let level = raised_cap_level(&case, case.limit);
        validate_level(&level).unwrap_or_else(|error| {
            panic!("a level at the {} cap must validate: {error}", case.field)
        });
    }
}

#[test]
fn test_validate_rejects_one_over_every_raised_element_cap() {
    for case in raised_cap_cases() {
        let level = raised_cap_level(&case, case.limit.saturating_add(1));
        let error =
            validate_level(&level).expect_err("a level one past a raised cap must be rejected");
        assert!(
            error.contains(case.fragment),
            "the {} over-cap error must contain `{}`: {error}",
            case.field,
            case.fragment
        );
    }
}

#[test]
fn test_validate_rejects_too_many_rooms() {
    let rooms = usize::try_from(crate::level::MAX_LEVEL_ROOMS).expect("fixture integer fits usize");
    let build = |count: usize| {
        LevelDef::from_json(&format!(
            r#"{{ "format_version": 3, "id": "rooms_cap", "name": "Rooms Cap",
                 "spawn": {{ "x": 1.0, "z": 1.0 }}, "rooms": {} }}"#,
            cap_rooms(count)
        ))
        .expect("the rooms cap level parses")
    };
    validate_level(&build(rooms)).expect("a level at the rooms cap must validate");
    let error = validate_level(&build(rooms.saturating_add(1)))
        .expect_err("one over the rooms cap must be rejected");
    assert!(error.contains("too many rooms"), "{error}");
}

#[test]
fn test_validate_rejects_too_many_sequence_steps() {
    let steps = |count: usize| -> String {
        let entries: Vec<String> = (0..count)
            .map(|_| r#"{ "step": "wait", "seconds": 0.1 }"#.to_string())
            .collect();
        format!(r#"{{ "id": "long", "steps": [{}] }}"#, entries.join(","))
    };
    let build = |count: usize| {
        LevelDef::from_json(&format!(
            r#"{{ "format_version": 3, "id": "steps_cap", "name": "Steps Cap",
                 "spawn": {{ "x": 1.0, "z": 1.0 }},
                 "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }} ],
                 "sequences": [ {} ] }}"#,
            steps(count)
        ))
        .expect("the step cap level parses")
    };
    let cap = crate::entities::sequences::MAX_SEQUENCE_STEPS;
    validate_level(&build(cap)).expect("a sequence at the step cap must validate");
    let error = validate_level(&build(cap.saturating_add(1)))
        .expect_err("one over the step cap must be rejected");
    assert!(error.contains("steps"), "{error}");
}

#[test]
fn test_validate_rejects_too_many_distinct_materials_by_name() {
    // Distinct material ids are manufactured through one wall's face map:
    // every value is a referenced material, and the count is exactly the set
    // `referenced_material_ids` resolves. The wall itself plus the three
    // defaults are the other four ids.
    let faces = |count: usize| -> String {
        let entries: Vec<String> = (0..count)
            .map(|index| format!(r#""cap_face_{index:06}": "cap:mat_{index:06}""#))
            .collect();
        format!("{{ {} }}", entries.join(","))
    };
    let level = |count: usize| -> LevelDef {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "materials_cap",
                "name": "Materials Cap",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "defaults": {{ "wall": "cap:default_wall", "floor": "cap:default_floor",
                               "ceiling": "cap:default_ceiling" }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }} ],
                "walls": [ {{ "x": 0.0, "z": 2.0, "width": 4.0, "depth": 0.2,
                              "material": "cap:wall_body", "faces": {} }} ]
            }}"#,
            faces(count)
        ))
        .expect("the materials cap level parses")
    };
    // 4 ids outside the face map: three defaults plus the wall body.
    let face_budget = usize::try_from(crate::level::MAX_LEVEL_MATERIALS)
        .expect("fixture integer fits usize")
        .saturating_sub(4);
    let at_limit = level(face_budget);
    assert_eq!(
        crate::materials::referenced_material_ids(&at_limit).len(),
        usize::try_from(crate::level::MAX_LEVEL_MATERIALS).expect("fixture integer fits usize"),
        "the at-limit level must resolve exactly the cap"
    );
    validate_level(&at_limit).expect("a level at the material cap must validate");
    let over = level(face_budget.saturating_add(1));
    let error = validate_level(&over).expect_err("one over the material cap must be rejected");
    assert!(
        error.contains(&format!(
            "Level declares too many distinct materials: {} (limit {})",
            crate::level::MAX_LEVEL_MATERIALS.saturating_add(1),
            crate::level::MAX_LEVEL_MATERIALS
        )),
        "{error}"
    );
}

/// Display edits reuse expensive static records; every physical input remains
/// in the versioned stage identity. This is an input-dependency oracle.
#[test]
fn display_metadata_reuse_keeps_every_physical_dependency_in_the_stage_key() {
    let level = LevelDef::from_json(include_str!("../../tests/fixtures/levels/test_room.json"))
        .expect("checked-in room");
    let variants = crate::quality::LightmapQuality::ALL;
    let dependencies = vec![crate::package::manifest::PackageDependency {
        kind: crate::package::manifest::DependencyKind::Model,
        path: "models/example.glb".into(),
        sha256: "ab".repeat(32),
        bytes: 123,
    }];
    let revision = crate::render::GEOMETRY_REVISION;
    let key =
        |physical_level: &LevelDef,
         physical_dependencies: &[crate::package::manifest::PackageDependency]| {
            crate::compiler::lighting_fingerprint_with_revision(
                physical_level,
                &variants,
                physical_dependencies,
                revision,
            )
            .expect("stage identity")
        };
    let original = key(&level, &dependencies);
    let mut display_edit = level.clone();
    display_edit.name = "Edited display title".into();
    display_edit.author = "Edited attribution".into();
    assert_eq!(key(&display_edit, &dependencies), original);
    let mut geometry_edit = level.clone();
    geometry_edit.rooms[0].width += 0.01;
    assert_ne!(key(&geometry_edit, &dependencies), original);
    let mut light_edit = level.clone();
    light_edit.ceiling_lights[0].x += 0.01;
    assert_ne!(key(&light_edit, &dependencies), original);
    let mut material_edit = level.clone();
    material_edit.defaults.floor = "core:carpet_grey_01".into();
    assert_ne!(key(&material_edit, &dependencies), original);
    let mut model_edit = dependencies.clone();
    model_edit[0].sha256 = "cd".repeat(32);
    assert_ne!(key(&level, &model_edit), original);
    let mut dependency_path_edit = dependencies.clone();
    dependency_path_edit[0].path = "models/other.glb".into();
    assert_ne!(key(&level, &dependency_path_edit), original);
    assert_ne!(
        crate::compiler::lighting_fingerprint_with_revision(
            &level,
            &[crate::quality::LightmapQuality::Off],
            &dependencies,
            revision
        )
        .expect("quality key"),
        original
    );
    assert_ne!(
        crate::compiler::lighting_fingerprint_with_revision(
            &level,
            &variants,
            &dependencies,
            revision + 1
        )
        .expect("algorithm version key"),
        original
    );
}

#[test]
fn environment_defaults_authored_controls_and_validation_are_explicit() {
    let base = include_str!("../../tests/fixtures/levels/art_style_hero.json");
    let mut json: serde_json::Value = serde_json::from_str(base).expect("hero JSON");
    let legacy = LevelDef::from_json(base).expect("legacy defaults");
    assert!(legacy.environment.is_none());
    json["environment"] = serde_json::json!({});
    let defaulted: LevelDef = serde_json::from_value(json.clone()).expect("default environment");
    validate_level(&defaulted).expect("compatible default environment");
    assert_eq!(
        defaulted.environment,
        Some(crate::environment::EnvironmentDef::default())
    );
    let historical_fog = crate::render::LevelFog::from_level(&legacy);
    assert_eq!(
        historical_fog,
        crate::render::LevelFog::from_level(&defaulted)
    );
    for (field, value) in [
        ("exposure", 0.0_f64),
        ("exposure", 8.01_f64),
        ("tone_knee", 1.0_f64),
        ("saturation", 2.0_f64),
        ("contrast", -1.0_f64),
    ] {
        json["environment"] = serde_json::json!({"presentation":{field:value}});
        let invalid: LevelDef = serde_json::from_value(json.clone()).expect("numeric JSON");
        assert!(
            validate_level(&invalid)
                .expect_err("reject authored presentation")
                .contains(field)
        );
    }
    for fog in [
        serde_json::json!({"density":-0.1_f64}),
        serde_json::json!({"color":[1.1_f64,0.0_f64,0.0_f64]}),
        serde_json::json!({"height_gain":2.0_f64}),
    ] {
        json["environment"] = serde_json::json!({"fog":fog});
        let invalid: LevelDef = serde_json::from_value(json.clone()).expect("numeric JSON");
        assert!(validate_level(&invalid).is_err());
    }
    json["environment"] = serde_json::json!({"auto_exposure":true});
    assert!(
        serde_json::from_value::<LevelDef>(json).is_err(),
        "unknown authoring controls must not silently disappear"
    );
}

#[test]
fn environment_roundtrip_preserves_exposure_fog_and_sky_colour() {
    let mut json: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/levels/art_style_hero.json"
    ))
    .expect("hero JSON");
    json["environment"] = serde_json::json!({"presentation":{"exposure":1.25_f64,"saturation":1.0_f64},"fog":{"color":[0.12_f64,0.18_f64,0.3_f64],"density":0.02_f64,"height_gain":0.1_f64}});
    json["sky"]["ambient_color"] = serde_json::json!([0.2_f64, 0.35_f64, 0.7_f64]);
    let level: LevelDef = serde_json::from_value(json.clone()).expect("authored controls");
    validate_level(&level).expect("authored controls validate");
    let replay = LevelDef::from_json(&serde_json::to_string(&level).expect("serialize"))
        .expect("replay controls");
    assert_eq!(replay.environment, level.environment);
    assert_eq!(
        replay.sky.as_ref().expect("sky").ambient_color,
        Some([0.2, 0.35, 0.7])
    );
    let fog = crate::render::LevelFog::from_level(&replay);
    assert_eq!(fog.global.color, [0.12, 0.18, 0.3]);
    assert_exact(fog.global.density, 0.02);
    assert_exact(
        replay
            .environment
            .expect("environment")
            .presentation
            .exposure,
        1.25,
    );
    json["sky"]["ambient_color"] = serde_json::json!([0.2_f64, 1.1_f64, 0.7_f64]);
    assert!(validate_level(&serde_json::from_value(json).expect("bad sky colour")).is_err());
}

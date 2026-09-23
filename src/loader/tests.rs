//! Unit tests for level packs, validation and the level manager.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_clone,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

use super::*;
use crate::level::{RoomDef, WallDef};
use crate::test_support::{assert_exact, assert_exact_array};
use std::io::Cursor;

#[test]
fn test_validate_level_success() {
    let level = LevelDef {
        format_version: 1,
        id: "test_level".into(),
        name: "Test Level".into(),
        author: "Author".into(),
        room: None,
        rooms: vec![RoomDef {
            ceiling: crate::level::CeilingProfileDef::Flat,
            floor_y: 0.0,
            x: 0.0,
            z: 0.0,
            width: 20.0,
            depth: 20.0,
            height: 3.5,
            material: None,
            ceiling_material: None,
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
        }],
        floor_patches: vec![],
        floor_regions: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
        animated_emissions: Vec::new(),
    };
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_level_invalid_version() {
    let level = LevelDef {
        format_version: 2,
        id: "test".into(),
        name: "Test".into(),
        author: String::new(),
        room: None,
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
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
        animated_emissions: Vec::new(),
    };
    assert!(validate_level(&level).is_err());
}

#[test]
fn test_validate_level_preserves_overlapping_geometry() {
    // Overlapping walls and rooms are explicitly legal
    let level = LevelDef {
        format_version: 1,
        id: "overlap".into(),
        name: "Overlap".into(),
        author: String::new(),
        room: None,
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
                ceiling_material: None,
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
                ceiling_material: None,
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
            },
        ],
        floor_patches: vec![],
        floor_regions: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
        animated_emissions: Vec::new(),
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
fn test_zip_extraction_and_path_traversal_rejection() {
    use zip::write::SimpleFileOptions;

    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        writer.start_file("../evil.txt", options).unwrap();
        std::io::Write::write_all(&mut writer, b"evil").unwrap();
        writer.finish().unwrap();
    }

    let result = extract_zip(Cursor::new(&buffer));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unsafe path traversal"));
}

#[test]
fn test_zip_level_pack_extraction() {
    use zip::write::SimpleFileOptions;

    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        let level_json = r#"{
            "format_version": 1,
            "id": "zip_test",
            "name": "Zip Test Level",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }]
        }"#;

        writer.start_file("level.json", options).unwrap();
        std::io::Write::write_all(&mut writer, level_json.as_bytes()).unwrap();

        let materials_json = r#"{
            "pack:wall": "textures/wall.png"
        }"#;
        writer.start_file("materials.json", options).unwrap();
        std::io::Write::write_all(&mut writer, materials_json.as_bytes()).unwrap();

        writer.start_file("textures/wall.png", options).unwrap();
        std::io::Write::write_all(&mut writer, b"mock_png_bytes").unwrap();

        writer.finish().unwrap();
    }

    let pack = extract_zip(Cursor::new(&buffer)).expect("valid zip");
    assert!(pack.level_json.contains("zip_test"));
    assert!(pack.materials_json.is_some());
    assert!(pack.textures.contains_key("textures/wall.png"));
}

#[test]
fn test_missing_pack_materials_use_the_diagnostic_texture_with_an_error() {
    let level = LevelDef {
        format_version: 1,
        id: "fallback_test".into(),
        name: "Fallback Test".into(),
        author: String::new(),
        room: None,
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
        },
        walls: vec![],
        floor_patches: vec![],
        floor_regions: vec![],
        ceiling_lights: vec![],
        decals: Vec::new(),
        props: vec![],
        animated_emissions: Vec::new(),
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
    assert_eq!(image.rgba.len(), (64 * 64 * 4) as usize);
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
                (hash ^ (u64::from(*byte) + index as u64)).wrapping_mul(0x0100_0000_01b3)
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
                r#"{"format_version": 1, "id": "x", "name": "x", "spawn": {"x": 0.0, "z": 0.0}}"#,
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
            "format_version": 1,
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
    let rooms: Vec<String> = (0..10)
        .map(|i| {
            format!(
                r#"{{ "x": {}, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }}"#,
                i as f32 * 100.0
            )
        })
        .collect();
    let level = level_from_rooms_json(&format!("[{}]", rooms.join(",")));
    assert!(validate_level(&level).is_ok());
}

#[test]
fn test_validate_rejects_pathological_huge_room() {
    let level = level_from_rooms_json(
        r#"[{ "x": 0.0, "z": 0.0, "width": 2000.0, "depth": 2000.0, "height": 3.5 }]"#,
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
    let rooms: Vec<String> = (0..50)
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
            "format_version": 1,
            "id": "vertical",
            "name": "Vertical",
            "spawn": {{ "x": 4.0, "z": 4.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0{room_extra} }}{level_extra}
        }}"#
    ))
    .expect("valid vertical json")
}

#[test]
fn test_validate_accepts_legacy_and_vertical_rooms() {
    // Legacy: no elevation, no profile.
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
    let level = vertical_level(r#", "floor_y": 1.0e38"#, "");
    let level = {
        // serde_json cannot express infinity, so build it programmatically.
        let mut level = level;
        level.rooms.push(crate::level::RoomDef {
            x: 20.0,
            z: 0.0,
            width: 4.0,
            depth: 4.0,
            height: 3.0,
            floor_y: f32::INFINITY,
            ceiling: crate::level::CeilingProfileDef::Flat,
            material: None,
            ceiling_material: None,
        });
        level
    };
    let err = validate_level(&level).expect_err("non-finite elevation must be rejected");
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
        ceiling_material: None,
    });
    let err = validate_level(&level).expect_err("NaN ridge must be rejected");
    assert!(err.contains("ridge rise"), "unexpected error: {err}");
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
    let level = vertical_level(
        r#", "floor_y": 2.0"#,
        r#", "decals": [
            { "x": 4.0, "y": 5.0, "z": 4.0, "width": 1.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "ceiling" }
        ]"#,
    );
    assert!(validate_level(&level).is_ok());
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
    let level = vertical_level(
        "",
        r#", "floor_regions": [
            { "x": 4.0, "z": 3.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0 }
        ],
        "decals": [
            { "x": 2.0, "y": 0.0, "z": 5.0, "width": 2.0, "height": 1.0,
              "material": "core:decal_test_01", "surface": "floor" }
        ]"#,
    );
    assert!(validate_level(&level).is_ok());
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
            "format_version": 1,
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
fn test_zip_reads_are_capped_by_output_not_the_declared_size() {
    // A lying header can declare one byte and still expand past the cap; the
    // reader must bound its own output rather than trust `entry.size()`.
    let payload = vec![0u8; usize::try_from(MAX_ZIP_ENTRY_SIZE + 1).expect("cap fits usize")];
    let mut cursor = Cursor::new(payload);
    let error = read_zip_entry_capped(&mut cursor, 1, MAX_ZIP_ENTRY_SIZE, "bomb.bin")
        .expect_err("output past the cap must fail");
    assert!(error.contains("decompression limit"), "unexpected: {error}");
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

#[test]
fn test_read_zip_level_json_only_reads_level_json() {
    use zip::write::SimpleFileOptions;

    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        let level_json = r#"{ "format_version": 1, "id": "probe", "name": "Probe",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }] }"#;
        writer.start_file("level.json", options).unwrap();
        std::io::Write::write_all(&mut writer, level_json.as_bytes()).unwrap();

        // A large texture that probing must not read/decompress.
        writer.start_file("textures/huge.png", options).unwrap();
        std::io::Write::write_all(&mut writer, &vec![0u8; 4096]).unwrap();
        writer.finish().unwrap();
    }

    let json = read_zip_level_json(Cursor::new(&buffer)).expect("reads level.json");
    assert!(json.contains("\"probe\""));
}

#[test]
fn test_read_zip_level_json_missing_entry_errors() {
    use zip::write::SimpleFileOptions;

    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("readme.txt", options).unwrap();
        std::io::Write::write_all(&mut writer, b"no level here").unwrap();
        writer.finish().unwrap();
    }

    assert!(read_zip_level_json(Cursor::new(&buffer)).is_err());
}

#[test]
fn test_extract_zip_shares_texture_blobs_between_aliases() {
    use zip::write::SimpleFileOptions;

    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("level.json", options).unwrap();
        std::io::Write::write_all(&mut writer, b"{}").unwrap();
        writer.start_file("textures/wall.png", options).unwrap();
        std::io::Write::write_all(&mut writer, b"PAYLOAD").unwrap();
        writer.finish().unwrap();
    }

    let pack = extract_zip(Cursor::new(&buffer)).expect("valid zip");
    let full = pack
        .textures
        .get("textures/wall.png")
        .expect("full path alias");
    let bare = pack.textures.get("wall.png").expect("bare name alias");
    assert_eq!(&**full, b"PAYLOAD");
    // Aliases must reference the same physical allocation.
    assert!(Rc::ptr_eq(full, bare));
}

fn level_with_opening_json(opening_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "opening_test",
            "name": "Opening Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
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
            "format_version": 1,
            "id": "props_test",
            "name": "Props Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
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
    let err = validate_level(&vent).expect_err("vent must not extend past the wall");
    assert!(
        err.starts_with("Opening extends beyond this wall"),
        "unexpected error: {err}"
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
    let err = validate_level(&negative_offset).expect_err("negative offset is invalid");
    assert!(
        err.contains("starts before the wall"),
        "unexpected error: {err}"
    );

    let zero_width = level_with_opening_json(
        r#"{ "kind": "door", "offset": 1.0, "width": 0.0, "height": 2.1 }"#,
    );
    let err = validate_level(&zero_width).expect_err("zero width is invalid");
    assert!(
        err.contains("must have a positive width and height"),
        "unexpected error: {err}"
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
    let mut non_finite = non_finite;
    non_finite.props[0].x = f32::INFINITY;
    assert!(
        validate_level(&non_finite)
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
            0x6b as f32 / 255.0,
            0x5f as f32 / 255.0,
            0x4a as f32 / 255.0
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
        "format_version": 1,
        "props": [
            { "id": "core:couch", "name": "Couch", "category": "Furniture",
              "size": [2.0, 0.9, 0.9], "color": "#6b5f4a", "model": null, "solid": true },
            { "id": "core:lamp" }
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
    assert!(mesh.batches.wall_batch.count > 0);
    assert!(mesh.batches.prop_batch.count > 0);
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
    assert!(mesh.batches.wall_batch.count > 0);
    assert!(mesh.batches.floor_batch.count > 0);
    assert!(mesh.batches.light_batch.count > 0);
    assert_eq!(
        mesh.batches.decal_batch.count,
        i32::try_from(level.decals.len() * 6).expect("decal quad count fits"),
        "one quad per placed decal"
    );
    let sheets = crate::render::decal_external_sheet_ids(&level, &assets);
    assert_eq!(
        sheets,
        vec![
            "core:decal_no_diving_01".to_string(),
            "core:decal_stripes_01".to_string()
        ],
        "both decal sheets the demo places resolve as external PNG artwork"
    );
}

/// External/user levels are discovered and loaded independently of the bundled
/// demo: a `.json` dropped into the installed levels directory appears in the
/// level list and loads through the ordinary custom-level path.
#[test]
fn test_custom_levels_are_discovered_and_loaded() {
    use zip::write::SimpleFileOptions;

    let root =
        std::env::temp_dir().join(format!("liminal-custom-level-test-{}", std::process::id()));
    let assets_dir = root.join("assets/levels");
    let levels_dir = root.join("levels");
    let import_dir = root.join("import");
    fs::create_dir_all(&assets_dir).expect("test assets dir");
    fs::create_dir_all(&levels_dir).expect("test levels dir");

    let level_json = r#"{
        "format_version": 1,
        "id": "community_room",
        "name": "Community Room",
        "author": "A Player",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
        "ceiling_lights": [{ "fixture": "core:ceiling_panel_01", "x": 2.0, "z": 2.0 }]
    }"#;
    fs::write(levels_dir.join("community_room.json"), level_json).expect("write the drop-in level");

    let manager = LevelManager::with_paths(assets_dir, levels_dir.clone(), import_dir.clone());
    let entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == "community_room")
        .cloned()
        .expect("a drop-in level is discovered");
    assert_eq!(entry.source_type, LevelSourceType::CustomJson);
    assert_eq!(entry.name, "Community Room");
    let loaded = manager.load_level(&entry).expect("the drop-in level loads");
    assert_eq!(loaded.level.name, "Community Room");

    // A `.zip` pack in the same directory is discovered and loaded as a pack.
    let pack_path = levels_dir.join("pack_room.zip");
    {
        let file = fs::File::create(&pack_path).expect("create the pack");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let pack_json = r#"{
            "format_version": 1,
            "id": "pack_room",
            "name": "Pack Room",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
        }"#;
        writer
            .start_file("level.json", options)
            .expect("pack level.json");
        std::io::Write::write_all(&mut writer, pack_json.as_bytes()).expect("write level.json");
        writer.finish().expect("finish the pack");
    }
    let manager = LevelManager::with_paths(root.join("assets/levels"), levels_dir, import_dir);
    let pack_entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == "pack_room")
        .cloned()
        .expect("a drop-in pack is discovered");
    assert_eq!(pack_entry.source_type, LevelSourceType::PackZip);
    let loaded_pack = manager
        .load_level(&pack_entry)
        .expect("the drop-in pack loads");
    assert_eq!(loaded_pack.level.name, "Pack Room");

    fs::remove_dir_all(&root).expect("clean up the test directory");
}

#[test]
fn test_the_default_level_is_the_shipped_demo() {
    let manager = LevelManager::new();
    // The Level Select menu renders exactly this list: the demo is the only
    // bundled (Official) entry; any drop-in levels are CustomJson/PackZip.
    let official: Vec<&LevelEntry> = manager
        .entries()
        .iter()
        .filter(|entry| entry.source_type == LevelSourceType::Official)
        .collect();
    assert_eq!(
        official.len(),
        1,
        "Places Demo must be the only bundled level, found {:?}",
        official
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(official[0].id, "places_demo");

    let loaded = manager.load_default().expect("the shipped demo loads");
    assert_eq!(loaded.entry.id, "places_demo");
    assert_eq!(loaded.entry.source_type, LevelSourceType::Official);
    assert_eq!(loaded.level.name, "Places Demo");
}

#[test]
fn test_ceiling_light_intensity_is_optional_and_sanitized() {
    let base = |lights: &str| {
        format!(
            r#"{{
                "format_version": 1,
                "id": "intensity",
                "name": "Intensity",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }},
                "ceiling_lights": {lights}
            }}"#
        )
    };

    // Backward compatibility: an omitted intensity is the standard fixture.
    let omitted = LevelDef::from_json(&base(
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }]"#,
    ))
    .expect("omitted intensity parses");
    validate_level(&omitted).expect("an omitted intensity validates");
    assert_eq!(omitted.ceiling_lights[0].brightness, None);
    assert_exact(omitted.ceiling_lights[0].intensity(), 1.0);

    // The editor's `brightness` key and the `intensity` alias both load.
    let both = LevelDef::from_json(&base(
        r#"[
            { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 0.8 },
            { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0, "intensity": 1.4 }
        ]"#,
    ))
    .expect("both spellings parse");
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
                "format_version": 1,
                "id": "colour",
                "name": "Colour",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }},
                "ceiling_lights": {lights}
            }}"#
        )
    };

    // Omitted colour: legacy levels keep loading and emit the documented
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

    // Serialisation round trip: the array shape survives editor saves.
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

    let mut levels_checked = 0;
    let mut props_checked = 0;
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
        levels_checked += 1;
        for prop in &level.props {
            assert!(
                catalog.contains(&prop.model),
                "{} uses prop '{}' which is missing from assets/catalog.json",
                path.display(),
                prop.model
            );
            props_checked += 1;
        }
    }
    assert!(levels_checked >= 1, "expected the shipped level file");
    assert!(props_checked >= 1, "expected at least one placed prop");
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
            "format_version": 1,
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
    let mut level = base.clone();
    level.decals[0].height = -1.0;
    assert!(validate_level(&level).is_err(), "negative height");
    let mut level = base.clone();
    level.decals[0].width = 11.0;
    assert!(validate_level(&level).is_err(), "oversized width");
    let mut level = base.clone();
    level.decals[0].material = "  ".into();
    assert!(validate_level(&level).is_err(), "empty material");

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
    let decals = std::iter::repeat_n(decal, 5001)
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
        i32::try_from(level.decals.len()).unwrap_or(0) * 6,
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
        i32::try_from(level.decals.len()).unwrap_or(0) * 6
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
            "format_version": 1,
            "id": "fixture_sheets",
            "name": "Fixture Sheets",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 6.0, "height": 3.0 }],
            "ceiling_lights": [
                { "fixture": "core:pool_light_round", "x": 2.0, "z": 2.0 },
                { "fixture": "core:pool_light_round", "x": 4.0, "z": 2.0 },
                { "fixture": "core:pool_light_wall", "x": 6.0, "z": 2.0,
                  "mount": "wall", "y": 1.7, "rotation_degrees": 180.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let catalog = crate::assets::AssetCatalog::load_default();
    let mut cache = TextureCache::new();
    let sheets = resolve_fixture_sheets(&level, &catalog, None, &mut cache);

    // One sheet per family, in the level's first-use order; the second round
    // fixture adds nothing.
    let kinds: Vec<crate::lighting::FixtureKind> = sheets.iter().map(|sheet| sheet.kind).collect();
    assert_eq!(
        kinds,
        [
            crate::lighting::FixtureKind::RoundRecessed,
            crate::lighting::FixtureKind::WallSconce,
            crate::lighting::FixtureKind::FluorescentPanel,
        ]
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
            sheet.key.to_ascii_lowercase().ends_with(".png") && !sheet.key.starts_with("assets/"),
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
    // is stretched: the panel and the wall lens are 2:1, the round sheet 1:1.
    let dimensions = |kind| {
        let sheet = sheet_for(kind);
        (sheet.image.width, sheet.image.height)
    };
    assert_eq!(
        dimensions(crate::lighting::FixtureKind::FluorescentPanel),
        (1024, 512)
    );
    assert_eq!(
        dimensions(crate::lighting::FixtureKind::RoundRecessed),
        (128, 128)
    );
    assert_eq!(
        dimensions(crate::lighting::FixtureKind::WallSconce),
        (128, 64)
    );
}

/// A fixture with no sheet to draw (an unknown id, or a `pack:` id whose pack
/// carries nothing) resolves to nothing at all: the family draws the shared
/// white sheet instead of borrowing some other fixture's artwork.
#[test]
fn test_fixtures_without_a_sheet_resolve_to_nothing() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
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
    let _ = std::fs::remove_dir_all(scratch);
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

    let _ = std::fs::remove_dir_all(scratch);
}

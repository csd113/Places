//! Unit tests for the material pipeline.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::suboptimal_flops,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::needless_raw_string_hashes,
    clippy::panic,
    clippy::redundant_clone,
    clippy::float_cmp
)]

use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

use super::*;
use crate::assets::{
    AssetCatalog, AssetSource, MAX_TEXTURE_DIMENSION, ShippedTextureKind, decoded_rgba_bytes,
};
use crate::level::LevelDef;

fn level_from(json: &str) -> LevelDef {
    LevelDef::from_json(json).expect("test level parses")
}

fn basic_level(wall: &str, floor: &str, ceiling: &str) -> LevelDef {
    level_from(&format!(
        r##"{{
            "format_version": 1,
            "id": "material_test", "name": "Material Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "defaults": {{ "wall": "{wall}", "floor": "{floor}", "ceiling": "{ceiling}" }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }}]
        }}"##
    ))
}

fn shipped_catalog() -> AssetCatalog {
    AssetCatalog::load_default()
}

/// Encodes raw samples with an explicit PNG colour type (test helper).
fn encode_as(
    color: png::ColorType,
    depth: png::BitDepth,
    width: u32,
    height: u32,
    data: &[u8],
    palette: Option<Vec<u8>>,
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(data).expect("data");
    }
    out
}

#[test]
fn decode_png_round_trips_rgba_exactly() {
    let image = RawImage::new(
        3,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 255, 40, 50, 60, 255, 70, 80,
            90, 64,
        ],
    );
    let encoded = encode_png(&image).expect("encode");
    let decoded = decode_png(&encoded).expect("decode");
    assert_eq!(decoded, image);
}

#[test]
fn decode_png_accepts_rgb_grayscale_palette_and_sixteen_bit_images() {
    let rgb = encode_as(
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        2,
        1,
        &[1, 2, 3, 4, 5, 6],
        None,
    );
    assert_eq!(
        decode_png(&rgb).expect("rgb").rgba,
        vec![1, 2, 3, 255, 4, 5, 6, 255]
    );

    let gray = encode_as(
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        2,
        1,
        &[7, 9],
        None,
    );
    assert_eq!(
        decode_png(&gray).expect("gray").rgba,
        vec![7, 7, 7, 255, 9, 9, 9, 255]
    );

    let gray_alpha = encode_as(
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Eight,
        2,
        1,
        &[7, 128, 9, 0],
        None,
    );
    assert_eq!(
        decode_png(&gray_alpha).expect("gray alpha").rgba,
        vec![7, 7, 7, 128, 9, 9, 9, 0]
    );

    let palette = encode_as(
        png::ColorType::Indexed,
        png::BitDepth::Eight,
        2,
        1,
        &[0, 1],
        Some(vec![10, 20, 30, 40, 50, 60]),
    );
    assert_eq!(
        decode_png(&palette).expect("palette").rgba,
        vec![10, 20, 30, 255, 40, 50, 60, 255]
    );

    // 16-bit samples are stripped to their high byte, not rejected.
    let sixteen = encode_as(
        png::ColorType::Rgb,
        png::BitDepth::Sixteen,
        1,
        1,
        &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC],
        None,
    );
    let decoded = decode_png(&sixteen).expect("16-bit");
    assert_eq!(decoded.rgba, vec![0x12, 0x56, 0x9A, 255]);
}

#[test]
fn malformed_and_truncated_pngs_are_errors_not_panics() {
    assert!(decode_png(b"").is_err());
    assert!(decode_png(b"not a png at all").is_err());
    assert!(decode_png(b"\x89PNG\r\n\x1a\n").is_err());

    let image = RawImage::new(4, 4, vec![200; 4 * 4 * 4]);
    let bytes = encode_png(&image).expect("encode");
    let truncated = &bytes[..bytes.len() / 2];
    assert!(decode_png(truncated).is_err(), "truncated PNG must fail");
    let mut corrupted = bytes.clone();
    let mid = corrupted.len() / 2;
    corrupted[mid] ^= 0xFF;
    assert!(decode_png(&corrupted).is_err(), "corrupt PNG must fail");
}

#[test]
fn oversized_pngs_are_rejected_with_a_clear_message() {
    let wide = RawImage::new(
        MAX_TEXTURE_DIMENSION + 1,
        1,
        vec![0; 4 * (MAX_TEXTURE_DIMENSION as usize + 1)],
    );
    let bytes = encode_png(&wide).expect("encode");
    let error = decode_png(&bytes).expect_err("oversized must fail");
    assert!(error.contains("exceed"), "unexpected error: {error}");
}

#[test]
fn cache_decodes_each_key_once_and_reuses_the_buffer() {
    let mut cache = TextureCache::new();
    assert!(cache.get("core:tex_a").is_none());
    let first = cache.insert("core:tex_a", RawImage::new(1, 1, vec![1, 2, 3, 4]));
    let second = cache.get("core:tex_a").expect("cached");
    assert!(Rc::ptr_eq(&first, &second), "the same buffer is shared");
    assert_eq!(cache.decoded_count(), 1);
    assert_eq!(cache.len(), 1);
}

#[test]
fn shipped_materials_resolve_through_the_catalog() {
    let catalog = shipped_catalog();
    let level = basic_level(
        "core:wallpaper_yellow_01",
        "core:carpet_damp_01",
        "core:ceiling_panel_01",
    );
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

    assert_eq!(table.len(), 3);
    assert!(table.errors().is_empty(), "errors: {:?}", table.errors());
    let wall = table.entry_of("core:wallpaper_yellow_01").expect("wall");
    assert_eq!(wall.origin, TextureOrigin::Catalog);
    assert_eq!(wall.texture_key, "core:tex_wallpaper_yellow_01");
    assert_eq!(wall.tile_metres, 2.0);
    assert_eq!(wall.tint, [0.85, 0.80, 0.42]);
    let image = wall.image.as_ref().expect("decoded image");
    ShippedTextureKind::Surface
        .check_dimensions(image.width, image.height)
        .unwrap_or_else(|error| panic!("core:tex_wallpaper_yellow_01: {error}"));
    assert_eq!(
        image.width, image.height,
        "a surface sheet is sampled as a square tile_metres cell"
    );
    assert!(
        image.width > 0 && image.height > 0,
        "the decoded sheet must be non-empty"
    );
    assert!(
        image.width <= MAX_TEXTURE_DIMENSION && image.height <= MAX_TEXTURE_DIMENSION,
        "{}x{} is over the hard {MAX_TEXTURE_DIMENSION}px limit",
        image.width,
        image.height
    );
    assert_eq!(
        image.rgba.len(),
        decoded_rgba_bytes(image.width, image.height),
        "the decoded buffer must be width*height RGBA8"
    );

    let floor = table.entry_of("core:carpet_damp_01").expect("floor");
    assert_eq!(floor.tint, DEFAULT_TINT);
    assert_eq!(table.textures().len(), 3);
    assert_eq!(cache.decoded_count(), 3);
}

#[test]
fn two_materials_sharing_a_texture_share_one_resolved_texture() {
    let catalog = shipped_catalog();
    // A synthetic catalog where two materials point at one texture.
    let json = r##"{
        "themes": [{ "id": "office" }],
        "assets": [
            { "id": "core:tex_shared", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_a", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_shared" },
            { "id": "core:mat_b", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_shared", "tile_metres": 4.0 }
        ]
    }"##;
    let catalog2 = AssetCatalog::from_json_str(json).expect("synthetic catalog");
    let level = basic_level("core:mat_a", "core:mat_b", "core:mat_a");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog2, None, Some(&root), &mut cache);

    assert_eq!(table.len(), 2);
    assert_eq!(table.textures().len(), 1, "one shared texture");
    assert_eq!(cache.decoded_count(), 1, "decoded once");
    assert_eq!(table.entry_of("core:mat_b").expect("b").tile_metres, 4.0);
    let a = table.entry_of("core:mat_a").expect("a");
    let b = table.entry_of("core:mat_b").expect("b");
    assert_eq!(a.texture_index, b.texture_index, "one GPU upload slot");
    assert_eq!(a.texture_index, 0);
    let _ = catalog;
}

#[test]
fn unknown_material_uses_the_diagnostic_texture_with_a_useful_error() {
    let catalog = shipped_catalog();
    let level = basic_level(
        "core:not_a_material",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
    );
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

    let entry = table.entry_of("core:not_a_material").expect("entry");
    assert_eq!(entry.origin, TextureOrigin::Missing);
    assert_eq!(entry.texture_key, MISSING_TEXTURE_KEY);
    let error = entry.error.as_deref().expect("error");
    assert!(error.contains("core:not_a_material"), "error: {error}");
    assert!(error.contains("catalog"), "error: {error}");
    assert!(table.first_missing().is_some());
}

#[test]
fn missing_png_falls_back_to_the_diagnostic_and_names_both_ids() {
    let catalog = shipped_catalog();
    let level = basic_level(
        "core:wallpaper_yellow_01",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
    );
    let mut cache = TextureCache::new();
    let empty =
        std::env::temp_dir().join(format!("places_materials_missing_{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty);
    fs::create_dir_all(&empty).expect("temp dir");
    let table = resolve_materials(&level, &catalog, None, Some(&empty), &mut cache);

    let wall = table.entry_of("core:wallpaper_yellow_01").expect("wall");
    let error = wall.error.as_deref().expect("error");
    assert!(error.contains("core:wallpaper_yellow_01"), "error: {error}");
    assert!(
        error.contains("core:tex_wallpaper_yellow_01"),
        "error: {error}"
    );
    assert_eq!(wall.origin, TextureOrigin::Missing);
    let _ = fs::remove_dir_all(&empty);
}

#[test]
fn material_id_in_the_wrong_type_is_reported() {
    let catalog = shipped_catalog();
    let level = basic_level("core:desk", "core:carpet_beige_01", "core:ceiling_panel_01");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    let error = table
        .entry_of("core:desk")
        .and_then(|entry| entry.error.clone())
        .expect("error");
    assert!(error.contains("`prop` asset"), "error: {error}");
}

#[test]
fn referenced_ids_are_deterministic_and_cover_faces_patches_and_regions() {
    let level = level_from(
        r##"{
            "format_version": 1,
            "id": "scan_test", "name": "Scan Test",
            "spawn": { "x": 0.0, "z": 0.0 },
            "defaults": { "wall": "w", "floor": "f", "ceiling": "c" },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0,
                        "material": "room-floor", "ceiling_material": "room-ceiling" }],
            "walls": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.2,
                        "material": "wall-body", "faces": { "south": "face-s", "north": "face-n" } }],
            "floor_patches": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0, "material": "patch" }],
            "floor_regions": [{ "x": 2.0, "z": 2.0, "width": 1.0, "depth": 1.0,
                                "offset_y": -0.5, "material": "region-floor", "edge_material": "region-edge" }],
            "ramps": [{ "x": 0.0, "z": 0.0, "width": 1.0, "depth": 2.0,
                        "rise": 0.4, "material": "ramp-top", "edge_material": "ramp-edge" }],
            "stairs": [{ "x": 0.0, "z": 0.0, "width": 1.0, "depth": 2.0, "rise": 0.6, "steps": 3,
                         "material": "stair-tread", "riser_material": "stair-riser",
                         "side_material": "stair-side" }],
            "half_walls": [{ "x": 0.0, "z": 0.0, "width": 2.0, "depth": 0.2, "height": 1.05,
                             "material": "knee", "end_material": "knee-end", "cap_material": "knee-cap" }],
            "columns": [{ "x": 0.0, "z": 0.0, "width": 0.3, "depth": 0.3,
                          "material": "post", "cap_material": "post-cap" }],
            "archways": [{ "x": 0.0, "z": 0.0, "width": 0.3, "depth": 1.4, "height": 3.0,
                           "opening_width": 1.0, "opening_height": 2.1, "arch_rise": 0.2,
                           "material": "arch-body", "reveal_material": "arch-reveal" }],
            "guardrails": [{ "x": 0.0, "z": 0.0, "length": 2.0,
                             "material": "rail", "post_material": "rail-post" }],
            "thresholds": [{ "x": 3.0, "z": 0.0, "length": 1.0, "material": "strip" }],
            "baseboards": [{ "x": 0.0, "z": 0.0, "length": 2.0, "material": "skirt" }]
        }"##,
    );
    let ids = referenced_material_ids(&level);
    assert_eq!(
        ids,
        vec![
            "w",
            "f",
            "c",
            "room-floor",
            "room-ceiling",
            "wall-body",
            "face-n",
            "face-s",
            "patch",
            "region-floor",
            "region-edge",
            "ramp-top",
            "ramp-edge",
            "stair-tread",
            "stair-riser",
            "stair-side",
            "knee",
            "knee-end",
            "knee-cap",
            "post",
            "post-cap",
            "arch-body",
            "arch-reveal",
            "rail",
            "rail-post",
            "strip",
            "skirt",
        ]
    );
    let again = referenced_material_ids(&level);
    assert_eq!(ids, again, "the scan must be deterministic");
}

#[test]
fn pack_materials_parse_both_shapes_and_decode_from_pack_bytes() {
    let png = encode_png(&RawImage::new(
        2,
        2,
        vec![9, 8, 7, 255, 6, 5, 4, 255, 3, 2, 1, 255, 0, 0, 0, 255],
    ))
    .expect("encode");
    let mut textures = HashMap::new();
    textures.insert("textures/wall.png".to_string(), Rc::from(png.clone()));
    textures.insert("wall.png".to_string(), Rc::<[u8]>::from(png.clone()));
    let json = r#"{
        "materials": {
            "pack:wall": { "texture": "textures/wall.png", "tile_metres": 3.0,
                           "tint": [0.5, 0.5, 0.5] },
            "pack:legacy": "textures/wall.png"
        }
    }"#;
    let pack = PackMaterials::new("unit_pack", Some(json), textures);
    let level = basic_level("pack:wall", "pack:legacy", "pack:wall");
    let catalog = AssetCatalog::builtin();
    let mut cache = TextureCache::new();
    let table = resolve_materials(&level, &catalog, Some(&pack), None, &mut cache);

    assert!(table.errors().is_empty(), "errors: {:?}", table.errors());
    let wall = table.entry_of("pack:wall").expect("wall");
    assert_eq!(wall.origin, TextureOrigin::Pack);
    assert_eq!(wall.tile_metres, 3.0);
    assert_eq!(wall.tint, [0.5, 0.5, 0.5]);
    assert_eq!(wall.image.as_ref().expect("image").width, 2);
    assert_eq!(cache.decoded_count(), 1, "shared key decodes once");
    assert_eq!(
        table.textures().len(),
        1,
        "both materials share one texture"
    );
}

#[test]
fn pack_material_without_a_texture_is_a_context_rich_error() {
    let pack = PackMaterials::new("unit_pack", None, HashMap::new());
    let level = basic_level("pack:missing_wall", "f", "c");
    let catalog = AssetCatalog::builtin();
    let mut cache = TextureCache::new();
    let table = resolve_materials(&level, &catalog, Some(&pack), None, &mut cache);
    let error = table
        .entry_of("pack:missing_wall")
        .and_then(|entry| entry.error.clone())
        .expect("error");
    assert!(error.contains("pack:missing_wall"), "error: {error}");
    assert!(
        error.contains("materials.json") && error.contains("PNG"),
        "error should say what to add: {error}"
    );
}

#[test]
fn pack_materials_may_reuse_a_catalog_texture_or_name_a_missing_file() {
    // `materials.json` naming a catalog texture id resolves it through the
    // catalog; naming an absent pack file is a named error.
    let json = r#"{
        "materials": {
            "pack:builtin": { "texture": "core:tex_ceiling_panel_01" },
            "pack:typo": { "texture": "textures/typo_wall.png" }
        }
    }"#;
    let pack = PackMaterials::new("unit_pack", Some(json), HashMap::new());
    let level = basic_level("pack:builtin", "pack:typo", "core:ceiling_panel_01");
    let catalog = AssetCatalog::load_default();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let mut cache = TextureCache::new();
    let table = resolve_materials(&level, &catalog, Some(&pack), Some(&root), &mut cache);

    let builtin = table.entry_of("pack:builtin").expect("builtin entry");
    assert_eq!(builtin.origin, TextureOrigin::Catalog);
    assert_eq!(builtin.texture_key, "core:tex_ceiling_panel_01");
    assert!(builtin.image.is_some());

    let typo = table.entry_of("pack:typo").expect("typo entry");
    assert_eq!(typo.origin, TextureOrigin::Missing);
    let error = typo.error.as_deref().expect("error");
    assert!(error.contains("textures/typo_wall.png"), "error: {error}");
    assert!(error.contains("materials.json"), "error: {error}");
}

#[test]
fn catalog_rejects_materials_with_dangling_or_non_png_textures() {
    let dangling = r##"{
        "assets": [
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_nope" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(dangling).expect_err("dangling texture");
    assert!(error.contains("core:tex_nope"), "error: {error}");

    let not_a_texture = r##"{
        "assets": [
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:prop" },
            { "id": "core:prop", "asset_class": "environment", "asset_type": "prop",
              "source": "file", "model": "core/props/models/couch.glb" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(not_a_texture).expect_err("wrong type");
    assert!(error.contains("not a texture"), "error: {error}");

    let not_png = r##"{
        "assets": [
            { "id": "core:tex_bad", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "core/textures/bad.jpg" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(not_png).expect_err("not a png");
    assert!(error.contains(".png"), "error: {error}");
}

#[test]
fn catalog_rejects_materials_without_a_texture_and_bad_material_metadata() {
    let no_texture = r##"{
        "assets": [
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "definition" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(no_texture).expect_err("no texture");
    assert!(error.contains("texture"), "error: {error}");

    let bad_tile = r##"{
        "assets": [
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "core/textures/a.png" },
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_a", "tile_metres": 0.0 }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(bad_tile).expect_err("bad tile");
    assert!(error.contains("tile_metres"), "error: {error}");

    let bad_tint = r##"{
        "assets": [
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "core/textures/a.png" },
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_a", "tint": [1.5, 0.0, 0.0] }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(bad_tint).expect_err("bad tint");
    assert!(error.contains("tint"), "error: {error}");

    let texture_on_prop = r##"{
        "assets": [
            { "id": "core:p", "asset_class": "environment", "asset_type": "prop",
              "source": "file", "model": "core/props/models/couch.glb",
              "texture": "core:tex_a" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(texture_on_prop).expect_err("texture on prop");
    assert!(error.contains("material"), "error: {error}");
}

#[test]
fn every_shipped_material_resolves_to_a_png_texture() {
    let catalog = shipped_catalog();
    for material in catalog.materials() {
        let texture_id = material
            .texture
            .as_deref()
            .unwrap_or_else(|| panic!("{}: materials must declare a texture", material.id));
        let path = catalog
            .texture_path(texture_id)
            .unwrap_or_else(|| panic!("{}: texture {texture_id} has no PNG", material.id));
        assert!(path.ends_with(".png"), "{}: {path}", material.id);
        assert_eq!(material.source, AssetSource::Definition);
    }
}

/// A synthetic catalog with two emissive materials sharing one mask sheet.
///
/// Both PNG paths are shipped artwork, so the test needs no new asset files.
fn emissive_catalog() -> AssetCatalog {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:tex_mask", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/floors/carpet_beige_01.png" },
            { "id": "core:mat_glow_a", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "emissive": [0.25, 0.5, 1.0], "emissive_intensity": 2.0,
              "emissive_mask": "core:tex_mask" },
            { "id": "core:mat_glow_b", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core:tex_mask" }
        ]
    }"##;
    AssetCatalog::from_json_str(json).expect("synthetic emissive catalog")
}

#[test]
fn materials_without_emission_stay_non_emissive() {
    let catalog = shipped_catalog();
    for id in [
        "core:wallpaper_yellow_01",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
    ] {
        assert_eq!(catalog.material_emissive(id), None, "{id}");
        assert_eq!(catalog.material_emissive_intensity(id), None, "{id}");
        assert_eq!(catalog.material_emissive_mask(id), None, "{id}");
    }

    let level = basic_level(
        "core:wallpaper_yellow_01",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
    );
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    for entry in table.entries() {
        assert_eq!(entry.emission, MaterialEmission::NONE, "{}", entry.id);
        assert_eq!(entry.emission.mask, None, "{}", entry.id);
        assert!(!entry.emission.is_emissive(), "{}", entry.id);
        assert_eq!(entry.emission.effective_color(), [0.0, 0.0, 0.0]);
    }
}

#[test]
fn emissive_catalog_material_resolves_colour_intensity_and_shared_mask() {
    let catalog = emissive_catalog();
    assert_eq!(
        catalog.material_emissive("core:mat_glow_a"),
        Some([0.25, 0.5, 1.0])
    );
    assert_eq!(
        catalog.material_emissive_intensity("core:mat_glow_a"),
        Some(2.0)
    );
    assert_eq!(
        catalog.material_emissive_mask("core:mat_glow_a"),
        Some("core:tex_mask")
    );

    let level = basic_level("core:mat_glow_a", "core:mat_glow_b", "core:mat_glow_a");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    assert!(table.errors().is_empty(), "errors: {:?}", table.errors());

    let a = table.entry_of("core:mat_glow_a").expect("a");
    assert_eq!(a.emission.color, [0.25, 0.5, 1.0]);
    assert_eq!(a.emission.intensity, 2.0);
    assert_eq!(a.emission.effective_color(), [0.5, 1.0, 2.0]);
    assert!(a.emission.is_emissive());

    // Without an authored intensity the default applies.
    let b = table.entry_of("core:mat_glow_b").expect("b");
    assert_eq!(b.emission.intensity, DEFAULT_EMISSION_INTENSITY);

    let mask_index = a.emission.mask.expect("a mask index");
    assert_eq!(
        b.emission.mask,
        Some(mask_index),
        "two materials share one mask"
    );
    assert_ne!(
        mask_index, a.texture_index,
        "the mask is a separate texture"
    );
    let mask = &table.textures()[mask_index as usize];
    assert_eq!(mask.key, "core:tex_mask");
    assert_eq!(mask.origin, TextureOrigin::Catalog);
    assert!(Rc::ptr_eq(
        &mask.image,
        &cache.get("core:tex_mask").expect("the mask decoded once")
    ));

    assert_eq!(table.textures().len(), 2, "albedo plus one shared mask");
    assert_eq!(cache.decoded_count(), 2, "each distinct PNG decodes once");
}

#[test]
fn emissive_material_without_a_mask_has_no_mask_index() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_glow", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "emissive": [0.5, 0.5, 0.5] }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("catalog");
    let level = basic_level("core:mat_glow", "core:mat_glow", "core:mat_glow");

    // A logical table describes emission without decoding any image; a mask
    // index would be meaningless there, so it stays `None`.
    let logical = MaterialTable::logical(&level, &catalog, None);
    let entry = logical.entry_of("core:mat_glow").expect("logical entry");
    assert_eq!(entry.emission.color, [0.5, 0.5, 0.5]);
    assert_eq!(entry.emission.intensity, DEFAULT_EMISSION_INTENSITY);
    assert_eq!(entry.emission.mask, None);
    assert!(entry.image.is_none());

    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    let entry = table.entry_of("core:mat_glow").expect("resolved entry");
    assert_eq!(entry.emission.mask, None);
    assert_eq!(entry.emission.effective_color(), [0.5, 0.5, 0.5]);
    assert_eq!(table.textures().len(), 1, "only the albedo uploads");
}

#[test]
fn missing_emissive_mask_falls_back_to_the_diagnostic_texture() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:tex_mask", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/masks/does_not_exist.png" },
            { "id": "core:mat_glow", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core:tex_mask" }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("catalog");
    let level = basic_level("core:mat_glow", "core:mat_glow", "core:mat_glow");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

    let entry = table.entry_of("core:mat_glow").expect("entry");
    assert_eq!(entry.origin, TextureOrigin::Missing);
    assert_eq!(entry.texture_key, MISSING_TEXTURE_KEY);
    assert_eq!(
        entry.emission,
        MaterialEmission::NONE,
        "a material that cannot bind its mask must not glow"
    );
    let error = entry.error.as_deref().expect("error");
    assert!(error.contains("core:mat_glow"), "error: {error}");
    assert!(error.contains("core:tex_mask"), "error: {error}");
    assert!(error.contains("does_not_exist.png"), "error: {error}");
    assert_eq!(table.textures().len(), 1);
    assert_eq!(table.textures()[0].key, MISSING_TEXTURE_KEY);
}

#[test]
fn pack_material_emission_resolves_pack_local_and_catalog_masks() {
    let albedo = encode_png(&RawImage::new(2, 2, vec![1; 16])).expect("encode albedo");
    let mask = encode_png(&RawImage::new(1, 1, vec![10, 20, 30, 255])).expect("encode mask");
    let mut textures: HashMap<String, Rc<[u8]>> = HashMap::new();
    textures.insert("textures/wall.png".to_string(), Rc::from(albedo));
    textures.insert("textures/glow_mask.png".to_string(), Rc::from(mask));
    let json = r#"{
        "materials": {
            "pack:glow": { "texture": "textures/wall.png", "emissive": [0.5, 0.25, 0.0],
                           "emissive_intensity": 3.0,
                           "emissive_mask": "textures/glow_mask.png" },
            "pack:builtin_mask": { "texture": "textures/wall.png", "emissive": [1.0, 1.0, 1.0],
                                   "emissive_mask": "core:tex_ceiling_panel_01" },
            "pack:plain": "textures/wall.png"
        }
    }"#;
    let pack = PackMaterials::new("unit_pack", Some(json), textures);
    let level = basic_level("pack:glow", "pack:plain", "pack:builtin_mask");
    let catalog = shipped_catalog();
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, Some(&pack), Some(&root), &mut cache);
    assert!(table.errors().is_empty(), "errors: {:?}", table.errors());

    let glow = table.entry_of("pack:glow").expect("glow");
    assert_eq!(glow.emission.color, [0.5, 0.25, 0.0]);
    assert_eq!(glow.emission.intensity, 3.0);
    assert_eq!(glow.emission.effective_color(), [1.5, 0.75, 0.0]);
    let local_mask = glow.emission.mask.expect("pack-local mask");
    assert_eq!(
        table.textures()[local_mask as usize].key,
        "pack:unit_pack:textures/glow_mask.png"
    );
    assert_eq!(
        table.textures()[local_mask as usize].origin,
        TextureOrigin::Pack
    );

    let builtin = table.entry_of("pack:builtin_mask").expect("builtin mask");
    assert_eq!(builtin.emission.intensity, DEFAULT_EMISSION_INTENSITY);
    let catalog_mask = builtin.emission.mask.expect("catalog mask");
    assert_eq!(
        table.textures()[catalog_mask as usize].key,
        "core:tex_ceiling_panel_01"
    );
    assert_eq!(
        table.textures()[catalog_mask as usize].origin,
        TextureOrigin::Catalog
    );
    assert_ne!(local_mask, catalog_mask);

    let plain = table.entry_of("pack:plain").expect("plain");
    assert_eq!(
        plain.emission,
        MaterialEmission::NONE,
        "the string shorthand stays emission-free"
    );

    // The shared albedo, the pack mask and the catalog mask.
    assert_eq!(table.textures().len(), 3);
}

// ------------------------------------------------- surface response and alpha

/// A synthetic catalog exercising the surface-response and alpha fields.
///
/// Every texture path is shipped artwork, so the test needs no new asset files.
fn response_catalog() -> AssetCatalog {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:tex_normal", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "core/textures/normals/normal_panel_01.png" },
            { "id": "core:mat_plain", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo" },
            { "id": "core:mat_gloss", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.5, "roughness": 0.15,
              "normal_texture": "core:tex_normal", "normal_strength": 0.75 },
            { "id": "core:mat_metal", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.6, "specular_color": [0.5, 0.25, 0.1], "roughness": 0.3 },
            { "id": "core:mat_glass", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_normal",
              "alpha_mode": "blend", "opacity": 0.5 },
            { "id": "core:mat_grille", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_normal",
              "alpha_mode": "cutout", "alpha_cutoff": 0.25 },
            { "id": "core:mat_glow_glass", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_normal",
              "emissive": [0.4, 0.6, 1.0], "emissive_intensity": 2.0,
              "alpha_mode": "blend", "opacity": 0.8 }
        ]
    }"##;
    AssetCatalog::from_json_str(json).expect("synthetic response catalog")
}

/// Resolves one synthetic catalog against the shipped asset root.
///
/// The level references every synthetic material, so the resolved table covers
/// all of them: `defaults` carries three and the floor patches the rest.
fn resolved_response_table() -> MaterialTable {
    let level_json = r#"{
        "format_version": 1,
        "id": "response_level",
        "name": "Response Level",
        "spawn": { "x": 1.0, "z": 1.0 },
        "defaults": { "wall": "core:mat_gloss", "floor": "core:mat_plain",
                      "ceiling": "core:mat_metal" },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 2.7 } ],
        "floor_patches": [
            { "x": 0.0, "z": 0.0, "width": 1.0, "depth": 1.0,
              "material": "core:mat_glass" },
            { "x": 1.0, "z": 0.0, "width": 1.0, "depth": 1.0,
              "material": "core:mat_grille" },
            { "x": 2.0, "z": 0.0, "width": 1.0, "depth": 1.0,
              "material": "core:mat_glow_glass" }
        ]
    }"#;
    let level = serde_json::from_str::<LevelDef>(level_json).expect("level parses");
    let catalog = response_catalog();
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    resolve_materials(&level, &catalog, None, Some(&root), &mut cache)
}

#[test]
fn a_material_without_response_fields_keeps_the_default_surface_look() {
    let table = resolved_response_table();
    let plain = table.entry_of("core:mat_plain").expect("plain");
    assert_eq!(plain.response, MaterialResponse::NONE);
    assert!(!plain.response.is_active());
    assert!(!plain.response.has_normal());
    assert!(!plain.response.has_sheen());
    assert_eq!(plain.alpha, MaterialAlpha::OPAQUE);
    assert!(!plain.alpha.is_translucent());
    assert!(!plain.alpha.is_cutout());
    assert_eq!(plain.emission, MaterialEmission::NONE);
}

#[test]
fn a_normal_map_resolves_into_the_texture_table_at_its_authored_strength() {
    let table = resolved_response_table();
    let gloss = table.entry_of("core:mat_gloss").expect("gloss");
    assert!(gloss.response.is_active());
    assert!(gloss.response.has_normal());
    assert!((gloss.response.normal_strength - 0.75).abs() < f32::EPSILON);
    let normal = gloss.response.normal.expect("a normal texture index");
    assert_ne!(normal, gloss.texture_index, "the map is its own texture");
    let texture = &table.textures()[normal as usize];
    assert_eq!(texture.key, "core:tex_normal");
    assert_eq!(texture.origin, TextureOrigin::Catalog);

    // A sheen without a normal map is a sheen only.
    let metal = table.entry_of("core:mat_metal").expect("metal");
    assert!(metal.response.is_active());
    assert!(!metal.response.has_normal());
    assert!(metal.response.normal.is_none());
}

#[test]
fn specular_strength_and_colour_are_the_product_the_shader_uploads() {
    let table = resolved_response_table();
    let metal = table.entry_of("core:mat_metal").expect("metal");
    // The authored colour is scaled by the authored strength, channel-wise.
    let expected = [0.6 * 0.5, 0.6 * 0.25, 0.6 * 0.1];
    for (channel, value) in metal.response.specular.iter().zip(expected) {
        assert!((channel - value).abs() < 1.0e-6, "{channel} != {value}");
    }
    assert!((metal.response.roughness - 0.3).abs() < f32::EPSILON);

    // A scalar strength with no colour is white at that strength.
    let gloss = table.entry_of("core:mat_gloss").expect("gloss");
    assert_eq!(gloss.response.specular, [0.5; 3]);
    assert!((gloss.response.roughness - 0.15).abs() < f32::EPSILON);
}

#[test]
fn roughness_defaults_and_out_of_range_values_are_clamped_by_the_catalog() {
    // A sheen with no authored roughness keeps the documented default.
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_sheen", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo", "specular": 0.4 }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("catalog");
    let level = basic_level("core:mat_sheen", "core:mat_sheen", "core:mat_sheen");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    let sheen = table.entry_of("core:mat_sheen").expect("sheen");
    assert!((sheen.response.roughness - DEFAULT_ROUGHNESS).abs() < f32::EPSILON);
    assert!(sheen.response.has_sheen());

    // A roughness outside 0..1 is a catalog error, not a silent clamp.
    let bad = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_bad", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo", "roughness": 4.0 }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(bad).expect_err("roughness 4.0 must be rejected");
    assert!(error.contains("roughness"), "{error}");
}

// ------------------------------------------------- shine

/// A synthetic catalog exercising `shine` (and its legacy inverse
/// `roughness`). Every texture path is shipped artwork, so the test needs no
/// new asset files.
fn shine_catalog() -> AssetCatalog {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_matte", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.4, "shine": 0.0 },
            { "id": "core:mat_gloss", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.4, "shine": 1.0 },
            { "id": "core:mat_waxed", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.4, "shine": 0.5 },
            { "id": "core:mat_legacy", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.4, "roughness": 0.25 },
            { "id": "core:mat_mirror", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.9, "shine": 1.0,
              "reflection_mode": "planar", "reflection_strength": 0.9 }
        ]
    }"##;
    AssetCatalog::from_json_str(json).expect("synthetic shine catalog")
}

/// Resolves the synthetic shine catalog against the shipped asset root.
///
/// The level references every synthetic material once, so the resolved table
/// covers all of them.
fn resolved_shine_table() -> MaterialTable {
    let level = level_from(
        r##"{
            "format_version": 1, "id": "shine", "name": "Shine",
            "spawn": { "x": 0.0, "z": 0.0 },
            "defaults": { "wall": "core:mat_matte", "floor": "core:mat_gloss",
                          "ceiling": "core:mat_waxed" },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ],
            "floor_patches": [
                { "x": 0.0, "z": 0.0, "width": 1.0, "depth": 1.0,
                  "material": "core:mat_legacy" },
                { "x": 1.0, "z": 0.0, "width": 1.0, "depth": 1.0,
                  "material": "core:mat_mirror" }
            ]
        }"##,
    );
    let catalog = shine_catalog();
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    resolve_materials(&level, &catalog, None, Some(&root), &mut cache)
}

#[test]
fn shine_is_the_author_facing_spelling_and_roughness_is_its_inverse() {
    let table = resolved_shine_table();
    let matte = table.entry_of("core:mat_matte").expect("matte");
    assert!((matte.response.roughness - 1.0).abs() < f32::EPSILON);
    assert!((matte.response.shine() - 0.0).abs() < f32::EPSILON);

    let gloss = table.entry_of("core:mat_gloss").expect("gloss");
    assert!(gloss.response.roughness.abs() < f32::EPSILON);
    assert!((gloss.response.shine() - 1.0).abs() < f32::EPSILON);

    let waxed = table.entry_of("core:mat_waxed").expect("waxed");
    assert!((waxed.response.roughness - 0.5).abs() < f32::EPSILON);

    // The two conversion helpers are defined as inverses and stay that way.
    for shine in [0.0_f32, 0.05, 0.5, 0.75, 1.0] {
        assert!((shine_from_roughness(roughness_from_shine(shine)) - shine).abs() < 1.0e-6);
    }
    assert!((DEFAULT_SHINE - (1.0 - DEFAULT_ROUGHNESS)).abs() < f32::EPSILON);
}

#[test]
fn a_legacy_roughness_material_keeps_its_exact_roughness() {
    // Catalogs authored before `shine` exist must render exactly as they did:
    // `roughness` is still accepted verbatim.
    let table = resolved_shine_table();
    let legacy = table.entry_of("core:mat_legacy").expect("legacy");
    assert!((legacy.response.roughness - 0.25).abs() < f32::EPSILON);
    assert!(legacy.response.has_sheen());
}

#[test]
fn shine_outside_the_unit_range_is_a_named_catalog_error() {
    for (value, expected) in [("-0.25", "shine"), ("1.5", "shine")] {
        let json = format!(
            r##"{{
                "assets": [
                    {{ "id": "core:tex_albedo", "asset_class": "environment",
                       "asset_type": "texture", "source": "file",
                       "model": "environment/office/textures/ceilings/ceiling_panel_01.png" }},
                    {{ "id": "core:mat_bad", "asset_class": "environment",
                       "asset_type": "material", "source": "definition",
                       "texture": "core:tex_albedo", "shine": {value} }}
                ]
            }}"##
        );
        let error =
            AssetCatalog::from_json_str(&json).expect_err("an out-of-range shine must be rejected");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn authoring_both_shine_and_roughness_is_a_named_catalog_error() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_both", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "shine": 0.4, "roughness": 0.6 }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(json).expect_err("both spellings must be rejected");
    assert!(
        error.contains("shine") && error.contains("roughness"),
        "{error}"
    );
}

#[test]
fn a_shiny_material_is_not_a_mirror_without_a_reflection_mode() {
    // Shine shapes the sheen; it must never switch the reflection on. A
    // `shine: 1.0` material with no `reflection_mode` reflects nothing, and a
    // mirror keeps its planar mode even at shine 1.0.
    let table = resolved_shine_table();
    let gloss = table.entry_of("core:mat_gloss").expect("gloss");
    assert!(gloss.response.has_sheen());
    assert_eq!(gloss.reflection, MaterialReflection::NONE);
    assert!(!gloss.reflection.is_active());

    let mirror = table.entry_of("core:mat_mirror").expect("mirror");
    assert_eq!(mirror.reflection.mode, ReflectionMode::Planar);
    assert!(mirror.reflection.is_active());
    assert!(mirror.reflection.is_planar());
    assert!(
        mirror.reflection.strength > 0.5,
        "a mirror keeps its authored strength"
    );
}

#[test]
fn pack_materials_accept_shine_and_prefer_it_over_roughness() {
    // A pack may use the author-facing spelling, the legacy one, or both; the
    // author-facing one wins so an upgraded pack cannot keep the old value.
    let json = r#"{
        "materials": {
            "pack:matte": { "texture": "textures/wall.png", "specular": 0.4, "shine": 0.0 },
            "pack:legacy": { "texture": "textures/wall.png", "specular": 0.4, "roughness": 0.2 },
            "pack:both": { "texture": "textures/wall.png", "specular": 0.4,
                           "shine": 0.5, "roughness": 0.9 }
        }
    }"#;
    let definitions = parse_materials_json(Some(json));
    let response = |id: &str| definitions.get(id).expect(id).response();
    assert!((response("pack:matte").roughness - 1.0).abs() < f32::EPSILON);
    assert!((response("pack:legacy").roughness - 0.2).abs() < f32::EPSILON);
    assert!((response("pack:both").roughness - 0.5).abs() < f32::EPSILON);

    // An out-of-range pack value is discarded, exactly like every other
    // malformed pack field: the documented default stays in place.
    let json = r#"{ "materials": {
        "pack:bad": { "texture": "textures/wall.png", "specular": 0.4, "shine": 4.0 }
    } }"#;
    let definitions = parse_materials_json(Some(json));
    let bad = definitions.get("pack:bad").expect("pack:bad").response();
    assert!((bad.roughness - DEFAULT_ROUGHNESS).abs() < f32::EPSILON);
}

#[test]
fn alpha_modes_resolve_to_the_three_draw_passes() {
    let table = resolved_response_table();
    assert_eq!(
        table.entry_of("core:mat_plain").expect("plain").alpha.mode,
        AlphaMode::Opaque
    );
    let glass = table.entry_of("core:mat_glass").expect("glass");
    assert_eq!(glass.alpha.mode, AlphaMode::Blend);
    assert!((glass.alpha.opacity - 0.5).abs() < f32::EPSILON);
    assert!(glass.alpha.is_translucent());
    let grille = table.entry_of("core:mat_grille").expect("grille");
    assert_eq!(grille.alpha.mode, AlphaMode::Cutout);
    assert!(grille.alpha.is_cutout());
    assert!((grille.alpha.cutoff - 0.25).abs() < f32::EPSILON);
}

#[test]
fn a_translucent_emissive_material_keeps_both_properties() {
    let table = resolved_response_table();
    let sign = table.entry_of("core:mat_glow_glass").expect("sign");
    assert!(sign.emission.is_emissive());
    assert_eq!(sign.emission.effective_color(), [0.8, 1.2, 2.0]);
    assert!(sign.alpha.is_translucent());
    // Neither property cancels the other: emission is additive on top of the
    // lit term and alpha only decides how the result blends.
    assert_eq!(sign.response, MaterialResponse::NONE);
}

#[test]
fn an_opacity_or_cutoff_without_a_mode_is_a_catalog_error() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_bad", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo", "opacity": 0.4 }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(json).expect_err("opacity needs a mode");
    assert!(error.contains("alpha_mode"), "{error}");
}

#[test]
fn an_unknown_alpha_mode_is_rejected_by_name() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_bad", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "alpha_mode": "translucent" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(json).expect_err("unknown mode");
    assert!(error.contains("translucent"), "{error}");
}

#[test]
fn a_normal_map_that_cannot_resolve_degrades_the_whole_material() {
    let json = r##"{
        "assets": [
            { "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
              "source": "file",
              "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
            { "id": "core:mat_broken", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_albedo",
              "specular": 0.5, "roughness": 0.2,
              "normal_texture": "core:tex_nowhere" }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("catalog parses; the reference dangles");
    let level = basic_level("core:mat_broken", "core:mat_broken", "core:mat_broken");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    let broken = table.entry_of("core:mat_broken").expect("broken");
    assert_eq!(broken.origin, TextureOrigin::Missing);
    assert!(broken.response.normal.is_none());
    assert_eq!(broken.response, MaterialResponse::NONE);
    assert!(
        broken
            .error
            .as_deref()
            .is_some_and(|error| error.contains("core:tex_nowhere")),
        "the error names the missing texture: {:?}",
        broken.error
    );
}

#[test]
fn pack_materials_may_author_response_and_alpha_fields() {
    let albedo = encode_png(&RawImage::new(2, 2, vec![9; 16])).expect("encode albedo");
    let normal = encode_png(&RawImage::new(1, 1, vec![128, 128, 255, 255])).expect("encode normal");
    let mut textures: HashMap<String, Rc<[u8]>> = HashMap::new();
    textures.insert("textures/wall.png".to_string(), Rc::from(albedo));
    textures.insert("textures/bump.png".to_string(), Rc::from(normal));
    let json = r#"{
        "materials": {
            "pack:glass": { "texture": "textures/wall.png", "alpha_mode": "blend",
                            "opacity": 0.35, "specular": 0.4, "roughness": 0.2,
                            "normal_texture": "textures/bump.png", "normal_strength": 1.5 }
        }
    }"#;
    let pack = PackMaterials::new("response_pack", Some(json), textures);
    let catalog = shipped_catalog();
    let level = basic_level("pack:glass", "pack:glass", "pack:glass");
    let mut cache = TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let table = resolve_materials(&level, &catalog, Some(&pack), Some(&root), &mut cache);
    assert!(table.errors().is_empty(), "errors: {:?}", table.errors());

    let glass = table.entry_of("pack:glass").expect("glass");
    assert!(glass.alpha.is_translucent());
    assert!((glass.alpha.opacity - 0.35).abs() < f32::EPSILON);
    assert!(glass.response.has_sheen());
    let normal = glass.response.normal.expect("pack normal index");
    assert_eq!(
        table.textures()[normal as usize].key,
        "pack:response_pack:textures/bump.png"
    );
    assert_eq!(
        table.textures()[normal as usize].origin,
        TextureOrigin::Pack
    );
}

#[test]
fn opening_glass_materials_are_referenced_by_the_material_scan() {
    let level_json = r#"{
        "format_version": 1,
        "id": "glass_scan",
        "name": "Glass Scan",
        "spawn": { "x": 1.0, "z": 1.0 },
        "defaults": { "wall": "core:wallpaper_yellow_01", "floor": "core:carpet_beige_01",
                      "ceiling": "core:ceiling_panel_01" },
        "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 2.7 } ],
        "walls": [
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.3,
              "openings": [ { "kind": "window", "offset": 1.0, "width": 1.0,
                              "height": 1.0, "sill": 1.2,
                              "glass": "core:glass_window_clear_01" } ] }
        ]
    }"#;
    let level = serde_json::from_str::<LevelDef>(level_json).expect("level parses");
    let ids = referenced_material_ids(&level);
    assert!(
        ids.iter().any(|id| id == "core:glass_window_clear_01"),
        "the pane material must be part of the level's material set: {ids:?}"
    );
}

#[test]
fn the_shipped_glass_and_response_materials_resolve_with_their_pngs() {
    let catalog = shipped_catalog();
    for (material, mode, sheen, authors_mode) in [
        ("core:glass_window_clear_01", AlphaMode::Blend, true, true),
        ("core:glass_window_dirty_01", AlphaMode::Blend, true, true),
        ("core:glass_tinted_01", AlphaMode::Blend, true, true),
        ("core:glass_sign_lit_01", AlphaMode::Blend, true, true),
        ("core:linoleum_polished_01", AlphaMode::Opaque, true, false),
        ("core:metal_brushed_01", AlphaMode::Opaque, true, false),
        ("core:plastic_panel_01", AlphaMode::Opaque, true, false),
        ("core:pool_deck_wet_01", AlphaMode::Opaque, true, false),
        ("core:painting_dull_01", AlphaMode::Opaque, false, false),
    ] {
        let entry = catalog.material(material).unwrap_or_else(|| {
            panic!("{material} must be a shipped material");
        });
        // Only a translucent shipped material has to author the mode: an opaque
        // one that authors nothing keeps the legacy opaque default.
        if authors_mode {
            assert_eq!(entry.alpha_mode.as_deref(), Some(mode.name()), "{material}");
        }
        let level = basic_level(material, material, material);
        let mut cache = TextureCache::new();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
        assert!(
            table.errors().is_empty(),
            "{material}: {:?}",
            table.errors()
        );
        let resolved = table.entry_of(material).expect("resolved");
        assert_eq!(resolved.alpha.mode, mode, "{material}");
        assert_eq!(resolved.response.has_sheen(), sheen, "{material}");
        if material == "core:metal_brushed_01" || material == "core:plastic_panel_01" {
            assert!(
                resolved.response.has_normal(),
                "{material} ships a normal map"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Run 06: pool tile sheen
// ---------------------------------------------------------------------------

/// The pool tile materials carry the raised restrained sheen: a specular
/// strength at the top of the glazed-tile band and a shine that leaves the
/// surface glossy but never mirror-like, with the texture art and tiling
/// untouched.
#[test]
fn pool_tiles_keep_a_restrained_visible_sheen() {
    let level = level_from(
        r#"{
            "format_version": 1,
            "id": "pool_sheen",
            "name": "Pool Sheen",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                  "material": "core:pool_tile_deck_01",
                  "ceiling_material": "core:pool_ceiling_01" }
            ],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0,
                  "material": "core:pool_tile_basin_01",
                  "edge_material": "core:pool_tile_wall_01" }
            ],
            "walls": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3,
                  "material": "core:pool_tile_wall_01" }
            ]
        }"#,
    );
    let materials = crate::render::logical_materials(&level);
    for (id, tile) in [
        ("core:pool_tile_deck_01", 1.5),
        ("core:pool_tile_basin_01", 1.0),
        ("core:pool_tile_wall_01", 1.0),
    ] {
        let entry = materials.entry_of(id).expect(id);
        assert!(
            entry.response.specular[0] >= 0.28 && entry.response.specular[0] <= 0.32,
            "{id} specular {} must stay in the restrained glazed-tile band",
            entry.response.specular[0]
        );
        assert!(
            (entry.response.roughness - 0.6).abs() < 1.0e-3,
            "{id} roughness {} must match shine 0.4",
            entry.response.roughness
        );
        assert!(
            entry.response.roughness > 0.0,
            "{id} must never become a mirror through shine alone"
        );
        assert_eq!(entry.tile_metres, tile, "{id} tiling unchanged");
        assert!(
            entry.reflection.mode == crate::materials::MaterialReflection::NONE.mode,
            "{id} keeps no reflection mode"
        );
    }
}

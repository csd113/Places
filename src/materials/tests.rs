//! Unit tests for the material pipeline.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
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
                                "offset_y": -0.5, "material": "region-floor", "edge_material": "region-edge" }]
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

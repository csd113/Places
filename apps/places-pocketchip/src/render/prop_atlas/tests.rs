//! Unit tests for the prop albedo atlas.
//!
//! They pin the layout arithmetic (cell origins, gutter replication, the mip
//! cap), the UV remap, the fallback rules and the end-to-end effect on a real
//! level build.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else in the
// crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use super::*;
use crate::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::materials::MaterialEmission;

/// One RGBA8 texel of a placed atlas, for direct pixel assertions.
fn texel(atlas: &PropAtlas, x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize * ATLAS_EDGE as usize + x as usize) * 4;
    let slice = atlas
        .pixels()
        .get(offset..offset + 4)
        .expect("the texel lies inside the atlas");
    <[u8; 4]>::try_from(slice).expect("a texel is four channels")
}

/// Whether two placements' content rectangles overlap on both axes.
fn overlaps(a: &AtlasPlacement, b: &AtlasPlacement) -> bool {
    let u = a.origin_x < b.origin_x + b.width && b.origin_x < a.origin_x + a.width;
    let v = a.origin_y < b.origin_y + b.height && b.origin_y < a.origin_y + a.height;
    u && v
}

/// A solid image of the given size and colour.
fn solid(width: u32, height: u32, colour: [u8; 4]) -> RawImage {
    RawImage::new(
        width,
        height,
        colour
            .iter()
            .copied()
            .cycle()
            .take((width as usize) * (height as usize) * 4)
            .collect(),
    )
}

#[test]
fn the_atlas_layout_numbers_fit_the_sheet() {
    assert_eq!(ATLAS_EDGE, 1024);
    assert_eq!(CELL_STRIDE, 144);
    assert_eq!(GUTTER, 8);
    assert_eq!(CONTENT_EDGE, 128);
    // A cell is the content plus one gutter on each side...
    assert_eq!(CELL_STRIDE, CONTENT_EDGE + 2 * GUTTER);
    // ...and 7 x 7 of them fit inside 1008 <= 1024.
    assert_eq!(CELL_COUNT, 49);
    const { assert!(CELL_COLUMNS * CELL_STRIDE <= ATLAS_EDGE) };
    assert_eq!(
        (CELL_COLUMNS - 1) * CELL_STRIDE + GUTTER + CONTENT_EDGE,
        1000,
        "the last cell's far gutter edge stays inside the sheet"
    );
}

#[test]
fn remap_puts_the_unit_square_inside_the_cells_content_rect() {
    let mut atlas = PropAtlas::new();
    let placement = atlas
        .place(&solid(128, 128, [1, 2, 3, 255]))
        .expect("a 128x128 image fits the first cell");

    // Cell 0's content starts one gutter inside the sheet.
    assert_eq!((placement.origin_x, placement.origin_y), (GUTTER, GUTTER));
    assert_eq!((placement.width, placement.height), (128, 128));
    let scale = 1.0 / 1024.0;
    assert_eq!(placement.remap_uv([0.0, 0.0]), [8.0 * scale, 8.0 * scale]);
    assert_eq!(
        placement.remap_uv([1.0, 1.0]),
        [136.0 * scale, 136.0 * scale],
        "(1,1) is the content rect's far corner, not the gutter"
    );
    // Out-of-range UVs clamp to the same place as the edge: the model's own
    // texture was CLAMP_TO_EDGE, so an overshoot must not reach a neighbour.
    assert_eq!(
        placement.remap_uv([-0.5, 1.5]),
        placement.remap_uv([0.0, 1.0])
    );
    assert_eq!(placement.remap_uv([0.5, 0.5]), [72.0 * scale, 72.0 * scale]);
}

#[test]
fn a_smaller_image_remaps_by_its_real_size_not_the_content_edge() {
    let mut atlas = PropAtlas::new();
    let placement = atlas
        .place(&solid(64, 64, [9, 9, 9, 255]))
        .expect("a 64x64 image fits");
    assert_eq!((placement.width, placement.height), (64, 64));
    let scale = 1.0 / 1024.0;
    assert_eq!(
        placement.remap_uv([1.0, 1.0]),
        [72.0 * scale, 72.0 * scale],
        "the image keeps its own size; the copy is 1:1 at the cell's top-left"
    );
}

#[test]
fn different_cells_never_produce_overlapping_uv_ranges() {
    let mut atlas = PropAtlas::new();
    let image = solid(4, 4, [255, 255, 255, 255]);
    let mut placements = Vec::new();
    for _ in 0..CELL_COUNT {
        placements.push(atlas.place(&image).expect("every cell is free"));
    }
    assert_eq!(atlas.cells_used(), CELL_COUNT);

    // Cells advance along a row first and then wrap to the next row.
    assert_eq!(placements[1].origin_x, CELL_STRIDE + GUTTER);
    assert_eq!(placements[1].origin_y, GUTTER);
    assert_eq!(placements[CELL_COLUMNS as usize].origin_x, GUTTER);
    assert_eq!(
        placements[CELL_COLUMNS as usize].origin_y,
        CELL_STRIDE + GUTTER
    );

    for (index, a) in placements.iter().enumerate() {
        for b in placements.iter().skip(index + 1) {
            assert!(!overlaps(a, b), "cells {a:?} and {b:?} share atlas space");
        }
    }
}

#[test]
fn the_gutter_replicates_the_nearest_content_edge_texel() {
    // A 2x2 image with four distinct colours; every texel outside the image
    // must be the nearest of the four, so a bilinear or mip sample that strays
    // out of the cell reads this cell's own edge instead of a neighbour's.
    let pixels = vec![
        10, 20, 30, 255, 40, 50, 60, 255, // row 0
        70, 80, 90, 255, 100, 110, 120, 255, // row 1
    ];
    let source = RawImage::new(2, 2, pixels.clone());
    let mut atlas = PropAtlas::new();
    let placement = atlas.place(&source).expect("a 2x2 image fits");
    assert_eq!((placement.origin_x, placement.origin_y), (GUTTER, GUTTER));

    let source_texel = |x: u32, y: u32| -> [u8; 4] {
        let offset = (y as usize * 2 + x as usize) * 4;
        <[u8; 4]>::try_from(&pixels[offset..offset + 4]).expect("four channels")
    };
    for y in 0..CELL_STRIDE {
        for x in 0..CELL_STRIDE {
            let source_x = (x.saturating_sub(placement.origin_x)).min(placement.width - 1);
            let source_y = (y.saturating_sub(placement.origin_y)).min(placement.height - 1);
            assert_eq!(
                texel(&atlas, x, y),
                source_texel(source_x, source_y),
                "cell texel ({x}, {y}) must replicate its nearest content edge"
            );
        }
    }
}

#[test]
fn the_deepest_mip_level_keeps_a_one_texel_gutter() {
    // The gutter at level L is GUTTER / 2^L, so level 3 is the deepest level
    // where it is still at least one texel. Documented in the module header.
    assert_eq!(MAX_MIP_LEVEL, 3);
    let gutter_at = |level: u32| GUTTER >> level;
    assert!(gutter_at(MAX_MIP_LEVEL) >= 1);
    assert_eq!(gutter_at(MAX_MIP_LEVEL + 1), 0);
    assert_eq!(
        CELL_STRIDE >> MAX_MIP_LEVEL,
        (CONTENT_EDGE >> MAX_MIP_LEVEL) + 2 * gutter_at(MAX_MIP_LEVEL),
        "the deepest mip still separates neighbouring content by its gutter"
    );
}

/// A one-triangle model whose single submesh carries `emission` and `texture`.
fn single_quad_model(texture: Option<u16>, emission: MaterialEmission) -> PropModel {
    PropModel {
        vertices: vec![
            PropVertex {
                pos: [0.0, 0.0, 0.0],
                color: [1.0; 4],
                uv: [0.0, 0.0],
            },
            PropVertex {
                pos: [1.0, 0.0, 0.0],
                color: [1.0; 4],
                uv: [1.0, 0.0],
            },
            PropVertex {
                pos: [1.0, 0.0, 1.0],
                color: [1.0; 4],
                uv: [1.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2],
        textures: vec![solid(2, 2, [7, 7, 7, 255])],
        submeshes: vec![PropSubmesh {
            material: 0,
            texture,
            emission,
            double_sided: false,
            first_index: 0,
            index_count: 3,
        }],
        triangles: 1,
        materials: 1,
    }
}

#[test]
fn albedo_slot_accepts_one_unmasked_texture_and_rejects_the_rest() {
    let plain = single_quad_model(Some(0), MaterialEmission::NONE);
    assert_eq!(albedo_slot(&plain), Some(0));

    // An emissive mask is a second sampler per draw; the model keeps its own
    // texture path.
    let masked = single_quad_model(
        Some(0),
        MaterialEmission::new([1.0, 0.0, 0.0], 1.0).with_mask(Some(0)),
    );
    assert_eq!(albedo_slot(&masked), None);

    // Unmasked emission is a uniform, so it stays atlasable.
    let glowing = single_quad_model(Some(0), MaterialEmission::new([0.5, 0.5, 0.5], 2.0));
    assert_eq!(albedo_slot(&glowing), Some(0));

    // A submesh that samples nothing draws through the white sheet, which the
    // atlas must not silently replace.
    let untextured = single_quad_model(None, MaterialEmission::NONE);
    assert_eq!(albedo_slot(&untextured), None);
}

#[test]
fn a_full_atlas_refuses_more_models_without_touching_its_pixels() {
    let mut atlas = PropAtlas::new();
    let image = solid(2, 2, [3, 4, 5, 255]);
    for _ in 0..CELL_COUNT {
        atlas.place(&image).expect("cells remain");
    }
    let before = atlas.pixels().to_vec();
    assert!(atlas.place(&image).is_none(), "cell 50 has nowhere to go");
    assert_eq!(atlas.cells_used(), CELL_COUNT);
    assert_eq!(atlas.pixels(), before, "a refused model changes nothing");
}

/// A minimal 10x10 room, matching the render tests' own fixture so the level
/// builder path is exercised for real.
fn level_with_prop(model: &str) -> crate::level::LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "atlas_test",
            "name": "Atlas Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "room": {{ "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
            "props": [{{ "model": "{model}", "x": 0.0, "z": 0.0 }}]
        }}"#
    );
    crate::level::LevelDef::from_json(&json).expect("valid level json")
}

#[test]
fn a_shipped_single_albedo_model_is_atlased_end_to_end() {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    if assets.root().is_none() {
        return;
    }
    let level = level_with_prop("core:chair");
    let (_, batches) =
        crate::render::build_level_geometry_with_assets(&level, &catalog, &mut assets);
    let batch = batches.first().expect("the chair batches");
    let atlas = batch.atlas.as_ref().expect("the chair is atlased");
    assert_eq!(atlas.cells_used(), 1);
    assert_eq!(
        atlas.pixels().len(),
        ATLAS_EDGE as usize * ATLAS_EDGE as usize * 4
    );
    let placement = atlas.placements().first().copied().expect("one placement");

    // Every remapped UV must lie inside the chair's content rect: the atlas is
    // addressed exactly like the model's own clamped texture used to be.
    let u0 = placement.remap_uv([0.0, 0.0]);
    let u1 = placement.remap_uv([1.0, 1.0]);
    for vertex in &batch.vertices {
        assert!(
            (u0[0]..=u1[0]).contains(&vertex.uv[0]) && (u0[1]..=u1[1]).contains(&vertex.uv[1]),
            "UV {:?} escaped the chair's cell {:?}..{:?}",
            vertex.uv,
            u0,
            u1
        );
    }
    // The model still draws its whole vertex list (geometry unchanged).
    let asset = assets
        .resolve("environment/office/props/models/chair.glb")
        .expect("the chair asset is cached");
    assert_eq!(batch.vertices.len(), asset.model.vertices.len());
}

#[test]
fn the_shipped_demo_atlases_every_prop_model() {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    if assets.root().is_none() {
        return;
    }
    let json = include_str!("../../../assets/levels/places_demo.json");
    let level = crate::level::LevelDef::from_json(json).expect("places_demo parses");
    let (_, batches) =
        crate::render::build_level_geometry_with_assets(&level, &catalog, &mut assets);
    assert!(!batches.is_empty(), "the demo places props");

    let mut models: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let fell_back: Vec<&str> = batches
        .iter()
        .filter(|batch| batch.atlas.is_none())
        .map(|batch| batch.model.as_str())
        .collect();
    for batch in &batches {
        models.insert(batch.model.as_str());
    }
    assert!(
        fell_back.is_empty(),
        "every shipped model must atlas: {fell_back:?}"
    );
    assert!(
        models.len() <= CELL_COUNT,
        "the demo's {} distinct models exceed the {CELL_COUNT}-cell atlas",
        models.len()
    );
}

#[test]
fn the_shipped_demo_props_fit_one_index_chunk() {
    // The office benchmark viewpoint draws its props in a single submission:
    // every model is atlased, every material is non-emissive, and the whole
    // prop pass fits one 16-bit index chunk, which is exactly the condition
    // `merge_atlas_prop_draws` collapses on.
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    if assets.root().is_none() {
        return;
    }
    let json = include_str!("../../../assets/levels/places_demo.json");
    let level = crate::level::LevelDef::from_json(json).expect("places_demo parses");
    let (_, batches) =
        crate::render::build_level_geometry_with_assets(&level, &catalog, &mut assets);

    let mut packer = crate::render::MeshPacker::default();
    for batch in &batches {
        assert!(batch.atlas.is_some(), "{} is not atlased", batch.model);
        for submesh in &batch.submeshes {
            assert!(
                !submesh.emission.is_emissive(),
                "{} is emissive; the merge keeps one emission per draw",
                batch.model
            );
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let Some(indices) = batch.indices.get(start..start.saturating_add(count)) else {
                continue;
            };
            packer.push(&batch.vertices, indices);
        }
    }
    assert_eq!(
        packer.chunks.len(),
        1,
        "the demo's props must fit one chunk for the single-draw claim"
    );
}

#[test]
fn a_masked_model_falls_back_with_its_authored_uvs_untouched() {
    use crate::test_support::{TestGlbMaterial, two_material_glb};

    let directory = std::path::PathBuf::from("target/agent-work/tests/prop_atlas_masked");
    std::fs::create_dir_all(&directory).expect("scratch directory is writable");
    let model_path = "masked.glb";
    let glb = two_material_glb(
        TestGlbMaterial::plain([200, 40, 40, 255]),
        TestGlbMaterial::emissive([20, 60, 220, 255], [0.25, 0.5, 1.0], true),
    );
    std::fs::write(directory.join(model_path), glb).expect("scratch GLB is writable");

    let catalog = crate::loader::PropCatalog::from_json_str(&format!(
        r##"{{
            "format_version": 1,
            "props": [{{
                "id": "core:test_masked", "name": "Test Masked", "category": "Decorative",
                "model": "{model_path}", "size": [1.0, 0.2, 2.0],
                "color": "#808080", "solid": false
            }}]
        }}"##
    ))
    .expect("the test catalog parses");
    let mut assets = crate::props::PropAssets::with_root(&directory);
    let level = level_with_prop("core:test_masked");
    let (_, batches) =
        crate::render::build_level_geometry_with_assets(&level, &catalog, &mut assets);
    let batch = batches.first().expect("the masked model batches");
    assert!(
        batch.atlas.is_none(),
        "a model whose emission carries a mask must keep its per-model texture path"
    );

    // The fallback leaves geometry exactly as authored: the vertices are the
    // model's own, UV for UV, so the per-model texture still samples right.
    let asset = assets.resolve(model_path).expect("the masked asset loads");
    assert_eq!(batch.vertices.len(), asset.model.vertices.len());
    for (vertex, source) in batch.vertices.iter().zip(&asset.model.vertices) {
        assert_eq!(vertex.uv, source.uv);
        assert_eq!(vertex.pos, source.pos);
    }
}

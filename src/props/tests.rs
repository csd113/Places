//! Unit tests for the prop asset pipeline.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use super::*;
use crate::gltf::{PropSubmesh, PropVertex};
use crate::level::{
    MAX_PROP_TEXTURE_BYTES, MAX_PROP_TEXTURE_SIZE, MAX_PROP_TRIANGLES, MAX_PROP_VERTICES,
    PROP_TEXTURE_NATIVE_SIZE, PROP_TEXTURE_PACK_BUDGET_BYTES, PROP_TRIANGLE_BUDGET,
    PROP_TRIANGLE_REVIEW, PROP_TRIANGLE_TARGET,
};
use crate::loader::{PropCatalog, RawImage};
use crate::materials::MaterialEmission;

/// Catalogue + shipped GLB validation, the automated half of the asset
/// budgets in `assets/README.md`. Every failure message names the prop
/// and the concrete rule so the fix is obvious.
#[test]
fn shipped_prop_assets_match_the_catalogue_and_budgets() {
    let catalog = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    assert!(
        assets.root().is_some(),
        "asset directory not found; expected assets/ next to the crate"
    );

    let entries = catalog.entries();
    assert!(
        !entries.is_empty(),
        "the asset catalogue assets/catalog.json is empty"
    );
    // The pack scope is the catalogue itself, so adding a themed prop
    // family never needs this test edited; the lower bound keeps a
    // truncated catalogue from passing silently.
    assert!(
        entries.len() >= 30,
        "the pack is the core/office set plus the Pool family (at least 30 props); \
         the catalogue now lists {}",
        entries.len()
    );

    let mut ids = std::collections::HashSet::new();
    let mut models = std::collections::HashSet::new();
    let mut total_triangles = 0usize;
    let mut total_texture_bytes = 0usize;

    for entry in &entries {
        assert!(
            ids.insert(entry.id.clone()),
            "duplicate prop id {} in the catalogue",
            entry.id
        );
        let Some(model_path) = entry.model.as_deref() else {
            panic!(
                "{}: catalogue entry declares no model; every core prop must ship a GLB",
                entry.id
            );
        };
        assert!(
            models.insert(model_path.to_string()),
            "{}: model path {model_path} is used by more than one prop",
            entry.id
        );

        let asset = assets
            .resolve(model_path)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.id));
        let model = &asset.model;

        assert!(model.triangles > 0, "{}: model has no triangles", entry.id);
        assert!(
            model.triangles <= MAX_PROP_TRIANGLES,
            "{}: model has {} triangles; the engine hard ceiling is {MAX_PROP_TRIANGLES}",
            entry.id,
            model.triangles
        );
        assert!(
            model.vertices.len() <= MAX_PROP_VERTICES,
            "{}: model has {} vertices; the limit is {MAX_PROP_VERTICES}",
            entry.id,
            model.vertices.len()
        );
        assert!(
            model.texture_count() >= 1,
            "{}: every shipped prop embeds at least one texture",
            entry.id
        );
        for texture in &model.textures {
            assert!(
                texture.width <= MAX_PROP_TEXTURE_SIZE && texture.height <= MAX_PROP_TEXTURE_SIZE,
                "{}: texture is {}x{}; the engine texture limit is {MAX_PROP_TEXTURE_SIZE}x{MAX_PROP_TEXTURE_SIZE}",
                entry.id,
                texture.width,
                texture.height
            );
            assert!(
                crate::assets::decoded_rgba_bytes(texture.width, texture.height)
                    <= MAX_PROP_TEXTURE_BYTES,
                "{}: texture is {}x{}, over the {MAX_PROP_TEXTURE_BYTES}-byte decoded engine ceiling",
                entry.id,
                texture.width,
                texture.height
            );
            assert!(
                texture.rgba.len() == (texture.width * texture.height * 4) as usize,
                "{}: decoded texture buffer does not match its dimensions",
                entry.id
            );
        }
        // Art budget, not engine ceiling: shipped assets must stay inside the
        // numbers `assets/README.md` and `tools/props/build.py` enforce.
        assert!(
            model.triangles <= PROP_TRIANGLE_BUDGET,
            "{}: model has {} triangles, above the {PROP_TRIANGLE_BUDGET}-triangle art budget",
            entry.id,
            model.triangles
        );
        for texture in &model.textures {
            assert!(
                texture.width <= PROP_TEXTURE_NATIVE_SIZE
                    && texture.height <= PROP_TEXTURE_NATIVE_SIZE,
                "{}: texture is {}x{}, above the {PROP_TEXTURE_NATIVE_SIZE}px native prop size",
                entry.id,
                texture.width,
                texture.height
            );
        }
        // Every model is drawable: at least one submesh, each range inside the
        // index buffer, each material a real index.
        assert!(
            !model.submeshes.is_empty(),
            "{}: model declares no draw ranges",
            entry.id
        );
        for submesh in &model.submeshes {
            assert!(
                submesh.index_count > 0 && submesh.index_count % 3 == 0,
                "{}: submesh has {} indices; triangle lists need a positive multiple of three",
                entry.id,
                submesh.index_count
            );
            assert!(
                usize::from(submesh.material) < model.materials,
                "{}: submesh references material {} of {} declared",
                entry.id,
                submesh.material,
                model.materials
            );
            let first = usize::try_from(submesh.first_index).expect("first index fits");
            let count = usize::try_from(submesh.index_count).expect("index count fits");
            assert!(
                first + count <= model.indices.len(),
                "{}: submesh range {first}..{} exceeds the {} index buffer",
                entry.id,
                first + count,
                model.indices.len()
            );
        }

        // Scale/origin conventions: 1 unit = 1 metre, base at y = 0, centred.
        let (low, high) = model.bounds().expect("model has vertices");
        let dimensions = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
        for axis in 0..3 {
            let expected = entry.size[axis];
            let tolerance = (expected * 0.06).max(0.02);
            assert!(
                (dimensions[axis] - expected).abs() <= tolerance,
                "{}: model {} extent is {:.3} m but the catalogue says {:.3} m \
                 (tolerance {:.3} m); fix the model or the catalogue entry",
                entry.id,
                ["width", "height", "depth"][axis],
                dimensions[axis],
                expected,
                tolerance
            );
        }
        assert!(
            low[1].abs() <= 0.012,
            "{}: model base sits at y={:.3}; props must rest on y=0",
            entry.id,
            low[1]
        );
        let center_x = f32::midpoint(low[0], high[0]);
        let center_z = f32::midpoint(low[2], high[2]);
        assert!(
            center_x.abs() <= 0.02 && center_z.abs() <= 0.02,
            "{}: model is not horizontally centred (x={center_x:.3}, z={center_z:.3})",
            entry.id
        );

        total_triangles += model.triangles;
        total_texture_bytes += texture_bytes(model);
    }

    // Props that legitimately need more silhouette detail than a box-shaped
    // object are allowlisted explicitly instead of raising the global limit.
    // spooner-man is a tuxedo cat: a recognisable creature needs a head,
    // legs, a tail and readable markings, so he sits above the 800-triangle
    // review threshold and well under the 1500 hard ceiling. The run-04
    // entities are the same case: a readable human mannequin, a rat with
    // four articulated legs and a tail, and an articulated skeleton all need
    // anatomy no box-shaped prop has (1130 / 856 / 1352 triangles, all under
    // the 1500 shipped-art ceiling).
    let detailed_props = ["spooner-man", "mannequin", "rat", "skeleton"];
    let over_review: Vec<(String, usize)> = entries
        .iter()
        .filter_map(|entry| entry.model.as_deref().map(|path| (entry.id.clone(), path)))
        .filter_map(|(id, path)| {
            let triangles = assets.resolve(path).ok()?.model.triangles;
            (triangles > PROP_TRIANGLE_REVIEW).then_some((id, triangles))
        })
        .collect();
    for (id, triangles) in &over_review {
        assert!(
            detailed_props.contains(&id.as_str()),
            "{id} uses {triangles} triangles, above the {PROP_TRIANGLE_REVIEW}-triangle review \
             threshold, and is not in the documented allowlist"
        );
        println!("justified above-review prop: {id} ({triangles} triangles)");
    }

    // Whole-pack budget: the pack must stay small enough for one handheld.
    let review_budget = entries.len() * PROP_TRIANGLE_REVIEW;
    assert!(
        total_triangles <= review_budget,
        "the pack totals {total_triangles} triangles across {} props; investigate the outliers",
        entries.len()
    );
    // Whole-pack decoded-memory budget: a desktop-scale cap that holds 256
    // native 256x256 sheets, so the shipped pack's texture quality is never a
    // function of how many props happen to be catalogued together.
    assert!(
        !pack_texture_budget_exceeded(total_texture_bytes),
        "decoded prop textures total {total_texture_bytes} bytes, over the \
         {PROP_TEXTURE_PACK_BUDGET_BYTES}-byte desktop pack budget"
    );

    let stats = assets.stats();
    assert_eq!(stats.models_failed, 0, "some models failed to load");
    assert_eq!(
        stats.models_loaded,
        entries.len(),
        "expected every catalogued prop model to load"
    );

    // Report the pack's cost so `cargo test -- --nocapture` doubles as the
    // budget report developers paste into reviews.
    println!(
        "prop pack: {} props, {} triangles, {} bytes of decoded texture, {} models cached",
        entries.len(),
        total_triangles,
        total_texture_bytes,
        stats.models_loaded
    );
    println!(
        "budget: target {PROP_TRIANGLE_TARGET} triangles/prop, review above {PROP_TRIANGLE_REVIEW}, \
         hard ceiling {MAX_PROP_TRIANGLES}; native texture {PROP_TEXTURE_NATIVE_SIZE}px \
         (engine max {MAX_PROP_TEXTURE_SIZE}px, pack budget {PROP_TEXTURE_PACK_BUDGET_BYTES} bytes)"
    );
}

#[test]
fn a_missing_model_is_reported_once_and_never_retried() {
    let mut assets = PropAssets::with_root("target/definitely-not-here");
    let first = assets.resolve("environment/office/props/models/chair.glb");
    let second = assets.resolve("environment/office/props/models/chair.glb");
    assert!(first.is_err() && second.is_err());
    assert_eq!(first.unwrap_err(), second.unwrap_err());
    let stats = assets.stats();
    assert_eq!(stats.models_failed, 1);
    assert_eq!(stats.models_loaded, 0);
}

#[test]
fn assets_are_shared_between_instances() {
    let mut assets = PropAssets::load_default();
    let first = assets
        .resolve("environment/office/props/models/chair.glb")
        .expect("chair loads");
    let second = assets
        .resolve("environment/office/props/models/chair.glb")
        .expect("chair loads");
    assert!(
        Arc::ptr_eq(&first, &second),
        "identical models must share one decoded copy"
    );
    assert_eq!(assets.stats().models_loaded, 1);
}

/// RGBA bytes of a `width x height` 8-bit image, for fixture buffers.
fn rgba_bytes(width: u32, height: u32) -> usize {
    usize::try_from(width)
        .expect("fixture width fits")
        .saturating_mul(usize::try_from(height).expect("fixture height fits"))
        .saturating_mul(4)
}

/// A synthetic model with the given triangle count and texture dimensions,
/// for the art-budget predicate tests below.
fn synthetic_model(triangles: usize, textures: &[(u32, u32)]) -> PropModel {
    PropModel {
        vertices: vec![PropVertex {
            pos: [0.0; 3],
            color: [1.0; 4],
            uv: [0.0; 2],
        }],
        indices: vec![0u16; triangles.saturating_mul(3)],
        textures: textures
            .iter()
            .map(|&(width, height)| {
                RawImage::new(width, height, vec![0u8; rgba_bytes(width, height)])
            })
            .collect(),
        submeshes: vec![PropSubmesh {
            material: 0,
            texture: (!textures.is_empty()).then_some(0u16),
            emission: MaterialEmission::NONE,
            first_index: 0,
            index_count: u32::try_from(triangles.saturating_mul(3))
                .expect("fixture index count fits"),
        }],
        triangles,
        materials: 1,
        ..PropModel::default()
    }
}

#[test]
fn texture_bytes_sums_every_texture_in_the_model() {
    let model = synthetic_model(2, &[(4, 4), (8, 8)]);
    assert_eq!(texture_bytes(&model), 4 * 4 * 4 + 8 * 8 * 4);
    assert_eq!(texture_bytes(&synthetic_model(1, &[])), 0);
}

/// 256x256 is the normal native prop texture, not an over-budget asset.
#[test]
fn a_native_256_prop_texture_is_unremarkable() {
    assert_eq!(
        art_budget_warning(&synthetic_model(100, &[(256, 256)])),
        None,
        "a native 256px prop texture must not warn"
    );
    assert_eq!(
        texture_bytes(&synthetic_model(100, &[(256, 256)])),
        256 * 256 * 4,
        "a native sheet decodes to exactly its RGBA8 buffer"
    );
}

#[test]
fn art_budget_warnings_name_the_broken_budget_and_never_fail_a_load() {
    assert_eq!(
        art_budget_warning(&synthetic_model(PROP_TRIANGLE_BUDGET, &[(128, 128)])),
        None,
        "a model inside the art budget warns about nothing"
    );
    let warning = art_budget_warning(&synthetic_model(PROP_TRIANGLE_BUDGET + 1, &[(128, 128)]))
        .expect("over-budget triangles warn");
    assert!(warning.contains("triangles"), "{warning}");

    let warning = art_budget_warning(&synthetic_model(100, &[(PROP_TEXTURE_NATIVE_SIZE + 1, 8)]))
        .expect("textures above the native size warn");
    assert!(warning.contains("texture"), "{warning}");

    let mut assets = PropAssets::default();
    assets.report_budget_warning("models/many_tris.glb", "warning");
    assets.report_budget_warning("models/many_tris.glb", "warning");
    assert_eq!(
        assets.reported_budget_warnings.len(),
        1,
        "a model warns exactly once no matter how often it resolves"
    );
}

/// A normal collection of native 256x256 sheets fits the desktop budget with
/// years of headroom: the eight refreshed domestic props together use 2 MiB
/// against a 64 MiB pack budget.
#[test]
fn a_normal_256_collection_fits_the_desktop_pack_budget_with_headroom() {
    let native_props = 8usize;
    let collection = native_props * 256 * 256 * 4;
    assert_eq!(
        collection,
        2 * 1024 * 1024,
        "the reference collection is 2 MiB"
    );
    assert!(
        !pack_texture_budget_exceeded(collection),
        "a handful of 256px props must be unremarkable"
    );
    assert!(
        collection.saturating_mul(8) <= PROP_TEXTURE_PACK_BUDGET_BYTES,
        "the reference collection must leave at least 8x headroom"
    );
}

/// The pack budget still rejects pathological content: sixteen engine-max
/// images are 64 MiB and one byte more is over budget.
#[test]
fn a_pathological_texture_pack_is_still_rejected() {
    let engine_max =
        crate::assets::decoded_rgba_bytes(MAX_PROP_TEXTURE_SIZE, MAX_PROP_TEXTURE_SIZE);
    assert_eq!(engine_max, MAX_PROP_TEXTURE_BYTES);
    assert_eq!(PROP_TEXTURE_PACK_BUDGET_BYTES, 64 * 1024 * 1024);
    assert!(
        pack_texture_budget_exceeded(PROP_TEXTURE_PACK_BUDGET_BYTES + 1),
        "one byte over the pack budget must be rejected"
    );
    assert!(
        !pack_texture_budget_exceeded(PROP_TEXTURE_PACK_BUDGET_BYTES),
        "the budget itself must be accepted"
    );
}
// These tiny GLBs contain geometry only.
fn refresh_triangle_glb(extent: f32) -> Vec<u8> {
    let mut binary = Vec::new();
    for point in [[0.0_f32, 0.0, 0.0], [extent, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for coordinate in point {
            binary.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    for uv in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for coordinate in uv {
            binary.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    for index in [0_u16, 1, 2] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let mut json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2}]}],"buffers":[{"byteLength":68}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":24},{"buffer":0,"byteOffset":60,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"},{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}]}"#.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 28_usize
        .saturating_add(json.len())
        .saturating_add(binary.len());
    let mut output = Vec::new();
    for word in [
        0x4654_6c67_u32,
        2,
        u32::try_from(total).expect("tiny GLB length"),
        u32::try_from(json.len()).expect("tiny JSON length"),
        0x4e4f_534a,
    ] {
        output.extend_from_slice(&word.to_le_bytes());
    }
    output.extend_from_slice(&json);
    output.extend_from_slice(
        &u32::try_from(binary.len())
            .expect("tiny binary length")
            .to_le_bytes(),
    );
    output.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    output.extend_from_slice(&binary);
    output
}

#[test]
fn refreshed_prop_inputs_preserve_shared_models_and_replace_changed_or_restored_files() {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Cleanup(std::env::temp_dir().join(format!(
        "places-prop-refresh-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir(&scratch.0).expect("isolated fixture directory");
    let path = scratch.0.join("used.glb");
    let original = refresh_triangle_glb(1.0);
    let changed = refresh_triangle_glb(2.0);
    assert_eq!(original.len(), changed.len());
    std::fs::write(&path, &original).expect("write tiny GLB");
    let modified = std::fs::metadata(&path)
        .expect("metadata")
        .modified()
        .expect("mtime");
    let paths = ["used.glb".to_string()];
    let mut assets = PropAssets::with_root(&scratch.0);
    assets.refresh_inputs(&paths);
    let first = assets.resolve("used.glb").expect("parse first triangle");
    assert_eq!(first.model.triangles, 1);
    assets.refresh_inputs(&paths);
    let hit = assets
        .resolve("used.glb")
        .expect("reuse unchanged triangle");
    assert!(Arc::ptr_eq(&first, &hit));

    std::fs::write(
        scratch.0.join("unrelated.glb"),
        b"different unrelated bytes",
    )
    .expect("unrelated edit");
    assets.refresh_inputs(&paths);
    assert!(Arc::ptr_eq(
        &first,
        &assets.resolve("used.glb").expect("unchanged used model")
    ));

    std::fs::write(&path, &changed).expect("edit used triangle");
    std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("file")
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .expect("restore mtime");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        modified
    );
    assets.refresh_inputs(&paths);
    let second = assets.resolve("used.glb").expect("parse modified triangle");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(
        first
            .model
            .vertices
            .iter()
            .any(|vertex| vertex.pos[0].to_bits() == 1.0_f32.to_bits())
    );
    assert!(
        second
            .model
            .vertices
            .iter()
            .any(|vertex| vertex.pos[0].to_bits() == 2.0_f32.to_bits())
    );
    assert!(
        !first
            .model
            .vertices
            .iter()
            .any(|vertex| vertex.pos[0].to_bits() == 2.0_f32.to_bits()),
        "live old model immutable"
    );

    std::fs::remove_file(&path).expect("remove used file");
    assets.refresh_inputs(&paths);
    assert!(
        assets.resolve("used.glb").is_err(),
        "missing file must not return stale parsed asset"
    );
    std::fs::write(&path, &original).expect("restore used file");
    assets.refresh_inputs(&paths);
    let restored = assets
        .resolve("used.glb")
        .expect("recover previously failed input");
    assert!(!Arc::ptr_eq(&second, &restored));
    assert!(
        restored
            .model
            .vertices
            .iter()
            .any(|vertex| vertex.pos[0].to_bits() == 1.0_f32.to_bits())
    );
    assert!(
        !restored
            .model
            .vertices
            .iter()
            .any(|vertex| vertex.pos[0].to_bits() == 2.0_f32.to_bits())
    );
}

#[test]
fn request_retention_removes_unrelated_inputs_and_failures() {
    let mut assets = PropAssets::default();
    for path in ["used", "old"] {
        assets
            .inputs
            .insert(path.to_string(), Err("missing".to_string()));
        assets
            .models
            .insert(path.to_string(), Err("missing".to_string()));
        assets.reported_failures.push(path.to_string());
        assets.reported_budget_warnings.push(path.to_string());
    }
    assets.retain_request_paths(&["used".to_string()]);
    assert_eq!(assets.inputs.len(), 1);
    assert!(assets.inputs.contains_key("used"));
    assert_eq!(assets.models.len(), 1);
    assert_eq!(assets.reported_failures, ["used"]);
    assert_eq!(assets.reported_budget_warnings, ["used"]);
    assets.retain_request_paths(&[]);
    assert!(assets.inputs.is_empty());
    assert!(assets.models.is_empty());
}

//! Unit tests for the prop asset pipeline.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests and developer measurement output"
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

/// Tiny allocation witness: family models share identical decoded sheets,
/// while a different size or changed pigment keeps a distinct allocation.
#[test]
fn equal_family_images_share_bytes_without_changing_numeric_masks() {
    let mut assets = PropAssets::default();
    let mut first = Arc::new(RawImage::new(1, 1, vec![17, 43, 89, 255]));
    let mut second = Arc::new((*first).clone());
    assets.intern_texture(&mut first);
    assets.intern_texture(&mut second);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(assets.stats().texture_bytes, 4);
    let mut mask = Arc::new(RawImage::new(1, 1, vec![17, 43, 90, 255]));
    assets.intern_texture(&mut mask);
    assert!(!Arc::ptr_eq(&first, &mask));
    assert_eq!(mask.rgba, [17, 43, 90, 255]);
}

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
        "the pack is the shared/office set plus the Pool family (at least 30 props); \
         the catalogue now lists {}",
        entries.len()
    );

    let mut ids = std::collections::HashSet::new();
    let mut models = std::collections::HashSet::new();
    let mut total_triangles = 0usize;
    let mut total_texture_bytes = 0usize;
    let mut textures = std::collections::HashSet::new();

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
        let first_model = models.insert(model_path.to_string());
        assert!(
            first_model || catalog.tint(&entry.id).is_some(),
            "{}: a shared model path requires an explicit colour preset",
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
            model.texture_count() >= 1
                || completed_model_review(model_path)
                    .is_some_and(|review| review.texture_edge == 0),
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
                texture.rgba.len()
                    == usize::try_from(texture.width * texture.height * 4)
                        .expect("fixture integer fits usize"),
                "{}: decoded texture buffer does not match its dimensions",
                entry.id
            );
        }
        // Art budget, not engine ceiling: shipped assets must stay inside the
        // numbers `assets/README.md` and `tools/props/build.py` enforce. A
        // skinned character follows the documented entity budget instead of
        // the static-prop target.
        let triangle_budget = prop_triangle_art_budget(model_path, model.skin.is_some());
        assert!(
            model.triangles <= triangle_budget,
            "{}: model has {} triangles, above the {triangle_budget}-triangle art budget",
            entry.id,
            model.triangles
        );
        for texture in &model.textures {
            // These reviewed large surfaces need crisp print at their actual
            // metre scale. All other shipped sheets keep the ordinary budget.
            let native_size = prop_texture_art_budget(model_path);
            assert!(
                texture.width <= native_size && texture.height <= native_size,
                "{}: texture is {}x{}, above its {native_size}px reviewed native prop size",
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
                if model.skin.is_some() && completed_model_review(model_path).is_some() && axis != 1
                {
                    low[axis] >= (-expected).mul_add(0.5, -tolerance)
                        && high[axis] <= expected.mul_add(0.5, tolerance)
                } else {
                    (dimensions[axis] - expected).abs() <= tolerance
                },
                "{}: model {} extent is {:.3} m but the catalogue says {:.3} m \
                 (tolerance {:.3} m); fix the model or the catalogue entry",
                entry.id,
                ["width", "height", "depth"][axis],
                dimensions[axis],
                expected,
                tolerance
            );
        }
        // A grounded model rests its base on y = 0. A hovering skinned
        // character (the sheet ghost) keeps its lowest vertex above the origin
        // and inside its own height: the level places it at y = 0 and the
        // hover is the asset's own bind pose, so it must neither sink below
        // the origin nor float away from it.
        if model.skin.is_some() {
            assert!(
                low[1] >= -0.012 && low[1] <= dimensions[1],
                "{}: skinned model base sits at y={:.3}; a grounded character must \
                 rest on y=0 and a hovering one must stay inside its own height",
                entry.id,
                low[1]
            );
        } else {
            assert!(
                low[1].abs() <= 0.012,
                "{}: model base sits at y={:.3}; props must rest on y=0",
                entry.id,
                low[1]
            );
        }
        let center_x = f32::midpoint(low[0], high[0]);
        let center_z = f32::midpoint(low[2], high[2]);
        assert!(
            (model.skin.is_some() && completed_model_review(model_path).is_some())
                || (center_x.abs() <= 0.02 && center_z.abs() <= 0.02),
            "{}: model is not horizontally centred (x={center_x:.3}, z={center_z:.3})",
            entry.id
        );

        if first_model {
            total_triangles += model.triangles;
        }
        for image in &model.textures {
            if textures.insert(std::sync::Arc::as_ptr(image)) {
                total_texture_bytes += image.rgba.len();
            }
        }
    }

    // Props that legitimately need more silhouette detail than a box-shaped
    // object are allowlisted explicitly instead of raising the global limit.
    // spooner-man is a tuxedo cat: a recognisable creature needs a head,
    // legs, a tail and readable markings, so he sits above the 800-triangle
    // review threshold and well under the 1500 hard ceiling. The shipped
    // entity models are the same case: a readable human mannequin, a rat with
    // four articulated legs and a tail, an articulated skeleton, and the
    // Halloween creatures (a carved candle pumpkin at 1498 triangles, a
    // painted sheet ghost at 1270, and a 94-joint pumpkin-head skeleton at
    // 2278, all under the 3000-triangle skinned-character ceiling).
    // Winter evergreens preserve all 708 canonical triangles and add closed,
    // supported bough coats: 1284/1380 triangles, both under 1500.
    // Aero dome retains 16 structural ribs, a closed glazed shell and radial soffit
    // at meaningful 12 m scale: 1152 triangles, under the existing 1500 ceiling.
    // Party cakes use closed piped frosting shells, eight scalloped icing accents,
    // eighteen sprinkles and seven striped candles/wicks; the lit variant also
    // retains seven tiny animated flames. Their reviewed 1352/1492 triangles
    // preserve those focal-point silhouettes below the existing 1500 ceiling.
    let detailed_props = [
        "arcade:party_cake_lit",
        "arcade:party_cake_unlit",
        "frutiger_aero:atrium_dome",
        "spooner-man",
        "mannequin",
        "rat",
        "skeleton",
        "carved-pumpkin",
        "sheet-ghost",
        "pumpkin-skeleton",
        "sheet-ghost-cat",
        "winter:tree_snow_01",
        "winter:tree_snow_02",
    ];
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
            detailed_props.contains(&id.as_str())
                || catalog
                    .get(id)
                    .model
                    .as_deref()
                    .and_then(completed_model_review)
                    .is_some(),
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
        models.len(),
        "expected every unique catalogued model to load"
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
    let first = assets.resolve("environments/office/props/chair.glb");
    let second = assets.resolve("environments/office/props/chair.glb");
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
        .resolve("environments/office/props/chair.glb")
        .expect("chair loads");
    let second = assets
        .resolve("environments/office/props/chair.glb")
        .expect("chair loads");
    assert!(
        Arc::ptr_eq(&first, &second),
        "identical models must share one decoded copy"
    );
    assert_eq!(assets.stats().models_loaded, 1);
}

/// Two tiny generated GLBs isolate model/image identity and request eviction;
/// no shipped map bake, filesystem scan or GPU upload is needed.
#[test]
fn request_dependency_hashes_share_images_and_release_stale_models() {
    let model_path = "fixtures/shared.glb";
    let source = |colour| {
        crate::test_support::two_material_glb(
            crate::test_support::TestGlbMaterial::plain(colour),
            crate::test_support::TestGlbMaterial::plain([0, 0, 0, 255]),
        )
    };
    let first_bytes = source([255, 0, 0, 255]);
    let dependency = |bytes: &[u8]| crate::package::PackageDependency {
        kind: crate::package::DependencyKind::Model,
        path: model_path.to_owned(),
        sha256: crate::package::hash::sha256_hex(bytes),
        bytes: u64::try_from(bytes.len()).expect("fixture length"),
    };
    let first_dependency = dependency(&first_bytes);
    let mut assets = PropAssets::with_root("unused-model-identity-fixture");
    assets.synchronize_dependencies(std::slice::from_ref(&first_dependency));
    drop(
        assets
            .inputs
            .insert(model_path.to_owned(), Ok(Arc::from(first_bytes))),
    );
    let first = assets.resolve(model_path).expect("first tiny model");
    let batch_images = first.model.textures.clone();
    assets.synchronize_dependencies(std::slice::from_ref(&first_dependency));
    let reusable = assets.resolve(model_path).expect("unchanged tiny model");
    assert!(
        Arc::ptr_eq(&first, &reusable),
        "unchanged dependencies share the decoded model"
    );
    assert!(
        Arc::ptr_eq(&batch_images[0], &reusable.model.textures[0]),
        "batches share the model's exact pixel allocation"
    );

    let second_bytes = source([0, 255, 0, 255]);
    assets.synchronize_dependencies(&[dependency(&second_bytes)]);
    drop(
        assets
            .inputs
            .insert(model_path.to_owned(), Ok(Arc::from(second_bytes))),
    );
    let changed = assets.resolve(model_path).expect("changed tiny model");
    assert!(
        !Arc::ptr_eq(&first, &changed),
        "a changed dependency cannot reuse a decoded model"
    );
    assert_ne!(first.model.textures[0].rgba, changed.model.textures[0].rgba);
    assert!(
        Arc::ptr_eq(&batch_images[0], &first.model.textures[0]),
        "the active world's old images remain valid"
    );
    assets.synchronize_dependencies(&[]);
    assert_eq!(
        assets.stats().models_loaded,
        0,
        "unrelated models are released at the next request"
    );
    assert_eq!(
        first.model.textures[0].rgba, batch_images[0].rgba,
        "eviction does not invalidate active image owners"
    );
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
            normal: None,
            pos: [0.0; 3],
            color: [1.0; 4],
            uv: [0.0; 2],
        }],
        indices: vec![0u16; triangles.saturating_mul(3)],
        textures: textures
            .iter()
            .map(|&(width, height)| {
                Arc::new(RawImage::new(
                    width,
                    height,
                    vec![0u8; rgba_bytes(width, height)],
                ))
            })
            .collect(),
        submeshes: vec![PropSubmesh {
            double_sided: true,
            response: crate::materials::MaterialResponse::NONE,
            alpha: crate::materials::MaterialAlpha::OPAQUE,
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
        art_budget_warning("models/ordinary.glb", &synthetic_model(100, &[(256, 256)])),
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
        art_budget_warning(
            "models/ordinary.glb",
            &synthetic_model(PROP_TRIANGLE_BUDGET, &[(128, 128)]),
        ),
        None,
        "a model inside the art budget warns about nothing"
    );
    let warning = art_budget_warning(
        "models/ordinary.glb",
        &synthetic_model(PROP_TRIANGLE_BUDGET + 1, &[(128, 128)]),
    )
    .expect("over-budget triangles warn");
    assert!(warning.contains("triangles"), "{warning}");

    let texture_warning = art_budget_warning(
        "models/ordinary.glb",
        &synthetic_model(100, &[(PROP_TEXTURE_NATIVE_SIZE + 1, 8)]),
    )
    .expect("textures above the native size warn");
    assert!(texture_warning.contains("texture"), "{texture_warning}");

    let mut assets = PropAssets::default();
    assets.report_budget_warning("models/many_tris.glb", "warning");
    assets.report_budget_warning("models/many_tris.glb", "warning");
    assert_eq!(
        assets.reported_budget_warnings.len(),
        1,
        "a model warns exactly once no matter how often it resolves"
    );
}

// Tiny thin sheets test the warning policy without decoding real 1024-square
// artwork. The shipped-asset test covers every reviewed file and its bounds;
// these cases uniquely protect exact review scope and both oversized edges.
#[test]
fn reviewed_texture_budgets_are_exact_and_still_enforced() {
    let cloth = "environments/arcade/party/models/party_cloth_rect_confetti.glb";
    let gift = "environments/arcade/party/models/party_gift_stars.glb";
    for (path, dimensions, warns) in [
        (cloth, (1_024, 1), false),
        (cloth, (1, 1_024), false),
        (gift, (512, 1), false),
        (gift, (1, 513), true),
        (cloth, (1_025, 1), true),
        ("models/party_cloth_rect_confetti.glb", (1_024, 1), true),
        (
            "environments/arcade/party/models/party_cloth_unreviewed.glb",
            (257, 1),
            true,
        ),
        (
            "environments/arcade/party/models/party_gift_unreviewed.glb",
            (1, 257),
            true,
        ),
    ] {
        assert_eq!(
            art_budget_warning(path, &synthetic_model(100, &[dimensions])).is_some(),
            warns,
            "{path} with {dimensions:?} must respect its reviewed texture budget"
        );
    }
    let warning = art_budget_warning(
        cloth,
        &synthetic_model(PROP_TRIANGLE_BUDGET + 1, &[(1_024, 1)]),
    )
    .expect("reviewed textures do not exempt triangle budgets");
    assert!(warning.contains("triangles"), "{warning}");
}

/// A normal collection of native 256x256 sheets fits the desktop budget with
/// years of headroom: the eight refreshed domestic props together use 2 MiB
/// against a 106 MiB pack budget.
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

/// The pack budget still rejects pathological content: one byte beyond its
/// documented limit is over budget.
#[test]
fn a_pathological_texture_pack_is_still_rejected() {
    let engine_max =
        crate::assets::decoded_rgba_bytes(MAX_PROP_TEXTURE_SIZE, MAX_PROP_TEXTURE_SIZE);
    assert_eq!(engine_max, MAX_PROP_TEXTURE_BYTES);
    assert_eq!(PROP_TEXTURE_PACK_BUDGET_BYTES, 106 * 1024 * 1024);
    assert!(
        pack_texture_budget_exceeded(PROP_TEXTURE_PACK_BUDGET_BYTES + 1),
        "one byte over the pack budget must be rejected"
    );
    assert!(
        !pack_texture_budget_exceeded(PROP_TEXTURE_PACK_BUDGET_BYTES),
        "the budget itself must be accepted"
    );
}

//! Unit tests for the prop asset pipeline.

use super::*;
use crate::level::{
    MAX_PROP_TEXTURE_SIZE, MAX_PROP_TRIANGLES, MAX_PROP_VERTICES, PROP_TRIANGLE_REVIEW,
    PROP_TRIANGLE_TARGET,
};
use crate::loader::PropCatalog;

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
            "{}: model has {} triangles; the PocketCHIP hard ceiling is {MAX_PROP_TRIANGLES}",
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
            model.texture.width <= MAX_PROP_TEXTURE_SIZE
                && model.texture.height <= MAX_PROP_TEXTURE_SIZE,
            "{}: texture is {}x{}; the PocketCHIP asset limit is {MAX_PROP_TEXTURE_SIZE}x{MAX_PROP_TEXTURE_SIZE}",
            entry.id,
            model.texture.width,
            model.texture.height
        );
        assert!(
            model.texture.rgba.len() == (model.texture.width * model.texture.height * 4) as usize,
            "{}: decoded texture buffer does not match its dimensions",
            entry.id
        );

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
    // review threshold and well under the 1500 hard ceiling.
    let detailed_props = ["spooner-man"];
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
    // Every prop may ship a preferred-size 128x128 texture (64 KiB decoded);
    // the pack-wide budget is that cap for the whole catalogue, so the
    // number scales with the content set instead of being a magic constant.
    let texture_budget = entries.len() * 128 * 128 * 4;
    assert!(
        total_texture_bytes <= texture_budget,
        "decoded prop textures total {total_texture_bytes} bytes; the pack budget for {} \
         props at 128x128 is {texture_budget} bytes",
        entries.len()
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
         hard ceiling {MAX_PROP_TRIANGLES}; textures <= {MAX_PROP_TEXTURE_SIZE}px"
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
        Rc::ptr_eq(&first, &second),
        "identical models must share one decoded copy"
    );
    assert_eq!(assets.stats().models_loaded, 1);
}

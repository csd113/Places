//! Prop asset loading, caching and budget validation.
//!
//! Levels reference props by catalogue id (`core:chair`); the catalogue maps
//! that id to a model path (`models/chair.glb`). This module resolves those
//! paths, parses each GLB exactly once, and keeps the decoded model (vertices,
//! indices, texture) behind an `Rc` so twenty placed chairs share one CPU copy
//! and one GPU texture upload.
//!
//! Failure is always graceful: a missing or malformed model is remembered as an
//! error (never retried in a loop) and the renderer falls back to the prop's
//! catalogue-sized placeholder box, with a developer-facing message on stderr.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::gltf::{GltfError, PropModel, parse_glb};

/// Directory candidates for the shipped prop assets, mirroring the catalogue's
/// own search order so a run from the app dir and from a packaged build both work.
const PROP_ROOT_CANDIDATES: [&str; 4] = [
    "assets/props",
    "./assets/props",
    "../assets/props",
    "assets",
];

/// One model's decoded asset plus the path it came from.
#[derive(Debug)]
pub struct LoadedPropAsset {
    pub model_path: String,
    pub model: PropModel,
}

/// Decoded texture memory held by the cache, in bytes (RGBA, uncompressed).
#[must_use]
pub const fn texture_bytes(model: &PropModel) -> usize {
    model.texture.rgba.len()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PropAssetStats {
    pub models_loaded: usize,
    pub models_failed: usize,
    pub triangles: usize,
    pub texture_bytes: usize,
}

/// Cache of parsed prop models keyed by their catalogue model path.
#[derive(Debug, Default)]
pub struct PropAssets {
    root: Option<PathBuf>,
    models: HashMap<String, Result<Rc<LoadedPropAsset>, String>>,
    /// Model paths that already produced a fallback, so the renderer logs each
    /// broken asset exactly once instead of once per placement.
    reported_failures: Vec<String>,
}

impl PropAssets {
    /// Creates a cache using the standard asset search order.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            root: resolve_prop_root(),
            models: HashMap::new(),
            reported_failures: Vec::new(),
        }
    }

    /// Creates a cache rooted at an explicit directory (tests, tools).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            models: HashMap::new(),
            reported_failures: Vec::new(),
        }
    }

    /// Resolved asset root, if one exists on disk.
    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Loads (or returns the cached) model for a catalogue `model` path.
    /// # Errors
    ///
    /// Returns a message when the model file is missing, is not a valid GLB, or
    /// exceeds the prop budgets.
    pub fn resolve(&mut self, model_path: &str) -> Result<Rc<LoadedPropAsset>, String> {
        if let Some(cached) = self.models.get(model_path) {
            return cached.clone();
        }
        let result = self.load(model_path);
        self.models.insert(model_path.to_string(), result.clone());
        result
    }

    fn load(&self, model_path: &str) -> Result<Rc<LoadedPropAsset>, String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "no prop asset directory found (expected assets/props)".to_string())?;
        let full_path = root.join(model_path);
        let bytes = fs::read(&full_path)
            .map_err(|error| format!("cannot read prop model {}: {error}", full_path.display()))?;
        let model = parse_glb(&bytes)
            .map_err(|error: GltfError| format!("prop model {model_path} is invalid: {error}"))?;
        Ok(Rc::new(LoadedPropAsset {
            model_path: model_path.to_string(),
            model,
        }))
    }

    /// Prints a one-time developer warning for a model that fell back.
    pub fn report_failure(&mut self, model_path: &str, message: &str) {
        if self.reported_failures.iter().any(|path| path == model_path) {
            return;
        }
        self.reported_failures.push(model_path.to_string());
        eprintln!("[props] {message} - using the catalogue placeholder box");
    }

    /// Cache statistics, used by the performance overlay and tests.
    #[must_use]
    pub fn stats(&self) -> PropAssetStats {
        let mut stats = PropAssetStats::default();
        for entry in self.models.values() {
            match entry {
                Ok(asset) => {
                    stats.models_loaded += 1;
                    stats.triangles += asset.model.triangles;
                    stats.texture_bytes += texture_bytes(&asset.model);
                }
                Err(_) => stats.models_failed += 1,
            }
        }
        stats
    }
}

/// Finds the shipped prop asset directory.
pub fn resolve_prop_root() -> Option<PathBuf> {
    PROP_ROOT_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{
        MAX_PROP_TEXTURE_SIZE, MAX_PROP_TRIANGLES, MAX_PROP_VERTICES, PROP_TRIANGLE_REVIEW,
        PROP_TRIANGLE_TARGET,
    };
    use crate::loader::PropCatalog;

    /// Catalogue + shipped GLB validation, the automated half of the asset
    /// budgets in `assets/props/README.md`. Every failure message names the prop
    /// and the concrete rule so the fix is obvious.
    #[test]
    fn shipped_prop_assets_match_the_catalogue_and_budgets() {
        let catalog = PropCatalog::load_default();
        let mut assets = PropAssets::load_default();
        assert!(
            assets.root().is_some(),
            "prop asset directory not found; expected assets/props next to the crate"
        );

        let entries = catalog.entries();
        assert!(
            !entries.is_empty(),
            "prop catalogue assets/props/props.json is empty"
        );
        assert_eq!(
            entries.len(),
            21,
            "the pack is twenty core props plus spooner-man; the catalogue now lists {}",
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
                model.texture.rgba.len()
                    == (model.texture.width * model.texture.height * 4) as usize,
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
        assert!(
            total_triangles <= 21 * PROP_TRIANGLE_REVIEW,
            "the pack totals {total_triangles} triangles across 21 props; investigate the outliers"
        );
        assert!(
            total_texture_bytes <= 1_200_000,
            "decoded prop textures total {total_texture_bytes} bytes; keep the pack under ~1.2 MiB"
        );

        let stats = assets.stats();
        assert_eq!(stats.models_failed, 0, "some models failed to load");
        assert_eq!(
            stats.models_loaded, 21,
            "expected twenty loaded prop models"
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
        let first = assets.resolve("models/chair.glb");
        let second = assets.resolve("models/chair.glb");
        assert!(first.is_err() && second.is_err());
        assert_eq!(first.unwrap_err(), second.unwrap_err());
        let stats = assets.stats();
        assert_eq!(stats.models_failed, 1);
        assert_eq!(stats.models_loaded, 0);
    }

    #[test]
    fn assets_are_shared_between_instances() {
        let mut assets = PropAssets::load_default();
        let first = assets.resolve("models/chair.glb").expect("chair loads");
        let second = assets.resolve("models/chair.glb").expect("chair loads");
        assert!(
            Rc::ptr_eq(&first, &second),
            "identical models must share one decoded copy"
        );
        assert_eq!(assets.stats().models_loaded, 1);
    }
}

//! Placeable asset loading, caching and budget validation.
//!
//! Levels reference assets by logical id (`core:chair`, `spooner-man`); the
//! asset catalog maps that id to a canonical resource path below the asset root
//! (`assets/`). This module resolves those paths, parses each GLB exactly once,
//! and keeps the decoded model (vertices, indices, textures) behind an `Rc` so
//! twenty placed chairs share one CPU copy and one GPU texture upload per
//! texture the model uses.
//!
//! Failure is always graceful: a missing or malformed model is remembered as an
//! error (never retried in a loop) and the renderer falls back to the prop's
//! catalogue-sized placeholder box, with a developer-facing message on stderr.
//! A model that loads but exceeds the Places art budget is *not* a failure: it
//! draws normally and gets one developer warning naming the budget it breaks.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::gltf::{GltfError, PropModel, parse_glb};
use crate::level::{PROP_TEXTURE_PREFERRED_SIZE, PROP_TRIANGLE_BUDGET};

/// One model's decoded asset plus the path it came from.
#[derive(Debug)]
pub struct LoadedPropAsset {
    pub model_path: String,
    pub model: PropModel,
}

/// Decoded texture memory held by the cache, in bytes (RGBA, uncompressed).
#[must_use]
pub fn texture_bytes(model: &PropModel) -> usize {
    model
        .textures
        .iter()
        .map(|texture| texture.rgba.len())
        .sum()
}

/// The Places art budget a loaded model breaks, if any.
///
/// The art budget is deliberately softer than the parser's engine ceilings: a
/// model above it is a note for the artist, not a load failure. The message
/// names the broken budget so the fix is obvious.
fn art_budget_warning(model: &PropModel) -> Option<String> {
    let mut reasons: Vec<String> = Vec::new();
    if model.triangles > PROP_TRIANGLE_BUDGET {
        reasons.push(format!(
            "{} triangles (art budget {PROP_TRIANGLE_BUDGET})",
            model.triangles
        ));
    }
    let oversized: Vec<String> = model
        .textures
        .iter()
        .filter(|texture| {
            texture.width > PROP_TEXTURE_PREFERRED_SIZE
                || texture.height > PROP_TEXTURE_PREFERRED_SIZE
        })
        .map(|texture| format!("{}x{}", texture.width, texture.height))
        .collect();
    if !oversized.is_empty() {
        reasons.push(format!(
            "texture {} (art budget {PROP_TEXTURE_PREFERRED_SIZE}px)",
            oversized.join(", ")
        ));
    }
    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    }
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
    /// Model paths that already produced an art-budget warning, so a model
    /// with many placements still warns exactly once.
    reported_budget_warnings: Vec<String>,
}

impl PropAssets {
    /// Creates a cache using the standard asset search order.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            root: resolve_prop_root(),
            models: HashMap::new(),
            reported_failures: Vec::new(),
            reported_budget_warnings: Vec::new(),
        }
    }

    /// Creates a cache rooted at an explicit directory (tests, tools).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            models: HashMap::new(),
            reported_failures: Vec::new(),
            reported_budget_warnings: Vec::new(),
        }
    }

    /// Resolved asset root, if one exists on disk.
    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Loads (or returns the cached) model for a catalogue `model` path.
    ///
    /// A model above the Places art budget still loads; it is logged once as a
    /// developer warning. Only a missing file, invalid GLB, or a model above
    /// the engine ceilings fails.
    /// # Errors
    ///
    /// Returns a message when the model file is missing, is not a valid GLB, or
    /// exceeds the prop engine ceilings.
    pub fn resolve(&mut self, model_path: &str) -> Result<Rc<LoadedPropAsset>, String> {
        if let Some(cached) = self.models.get(model_path) {
            return cached.clone();
        }
        let result = self.load(model_path);
        if let Ok(asset) = &result
            && let Some(warning) = art_budget_warning(&asset.model)
        {
            self.report_budget_warning(model_path, &warning);
        }
        self.models.insert(model_path.to_string(), result.clone());
        result
    }

    fn load(&self, model_path: &str) -> Result<Rc<LoadedPropAsset>, String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "no asset directory found (expected assets/)".to_string())?;
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
    ///
    /// Missing assets have no logger to route through, and the message is
    /// aimed at a developer watching the terminal, so it goes to stderr as it
    /// always has.
    #[allow(clippy::print_stderr)]
    pub fn report_failure(&mut self, model_path: &str, message: &str) {
        if self.reported_failures.iter().any(|path| path == model_path) {
            return;
        }
        self.reported_failures.push(model_path.to_string());
        eprintln!("[props] {message} - using the catalogue placeholder box");
    }

    /// Prints a one-time art-budget warning for a model that loaded anyway.
    ///
    /// This is deliberately not a failure: the renderer draws the model, and
    /// the note exists so the artist knows the shipped pack wants a lighter
    /// mesh or a smaller texture. Deduped per model like [`Self::report_failure`].
    #[allow(clippy::print_stderr)]
    fn report_budget_warning(&mut self, model_path: &str, message: &str) {
        if self
            .reported_budget_warnings
            .iter()
            .any(|path| path == model_path)
        {
            return;
        }
        self.reported_budget_warnings.push(model_path.to_string());
        eprintln!("[props] art budget warning: {model_path} {message}; the asset still loads");
    }

    /// Cache statistics, used by the performance overlay and tests.
    #[must_use]
    pub fn stats(&self) -> PropAssetStats {
        let mut stats = PropAssetStats::default();
        for entry in self.models.values() {
            match entry {
                Ok(asset) => {
                    stats.models_loaded = stats.models_loaded.saturating_add(1);
                    stats.triangles = stats.triangles.saturating_add(asset.model.triangles);
                    stats.texture_bytes = stats
                        .texture_bytes
                        .saturating_add(texture_bytes(&asset.model));
                }
                Err(_) => stats.models_failed = stats.models_failed.saturating_add(1),
            }
        }
        stats
    }
}

/// Finds the shipped asset root (`assets/`).
///
/// The single definition lives in [`crate::assets`], so the catalog and the
/// model cache can never disagree about where files are stored.
#[must_use]
pub fn resolve_prop_root() -> Option<PathBuf> {
    crate::assets::resolve_asset_root()
}

#[cfg(test)]
mod tests;

//! Placeable asset loading, caching and budget validation.
//!
//! Levels reference assets by logical id (`core:chair`, `spooner-man`); the
//! asset catalog maps that id to a canonical resource path below the asset root
//! (`assets/`). This module resolves those paths, parses each GLB exactly once,
//! and keeps the decoded model (vertices, indices, texture) behind an `Rc` so
//! twenty placed chairs share one CPU copy and one GPU texture upload.
//!
//! Failure is always graceful: a missing or malformed model is remembered as an
//! error (never retried in a loop) and the renderer falls back to the prop's
//! catalogue-sized placeholder box, with a developer-facing message on stderr.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::gltf::{GltfError, PropModel, parse_glb};

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

/// Finds the shipped asset root (`assets/`).
///
/// The single definition lives in [`crate::assets`], so the catalog and the
/// model cache can never disagree about where files are stored.
pub fn resolve_prop_root() -> Option<PathBuf> {
    crate::assets::resolve_asset_root()
}

#[cfg(test)]
mod tests;

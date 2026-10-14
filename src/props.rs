//! Placeable asset loading, caching and budget validation.
//!
//! Levels reference assets by logical id (`core:chair`, `spooner-man`); the
//! asset catalog maps that id to a canonical resource path below the asset root
//! (`assets/`). This module resolves those paths and parses each GLB once
//! while its request dependency is retained, and keeps the decoded model
//! (vertices, indices, textures) behind an `Arc` so
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
use std::sync::{Arc, OnceLock, Weak};

use crate::gltf::{GltfError, PropModel, parse_glb};
use crate::level::{
    ENTITY_TRIANGLE_BUDGET, PROP_TEXTURE_NATIVE_SIZE, PROP_TEXTURE_PACK_BUDGET_BYTES,
    PROP_TRIANGLE_BUDGET,
};

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

/// True when a pack's decoded prop textures exceed the desktop pack budget.
///
/// The budget is [`PROP_TEXTURE_PACK_BUDGET_BYTES`]: generous enough that a
/// catalogue of native 256x256 sheets is unremarkable, firm enough to reject a
/// pathological or accidentally duplicated multi-gigabyte set. The prop tests
/// and `tools/props/build.py --check` enforce the same number.
#[must_use]
pub const fn pack_texture_budget_exceeded(decoded_bytes: usize) -> bool {
    decoded_bytes > PROP_TEXTURE_PACK_BUDGET_BYTES
}

/// Reviewed native texture edge for a model, shared by runtime diagnostics
/// and shipped-asset validation. These cloth prints and gift wraps need their
/// documented detail at metre scale; new or similarly named models still use
/// the ordinary budget until reviewed.
fn prop_texture_art_budget(model_path: &str) -> u32 {
    if let Some(review) = completed_model_review(model_path) {
        return review.texture_edge.max(PROP_TEXTURE_NATIVE_SIZE);
    }
    match model_path {
        "environments/arcade/party/models/party_cloth_rect_confetti.glb"
        | "environments/arcade/party/models/party_cloth_round_confetti.glb"
        | "environments/arcade/party/models/party_cloth_rect_balloons.glb"
        | "environments/arcade/party/models/party_cloth_round_balloons.glb"
        | "environments/arcade/party/models/party_cloth_rect_stars.glb"
        | "environments/arcade/party/models/party_cloth_round_stars.glb"
        | "environments/arcade/party/models/party_cloth_rect_arcade.glb"
        | "environments/arcade/party/models/party_cloth_round_arcade.glb" => 1_024,
        "environments/arcade/party/models/party_gift_stars.glb"
        | "environments/arcade/party/models/party_gift_confetti.glb"
        | "environments/arcade/party/models/party_gift_stripes.glb"
        | "environments/arcade/party/models/party_gift_arcade.glb"
        | "environments/arcade/party/models/party_gift_red.glb"
        | "environments/arcade/party/models/party_gift_green.glb" => 512,
        _ => PROP_TEXTURE_NATIVE_SIZE,
    }
}

/// Exact reviewed exports retain their authored detail without increasing the
/// ordinary asset limits. The shipped gate also verifies each accepted hash.
#[derive(serde::Deserialize)]
struct CompletedModelReview {
    sha256: String,
    triangles: usize,
    texture_edge: u32,
    #[serde(default)]
    clamped_uv_overscan: bool,
}

fn completed_model_review(model_path: &str) -> Option<&'static CompletedModelReview> {
    static REVIEWS: OnceLock<std::collections::BTreeMap<String, CompletedModelReview>> =
        OnceLock::new();
    REVIEWS
        .get_or_init(|| {
            serde_json::from_str(include_str!("../assets/completed-models.json"))
                .unwrap_or_default()
        })
        .get(model_path)
}

fn prop_triangle_art_budget(model_path: &str, skinned: bool) -> usize {
    completed_model_review(model_path).map_or(
        if skinned {
            ENTITY_TRIANGLE_BUDGET
        } else {
            PROP_TRIANGLE_BUDGET
        },
        |review| {
            review.triangles.max(if skinned {
                ENTITY_TRIANGLE_BUDGET
            } else {
                PROP_TRIANGLE_BUDGET
            })
        },
    )
}

/// The Places art budget a loaded model breaks, if any.
///
/// The art budget is deliberately softer than the parser's engine ceilings: a
/// model above it is a note for the artist, not a load failure. The message
/// names the broken budget so the fix is obvious.
fn art_budget_warning(model_path: &str, model: &PropModel) -> Option<String> {
    let mut reasons: Vec<String> = Vec::new();
    // A skinned model is a character; it gets the documented entity art
    // budget rather than the static-prop one.
    let triangle_budget = prop_triangle_art_budget(model_path, model.skin.is_some());
    if model.triangles > triangle_budget {
        reasons.push(format!(
            "{} triangles (art budget {triangle_budget})",
            model.triangles
        ));
    }
    let texture_budget = prop_texture_art_budget(model_path);
    let oversized: Vec<String> = model
        .textures
        .iter()
        .filter(|texture| texture.width > texture_budget || texture.height > texture_budget)
        .map(|texture| format!("{}x{}", texture.width, texture.height))
        .collect();
    if !oversized.is_empty() {
        reasons.push(format!(
            "texture {} (native prop size {texture_budget}px; \
             larger artwork requires an authoring review)",
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
#[derive(Debug, Default, Clone)]
pub struct PropAssets {
    inputs: HashMap<String, ModelInput>,
    model_identities: HashMap<String, String>,
    root: Option<PathBuf>,
    models: HashMap<String, Result<Arc<LoadedPropAsset>, String>>,
    /// Weak ownership permits inactive request images to disappear while
    /// equal family atlases share one decoded allocation across model paths.
    textures: HashMap<(u32, u32, String), Weak<crate::loader::RawImage>>,
    /// Model paths that already produced a fallback, so the renderer logs each
    /// broken asset exactly once instead of once per placement.
    reported_failures: Vec<String>,
    /// Model paths that already produced an art-budget warning, so a model
    /// with many placements still warns exactly once.
    reported_budget_warnings: Vec<String>,
}

pub(crate) type ModelInput = Result<Arc<[u8]>, String>;

impl PropAssets {
    fn read_input(&self, model_path: &str) -> ModelInput {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "no asset directory found (expected assets/)".to_string())?;
        let full_path = root.join(model_path);
        fs::read(&full_path)
            .map(Arc::from)
            .map_err(|error| format!("cannot read prop model {}: {error}", full_path.display()))
    }

    /// Creates a cache using the standard asset search order.
    #[must_use]
    pub fn load_default() -> Self {
        Self {
            root: resolve_prop_root(),
            models: HashMap::new(),
            textures: HashMap::new(),
            inputs: HashMap::new(),
            model_identities: HashMap::new(),
            reported_failures: Vec::new(),
            reported_budget_warnings: Vec::new(),
        }
    }

    /// Creates a cache rooted at an explicit directory (tests, tools).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            models: HashMap::new(),
            textures: HashMap::new(),
            inputs: HashMap::new(),
            model_identities: HashMap::new(),
            reported_failures: Vec::new(),
            reported_budget_warnings: Vec::new(),
        }
    }

    /// Resolved asset root, if one exists on disk.
    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Retains only successfully decoded models with unchanged declared bytes.
    /// Active worlds keep their own model/image Arcs when a request changes its
    /// dependency set; failed entries are retried once on the next request.
    pub(crate) fn synchronize_dependencies(
        &mut self,
        dependencies: &[crate::package::PackageDependency],
    ) {
        let identities: HashMap<_, _> = dependencies
            .iter()
            .filter(|dependency| dependency.kind == crate::package::DependencyKind::Model)
            .map(|dependency| (dependency.path.clone(), dependency.sha256.clone()))
            .collect();
        self.models.retain(|path, cached| {
            cached.is_ok()
                && self.model_identities.get(path).is_some_and(|previous| {
                    identities
                        .get(path)
                        .is_some_and(|current| current == previous)
                })
        });
        self.inputs.retain(|path, _| {
            self.model_identities.get(path).is_some_and(|previous| {
                identities
                    .get(path)
                    .is_some_and(|current| current == previous)
            })
        });
        self.reported_failures
            .retain(|path| self.models.contains_key(path));
        self.reported_budget_warnings
            .retain(|path| self.models.contains_key(path));
        self.textures.retain(|_, image| image.strong_count() > 0);
        self.model_identities = identities;
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
    pub fn resolve(&mut self, model_path: &str) -> Result<Arc<LoadedPropAsset>, String> {
        if let Some(cached) = self.models.get(model_path) {
            return cached.clone();
        }
        let result = self.load(model_path);
        if let Ok(asset) = &result
            && let Some(warning) = art_budget_warning(model_path, &asset.model)
        {
            self.report_budget_warning(model_path, &warning);
        }
        drop(self.models.insert(model_path.to_string(), result.clone()));
        result
    }

    fn load(&mut self, model_path: &str) -> Result<Arc<LoadedPropAsset>, String> {
        let bytes = match self.inputs.get(model_path) {
            Some(input) => input.clone()?,
            None => self.read_input(model_path)?,
        };
        let review = completed_model_review(model_path);
        if let Some(accepted) = review
            && crate::package::hash::sha256_hex(&bytes) != accepted.sha256
        {
            return Err(format!(
                "completed model {model_path} differs from its accepted export"
            ));
        }
        let parsed = if review.is_some_and(|entry| entry.clamped_uv_overscan) {
            crate::gltf::parse_glb_with_clamped_overscan(&bytes)
        } else {
            parse_glb(&bytes)
        };
        let mut model = parsed
            .map_err(|error: GltfError| format!("prop model {model_path} is invalid: {error}"))?;
        for image in &mut model.textures {
            self.intern_texture(image);
        }
        Ok(Arc::new(LoadedPropAsset {
            model_path: model_path.to_string(),
            model,
        }))
    }

    fn intern_texture(&mut self, image: &mut Arc<crate::loader::RawImage>) {
        let key = (
            image.width,
            image.height,
            crate::package::hash::sha256_hex(&image.rgba),
        );
        if let Some(shared) = self.textures.get(&key).and_then(Weak::upgrade) {
            *image = shared;
        } else {
            drop(self.textures.insert(key, Arc::downgrade(image)));
        }
    }

    /// Prints a one-time developer warning for a model that fell back.
    ///
    /// Missing assets are reported through [`crate::logging`], deduplicated per
    /// model path, and the message is aimed at a developer watching the
    /// terminal.
    pub fn report_failure(&mut self, model_path: &str, message: &str) {
        if self.reported_failures.iter().any(|path| path == model_path) {
            return;
        }
        self.reported_failures.push(model_path.to_string());
        crate::logging::warn_once(
            format!("prop-fallback:{model_path}"),
            format!("[props] {message} - using the catalogue placeholder box"),
        );
    }

    /// Prints a one-time art-budget warning for a model that loaded anyway.
    ///
    /// This is deliberately not a failure: the renderer draws the model, and
    /// the note exists so the artist knows the shipped pack wants a lighter
    /// mesh or a smaller texture. Deduped per model like [`Self::report_failure`].
    fn report_budget_warning(&mut self, model_path: &str, message: &str) {
        if self
            .reported_budget_warnings
            .iter()
            .any(|path| path == model_path)
        {
            return;
        }
        self.reported_budget_warnings.push(model_path.to_string());
        crate::logging::warn_once(
            format!("prop-budget:{model_path}"),
            format!("[props] art budget warning: {model_path} {message}; the asset still loads"),
        );
    }

    /// Cache statistics, used by the performance overlay and tests.
    #[must_use]
    #[expect(
        clippy::iter_over_hash_type,
        reason = "only commutative saturating integer totals are computed; cache order cannot affect statistics"
    )]
    pub fn stats(&self) -> PropAssetStats {
        let mut stats = PropAssetStats::default();
        for entry in self.models.values() {
            match entry {
                Ok(asset) => {
                    stats.models_loaded = stats.models_loaded.saturating_add(1);
                    stats.triangles = stats.triangles.saturating_add(asset.model.triangles);
                }
                Err(_) => stats.models_failed = stats.models_failed.saturating_add(1),
            }
        }
        stats.texture_bytes = self
            .textures
            .values()
            .filter_map(Weak::upgrade)
            .map(|image| image.rgba.len())
            .sum();
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

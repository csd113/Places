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
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};

use crate::engine::gltf::{GltfError, PropModel, parse_glb};
use crate::engine::level::{
    ENTITY_TRIANGLE_BUDGET, PROP_TEXTURE_NATIVE_SIZE, PROP_TEXTURE_PACK_BUDGET_BYTES,
    PROP_TRIANGLE_BUDGET,
};

/// One model's decoded asset plus the path it came from.
#[derive(Debug)]
pub struct LoadedPropAsset {
    pub model_path: String,
    pub model: PropModel,
}

/// Stable prepared-batch identity. The suffix distinguishes material variants
/// in transport, package texture restoration and GPU caches; it is never used
/// as a filesystem path. Source resources retain their normal catalog paths.
pub(crate) fn model_variant_identity(
    model_path: &str,
    overrides: &std::collections::BTreeMap<u16, String>,
) -> String {
    if overrides.is_empty() {
        return model_path.to_string();
    }
    let mut bytes = Vec::new();
    for (slot, id) in overrides {
        bytes.extend_from_slice(&slot.to_le_bytes());
        bytes.extend_from_slice(&u64::try_from(id.len()).unwrap_or(u64::MAX).to_le_bytes());
        bytes.extend_from_slice(id.as_bytes());
    }
    format!(
        "{model_path}#materials-{}",
        crate::engine::package::hash::sha256_hex(&bytes)
    )
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
    completed_model_review(model_path).map_or(PROP_TEXTURE_NATIVE_SIZE, |review| {
        review.texture_edge.max(PROP_TEXTURE_NATIVE_SIZE)
    })
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
    #[serde(default)]
    engine_triangle_limit: Option<usize>,
    #[serde(default)]
    bounds_min: Option<[f32; 3]>,
    #[serde(default)]
    bounds_max: Option<[f32; 3]>,
}

fn completed_model_review(model_path: &str) -> Option<&'static CompletedModelReview> {
    static REVIEWS: OnceLock<std::collections::BTreeMap<String, CompletedModelReview>> =
        OnceLock::new();
    REVIEWS
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../assets/completed-models.json"))
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
    character_textures: HashMap<String, Arc<crate::engine::loader::RawImage>>,
    model_identities: HashMap<String, String>,
    root: Option<PathBuf>,
    models: HashMap<String, Result<Arc<LoadedPropAsset>, String>>,
    /// Weak ownership permits inactive request images to disappear while
    /// equal family atlases share one decoded allocation across model paths.
    textures: HashMap<(u32, u32, String), Weak<crate::engine::loader::RawImage>>,
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
        crate::engine::assets::read_asset_bytes(
            &full_path,
            crate::engine::gltf::MAX_ENCODED_GLB_BYTES,
        )
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
            character_textures: HashMap::new(),
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
            character_textures: HashMap::new(),
            model_identities: HashMap::new(),
            reported_failures: Vec::new(),
            reported_budget_warnings: Vec::new(),
        }
    }

    /// Resolves one fitted character albedo without copying the model or rig.
    ///
    /// # Errors
    /// Rejects missing, oversized or non-opaque fitted textures.
    pub fn character_texture(
        &mut self,
        catalog: &crate::engine::assets::AssetCatalog,
        id: &str,
    ) -> Result<Arc<crate::engine::loader::RawImage>, String> {
        if let Some(image) = self.character_textures.get(id) {
            return Ok(Arc::clone(image));
        }
        let path = catalog
            .texture_path(id)
            .ok_or_else(|| format!("Unknown character texture `{id}`"))?;
        let root = self
            .root
            .as_deref()
            .ok_or("Character texture requires the installed assets")?;
        let decoded = crate::engine::materials::load_png_relative(root, path)?;
        if decoded.width != 1024
            || decoded.height != 1024
            || decoded
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel.get(3) != Some(&255))
        {
            return Err(format!(
                "Character texture `{id}` must be an opaque fitted 1024x1024 PNG"
            ));
        }
        if self.character_textures.len() >= 128 {
            return Err("Character texture budget exceeded (128 distinct appearances)".into());
        }
        let mut image = Arc::new(decoded);
        self.intern_texture(&mut image);
        drop(
            self.character_textures
                .insert(id.to_string(), Arc::clone(&image)),
        );
        Ok(image)
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
        dependencies: &[crate::engine::package::PackageDependency],
    ) {
        let identities: HashMap<_, _> = dependencies
            .iter()
            .filter(|dependency| dependency.kind == crate::engine::package::DependencyKind::Model)
            .map(|dependency| (dependency.path.clone(), dependency.sha256.clone()))
            .collect();
        self.character_textures.clear();
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

    /// Editor previews retain only the current scene's model dependencies.
    /// The previous GPU scene owns its own Arcs until replacement completes.
    pub(crate) fn retain_preview_models(&mut self, paths: &std::collections::BTreeSet<String>) {
        self.models.retain(|path, _| paths.contains(path));
        self.inputs.retain(|path, _| paths.contains(path));
        self.model_identities.retain(|path, _| paths.contains(path));
        self.reported_failures.retain(|path| paths.contains(path));
        self.reported_budget_warnings
            .retain(|path| paths.contains(path));
        self.textures.retain(|_, image| image.strong_count() > 0);
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
            && crate::engine::package::hash::sha256_hex(&bytes) != accepted.sha256
        {
            return Err(format!(
                "completed model {model_path} differs from its accepted export"
            ));
        }
        let parsed = review
            .and_then(|entry| entry.engine_triangle_limit)
            .map_or_else(
                || {
                    if review.is_some_and(|entry| entry.clamped_uv_overscan) {
                        crate::engine::gltf::parse_glb_with_clamped_overscan(&bytes)
                    } else {
                        parse_glb(&bytes)
                    }
                },
                |limit| crate::engine::gltf::parse_glb_with_reviewed_triangle_budget(&bytes, limit),
            );
        let mut model = parsed
            .map_err(|error: GltfError| format!("prop model {model_path} is invalid: {error}"))?;
        if let Some(accepted) = review
            && let (Some(expected_min), Some(expected_max)) =
                (accepted.bounds_min, accepted.bounds_max)
            && !model.bounds().is_some_and(|(low, high)| {
                low.iter()
                    .chain(high.iter())
                    .zip(expected_min.iter().chain(expected_max.iter()))
                    .all(|(actual, expected)| (actual - expected).abs() <= 0.000_02)
            })
        {
            return Err(format!(
                "completed model {model_path} differs from its reviewed bounds"
            ));
        }
        for image in &mut model.textures {
            self.intern_texture(image);
        }
        Ok(Arc::new(LoadedPropAsset {
            model_path: model_path.to_string(),
            model,
        }))
    }

    fn intern_texture(&mut self, image: &mut Arc<crate::engine::loader::RawImage>) {
        let key = (
            image.width,
            image.height,
            crate::engine::package::hash::sha256_hex(&image.rgba),
        );
        if let Some(shared) = self.textures.get(&key).and_then(Weak::upgrade) {
            *image = shared;
        } else {
            drop(self.textures.insert(key, Arc::downgrade(image)));
        }
    }

    /// Prints a one-time developer warning for a model that fell back.
    ///
    /// Missing assets are reported through [`crate::engine::logging`], deduplicated per
    /// model path, and the message is aimed at a developer watching the
    /// terminal.
    pub fn report_failure(&mut self, model_path: &str, message: &str) {
        if self.reported_failures.iter().any(|path| path == model_path) {
            return;
        }
        self.reported_failures.push(model_path.to_string());
        crate::engine::logging::warn_once(
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
        crate::engine::logging::warn_once(
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
/// The single definition lives in [`crate::engine::assets`], so the catalog and the
/// model cache can never disagree about where files are stored.
#[must_use]
pub fn resolve_prop_root() -> Option<PathBuf> {
    crate::engine::assets::resolve_asset_root()
}

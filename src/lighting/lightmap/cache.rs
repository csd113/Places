//! The deterministic content key and the level lightmap cache.
//!
//! A lightmap is a pure function of the level's geometry and light definitions,
//! the quality profile and the lightmap format. [`content_key`] reduces exactly
//! those inputs to one stable string:
//!
//! ```text
//! v1-<64-bit FNV-1a hash of>
//!     format version
//!     quality profile name
//!     every lightmap config field (density, page edge, budget, padding)
//!     the level's serialised bytes (rooms, walls, openings, floors,
//!     fixtures, prop placements, decal definitions)
//!     the occluder-set fingerprint (walls, slabs and derived prop boxes)
//! ```
//!
//! The key never depends on wall-clock time, iteration order or floating-point
//! formatting: serialising the same level twice produces the same bytes, and
//! so does hashing them. The on-disk cache under `cache/lightmaps/` (below the
//! runtime state root) is *never* consulted without that key, so stale data
//! cannot be reused after an edit; a cache miss simply bakes again.
//!
//! The cache is deliberately allowed to fail silently: it is an optimisation,
//! and a read-only or full filesystem must never break a level load.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{
    LIGHTMAP_ATLAS_MAX_PAGES, LevelLightmaps, LightmapConfig, LightmapPage, LightmapStats,
};

/// Bumped whenever the atlas layout, texel encoding or key inputs change.
///
/// It is part of every content key, so an implementation change can never
/// silently reuse an atlas baked by an older build; a stale directory under
/// `cache/lightmaps/` is simply ignored.
///
/// * `1` — level definition, lightmap config, quality profile.
/// * `2` — adds the occluder-set fingerprint, so a change to a placed prop
///   model's geometry (which now occludes the bake) invalidates the atlas.
/// * `3` — chart texels span their patch edge to edge (so coplanar charts agree
///   on a shared edge), and the ceiling-slab visibility clip no longer deletes a
///   fixture's pool on grazing samples. Both change baked texel values, so an
///   atlas from an older build must not be reused.
/// * `4` — soft shadows: the local pools are gated by an emitter-area
///   visibility fraction and the chart density/packing changed, so every texel
///   value differs from a version-3 atlas.
pub const LIGHTMAP_FORMAT_VERSION: u32 = 4;

/// Root of the runtime-owned on-disk cache, below the state root.
///
/// The state root is the package root (or `PLACES_STATE_ROOT` when set), so a
/// packaged build caches next to its own payload instead of creating a
/// development-flavoured `target/` directory.
pub const LIGHTMAP_CACHE_ROOT: &str = "cache/lightmaps";

/// One serialised cache directory: the key, the page edge and the charts.
#[derive(serde::Serialize, serde::Deserialize)]
struct DiskMeta {
    version: u32,
    edge: u32,
    charts: Vec<(super::LightmapPatch, super::Chart)>,
}

/// Process-level memory cache plus an optional on-disk store.
///
/// A hit is returned as the exact `Arc` the bake produced, so a level switch
/// costs no copy of the pages. The renderer owns one instance for the session;
/// tests use [`LightmapCache::memory_only`].
#[derive(Debug, Default)]
pub struct LightmapCache {
    entries: HashMap<String, Arc<LevelLightmaps>>,
    disk: bool,
}

impl LightmapCache {
    /// A memory-only cache, used by tests and by callers that do not want the
    /// project-owned cache directory touched.
    #[must_use]
    pub fn memory_only() -> Self {
        Self::default()
    }

    /// A cache that also reads and writes `cache/lightmaps/` below the state
    /// root.
    #[must_use]
    pub fn with_disk() -> Self {
        Self {
            entries: HashMap::new(),
            disk: true,
        }
    }

    /// Number of in-memory entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is cached in memory.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns a cached atlas for exactly this content key, if one exists.
    ///
    /// Checks memory first, then the on-disk store when enabled. A disk entry
    /// that does not parse, does not match the format version, or does not
    /// describe a self-consistent page set is rejected as a miss.
    pub fn get(&mut self, key: &str) -> Option<Arc<LevelLightmaps>> {
        if let Some(lightmaps) = self.entries.get(key) {
            return Some(Arc::clone(lightmaps));
        }
        if !self.disk {
            return None;
        }
        let root = crate::assets::state_path(LIGHTMAP_CACHE_ROOT);
        let lightmaps = disk_load(&root, key)?;
        let lightmaps = Arc::new(lightmaps);
        self.entries.insert(key.to_string(), Arc::clone(&lightmaps));
        Some(lightmaps)
    }

    /// Stores a freshly baked atlas under its content key.
    ///
    /// Memory always receives it; the disk store is best-effort and ignored
    /// when unavailable.
    pub fn insert(&mut self, key: &str, lightmaps: Arc<LevelLightmaps>) {
        if self.disk {
            let root = crate::assets::state_path(LIGHTMAP_CACHE_ROOT);
            disk_store(&root, key, &lightmaps);
        }
        self.entries.insert(key.to_string(), lightmaps);
    }

    /// Drops every in-memory entry (the disk store, if any, is left alone).
    pub fn clear_memory(&mut self) {
        self.entries.clear();
    }
}

/// Deterministic content key of one lightmap bake.
///
/// See the module docs for the exact inputs, minus the occluder fingerprint:
/// this is the convenience form for callers that have no bake at hand (tests,
/// tooling). The renderer uses [`content_key_with_extra`] with
/// [`crate::lighting::LevelLighting::occlusion_fingerprint`], which is what
/// makes a prop-model edit invalidate a cached atlas.
#[must_use]
pub fn content_key(
    level: &crate::level::LevelDef,
    config: &LightmapConfig,
    profile: crate::quality::QualityProfile,
) -> String {
    content_key_with_extra(level, config, profile, &[])
}

/// [`content_key`] with extra caller-supplied bytes folded into the hash.
#[must_use]
pub fn content_key_with_extra(
    level: &crate::level::LevelDef,
    config: &LightmapConfig,
    profile: crate::quality::QualityProfile,
    extra: &[u8],
) -> String {
    let mut hasher = Fnv1a::new();
    hasher.write_u32(LIGHTMAP_FORMAT_VERSION);
    hasher.write(profile.name().as_bytes());
    hasher.write_f32(config.texels_per_metre);
    hasher.write_u32(config.page_edge);
    hasher.write_u64(u64::try_from(config.max_pages).unwrap_or(u64::MAX));
    hasher.write_u32(config.padding);
    hasher.write_u32(config.bytes_per_texel);
    hasher.write(extra);
    // `serde_json` rejects non-finite floats, which the loader already rejects
    // in level files; a hand-built test level could still carry one, so fall
    // back to the lossless debug form rather than making the key collide.
    let bytes = serde_json::to_vec(level).unwrap_or_else(|_| format!("{level:?}").into_bytes());
    hasher.write(&bytes);
    format!("v{LIGHTMAP_FORMAT_VERSION}-{:016x}", hasher.finish())
}

/// FNV-1a 64-bit over arbitrary bytes: tiny, dependency-free and stable across
/// platforms and Rust versions, which is what a cache key needs.
struct Fnv1a {
    hash: u64,
}

impl Fnv1a {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self {
            hash: Self::OFFSET_BASIS,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(Self::PRIME);
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_f32(&mut self, value: f32) {
        self.write(&value.to_bits().to_le_bytes());
    }

    const fn finish(&self) -> u64 {
        self.hash
    }
}

/// True when `key` is safe to use as a single path component.
fn key_is_safe(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Reads one cached atlas from `root/key`, rejecting anything inconsistent.
pub(super) fn disk_load(root: &Path, key: &str) -> Option<LevelLightmaps> {
    if !key_is_safe(key) {
        return None;
    }
    let dir = root.join(key);
    let meta: DiskMeta =
        serde_json::from_slice(&std::fs::read(dir.join("meta.json")).ok()?).ok()?;
    if meta.version != LIGHTMAP_FORMAT_VERSION || meta.edge == 0 {
        return None;
    }
    let edge = usize::try_from(meta.edge).ok()?;
    let page_bytes = edge.checked_mul(edge)?.checked_mul(3)?;
    let raw = std::fs::read(dir.join("pages.bin")).ok()?;
    if page_bytes == 0 || !raw.len().is_multiple_of(page_bytes) {
        return None;
    }
    let page_count = raw.len().checked_div(page_bytes)?;
    if page_count == 0 || page_count > LIGHTMAP_ATLAS_MAX_PAGES {
        return None;
    }
    let mut pages = Vec::with_capacity(page_count);
    for chunk in raw.chunks_exact(page_bytes) {
        pages.push(LightmapPage {
            width: meta.edge,
            height: meta.edge,
            rgb: chunk.to_vec(),
        });
    }
    let mut texels = 0usize;
    for (_, chart) in &meta.charts {
        if !chart_fits(meta.edge, chart) || usize::from(chart.page) >= page_count {
            return None;
        }
        texels = texels.checked_add(chart_texels(chart)?)?;
    }
    let stats = LightmapStats {
        charts: meta.charts.len(),
        pages: page_count,
        texels,
        page_texels: page_count.checked_mul(edge.checked_mul(edge)?)?,
        bake_millis: 0.0,
        cache_hit: true,
    };
    Some(LevelLightmaps {
        pages,
        charts: meta.charts,
        stats,
        cache_key: key.to_string(),
    })
}

/// Writes one atlas to `root/key`. Failures are ignored by design.
pub(super) fn disk_store(root: &Path, key: &str, lightmaps: &LevelLightmaps) {
    if !key_is_safe(key) {
        return;
    }
    let Some(edge) = lightmaps.pages.first().map(|page| page.width) else {
        return;
    };
    if lightmaps
        .pages
        .iter()
        .any(|page| page.width != edge || page.height != edge)
    {
        return;
    }
    let meta = DiskMeta {
        version: LIGHTMAP_FORMAT_VERSION,
        edge,
        charts: lightmaps.charts.clone(),
    };
    let Ok(meta_bytes) = serde_json::to_vec(&meta) else {
        return;
    };
    let dir = root.join(key);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut pages: Vec<u8> = Vec::new();
    for page in &lightmaps.pages {
        pages.extend_from_slice(&page.rgb);
    }
    if std::fs::write(dir.join("meta.json"), meta_bytes).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join("pages.bin"), pages);
}

/// True when one chart's data rectangle lies inside a square page.
fn chart_fits(edge: u32, chart: &super::Chart) -> bool {
    chart.width > 0
        && chart.height > 0
        && chart
            .x
            .checked_add(chart.width)
            .is_some_and(|right| right <= edge)
        && chart
            .y
            .checked_add(chart.height)
            .is_some_and(|bottom| bottom <= edge)
}

/// Texels one chart holds, when addressable.
fn chart_texels(chart: &super::Chart) -> Option<usize> {
    usize::try_from(chart.width)
        .ok()?
        .checked_mul(usize::try_from(chart.height).ok()?)
}

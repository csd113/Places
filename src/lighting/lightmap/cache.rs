//! The deterministic content key and the level lightmap memory cache.
//!
//! A lightmap also depends on resolved material, image and model content.
//! This transient helper reduces source/settings inputs to one stable string;
//! manually retained caches require immutable resolved assets or explicit
//! clearing after an asset edit. The compiler's package/stage fingerprints
//! cover those resources, and player loading verifies their recorded hashes.
//!
//! ```text
//! v<format>-<64-bit FNV-1a hash of>
//!     format version
//!     quality profile name
//!     every lightmap config field (density, page edge, budget, padding)
//!     the solver/occluder fingerprint, taps, bounces and gather budget
//!     the level's serialised bytes (rooms, walls, openings, floors,
//!     fixtures, prop placements, decal definitions)
//! ```
//!
//! The key never depends on wall-clock time, iteration order or floating-point
//! formatting: serialising the same level twice produces the same bytes, and
//! so does hashing them.

use std::collections::VecDeque;
use std::sync::Arc;

use super::{LevelLightmaps, LightmapConfig};

/// Bumped whenever the atlas layout, texel encoding or key inputs change.
///
/// It is part of every content key, so an implementation change can never
/// silently reuse an atlas produced by an older build.
///
/// * `1`–`9` — the historical display-space pool/blend bake, where texels were
///   RGB8 values of [`crate::lighting::LevelLighting::sample_in_room`].
/// * `10` — the four-page array layout and bounce-fill model.
/// * `11` — the final display-space calibration before the transport upgrade.
/// * `12` — the offline HDR transport solve: an RGBA16F atlas carrying an
///   irradiance term and a directional moment per texel, real visible-emitter
///   shadowing and diffuse bounces, and prepared switchable-light layer groups.
///   No value from a version-11 or older atlas is valid.
/// * `13` — mesh endpoints address the first/last data texel centres; prop
///   chart density follows the selected architecture profile and size class.
pub const LIGHTMAP_FORMAT_VERSION: u32 = 13;

/// Process-level memory cache of prepared atlases.
///
/// A hit is returned as the exact `Arc` the bake produced, so a level switch
/// costs no copy of the pages. The compiler and the preparation worker own the
/// session cache; tests use [`LightmapCache::memory_only`].
#[derive(Debug, Default)]
pub struct LightmapCache {
    entries: VecDeque<(String, Arc<LevelLightmaps>)>,
}

impl LightmapCache {
    /// A memory-only cache, used by tests and by callers that do not want the
    /// project-owned cache directory touched.
    #[must_use]
    pub fn memory_only() -> Self {
        Self::default()
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
    pub fn get(&mut self, key: &str) -> Option<Arc<LevelLightmaps>> {
        if let Some(index) = self.entries.iter().position(|(entry, _)| entry == key) {
            let entry = self.entries.remove(index)?;
            let lightmaps = Arc::clone(&entry.1);
            self.entries.push_back(entry);
            return Some(lightmaps);
        }
        None
    }

    /// Stores a freshly baked atlas under its content key.
    pub fn insert(&mut self, key: &str, lightmaps: Arc<LevelLightmaps>) {
        if !key_is_safe(key) || lightmaps.cache_key != key {
            return;
        }
        self.retain(key, lightmaps);
    }

    /// Retains at most four atlases and 128 MiB; callers holding an Arc keep
    /// their active world valid even when its reusable cache entry is evicted.
    fn retain(&mut self, key: &str, lightmaps: Arc<LevelLightmaps>) {
        self.entries.retain(|(entry, _)| entry != key);
        let incoming = retained_bytes(&lightmaps);
        if incoming > MAX_MEMORY_BYTES {
            return;
        }
        while self.entries.len() >= MAX_MEMORY_ENTRIES
            || self.entries.iter().fold(incoming, |bytes, (_, entry)| {
                bytes.saturating_add(retained_bytes(entry))
            }) > MAX_MEMORY_BYTES
        {
            if self.entries.pop_front().is_none() {
                return;
            }
        }
        self.entries.push_back((key.to_string(), lightmaps));
    }

    /// Drops every in-memory entry.
    pub fn clear_memory(&mut self) {
        self.entries.clear();
    }
}

/// Deterministic content key of one lightmap bake.
///
/// See the module docs for the exact inputs, minus the solver/occluder
/// fingerprints: this is the convenience form for callers that have no bake at
/// hand (tests, tooling). The renderer uses [`content_key_with_extra`] with
/// [`crate::lighting::LevelLighting::occlusion_fingerprint`],
/// [`crate::lighting::model_fingerprint`] and
/// [`crate::lighting::transport::solver_fingerprint`], which is what makes an
/// authored occluder, lighting-model or solver change invalidate a cached atlas.
/// Resolved catalog/image/model bytes are absent: callers manually retaining
/// this transient cache must keep those inputs immutable or clear it.
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
    // Canonical JSON: `LevelDef` carries `HashMap` fields, so a direct
    // `serde_json::to_vec` would vary with iteration order and produce a
    // different cache key for identical content. Non-finite floats (which the
    // loader already rejects) fall back to the lossless debug form rather than
    // making the key collide.
    let bytes = crate::canonical_json::canonical_json_bytes(level)
        .unwrap_or_else(|_| format!("{level:?}").into_bytes());
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

const MAX_MEMORY_ENTRIES: usize = 4;
const MAX_MEMORY_BYTES: usize = 128 * 1_024 * 1_024;

fn retained_bytes(lightmaps: &LevelLightmaps) -> usize {
    let charts = lightmaps
        .charts
        .capacity()
        .saturating_mul(std::mem::size_of::<(super::LightmapPatch, super::Chart)>());
    let pages = lightmaps.pages.iter().fold(0usize, |bytes, page| {
        bytes.saturating_add(page.texels.capacity())
    });
    let switchable = lightmaps.switchable.iter().fold(
        std::mem::size_of::<super::SwitchableLightmaps>(),
        |bytes, group| {
            let group_bytes = group.pages.iter().fold(0usize, |page_bytes, page| {
                page_bytes.saturating_add(page.texels.capacity())
            });
            bytes
                .saturating_add(std::mem::size_of::<super::LightmapPage>())
                .saturating_add(group_bytes)
        },
    );
    charts.saturating_add(pages).saturating_add(switchable)
}

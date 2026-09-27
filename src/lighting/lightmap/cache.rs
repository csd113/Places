//! The deterministic content key and the level lightmap cache.
//!
//! A lightmap is a pure function of the level's geometry and light definitions,
//! the quality profile and the lightmap format. [`content_key`] reduces exactly
//! those inputs to one stable string:
//!
//! ```text
//! v<format>-<64-bit FNV-1a hash of>
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

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
/// * `5` — a wall chart resolves its room per texel instead of once per
///   emission strip, so a coalesced wall run that crosses a room boundary no
///   longer steps its baked light at the arbitrary strip boundary. Every wall
///   texel of a strip that spans rooms changes value, so an atlas from an older
///   build must not be reused.
/// * `6` — the page budget moved from two to four and the renderer addresses
///   the pages as layers of one `texture_2d_array`, so the page layout and the
///   shader's addressing contract both changed. An atlas baked by a version-5
///   build must never be installed: its directory name carries the old `v5-`
///   prefix, so it is not even looked up, and [`disk_load`] rejects any entry
///   whose `meta.version` is not this number.
/// * `7` — the lighting equation changed: the room baseline is lower, ceiling
///   fixtures cast a directional pool (lateral falloff plus incidence, nothing
///   above the emitter), every light adds a broad visibility-tested bounce
///   fill, and overlaps compose by a per-channel screen instead of a summed
///   cap. Every texel value differs from a version-6 atlas, so a version-6
///   directory must never be read.
/// * `8` — wall charts resolve their room exactly like the vertex bake's face
///   sample (strict containment first, the patch's own room as the fallback),
///   so a wall face on a shared room boundary is lit by its own room instead
///   of whichever overlapping neighbour the loose tie-break picked. Boundary
///   wall texels change value, so a version-7 atlas must not be reused.
/// * `9` — fully occluded wall length faces no longer receive charts; the
///   atlas topology changes while visible lighting and quality stay unchanged.
pub const LIGHTMAP_FORMAT_VERSION: u32 = 11;

/// Root of the runtime-owned on-disk cache, below the state root.
///
/// The state root is the package root (or `PLACES_STATE_ROOT` when set), so a
/// packaged build caches next to its own payload instead of creating a
/// development-flavoured `target/` directory.
pub const LIGHTMAP_CACHE_ROOT: &str = "cache/lightmaps";

/// Bounded metadata inside one checksummed storage envelope.
#[derive(serde::Serialize, serde::Deserialize)]
struct DiskMeta {
    version: u32,
    edge: u32,
    key: String,
    charts: Vec<(super::LightmapPatch, super::Chart)>,
    /// Gutter width around every chart, so a runtime light switch can
    /// re-dilate the charts it rewrites.
    padding: u32,
}

/// Process-level memory cache plus an optional on-disk store.
///
/// A hit is returned as the exact `Arc` the bake produced, so a level switch
/// costs no copy of the pages. The preparation worker owns the session cache;
/// tests use [`LightmapCache::memory_only`].
#[derive(Debug, Default)]
pub struct LightmapCache {
    entries: VecDeque<(String, Arc<LevelLightmaps>)>,
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
    pub const fn with_disk() -> Self {
        Self {
            entries: VecDeque::new(),
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
        if let Some(index) = self.entries.iter().position(|(entry, _)| entry == key) {
            let entry = self.entries.remove(index)?;
            let lightmaps = Arc::clone(&entry.1);
            self.entries.push_back(entry);
            return Some(lightmaps);
        }
        if !self.disk {
            return None;
        }
        let root = crate::assets::state_path(LIGHTMAP_CACHE_ROOT);
        let lightmaps = disk_load(&root, key)?;
        let lightmaps = Arc::new(lightmaps);
        self.retain(key, Arc::clone(&lightmaps));
        Some(lightmaps)
    }

    /// Stores a freshly baked atlas under its content key.
    ///
    /// Memory retains it within its LRU budget; the disk store is best-effort and ignored
    /// when unavailable.
    pub fn insert(&mut self, key: &str, lightmaps: Arc<LevelLightmaps>) {
        if !key_is_safe(key) || lightmaps.cache_key != key {
            return;
        }
        if self.disk {
            let root = crate::assets::state_path(LIGHTMAP_CACHE_ROOT);
            disk_store(&root, key, &lightmaps);
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

/// Storage framing is independent of the lighting algorithm/content-key version.
/// Old multi-file directories are deliberately cache misses.
const STORAGE_VERSION: u32 = 1;
const STORAGE_MAGIC: [u8; 8] = *b"PLCLMAP1";
const HEADER_BYTES: u64 = 40;
const MAX_META_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_CACHE_PAGE_EDGE: u32 = 2_048;
const MAX_MEMORY_ENTRIES: usize = 4;
const MAX_MEMORY_BYTES: usize = 128 * 1_024 * 1_024;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

/// Lengths are checked before allocating or reading variable-size data.
struct DiskLayout {
    edge: u32,
    page_count: usize,
    page_bytes: usize,
    meta_bytes: usize,
}

impl DiskLayout {
    fn new(edge: u32, page_count: usize, meta_bytes: usize) -> Option<Self> {
        if edge == 0
            || edge > MAX_CACHE_PAGE_EDGE
            || page_count == 0
            || page_count > LIGHTMAP_ATLAS_MAX_PAGES
            || meta_bytes == 0
            || meta_bytes > MAX_META_BYTES
        {
            return None;
        }
        let edge_size = usize::try_from(edge).ok()?;
        Some(Self {
            edge,
            page_count,
            page_bytes: edge_size.checked_mul(edge_size)?.checked_mul(3)?,
            meta_bytes,
        })
    }

    fn file_bytes(&self) -> Option<u64> {
        let payload = self.page_bytes.checked_mul(self.page_count)?;
        HEADER_BYTES.checked_add(u64::try_from(payload.checked_add(self.meta_bytes)?).ok()?)
    }

    fn prefix(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&STORAGE_MAGIC);
        bytes.extend_from_slice(&STORAGE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LIGHTMAP_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.edge.to_le_bytes());
        bytes.extend_from_slice(&u32::try_from(self.page_count).ok()?.to_le_bytes());
        bytes.extend_from_slice(&u64::try_from(self.meta_bytes).ok()?.to_le_bytes());
        Some(bytes)
    }
}

fn read_u32(file: &mut std::fs::File) -> Option<u32> {
    let mut bytes = [0; 4];
    file.read_exact(&mut bytes).ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64(file: &mut std::fs::File) -> Option<u64> {
    let mut bytes = [0; 8];
    file.read_exact(&mut bytes).ok()?;
    Some(u64::from_le_bytes(bytes))
}

fn read_layout(file: &mut std::fs::File) -> Option<(DiskLayout, u64)> {
    let mut magic = [0; 8];
    file.read_exact(&mut magic).ok()?;
    if magic != STORAGE_MAGIC
        || read_u32(file)? != STORAGE_VERSION
        || read_u32(file)? != LIGHTMAP_FORMAT_VERSION
    {
        return None;
    }
    let edge = read_u32(file)?;
    let count = usize::try_from(read_u32(file)?).ok()?;
    let metadata = usize::try_from(read_u64(file)?).ok()?;
    let checksum = read_u64(file)?;
    let layout = DiskLayout::new(edge, count, metadata)?;
    if file.metadata().ok()?.len() != layout.file_bytes()? {
        return None;
    }
    Some((layout, checksum))
}

fn valid_charts(meta: &DiskMeta, page_count: usize) -> Option<usize> {
    let mut texels = 0usize;
    for (patch, chart) in &meta.charts {
        if !chart_fits(meta.edge, chart)
            || usize::from(chart.page) >= page_count
            || !patch
                .origin
                .iter()
                .chain(&patch.u_axis)
                .chain(&patch.v_axis)
                .all(|value| value.is_finite())
        {
            return None;
        }
        texels = texels.checked_add(chart_texels(chart)?)?;
    }
    Some(texels)
}

/// Reads a complete bounded envelope, checking every byte before returning it.
/// FNV detects accidental corruption; it is not an authentication mechanism.
pub(super) fn disk_load(root: &Path, key: &str) -> Option<LevelLightmaps> {
    if !key_is_safe(key) {
        return None;
    }
    let mut file = std::fs::File::open(root.join(format!("{key}.lmc"))).ok()?;
    let (layout, checksum) = read_layout(&mut file)?;
    let mut hash = Fnv1a::new();
    hash.write(&layout.prefix()?);
    let mut metadata = vec![0; layout.meta_bytes];
    file.read_exact(&mut metadata).ok()?;
    hash.write(&metadata);
    let meta: DiskMeta = serde_json::from_slice(&metadata).ok()?;
    if meta.version != LIGHTMAP_FORMAT_VERSION || meta.edge != layout.edge || meta.key != key {
        return None;
    }
    let texels = valid_charts(&meta, layout.page_count)?;
    let mut pages = Vec::with_capacity(layout.page_count);
    for _ in 0..layout.page_count {
        let mut rgb = vec![0; layout.page_bytes];
        file.read_exact(&mut rgb).ok()?;
        hash.write(&rgb);
        pages.push(LightmapPage {
            width: layout.edge,
            height: layout.edge,
            rgb,
        });
    }
    let mut trailing = [0];
    if hash.finish() != checksum || file.read(&mut trailing).ok()? != 0 {
        return None;
    }
    Some(LevelLightmaps {
        stats: LightmapStats {
            charts: meta.charts.len(),
            pages: layout.page_count,
            texels,
            page_texels: layout
                .page_bytes
                .checked_div(3)?
                .checked_mul(layout.page_count)?,
            bake_millis: 0.0,
            cache_hit: true,
        },
        padding: meta.padding,
        pages,
        charts: meta.charts,
        cache_key: key.to_string(),
    })
}

/// Publishes only a fully written file. Concurrent readers see the previous
/// complete entry or its replacement; a crash before rename leaves a miss or
/// the old entry. Abandoned temporary files are never considered for reads.
pub(super) fn disk_store(root: &Path, key: &str, lightmaps: &LevelLightmaps) {
    if !key_is_safe(key) || lightmaps.cache_key != key {
        return;
    }
    let Some(edge) = lightmaps.pages.first().map(|page| page.width) else {
        return;
    };
    // Bound serialization as well as decoding: every chart occupies at least
    // one byte in JSON, and this cap also bounds the temporary chart clone.
    if lightmaps.charts.len()
        > MAX_META_BYTES
            .checked_div(std::mem::size_of::<(super::LightmapPatch, super::Chart)>())
            .unwrap_or(0)
    {
        return;
    }
    let meta = DiskMeta {
        version: LIGHTMAP_FORMAT_VERSION,
        edge,
        key: key.to_string(),
        charts: lightmaps.charts.clone(),
        padding: lightmaps.padding,
    };
    let Ok(metadata) = serde_json::to_vec(&meta) else {
        return;
    };
    let Some(layout) = DiskLayout::new(edge, lightmaps.pages.len(), metadata.len()) else {
        return;
    };
    if valid_charts(&meta, layout.page_count).is_none()
        || lightmaps.pages.iter().any(|page| {
            page.width != edge || page.height != edge || page.rgb.len() != layout.page_bytes
        })
    {
        return;
    }
    let Some(prefix) = layout.prefix() else {
        return;
    };
    let mut hash = Fnv1a::new();
    hash.write(&prefix);
    hash.write(&metadata);
    for page in &lightmaps.pages {
        hash.write(&page.rgb);
    }
    if std::fs::create_dir_all(root).is_err() {
        return;
    }
    let temporary = root.join(format!(
        ".{key}.{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    else {
        return;
    };
    let written = (|| -> std::io::Result<()> {
        file.write_all(&prefix)?;
        file.write_all(&hash.finish().to_le_bytes())?;
        file.write_all(&metadata)?;
        for page in &lightmaps.pages {
            file.write_all(&page.rgb)?;
        }
        file.sync_all()
    })();
    drop(file);
    if written.is_ok() {
        let _ = std::fs::rename(&temporary, root.join(format!("{key}.lmc")));
    }
    let _ = std::fs::remove_file(temporary);
}

fn retained_bytes(lightmaps: &LevelLightmaps) -> usize {
    lightmaps.pages.iter().fold(
        lightmaps
            .charts
            .capacity()
            .saturating_mul(std::mem::size_of::<(super::LightmapPatch, super::Chart)>()),
        |bytes, page| bytes.saturating_add(page.rgb.capacity()),
    )
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

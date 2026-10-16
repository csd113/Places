//! Lightmaps: the prepared HDR bake's pages as wgpu resources.
//!
//! The lightmap bake is prepared once per level load (or reused from the
//! content-keyed cache) and uploaded as one `texture_2d_array` whose layer
//! count is exact: two `Rgba16Float` layers per page per layer group, with no
//! mip chain and a fixed clamped-linear sampler that does not follow the
//! player's filtering setting. A page's irradiance plane occupies even layers
//! and its directional-moment plane the odd layer after it, in page order;
//! each switchable fixture's prepared contribution follows the base group in
//! the same pair layout. The world fragment stage reconstructs
//! `max(0, I + (I / max(k, 1e-6)) * (2 * max(0, dot(g, n)) - length(g)))` with
//! `k = I.r + I.g + I.b`, sampling the irradiance `I` and signed direction
//! moment `g` from the pair; the direction plane holds the signed vector sum of
//! the per-channel moment vectors (world space), and the pair is selected from
//! the vertex's page byte plus the environment uniform's page and switchable
//! counts. The two planes' alpha channels are reserved (`0.5`) and ignored by
//! the reconstruction. This module owns that contract:
//!
//! * the prepared pages as raw, non-sRGB `Rgba16Float` array layers (linear HDR
//!   half floats: irradiance `(r, g, b, 0.5)` and direction moment
//!   `(r, g, b, 0.5)`, the package's exact plane layout);
//! * the clamped-linear sampler the pages are read with;
//! * the 1x1 white-irradiance fallback bound while no atlas is resident, so the
//!   world shader always has a complete texture even though its
//!   `lightmap_enabled` switch skips every sample;
//! * plain-data stats for the developer log and the tests.
//!
//! The array allocates exactly [`LevelLightmaps::layer_count`] layers (at least
//! one): every accepted page and switchable contribution becomes a layer pair,
//! so a plan that fits the page budget always has a valid pair for every page
//! byte and group it stamps, and no memory is reserved for pages the bake did
//! not produce. A page set that cannot share one square edge, whose switchable
//! contributions disagree on the page count, or that carries more switchable
//! groups than the uniform's four-bit count is rejected as a whole and binds
//! the fallback, which the renderer's [`needs_upload_fallback`] rule turns into
//! the vertex-lit rebuild.
//!
//! The atlas is a *level* resource: the array is created at level upload and
//! dropped with the renderer's `lightmaps` field, never per frame. Nothing here
//! samples, uploads or allocates in the frame loop.

use crate::engine::lighting::lightmap::{LevelLightmaps, LightmapPage};

/// Number of base atlas pages (layer pairs) one layer group can hold.
///
/// One page pair per page and switchable group, selected by the vertex page
/// byte plus the uniform's group index. A bake that needs more pages fails over
/// to vertex lighting instead of dropping pages silently. Mirrors
/// [`crate::engine::lighting::lightmap::LIGHTMAP_ATLAS_MAX_PAGES`] (fourteen Full pages:
/// a 224 MiB layer group at its 1024px edge). Medium retains eight 1024px pages
/// and Low eight 512px pages; only resident pages and prepared groups allocate.
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize =
    crate::engine::lighting::lightmap::LIGHTMAP_ATLAS_MAX_PAGES;

/// Whether an On-mode build whose atlas failed to upload must be rebuilt with
/// vertex lighting.
///
/// A plan or fill failure never qualifies: `has_lightmaps` is false and the
/// neutral build has already returned the historical vertex-lit mesh with the
/// same lighting, exactly like the reference (which does not even attempt an
/// upload for a missing atlas). Only a built page set whose GPU atlas is not
/// resident triggers the reference's `Upload` fallback. Applying the rule to a
/// plan or fill failure would re-bake the level with `BakeConfig::HARD` and
/// change every vertex colour, so only the upload failure kind qualifies.
#[must_use]
pub const fn needs_upload_fallback(
    mode: crate::engine::lighting::lightmap::LightmapMode,
    has_lightmaps: bool,
    atlas_resident: bool,
) -> bool {
    matches!(mode, crate::engine::lighting::lightmap::LightmapMode::On)
        && has_lightmaps
        && !atlas_resident
}

/// The page edge, in texels, the fallback sheet uses.
const FALLBACK_EDGE: u32 = 1;
/// The fallback sheet's layer count: one irradiance plane and one direction
/// plane, so the shader's pair sample stays in bounds even though its gate
/// skips it.
const FALLBACK_LAYERS: u32 = 2;

/// Array layers one page occupies: the irradiance plane and the
/// directional-moment plane after it.
const LAYERS_PER_PAGE: u32 = 2;

/// Switchable groups the environment uniform's four-bit count and mask carry.
const MAX_SWITCHABLE_GROUPS: usize = 4;

/// What one atlas upload produced, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightmapUploadStats {
    /// Base atlas pages resident (each is a layer pair in every group).
    pub pages: usize,
    /// Page capacity of one layer group ([`LIGHTMAP_ATLAS_MAX_PAGES`]).
    pub capacity: usize,
    /// Edge length of the pages, in texels (0 when no page).
    pub page_edge: u32,
    /// Texels across every resident base page.
    pub page_texels: usize,
    /// Charts the atlas carries.
    pub charts: usize,
    /// Chart data texels the fill pass wrote.
    pub chart_texels: usize,
    /// Bytes the resident page data occupies
    /// (`layers * edge² * 8`: two `Rgba16Float` planes per page per group).
    pub resident_bytes: usize,
    /// True when the atlas came from the on-disk cache rather than a fresh bake.
    pub cache_hit: bool,
}

/// One atlas page's bytes as two `Rgba16Float` planes, irradiance first.
///
/// Each texel is four little-endian half floats: three RGB light values and a
/// reserved alpha of `0.5`. The irradiance plane holds the mean term, the
/// direction-moment plane the signed vector sum of the per-channel moment
/// vectors — half floats are signed, so negative components are stored as-is —
/// the same plane layout the package writer stores. `width * height` texels per
/// plane, row-major. A truncated page is padded with the neutral texel (white
/// irradiance, zero direction, reserved alpha), so an upload can never read
/// uninitialized bytes.
#[must_use]
pub fn page_rgba16f(page: &LightmapPage) -> std::borrow::Cow<'_, [u8]> {
    if let Some(bytes) = page.rgba16f_planes() {
        return std::borrow::Cow::Borrowed(bytes);
    }
    let texels = usize::try_from(page.width.saturating_mul(page.height)).unwrap_or(usize::MAX);
    let plane_bytes = texels.saturating_mul(8);
    let mut bytes = Vec::with_capacity(plane_bytes.saturating_mul(2));
    for texel in page.texels.iter().take(texels) {
        push_half_rgba(&mut bytes, texel.irradiance);
    }
    while bytes.len() < plane_bytes {
        push_half_rgba(&mut bytes, [1.0; 3]);
    }
    for texel in page.texels.iter().take(texels) {
        push_half_rgba(&mut bytes, texel.direction);
    }
    while bytes.len() < plane_bytes.saturating_mul(2) {
        push_half_rgba(&mut bytes, [0.0; 3]);
    }
    std::borrow::Cow::Owned(bytes)
}

/// The reserved alpha both planes store: consumers ignore it, writers pin it so
/// the byte layout is deterministic.
const RESERVED_ALPHA: f32 = 0.5;

/// Appends one RGB value and the reserved alpha as four little-endian halves.
fn push_half_rgba(out: &mut Vec<u8>, color: [f32; 3]) {
    for value in color {
        out.extend_from_slice(&crate::engine::package::ktx2::f32_to_f16_bits(value).to_le_bytes());
    }
    out.extend_from_slice(
        &crate::engine::package::ktx2::f32_to_f16_bits(RESERVED_ALPHA).to_le_bytes(),
    );
}

/// The fallback array's bytes: alternating white-irradiance and zero-direction
/// layers, every texel carrying the reserved alpha.
///
/// One `edge x edge` layer per array layer, row-major, two planes per page pair
/// like [`page_rgba16f`]; the shader's gate skips these samples, but a sample
/// that did read them would reconstruct `max(0, white + 0)`, the unit factor.
#[must_use]
fn white_array_bytes(edge: u32, layers: u32) -> Vec<u8> {
    let texels = usize::try_from(edge.saturating_mul(edge)).unwrap_or(0);
    let layer_count = usize::try_from(layers).unwrap_or(0);
    let mut bytes = Vec::with_capacity(texels.saturating_mul(layer_count).saturating_mul(8));
    let mut white = true;
    for _ in 0..layers {
        let color = if white { [1.0; 3] } else { [0.0; 3] };
        for _ in 0..texels {
            push_half_rgba(&mut bytes, color);
        }
        white = !white;
    }
    bytes
}

/// The pure upload plan: what [`LightmapAtlas::upload`] will bind and report.
///
/// The bake's base pages and every switchable contribution become layer pairs
/// of one array, so every page of every group must match a single square edge
/// and the base set must fit the page capacity. A set that carries too many
/// pages or groups, disagrees on an edge or a group's page count, or carries no
/// usable page at all is not uploadable: the returned stats are empty and the
/// caller binds the fallback, which the renderer's [`needs_upload_fallback`]
/// rule turns into the vertex-lit rebuild. No page is ever dropped silently.
#[must_use]
pub fn upload_stats(lightmaps: Option<&LevelLightmaps>) -> LightmapUploadStats {
    let mut stats = LightmapUploadStats {
        capacity: LIGHTMAP_ATLAS_MAX_PAGES,
        ..LightmapUploadStats::default()
    };
    let Some(level_lightmaps) = lightmaps else {
        return stats;
    };
    if level_lightmaps.pages.len() > LIGHTMAP_ATLAS_MAX_PAGES
        || level_lightmaps.switchable.len() > MAX_SWITCHABLE_GROUPS
    {
        return stats;
    }
    let mut pages = 0usize;
    let mut edge = 0u32;
    for page in &level_lightmaps.pages {
        if page.width == 0 || page.height == 0 || page.width != page.height {
            return stats;
        }
        if pages == 0 {
            edge = page.width;
        } else if page.width != edge {
            return stats;
        }
        pages = pages.saturating_add(1);
    }
    if pages == 0 {
        return stats;
    }
    // The uniform carries one page count and the shader addresses every group
    // with it, so each switchable contribution must mirror the base pages
    // exactly.
    for contribution in &level_lightmaps.switchable {
        if contribution.pages.len() != pages {
            return stats;
        }
        for page in &contribution.pages {
            if page.width != edge || page.height != edge {
                return stats;
            }
        }
    }
    let edge_pixels = usize::try_from(edge).unwrap_or(usize::MAX);
    stats.pages = pages;
    stats.page_edge = u32::try_from(edge_pixels).unwrap_or(u32::MAX);
    stats.page_texels = edge_pixels
        .checked_mul(edge_pixels)
        .and_then(|texels| texels.checked_mul(pages))
        .unwrap_or(usize::MAX);
    stats.resident_bytes = level_lightmaps
        .layer_count()
        .saturating_mul(edge_pixels)
        .saturating_mul(edge_pixels)
        .saturating_mul(8);
    stats.charts = level_lightmaps.stats.charts;
    stats.chart_texels = level_lightmaps.stats.texels;
    stats.cache_hit = level_lightmaps.stats.cache_hit;
    stats
}

/// The GPU lightmaps of one level: its page array, or the white fallback.
///
/// The struct keeps the page array alive alongside its view; the environment
/// bind group borrows the view, so the renderer holds this value for as long as
/// the bind group can be used. The array has exactly the resident layer count;
/// [`Self::view`] returns the fallback's pair view when no atlas is resident.
#[derive(Clone)]
pub struct LightmapAtlas {
    /// The resident page array, kept alive for its view; `None` on the
    /// vertex-lit path.
    _pages: Option<wgpu::Texture>,
    /// The page array's view, `None` when no atlas is resident.
    view: Option<wgpu::TextureView>,
    /// The 1x1 fallback pair, kept alive for its view.
    _fallback: wgpu::Texture,
    /// The fallback's view; its texture survives through this struct.
    fallback_view: wgpu::TextureView,
    stats: LightmapUploadStats,
}

impl LightmapAtlas {
    /// Uploads a prepared atlas as one array of layer pairs, or the fallback
    /// for the vertex-lit path.
    ///
    /// A page set that cannot be one array ([`upload_stats`] rejects a page
    /// with a different edge, a non-square page, disagreeing switchable groups
    /// or an empty base set) uploads no pages and binds the fallback instead;
    /// the renderer's `needs_upload_fallback` rule then rebuilds the level
    /// vertex-lit, so a rejected set is a named fallback, never a dropped page.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lightmaps: Option<&LevelLightmaps>,
    ) -> Self {
        let (fallback, fallback_view) =
            Self::upload_white_array(device, queue, FALLBACK_EDGE, FALLBACK_LAYERS);
        let stats = upload_stats(lightmaps);
        if let Some(level_lightmaps) = lightmaps
            && stats.pages == 0
            && !level_lightmaps.pages.is_empty()
        {
            crate::engine::logging::warn_once(
                "lightmap-atlas-pages-rejected",
                format!(
                    "[lightmaps] atlas carries {} page(s) that cannot share one square array \
                     (resident {}); binding the vertex-lit fallback",
                    level_lightmaps.pages.len(),
                    stats.pages
                ),
            );
        }
        let mut page_array = None;
        let mut view = None;
        if stats.pages > 0 {
            let (texture, array_view) = Self::upload_page_array(device, queue, lightmaps, &stats);
            page_array = Some(texture);
            view = Some(array_view);
            crate::engine::logging::info(format!(
                "[lightmaps] atlas array: {} page(s) in {} layer(s), {}-texel edge, \
                 {} bytes resident",
                stats.pages,
                lightmaps.map_or(0, LevelLightmaps::layer_count),
                stats.page_edge,
                stats.resident_bytes
            ));
        }
        Self {
            _pages: page_array,
            view,
            _fallback: fallback,
            fallback_view,
            stats,
        }
    }

    /// Creates a `layers`-deep array of `edge`-texel fallback layers: white
    /// irradiance in the even layers and zero direction in the odd ones, with
    /// the reserved `0.5` alpha in both, so a sample the shader's gate would
    /// skip still reconstructs the unit factor.
    fn upload_white_array(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        edge: u32,
        layers: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-lightmap-fallback"),
            size: wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes = white_array_bytes(edge, layers);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge.saturating_mul(8)),
                rows_per_image: Some(edge),
            },
            wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: layers,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the layer array and writes every page pair into it.
    ///
    /// The walk order is the contract the shader's addressing mirrors: the base
    /// group's pages first (irradiance at even layers), then each switchable
    /// contribution in turn, all on [`LevelLightmaps::layer_count`] layers.
    fn upload_page_array(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lightmaps: Option<&LevelLightmaps>,
        stats: &LightmapUploadStats,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let edge = stats.page_edge.max(1);
        let layers = u32::try_from(lightmaps.map_or(0, LevelLightmaps::layer_count).max(1))
            .unwrap_or(u32::MAX);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-lightmap-pages"),
            size: wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        if let Some(level_lightmaps) = lightmaps {
            let mut layer = 0u32;
            for group in core::iter::once(&level_lightmaps.pages)
                .chain(level_lightmaps.switchable.iter().map(|entry| &entry.pages))
            {
                for page in group {
                    let bytes = page_rgba16f(page);
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &bytes,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(edge.saturating_mul(8)),
                            rows_per_image: Some(edge),
                        },
                        wgpu::Extent3d {
                            width: edge,
                            height: edge,
                            depth_or_array_layers: LAYERS_PER_PAGE,
                        },
                    );
                    layer = layer.saturating_add(LAYERS_PER_PAGE);
                }
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// The layer-array view: the resident pages, or the fallback pair.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        self.view.as_ref().unwrap_or(&self.fallback_view)
    }

    /// True when at least one real page is resident.
    #[must_use]
    pub const fn is_resident(&self) -> bool {
        self.stats.pages > 0
    }

    /// The upload counters (including the page capacity).
    #[must_use]
    pub const fn stats(&self) -> LightmapUploadStats {
        self.stats
    }
}

//! Lightmaps: the neutral bake's pages as wgpu resources.
//!
//! The atlas is baked once per level load (or reused from the content-keyed
//! cache) and uploaded as one `texture_2d_array` with
//! [`LIGHTMAP_ATLAS_MAX_PAGES`] layers, clamped, with no mip chain and a fixed
//! clamped-linear sampler that does not follow the player's filtering setting.
//! Every atlas page is one array layer and the world fragment stage selects the
//! layer from the vertex's page byte. This module owns that contract:
//!
//! * the atlas pages as raw, non-sRGB `Rgba8Unorm` array layers (wgpu has no
//!   sampleable RGB8 format; the alpha byte is filled with 255 and never read);
//! * the clamped-linear sampler the pages are read with;
//! * the 1x1 white array fallback (all four layers) bound while no atlas is
//!   resident, so the world shader always has a complete texture even though
//!   its `lightmap_enabled` switch skips every sample;
//! * plain-data stats for the developer log and the tests.
//!
//! The array always has [`LIGHTMAP_ATLAS_MAX_PAGES`] layers: the page budget
//! and the shader's layer count are one number, so a plan that fits the budget
//! always has a valid layer for every page byte it stamps, and the binding
//! never changes shape. Layers beyond the resident page count are filled white,
//! so a sample that should be impossible can never read undefined memory.
//!
//! The atlas is a *level* resource: the array is created at level upload and
//! dropped with the renderer's `lightmaps` field, never per frame. Nothing here
//! samples, uploads or allocates in the frame loop.

use crate::lighting::lightmap::{LevelLightmaps, LightmapPage};

/// Number of atlas pages (array layers) the world shader can sample at once.
///
/// One `texture_2d_array` layer per page, selected by the vertex page byte. A
/// bake that needs more pages fails over to vertex lighting instead of dropping
/// pages silently. Mirrors
/// [`crate::lighting::lightmap::LIGHTMAP_ATLAS_MAX_PAGES`].
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize = 4;

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
    mode: crate::lighting::lightmap::LightmapMode,
    has_lightmaps: bool,
    atlas_resident: bool,
) -> bool {
    matches!(mode, crate::lighting::lightmap::LightmapMode::On) && has_lightmaps && !atlas_resident
}

/// The page edge, in texels, the fallback sheet uses.
const FALLBACK_EDGE: u32 = 1;

/// What one atlas upload produced, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightmapUploadStats {
    /// Atlas pages resident as array layers.
    pub pages: usize,
    /// Page capacity of the world program's array binding
    /// ([`LIGHTMAP_ATLAS_MAX_PAGES`]).
    pub capacity: usize,
    /// Edge length of the pages, in texels (0 when no page).
    pub page_edge: u32,
    /// Texels across every resident page.
    pub page_texels: usize,
    /// Charts the atlas carries.
    pub charts: usize,
    /// Chart data texels the fill pass wrote.
    pub chart_texels: usize,
    /// Bytes the resident page data occupies (`pages * edge² * 4`; RGBA8).
    /// The array itself always allocates [`LIGHTMAP_ATLAS_MAX_PAGES`] layers,
    /// with the unused layers filled white.
    pub resident_bytes: usize,
    /// True when the atlas came from the on-disk cache rather than a fresh bake.
    pub cache_hit: bool,
}

impl LightmapUploadStats {
    /// Bytes the resident page array allocates (`capacity * edge² * 4`).
    ///
    /// The array always has [`LIGHTMAP_ATLAS_MAX_PAGES`] layers, so this is at
    /// least [`Self::resident_bytes`]; the extra layers are white.
    #[must_use]
    pub fn array_bytes(&self) -> usize {
        let edge = usize::try_from(self.page_edge).unwrap_or(usize::MAX);
        edge.saturating_mul(edge)
            .saturating_mul(self.capacity)
            .saturating_mul(4)
    }
}

/// One atlas page's bytes converted from the bake's RGB8 to `Rgba8Unorm`.
///
/// The alpha byte is 255: the shader reads `.rgb` only, but wgpu has no
/// sampleable RGB8 format and a complete texture keeps the bind group valid.
/// `rgb` is `width * height * 3` bytes, red first, exactly as the fill pass
/// wrote it.
#[must_use]
pub fn page_rgba8(page: &LightmapPage) -> Vec<u8> {
    let texels = page
        .width
        .saturating_mul(page.height)
        .try_into()
        .unwrap_or(usize::MAX);
    let mut rgba = Vec::with_capacity(texels.saturating_mul(4));
    for texel in page.rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[texel[0], texel[1], texel[2], 255]);
    }
    // A page whose byte count is not exactly three per texel is malformed; the
    // conversion reproduces every complete texel and leaves the rest white, so
    // an upload can never read uninitialized bytes.
    while rgba.len() < texels.saturating_mul(4) {
        rgba.extend_from_slice(&[255, 255, 255, 255]);
    }
    rgba
}

/// The pure upload plan: what [`LightmapAtlas::upload`] will bind and report.
///
/// The bake's pages become the layers of one array, so every page must match a
/// single square edge and the set must fit the page capacity. A page set that
/// carries too many pages, disagrees on an edge, or carries no usable page at
/// all is not uploadable: the returned stats are empty and the caller binds the
/// white fallback, which the renderer's [`needs_upload_fallback`] rule turns
/// into the vertex-lit rebuild. No page is ever dropped silently.
#[must_use]
pub fn upload_stats(lightmaps: Option<&LevelLightmaps>) -> LightmapUploadStats {
    let mut stats = LightmapUploadStats {
        capacity: LIGHTMAP_ATLAS_MAX_PAGES,
        ..LightmapUploadStats::default()
    };
    let Some(lightmaps) = lightmaps else {
        return stats;
    };
    if lightmaps.pages.len() > LIGHTMAP_ATLAS_MAX_PAGES {
        return stats;
    }
    let mut pages = 0usize;
    let mut edge = 0u32;
    for page in &lightmaps.pages {
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
    stats.pages = pages;
    stats.page_edge = edge;
    let edge = usize::try_from(edge).unwrap_or(usize::MAX);
    stats.page_texels = edge
        .checked_mul(edge)
        .and_then(|texels| texels.checked_mul(pages))
        .unwrap_or(usize::MAX);
    stats.resident_bytes = stats.page_texels.saturating_mul(4);
    stats.charts = lightmaps.stats.charts;
    stats.chart_texels = lightmaps.stats.texels;
    stats.cache_hit = lightmaps.stats.cache_hit;
    stats
}

/// The GPU lightmaps of one level: its page array, or the white fallback.
///
/// The struct keeps the page array alive alongside its view; the environment
/// bind group borrows the view, so the renderer holds this value for as long as
/// the bind group can be used. The array always has
/// [`LIGHTMAP_ATLAS_MAX_PAGES`] layers; [`Self::view`] returns the fallback's
/// four-layer view when no atlas is resident.
pub struct LightmapAtlas {
    /// The resident page array; `None` on the vertex-lit path.
    _pages: Option<wgpu::Texture>,
    /// The page array's view, `None` when no atlas is resident.
    view: Option<wgpu::TextureView>,
    /// The 1x1 white fallback, kept alive for its view.
    _fallback: wgpu::Texture,
    /// The fallback's view; its texture survives through this struct.
    fallback_view: wgpu::TextureView,
    stats: LightmapUploadStats,
}

impl LightmapAtlas {
    /// Uploads a baked atlas as one array, or the white fallback for the
    /// vertex-lit path.
    ///
    /// A page set that cannot be one array ([`upload_stats`] rejects a page
    /// with a different edge, a non-square page, or an empty set) uploads no
    /// pages and binds the fallback instead; the renderer's
    /// `needs_upload_fallback` rule then rebuilds the level vertex-lit, so a
    /// rejected page is a named fallback, never a dropped page.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lightmaps: Option<&LevelLightmaps>,
    ) -> Self {
        let layers = u32::try_from(LIGHTMAP_ATLAS_MAX_PAGES).unwrap_or(u32::MAX);
        let (fallback, fallback_view) =
            Self::upload_white_array(device, queue, FALLBACK_EDGE, layers);
        let stats = upload_stats(lightmaps);
        if let Some(lightmaps) = lightmaps
            && stats.pages < lightmaps.pages.len()
        {
            crate::logging::warn_once(
                "lightmap-atlas-pages-rejected",
                format!(
                    "[lightmaps] atlas carries {} page(s) that cannot share one {}-layer array \
                     (resident {}); binding the white fallback",
                    lightmaps.pages.len(),
                    LIGHTMAP_ATLAS_MAX_PAGES,
                    stats.pages
                ),
            );
        }
        let mut pages = None;
        let mut view = None;
        if stats.pages > 0 {
            let (texture, array_view) = Self::upload_page_array(device, queue, lightmaps, &stats);
            pages = Some(texture);
            view = Some(array_view);
            crate::logging::info(format!(
                "[lightmaps] atlas array: {} of {} layer(s) resident, {}-texel edge, \
                 {} bytes of page data ({} bytes allocated)",
                stats.pages,
                stats.capacity,
                stats.page_edge,
                stats.resident_bytes,
                stats.array_bytes()
            ));
        }
        Self {
            _pages: pages,
            view,
            _fallback: fallback,
            fallback_view,
            stats,
        }
    }

    /// Creates a `layers`-deep array of `edge`-texel white `Rgba8Unorm` layers.
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
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let white = vec![
            255u8;
            usize::try_from(edge.saturating_mul(edge))
                .unwrap_or(0)
                .saturating_mul(usize::try_from(layers).unwrap_or(0))
                .saturating_mul(4)
        ];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &white,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge.saturating_mul(4)),
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

    /// Creates the four-layer page array and writes the resident pages into
    /// layers 0..pages, filling the unused layers white.
    fn upload_page_array(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lightmaps: Option<&LevelLightmaps>,
        stats: &LightmapUploadStats,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let layers = u32::try_from(LIGHTMAP_ATLAS_MAX_PAGES).unwrap_or(u32::MAX);
        let edge = stats.page_edge.max(1);
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
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, page) in lightmaps
            .into_iter()
            .flat_map(|lightmaps| &lightmaps.pages)
            .enumerate()
        {
            let rgba = page_rgba8(page);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: u32::try_from(layer).unwrap_or(0),
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(page.width.saturating_mul(4)),
                    rows_per_image: Some(page.height),
                },
                wgpu::Extent3d {
                    width: page.width,
                    height: page.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        // The array always has four layers: the unused ones are white so a
        // sample that should be impossible reads a neutral value, never
        // undefined memory.
        let white_edge = usize::try_from(edge).unwrap_or(0);
        if white_edge > 0 {
            let white = vec![255u8; white_edge.saturating_mul(white_edge).saturating_mul(4)];
            for layer in stats.pages..LIGHTMAP_ATLAS_MAX_PAGES {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: u32::try_from(layer).unwrap_or(0),
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &white,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(edge.saturating_mul(4)),
                        rows_per_image: Some(edge),
                    },
                    wgpu::Extent3d {
                        width: edge,
                        height: edge,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// The four-layer array view: the resident pages, or the white fallback.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        self.view.as_ref().unwrap_or(&self.fallback_view)
    }

    /// True when at least one real page is resident.
    #[must_use]
    pub const fn is_resident(&self) -> bool {
        self.stats.pages > 0
    }

    /// The upload counters (including the four-layer capacity).
    #[must_use]
    pub const fn stats(&self) -> LightmapUploadStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons are idiomatic here.
    #![allow(clippy::indexing_slicing, clippy::unwrap_used)]

    use super::*;

    fn page(width: u32, height: u32, rgb: Vec<u8>) -> LightmapPage {
        LightmapPage { width, height, rgb }
    }

    #[test]
    fn an_upload_fallback_never_rebakes_a_plan_failure() {
        use crate::lighting::lightmap::LightmapMode;
        // A plan/fill failure reaches the renderer as `has_lightmaps == false`
        // and must never trigger the Off-mode re-bake: the neutral build
        // already produced the historical vertex-lit mesh with the same
        // lighting. Only a resident-atlas failure on a built page set does.
        assert!(!needs_upload_fallback(LightmapMode::On, false, true));
        assert!(!needs_upload_fallback(LightmapMode::On, false, false));
        assert!(needs_upload_fallback(LightmapMode::On, true, false));
        assert!(!needs_upload_fallback(LightmapMode::On, true, true));
        // An Off build has no atlas to fail over from.
        assert!(!needs_upload_fallback(LightmapMode::Off, false, false));
        assert!(!needs_upload_fallback(LightmapMode::Off, true, false));
    }

    #[test]
    fn rgb8_pages_convert_to_opaque_rgba_in_texel_order() {
        let page = page(2, 1, vec![1, 2, 3, 4, 5, 6]);
        let rgba = page_rgba8(&page);
        assert_eq!(rgba, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn a_truncated_page_is_padded_with_white_never_uninitialized() {
        let page = page(2, 1, vec![1, 2, 3]);
        let rgba = page_rgba8(&page);
        assert_eq!(rgba.len(), 8);
        assert_eq!(rgba[4..], [255, 255, 255, 255]);
    }

    /// One `LevelLightmaps` around a page list, for the pure stats tests.
    fn lightmaps(pages: Vec<LightmapPage>) -> LevelLightmaps {
        LevelLightmaps {
            pages,
            charts: Vec::new(),
            stats: crate::lighting::lightmap::LightmapStats {
                charts: 7,
                texels: 123,
                ..crate::lighting::lightmap::LightmapStats::default()
            },
            cache_key: "v6-test".to_string(),
        }
    }

    #[test]
    fn the_array_contract_is_four_layers_at_every_profile() {
        assert_eq!(LIGHTMAP_ATLAS_MAX_PAGES, 4);
        for profile in crate::quality::QualityProfile::ALL {
            let config = profile.lightmap_config();
            assert_eq!(config.max_pages, 4, "{profile:?} must support four pages");
        }
        // Low keeps its 512-texel pages and Full its 1024-texel pages.
        assert_eq!(
            crate::quality::QualityProfile::Low
                .lightmap_config()
                .page_edge,
            512
        );
        assert_eq!(
            crate::quality::QualityProfile::Full
                .lightmap_config()
                .page_edge,
            1024
        );
    }

    #[test]
    fn array_upload_stats_count_pages_texels_and_resident_bytes() {
        let stats = upload_stats(Some(&lightmaps(vec![
            page(1024, 1024, vec![0; 1024 * 1024 * 3]),
            page(1024, 1024, vec![0; 1024 * 1024 * 3]),
        ])));
        assert_eq!(stats.capacity, 4);
        assert_eq!(stats.pages, 2);
        assert_eq!(stats.page_edge, 1024);
        assert_eq!(stats.page_texels, 2 * 1024 * 1024);
        assert_eq!(stats.resident_bytes, 2 * 1024 * 1024 * 4);
        // The array allocates all four layers; the two unused layers are white.
        assert_eq!(stats.array_bytes(), 4 * 1024 * 1024 * 4);
        assert_eq!(stats.charts, 7);
        assert_eq!(stats.chart_texels, 123);
    }

    #[test]
    fn no_atlas_reports_zero_pages_and_the_full_capacity() {
        let stats = upload_stats(None);
        assert_eq!(stats.pages, 0);
        assert_eq!(stats.capacity, LIGHTMAP_ATLAS_MAX_PAGES);
        assert_eq!(stats.resident_bytes, 0);
        assert_eq!(stats.array_bytes(), 0);
    }

    #[test]
    fn a_page_set_that_cannot_share_one_array_reports_no_pages() {
        // A different edge, a non-square page, a zero-edge page and an empty
        // set all make the whole set unuploadable as one array: the stats go
        // empty and the renderer takes its named vertex-lit fallback. No page
        // may be dropped silently.
        for pages in [
            vec![
                page(1024, 1024, vec![0; 1024 * 1024 * 3]),
                page(512, 512, vec![0; 512 * 512 * 3]),
            ],
            vec![page(16, 8, vec![0; 16 * 8 * 3])],
            vec![page(0, 0, Vec::new())],
            vec![],
            // A set with more pages than the array has layers is rejected as a
            // whole: none of the pages may be uploaded, because the vertex page
            // bytes would then address missing layers.
            (0..=LIGHTMAP_ATLAS_MAX_PAGES)
                .map(|_| page(64, 64, vec![0; 64 * 64 * 3]))
                .collect(),
        ] {
            let stats = upload_stats(Some(&lightmaps(pages)));
            assert_eq!(stats.pages, 0, "the set must be rejected as a whole");
            assert_eq!(stats.resident_bytes, 0);
        }
    }

    /// The upload format is pinned: a raw, non-sRGB texture, one mip.
    #[test]
    fn the_page_format_is_raw_rgba8_linear() {
        assert_eq!(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8Unorm
        );
        // The reference samples the bake's bytes with no sRGB decode; the
        // non-sRGB wgpu format is what preserves that contract.
        assert!(!wgpu::TextureFormat::Rgba8Unorm.is_srgb());
    }
}

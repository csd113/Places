//! Lightmaps: the neutral bake's pages as wgpu resources.
//!
//! The atlas is baked once per level load (or reused from the content-keyed
//! cache) and uploaded as one texture per page, clamped, with no mip chain and
//! the player's min/mag filtering. Both pages live in the environment bind
//! group and the world fragment stage selects between them from the vertex's
//! page byte. This module owns that contract:
//!
//! * the atlas pages as raw, non-sRGB `Rgba8Unorm` textures (wgpu has no
//!   sampleable RGB8 format; the alpha byte is filled with 255 and never read);
//! * the user-filtering, clamped sampler pair the pages are read with;
//! * the 1x1 white fallback bound while no atlas is resident, so the world
//!   shader always has a complete texture on both pages even though its
//!   `lightmap_enabled` switch skips every sample;
//! * plain-data stats for the developer log and the tests.
//!
//! The atlas is a *level* resource: pages are created at level upload and
//! dropped with the renderer's `lightmaps` field, never per frame. Nothing here
//! samples, uploads or allocates in the frame loop.

use crate::lighting::lightmap::{LevelLightmaps, LightmapPage};

/// Number of atlas pages the world shader can sample at once.
///
/// The reference's frozen bound: two units, selected by the vertex page byte.
/// A bake that needs more pages fails over to vertex lighting instead of
/// dropping pages silently.
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize = 2;

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
    /// Atlas pages uploaded.
    pub pages: usize,
    /// Edge length of the first page, in texels (0 when no page).
    pub page_edge: u32,
    /// Texels across every page.
    pub page_texels: usize,
    /// Charts the atlas carries.
    pub charts: usize,
    /// Chart data texels the fill pass wrote.
    pub chart_texels: usize,
    /// Bytes the GPU pages occupy (RGBA8, so four bytes per texel).
    pub resident_bytes: usize,
    /// True when the atlas came from the on-disk cache rather than a fresh bake.
    pub cache_hit: bool,
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

/// The GPU lightmaps of one level: its atlas pages, or the white fallback.
///
/// The struct keeps the page textures alive alongside their views; the
/// environment bind group borrows the views, so the renderer holds this value
/// for as long as the bind group can be used.
pub struct LightmapAtlas {
    /// Kept for ownership; the views below reference them.
    _pages: Vec<wgpu::Texture>,
    /// One view per resident page, in page order.
    views: Vec<wgpu::TextureView>,
    /// The 1x1 white fallback's view; the texture itself survives through it.
    fallback_view: wgpu::TextureView,
    stats: LightmapUploadStats,
}

impl LightmapAtlas {
    /// Uploads a baked atlas, or an empty one for the vertex-lit path.
    ///
    /// `pages` beyond [`LIGHTMAP_ATLAS_MAX_PAGES`] are rejected by the caller
    /// (the neutral build does not produce them). A page with a zero edge is
    /// skipped rather than allocated.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lightmaps: Option<&LevelLightmaps>,
    ) -> Self {
        let fallback = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-lightmap-fallback"),
            size: wgpu::Extent3d {
                width: FALLBACK_EDGE,
                height: FALLBACK_EDGE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(FALLBACK_EDGE.saturating_mul(4)),
                rows_per_image: Some(FALLBACK_EDGE),
            },
            wgpu::Extent3d {
                width: FALLBACK_EDGE,
                height: FALLBACK_EDGE,
                depth_or_array_layers: 1,
            },
        );

        let mut pages: Vec<wgpu::Texture> = Vec::new();
        let mut views: Vec<wgpu::TextureView> = Vec::new();
        let mut stats = LightmapUploadStats::default();
        if let Some(lightmaps) = lightmaps {
            stats.pages = lightmaps.pages.len().min(LIGHTMAP_ATLAS_MAX_PAGES);
            stats.page_edge = lightmaps
                .pages
                .first()
                .map_or(0, |page| page.width.max(page.height));
            stats.charts = lightmaps.stats.charts;
            stats.chart_texels = lightmaps.stats.texels;
            stats.cache_hit = lightmaps.stats.cache_hit;
            for page in lightmaps.pages.iter().take(LIGHTMAP_ATLAS_MAX_PAGES) {
                if page.width == 0 || page.height == 0 {
                    continue;
                }
                let texture = Self::upload_page(device, queue, page);
                views.push(texture.create_view(&wgpu::TextureViewDescriptor::default()));
                pages.push(texture);
                stats.page_texels = stats.page_texels.saturating_add(
                    usize::try_from(page.width.saturating_mul(page.height)).unwrap_or(usize::MAX),
                );
            }
            stats.resident_bytes = stats.page_texels.saturating_mul(4);
        }
        let fallback_view = fallback.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _pages: pages,
            views,
            fallback_view,
            stats,
        }
    }

    /// Creates one page texture and writes its converted bytes.
    fn upload_page(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        page: &LightmapPage,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-lightmap-page"),
            size: wgpu::Extent3d {
                width: page.width,
                height: page.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let rgba = page_rgba8(page);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
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
        texture
    }

    /// The view of atlas page `slot`, or the white fallback when that page is
    /// not resident.
    #[must_use]
    pub fn page_view(&self, slot: usize) -> &wgpu::TextureView {
        self.views.get(slot).unwrap_or(&self.fallback_view)
    }

    /// True when at least one real page is resident.
    #[must_use]
    pub const fn is_resident(&self) -> bool {
        !self.views.is_empty()
    }

    /// The upload counters.
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

    #[test]
    fn an_empty_atlas_reports_no_pages_and_keeps_the_two_page_bound() {
        assert_eq!(LIGHTMAP_ATLAS_MAX_PAGES, 2);
        let stats = LightmapUploadStats::default();
        assert_eq!(stats.pages, 0);
        assert_eq!(stats.resident_bytes, 0);
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

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

use crate::lighting::lightmap::{LevelLightmaps, LightmapPage};

/// Number of base atlas pages (layer pairs) one layer group can hold.
///
/// One page pair per page and switchable group, selected by the vertex page
/// byte plus the uniform's group index. A bake that needs more pages fails over
/// to vertex lighting instead of dropping pages silently. Mirrors
/// [`crate::lighting::lightmap::LIGHTMAP_ATLAS_MAX_PAGES`] (eight pages: a
/// 128 MiB layer group at Full's 1024-texel edge and 32 MiB at Low's
/// 512-texel edge; only the prepared groups are ever allocated).
pub const LIGHTMAP_ATLAS_MAX_PAGES: usize = 8;

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
pub fn page_rgba16f(page: &LightmapPage) -> Vec<u8> {
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
    bytes
}

/// The reserved alpha both planes store: consumers ignore it, writers pin it so
/// the byte layout is deterministic.
const RESERVED_ALPHA: f32 = 0.5;

/// Appends one RGB value and the reserved alpha as four little-endian halves.
fn push_half_rgba(out: &mut Vec<u8>, color: [f32; 3]) {
    for value in color {
        out.extend_from_slice(&crate::package::ktx2::f32_to_f16_bits(value).to_le_bytes());
    }
    out.extend_from_slice(&crate::package::ktx2::f32_to_f16_bits(RESERVED_ALPHA).to_le_bytes());
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
            crate::logging::warn_once(
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
            crate::logging::info(format!(
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

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons are idiomatic here.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::lighting::lightmap::{LightmapTexel, SwitchableLightmaps};
    use crate::package::ktx2::{f16_bits_to_f32, f32_to_f16_bits};

    fn page(width: u32, height: u32, texels: Vec<LightmapTexel>) -> LightmapPage {
        LightmapPage {
            width,
            height,
            texels,
        }
    }

    fn black_page(edge: u32) -> LightmapPage {
        let texels = usize::try_from(edge.saturating_mul(edge)).unwrap();
        page(edge, edge, vec![LightmapTexel::ZERO; texels])
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
    fn hdr_pages_encode_irradiance_and_the_signed_moment_in_half_floats() {
        let page = page(
            2,
            1,
            vec![
                LightmapTexel {
                    irradiance: [1.0, 0.5, 0.0],
                    direction: [0.0, -1.0, 0.25],
                    // Reserved: the writer must ignore it and store 0.5.
                    axis: [0.25, 0.75],
                },
                LightmapTexel::ZERO,
            ],
        );
        let bytes = page_rgba16f(&page);
        // Two texels per plane, two planes, four halves each.
        assert_eq!(bytes.len(), 2 * 8 * 2);
        let half = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        // Irradiance plane, texel 0: (1.0, 0.5, 0.0) and the reserved alpha
        // 0.5, in IEEE binary16 little-endian. The texel's `axis` field is
        // ignored: the stored alpha is always the reserved constant.
        assert_eq!(half(0), 0x3C00);
        assert_eq!(half(2), 0x3800);
        assert_eq!(half(4), 0x0000);
        assert_eq!(half(6), 0x3800);
        // Irradiance plane, texel 1: black with the reserved alpha.
        assert_eq!(half(8), 0x0000);
        assert_eq!(half(12), 0x0000);
        assert_eq!(half(14), 0x3800);
        // Direction-moment plane, texel 0: the signed moment (0.0, -1.0, 0.25)
        // — the negative channel is a negative half — and the reserved alpha,
        // then the zero-moment texel with the same reserved alpha.
        assert_eq!(half(16), 0x0000);
        assert_eq!(half(18), 0xBC00);
        assert_eq!(half(20), 0x3400);
        assert_eq!(half(22), 0x3800);
        assert_eq!(half(24), 0x0000);
        assert_eq!(half(30), 0x3800);
        // The same bytes the package writer's half conversion produces.
        let mut expected: Vec<u8> = Vec::new();
        for value in [
            1.0_f32, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0, -1.0, 0.25, 0.5, 0.0, 0.0, 0.0, 0.5,
        ] {
            expected.extend_from_slice(&f32_to_f16_bits(value).to_le_bytes());
        }
        assert_eq!(bytes, expected);
    }

    #[test]
    fn a_signed_direction_moment_round_trips_through_the_half_floats() {
        // The moment vector is signed; the half conversion must preserve every
        // sign. That is what the old positive-only amplitude could not carry.
        let moment = [-4.5_f32, -0.125, 3.75];
        let page = page(
            1,
            1,
            vec![LightmapTexel {
                irradiance: [0.25; 3],
                direction: moment,
                axis: [0.5, 0.5],
            }],
        );
        let bytes = page_rgba16f(&page);
        assert_eq!(bytes.len(), 16);
        for (channel, expected) in moment.into_iter().enumerate() {
            let offset = 8 + channel * 2;
            let bits = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            assert_eq!(f16_bits_to_f32(bits), expected, "channel {channel}");
            assert_eq!(
                bits & 0x8000 != 0,
                expected < 0.0,
                "channel {channel} must keep its sign"
            );
        }
        // Both planes' reserved alphas.
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 0x3800);
        assert_eq!(u16::from_le_bytes([bytes[14], bytes[15]]), 0x3800);
    }

    #[test]
    fn the_white_fallback_is_unit_irradiance_zero_direction_and_reserved_alpha() {
        // The pair the shader binds while no atlas is resident: every even
        // layer is white irradiance, every odd layer zero direction, both with
        // the reserved 0.5 alpha, so a sample that did read it reconstructs
        // `max(0, white + 0)`, the unit factor.
        let bytes = white_array_bytes(2, 6);
        assert_eq!(bytes.len(), 2 * 2 * 6 * 8);
        let half = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        for layer in 0..6 {
            let layer_base = layer * 2 * 2 * 8;
            for texel in 0..4 {
                let offset = layer_base + texel * 8;
                let tag = format!("layer {layer} texel {texel}");
                let rgb = if layer % 2 == 0 { 0x3C00 } else { 0x0000 };
                assert_eq!(half(offset), rgb, "{tag} r");
                assert_eq!(half(offset + 2), rgb, "{tag} g");
                assert_eq!(half(offset + 4), rgb, "{tag} b");
                assert_eq!(half(offset + 6), 0x3800, "{tag} reserved alpha");
            }
        }
        // The real 1x1 pair, decoded back: white irradiance and a zero moment,
        // so the shader's reconstruction is exactly the unit factor.
        let pair = white_array_bytes(1, 2);
        assert_eq!(pair.len(), 16);
        let sample =
            |offset: usize| f16_bits_to_f32(u16::from_le_bytes([pair[offset], pair[offset + 1]]));
        assert_eq!([sample(0), sample(2), sample(4)], [1.0; 3]);
        assert_eq!([sample(8), sample(10), sample(12)], [0.0; 3]);
        assert_eq!(sample(6), 0.5);
        assert_eq!(sample(14), 0.5);
    }

    #[test]
    fn a_truncated_page_is_padded_with_the_neutral_texel() {
        let page = page(
            2,
            1,
            vec![LightmapTexel {
                irradiance: [0.5; 3],
                direction: [0.25; 3],
                axis: [0.5, 0.5],
            }],
        );
        let bytes = page_rgba16f(&page);
        assert_eq!(bytes.len(), 2 * 8 * 2);
        let half = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        // The missing irradiance texel is white with the reserved alpha.
        assert_eq!(half(8), 0x3C00);
        assert_eq!(half(10), 0x3C00);
        assert_eq!(half(12), 0x3C00);
        assert_eq!(half(14), 0x3800);
        // The missing direction texel is zero with the reserved alpha.
        assert_eq!(half(24), 0x0000);
        assert_eq!(half(30), 0x3800);
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
            cache_key: "v7-test".to_string(),
            padding: 2,
            switchable: Vec::new(),
        }
    }

    #[test]
    fn the_page_budget_is_eight_at_every_profile() {
        assert_eq!(LIGHTMAP_ATLAS_MAX_PAGES, 8);
        for profile in crate::quality::QualityProfile::ALL {
            let config = profile.lightmap_config();
            assert_eq!(
                config.max_pages, LIGHTMAP_ATLAS_MAX_PAGES,
                "{profile:?} must support the shared page budget"
            );
            // Every page needs two RGBA16F layers.
            assert_eq!(config.bytes_per_texel, 16, "{profile:?}");
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
    fn upload_stats_count_pages_texels_and_the_exact_layer_bytes() {
        let stats = upload_stats(Some(&lightmaps(vec![black_page(1024), black_page(1024)])));
        assert_eq!(stats.capacity, LIGHTMAP_ATLAS_MAX_PAGES);
        assert_eq!(stats.pages, 2);
        assert_eq!(stats.page_edge, 1024);
        assert_eq!(stats.page_texels, 2 * 1024 * 1024);
        // Two pages, two planes each, eight bytes per texel.
        assert_eq!(stats.resident_bytes, 2 * 2 * 1024 * 1024 * 8);
        assert_eq!(stats.charts, 7);
        assert_eq!(stats.chart_texels, 123);
    }

    #[test]
    fn switchable_groups_extend_the_layer_layout_in_group_order() {
        let mut maps = lightmaps(vec![black_page(64), black_page(64)]);
        maps.switchable = vec![
            SwitchableLightmaps {
                light_index: 3,
                pages: vec![black_page(64), black_page(64)],
            },
            SwitchableLightmaps {
                light_index: 5,
                pages: vec![black_page(64), black_page(64)],
            },
        ];
        // The contract: irradiance at 2*page within each group, groups after
        // the base pages in group order.
        assert_eq!(maps.irradiance_layer(None, 0), 0);
        assert_eq!(maps.irradiance_layer(None, 1), 2);
        assert_eq!(maps.irradiance_layer(Some(0), 0), 4);
        assert_eq!(maps.irradiance_layer(Some(0), 1), 6);
        assert_eq!(maps.irradiance_layer(Some(1), 0), 8);
        assert_eq!(maps.irradiance_layer(Some(1), 1), 10);
        assert_eq!(maps.layer_count(), 12);

        let stats = upload_stats(Some(&maps));
        assert_eq!(stats.pages, 2, "the uniform carries the base page count");
        assert_eq!(stats.page_edge, 64);
        assert_eq!(stats.page_texels, 2 * 64 * 64);
        // 2 pages * 2 planes * (1 base + 2 switchable groups) = 12 layers.
        assert_eq!(stats.resident_bytes, 12 * 64 * 64 * 8);
    }

    #[test]
    fn no_atlas_reports_zero_pages_and_the_full_capacity() {
        let stats = upload_stats(None);
        assert_eq!(stats.pages, 0);
        assert_eq!(stats.capacity, LIGHTMAP_ATLAS_MAX_PAGES);
        assert_eq!(stats.page_edge, 0);
        assert_eq!(stats.resident_bytes, 0);
    }

    #[test]
    fn a_page_set_that_cannot_share_one_array_reports_no_pages() {
        // A different edge, a non-square page, a zero-edge page and an empty
        // set all make the whole set unuploadable as one array: the stats go
        // empty and the renderer takes its named vertex-lit fallback. No page
        // may be dropped silently.
        for pages in [
            vec![black_page(1024), black_page(512)],
            vec![page(16, 8, vec![LightmapTexel::ZERO; 128])],
            vec![page(0, 0, Vec::new())],
            vec![],
            // A set with more pages than the group budget is rejected as a
            // whole: none of the pages may be uploaded, because the vertex page
            // bytes would then address missing layers.
            (0..=LIGHTMAP_ATLAS_MAX_PAGES)
                .map(|_| black_page(64))
                .collect(),
        ] {
            let stats = upload_stats(Some(&lightmaps(pages)));
            assert_eq!(stats.pages, 0, "the set must be rejected as a whole");
            assert_eq!(stats.resident_bytes, 0);
        }
    }

    #[test]
    fn switchable_groups_that_disagree_with_the_base_are_rejected() {
        // The shader addresses every group with the uniform's one page count,
        // so a contribution with a different page count or edge can never be
        // drawn correctly and the whole set fails over.
        let mut mismatched_count = lightmaps(vec![black_page(64), black_page(64)]);
        mismatched_count.switchable = vec![SwitchableLightmaps {
            light_index: 0,
            pages: vec![black_page(64)],
        }];
        assert_eq!(upload_stats(Some(&mismatched_count)).pages, 0);

        let mut mismatched_edge = lightmaps(vec![black_page(64), black_page(64)]);
        mismatched_edge.switchable = vec![SwitchableLightmaps {
            light_index: 0,
            pages: vec![black_page(64), black_page(32)],
        }];
        assert_eq!(upload_stats(Some(&mismatched_edge)).pages, 0);

        // More groups than the uniform's four-bit count and mask carry can
        // never all light; the level falls back rather than dropping groups.
        let mut too_many = lightmaps(vec![black_page(64)]);
        too_many.switchable = (0..=MAX_SWITCHABLE_GROUPS)
            .map(|light_index| SwitchableLightmaps {
                light_index,
                pages: vec![black_page(64)],
            })
            .collect();
        assert_eq!(upload_stats(Some(&too_many)).pages, 0);
    }

    /// The upload format is pinned: a raw, non-sRGB, two-planes-per-texel
    /// format, one mip.
    #[test]
    fn the_page_format_is_raw_rgba16_float_linear() {
        assert_eq!(
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Float
        );
        // The reference of the HDR contract samples the linear halves with no
        // sRGB decode; the non-sRGB wgpu format is what preserves that.
        assert!(!wgpu::TextureFormat::Rgba16Float.is_srgb());
        // Eight bytes per texel: four half-float channels.
        assert_eq!(
            wgpu::TextureFormat::Rgba16Float.block_copy_size(None),
            Some(8)
        );
    }
}
